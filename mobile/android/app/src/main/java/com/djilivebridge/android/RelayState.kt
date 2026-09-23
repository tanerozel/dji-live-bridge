package com.djilivebridge.android

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import org.json.JSONObject

object NativeRelay {
    init {
        System.loadLibrary("dji_relay_core")
    }

    external fun nativeStart(
        targetServerUrl: String,
        targetStreamKey: String,
        tlsCaFile: String,
    ): String

    external fun nativeStartLocal(): String
    external fun nativeSnapshot(): String
    external fun nativeStop()
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
    val virtualCameraStatus: String = "stopped",
    val virtualCameraDetail: String = "Sanal kamera çıkışı kapalı",
    val virtualCameraClients: Long = 0,
    val droppedVirtualCameraFrames: Long = 0,
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
                virtualCameraStatus = value.optString("virtualCameraStatus", "stopped"),
                virtualCameraDetail = value.optString(
                    "virtualCameraDetail",
                    "Sanal kamera çıkışı kapalı",
                ),
                virtualCameraClients = value.optLong("virtualCameraClients"),
                droppedVirtualCameraFrames = value.optLong("droppedVirtualCameraFrames"),
            )
        }
    }
}

data class RelayServiceUiState(
    val isActive: Boolean = false,
    val snapshot: RelaySnapshot = RelaySnapshot(),
)

object RelayServiceState {
    var value by mutableStateOf(RelayServiceUiState())
        private set

    fun starting() {
        value = RelayServiceUiState(
            isActive = true,
            snapshot = RelaySnapshot(
                status = "starting",
                detail = "Arka plan aktarım servisi hazırlanıyor",
                outputStatus = "starting",
                outputDetail = "Harici RTMP hedefi hazırlanıyor",
            ),
        )
    }

    fun running(snapshot: RelaySnapshot) {
        value = value.copy(isActive = true, snapshot = snapshot)
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
