package com.djilivebridge.android

/** The relay state reduced to the situations the screen explains to the user. */
enum class BridgePhase {
    /** The receiver is off. */
    IDLE,

    /** The receiver is off because it could not start. */
    START_FAILED,
    STARTING,
    WAITING_FOR_DRONE,

    /** DJI Fly opened the connection but has not started publishing yet. */
    DRONE_CONNECTED,

    /** The drone's picture reaches the phone; nothing goes to a platform yet. */
    PREVIEW,
    CONNECTING_TARGET,
    LIVE,

    /** The drone stream still arrives; the target connection dropped and is being retried. */
    RECONNECTING,
    RECEIVER_ERROR,
    ;

    /** The drone stream goes to a platform, so stopping now ends a broadcast. */
    val isStreaming: Boolean
        get() = this == CONNECTING_TARGET || this == LIVE || this == RECONNECTING

    /** The drone's picture arrives, whether or not it goes out. */
    val hasPicture: Boolean
        get() = this == PREVIEW || isStreaming
}

/** The output reaches the platform; "congested" still does, with video frames skipped. */
internal val LIVE_OUTPUT_STATUSES = setOf("forwarding", "congested")

fun bridgePhase(state: RelayServiceUiState): BridgePhase {
    val snapshot = state.snapshot
    if (!state.isActive) {
        return if (snapshot.status == "error") BridgePhase.START_FAILED else BridgePhase.IDLE
    }
    val publishing = snapshot.status == "publishing"
    return when {
        snapshot.status == "error" -> BridgePhase.RECEIVER_ERROR
        snapshot.status == "starting" -> BridgePhase.STARTING
        publishing && !state.isLive -> BridgePhase.PREVIEW
        publishing && snapshot.outputStatus == "reconnecting" -> BridgePhase.RECONNECTING
        publishing && snapshot.outputStatus in LIVE_OUTPUT_STATUSES -> BridgePhase.LIVE
        publishing -> BridgePhase.CONNECTING_TARGET
        snapshot.status == "connected" -> BridgePhase.DRONE_CONNECTED
        else -> BridgePhase.WAITING_FOR_DRONE
    }
}
