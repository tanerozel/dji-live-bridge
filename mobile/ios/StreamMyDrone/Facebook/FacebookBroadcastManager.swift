import Foundation

struct FacebookSession: Codable, Equatable {
    let profileId: String
    let owner: UUID
    let userId: String
    let targetId: String
    let page: Bool
    let liveVideoId: String
}

/// Recovery IDs only, excluded from backup. No access token or stream URL is written.
final class FacebookSessionStore {
    private let file: URL
    init(directory: URL? = nil) {
        let base = directory ?? FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
        file = base.appendingPathComponent("facebook-sessions-v1.json")
    }
    func load() throws -> [FacebookSession] {
        guard FileManager.default.fileExists(atPath: file.path) else { return [] }
        return try JSONDecoder().decode([FacebookSession].self, from: Data(contentsOf: file))
    }
    func save(_ sessions: [FacebookSession]) throws {
        var directory = file.deletingLastPathComponent()
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        var values = URLResourceValues()
        values.isExcludedFromBackup = true
        try directory.setResourceValues(values)
        try JSONEncoder().encode(sessions).write(to: file, options: [.atomic, .completeFileProtectionUntilFirstUserAuthentication])
        var url = file
        try? url.setResourceValues(values)
    }
}

/// Creates a live video for `connection` and records its ID before the stream is handed out.
func prepareFacebookLiveVideo(_ api: FacebookAPI, connection: FacebookConnection, title: String, userToken: String,
                              record: (String) throws -> Void) async throws -> FacebookLiveVideo {
    try await api.checkAccount(connection.userId, token: userToken)
    let token = try await api.targetToken(connection.targetId, page: connection.page, userToken: userToken)
    let video = try await api.create(connection, title: title, token: token)
    do { try record(video.id) } catch {
        try? await api.finish(video.id, token: token)
        throw error
    }
    return video
}

/**
 * One ordered lane for Facebook's remote lifecycle, independent of the native relay queue, like
 * `YouTubeBroadcastManager`. Every attempt owns its session, so a delayed stop cannot end a
 * newer live video. The stream URL Facebook returns goes to the caller and is never stored.
 */
@MainActor
final class FacebookBroadcastManager {
    static let shared = FacebookBroadcastManager()
    typealias TokenProvider = @MainActor (_ userId: String, _ page: Bool) throws -> String
    private let store: FacebookSessionStore
    private let api: FacebookAPI
    private let tokenProvider: TokenProvider
    private var tail: Task<Void, Never>?
    private var recoveryStarted = false

    init(store: FacebookSessionStore = FacebookSessionStore(), api: FacebookAPI = FacebookAPI(transport: GraphFacebookTransport()),
         tokenProvider: TokenProvider? = nil) {
        self.store = store
        self.api = api
        self.tokenProvider = tokenProvider ?? { try FacebookAuthorization.shared.token(for: $0, page: $1) }
    }

    private func ordered<T>(_ action: @escaping @MainActor () async throws -> T) async throws -> T {
        let previous = tail
        // Once Facebook creates a live video its ID must be saved/cleaned even if the user pressed stop.
        let task = Task { @MainActor in
            await previous?.value
            return try await action()
        }
        tail = Task { _ = await task.result }
        return try await task.value
    }

    /// Creates this broadcast's live video and returns where to send the video.
    func prepare(_ profile: DestinationProfile, title: String, owner: UUID) async throws -> DestinationCredentials {
        guard let connection = profile.facebook else { throw DestinationProfileError("profile_error_not_found") }
        let title = try validateBroadcastTitle(title)
        return try await ordered { [self] in
            try await finishStored(profileId: profile.id, owner: nil)
            let token = try tokenProvider(connection.userId, connection.page)
            let video = try await prepareFacebookLiveVideo(api, connection: connection, title: title, userToken: token) { id in
                var sessions = try store.load()
                sessions.append(FacebookSession(profileId: profile.id, owner: owner, userId: connection.userId,
                                                targetId: connection.targetId, page: connection.page, liveVideoId: id))
                try store.save(sessions) // Persist before the relay attaches or any cancellation check.
            }
            return DestinationCredentials(serverUrl: video.serverUrl, streamKey: video.key)
        }
    }

    func finish(profileId: String, owner: UUID? = nil) async throws {
        try await ordered { [self] in try await finishStored(profileId: profileId, owner: owner) }
    }

    /// The app's current live video for `profileId`, for reading its state.
    func session(for profileId: String) -> FacebookSession? {
        (try? store.load())?.last { $0.profileId == profileId }
    }

    /// A token for reading `session`'s live video: the user's, or the Page's.
    func readToken(for session: FacebookSession) async throws -> String {
        try await api.targetToken(session.targetId, page: session.page, userToken: tokenProvider(session.userId, session.page))
    }

    /// Recovery runs once when the controller is created, before new preparation enters the lane.
    func recoverOnce() async throws {
        guard !recoveryStarted else { return }
        recoveryStarted = true
        try await ordered { [self] in
            var failed = false
            for record in try store.load() {
                do { try await finishStored(profileId: record.profileId, owner: record.owner) }
                catch { failed = true }
            }
            if failed { throw FacebookError("facebook_error_end") }
        }
    }

    private func finishStored(profileId: String, owner: UUID?) async throws {
        for record in try store.load().filter({ $0.profileId == profileId && (owner == nil || $0.owner == owner) }) {
            let token = try await readToken(for: record)
            try await api.finish(record.liveVideoId, token: token)
            try store.save(try store.load().filter { $0 != record })
        }
    }
}
