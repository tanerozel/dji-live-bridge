import XCTest
@testable import StreamMyDrone

private let profileTarget = FacebookConnection(userId: "user-a", userName: "Ayşe", targetId: "user-a", targetName: "Ayşe",
                                               page: false, title: "Drone flight", privacy: "SELF")
private let pageTarget = FacebookConnection(userId: "user-a", userName: "Ayşe", targetId: "page-a", targetName: "Farm Page",
                                            page: true, title: "Drone flight", privacy: "EVERYONE")
private let streamUrl = "rtmps://live-api-s.facebook.com:443/rtmp/FB-123-0-AbC?s_bl=1&s_psm=1&s_sw=0&s_vt=api-s&a=XyZ"

private final class FakeFacebook: FacebookTransport {
    struct Call { let method: String; let path: String; let params: [String: String]; let token: String }
    var calls: [Call] = []
    var answer: (Call) throws -> [String: Any]

    init(_ answer: @escaping (Call) throws -> [String: Any]) { self.answer = answer }

    func request(_ method: String, _ path: String, params: [String: String], token: String) async throws -> [String: Any] {
        let call = Call(method: method, path: path, params: params, token: token)
        calls.append(call)
        return try answer(call)
    }
}

/// Answers like Graph API for one user, one Page and one live video whose status can change.
private func graph(user: String = "user-a", status: String = "LIVE", url: String = streamUrl) -> FakeFacebook {
    FakeFacebook { call in
        switch (call.method, call.path) {
        case ("GET", "me"): return ["id": user, "name": "Ayşe"]
        case ("GET", "page-a"): return ["access_token": "page-token"]
        case ("POST", let path) where path.hasSuffix("/live_videos"): return ["id": "video-a", "secure_stream_url": url]
        case ("GET", "video-a"): return ["id": "video-a", "status": status]
        default: return ["success": true]
        }
    }
}

final class FacebookAPITests: XCTestCase {
    func testProfileLiveVideoIsRecordedBeforeTheStreamIsHandedOut() async throws {
        let remote = graph()
        var recorded: String?
        let video = try await prepareFacebookLiveVideo(FacebookAPI(transport: remote), connection: profileTarget,
                                                       title: "Morning flight", userToken: "user-token") { recorded = $0 }
        XCTAssertEqual(recorded, "video-a")
        XCTAssertEqual(video.serverUrl, "rtmps://live-api-s.facebook.com:443/rtmp")
        XCTAssertEqual(video.key, "FB-123-0-AbC?s_bl=1&s_psm=1&s_sw=0&s_vt=api-s&a=XyZ")
        let create = try XCTUnwrap(remote.calls.first { $0.method == "POST" })
        XCTAssertEqual(create.token, "user-token")
        XCTAssertEqual(create.params["status"], "LIVE_NOW")
        XCTAssertEqual(create.params["title"], "Morning flight")
        XCTAssertEqual(create.params["privacy"], #"{"value":"SELF"}"#)
    }

    func testPageLiveVideoUsesThePageTokenAndNoProfileAudience() async throws {
        let remote = graph()
        _ = try await prepareFacebookLiveVideo(FacebookAPI(transport: remote), connection: pageTarget,
                                               title: "Drone flight", userToken: "user-token") { _ in }
        let create = try XCTUnwrap(remote.calls.first { $0.method == "POST" })
        XCTAssertEqual(create.path, "page-a/live_videos")
        XCTAssertEqual(create.token, "page-token")
        XCTAssertNil(create.params["privacy"])
    }

    func testAnotherSignedInUserNeverCreatesALiveVideo() async {
        let remote = graph(user: "user-b")
        do {
            _ = try await prepareFacebookLiveVideo(FacebookAPI(transport: remote), connection: profileTarget,
                                                   title: "Drone flight", userToken: "user-token") { _ in XCTFail("Must not record") }
            XCTFail("Expected an error")
        } catch { XCTAssertEqual(error as? FacebookError, FacebookError("facebook_error_account")) }
        XCTAssertEqual(remote.calls.map(\.method), ["GET"])
    }

    func testAnUnusableStreamURLDeletesTheUnseenLiveVideo() async {
        let remote = graph(status: "UNPUBLISHED", url: "rtmp://insecure.example/rtmp/key")
        do {
            _ = try await prepareFacebookLiveVideo(FacebookAPI(transport: remote), connection: profileTarget,
                                                   title: "Drone flight", userToken: "user-token") { _ in XCTFail("Must not record") }
            XCTFail("Expected an error")
        } catch {}
        XCTAssertEqual(remote.calls.last?.method, "DELETE")
        XCTAssertEqual(remote.calls.last?.path, "video-a")
    }

    func testFailingToRecordEndsTheLiveVideo() async {
        let remote = graph(status: "LIVE")
        struct DiskFull: Error {}
        do {
            _ = try await prepareFacebookLiveVideo(FacebookAPI(transport: remote), connection: profileTarget,
                                                   title: "Drone flight", userToken: "user-token") { _ in throw DiskFull() }
            XCTFail("Expected an error")
        } catch {}
        XCTAssertEqual(remote.calls.last?.params["end_live_video"], "true")
    }

    func testFinishIsIdempotentForEndedAndMissingLiveVideos() async throws {
        for ended in ["LIVE_STOPPED", "VOD", "PROCESSING"] {
            let remote = graph(status: ended)
            try await FacebookAPI(transport: remote).finish("video-a", token: "token")
            XCTAssertEqual(remote.calls.map(\.method), ["GET"])
        }
        let missing = FakeFacebook { _ in throw FacebookError.notFound }
        try await FacebookAPI(transport: missing).finish("video-a", token: "token")
        XCTAssertEqual(missing.calls.count, 1)
    }

    func testGraphErrorsMapToWhatTheUserCanDo() {
        XCTAssertEqual(facebookFailure(code: 200, subcode: 1363144).key, "facebook_error_live")
        XCTAssertEqual(facebookFailure(code: 200, subcode: 1363120).key, "facebook_error_live")
        XCTAssertEqual(facebookFailure(code: 190, subcode: 0).key, "facebook_error_auth")
        XCTAssertEqual(facebookFailure(code: 613, subcode: 0).key, "facebook_error_rate")
        XCTAssertEqual(facebookFailure(code: 100, subcode: 33), .notFound)
        XCTAssertEqual(facebookFailure(code: 368, subcode: 0).key, "facebook_error_api")
    }

    func testOnlyFacebookAddressesAreOpened() {
        XCTAssertEqual(facebookPermalink("/user-a/videos/1/")?.absoluteString, "https://www.facebook.com/user-a/videos/1/")
        XCTAssertNil(facebookPermalink("https://facebook.com.evil.example/x"))
        XCTAssertNil(facebookPermalink("http://www.facebook.com/x"))
    }

    func testStreamURLSplitsIntoServerAndKey() throws {
        let (server, key) = try splitFacebookStreamUrl(streamUrl)
        XCTAssertEqual(server, "rtmps://live-api-s.facebook.com:443/rtmp")
        XCTAssertTrue(key.hasPrefix("FB-123-0-AbC"))
        XCTAssertThrowsError(try splitFacebookStreamUrl("rtmps://live-api-s.facebook.com:443/live/key"))
        XCTAssertThrowsError(try splitFacebookStreamUrl("rtmps://live-api-s.facebook.com:443/rtmp/"))
    }

    func testLoginCallbackNeedsTheSameStateAndReadsTheGrant() throws {
        let now = Date(timeIntervalSince1970: 1_000)
        let url = URL(string: "fb123://authorize/#access_token=EAAB%2Bx&expires_in=5184000&granted_scopes=public_profile,publish_video&state=s1")!
        let result = try parseFacebookCallback(url, state: "s1", now: now)
        XCTAssertEqual(result.token, "EAAB+x")
        XCTAssertEqual(result.expires, now.addingTimeInterval(5_184_000))
        XCTAssertEqual(result.scopes, ["public_profile", "publish_video"])
        XCTAssertThrowsError(try parseFacebookCallback(url, state: "other"))
        let denied = URL(string: "fb123://authorize/?error=access_denied&error_reason=user_denied&state=s1#_=_")!
        XCTAssertThrowsError(try parseFacebookCallback(denied, state: "s1"))
    }

    func testPermissionsAreCheckedPerTarget() {
        XCTAssertTrue(FacebookPermissions.allow(["publish_video"], page: false))
        XCTAssertFalse(FacebookPermissions.allow(["publish_video"], page: true))
        XCTAssertTrue(FacebookPermissions.allow(["pages_show_list", "pages_read_engagement", "pages_manage_posts"], page: true))
    }
}

@MainActor
final class FacebookBroadcastManagerTests: XCTestCase {
    private func directory() throws -> URL {
        let url = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
        addTeardownBlock { try? FileManager.default.removeItem(at: url) }
        return url
    }

    func testPrepareHandsOutTheStreamAndAnOwnerEndsOnlyItsOwnVideo() async throws {
        let remote = graph()
        let store = FacebookSessionStore(directory: try directory())
        let manager = FacebookBroadcastManager(store: store, api: FacebookAPI(transport: remote), tokenProvider: { _, _ in "user-token" })
        let profile = DestinationProfile(id: "p1", name: "Facebook", kind: .facebook, serverUrl: "rtmps://live-api-s.facebook.com:443/rtmp",
                                         facebook: profileTarget)
        let owner = UUID()
        let credentials = try await manager.prepare(profile, title: "Drone flight", owner: owner)
        XCTAssertEqual(credentials.serverUrl, "rtmps://live-api-s.facebook.com:443/rtmp")
        XCTAssertEqual(try store.load().map(\.liveVideoId), ["video-a"])
        try await manager.finish(profileId: "p1", owner: UUID())
        XCTAssertEqual(try store.load().count, 1, "Another attempt's stop must not end this video")
        try await manager.finish(profileId: "p1", owner: owner)
        XCTAssertEqual(try store.load(), [])
        XCTAssertEqual(remote.calls.last?.params["end_live_video"], "true")
    }

    func testSignedOutCleanupKeepsTheRecordForALaterRetry() async throws {
        let store = FacebookSessionStore(directory: try directory())
        try store.save([FacebookSession(profileId: "p1", owner: UUID(), userId: "user-a", targetId: "user-a", page: false, liveVideoId: "video-a")])
        let manager = FacebookBroadcastManager(store: store, api: FacebookAPI(transport: graph()),
                                               tokenProvider: { _, _ in throw FacebookError("facebook_error_auth") })
        do { try await manager.recoverOnce(); XCTFail("Expected an error") } catch {}
        XCTAssertEqual(try store.load().count, 1)
    }
}

final class FacebookProfileStoreTests: XCTestCase {
    func testConnectedFacebookProfileKeepsNoKey() throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: directory) }
        let keys = MemoryKeys()
        let store = DestinationProfileStore(directory: directory, keys: keys)
        let saved = try store.save(existingId: nil, name: "Facebook", kind: .facebook, serverUrl: "rtmps://live-api-s.facebook.com:443/rtmp",
                                   streamKey: "", facebook: pageTarget)
        let profile = try XCTUnwrap(saved.profiles.first)
        XCTAssertEqual(try store.load().profiles.first?.facebook, pageTarget)
        XCTAssertTrue(keys.values.isEmpty)
        XCTAssertThrowsError(try store.save(existingId: profile.id, name: "Facebook", kind: .facebook,
                                            serverUrl: "rtmps://live-api-s.facebook.com:443/rtmp", streamKey: ""))
    }
}

private final class MemoryKeys: StreamKeyStore {
    var values: [String: String] = [:]
    func save(_ key: String, for profileId: String) throws { values[profileId] = key }
    func load(for profileId: String) throws -> String {
        guard let key = values[profileId] else { throw KeychainError(status: errSecItemNotFound) }
        return key
    }
    func delete(for profileId: String) { values[profileId] = nil }
}
