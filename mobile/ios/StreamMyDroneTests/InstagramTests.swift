import XCTest
@testable import StreamMyDrone

/// The Instagram Producer rules, kept the same as the Android app's InstagramSetupTest and friends.
final class InstagramTests: XCTestCase {
    func testOnlyInstagramsOwnPagesAreCaptured() {
        XCTAssertTrue(instagramCaptureOrigin(URL(string: "https://www.instagram.com/")))
        XCTAssertTrue(instagramCaptureOrigin(URL(string: "https://www.instagram.com/live/producer/")))
        XCTAssertFalse(instagramCaptureOrigin(URL(string: "https://www.instagram.com/accounts/login/")))
        XCTAssertFalse(instagramCaptureOrigin(URL(string: "https://www.instagram.com/challenge/")))
        XCTAssertFalse(instagramCaptureOrigin(URL(string: "https://www.instagram.com/checkpoint")))
        XCTAssertFalse(instagramCaptureOrigin(URL(string: "http://www.instagram.com/")))
        XCTAssertFalse(instagramCaptureOrigin(URL(string: "https://instagram.com/")))
        XCTAssertFalse(instagramCaptureOrigin(URL(string: "https://user@www.instagram.com/")))
        XCTAssertFalse(instagramCaptureOrigin(URL(string: "https://www.instagram.com:8443/")))
        XCTAssertFalse(instagramCaptureOrigin(nil))
    }

    func testThePageStaysOnInstagramAndItsFacebookSignIn() {
        XCTAssertTrue(instagramNavigationAllowed(URL(string: "https://www.instagram.com/accounts/login/")))
        XCTAssertTrue(instagramNavigationAllowed(URL(string: "https://www.facebook.com/login")))
        XCTAssertFalse(instagramNavigationAllowed(URL(string: "https://evil.example/")))
        XCTAssertFalse(instagramNavigationAllowed(URL(string: "instagram://user")))
        XCTAssertFalse(instagramNavigationAllowed(URL(string: "https://www.instagram.com.evil.example/")))
    }

    func testOnlyAnInstagramIngestAndARealKeyAreAccepted() throws {
        XCTAssertEqual(try validateInstagramServer(" rtmps://edgetee-upload-ist1-2.xx.fbcdn.net:443/rtmp/ "),
                       "rtmps://edgetee-upload-ist1-2.xx.fbcdn.net:443/rtmp")
        for bad in ["rtmp://edgetee-upload.xx.fbcdn.net:443/rtmp", "rtmps://example.com:443/rtmp",
                    "rtmps://edgetee-upload.xx.fbcdn.net:443/rtmp/KEY", "rtmps://edgetee-upload.xx.fbcdn.net/rtmp?x=1",
                    "rtmps://user@edgetee-upload.xx.fbcdn.net/rtmp", "https://www.instagram.com/accounts/login"] {
            XCTAssertThrowsError(try validateInstagramServer(bad), bad)
        }
        XCTAssertEqual(try validateInstagramKey(" FB-1234-abcd "), "FB-1234-abcd")
        XCTAssertThrowsError(try validateInstagramKey("********"))
        XCTAssertThrowsError(try validateInstagramKey("----"))
        XCTAssertThrowsError(try validateInstagramKey("a b c d"))
    }

    func testACaptureIsReadOnlyWhenTheProducerIsReady() {
        let ready = #"{"status":"ready","origin":"https://www.instagram.com","serverUrl":"rtmps://edgetee-upload.xx.fbcdn.net:443/rtmp","streamKey":"FB-1-abc","titleMatches":true,"audienceMatches":false}"#
        let credentials = decodeInstagramCapture(ready)
        XCTAssertEqual(credentials?.serverUrl, "rtmps://edgetee-upload.xx.fbcdn.net:443/rtmp")
        XCTAssertEqual(credentials?.streamKey, "FB-1-abc")
        XCTAssertEqual(credentials?.titleMatches, true)
        XCTAssertEqual(credentials?.audienceMatches, false)
        XCTAssertNil(decodeInstagramCapture(#"{"status":"waiting"}"#))
        XCTAssertNil(decodeInstagramCapture(ready.replacingOccurrences(of: "https://www.instagram.com", with: "https://evil.example")))
        XCTAssertNil(decodeInstagramCapture(ready.replacingOccurrences(of: "FB-1-abc", with: "********")))
        XCTAssertNil(decodeInstagramCapture(String(repeating: "x", count: 9_000)))
        XCTAssertNil(decodeInstagramCapture(42))
    }

    func testStartIsClickedOnceAndOnlyOnTheSamePage() throws {
        let url = try XCTUnwrap(URL(string: "https://www.instagram.com/live/producer/"))
        let request = InstagramPublishRequest(profileId: "p", document: 3, url: url, deadline: 100)
        XCTAssertTrue(request.valid(now: 10, active: true, document: 3, url: url))
        XCTAssertFalse(request.valid(now: 10, active: false, document: 3, url: url))
        XCTAssertFalse(request.valid(now: 100, active: true, document: 3, url: url))
        XCTAssertFalse(request.valid(now: 10, active: true, document: 4, url: url))
        XCTAssertFalse(request.valid(now: 10, active: true, document: 3, url: URL(string: "https://www.instagram.com/")))
        XCTAssertTrue(request.claimClick())
        XCTAssertFalse(request.claimClick())
        // After the click Instagram may change its address within its own site.
        XCTAssertTrue(request.valid(now: 10, active: true, document: 3, url: URL(string: "https://www.instagram.com/")))
    }

    func testEndAndItsConfirmationAreEachClickedOnce() {
        let request = InstagramEndRequest(profileId: "p", document: 1, deadline: 8)
        XCTAssertFalse(request.claim("ended"))
        XCTAssertTrue(request.claim("end"))
        XCTAssertFalse(request.claim("end"))
        XCTAssertTrue(request.claim("confirm"))
        XCTAssertFalse(request.claim("confirm"))
        let confirmedFirst = InstagramEndRequest(profileId: "p", document: 1, deadline: 8)
        XCTAssertTrue(confirmedFirst.claim("confirm"))
        XCTAssertFalse(confirmedFirst.claim("end"))
        XCTAssertFalse(request.valid(now: 8, document: 1, url: URL(string: "https://www.instagram.com/")))
    }

    func testInstagramCountsAsLiveOnlyWithItsOwnConfirmationAndVideo() {
        let confirmation = InstagramPublicationConfirmation(profileId: "p", since: 1)
        let sending = OutputSnapshot(id: "p", status: "forwarding", outboundBytes: 10)
        XCTAssertTrue(instagramConfirmedOutput(profileId: "p", output: sending, confirmation: confirmation))
        XCTAssertFalse(instagramConfirmedOutput(profileId: "p", output: sending, confirmation: nil))
        XCTAssertFalse(instagramConfirmedOutput(profileId: "p", output: OutputSnapshot(id: "p", status: "reconnecting", outboundBytes: 10), confirmation: confirmation))
        XCTAssertFalse(instagramConfirmedOutput(profileId: "p", output: OutputSnapshot(id: "p", status: "forwarding"), confirmation: confirmation))
    }

    func testScriptPlaceholdersGetJavaScriptLiterals() {
        XCTAssertEqual(javaScriptLiteral("a\"b'c</script>"), #""a\"b'c<\/script>""#)
        XCTAssertEqual(javaScriptLiteral("Mardin Sürgücü yolu"), "\"Mardin Sürgücü yolu\"")
    }

    func testTheTitleFallsBackToAValidOne() throws {
        XCTAssertEqual(initialBroadcastTitle(saved: "Flight", profiles: []), "Flight")
        XCTAssertEqual(initialBroadcastTitle(saved: "<bad>", profiles: []), "Stream My Drone")
        XCTAssertEqual(try BroadcastStartRequest(profileIds: ["a", "b", "a"], title: " Trip ").profileIds, ["a", "b"])
        XCTAssertThrowsError(try BroadcastStartRequest(profileIds: ["a"], title: "  "))
        XCTAssertEqual(InstagramAudience.fromStorage(nil), .practice)
        XCTAssertEqual(InstagramAudience.fromStorage("PUBLIC"), .public)
    }

    func testAppStartedPlatformsGetNoGoLiveReminder() {
        let instagram = DestinationProfile(id: "i", name: "Instagram", kind: .instagram, serverUrl: "rtmps://x.fbcdn.net:443/rtmp")
        let tiktok = DestinationProfile(id: "t", name: "TikTok", kind: .tiktok, serverUrl: "rtmp://push.tiktokcdn.com/game")
        let snapshot = RelaySnapshot(status: "publishing", outputs: [OutputSnapshot(id: "i", status: "forwarding")])
        XCTAssertEqual(liveTips(phase: .live, snapshot: snapshot, destinations: [instagram, tiktok], skipReminders: ["i"]),
                       [tr("live_reminder_tiktok")])
        XCTAssertEqual(liveTips(phase: .live, snapshot: snapshot, destinations: [instagram]).count, 1)
    }

    /// The scripts are the Android app's; both apps must run the same ones.
    func testTheBundledScriptsMatchTheAndroidApp() throws {
        let android = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
            .appendingPathComponent("android/app/src/main/assets")
        let names = ["capture", "controls", "diagnose", "end", "open", "publish", "setup", "title"]
        let bundle = Bundle(for: InstagramProducerSession.self)
        for name in names {
            let bundled = try XCTUnwrap(Bundle.main.url(forResource: "instagram-producer-\(name)", withExtension: "js")
                ?? bundle.url(forResource: "instagram-producer-\(name)", withExtension: "js"), name)
            XCTAssertEqual(try String(contentsOf: bundled, encoding: .utf8),
                           try String(contentsOf: android.appendingPathComponent("instagram-producer-\(name).js"), encoding: .utf8), name)
        }
    }
}
