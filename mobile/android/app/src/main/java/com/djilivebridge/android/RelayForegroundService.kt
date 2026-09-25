package com.djilivebridge.android

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.content.res.Configuration
import android.net.Uri
import android.net.wifi.WifiManager
import android.os.Build
import android.os.Handler
import android.os.IBinder
import android.os.Looper
import android.os.PowerManager
import android.os.SystemClock
import android.util.Log
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
    /** Turns a heavy test video into DJI Fly's format before it plays. Main thread only. */
    private var testVideoConverter: TestVideoConverter? = null
    /** The profiles the stream goes to. Touched on the main thread only. */
    private val liveProfileIds = mutableListOf<String>()
    @Volatile private var tlsCaBundle: File? = null
    private var wakeLock: PowerManager.WakeLock? = null
    private var wifiLock: WifiManager.WifiLock? = null
    private var shownNotification: Pair<String, Boolean>? = null

    /** Set when the receiver stops because of a failure the user should see, not by request. */
    @Volatile private var failureMessage: UiText? = null

    override fun onCreate() {
        super.onCreate()
        createNotificationChannel()
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        when (intent?.action) {
            ACTION_START_RECEIVER -> if (enterForeground()) {
                if (intent.getBooleanExtra(EXTRA_RESTART, false)) restartReceiver() else startReceiver()
                intent.data?.let { uri ->
                    val durationMs = intent.getLongExtra(EXTRA_TEST_VIDEO_DURATION_MS, -1).takeIf { it >= 0 }
                    startTestVideo(
                        uri,
                        testVideoLabel(intent.getStringExtra(EXTRA_TEST_VIDEO_NAME), durationMs),
                        convert = intent.getBooleanExtra(EXTRA_TEST_VIDEO_CONVERT, false),
                    )
                }
            }
            ACTION_GO_LIVE -> if (enterForeground()) {
                startReceiver()
                goLive(intent.getStringArrayExtra(EXTRA_DESTINATION_PROFILE_IDS).orEmpty().toList())
            }
            ACTION_END_LIVE -> endLive(intent.getStringExtra(EXTRA_DESTINATION_PROFILE_ID))
            ACTION_STOP_TEST_VIDEO -> stopTestVideo()
            ACTION_STOP -> requestStop()
        }
        // A stray end-live or stop-video command must not leave an idle service behind.
        if (!receiverStarted) stopSelf(startId)
        return START_NOT_STICKY
    }

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onTimeout(startId: Int, fgsType: Int) {
        failureMessage = uiText(R.string.error_background_limit)
        requestStop()
    }

    /** Swiping the app away closes the receiver, but never a broadcast in progress. */
    override fun onTaskRemoved(rootIntent: Intent?) {
        if (liveProfileIds.isEmpty()) requestStop()
        super.onTaskRemoved(rootIntent)
    }

    override fun onDestroy() {
        stopRequested.set(true)
        pollThread?.interrupt()
        stopTestVideoWork()
        commands.shutdown()
        NativeRelay.nativeStop()
        deleteTlsCaBundle()
        releaseWakeLock()
        releaseWifiLock()
        if (RelayServiceState.value.isActive) publishStoppedState()
        super.onDestroy()
    }

    private fun enterForeground(): Boolean = try {
        val text = shownNotification?.first ?: getString(R.string.notification_waiting)
        ServiceCompat.startForeground(
            this,
            NOTIFICATION_ID,
            buildNotification(text, live = liveProfileIds.isNotEmpty()),
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
                ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC
            } else {
                0
            },
        )
        true
    } catch (error: RuntimeException) {
        RelayServiceState.failed(withCause(R.string.error_service_start, error))
        stopSelf()
        false
    }

    private fun startReceiver() {
        if (receiverStarted) return
        receiverStarted = true
        stopRequested.set(false)
        failureMessage = null
        RelayServiceState.starting()
        acquireWifiLock()
        commands.execute {
            val error = runCatching { NativeRelay.nativeStartReceiver() }.getOrDefault("jni_argument")
            if (error.isNotEmpty()) {
                mainHandler.post {
                    RelayServiceState.failed(nativeError(error))
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
        if (liveProfileIds.isNotEmpty()) return
        stopTestVideoWork()
        RelayServiceState.starting()
        commands.execute { NativeRelay.nativeStartReceiver() }
    }

    private fun pollSnapshots() {
        var previous = RelaySnapshot()
        try {
            while (!stopRequested.get()) {
                val snapshot = runCatching {
                    RelaySnapshot.fromJson(NativeRelay.nativeSnapshot())
                }.getOrElse { error ->
                    RelaySnapshot(status = "error", error = withCause(R.string.error_internal, error))
                }
                logInterruptions(previous, snapshot)
                previous = snapshot
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

    /**
     * Leaves a line in logcat for each pause or reconnect of the drone's stream and each change
     * of a platform's state, so a capture shows which side a gap in the broadcast came from.
     * Only codes and counts: never an address or a key.
     */
    private fun logInterruptions(previous: RelaySnapshot, snapshot: RelaySnapshot) {
        if (snapshot.stalls > previous.stalls) {
            Log.i(
                LOG_TAG,
                "Drone video paused (${snapshot.stalls} so far, longest ${snapshot.longestStallMs} ms, " +
                    "${snapshot.recentInterruptions} in the last minute)",
            )
        }
        if (snapshot.sourceReconnects > previous.sourceReconnects) {
            Log.i(LOG_TAG, "DJI Fly reconnected (${snapshot.sourceReconnects} so far)")
        }
        snapshot.outputs.forEach { output ->
            val before = previous.output(output.id)?.status
            if (output.status != before) {
                Log.i(
                    LOG_TAG,
                    "Platform ${output.id.take(8)}: ${before ?: "added"} -> ${output.status}" +
                        (output.reason?.let { " ($it)" }.orEmpty()) +
                        ", ${output.droppedFrames} frames skipped so far",
                )
            }
        }
    }

    /** Adds [profileIds] to the broadcast. A platform that fails is reported; the others go on. */
    private fun goLive(profileIds: List<String>) {
        if (profileIds.isEmpty()) {
            RelayServiceState.notLive(emptyList(), RelayNotice(uiText(R.string.go_live_failed), uiText(R.string.pick_platform_first)))
            return
        }
        val added = profileIds.filterNot { it in liveProfileIds }
        if (added.isEmpty()) return
        liveProfileIds += added
        RelayServiceState.goingLive(added)
        acquireWakeLock()
        showNotification(notificationText(RelayServiceState.value.snapshot))
        commands.execute {
            val store = DestinationProfileStore(applicationContext)
            val failures = added.mapNotNull { profileId ->
                val error = try {
                    attachDestination(store, profileId).takeIf(String::isNotEmpty)?.let(::nativeError)
                } catch (failure: DestinationProfileException) {
                    failure.text
                } catch (_: Exception) {
                    uiText(R.string.error_internal)
                } ?: return@mapNotNull null
                val kind = runCatching { store.load().profiles.firstOrNull { it.id == profileId }?.kind }.getOrNull()
                profileId to (kind?.let { uiText(R.string.named_error, it.nameText, error) } ?: error)
            }
            if (failures.isNotEmpty()) mainHandler.post { goLiveFailed(failures) }
        }
    }

    private fun goLiveFailed(failures: List<Pair<String, UiText>>) {
        val failed = failures.map { it.first }.filter { it in liveProfileIds }
        if (failed.isEmpty()) return
        liveProfileIds.removeAll(failed)
        val message = UiText.Raw(failures.joinToString("\n") { it.second.resolve(this) })
        RelayServiceState.notLive(failed, RelayNotice(uiText(R.string.go_live_failed), message))
        if (liveProfileIds.isEmpty()) {
            releaseWakeLock()
            commands.execute { deleteTlsCaBundle() }
        }
        showNotification(notificationText(RelayServiceState.value.snapshot))
    }

    private fun attachDestination(store: DestinationProfileStore, profileId: String): String {
        val credentials = store.credentials(profileId)
        val tlsCaFile = if (credentials.serverUrl.startsWith("rtmps://")) {
            try {
                // One bundle for every RTMPS platform; it goes when the last one ends.
                (tlsCaBundle?.takeIf(File::exists) ?: createAndroidSystemCaBundle(this))
                    .also { file -> tlsCaBundle = file }
                    .absolutePath
            } catch (_: Exception) {
                throw DestinationProfileException(R.string.error_ca_bundle)
            }
        } else {
            ""
        }
        return NativeRelay.nativeGoLive(profileId, credentials.serverUrl, credentials.streamKey, tlsCaFile)
    }

    /** Ends the broadcast on [profileId], or on every platform when null; the drone stays connected. */
    private fun endLive(profileId: String?) {
        val ended = if (profileId == null) liveProfileIds.toList() else listOf(profileId).filter { it in liveProfileIds }
        if (ended.isEmpty()) return
        liveProfileIds.removeAll(ended)
        RelayServiceState.notLive(ended)
        val stillLive = liveProfileIds.isNotEmpty()
        if (!stillLive) releaseWakeLock()
        showNotification(notificationText(RelayServiceState.value.snapshot))
        commands.execute {
            NativeRelay.nativeEndLive(if (stillLive) profileId.orEmpty() else "")
            if (!stillLive) deleteTlsCaBundle()
        }
    }

    /** Plays [uri] in place of DJI Fly, converted first to what DJI Fly sends when [convert]. */
    private fun startTestVideo(uri: Uri, name: UiText, convert: Boolean) {
        stopTestVideoWork()
        RelayServiceState.testVideo(name)
        if (!convert) {
            playTestVideo(uri)
            return
        }
        RelayServiceState.testVideoConverting(0)
        testVideoConverter = TestVideoConverter(
            applicationContext,
            onProgress = RelayServiceState::testVideoConverting,
            onFinished = { result ->
                testVideoConverter = null
                result
                    .onSuccess(::playTestVideo)
                    .onFailure { error ->
                        Log.w(LOG_TAG, "The test video could not be converted", error)
                        val notice = RelayNotice(uiText(R.string.test_video_stopped), uiText(R.string.test_video_convert_failed))
                        RelayServiceState.testVideo(null, notice)
                    }
            },
        ).also { it.start(uri) }
    }

    private fun playTestVideo(uri: Uri) {
        RelayServiceState.testVideoConverting(null)
        testVideoThread = thread(name = "dji-test-video") { runTestVideo(uri) }
    }

    /** Ends what the test video is doing, converting or playing. */
    private fun stopTestVideoWork() {
        testVideoConverter?.cancel()
        testVideoConverter = null
        testVideoThread?.interrupt()
        testVideoThread = null
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
            val detail = (error as? TestVideoException)?.text
                ?: uiText(R.string.test_video_failed, error.message ?: error.javaClass.simpleName)
            val notice = RelayNotice(uiText(R.string.test_video_stopped), detail)
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
            if (SystemClock.elapsedRealtime() > deadline) {
                throw TestVideoException(uiText(R.string.test_video_receiver_not_ready))
            }
            Thread.sleep(RECEIVER_START_POLL_MS)
        }
    }

    private fun stopTestVideo() {
        stopTestVideoWork()
        RelayServiceState.testVideo(null)
    }

    private fun requestStop() {
        stopRequested.set(true)
        liveProfileIds.clear()
        stopTestVideoWork()
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
                bitrateKbps = 0.0,
                outputs = emptyList(),
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

    override fun onConfigurationChanged(newConfig: Configuration) {
        super.onConfigurationChanged(newConfig)
        // The app's language may have changed: the channel gets its new name now, and the next
        // status poll reposts the notification in the new language.
        createNotificationChannel()
    }

    private fun createNotificationChannel() {
        val manager = getSystemService(NotificationManager::class.java)
        manager.createNotificationChannel(
            NotificationChannel(
                CHANNEL_ID,
                getString(R.string.notification_channel),
                NotificationManager.IMPORTANCE_LOW,
            ).apply {
                description = getString(R.string.notification_channel_description)
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
            .setContentTitle(getString(R.string.app_name))
            .setContentText(text)
            .setContentIntent(openIntent)
            .setOngoing(true)
            .setOnlyAlertOnce(true)
            .setCategory(NotificationCompat.CATEGORY_SERVICE)
            .addAction(0, getString(if (live) R.string.end_broadcast else R.string.close), actionIntent)
            .build()
    }

    /** Posts only real changes; Android throttles apps that update notifications too often. */
    private fun showNotification(text: String) {
        val next = text to liveProfileIds.isNotEmpty()
        if (next == shownNotification) return
        shownNotification = next
        getSystemService(NotificationManager::class.java)
            .notify(NOTIFICATION_ID, buildNotification(next.first, next.second))
    }

    private fun notificationText(snapshot: RelaySnapshot): String {
        val publishing = snapshot.status == "publishing"
        return when {
            snapshot.status == "error" -> getString(R.string.notification_receiver_error)
            snapshot.status == "starting" -> getString(R.string.notification_starting)
            liveProfileIds.isNotEmpty() -> when {
                publishing && snapshot.outputStatus == "congested" -> getString(R.string.notification_live_slow)
                publishing && snapshot.outputStatus == "forwarding" && liveProfileIds.size > 1 ->
                    resources.getQuantityString(R.plurals.notification_live_many, liveProfileIds.size, liveProfileIds.size)
                publishing && snapshot.outputStatus == "forwarding" -> getString(R.string.notification_live)
                publishing && snapshot.outputStatus == "reconnecting" -> getString(R.string.notification_reconnecting)
                publishing -> getString(R.string.notification_connecting)
                snapshot.outputStatus == "holding" -> getString(R.string.notification_holding)
                else -> getString(R.string.notification_live_waiting)
            }
            publishing && RelayServiceState.value.testVideoName != null -> getString(R.string.notification_test_video)
            publishing -> getString(R.string.notification_drone)
            snapshot.status == "connected" -> getString(R.string.notification_connected)
            else -> getString(R.string.notification_waiting)
        }
    }

    companion object {
        private const val ACTION_START_RECEIVER = "com.djilivebridge.android.action.START_RECEIVER"
        private const val ACTION_GO_LIVE = "com.djilivebridge.android.action.GO_LIVE"
        private const val ACTION_END_LIVE = "com.djilivebridge.android.action.END_LIVE"
        private const val ACTION_STOP_TEST_VIDEO = "com.djilivebridge.android.action.STOP_TEST_VIDEO"
        private const val ACTION_STOP = "com.djilivebridge.android.action.STOP_RELAY"
        private const val EXTRA_DESTINATION_PROFILE_ID = "destination_profile_id"
        private const val EXTRA_DESTINATION_PROFILE_IDS = "destination_profile_ids"
        private const val EXTRA_TEST_VIDEO_NAME = "test_video_name"
        private const val EXTRA_TEST_VIDEO_DURATION_MS = "test_video_duration_ms"
        private const val EXTRA_TEST_VIDEO_CONVERT = "test_video_convert"
        private const val EXTRA_RESTART = "restart"
        private const val CHANNEL_ID = "relay_status"
        private const val NOTIFICATION_ID = 1935
        private const val SNAPSHOT_INTERVAL_MS = 500L
        private const val RECEIVER_START_TIMEOUT_MS = 5_000L
        private const val RECEIVER_START_POLL_MS = 50L
        private const val WAKE_LOCK_TIMEOUT_MS = 6L * 60L * 60L * 1_000L
        private val LISTENING_STATUSES = setOf("listening", "connected", "publishing")

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
                    .putExtra(EXTRA_TEST_VIDEO_DURATION_MS, testVideo.durationMs ?: -1)
                    .putExtra(EXTRA_TEST_VIDEO_CONVERT, testVideo.convert)
                    .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
            }
            ContextCompat.startForegroundService(context, intent)
        }

        /** Sends the received stream to every destination in [destinationProfileIds]. */
        fun goLive(context: Context, destinationProfileIds: List<String>) {
            ContextCompat.startForegroundService(
                context,
                Intent(context, RelayForegroundService::class.java)
                    .setAction(ACTION_GO_LIVE)
                    .putExtra(EXTRA_DESTINATION_PROFILE_IDS, destinationProfileIds.toTypedArray()),
            )
        }

        /**
         * Ends the broadcast on [destinationProfileId], or everywhere when null; the drone stays
         * connected and its picture keeps showing.
         */
        fun endLive(context: Context, destinationProfileId: String? = null) {
            context.startService(
                Intent(context, RelayForegroundService::class.java)
                    .setAction(ACTION_END_LIVE)
                    .putExtra(EXTRA_DESTINATION_PROFILE_ID, destinationProfileId),
            )
        }

        fun stopTestVideo(context: Context) {
            context.startService(Intent(context, RelayForegroundService::class.java).setAction(ACTION_STOP_TEST_VIDEO))
        }

        fun stop(context: Context) {
            context.startService(Intent(context, RelayForegroundService::class.java).setAction(ACTION_STOP))
        }
    }
}
