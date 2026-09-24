package com.djilivebridge.android

import android.os.SystemClock
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import org.json.JSONObject

object NativeRelay {
    init {
        System.loadLibrary("dji_relay_core")
    }

    /** Starts receiving DJI Fly with no platform attached; returns an error message or "". */
    external fun nativeStartReceiver(): String

    /** Sends the received stream to a platform; returns an error message or "". */
    external fun nativeGoLive(
        targetServerUrl: String,
        targetStreamKey: String,
        tlsCaFile: String,
    ): String

    /** Stops sending to the platform; the drone stays connected. */
    external fun nativeEndLive()

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
    val detail: String = "RTMP alıcısı kapalı",
    val remoteAddress: String? = null,
    val receivedBytes: Long = 0,
    val bitrateKbps: Double = 0.0,
    val videoCodec: String? = null,
    val audioCodec: String? = null,
    val videoFrames: Long = 0,
    val audioFrames: Long = 0,
    val rejectedPublishAttempts: Long = 0,
    val outputStatus: String = "disabled",
    val outputDetail: String = "Harici hedef yapılandırılmadı",
    val outboundBytes: Long = 0,
    val droppedOutputFrames: Long = 0,
    val outputReconnectAttempts: Long = 0,
    val outputSecure: Boolean = false,
) {
    companion object {
        fun fromJson(raw: String): RelaySnapshot {
            val value = JSONObject(raw)
            return RelaySnapshot(
                status = value.optString("status", "error"),
                detail = value.optString("detail", "Durum okunamadı"),
                remoteAddress = value.optionalString("remoteAddress"),
                receivedBytes = value.optLong("receivedBytes"),
                bitrateKbps = value.optDouble("bitrateKbps"),
                videoCodec = value.optionalString("videoCodec"),
                audioCodec = value.optionalString("audioCodec"),
                videoFrames = value.optLong("videoFrames"),
                audioFrames = value.optLong("audioFrames"),
                rejectedPublishAttempts = value.optLong("rejectedPublishAttempts"),
                outputStatus = value.optString("outputStatus", "disabled"),
                outputDetail = value.optString("outputDetail", "Harici hedef yapılandırılmadı"),
                outboundBytes = value.optLong("outboundBytes"),
                droppedOutputFrames = value.optLong("droppedOutputFrames"),
                outputReconnectAttempts = value.optLong("outputReconnectAttempts"),
                outputSecure = value.optBoolean("outputSecure"),
            )
        }
    }
}

data class RelayServiceUiState(
    /** The receiver runs: DJI Fly can connect and its picture shows on the phone. */
    val isActive: Boolean = false,
    val snapshot: RelaySnapshot = RelaySnapshot(),
    /** The destination profile the stream goes to; null while the phone only receives. */
    val liveProfileId: String? = null,
    /**
     * [SystemClock.elapsedRealtime] when the current drone stream first reached the target.
     * Target reconnects keep it; it resets when the drone stops publishing or the stream ends.
     */
    val liveSinceElapsedMillis: Long? = null,
    /** Display name of the video playing in place of the drone, or null for a real flight. */
    val testVideoName: String? = null,
    /** Why the last attempt to go live or play a test video failed; cleared by the next one. */
    val notice: RelayNotice? = null,
) {
    val isLive: Boolean
        get() = liveProfileId != null
}

/** Something that went wrong without stopping the receiver, shown until the next attempt. */
data class RelayNotice(val title: String, val message: String)

object RelayServiceState {
    var value by mutableStateOf(RelayServiceUiState())
        private set

    fun starting() {
        value = RelayServiceUiState(
            isActive = true,
            snapshot = RelaySnapshot(status = "starting", detail = "Alıcı hazırlanıyor"),
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

    fun goingLive(profileId: String) {
        value = value.copy(liveProfileId = profileId, liveSinceElapsedMillis = null, notice = null)
    }

    fun notLive(notice: RelayNotice? = null) {
        value = value.copy(liveProfileId = null, liveSinceElapsedMillis = null, notice = notice)
    }

    fun testVideo(name: String?, notice: RelayNotice? = null) {
        value = value.copy(testVideoName = name, notice = notice)
    }

    fun stopped(snapshot: RelaySnapshot = RelaySnapshot()) {
        value = RelayServiceUiState(isActive = false, snapshot = snapshot)
    }

    fun failed(message: String) {
        value = RelayServiceUiState(
            isActive = false,
            snapshot = RelaySnapshot(status = "error", detail = message),
        )
    }
}

private fun JSONObject.optionalString(name: String): String? =
    if (isNull(name)) null else optString(name).takeIf(String::isNotBlank)
