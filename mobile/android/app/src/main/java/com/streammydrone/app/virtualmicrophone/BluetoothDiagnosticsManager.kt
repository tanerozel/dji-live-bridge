package com.streammydrone.app.virtualmicrophone

import android.Manifest
import android.annotation.SuppressLint
import android.bluetooth.BluetoothAdapter
import android.bluetooth.BluetoothDevice
import android.bluetooth.BluetoothManager
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.content.pm.PackageManager
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.os.ParcelUuid
import android.os.SystemClock
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.core.content.ContextCompat

internal data class PairedDiagnosticDevice(val key: String, val label: String)

/** Visible-dialog device/SDP observations. The control service owns its socket independently. */
internal class BluetoothDiagnosticsManager(context: Context) {
    private val context = context.applicationContext
    private val handler = Handler(Looper.getMainLooper())
    private val adapter: BluetoothAdapter? get() = context.getSystemService(BluetoothManager::class.java)?.adapter
    private var monitorRegistered = false
    private var uuidReceiver: BroadcastReceiver? = null
    private var timeout: Runnable? = null
    private var devicesByKey = emptyMap<String, BluetoothDevice>()

    var adapterStatus by mutableStateOf(AdapterStatus.UNKNOWN)
        private set
    var pairedDevices by mutableStateOf(emptyList<PairedDiagnosticDevice>())
        private set
    var selectedKey by mutableStateOf<String?>(null)
        private set
    var cachedServices by mutableStateOf(emptySet<StandardAudioService>())
        private set
    var attempt by mutableStateOf(SdpAttempt())
        private set
    var errorCode by mutableStateOf<String?>(null)
        private set
    var privilegedPermission by mutableStateOf(false)
        private set

    val hasConnectPermission: Boolean get() = Build.VERSION.SDK_INT < 31 ||
        ContextCompat.checkSelfPermission(context, Manifest.permission.BLUETOOTH_CONNECT) == PackageManager.PERMISSION_GRANTED

    val canConnect: Boolean get() = adapterStatus == AdapterStatus.ON && hasConnectPermission &&
        selectedKey in devicesByKey && attempt.status != SdpStatus.PENDING && !VirtualMicrophoneControlState.value.isActive

    private val monitor = object : BroadcastReceiver() {
        override fun onReceive(context: Context, intent: Intent) {
            if (intent.action == BluetoothAdapter.ACTION_STATE_CHANGED || intent.action == BluetoothDevice.ACTION_BOND_STATE_CHANGED) refresh()
        }
    }

    fun start() {
        if (!monitorRegistered) {
            try {
                ContextCompat.registerReceiver(context, monitor, IntentFilter().apply {
                    addAction(BluetoothAdapter.ACTION_STATE_CHANGED)
                    addAction(BluetoothDevice.ACTION_BOND_STATE_CHANGED)
                }, ContextCompat.RECEIVER_EXPORTED)
                monitorRegistered = true
            } catch (_: SecurityException) {
                errorCode = "BLUETOOTH_RECEIVER_PERMISSION_DENIED"
            }
        }
        refresh()
    }

    fun stop() {
        cancelQuery()
        if (monitorRegistered) {
            unregister(monitor)
            monitorRegistered = false
        }
    }

    @SuppressLint("MissingPermission") // Rechecked here; permission can still race, so SecurityException is handled.
    fun refresh() {
        privilegedPermission = ContextCompat.checkSelfPermission(context, Manifest.permission.BLUETOOTH_PRIVILEGED) == PackageManager.PERMISSION_GRANTED
        try {
            val currentAdapter = adapter
            adapterStatus = when {
                currentAdapter == null -> AdapterStatus.UNAVAILABLE
                !hasConnectPermission -> AdapterStatus.PERMISSION_REQUIRED
                currentAdapter.isEnabled -> AdapterStatus.ON
                else -> AdapterStatus.OFF
            }
            if (adapterStatus != AdapterStatus.ON) {
                cancelQuery()
                clearPeers()
                return
            }
            devicesByKey = currentAdapter!!.bondedDevices.associateBy { it.address }
            pairedDevices = devicesByKey.map { (key, device) ->
                PairedDiagnosticDevice(key, device.name?.takeIf { it.isNotBlank() } ?: "Bluetooth")
            }.sortedBy { it.label }
            if (selectedKey !in devicesByKey) {
                cancelQuery()
                selectedKey = preferredControlPeer(pairedDevices, VirtualMicrophoneControlState.value.peerAddress)
                cachedServices = emptySet()
                attempt = SdpAttempt(generation = attempt.generation + 1)
            }
            readCache()
        } catch (_: SecurityException) {
            permissionLost()
        } catch (_: IllegalStateException) {
            cancelQuery()
            clearPeers()
            adapterStatus = AdapterStatus.UNKNOWN
            errorCode = "BLUETOOTH_UNAVAILABLE"
        }
    }

    fun selectDevice(key: String) {
        if (VirtualMicrophoneControlState.value.isActive) return
        if (key == selectedKey) return
        cancelQuery()
        selectedKey = key.takeIf { it in devicesByKey }
        attempt = SdpAttempt(generation = attempt.generation + 1)
        errorCode = null
        readCache()
    }

    fun connect() {
        refresh()
        if (!canConnect) return
        val key = selectedKey ?: return
        VirtualMicrophoneControlService.start(context, key)
    }

    fun disconnect() {
        VirtualMicrophoneControlService.stop(context)
    }

    @SuppressLint("MissingPermission")
    private fun readCache() {
        try {
            cachedServices = standardAudioServices(devicesByKey[selectedKey]?.uuids.orEmpty().map { it.uuid })
        } catch (_: SecurityException) {
            permissionLost()
        }
    }

    @SuppressLint("MissingPermission")
    fun queryServices() {
        if (VirtualMicrophoneControlState.value.isActive) return
        cancelQuery()
        refresh()
        val key = selectedKey ?: return
        val device = devicesByKey[key] ?: return
        if (adapterStatus != AdapterStatus.ON) return
        errorCode = null
        attempt = attempt.begin(key, SystemClock.elapsedRealtime(), SDP_TIMEOUT_MS)
        val generation = attempt.generation
        val receiver = object : BroadcastReceiver() {
            override fun onReceive(context: Context, intent: Intent) {
                if (intent.action != BluetoothDevice.ACTION_UUID) return
                try {
                    @Suppress("DEPRECATION")
                    val peer = intent.getParcelableExtra<BluetoothDevice>(BluetoothDevice.EXTRA_DEVICE) ?: return
                    if (peer.address != key) return
                    @Suppress("DEPRECATION")
                    val uuids = intent.getParcelableArrayExtra(BluetoothDevice.EXTRA_UUID)?.filterIsInstance<ParcelUuid>()?.map { it.uuid }
                    val next = attempt.receive(generation, key, SystemClock.elapsedRealtime(), uuids)
                    if (next != attempt) {
                        attempt = next
                        if (next.status == SdpStatus.TIMED_OUT) errorCode = "SDP_TIMEOUT"
                        finishQuery()
                    }
                } catch (_: SecurityException) {
                    permissionLost()
                }
            }
        }
        try {
            ContextCompat.registerReceiver(context, receiver, IntentFilter(BluetoothDevice.ACTION_UUID), ContextCompat.RECEIVER_EXPORTED)
            uuidReceiver = receiver
            timeout = Runnable {
                attempt = attempt.timeout(generation, SystemClock.elapsedRealtime())
                if (attempt.status == SdpStatus.TIMED_OUT) errorCode = "SDP_TIMEOUT"
                finishQuery()
            }.also { handler.postDelayed(it, SDP_TIMEOUT_MS) }
            if (!device.fetchUuidsWithSdp()) {
                attempt = attempt.copy(status = SdpStatus.FAILED)
                errorCode = "SDP_REQUEST_REJECTED"
                finishQuery()
            }
        } catch (_: SecurityException) {
            permissionLost()
        } catch (_: IllegalStateException) {
            attempt = attempt.copy(status = SdpStatus.FAILED)
            errorCode = "BLUETOOTH_UNAVAILABLE"
            finishQuery()
        }
    }

    fun cancelQuery() {
        attempt = attempt.cancel()
        finishQuery()
    }

    private fun finishQuery() {
        timeout?.let(handler::removeCallbacks)
        timeout = null
        uuidReceiver?.let(::unregister)
        uuidReceiver = null
    }

    private fun clearPeers() {
        pairedDevices = emptyList()
        devicesByKey = emptyMap()
        selectedKey = null
        cachedServices = emptySet()
        attempt = SdpAttempt(generation = attempt.generation + 1)
    }

    private fun unregister(receiver: BroadcastReceiver) {
        try {
            context.unregisterReceiver(receiver)
        } catch (_: IllegalArgumentException) {
            // Already removed during lifecycle cleanup.
        } catch (_: SecurityException) {
            errorCode = "BLUETOOTH_RECEIVER_PERMISSION_DENIED"
        }
    }

    private fun permissionLost() {
        cancelQuery()
        adapterStatus = AdapterStatus.PERMISSION_REQUIRED
        clearPeers()
        errorCode = "BLUETOOTH_PERMISSION_DENIED"
    }

    fun report(): MicrophoneDiagnosticReport = MicrophoneDiagnosticReport(
        model = "${Build.MANUFACTURER} ${Build.MODEL}",
        androidVersion = Build.VERSION.RELEASE,
        sdk = Build.VERSION.SDK_INT,
        evidence = MicrophoneEvidence(
            adapterStatus, privilegedPermission, attempt.services,
            controlPhase = VirtualMicrophoneControlState.value.phase.name,
            controlStage = VirtualMicrophoneControlState.value.stage,
            controlEvents = VirtualMicrophoneControlState.value.events,
            disconnectReason = VirtualMicrophoneControlState.value.disconnectReason,
        ),
        cachedServices = cachedServices,
        sdp = attempt.status,
        errorCode = VirtualMicrophoneControlState.value.errorCode ?: errorCode,
    )

    private companion object {
        const val SDP_TIMEOUT_MS = 15_000L
    }
}
