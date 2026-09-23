package com.djilivebridge.android

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder
import android.os.Looper
import android.os.PowerManager
import androidx.core.app.NotificationCompat
import androidx.core.app.ServiceCompat
import androidx.core.content.ContextCompat
import java.io.File
import java.util.concurrent.atomic.AtomicBoolean
import kotlin.concurrent.thread

class RelayForegroundService : Service() {
    private val stopRequested = AtomicBoolean(false)
    private val mainHandler = android.os.Handler(Looper.getMainLooper())
    private var workerThread: Thread? = null
    private var wakeLock: PowerManager.WakeLock? = null
    @Volatile private var tlsCaBundle: File? = null
    private var stopReason = "RTMP aktarımı durduruldu"

    override fun onCreate() {
        super.onCreate()
        createNotificationChannel()
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        when (intent?.action) {
            ACTION_STOP -> requestStop("RTMP aktarımı kullanıcı tarafından durduruldu")
            ACTION_START -> startRelay(intent)
        }
        return START_NOT_STICKY
    }

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onTimeout(startId: Int, fgsType: Int) {
        val message = "Android arka plan veri aktarımı süre sınırına ulaştı"
        RelayServiceState.failed(message)
        updateNotification(message)
        requestStop(message)
    }

    override fun onDestroy() {
        stopRequested.set(true)
        workerThread?.interrupt()
        NativeRelay.nativeStop()
        deleteTlsCaBundle()
        releaseWakeLock()
        workerThread = null
        if (RelayServiceState.value.isActive) {
            RelayServiceState.stopped(
                RelayServiceState.value.snapshot.copy(
                    status = "stopped",
                    detail = stopReason,
                    bitrateKbps = 0.0,
                    outputStatus = "stopped",
                    outputDetail = stopReason,
                ),
            )
        }
        super.onDestroy()
    }

    private fun startRelay(intent: Intent) {
        if (workerThread?.isAlive == true) return

        val destinationProfileId = intent.getStringExtra(EXTRA_DESTINATION_PROFILE_ID).orEmpty()
        intent.removeExtra(EXTRA_DESTINATION_PROFILE_ID)

        try {
            ServiceCompat.startForeground(
                this,
                NOTIFICATION_ID,
                buildNotification("RTMP aktarımı hazırlanıyor"),
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
                    ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC
                } else {
                    0
                },
            )
        } catch (error: RuntimeException) {
            RelayServiceState.failed("Arka plan servisi başlatılamadı: ${error.message}")
            stopSelf()
            return
        }

        acquireWakeLock()
        RelayServiceState.starting()
        stopRequested.set(false)
        stopReason = "RTMP aktarımı durduruldu"
        workerThread = thread(name = "dji-relay-service") {
            val startError = runCatching {
                startNativeRelay(destinationProfileId)
            }.getOrElse { error ->
                error.message ?: "RTMP çekirdeği başlatılamadı"
            }

            if (startError.isNotEmpty()) {
                mainHandler.post {
                    RelayServiceState.failed(startError)
                    updateNotification("RTMP başlatma hatası")
                    finishService()
                }
                return@thread
            }

            try {
                while (!stopRequested.get()) {
                    val snapshot = runCatching {
                        RelaySnapshot.fromJson(NativeRelay.nativeSnapshot())
                    }.getOrElse { error ->
                        RelaySnapshot(status = "error", detail = "Durum okunamadı: ${error.message}")
                    }
                    mainHandler.post {
                        RelayServiceState.running(snapshot)
                        updateNotification(notificationText(snapshot))
                    }
                    Thread.sleep(SNAPSHOT_INTERVAL_MS)
                }
            } catch (_: InterruptedException) {
                // Stop requests interrupt the sleep so native resources close immediately.
            } finally {
                NativeRelay.nativeStop()
                mainHandler.post {
                    val previous = RelayServiceState.value.snapshot
                    RelayServiceState.stopped(
                        previous.copy(
                            status = "stopped",
                            detail = stopReason,
                            bitrateKbps = 0.0,
                            outputStatus = "stopped",
                            outputDetail = stopReason,
                        ),
                    )
                    finishService()
                }
            }
        }
    }

    private fun startNativeRelay(destinationProfileId: String): String {
        if (destinationProfileId.isBlank()) {
            return NativeRelay.nativeStartLocal()
        }
        val credentials = DestinationProfileStore(applicationContext)
            .credentials(destinationProfileId)
        val tlsCaFile = if (credentials.serverUrl.startsWith("rtmps://")) {
            try {
                createAndroidSystemCaBundle(this).also { file -> tlsCaBundle = file }.absolutePath
            } catch (_: Exception) {
                throw DestinationProfileException("Android sistem sertifikaları hazırlanamadı")
            }
        } else {
            ""
        }
        return NativeRelay.nativeStart(
            credentials.serverUrl,
            credentials.streamKey,
            tlsCaFile,
        )
    }

    private fun requestStop(reason: String) {
        stopReason = reason
        stopRequested.set(true)
        workerThread?.interrupt()
        if (workerThread?.isAlive != true) {
            finishService()
        }
    }

    private fun finishService() {
        deleteTlsCaBundle()
        releaseWakeLock()
        ServiceCompat.stopForeground(this, ServiceCompat.STOP_FOREGROUND_REMOVE)
        stopSelf()
    }

    private fun deleteTlsCaBundle() {
        tlsCaBundle?.delete()
        tlsCaBundle = null
    }

    private fun acquireWakeLock() {
        val lock = getSystemService(PowerManager::class.java).newWakeLock(
            PowerManager.PARTIAL_WAKE_LOCK,
            "$packageName:rtmp-relay",
        ).apply {
            setReferenceCounted(false)
            acquire(WAKE_LOCK_TIMEOUT_MS)
        }
        wakeLock = lock
    }

    private fun releaseWakeLock() {
        wakeLock?.let { lock ->
            if (lock.isHeld) lock.release()
        }
        wakeLock = null
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

    private fun buildNotification(text: String): Notification {
        val openIntent = PendingIntent.getActivity(
            this,
            0,
            Intent(this, MainActivity::class.java).apply {
                flags = Intent.FLAG_ACTIVITY_SINGLE_TOP or Intent.FLAG_ACTIVITY_CLEAR_TOP
            },
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
        val stopIntent = PendingIntent.getService(
            this,
            1,
            Intent(this, RelayForegroundService::class.java).setAction(ACTION_STOP),
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
        return NotificationCompat.Builder(this, CHANNEL_ID)
            .setSmallIcon(R.drawable.app_icon)
            .setContentTitle("DJI Live Bridge")
            .setContentText(text)
            .setContentIntent(openIntent)
            .setOngoing(true)
            .setOnlyAlertOnce(true)
            .setCategory(NotificationCompat.CATEGORY_SERVICE)
            .addAction(0, "Durdur", stopIntent)
            .build()
    }

    private fun updateNotification(text: String) {
        getSystemService(NotificationManager::class.java)
            .notify(NOTIFICATION_ID, buildNotification(text))
    }

    private fun notificationText(snapshot: RelaySnapshot): String = when {
        snapshot.outputStatus == "error" -> "Harici hedef aktarım hatası"
        snapshot.outputStatus == "reconnecting" -> "Harici hedefe yeniden bağlanıyor"
        snapshot.status == "publishing" && snapshot.outputStatus == "forwarding" ->
            "Yayın alınıyor ve hedefe aktarılıyor"
        snapshot.status == "publishing" -> "DJI RC 2 yayını alınıyor"
        snapshot.status == "connected" -> "DJI RC 2 bağlandı"
        snapshot.status == "error" -> "RTMP alıcı hatası"
        else -> "DJI RC 2 yayını bekleniyor"
    }

    companion object {
        private const val ACTION_START = "com.djilivebridge.android.action.START_RELAY"
        private const val ACTION_STOP = "com.djilivebridge.android.action.STOP_RELAY"
        private const val EXTRA_DESTINATION_PROFILE_ID = "destination_profile_id"
        private const val CHANNEL_ID = "relay_status"
        private const val NOTIFICATION_ID = 1935
        private const val SNAPSHOT_INTERVAL_MS = 500L
        private const val WAKE_LOCK_TIMEOUT_MS = 6L * 60L * 60L * 1_000L

        fun start(
            context: Context,
            destinationProfileId: String,
        ) {
            val intent = Intent(context, RelayForegroundService::class.java)
                .setAction(ACTION_START)
                .putExtra(EXTRA_DESTINATION_PROFILE_ID, destinationProfileId)
            ContextCompat.startForegroundService(context, intent)
        }

        fun stop(context: Context) {
            context.startService(
                Intent(context, RelayForegroundService::class.java).setAction(ACTION_STOP),
            )
        }
    }
}
