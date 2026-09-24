package com.djilivebridge.android

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.net.Uri
import android.net.wifi.WifiManager
import android.os.Build
import android.os.Handler
import android.os.IBinder
import android.os.Looper
import android.os.PowerManager
import android.os.SystemClock
import androidx.core.app.NotificationCompat
import androidx.core.app.ServiceCompat
import androidx.core.content.ContextCompat
import java.io.File
import java.util.concurrent.ExecutorService
import java.util.concurrent.Executors
import java.util.concurrent.atomic.AtomicBoolean
import kotlin.concurrent.thread

/**
 * Keeps the receiver open so DJI Fly can connect and its picture shows on the phone, and sends
 * the stream to a platform only between "go live" and "end live".
 */
class RelayForegroundService : Service() {
    private val mainHandler = Handler(Looper.getMainLooper())

    /** Native start, go-live, end-live and stop calls run here, one at a time and in order. */
    private val commands: ExecutorService = Executors.newSingleThreadExecutor { task ->
        Thread(task, "dji-relay-commands")
    }
    private val stopRequested = AtomicBoolean(false)
    private var receiverStarted = false
    @Volatile private var receiverListening = false
    @Volatile private var pollThread: Thread? = null
    @Volatile private var testVideoThread: Thread? = null
    @Volatile private var liveProfileId: String? = null
    @Volatile private var tlsCaBundle: File? = null
    private var wakeLock: PowerManager.WakeLock? = null
    private var wifiLock: WifiManager.WifiLock? = null
    private var shownNotification: Pair<String, Boolean>? = null

    /** Set when the receiver stops because of a failure the user should see, not by request. */
    @Volatile private var failureMessage: String? = null
    private var stopReason = "Alıcı kapatıldı"

    override fun onCreate() {
        super.onCreate()
        createNotificationChannel()
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        when (intent?.action) {
            ACTION_START_RECEIVER -> if (enterForeground()) {
                if (intent.getBooleanExtra(EXTRA_RESTART, false)) restartReceiver() else startReceiver()
                intent.data?.let { uri ->
                    startTestVideo(uri, intent.getStringExtra(EXTRA_TEST_VIDEO_NAME) ?: "Test videosu")
                }
            }
            ACTION_GO_LIVE -> if (enterForeground()) {
                startReceiver()
                goLive(intent.getStringExtra(EXTRA_DESTINATION_PROFILE_ID).orEmpty())
            }
            ACTION_END_LIVE -> endLive()
            ACTION_STOP_TEST_VIDEO -> stopTestVideo()
            ACTION_STOP -> requestStop("Alıcı kapatıldı")
        }
        // A stray end-live or stop-video command must not leave an idle service behind.
        if (!receiverStarted) stopSelf(startId)
        return START_NOT_STICKY
    }

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onTimeout(startId: Int, fgsType: Int) {
        val message = "Android arka plan veri aktarımı süre sınırına ulaştı"
        failureMessage = message
        requestStop(message)
    }

    /** Swiping the app away closes the receiver, but never a broadcast in progress. */
    override fun onTaskRemoved(rootIntent: Intent?) {
        if (liveProfileId == null) requestStop("Uygulama kapatıldı")
        super.onTaskRemoved(rootIntent)
    }

    override fun onDestroy() {
        stopRequested.set(true)
        pollThread?.interrupt()
        testVideoThread?.interrupt()
        commands.shutdown()
        NativeRelay.nativeStop()
        deleteTlsCaBundle()
        releaseWakeLock()
        releaseWifiLock()
        if (RelayServiceState.value.isActive) publishStoppedState()
        super.onDestroy()
    }

    private fun enterForeground(): Boolean = try {
        val text = shownNotification?.first ?: "Drone bağlantısı bekleniyor"
        ServiceCompat.startForeground(
            this,
            NOTIFICATION_ID,
            buildNotification(text, live = liveProfileId != null),
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
                ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC
            } else {
                0
            },
        )
        true
    } catch (error: RuntimeException) {
        RelayServiceState.failed("Arka plan servisi başlatılamadı: ${error.message}")
        stopSelf()
        false
    }

    private fun startReceiver() {
        if (receiverStarted) return
        receiverStarted = true
        stopRequested.set(false)
        failureMessage = null
        stopReason = "Alıcı kapatıldı"
        RelayServiceState.starting()
        acquireWifiLock()
        commands.execute {
            val error = runCatching { NativeRelay.nativeStartReceiver() }
                .getOrElse { error -> error.message ?: "RTMP çekirdeği başlatılamadı" }
            if (error.isNotEmpty()) {
                mainHandler.post {
                    RelayServiceState.failed(error)
                    finishService()
                }
            } else {
                pollThread = thread(name = "dji-relay-status") { pollSnapshots() }
            }
        }
    }

    /** Opens the receiver again after it failed; never while a broadcast is on. */
    private fun restartReceiver() {
        if (!receiverStarted) {
            startReceiver()
            return
        }
        if (liveProfileId != null) return
        testVideoThread?.interrupt()
        testVideoThread = null
        RelayServiceState.starting()
        commands.execute { NativeRelay.nativeStartReceiver() }
    }

    private fun pollSnapshots() {
        try {
            while (!stopRequested.get()) {
                val snapshot = runCatching {
                    RelaySnapshot.fromJson(NativeRelay.nativeSnapshot())
                }.getOrElse { error ->
                    RelaySnapshot(status = "error", detail = "Durum okunamadı: ${error.message}")
                }
                receiverListening = snapshot.status in LISTENING_STATUSES
                mainHandler.post {
                    if (!stopRequested.get()) {
                        RelayServiceState.running(snapshot)
                        showNotification(notificationText(snapshot))
                    }
                }
                Thread.sleep(SNAPSHOT_INTERVAL_MS)
            }
        } catch (_: InterruptedException) {
            // Stopping interrupts the sleep.
        }
    }

    private fun goLive(profileId: String) {
        if (profileId.isBlank()) {
            RelayServiceState.notLive(RelayNotice(GO_LIVE_FAILED, "Önce bir platform seç"))
            return
        }
        liveProfileId = profileId
        RelayServiceState.goingLive(profileId)
        acquireWakeLock()
        showNotification(notificationText(RelayServiceState.value.snapshot))
        commands.execute {
            val error = runCatching { attachDestination(profileId) }
                .getOrElse { error -> error.message ?: "Canlı yayın başlatılamadı" }
            if (error.isNotEmpty()) {
                deleteTlsCaBundle()
                mainHandler.post {
                    if (liveProfileId == profileId) {
                        liveProfileId = null
                        releaseWakeLock()
                        RelayServiceState.notLive(RelayNotice(GO_LIVE_FAILED, error))
                        showNotification(notificationText(RelayServiceState.value.snapshot))
                    }
                }
            }
        }
    }

    private fun attachDestination(profileId: String): String {
        val credentials = DestinationProfileStore(applicationContext).credentials(profileId)
        deleteTlsCaBundle()
        val tlsCaFile = if (credentials.serverUrl.startsWith("rtmps://")) {
            try {
                createAndroidSystemCaBundle(this).also { file -> tlsCaBundle = file }.absolutePath
            } catch (_: Exception) {
                throw DestinationProfileException("Android sistem sertifikaları hazırlanamadı")
            }
        } else {
            ""
        }
        return NativeRelay.nativeGoLive(credentials.serverUrl, credentials.streamKey, tlsCaFile)
    }

    private fun endLive() {
        if (liveProfileId == null) return
        liveProfileId = null
        RelayServiceState.notLive()
        releaseWakeLock()
        showNotification(notificationText(RelayServiceState.value.snapshot))
        commands.execute {
            NativeRelay.nativeEndLive()
            deleteTlsCaBundle()
        }
    }

    private fun startTestVideo(uri: Uri, name: String) {
        testVideoThread?.interrupt()
        RelayServiceState.testVideo(name)
        testVideoThread = thread(name = "dji-test-video") { runTestVideo(uri) }
    }

    /** Plays the picked video into the receiver, standing in for DJI Fly. */
    private fun runTestVideo(uri: Uri) {
        val self = Thread.currentThread()
        try {
            awaitReceiver()
            TestVideoStreamer(applicationContext, uri).run()
        } catch (_: InterruptedException) {
            // Stopping interrupts the pacing sleep.
        } catch (error: Exception) {
            if (self.isInterrupted || stopRequested.get()) return
            val notice = RelayNotice("Test videosu durdu", error.message ?: "Bilinmeyen hata")
            mainHandler.post {
                if (testVideoThread === self) {
                    testVideoThread = null
                    RelayServiceState.testVideo(null, notice)
                }
            }
        }
    }

    private fun awaitReceiver() {
        val deadline = SystemClock.elapsedRealtime() + RECEIVER_START_TIMEOUT_MS
        while (!receiverListening) {
            if (SystemClock.elapsedRealtime() > deadline) throw TestVideoException("alıcı hazır değil")
            Thread.sleep(RECEIVER_START_POLL_MS)
        }
    }

    private fun stopTestVideo() {
        testVideoThread?.interrupt()
        testVideoThread = null
        RelayServiceState.testVideo(null)
    }

    private fun requestStop(reason: String) {
        stopReason = reason
        stopRequested.set(true)
        liveProfileId = null
        testVideoThread?.interrupt()
        testVideoThread = null
        pollThread?.interrupt()
        if (!receiverStarted) {
            finishService()
            return
        }
        commands.execute {
            NativeRelay.nativeStop()
            deleteTlsCaBundle()
            mainHandler.post {
                publishStoppedState()
                finishService()
            }
        }
    }

    private fun publishStoppedState() {
        val failure = failureMessage
        if (failure != null) {
            RelayServiceState.failed(failure)
            return
        }
        RelayServiceState.stopped(
            RelayServiceState.value.snapshot.copy(
                status = "stopped",
                detail = stopReason,
                bitrateKbps = 0.0,
                outputStatus = "disabled",
                outputDetail = "Harici hedef yapılandırılmadı",
            ),
        )
    }

    private fun finishService() {
        receiverStarted = false
        deleteTlsCaBundle()
        releaseWakeLock()
        releaseWifiLock()
        ServiceCompat.stopForeground(this, ServiceCompat.STOP_FOREGROUND_REMOVE)
        stopSelf()
    }

    private fun deleteTlsCaBundle() {
        tlsCaBundle?.delete()
        tlsCaBundle = null
    }

    /** Held only while live: a broadcast must survive a locked screen, a preview need not. */
    private fun acquireWakeLock() {
        if (wakeLock?.isHeld == true) return
        wakeLock = getSystemService(PowerManager::class.java).newWakeLock(
            PowerManager.PARTIAL_WAKE_LOCK,
            "$packageName:rtmp-relay",
        ).apply {
            setReferenceCounted(false)
            acquire(WAKE_LOCK_TIMEOUT_MS)
        }
    }

    private fun releaseWakeLock() {
        wakeLock?.let { lock ->
            if (lock.isHeld) lock.release()
        }
        wakeLock = null
    }

    /**
     * Keeps Wi-Fi out of power save while the receiver runs. In power save the access point holds
     * the remote's packets until the phone wakes for a beacon, which arrives as stutter.
     */
    private fun acquireWifiLock() {
        if (wifiLock?.isHeld == true) return
        val manager = applicationContext.getSystemService(WifiManager::class.java) ?: return
        val mode = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            WifiManager.WIFI_MODE_FULL_LOW_LATENCY
        } else {
            @Suppress("DEPRECATION")
            WifiManager.WIFI_MODE_FULL_HIGH_PERF
        }
        wifiLock = runCatching {
            manager.createWifiLock(mode, "$packageName:rtmp-receiver").apply {
                setReferenceCounted(false)
                acquire()
            }
        }.getOrNull()
    }

    private fun releaseWifiLock() {
        wifiLock?.let { lock ->
            if (lock.isHeld) lock.release()
        }
        wifiLock = null
    }

    private fun createNotificationChannel() {
        val manager = getSystemService(NotificationManager::class.java)
        manager.createNotificationChannel(
            NotificationChannel(
                CHANNEL_ID,
                "Canlı RTMP aktarımı",
                NotificationManager.IMPORTANCE_LOW,
            ).apply {
                description = "DJI RC 2 yayın alımı ve harici RTMP aktarım durumu"
                setShowBadge(false)
            },
        )
    }

    private fun buildNotification(text: String, live: Boolean): Notification {
        val openIntent = PendingIntent.getActivity(
            this,
            0,
            Intent(this, MainActivity::class.java).apply {
                flags = Intent.FLAG_ACTIVITY_SINGLE_TOP or Intent.FLAG_ACTIVITY_CLEAR_TOP
            },
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
        val actionIntent = PendingIntent.getService(
            this,
            1,
            Intent(this, RelayForegroundService::class.java).setAction(if (live) ACTION_END_LIVE else ACTION_STOP),
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
        return NotificationCompat.Builder(this, CHANNEL_ID)
            .setSmallIcon(R.drawable.ic_drone)
            .setColor(ContextCompat.getColor(this, R.color.bridge_brand))
            .setContentTitle("DJI Live Bridge")
            .setContentText(text)
            .setContentIntent(openIntent)
            .setOngoing(true)
            .setOnlyAlertOnce(true)
            .setCategory(NotificationCompat.CATEGORY_SERVICE)
            .addAction(0, if (live) "Yayını bitir" else "Kapat", actionIntent)
            .build()
    }

    /** Posts only real changes; Android throttles apps that update notifications too often. */
    private fun showNotification(text: String) {
        val next = text to (liveProfileId != null)
        if (next == shownNotification) return
        shownNotification = next
        getSystemService(NotificationManager::class.java)
            .notify(NOTIFICATION_ID, buildNotification(next.first, next.second))
    }

    private fun notificationText(snapshot: RelaySnapshot): String {
        val publishing = snapshot.status == "publishing"
        return when {
            snapshot.status == "error" -> "RTMP alıcı hatası"
            snapshot.status == "starting" -> "Alıcı hazırlanıyor"
            liveProfileId != null -> when {
                publishing && snapshot.outputStatus == "congested" -> "Canlı yayındasın · bağlantı yavaş"
                publishing && snapshot.outputStatus == "forwarding" -> "Canlı yayındasın"
                publishing && snapshot.outputStatus == "reconnecting" -> "Platforma yeniden bağlanıyor"
                publishing -> "Platforma bağlanıyor"
                snapshot.outputStatus == "holding" -> "Drone bağlantısı koptu · yayın açık tutuluyor"
                else -> "Canlı yayın açık · drone bekleniyor"
            }
            publishing ->
                if (RelayServiceState.value.testVideoName != null) "Test videosu alınıyor" else "Drone görüntüsü alınıyor"
            snapshot.status == "connected" -> "Kumanda bağlandı"
            else -> "Drone bağlantısı bekleniyor"
        }
    }

    companion object {
        private const val ACTION_START_RECEIVER = "com.djilivebridge.android.action.START_RECEIVER"
        private const val ACTION_GO_LIVE = "com.djilivebridge.android.action.GO_LIVE"
        private const val ACTION_END_LIVE = "com.djilivebridge.android.action.END_LIVE"
        private const val ACTION_STOP_TEST_VIDEO = "com.djilivebridge.android.action.STOP_TEST_VIDEO"
        private const val ACTION_STOP = "com.djilivebridge.android.action.STOP_RELAY"
        private const val EXTRA_DESTINATION_PROFILE_ID = "destination_profile_id"
        private const val EXTRA_TEST_VIDEO_NAME = "test_video_name"
        private const val EXTRA_RESTART = "restart"
        private const val CHANNEL_ID = "relay_status"
        private const val NOTIFICATION_ID = 1935
        private const val SNAPSHOT_INTERVAL_MS = 500L
        private const val RECEIVER_START_TIMEOUT_MS = 5_000L
        private const val RECEIVER_START_POLL_MS = 50L
        private const val WAKE_LOCK_TIMEOUT_MS = 6L * 60L * 60L * 1_000L
        private val LISTENING_STATUSES = setOf("listening", "connected", "publishing")
        private const val GO_LIVE_FAILED = "Canlı yayın başlatılamadı"

        /**
         * Opens the receiver; with [testVideo] the picked file plays in place of DJI Fly, and with
         * [restart] a receiver that stopped on an error opens again.
         */
        internal fun startReceiver(
            context: Context,
            testVideo: TestVideoSelection? = null,
            restart: Boolean = false,
        ) {
            val intent = Intent(context, RelayForegroundService::class.java)
                .setAction(ACTION_START_RECEIVER)
                .putExtra(EXTRA_RESTART, restart)
            if (testVideo != null) {
                intent.setData(testVideo.uri)
                    .putExtra(EXTRA_TEST_VIDEO_NAME, testVideo.displayName)
                    .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
            }
            ContextCompat.startForegroundService(context, intent)
        }

        /** Sends the received stream to the destination saved as [destinationProfileId]. */
        fun goLive(context: Context, destinationProfileId: String) {
            ContextCompat.startForegroundService(
                context,
                Intent(context, RelayForegroundService::class.java)
                    .setAction(ACTION_GO_LIVE)
                    .putExtra(EXTRA_DESTINATION_PROFILE_ID, destinationProfileId),
            )
        }

        /** Ends the broadcast; the drone stays connected and its picture keeps showing. */
        fun endLive(context: Context) {
            context.startService(Intent(context, RelayForegroundService::class.java).setAction(ACTION_END_LIVE))
        }

        fun stopTestVideo(context: Context) {
            context.startService(Intent(context, RelayForegroundService::class.java).setAction(ACTION_STOP_TEST_VIDEO))
        }

        fun stop(context: Context) {
            context.startService(Intent(context, RelayForegroundService::class.java).setAction(ACTION_STOP))
        }
    }
}
