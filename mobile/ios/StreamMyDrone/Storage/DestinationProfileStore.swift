import Foundation
import Security

/**
 * Supported platforms in display order. `brandName` is nil for a custom server, which is named
 * in the app's language instead. `defaultServerUrl` is each platform's published ingest address;
 * TikTok and custom servers hand out their own address, so they have none.
 */
enum DestinationKind: String, CaseIterable, Identifiable {
    case instagram, tiktok, youtube, facebook, twitch, kick, custom

    var id: String { rawValue }

    var brandName: String? {
        switch self {
        case .instagram: "Instagram"
        case .tiktok: "TikTok"
        case .youtube: "YouTube"
        case .facebook: "Facebook"
        case .twitch: "Twitch"
        case .kick: "Kick"
        case .custom: nil
        }
    }

    var defaultServerUrl: String? {
        switch self {
        case .instagram: "rtmps://live-upload.instagram.com:443/rtmp"
        case .youtube: "rtmps://a.rtmps.youtube.com/live2"
        case .facebook: "rtmps://live-api-s.facebook.com:443/rtmp"
        case .twitch: "rtmp://live.twitch.tv/app"
        case .kick: "rtmps://fa723fc1b171.global-contribute.live-video.net:443/app"
        case .tiktok, .custom: nil
        }
    }

    static func fromStorage(_ value: String) -> DestinationKind {
        DestinationKind(rawValue: value) ?? .custom
    }
}

struct DestinationProfile: Identifiable, Equatable {
    let id: String
    let name: String
    let kind: DestinationKind
    let serverUrl: String
}

struct DestinationProfiles: Equatable {
    var profiles: [DestinationProfile] = []
    /// Where the next broadcast goes, in the order the user picked them.
    var selectedProfileIds: [String] = []

    var selectedProfiles: [DestinationProfile] {
        selectedProfileIds.compactMap { id in profiles.first { $0.id == id } }
    }

    func isSelected(_ profileId: String) -> Bool { selectedProfileIds.contains(profileId) }
}

struct DestinationCredentials {
    let serverUrl: String
    let streamKey: String
}

/// A problem with the saved platforms, in words for the screen.
struct DestinationProfileError: Error {
    let text: UiText

    init(_ key: String) { text = .tr(key) }
}

/// Where stream keys are kept: the Keychain in the app, memory in tests.
protocol StreamKeyStore {
    func save(_ key: String, for profileId: String) throws
    func load(for profileId: String) throws -> String
    func delete(for profileId: String)
}

/**
 * The saved platforms. Their names, kinds and server addresses are written atomically to a JSON
 * file in Application Support that is excluded from backups; each stream key is a Keychain item
 * that never leaves this device (no iCloud Keychain, no backup) and is readable only while the
 * phone is unlocked. Keys are never written to the file, logs or notifications. Every access
 * holds one lock, so the store can be used from any thread.
 */
final class DestinationProfileStore: @unchecked Sendable {
    private static let lock = NSLock()
    private static let storageVersion = 1

    private let file: URL
    private let keys: StreamKeyStore

    init(directory: URL? = nil, keys: StreamKeyStore = KeychainStreamKeyStore()) {
        let base = directory ?? FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
        file = base.appendingPathComponent("destination-profiles-v1.json")
        self.keys = keys
    }

    func load() throws -> DestinationProfiles {
        try Self.lock.withLock { try readRecords() }
    }

    func save(
        existingId: String?,
        name: String,
        kind: DestinationKind,
        serverUrl: String,
        streamKey: String
    ) throws -> DestinationProfiles {
        try Self.lock.withLock {
            let normalizedName = try validateName(name)
            let normalizedUrl = try validateServerUrl(serverUrl)
            var current = try readRecords()
            let index = existingId.flatMap { id in current.profiles.firstIndex { $0.id == id } }
            if existingId != nil && index == nil {
                throw DestinationProfileError("profile_error_not_found")
            }
            let id = existingId ?? UUID().uuidString.lowercased()
            if !streamKey.isEmpty {
                try validateStreamKey(streamKey)
                do {
                    try keys.save(streamKey, for: id)
                } catch {
                    throw DestinationProfileError("profile_error_key_encrypt")
                }
            } else if index == nil {
                throw DestinationProfileError("profile_error_key_required")
            }
            let profile = DestinationProfile(id: id, name: normalizedName, kind: kind, serverUrl: normalizedUrl)
            if let index {
                current.profiles[index] = profile
            } else {
                current.profiles.append(profile)
                // A platform the user just added is one they mean to stream to.
                current.selectedProfileIds.append(id)
            }
            try writeRecords(current)
            return current
        }
    }

    /// Adds `profileId` to the platforms the next broadcast goes to, or takes it out.
    func setSelected(_ profileId: String, selected: Bool) throws -> DestinationProfiles {
        try Self.lock.withLock {
            var current = try readRecords()
            guard current.profiles.contains(where: { $0.id == profileId }) else {
                throw DestinationProfileError("profile_error_not_found")
            }
            current.selectedProfileIds.removeAll { $0 == profileId }
            if selected { current.selectedProfileIds.append(profileId) }
            try writeRecords(current)
            return current
        }
    }

    func delete(_ profileId: String) throws -> DestinationProfiles {
        try Self.lock.withLock {
            var current = try readRecords()
            guard current.profiles.contains(where: { $0.id == profileId }) else {
                throw DestinationProfileError("profile_error_not_found")
            }
            current.profiles.removeAll { $0.id == profileId }
            current.selectedProfileIds.removeAll { $0 == profileId }
            try writeRecords(current)
            keys.delete(for: profileId)
            return current
        }
    }

    func credentials(_ profileId: String) throws -> DestinationCredentials {
        try Self.lock.withLock {
            guard let profile = try readRecords().profiles.first(where: { $0.id == profileId }) else {
                throw DestinationProfileError("profile_error_not_found")
            }
            let streamKey: String
            do {
                streamKey = try keys.load(for: profile.id)
            } catch {
                throw DestinationProfileError("profile_error_key_decrypt")
            }
            return DestinationCredentials(serverUrl: profile.serverUrl, streamKey: streamKey)
        }
    }

    private func readRecords() throws -> DestinationProfiles {
        guard FileManager.default.fileExists(atPath: file.path) else { return DestinationProfiles() }
        let data: Data
        do {
            data = try Data(contentsOf: file)
        } catch {
            throw DestinationProfileError("profile_error_read")
        }
        let stored: StoredProfiles
        do {
            stored = try JSONDecoder().decode(StoredProfiles.self, from: data)
        } catch {
            throw DestinationProfileError("profile_error_corrupt")
        }
        guard stored.version == Self.storageVersion else {
            throw DestinationProfileError("profile_error_version")
        }
        let profiles = stored.profiles.map {
            DestinationProfile(id: $0.id, name: $0.name, kind: .fromStorage($0.kind), serverUrl: $0.serverUrl)
        }
        var seen = Set<String>()
        let selected = stored.selectedProfileIds.filter { id in
            profiles.contains { $0.id == id } && seen.insert(id).inserted
        }
        return DestinationProfiles(profiles: profiles, selectedProfileIds: selected)
    }

    private func writeRecords(_ value: DestinationProfiles) throws {
        let stored = StoredProfiles(
            version: Self.storageVersion,
            selectedProfileIds: value.selectedProfileIds,
            profiles: value.profiles.map {
                StoredProfile(id: $0.id, name: $0.name, kind: $0.kind.rawValue, serverUrl: $0.serverUrl)
            }
        )
        do {
            try FileManager.default.createDirectory(at: file.deletingLastPathComponent(), withIntermediateDirectories: true)
            let encoder = JSONEncoder()
            encoder.outputFormatting = [.sortedKeys]
            try encoder.encode(stored).write(to: file, options: [.atomic, .completeFileProtectionUntilFirstUserAuthentication])
            var url = file
            var values = URLResourceValues()
            values.isExcludedFromBackup = true
            try url.setResourceValues(values)
        } catch {
            throw DestinationProfileError("profile_error_write")
        }
    }
}

private struct StoredProfiles: Codable {
    var version: Int
    var selectedProfileIds: [String]
    var profiles: [StoredProfile]
}

private struct StoredProfile: Codable {
    var id: String
    var name: String
    var kind: String
    var serverUrl: String
}

/// Stream keys as generic-password items of this app, one per profile.
struct KeychainStreamKeyStore: StreamKeyStore {
    private let service = "com.streammydrone.app.stream-key"

    private func query(_ profileId: String) -> [String: Any] {
        [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: profileId,
            kSecAttrSynchronizable as String: false,
        ]
    }

    func save(_ key: String, for profileId: String) throws {
        let data = Data(key.utf8)
        let attributes: [String: Any] = [
            kSecValueData as String: data,
            kSecAttrAccessible as String: kSecAttrAccessibleWhenUnlockedThisDeviceOnly,
        ]
        var status = SecItemUpdate(query(profileId) as CFDictionary, attributes as CFDictionary)
        if status == errSecItemNotFound {
            status = SecItemAdd(query(profileId).merging(attributes) { $1 } as CFDictionary, nil)
        }
        guard status == errSecSuccess else { throw KeychainError(status: status) }
    }

    func load(for profileId: String) throws -> String {
        var request = query(profileId)
        request[kSecReturnData as String] = true
        request[kSecMatchLimit as String] = kSecMatchLimitOne
        var result: CFTypeRef?
        let status = SecItemCopyMatching(request as CFDictionary, &result)
        guard status == errSecSuccess, let data = result as? Data, let key = String(data: data, encoding: .utf8) else {
            throw KeychainError(status: status)
        }
        return key
    }

    func delete(for profileId: String) {
        SecItemDelete(query(profileId) as CFDictionary)
    }
}

struct KeychainError: Error {
    let status: OSStatus
}

func validateName(_ value: String) throws -> String {
    let normalized = value.trimmingCharacters(in: .whitespacesAndNewlines)
    if normalized.isEmpty || normalized.count > 64 || normalized.unicodeScalars.contains(where: { CharacterSet.controlCharacters.contains($0) }) {
        throw DestinationProfileError("profile_error_name")
    }
    return normalized
}

func validateServerUrl(_ value: String) throws -> String {
    var normalized = value.trimmingCharacters(in: .whitespacesAndNewlines)
    while normalized.hasSuffix("/") { normalized.removeLast() }
    let authorityAndApp: Substring
    if normalized.hasPrefix("rtmps://") {
        authorityAndApp = normalized.dropFirst("rtmps://".count)
    } else if normalized.hasPrefix("rtmp://") {
        authorityAndApp = normalized.dropFirst("rtmp://".count)
    } else {
        throw DestinationProfileError("error_destination_scheme")
    }
    let separator = authorityAndApp.firstIndex(of: "/")
    let authority = separator.map { authorityAndApp[..<$0] } ?? ""
    let app = separator.map { authorityAndApp[authorityAndApp.index(after: $0)...] } ?? ""
    if authority.isEmpty || app.isEmpty || authority.contains("@") || normalized.contains("?") ||
        normalized.contains("#") || normalized.contains(where: \.isWhitespace) {
        throw DestinationProfileError("error_destination_server")
    }
    return normalized
}

func validateStreamKey(_ value: String) throws {
    let invalid = !(4...512).contains(value.count) || value.unicodeScalars.contains { scalar in
        scalar.value > 127 || CharacterSet.whitespacesAndNewlines.contains(scalar) ||
            CharacterSet.controlCharacters.contains(scalar) || scalar == "/" || scalar == "#"
    }
    if invalid { throw DestinationProfileError("error_stream_key") }
}

func sameServerUrl(_ first: String, _ second: String) -> Bool {
    func normalized(_ value: String) -> String {
        var trimmed = value.trimmingCharacters(in: .whitespacesAndNewlines)
        while trimmed.hasSuffix("/") { trimmed.removeLast() }
        return trimmed
    }
    return normalized(first) == normalized(second)
}
