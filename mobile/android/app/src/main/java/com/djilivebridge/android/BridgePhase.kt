package com.djilivebridge.android

/** The relay state reduced to the situations the screen explains to the user. */
enum class BridgePhase {
    /** Nothing is running; the setup steps are shown. */
    IDLE,

    /** Nothing is running because the last start failed. */
    START_FAILED,
    STARTING,
    WAITING_FOR_DRONE,

    /** DJI Fly opened the connection but has not started publishing yet. */
    DRONE_CONNECTED,
    CONNECTING_TARGET,
    LIVE,

    /** The drone stream still arrives; the target connection dropped and is being retried. */
    RECONNECTING,
    RECEIVER_ERROR,
    ;

    /** The drone stream reaches the phone, so stopping now ends a broadcast. */
    val isStreaming: Boolean
        get() = this == CONNECTING_TARGET || this == LIVE || this == RECONNECTING
}

fun bridgePhase(state: RelayServiceUiState): BridgePhase {
    val snapshot = state.snapshot
    if (!state.isActive) {
        return if (snapshot.status == "error") BridgePhase.START_FAILED else BridgePhase.IDLE
    }
    return when {
        snapshot.status == "error" -> BridgePhase.RECEIVER_ERROR
        snapshot.status == "starting" -> BridgePhase.STARTING
        snapshot.outputStatus == "reconnecting" -> BridgePhase.RECONNECTING
        snapshot.status == "publishing" && snapshot.outputStatus == "forwarding" -> BridgePhase.LIVE
        snapshot.status == "publishing" -> BridgePhase.CONNECTING_TARGET
        snapshot.status == "connected" -> BridgePhase.DRONE_CONNECTED
        else -> BridgePhase.WAITING_FOR_DRONE
    }
}
