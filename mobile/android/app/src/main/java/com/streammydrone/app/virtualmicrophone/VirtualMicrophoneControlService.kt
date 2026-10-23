package com.streammydrone.app.virtualmicrophone

import android.Manifest
import android.annotation.SuppressLint
import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.bluetooth.BluetoothAdapter
import android.bluetooth.BluetoothDevice
import android.bluetooth.BluetoothManager
import android.bluetooth.BluetoothSocket
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.content.pm.PackageManager
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.Handler
import android.os.IBinder
import android.os.Looper
import androidx.core.app.NotificationCompat
import androidx.core.app.ServiceCompat
import androidx.core.content.ContextCompat
import com.streammydrone.app.MainActivity
import com.streammydrone.app.R
import java.io.InputStream
import java.io.OutputStream
import java.util.UUID

/** Holds only a selected paired controller's HFP control socket, independently of the RTMP relay. */
class VirtualMicrophoneControlService : Service() {
    private val main = Handler(Looper.getMainLooper())
    private var currentGeneration = 0L
    private var currentStartId = 0
    private var peerAddress: String? = null
    private var session: HfpControlSession? = null
    private var worker: Thread? = null
    private var receiverRegistered = false
    private var initializationProblem: String? = null

    private val bluetoothChanges = object : BroadcastReceiver() {
        @SuppressLint("MissingPermission") // Service preflight + SecurityException handles a permission-revocation race.
        override fun onReceive(context: Context?, intent: Intent?) {
            if (!VirtualMicrophoneControlState.value.isActive) return
            when (intent?.action) {
                BluetoothAdapter.ACTION_STATE_CHANGED -> if (
                    intent.getIntExtra(BluetoothAdapter.EXTRA_STATE, BluetoothAdapter.ERROR) != BluetoothAdapter.STATE_ON
                ) finish(currentGeneration, "BLUETOOTH_OFF", failed = false)
                BluetoothDevice.ACTION_BOND_STATE_CHANGED -> {
                    val device = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
                        intent.getParcelableExtra(BluetoothDevice.EXTRA_DEVICE, BluetoothDevice::class.java)
                    } else {
                        @Suppress("DEPRECATION")
                        intent.getParcelableExtra(BluetoothDevice.EXTRA_DEVICE)
                    }
                    try {
                        if (device?.address == peerAddress && intent.getIntExtra(
                                BluetoothDevice.EXTRA_BOND_STATE, BluetoothDevice.ERROR,
                            ) != BluetoothDevice.BOND_BONDED
                        ) finish(currentGeneration, "DEVICE_UNPAIRED", failed = false)
                    } catch (_: SecurityException) {
                        finish(currentGeneration, "PERMISSION_DENIED", failed = true)
                    }
                }
            }
        }
    }

    override fun onCreate() {
        super.onCreate()
        try {
            getSystemService(NotificationManager::class.java).createNotificationChannel(
                NotificationChannel(
                    CHANNEL_ID, getString(R.string.vmic_control_notification_channel), NotificationManager.IMPORTANCE_LOW,
                ).apply { setShowBadge(false) },
            )
            ContextCompat.registerReceiver(
                this,
                bluetoothChanges,
                IntentFilter().apply {
                    addAction(BluetoothAdapter.ACTION_STATE_CHANGED)
                    addAction(BluetoothDevice.ACTION_BOND_STATE_CHANGED)
                },
                ContextCompat.RECEIVER_EXPORTED,
            )
            receiverRegistered = true
        } catch (_: SecurityException) {
            initializationProblem = "BLUETOOTH_RECEIVER_PERMISSION_DENIED"
        } catch (_: RuntimeException) {
            initializationProblem = "FOREGROUND_SERVICE_START_FAILED"
        }
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        currentStartId = startId
        when (intent?.action) {
            ACTION_START -> {
                val generation = intent.getLongExtra(EXTRA_GENERATION, -1)
                if (generation != VirtualMicrophoneControlState.value.generation || !VirtualMicrophoneControlState.value.isActive) {
                    if (session == null) stopSelfResult(startId)
                    return START_NOT_STICKY
                }
                if (session != null && currentGeneration == generation) return START_NOT_STICKY
                currentGeneration = generation
                peerAddress = intent.getStringExtra(EXTRA_PEER)
                val problem = initializationProblem ?: connectionProblem(this, peerAddress)
                if (problem != null) {
                    finish(generation, problem, failed = true)
                    return START_NOT_STICKY
                }
                if (!enterForeground()) return START_NOT_STICKY
                connect(generation, checkNotNull(peerAddress))
            }
            ACTION_STOP -> finish(currentGeneration, "USER_DISCONNECTED", failed = false)
            else -> if (session == null) stopSelfResult(startId)
        }
        return START_NOT_STICKY // A user must explicitly connect again after process death.
    }

    override fun onBind(intent: Intent?): IBinder? = null

    private fun enterForeground(): Boolean = try {
        ServiceCompat.startForeground(
            this,
            NOTIFICATION_ID,
            notification(connected = false),
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) ServiceInfo.FOREGROUND_SERVICE_TYPE_CONNECTED_DEVICE else 0,
        )
        true
    } catch (_: RuntimeException) {
        finish(currentGeneration, "FOREGROUND_SERVICE_START_FAILED", failed = true)
        false
    }

    private fun connect(generation: Long, address: String) {
        // Android calls this on the main thread. The RFCOMM operations themselves run off the UI.
        val next = HfpControlSession(
            transport = AndroidRfcommTransport(this, address),
            healthCheck = { connectionProblem(this, address) },
            progress = { event ->
                main.post {
                    if (currentGeneration == generation && VirtualMicrophoneControlState.value.isActive) {
                        VirtualMicrophoneControlState.progress(generation, event)
                        if (event.connected) {
                            getSystemService(NotificationManager::class.java).notify(NOTIFICATION_ID, notification(connected = true))
                        }
                    }
                }
            },
        )
        session = next
        worker = Thread({
            val reason = try {
                next.run()
                "RFCOMM_CLOSED"
            } catch (error: HfpControlFailure) {
                error.code
            } catch (_: SecurityException) {
                "PERMISSION_DENIED"
            } catch (_: RuntimeException) {
                // Never copy exception messages or the selected Bluetooth address into state/logs.
                "RFCOMM_UNEXPECTED_FAILURE"
            } finally {
                next.close()
            }
            main.post {
                if (session === next && currentGeneration == generation) {
                    finish(generation, reason, failed = reason !in NORMAL_DISCONNECTS)
                }
            }
        }, "hfp-control-session").apply { isDaemon = true; start() }
    }

    private fun finish(generation: Long, code: String, failed: Boolean) {
        if (generation != currentGeneration) return
        val closing = session
        session = null // Prevent a late worker terminal callback from replacing the chosen reason.
        closing?.cancel(code)
        worker?.interrupt()
        worker = null
        peerAddress = null
        VirtualMicrophoneControlState.finish(generation, code, failed)
        ServiceCompat.stopForeground(this, ServiceCompat.STOP_FOREGROUND_REMOVE)
        stopSelfResult(currentStartId)
    }

    override fun onDestroy() {
        session?.cancel("SERVICE_DESTROYED")
        session = null
        worker?.interrupt()
        worker = null
        peerAddress = null
        if (receiverRegistered) unregisterReceiver(bluetoothChanges)
        receiverRegistered = false
        VirtualMicrophoneControlState.finish(currentGeneration, "SERVICE_DESTROYED", failed = false)
        ServiceCompat.stopForeground(this, ServiceCompat.STOP_FOREGROUND_REMOVE)
        super.onDestroy()
    }

    private fun notification(connected: Boolean): Notification {
        val open = PendingIntent.getActivity(
            this, 11119,
            Intent(this, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP or Intent.FLAG_ACTIVITY_CLEAR_TOP),
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
        val disconnect = PendingIntent.getService(
            this, 11120,
            Intent(this, VirtualMicrophoneControlService::class.java).setAction(ACTION_STOP),
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
        return NotificationCompat.Builder(this, CHANNEL_ID)
            .setSmallIcon(R.drawable.ic_drone)
            .setColor(ContextCompat.getColor(this, R.color.bridge_brand))
            .setContentTitle(getString(R.string.vmic_control_notification_title))
            .setContentText(getString(if (connected) R.string.vmic_control_notification_connected else R.string.vmic_control_notification_connecting))
            .setContentIntent(open)
            .setOngoing(true)
            .setOnlyAlertOnce(true)
            .setCategory(NotificationCompat.CATEGORY_SERVICE)
            .addAction(0, getString(R.string.vmic_control_notification_disconnect), disconnect)
            .build()
    }

    companion object {
        private const val CHANNEL_ID = "bluetooth_microphone_control"
        private const val NOTIFICATION_ID = 11118
        private const val ACTION_START = "com.streammydrone.app.action.START_MICROPHONE_CONTROL"
        private const val ACTION_STOP = "com.streammydrone.app.action.STOP_MICROPHONE_CONTROL"
        private const val EXTRA_PEER = "selected_paired_device"
        private const val EXTRA_GENERATION = "control_generation"
        private val NORMAL_DISCONNECTS = setOf("CANCELLED", "USER_DISCONNECTED", "RFCOMM_REMOTE_EOF", "BLUETOOTH_OFF", "DEVICE_UNPAIRED")

        internal fun start(context: Context, deviceAddress: String) {
            onMain {
                if (VirtualMicrophoneControlState.value.isActive) return@onMain
                val generation = VirtualMicrophoneControlState.begin(deviceAddress)
                val problem = connectionProblem(context, deviceAddress)
                if (problem != null) {
                    VirtualMicrophoneControlState.finish(generation, problem, failed = true)
                    return@onMain
                }
                try {
                    ContextCompat.startForegroundService(
                        context,
                        Intent(context, VirtualMicrophoneControlService::class.java)
                            .setAction(ACTION_START)
                            .putExtra(EXTRA_PEER, deviceAddress)
                            .putExtra(EXTRA_GENERATION, generation),
                    )
                } catch (_: RuntimeException) {
                    VirtualMicrophoneControlState.finish(generation, "FOREGROUND_SERVICE_START_FAILED", failed = true)
                }
            }
        }

        internal fun stop(context: Context) {
            onMain {
                if (!VirtualMicrophoneControlState.value.isActive) return@onMain
                try {
                    context.startService(Intent(context, VirtualMicrophoneControlService::class.java).setAction(ACTION_STOP))
                } catch (_: RuntimeException) {
                    // stopService remains allowed even if background service starts are restricted.
                    context.stopService(Intent(context, VirtualMicrophoneControlService::class.java))
                    VirtualMicrophoneControlState.finish(VirtualMicrophoneControlState.value.generation, "USER_DISCONNECTED", failed = false)
                }
            }
        }

        private fun onMain(action: () -> Unit) {
            if (Looper.myLooper() == Looper.getMainLooper()) action() else Handler(Looper.getMainLooper()).post(action)
        }

        private fun connectionProblem(context: Context, address: String?): String? {
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S && ContextCompat.checkSelfPermission(
                    context, Manifest.permission.BLUETOOTH_CONNECT,
                ) != PackageManager.PERMISSION_GRANTED
            ) return "PERMISSION_DENIED"
            if (address == null || !BluetoothAdapter.checkBluetoothAddress(address)) return "INVALID_PAIRED_DEVICE"
            return try {
                val adapter = context.getSystemService(BluetoothManager::class.java)?.adapter ?: return "BLUETOOTH_UNAVAILABLE"
                if (!adapter.isEnabled) "BLUETOOTH_OFF"
                else if (adapter.bondedDevices.none { it.address == address }) "DEVICE_UNPAIRED"
                else null
            } catch (_: SecurityException) {
                "PERMISSION_DENIED"
            } catch (_: RuntimeException) {
                "BLUETOOTH_UNAVAILABLE"
            }
        }
    }

    private class AndroidRfcommTransport(private val context: Context, private val address: String) : HfpControlTransport {
        @Volatile private var socket: BluetoothSocket? = null
        @Volatile private var closed = false
        override val input: InputStream get() = checkNotNull(socket).inputStream
        override val output: OutputStream get() = checkNotNull(socket).outputStream

        @SuppressLint("MissingPermission") // Preflight/periodic checks require CONNECT; session sanitizes a revocation race.
        override fun connect() {
            val adapter = checkNotNull(context.getSystemService(BluetoothManager::class.java)?.adapter)
            val selected = adapter.bondedDevices.singleOrNull { it.address == address }
                ?: throw HfpControlFailure("DEVICE_UNPAIRED")
            val next = selected.createRfcommSocketToServiceRecord(HFP_AG)
            synchronized(this) {
                if (closed) { next.close(); throw HfpControlFailure("CANCELLED") }
                socket = next
            }
            next.connect()
        }

        override fun close() {
            val closing = synchronized(this) {
                closed = true
                socket.also { socket = null }
            }
            closing?.close()
        }

        companion object {
            private val HFP_AG: UUID = UUID.fromString("0000111f-0000-1000-8000-00805f9b34fb")
        }
    }
}
