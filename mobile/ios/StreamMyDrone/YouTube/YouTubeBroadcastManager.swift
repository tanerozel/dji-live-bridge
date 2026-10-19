import Foundation

struct YouTubeSession: Codable, Equatable {
    let profileId: String
    let owner: UUID
    let email: String
    let channelId: String
    let broadcastId: String
}

/// Recovery IDs only, excluded from backup. No bearer token or ingestion credential is written.
final class YouTubeSessionStore {
    private let file: URL
    init(directory: URL? = nil) {
        let base = directory ?? FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
        file = base.appendingPathComponent("youtube-sessions-v1.json")
    }
    func load() throws -> [YouTubeSession] {
        guard FileManager.default.fileExists(atPath: file.path) else { return [] }
        return try JSONDecoder().decode([YouTubeSession].self, from: Data(contentsOf: file))
    }
    func save(_ sessions: [YouTubeSession]) throws {
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

/**
 * One ordered lane for remote lifecycle operations, independent of the native relay queue.
 * Swift actors are reentrant across await: an explicit task chain also serializes network waits.
 * Every attempt owns its session, so a delayed stop cannot complete a newer broadcast.
 */
@MainActor
final class YouTubeBroadcastManager {
    static let shared = YouTubeBroadcastManager()
    typealias APIProvider = @MainActor (String) async throws -> YouTubeAPI
    private let store: YouTubeSessionStore
    private let apiProvider: APIProvider
    private var tail: Task<Void, Never>?
    private var recoveryStarted = false

    init(store: YouTubeSessionStore = YouTubeSessionStore(), apiProvider: APIProvider? = nil) {
        self.store = store
        self.apiProvider = apiProvider ?? { try await YouTubeAuthorization.shared.api(for: $0) }
    }

    private func ordered<T>(_ action: @escaping @MainActor () async throws -> T) async throws -> T {
        let previous = tail
        // These tasks intentionally outlive cancellation of a screen or a preparation caller.
        // Once Google creates an event its ID must be saved/cleaned even if the user pressed stop.
        let task = Task { @MainActor in
            await previous?.value
            return try await action()
        }
        tail = Task { _ = await task.result }
        return try await task.value
    }

    /// Creates and binds this broadcast's event; `title` is the one confirmed before going live.
    func prepare(_ profile: DestinationProfile, title: String? = nil, owner: UUID) async throws {
        guard var connection = profile.youtube else { return }
        if let title { connection.title = try validateBroadcastTitle(title) }
        try await ordered { [self] in
            try await finishStored(profileId: profile.id, owner: nil)
            let api = try await apiProvider(connection.accountEmail)
            try await api.checkChannel(connection.channelId)
            let id = try await api.createBroadcast(connection)
            let record = YouTubeSession(profileId: profile.id, owner: owner, email: connection.accountEmail,
                                        channelId: connection.channelId, broadcastId: id)
            do {
                var sessions = try store.load()
                sessions.append(record)
                try store.save(sessions) // Persist before bind, native attach, or any cancellation check.
                try await api.bind(id, streamId: connection.streamId)
            } catch {
                // If completion fails, keep the recovery record for the next app launch/retry.
                if (try? await api.finish(id)) != nil {
                    try? store.save(try store.load().filter { $0 != record })
                }
                throw error
            }
        }
    }

    func finish(profileId: String, owner: UUID? = nil) async throws {
        try await ordered { [self] in try await finishStored(profileId: profileId, owner: owner) }
    }

    /// The app's current broadcast for `profileId`, for reading its state.
    func session(for profileId: String) -> YouTubeSession? {
        (try? store.load())?.last { $0.profileId == profileId }
    }

    /// Recovery runs once when the controller is created, before new preparation enters the lane.
    func recoverOnce() async throws {
        guard !recoveryStarted else { return }
        recoveryStarted = true
        try await recover()
    }

    func recover() async throws {
        try await ordered { [self] in
            var failed = false
            for record in try store.load() {
                do { try await finishStored(profileId: record.profileId, owner: record.owner) }
                catch { failed = true }
            }
            if failed { throw YouTubeError("youtube_error_end") }
        }
    }

    private func finishStored(profileId: String, owner: UUID?) async throws {
        for record in try store.load().filter({ $0.profileId == profileId && (owner == nil || $0.owner == owner) }) {
            let api = try await apiProvider(record.email)
            try await api.checkChannel(record.channelId)
            try await api.finish(record.broadcastId)
            try store.save(try store.load().filter { $0 != record })
        }
    }
}
