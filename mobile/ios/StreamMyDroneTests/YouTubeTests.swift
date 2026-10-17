import XCTest
@testable import StreamMyDrone

private func connection() -> YouTubeConnection {
    YouTubeConnection(accountEmail: "test@example.com", channelId: "channel", channelTitle: "My channel",
                      streamId: "stream", title: "Drone flight", privacy: "unlisted", madeForKids: false)
}

private final class FakeYouTube: YouTubeTransport {
    struct Call { let method: String; let path: String; let query: [String: String]; let body: [String: Any]? }
    var calls: [Call] = []
    var status = "live"
    var channel = "channel"
    var failBinding = false
    var failFinish = false
    var nextId = 0
    var streamResponse: [String: Any] = ["id": "stream", "cdn": ["ingestionInfo": ["rtmpsIngestionAddress": "rtmps://a.rtmps.youtube.com/live2", "streamName": "key-1234"]]]
    var onBind: (() throws -> Void)?
    var suspendCreate: CheckedContinuation<Void, Never>?
    var holdCreate = false
    var created: (() -> Void)?

    func request(_ method: String, _ path: String, query: [String: String], body: [String: Any]?) async throws -> [String: Any] {
        calls.append(Call(method: method, path: path, query: query, body: body))
        if path.hasSuffix("channels") { return ["items": [["id": channel, "snippet": ["title": "My channel"]]]] }
        if path.hasSuffix("liveStreams") && method == "POST" { return streamResponse }
        if path.hasSuffix("liveBroadcasts") && method == "POST" {
            nextId += 1
            if holdCreate {
                await withCheckedContinuation { suspendCreate = $0; created?() }
            }
            return ["id": "broadcast-\(nextId)"]
        }
        if path.hasSuffix("/bind") {
            try onBind?()
            if failBinding { throw YouTubeError() }
        }
        if method == "GET" && path.hasSuffix("liveBroadcasts") {
            if failFinish { throw YouTubeError("youtube_error_network") }
            return status == "missing" ? ["items": []] : ["items": [["status": ["lifeCycleStatus": status]]]]
        }
        return [:]
    }
}

final class YouTubeAPITests: XCTestCase {
    func testBroadcastCarriesExplicitAudienceVisibilityAndAutoLifecycle() async throws {
        let remote = FakeYouTube()
        let api = YouTubeAPI(transport: remote)
        _ = try await api.createBroadcast(connection())
        let call = try XCTUnwrap(remote.calls.last)
        let status = try XCTUnwrap(call.body?["status"] as? [String: Any])
        XCTAssertEqual(status["privacyStatus"] as? String, "unlisted")
        XCTAssertEqual(status["selfDeclaredMadeForKids"] as? Bool, false)
        let details = try XCTUnwrap(call.body?["contentDetails"] as? [String: Any])
        XCTAssertEqual(details["enableAutoStart"] as? Bool, true)
        XCTAssertEqual(details["enableAutoStop"] as? Bool, true)
        XCTAssertEqual((details["monitorStream"] as? [String: Any])?["enableMonitorStream"] as? Bool, false)
    }

    func testInvalidTitleNeverCreatesRemoteEvent() async throws {
        let remote = FakeYouTube()
        var settings = connection()
        settings.title = "<invalid>"
        do { _ = try await YouTubeAPI(transport: remote).createBroadcast(settings); XCTFail() }
        catch { XCTAssertEqual((error as? YouTubeError)?.key, "youtube_error_title") }
        XCTAssertTrue(remote.calls.isEmpty)
    }

    func testTitleUsesUnicodeCodePointsAndRejectsControlCharacters() throws {
        XCTAssertNoThrow(try validateYouTubeTitle(String(repeating: "🚁", count: 100)))
        XCTAssertThrowsError(try validateYouTubeTitle(String(repeating: "a", count: 101)))
        XCTAssertThrowsError(try validateYouTubeTitle("flight\nnow"))
        XCTAssertThrowsError(try validateYouTubeTitle("  "))
    }

    func testCreateStreamAcceptsOnlyValidatedRTMPSAndCleansMalformedInput() async throws {
        let remote = FakeYouTube()
        let api = YouTubeAPI(transport: remote)
        let stream = try await api.createStream()
        XCTAssertEqual(stream.key, "key-1234")
        remote.streamResponse = ["id": "bad-input", "cdn": ["ingestionInfo": ["rtmpsIngestionAddress": "rtmp://example.com/live", "streamName": "key-1234"]]]
        do { _ = try await api.createStream(); XCTFail() } catch {}
        XCTAssertEqual(remote.calls.last?.method, "DELETE")
        XCTAssertEqual(remote.calls.last?.query["id"], "bad-input")
    }

    func testMissingAndCompletedBroadcastsNeedNoMutation() async throws {
        for status in ["missing", "complete", "revoked"] {
            let remote = FakeYouTube()
            remote.status = status
            try await YouTubeAPI(transport: remote).finish("broadcast")
            XCTAssertEqual(remote.calls.count, 1)
        }
    }

    func testOnlyUnstartedEventsAreDeleted() async throws {
        for status in ["created", "ready", "testing", "live"] {
            let remote = FakeYouTube()
            remote.status = status
            try await YouTubeAPI(transport: remote).finish("broadcast")
            XCTAssertEqual(remote.calls.last?.method, ["created", "ready"].contains(status) ? "DELETE" : "POST")
            if status == "live" { XCTAssertEqual(remote.calls.last?.query["broadcastStatus"], "complete") }
        }
    }

    func testKnownAPIErrorsAreSanitized() {
        XCTAssertEqual(GoogleYouTubeTransport.failure(status: 401, json: [:]).key, "youtube_error_auth")
        for (reason, key) in [("quotaExceeded", "youtube_error_quota"), ("accessNotConfigured", "youtube_error_setup"),
                              ("liveStreamingNotEnabled", "youtube_error_live"), ("forbidden", "youtube_error_auth")] {
            let json: [String: Any] = ["error": ["errors": [["reason": reason]], "message": "secret must not escape"]]
            let error = GoogleYouTubeTransport.failure(status: 403, json: json)
            XCTAssertEqual(error.key, key)
            XCTAssertFalse(String(describing: error).contains("secret"))
        }
    }
}

@MainActor
final class YouTubeLifecycleTests: XCTestCase {
    private var directory: URL!
    private var sessions: YouTubeSessionStore!
    private var remote: FakeYouTube!
    private var manager: YouTubeBroadcastManager!
    private var profile: DestinationProfile {
        DestinationProfile(id: "profile", name: "YouTube", kind: .youtube, serverUrl: "rtmps://a.rtmps.youtube.com/live2", youtube: connection())
    }

    override func setUpWithError() throws {
        directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        sessions = YouTubeSessionStore(directory: directory)
        remote = FakeYouTube()
        let remote = remote!
        manager = YouTubeBroadcastManager(store: sessions, apiProvider: { _ in YouTubeAPI(transport: remote) })
    }
    override func tearDownWithError() throws { try? FileManager.default.removeItem(at: directory) }

    func testSessionIsDurableBeforeBindingAndHasNoCredentials() async throws {
        let owner = UUID()
        remote.onBind = { [sessions] in
            XCTAssertEqual(try sessions!.load().first?.owner, owner)
        }
        try await manager.prepare(profile, owner: owner)
        let data = try Data(contentsOf: directory.appendingPathComponent("youtube-sessions-v1.json"))
        let json = try XCTUnwrap(JSONSerialization.jsonObject(with: data) as? [[String: Any]])
        XCTAssertEqual(Set(json[0].keys), Set(["profileId", "owner", "email", "channelId", "broadcastId"]))
        let values = try directory.appendingPathComponent("youtube-sessions-v1.json").resourceValues(forKeys: [.isExcludedFromBackupKey])
        XCTAssertEqual(values.isExcludedFromBackup, true)
    }

    func testBindFailureCompletesAndRemovesRecoveryRecord() async throws {
        remote.failBinding = true
        do { try await manager.prepare(profile, owner: UUID()); XCTFail() } catch {}
        XCTAssertTrue(try sessions.load().isEmpty)
        XCTAssertTrue(remote.calls.contains { $0.path.hasSuffix("transition") })
    }

    func testCleanupFailureSurvivesRelaunchAndIsRetried() async throws {
        let owner = UUID()
        try await manager.prepare(profile, owner: owner)
        remote.failFinish = true
        do { try await manager.finish(profileId: profile.id, owner: owner); XCTFail() } catch {}
        XCTAssertEqual(try sessions.load().count, 1)
        remote.failFinish = false
        let remote = remote!
        let relaunched = YouTubeBroadcastManager(store: sessions, apiProvider: { _ in YouTubeAPI(transport: remote) })
        try await relaunched.recover()
        XCTAssertTrue(try sessions.load().isEmpty)
    }

    func testWrongChannelCannotCreateOrFinishAnotherChannelsEvent() async throws {
        remote.channel = "different-channel"
        do { try await manager.prepare(profile, owner: UUID()); XCTFail() } catch {}
        XCTAssertFalse(remote.calls.contains { $0.method == "POST" })
    }

    func testDelayedOldStopCannotCompleteNewSession() async throws {
        let first = UUID(), second = UUID()
        try await manager.prepare(profile, owner: first)
        try await manager.prepare(profile, owner: second)
        let count = remote.calls.count
        try await manager.finish(profileId: profile.id, owner: first)
        XCTAssertEqual(remote.calls.count, count)
        XCTAssertEqual(try sessions.load().first?.owner, second)
    }

    func testRecoveryRunsOnlyOncePerProcessManager() async throws {
        try await manager.recoverOnce()
        let owner = UUID()
        try await manager.prepare(profile, owner: owner)
        let count = remote.calls.count
        try await manager.recoverOnce()
        XCTAssertEqual(remote.calls.count, count)
        XCTAssertEqual(try sessions.load().first?.owner, owner)
    }

    func testFailedRecoveryPersistenceCompensatesTheRemoteEvent() async throws {
        let blocked = directory.appendingPathComponent("not-a-directory")
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        try Data().write(to: blocked)
        let remote = remote!
        let manager = YouTubeBroadcastManager(store: YouTubeSessionStore(directory: blocked),
                                              apiProvider: { _ in YouTubeAPI(transport: remote) })
        do { try await manager.prepare(profile, owner: UUID()); XCTFail() } catch {}
        XCTAssertFalse(remote.calls.contains { $0.path.hasSuffix("/bind") })
        XCTAssertTrue(remote.calls.contains { $0.path.hasSuffix("/transition") })
    }

    func testStopDuringRemoteCreationWaitsThenCleansTheCreatedEvent() async throws {
        remote.holdCreate = true
        let reached = expectation(description: "Remote creation is in progress")
        remote.created = { reached.fulfill() }
        let owner = UUID()
        let prepare = Task { try await manager.prepare(profile, owner: owner) }
        await fulfillment(of: [reached], timeout: 2)
        let stop = Task { try await manager.finish(profileId: profile.id, owner: owner) }
        prepare.cancel() // Caller cancellation must not lose the late server response/ID.
        remote.suspendCreate?.resume()
        remote.suspendCreate = nil
        try await prepare.value
        try await stop.value
        XCTAssertTrue(try sessions.load().isEmpty)
        XCTAssertEqual(remote.calls.last?.query["broadcastStatus"], "complete")
    }
}
