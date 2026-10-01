import XCTest
@testable import StreamMyDrone

/// Keys in memory, so the tests never touch the Keychain.
private final class MemoryKeys: StreamKeyStore {
    var keys: [String: String] = [:]
    var failSaving = false

    func save(_ key: String, for profileId: String) throws {
        if failSaving { throw KeychainError(status: errSecNotAvailable) }
        keys[profileId] = key
    }

    func load(for profileId: String) throws -> String {
        guard let key = keys[profileId] else { throw KeychainError(status: errSecItemNotFound) }
        return key
    }

    func delete(for profileId: String) { keys[profileId] = nil }
}

final class DestinationProfileStoreTests: XCTestCase {
    private var directory: URL!
    private var keys: MemoryKeys!
    private var store: DestinationProfileStore!

    override func setUpWithError() throws {
        directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        keys = MemoryKeys()
        store = DestinationProfileStore(directory: directory, keys: keys)
    }

    override func tearDownWithError() throws {
        try? FileManager.default.removeItem(at: directory)
    }

    private func key(_ error: Error) -> UiText? { (error as? DestinationProfileError)?.text }

    func testANewPlatformIsSavedSelectedAndItsKeyStaysOutOfTheFile() throws {
        let saved = try store.save(existingId: nil, name: "YouTube", kind: .youtube,
                                   serverUrl: "rtmps://a.rtmps.youtube.com/live2/", streamKey: "abcd-efgh-1234")
        let profile = try XCTUnwrap(saved.profiles.first)
        XCTAssertEqual(profile.serverUrl, "rtmps://a.rtmps.youtube.com/live2")
        XCTAssertEqual(saved.selectedProfileIds, [profile.id])
        XCTAssertEqual(try store.credentials(profile.id).streamKey, "abcd-efgh-1234")
        let file = try String(contentsOf: directory.appendingPathComponent("destination-profiles-v1.json"), encoding: .utf8)
        XCTAssertFalse(file.contains("abcd-efgh-1234"))
        XCTAssertEqual(try DestinationProfileStore(directory: directory, keys: keys).load(), saved)
    }

    func testEditingWithoutANewKeyKeepsTheOldOne() throws {
        let id = try store.save(existingId: nil, name: "Twitch", kind: .twitch, serverUrl: "rtmp://live.twitch.tv/app", streamKey: "live_123456")
            .profiles[0].id
        _ = try store.save(existingId: id, name: "Twitch", kind: .twitch, serverUrl: "rtmp://live-fra.twitch.tv/app", streamKey: "")
        XCTAssertEqual(try store.credentials(id).streamKey, "live_123456")
        XCTAssertEqual(try store.credentials(id).serverUrl, "rtmp://live-fra.twitch.tv/app")
    }

    func testANewPlatformNeedsAKey() {
        XCTAssertThrowsError(try store.save(existingId: nil, name: "Kick", kind: .kick, serverUrl: "rtmp://a/b", streamKey: "")) {
            XCTAssertEqual(key($0), .tr("profile_error_key_required"))
        }
    }

    func testAKeychainFailureIsReportedAsSuch() {
        keys.failSaving = true
        XCTAssertThrowsError(try store.save(existingId: nil, name: "Kick", kind: .kick, serverUrl: "rtmp://a/b", streamKey: "abcd1234")) {
            XCTAssertEqual(key($0), .tr("profile_error_key_encrypt"))
        }
    }

    func testSelectingAndDeletingKeepTheListConsistent() throws {
        let first = try store.save(existingId: nil, name: "YouTube", kind: .youtube, serverUrl: "rtmp://a/b", streamKey: "key-1111").profiles[0].id
        let second = try store.save(existingId: nil, name: "Kick", kind: .kick, serverUrl: "rtmp://c/d", streamKey: "key-2222").profiles[1].id
        XCTAssertEqual(try store.setSelected(first, selected: false).selectedProfileIds, [second])
        XCTAssertEqual(try store.setSelected(first, selected: true).selectedProfileIds, [second, first])
        let afterDelete = try store.delete(second)
        XCTAssertEqual(afterDelete.selectedProfileIds, [first])
        XCTAssertNil(keys.keys[second])
        XCTAssertThrowsError(try store.credentials(second))
    }

    func testAFileFromANewerVersionIsNotOverwritten() throws {
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        try Data(#"{"version":2,"selectedProfileIds":[],"profiles":[]}"#.utf8)
            .write(to: directory.appendingPathComponent("destination-profiles-v1.json"))
        XCTAssertThrowsError(try store.load()) { XCTAssertEqual(key($0), .tr("profile_error_version")) }
    }
}

final class ValidationTests: XCTestCase {
    func testServerAddressesNeedTheSchemeServerAndApp() throws {
        XCTAssertEqual(try validateServerUrl(" rtmps://edgetee-upload-ist1-2.xx.fbcdn.net:443/rtmp/ "), "rtmps://edgetee-upload-ist1-2.xx.fbcdn.net:443/rtmp")
        XCTAssertThrowsError(try validateServerUrl("https://example.com/live"))
        XCTAssertThrowsError(try validateServerUrl("rtmp://example.com"))
        XCTAssertThrowsError(try validateServerUrl("rtmp://user@example.com/live"))
        XCTAssertThrowsError(try validateServerUrl("rtmp://example.com/live?x=1"))
    }

    func testStreamKeysAreAsciiWithoutSlashesOrSpaces() {
        XCTAssertNoThrow(try validateStreamKey("live_1234-abcd"))
        XCTAssertThrowsError(try validateStreamKey("abc"))
        XCTAssertThrowsError(try validateStreamKey("with space"))
        XCTAssertThrowsError(try validateStreamKey("a/b/c/d"))
        XCTAssertThrowsError(try validateStreamKey("schlüssel"))
    }

    func testTheDefaultServerIsTheSameWithOrWithoutATrailingSlash() {
        XCTAssertTrue(sameServerUrl("rtmp://live.twitch.tv/app/", "rtmp://live.twitch.tv/app"))
        XCTAssertFalse(sameServerUrl("rtmp://live-fra.twitch.tv/app", "rtmp://live.twitch.tv/app"))
    }
}

/// Every language has every string with English's placeholders, like the Android TranslationsTest.
final class TranslationsTests: XCTestCase {
    private func strings(_ language: String) throws -> [String: String] {
        let bundle = try XCTUnwrap(Bundle.main.path(forResource: language, ofType: "lproj").flatMap(Bundle.init(path:)), language)
        let url = try XCTUnwrap(bundle.url(forResource: "Localizable", withExtension: "strings"), language)
        return try XCTUnwrap(NSDictionary(contentsOf: url) as? [String: String], language)
    }

    private func placeholders(_ text: String) -> [String] {
        let regex = try! NSRegularExpression(pattern: "%(\\d+\\$)?(@|ld)")
        return regex.matches(in: text, range: NSRange(text.startIndex..., in: text))
            .map { (text as NSString).substring(with: $0.range) }
            .sorted()
    }

    func testEveryLanguageHasEveryStringWithTheSamePlaceholders() throws {
        let english = try strings("en")
        XCTAssertGreaterThan(english.count, 200)
        for language in appLanguages.map(\.tag) where language != "en" {
            let translated = try strings(language)
            for (key, value) in english {
                let text = try XCTUnwrap(translated[key], "\(language) misses \(key)")
                XCTAssertEqual(placeholders(text), placeholders(value), "\(language): \(key)")
            }
        }
    }

    func testPluralsPickTheLanguagesForms() {
        let localizer = Localizer(defaults: UserDefaults(suiteName: UUID().uuidString)!)
        localizer.setLanguage(appLanguages.first { $0.tag == "en" }, defaults: UserDefaults(suiteName: UUID().uuidString)!)
        XCTAssertEqual(localizer.plural("platform_count", count: 1), "1 platform")
        XCTAssertEqual(localizer.plural("platform_count", count: 3), "3 platforms")
        XCTAssertEqual(localizer.string("platforms_active", [2, 4]), "2/4 active")
        XCTAssertTrue(localizer.plural("upload_note_total", count: 2, ["8.4 Mbps"]).contains("8.4 Mbps"))
        localizer.setLanguage(appLanguages.first { $0.tag == "tr" }, defaults: UserDefaults(suiteName: UUID().uuidString)!)
        XCTAssertEqual(localizer.string("go_live"), "Canlı yayını başlat")
        XCTAssertFalse(localizer.plural("output_retry", count: 5).isEmpty)
    }

    func testChineseGoesByScriptAndIndonesianByBothCodes() {
        XCTAssertEqual(appLanguage(forIdentifier: "zh-Hant-TW")?.tag, "zh-Hant")
        XCTAssertEqual(appLanguage(forIdentifier: "zh_HK")?.tag, "zh-Hant")
        XCTAssertEqual(appLanguage(forIdentifier: "zh-Hans-CN")?.tag, "zh-Hans")
        XCTAssertEqual(appLanguage(forIdentifier: "in")?.tag, "id")
        XCTAssertEqual(appLanguage(forIdentifier: "id-ID")?.tag, "id")
        XCTAssertNil(appLanguage(forIdentifier: "sv"))
    }
}
