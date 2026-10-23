package com.streammydrone.app.virtualmicrophone

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue

internal enum class VirtualMicrophoneControlPhase { IDLE, CONNECTING, NEGOTIATING, CONNECTED, DISCONNECTED, FAILED }

internal data class VirtualMicrophoneControlSnapshot(
    val generation: Long = 0,
    val phase: VirtualMicrophoneControlPhase = VirtualMicrophoneControlPhase.IDLE,
    /** Kept only in this process for selected-peer identity. Never put into reports or notifications. */
    val peerAddress: String? = null,
    val stage: String? = null,
    val events: List<String> = emptyList(),
    val agFeatures: Long? = null,
    val errorCode: String? = null,
    val disconnectReason: String? = null,
) {
    val isActive: Boolean get() = phase in setOf(
        VirtualMicrophoneControlPhase.CONNECTING,
        VirtualMicrophoneControlPhase.NEGOTIATING,
        VirtualMicrophoneControlPhase.CONNECTED,
    )
    val audioStatus: String get() = "NOT_STARTED"

    fun progress(generation: Long, event: HfpControlProgress): VirtualMicrophoneControlSnapshot {
        if (this.generation != generation || !isActive) return this
        return copy(
            phase = if (event.connected) VirtualMicrophoneControlPhase.CONNECTED else if (event.code == "RFCOMM_CONNECT_STARTED") {
                VirtualMicrophoneControlPhase.CONNECTING
            } else VirtualMicrophoneControlPhase.NEGOTIATING,
            stage = event.code,
            events = (events + event.code).takeLast(32),
            agFeatures = event.agFeatures ?: agFeatures,
        )
    }

    fun finish(generation: Long, code: String, failed: Boolean): VirtualMicrophoneControlSnapshot {
        if (this.generation != generation || !isActive) return this
        return copy(
            phase = if (failed) VirtualMicrophoneControlPhase.FAILED else VirtualMicrophoneControlPhase.DISCONNECTED,
            peerAddress = null,
            stage = code,
            events = (events + code).takeLast(32),
            errorCode = code.takeIf { failed },
            disconnectReason = code,
        )
    }
}

/** Only the service and its explicit start helper mutate this object, on the main thread. */
internal object VirtualMicrophoneControlState {
    var value by mutableStateOf(VirtualMicrophoneControlSnapshot())
        private set

    fun begin(peerAddress: String): Long {
        value = VirtualMicrophoneControlSnapshot(
            generation = value.generation + 1,
            phase = VirtualMicrophoneControlPhase.CONNECTING,
            peerAddress = peerAddress,
            stage = "START_REQUESTED",
            events = listOf("START_REQUESTED"),
        )
        return value.generation
    }

    fun progress(generation: Long, event: HfpControlProgress) { value = value.progress(generation, event) }
    fun finish(generation: Long, code: String, failed: Boolean) { value = value.finish(generation, code, failed) }
}
