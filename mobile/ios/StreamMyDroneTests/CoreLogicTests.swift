import AVFoundation
import XCTest
@testable import StreamMyDrone

final class BridgePhaseTests: XCTestCase {
    private func state(_ status: String, live: Bool = false, outputs: [String] = [], active: Bool = true) -> RelayUiState {
        RelayUiState(
            isActive: active,
            snapshot: RelaySnapshot(status: status, outputs: outputs.enumerated().map { OutputSnapshot(id: "\($0.offset)", status: $0.element) }),
            liveProfileIds: live ? ["0"] : []
        )
    }

    func testTheReceiverStatesMapToPhases() {
        XCTAssertEqual(bridgePhase(state("stopped", active: false)), .idle)
        XCTAssertEqual(bridgePhase(state("error", active: false)), .startFailed)
        XCTAssertEqual(bridgePhase(state("starting")), .starting)
        XCTAssertEqual(bridgePhase(state("listening")), .waitingForDrone)
        XCTAssertEqual(bridgePhase(state("connected")), .droneConnected)
        XCTAssertEqual(bridgePhase(state("publishing")), .preview)
        XCTAssertEqual(bridgePhase(state("error")), .receiverError)
    }

    func testTheBestPlatformDecidesTheBroadcastPhase() {
        XCTAssertEqual(bridgePhase(state("publishing", live: true, outputs: ["connecting"])), .connectingTarget)
        XCTAssertEqual(bridgePhase(state("publishing", live: true, outputs: ["reconnecting"])), .reconnecting)
        XCTAssertEqual(bridgePhase(state("publishing", live: true, outputs: ["reconnecting", "forwarding"])), .live)
        XCTAssertEqual(bridgePhase(state("publishing", live: true, outputs: ["congested"])), .live)
        XCTAssertTrue(BridgePhase.live.hasPicture)
        XCTAssertFalse(BridgePhase.droneConnected.hasPicture)
    }

    func testTheSnapshotJsonIsRead() throws {
        let json = """
        {"status":"publishing","errorCode":null,"errorDetail":null,"remoteAddress":"192.168.1.20:50000","receivedBytes":1000,
         "bitrateKbps":4200.5,"videoCodec":"avc1","audioCodec":"mp4a","videoFrames":30,"audioFrames":47,"rejectedPublishAttempts":0,
         "stalls":1,"longestStallMs":900,"sourceReconnects":0,"recentInterruptions":1,
         "outputs":[{"id":"yt","status":"reconnecting","reason":"connect_failed","reasonDetail":"refused","retryInSeconds":4,
         "outboundBytes":10,"droppedFrames":2,"reconnectAttempts":1,"secure":true}]}
        """
        let snapshot = try RelaySnapshot.fromJson(Data(json.utf8))
        XCTAssertEqual(snapshot.status, "publishing")
        XCTAssertEqual(snapshot.bitrateKbps, 4200.5)
        XCTAssertNil(snapshot.error)
        XCTAssertEqual(snapshot.output("yt")?.retryInSeconds, 4)
        XCTAssertEqual(snapshot.output("yt")?.secure, true)
        XCTAssertEqual(snapshot.outputStatus, "reconnecting")
    }

    func testANativeErrorIsPutIntoWords() throws {
        let snapshot = try RelaySnapshot.fromJson(Data(#"{"status":"error","errorCode":"listen_failed","errorDetail":"0.0.0.0:1935: in use"}"#.utf8))
        XCTAssertEqual(snapshot.error, .tr("error_with_detail", .text(.tr("error_listen_failed")), .string("0.0.0.0:1935: in use")))
        XCTAssertEqual(nativeError("something_new"), .raw("something_new"))
        XCTAssertEqual(nativeError("invalid_stream_key"), .tr("error_stream_key"))
    }
}

final class FormatTests: XCTestCase {
    private let english = Locale(identifier: "en_US")
    private let turkish = Locale(identifier: "tr_TR")

    func testBitratesAndSizesUseTheLanguagesNumbers() {
        XCTAssertEqual(formatBitrate(4_200, locale: english), "4.2 Mbps")
        XCTAssertEqual(formatBitrate(4_200, locale: turkish), "4,2 Mbps")
        XCTAssertEqual(formatBitrate(640, locale: english), "640 kbps")
        XCTAssertEqual(formatBytes(999, locale: english), "999 B")
        XCTAssertEqual(formatBytes(1_500_000, locale: english), "1.5 MB")
        XCTAssertEqual(formatBytes(2_340_000_000, locale: english), "2.34 GB")
        XCTAssertEqual(formatSeconds(millis: 1_234, locale: english), "1.2")
    }

    func testDurationsGrowAnHourField() {
        XCTAssertEqual(formatDuration(millis: 12_000), "00:12")
        XCTAssertEqual(formatDuration(millis: 3_725_000), "1:02:05")
        XCTAssertEqual(formatDuration(millis: -5), "00:00")
    }

    func testCodecsHaveTheirKnownNames() {
        XCTAssertEqual(friendlyCodecName("avc1"), "H.264")
        XCTAssertEqual(friendlyCodecName("mp4a"), "AAC")
        XCTAssertEqual(friendlyCodecName("xyz"), "xyz")
    }
}

final class LocalNetworkTests: XCTestCase {
    func testWiFiIsPreferredOverHotspotAndWired() {
        let address = preferredLanAddress([("en2", "192.168.42.129"), ("bridge100", "172.20.10.1"), ("en0", "192.168.1.101")])
        XCTAssertEqual(address, LanAddress(address: "192.168.1.101", kind: .wifi))
    }

    func testTheHotspotIsUsedWithoutWiFi() {
        let address = preferredLanAddress([("pdp_ip0", "10.12.34.56"), ("bridge100", "172.20.10.1")])
        XCTAssertEqual(address, LanAddress(address: "172.20.10.1", kind: .hotspot))
    }

    func testMobileDataVpnAndAppleLinksAreNeverOffered() {
        XCTAssertNil(preferredLanAddress([("pdp_ip0", "10.0.0.8"), ("utun3", "10.8.0.2"), ("ipsec0", "10.9.0.1"), ("lo0", "127.0.0.1")]))
        XCTAssertNil(lanKind("awdl0"))
        XCTAssertEqual(lanKind("EN0"), .wifi)
        XCTAssertEqual(lanKind("en5"), .wired)
    }

    func testThePublishUrlMatchesTheDesktopIngestPath() {
        XCTAssertEqual(LanAddress(address: "192.168.1.101", kind: .wifi).publishUrl, "rtmp://192.168.1.101:1935/drone")
    }
}

final class PictureFitTests: XCTestCase {
    private let wide: CGFloat = 16 / 9

    func testAutomaticFillsTheScreenOnlyWhenLittleIsCutOff() {
        // A vertical drone picture (DJI's vertical mode) on an upright phone: fill, cutting ~20%.
        XCTAssertEqual(shownFit(width: 412, height: 915, aspect: 720 / 1280, fit: .auto), .fill)
        // A wide picture on an upright phone would lose three quarters: show it whole.
        XCTAssertEqual(shownFit(width: 412, height: 915, aspect: wide, fit: .auto), .whole)
        // The same wide picture with the phone turned sideways: fill.
        XCTAssertEqual(shownFit(width: 915, height: 412, aspect: wide, fit: .auto), .fill)
        // A choice the user made is kept whatever the shape.
        XCTAssertEqual(shownFit(width: 915, height: 412, aspect: wide, fit: .whole), .whole)
        XCTAssertEqual(shownFit(width: 412, height: 915, aspect: wide, fit: .fill), .fill)
    }

    func testTheChoiceIsStoredByNameAndStartsAutomatic() {
        XCTAssertEqual(PictureFit.fromStorage("fill"), .fill)
        XCTAssertEqual(PictureFit.fromStorage("whole"), .whole)
        XCTAssertEqual(PictureFit.fromStorage(nil), .auto)
        XCTAssertEqual(PictureFit.fromStorage("something else"), .auto)
    }
}

final class TestVideoRulesTests: XCTestCase {
    private func video(
        codec: FourCharCode = kCMVideoCodecType_H264, width: Int = 1280, height: Int = 720, rotation: Int = 0,
        frameRate: Float? = 30, bitrate: Int64? = 4_000_000, hdr: Bool = false, audio: AudioFormatID? = kAudioFormatMPEG4AAC
    ) -> VideoFileInfo {
        VideoFileInfo(codec: codec, width: width, height: height, rotationDegrees: rotation, frameRate: frameRate,
                      bitrate: bitrate, hdr: hdr, audioFormat: audio)
    }

    func testAVideoLikeDjiFlysGoesOutAsItIs() {
        XCTAssertFalse(needsConversion(video()))
        XCTAssertFalse(needsConversion(video(width: 720, height: 1280, frameRate: 30.5)))
    }

    func testAnythingDjiFlyCouldNotHaveSentIsConverted() {
        XCTAssertTrue(needsConversion(video(codec: kCMVideoCodecType_HEVC)))
        XCTAssertTrue(needsConversion(video(width: 3840, height: 2160)))
        XCTAssertTrue(needsConversion(video(rotation: 90)))
        XCTAssertTrue(needsConversion(video(frameRate: 60)))
        XCTAssertTrue(needsConversion(video(bitrate: 50_000_000)))
        XCTAssertTrue(needsConversion(video(bitrate: nil)))
        XCTAssertTrue(needsConversion(video(hdr: true)))
        // Instagram shows nothing of a stream without sound.
        XCTAssertTrue(needsConversion(video(audio: nil)))
        XCTAssertTrue(needsConversion(video(audio: kAudioFormatAppleLossless)))
    }

    func testTheLabelHidesNamesThatAreOnlyNumbers() {
        XCTAssertEqual(testVideoLabel(displayName: "flight.mp4", durationMs: 20_000),
                       .joined([.raw("flight.mp4"), .raw("00:20"), .tr("test_video_looping")], separator: " · "))
        XCTAssertEqual(testVideoLabel(displayName: "43.mp4", durationMs: nil),
                       .joined([.tr("test_video_default_name"), .tr("test_video_looping")], separator: " · "))
    }
}

final class PictureSizeTests: XCTestCase {
    private let wide: CGFloat = 16 / 9

    private func assertSize(_ expected: CGSize, _ actual: CGSize, line: UInt = #line) {
        XCTAssertEqual(expected.width, actual.width, accuracy: 0.5, line: line)
        XCTAssertEqual(expected.height, actual.height, accuracy: 0.5, line: line)
    }

    func testAWidePictureOnAPhoneHeldUpright() {
        // The whole picture spans the width; filling the screen cuts off its sides.
        assertSize(CGSize(width: 400, height: 225), pictureSize(width: 400, height: 880, aspect: wide, fit: .whole))
        assertSize(CGSize(width: 1564.4, height: 880), pictureSize(width: 400, height: 880, aspect: wide, fit: .fill))
        assertSize(CGSize(width: 400, height: 225), pictureSize(width: 400, height: 880, aspect: wide, fit: .auto))
    }

    func testAWidePictureOnAPhoneTurnedSideways() {
        assertSize(CGSize(width: 711.1, height: 400), pictureSize(width: 880, height: 400, aspect: wide, fit: .whole))
        assertSize(CGSize(width: 880, height: 495), pictureSize(width: 880, height: 400, aspect: wide, fit: .fill))
    }
}
