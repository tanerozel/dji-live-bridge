import Foundation
import Observation

/// What YouTube last said about one connected profile's broadcast. Read-only: it never starts a
/// broadcast or changes its audience.
@MainActor
@Observable
final class YouTubeBroadcastView {
    let profileId: String
    let connection: YouTubeConnection
    private(set) var status: YouTubeBroadcastStatus?
    var error: UiText?
    @ObservationIgnored private var task: Task<Void, Never>?

    init(profileId: String, connection: YouTubeConnection) {
        self.profileId = profileId
        self.connection = connection
    }

    var live: Bool { status?.lifecycle == "live" }
    var testing: Bool { ["testing", "testStarting"].contains(status?.lifecycle ?? "") }
    var pending: Bool { error == nil && lifecycleKey == "youtube_status_pending" }

    var privacyKey: String {
        switch status?.privacy ?? connection.privacy {
        case "public": "youtube_public"
        case "private": "youtube_private"
        default: "youtube_unlisted"
        }
    }

    var lifecycleKey: String {
        switch status?.lifecycle {
        case "live": "youtube_status_live"
        case "testing", "testStarting": "youtube_status_testing"
        case "complete", "revoked": "youtube_status_ended"
        default: "youtube_status_pending"
        }
    }

    /// The broadcast's watch page once YouTube named it, YouTube Studio until then.
    var url: URL {
        guard let id = status?.id, var components = URLComponents(string: "https://www.youtube.com/watch") else {
            return URL(string: "https://studio.youtube.com/")!
        }
        components.queryItems = [URLQueryItem(name: "v", value: id)]
        return components.url ?? URL(string: "https://studio.youtube.com/")!
    }

    /// Asks YouTube every 15 seconds while the app is on screen.
    func start(_ manager: YouTubeBroadcastManager) {
        guard task == nil else { return }
        // A check from before must not stay labelled live after the app comes back.
        status = nil
        task = Task { [weak self] in
            while let self, !Task.isCancelled {
                do {
                    let next: YouTubeBroadcastStatus?
                    if let session = manager.session(for: self.profileId) {
                        guard session.channelId == self.connection.channelId, session.email == self.connection.accountEmail else {
                            throw YouTubeError("youtube_error_channel_changed")
                        }
                        let api = try await YouTubeAuthorization.shared.api(for: self.connection.accountEmail)
                        next = try await api.broadcastStatus(session.broadcastId, channelId: self.connection.channelId, streamId: self.connection.streamId)
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
                    self.error = youTubeErrorText(error)
                    if let failure = error as? YouTubeError, ["youtube_error_auth", "youtube_error_quota"].contains(failure.key) { return }
                }
                try? await Task.sleep(for: .seconds(15))
            }
        }
    }

    func stop() {
        task?.cancel()
        task = nil
    }
}
