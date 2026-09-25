package com.streammydrone.app

import android.os.SystemClock
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import org.json.JSONObject

/** The tag of the app's log lines: `adb logcat -s DJIBridge` shows how the stream is doing. */
internal const val LOG_TAG = "DJIBridge"

object NativeRelay {
    init {
        System.loadLibrary("dji_relay_core")
    }

    /** Starts receiving DJI Fly with no platform attached; returns an error message or "". */
    external fun nativeStartReceiver(): String

    /**
     * Sends the received stream to one more platform, known by [outputId] (the destination
     * profile id); returns an error message or "".
     */
    external fun nativeGoLive(
        outputId: String,
        targetServerUrl: String,
        targetStreamKey: String,
        tlsCaFile: String,
    ): String

    /** Stops sending to the platform [outputId], or to all of them when it is ""; the drone stays connected. */
    external fun nativeEndLive(outputId: String)

    external fun nativeSnapshot(): String
    external fun nativeStop()

    /** Opens a preview session on the relay's video; returns its number. */
    external fun nativePreviewStart(): Long

    /**
     * The next video tag of [session] within [timeoutMs]: a 4-byte big-endian RTMP timestamp and
     * the FLV video tag body, or null when nothing arrived or the session ended.
     */
    external fun nativePreviewNext(session: Long, timeoutMs: Int): ByteArray?

    external fun nativePreviewStop(session: Long)
}

data class RelaySnapshot(
    val status: String = "stopped",
    /** Why the receiver stopped, in words for the screen; null unless [status] is "error". */
    val error: UiText? = null,
    val remoteAddress: String? = null,
    val receivedBytes: Long = 0,
    val bitrateKbps: Double = 0.0,
    val videoCodec: String? = null,
    val audioCodec: String? = null,
    val videoFrames: Long = 0,
    val audioFrames: Long = 0,
    val rejectedPublishAttempts: Long = 0,
    /** Pauses of 0.7 s or more in the drone's video since the receiver started, and the longest. */
    val stalls: Long = 0,
    val longestStallMs: Long = 0,
    /** Times DJI Fly started publishing again after its stream ended. */
    val sourceReconnects: Long = 0,
    /** Stalls and reconnects within the last minute. */
    val recentInterruptions: Long = 0,
    /** One entry per platform the stream is going to. */
    val outputs: List<OutputSnapshot> = emptyList(),
) {
    /** What the broadcast as a whole does: the best that any platform does. */
    val outputStatus: String
        get() = OUTPUT_STATUS_RANK.firstOrNull { status -> outputs.any { it.status == status } } ?: "disabled"

    val outboundBytes: Long
        get() = outputs.sumOf { it.outboundBytes }

    fun output(id: String): OutputSnapshot? = outputs.firstOrNull { it.id == id }

    companion object {
        fun fromJson(raw: String): RelaySnapshot {
            val value = JSONObject(raw)
            return RelaySnapshot(
                status = value.optString("status", "error"),
                error = value.optionalString("errorCode")?.let { code ->
                    nativeError(listOfNotNull(code, value.optionalString("errorDetail")).joinToString(": "))
                },
                remoteAddress = value.optionalString("remoteAddress"),
                receivedBytes = value.optLong("receivedBytes"),
                bitrateKbps = value.optDouble("bitrateKbps"),
                videoCodec = value.optionalString("videoCodec"),
                audioCodec = value.optionalString("audioCodec"),
                videoFrames = value.optLong("videoFrames"),
                audioFrames = value.optLong("audioFrames"),
                rejectedPublishAttempts = value.optLong("rejectedPublishAttempts"),
                stalls = value.optLong("stalls"),
                longestStallMs = value.optLong("longestStallMs"),
                sourceReconnects = value.optLong("sourceReconnects"),
                recentInterruptions = value.optLong("recentInterruptions"),
                outputs = value.optJSONArray("outputs")?.let { outputs ->
                    List(outputs.length()) { index ->
                        val output = outputs.getJSONObject(index)
                        OutputSnapshot(
                            id = output.optString("id"),
                            status = output.optString("status", "armed"),
                            reason = output.optionalString("reason"),
                            reasonDetail = output.optionalString("reasonDetail"),
                            retryInSeconds = output.optLong("retryInSeconds").takeIf { output.has("retryInSeconds") && !output.isNull("retryInSeconds") },
                            outboundBytes = output.optLong("outboundBytes"),
                            droppedFrames = output.optLong("droppedFrames"),
                            reconnectAttempts = output.optLong("reconnectAttempts"),
                            secure = output.optBoolean("secure"),
                        )
                    }
                }.orEmpty(),
            )
        }
    }
}

/** One platform's output. */
data class OutputSnapshot(
    val id: String,
    val status: String = "armed",
    /** While reconnecting: why, as a code, the technical cause, and the wait before retrying. */
    val reason: String? = null,
    val reasonDetail: String? = null,
    val retryInSeconds: Long? = null,
    val outboundBytes: Long = 0,
    val droppedFrames: Long = 0,
    val reconnectAttempts: Long = 0,
    val secure: Boolean = false,
)

/** Best first: a broadcast is live while any platform receives it. */
private val OUTPUT_STATUS_RANK = listOf(
    "forwarding", "congested", "ready", "connecting", "holding", "reconnecting", "armed", "error", "stopped",
)

data class RelayServiceUiState(
    /** The receiver runs: DJI Fly can connect and its picture shows on the phone. */
    val isActive: Boolean = false,
    val snapshot: RelaySnapshot = RelaySnapshot(),
    /** The destination profiles the stream goes to; empty while the phone only receives. */
    val liveProfileIds: List<String> = emptyList(),
    /**
     * [SystemClock.elapsedRealtime] when the current drone stream first reached the target.
     * Target reconnects keep it; it resets when the drone stops publishing or the stream ends.
     */
    val liveSinceElapsedMillis: Long? = null,
    /** Display name of the video playing in place of the drone, or null for a real flight. */
    val testVideoName: UiText? = null,
    /** How far the test video's conversion to DJI Fly's format is, in percent; null when not converting. */
    val testVideoConverting: Int? = null,
    /** Why the last attempt to go live or play a test video failed; cleared by the next one. */
    val notice: RelayNotice? = null,
) {
    val isLive: Boolean
        get() = liveProfileIds.isNotEmpty()
}

/** Something that went wrong without stopping the receiver, shown until the next attempt. */
data class RelayNotice(val title: UiText, val message: UiText)

object RelayServiceState {
    var value by mutableStateOf(RelayServiceUiState())
        private set

    fun starting() {
        value = RelayServiceUiState(
            isActive = true,
            snapshot = RelaySnapshot(status = "starting"),
        )
    }

    fun running(snapshot: RelaySnapshot) {
        val liveSince = when {
            !value.isLive || snapshot.status != "publishing" -> null
            value.liveSinceElapsedMillis != null -> value.liveSinceElapsedMillis
            snapshot.outputStatus in LIVE_OUTPUT_STATUSES -> SystemClock.elapsedRealtime()
            else -> null
        }
        value = value.copy(isActive = true, snapshot = snapshot, liveSinceElapsedMillis = liveSince)
    }

    /** Adds platforms to the broadcast; one already running keeps its timer. */
    fun goingLive(profileIds: List<String>) {
        val live = (value.liveProfileIds + profileIds).distinct()
        value = value.copy(liveProfileIds = live, notice = null)
    }

    /** Ends the broadcast on [profileIds], or everywhere when null. */
    fun notLive(profileIds: List<String>? = null, notice: RelayNotice? = null) {
        val live = profileIds?.let { ended -> value.liveProfileIds - ended.toSet() }.orEmpty()
        value = value.copy(
            liveProfileIds = live,
            liveSinceElapsedMillis = value.liveSinceElapsedMillis.takeIf { live.isNotEmpty() },
            notice = notice,
        )
    }

    fun testVideo(name: UiText?, notice: RelayNotice? = null) {
        value = value.copy(testVideoName = name, testVideoConverting = null, notice = notice)
    }

    fun testVideoConverting(percent: Int?) {
        value = value.copy(testVideoConverting = percent)
    }

    fun stopped(snapshot: RelaySnapshot = RelaySnapshot()) {
        value = RelayServiceUiState(isActive = false, snapshot = snapshot)
    }

    fun failed(message: UiText) {
        value = RelayServiceUiState(
            isActive = false,
            snapshot = RelaySnapshot(status = "error", error = message),
        )
    }
}

private fun JSONObject.optionalString(name: String): String? =
    if (isNull(name)) null else optString(name).takeIf(String::isNotBlank)
