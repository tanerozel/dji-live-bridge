import Foundation

/// The relay state reduced to the situations the screen explains to the user.
enum BridgePhase {
    /// The receiver is off.
    case idle
    /// The receiver is off because it could not start.
    case startFailed
    case starting
    case waitingForDrone
    /// DJI Fly opened the connection but has not started publishing yet.
    case droneConnected
    /// The drone's picture reaches the phone; nothing goes to a platform yet.
    case preview
    case connectingTarget
    case live
    /// The drone stream still arrives; the target connection dropped and is being retried.
    case reconnecting
    case receiverError

    /// The drone stream goes to a platform, so stopping now ends a broadcast.
    var isStreaming: Bool { self == .connectingTarget || self == .live || self == .reconnecting }

    /// The drone's picture arrives, whether or not it goes out.
    var hasPicture: Bool { self == .preview || isStreaming }
}

/// What the relay does right now, as the screen sees it.
struct RelayUiState: Equatable {
    /// The receiver runs: DJI Fly can connect and its picture shows on the phone.
    var isActive = false
    var snapshot = RelaySnapshot()
    /// The destination profiles the stream goes to; empty while the phone only receives.
    var liveProfileIds: [String] = []
    /// When the current drone stream first reached a platform (system uptime, in seconds).
    /// Platform reconnects keep it; it resets when the drone stops publishing or the stream ends.
    var liveSince: TimeInterval?
    /// Display name of the video playing in place of the drone, or nil for a real flight.
    var testVideoName: UiText?
    /// How far the test video's conversion to DJI Fly's format is, in percent; nil when not converting.
    var testVideoConverting: Int?
    /// Why the last attempt to go live or play a test video failed; cleared by the next one.
    var notice: RelayNotice?

    var isLive: Bool { !liveProfileIds.isEmpty }
}

/// Something that went wrong without stopping the receiver, shown until the next attempt.
struct RelayNotice: Hashable {
    let title: UiText
    let message: UiText
}

func bridgePhase(_ state: RelayUiState) -> BridgePhase {
    let snapshot = state.snapshot
    if !state.isActive {
        return snapshot.status == "error" ? .startFailed : .idle
    }
    let publishing = snapshot.status == "publishing"
    if snapshot.status == "error" { return .receiverError }
    if snapshot.status == "starting" { return .starting }
    if publishing && !state.isLive { return .preview }
    if publishing && snapshot.outputStatus == "reconnecting" { return .reconnecting }
    if publishing && liveOutputStatuses.contains(snapshot.outputStatus) { return .live }
    if publishing { return .connectingTarget }
    if snapshot.status == "connected" { return .droneConnected }
    return .waitingForDrone
}
