package com.streammydrone.app.virtualmicrophone

import java.util.UUID

/** Evidence from SDP describes remote services only. It never establishes an audio session. */
internal enum class StandardAudioService(val shortId: String) {
    HANDSFREE("111e"), HANDSFREE_AUDIO_GATEWAY("111f"), HEADSET("1108"), HEADSET_AUDIO_GATEWAY("1112");

    val uuid: UUID get() = UUID.fromString("0000$shortId-0000-1000-8000-00805f9b34fb")
}

internal fun standardAudioServices(uuids: Collection<UUID>): Set<StandardAudioService> =
    StandardAudioService.entries.filterTo(linkedSetOf()) { it.uuid in uuids }

/** The user identified QCOM-BTD as their RC 2. Never choose between matching devices. */
internal fun preferredControlPeer(devices: List<PairedDiagnosticDevice>, activePeer: String?): String? =
    activePeer?.takeIf { peer -> devices.any { it.key == peer } }
        ?: devices.singleOrNull { it.label.equals("QCOM-BTD", ignoreCase = true) }?.key

internal enum class AdapterStatus { PERMISSION_REQUIRED, UNAVAILABLE, OFF, ON, UNKNOWN }
internal enum class SdpStatus { IDLE, PENDING, RECEIVED, EMPTY, TIMED_OUT, FAILED, CANCELLED }

/** Control evidence is separate from audio and DJI Fly's externally observed microphone state. */
internal data class MicrophoneEvidence(
    val adapter: AdapterStatus,
    val privilegedPermission: Boolean,
    val remoteServices: Set<StandardAudioService>,
    val controlPhase: String = "IDLE",
    val controlStage: String? = null,
    val controlEvents: List<String> = emptyList(),
    val disconnectReason: String? = null,
) {
    val hfpServiceLevelConnection: String get() = when (controlPhase) {
        "CONNECTED" -> "ESTABLISHED"
        "NEGOTIATING" -> "NEGOTIATING"
        "CONNECTING" -> "RFCOMM_CONNECTING"
        "DISCONNECTED" -> "DISCONNECTED"
        "FAILED" -> "FAILED"
        else -> "NOT_ESTABLISHED"
    }
    val scoAudio: String get() = "NOT_STARTED"
    val codec: String get() = "NOT_NEGOTIATED"
    val microphoneCapture: String get() = "NOT_STARTED"
    val audioRouting: String get() = "NOT_STARTED"
    val djiFlyMicrophone: String get() = "UNVERIFIED"
}

/** Generation and peer guards reject callbacks from cancelled or replaced requests. */
internal data class SdpAttempt(
    val generation: Long = 0,
    val peer: String? = null,
    val deadline: Long = 0,
    val status: SdpStatus = SdpStatus.IDLE,
    val services: Set<StandardAudioService> = emptySet(),
) {
    fun begin(peer: String, now: Long, timeoutMs: Long): SdpAttempt =
        SdpAttempt(generation + 1, peer, now + timeoutMs, SdpStatus.PENDING)

    fun receive(generation: Long, peer: String, now: Long, uuids: Collection<UUID>?): SdpAttempt {
        if (status != SdpStatus.PENDING || this.generation != generation || this.peer != peer) return this
        if (now >= deadline) return copy(status = SdpStatus.TIMED_OUT)
        return copy(
            status = if (uuids.isNullOrEmpty()) SdpStatus.EMPTY else SdpStatus.RECEIVED,
            services = standardAudioServices(uuids.orEmpty()),
        )
    }

    fun timeout(generation: Long, now: Long): SdpAttempt =
        if (status == SdpStatus.PENDING && this.generation == generation && now >= deadline) {
            copy(status = SdpStatus.TIMED_OUT)
        } else this

    fun cancel(): SdpAttempt = if (status == SdpStatus.PENDING) copy(status = SdpStatus.CANCELLED) else this
}

internal data class MicrophoneDiagnosticReport(
    val model: String,
    val androidVersion: String,
    val sdk: Int,
    val evidence: MicrophoneEvidence,
    val cachedServices: Set<StandardAudioService>,
    val sdp: SdpStatus,
    val errorCode: String?,
) {
    /** No peer names, MAC addresses, arbitrary UUIDs, exception messages, or audio are exported. */
    fun sanitizedText(): String = buildString {
        appendLine("Stream My Drone — Virtual Bluetooth Microphone diagnostics")
        appendLine("HFP control test for controller microphone recognition. No audio is sent.")
        appendLine("Device model: $model")
        appendLine("Android: $androidVersion (API $sdk)")
        appendLine("Bluetooth adapter: ${evidence.adapter}")
        appendLine("BLUETOOTH_PRIVILEGED granted: ${evidence.privilegedPermission}")
        appendLine("Public RFCOMM HFP control: AVAILABLE_WITH_BLUETOOTH_PERMISSION")
        appendLine("Cached remote standard services: ${cachedServices.sortedBy { it.shortId }.joinToString { "${it.name} (0x${it.shortId})" }.ifEmpty { "NONE_OR_UNKNOWN" }}")
        appendLine("SDP request: $sdp")
        appendLine("SDP remote standard services: ${evidence.remoteServices.sortedBy { it.shortId }.joinToString { "${it.name} (0x${it.shortId})" }.ifEmpty { "NONE_OR_UNKNOWN" }}")
        appendLine("SDP broadcasts have no request ID; service presence is not connection or audio proof.")
        appendLine("The following describes this diagnostics module only, not other phone Bluetooth sessions:")
        appendLine("Control phase: ${diagnosticCode(evidence.controlPhase)}")
        appendLine("Last HFP stage: ${diagnosticCode(evidence.controlStage)}")
        val events = evidence.controlEvents.mapNotNull(::safeDiagnosticCode)
        appendLine("Control events: ${events.joinToString().ifEmpty { "NONE" }}")
        appendLine("Verified HFP stages: ${events.filter { it.endsWith("_VALID_RESPONSE_AND_OK") }.joinToString().ifEmpty { "NONE" }}")
        appendLine("HFP service level connection: ${evidence.hfpServiceLevelConnection}")
        appendLine("SCO/eSCO audio: ${evidence.scoAudio}")
        appendLine("Codec: ${evidence.codec}")
        appendLine("Microphone capture: ${evidence.microphoneCapture}")
        appendLine("Audio routing: ${evidence.audioRouting}")
        appendLine("DJI Fly microphone recognition: ${evidence.djiFlyMicrophone}")
        appendLine("Last error: ${diagnosticCode(errorCode)}")
        appendLine("Disconnect reason: ${diagnosticCode(evidence.disconnectReason)}")
        append("No audio is captured or transmitted. Control connection alone does not verify DJI Fly microphone recognition.")
    }
}

private val diagnosticCodePattern = Regex("[A-Z0-9_]{1,80}")
private fun safeDiagnosticCode(value: String?): String? = value?.takeIf { diagnosticCodePattern.matches(it) }
private fun diagnosticCode(value: String?): String = safeDiagnosticCode(value) ?: if (value == null) "NONE" else "REDACTED"
