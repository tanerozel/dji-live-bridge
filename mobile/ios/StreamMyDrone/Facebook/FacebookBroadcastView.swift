import Foundation
import Observation

/// What Facebook last said about one connected profile's live video. Read-only: it never starts a
/// live video or changes its audience.
@MainActor
@Observable
final class FacebookBroadcastView {
    let profileId: String
    let connection: FacebookConnection
    private(set) var status: FacebookLiveStatus?
    var error: UiText?
    @ObservationIgnored private var task: Task<Void, Never>?

    init(profileId: String, connection: FacebookConnection) {
        self.profileId = profileId
        self.connection = connection
    }

    var live: Bool { status?.status == "LIVE" }
    var pending: Bool { error == nil && lifecycleKey == "facebook_status_pending" }
    var privacyKey: String { connection.privacyKey }

    var lifecycleKey: String {
        switch status?.status {
        case "LIVE": "facebook_status_live"
        case "LIVE_STOPPED", "VOD", "PROCESSING": "facebook_status_ended"
        default: "facebook_status_pending"
        }
    }

    /// The live video once Facebook named it, the profile or Page until then.
    var url: URL { status?.permalink ?? URL(string: "https://www.facebook.com/\(connection.targetId)")! }

    /// Asks Facebook every 30 seconds while the app is on screen; Graph API limits calls per user and hour.
    func start(_ manager: FacebookBroadcastManager) {
        guard task == nil else { return }
        // A check from before must not stay labelled live after the app comes back.
        status = nil
        task = Task { [weak self] in
            var token: String?
            while let self, !Task.isCancelled {
                do {
                    let next: FacebookLiveStatus?
                    if let session = manager.session(for: self.profileId) {
                        guard session.userId == self.connection.userId, session.targetId == self.connection.targetId else {
                            throw FacebookError("facebook_error_account")
                        }
                        let current: String
                        if let token { current = token } else { current = try await manager.readToken(for: session) }
                        token = current
                        next = try await FacebookAPI(transport: GraphFacebookTransport()).status(session.liveVideoId, token: current)
                    } else {
                        next = nil
                    }
                    guard !Task.isCancelled else { return }
                    self.status = next
                    self.error = nil
                } catch {
                    guard !Task.isCancelled else { return }
                    // A failed check must not leave an old "live" in place.
                    self.status = nil
                    self.error = facebookErrorText(error)
                    token = nil
                    if let failure = error as? FacebookError, ["facebook_error_auth", "facebook_error_rate"].contains(failure.key) { return }
                }
                try? await Task.sleep(for: .seconds(30))
            }
        }
    }

    func stop() {
        task?.cancel()
        task = nil
    }
}
