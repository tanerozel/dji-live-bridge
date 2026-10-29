import Foundation

/*
 * The Instagram Live Producer rules shared with the Android app (InstagramSetup.kt,
 * InstagramProducerSession.kt, InstagramPublishRequest.kt, InstagramEndRequest.kt): which pages
 * the retained Producer may show, which captured ingest is accepted, and the one-shot claims that
 * keep a Start or End from being clicked twice.
 */

/// A stream address and key read from Instagram's own Producer page; never logged.
struct InstagramCredentials {
    let serverUrl: String
    let streamKey: String
    var titleMatches = false
    var audienceMatches = false
}

/// The audience the app chooses in Instagram's Producer. First use defaults to Practice.
enum InstagramAudience: String, CaseIterable, Identifiable {
    case practice = "PRACTICE"
    case `public` = "PUBLIC"

    var id: String { rawValue }
    var labelKey: String { self == .practice ? "instagram_audience_practice" : "instagram_audience_public" }
    var hintKey: String { self == .practice ? "instagram_audience_practice_hint" : "instagram_audience_public_hint" }

    static func fromStorage(_ value: String?) -> InstagramAudience { value.flatMap(InstagramAudience.init(rawValue:)) ?? .practice }
}

/// One confirmed title, retained through setup and reused when a platform is added while live.
struct BroadcastStartRequest: Equatable {
    let profileIds: [String]
    let title: String
    var instagramAudience: InstagramAudience = .practice

    init(profileIds: [String], title: String, instagramAudience: InstagramAudience = .practice) throws {
        var seen = Set<String>()
        self.profileIds = profileIds.filter { seen.insert($0).inserted }
        self.title = try validateBroadcastTitle(title)
        self.instagramAudience = instagramAudience
    }
}

func validateBroadcastTitle(_ value: String) throws -> String { try validateYouTubeTitle(value) }

/// The last title, a connected YouTube's title, or the app's name: the first one that is valid.
func initialBroadcastTitle(saved: String?, profiles: [DestinationProfile]) -> String {
    let candidates = [saved].compactMap { $0 } + profiles.compactMap { $0.youtube?.title ?? $0.facebook?.title } + ["Stream My Drone"]
    return candidates.lazy.compactMap { try? validateBroadcastTitle($0) }.first ?? "Stream My Drone"
}

/// The Producer page itself: Instagram's site, never its login, checkpoint or 2FA pages.
func instagramCaptureOrigin(_ url: URL?) -> Bool {
    guard let url, let components = URLComponents(url: url, resolvingAgainstBaseURL: false) else { return false }
    guard components.scheme == "https", components.host == "www.instagram.com", components.user == nil,
          components.password == nil, components.port == nil || components.port == 443 else { return false }
    return components.path.range(of: "^/(accounts|auth_platform|challenge|checkpoint)(/|$)", options: .regularExpression) == nil
}

/// The retained page may only go to Instagram and the Facebook sign-in it offers.
func instagramNavigationAllowed(_ url: URL?) -> Bool {
    guard let url, let components = URLComponents(url: url, resolvingAgainstBaseURL: false) else { return false }
    return components.scheme == "https" && components.user == nil && components.password == nil &&
        (components.port == nil || components.port == 443) &&
        ["www.instagram.com", "instagram.com", "www.facebook.com", "m.facebook.com"].contains(components.host ?? "")
}

/// Instagram's RTMPS ingest, without the stream key, user info, query or fragment.
func validateInstagramServer(_ value: String) throws -> String {
    var server = value.trimmingCharacters(in: .whitespacesAndNewlines)
    while server.hasSuffix("/") { server.removeLast() }
    guard let components = URLComponents(string: server), let host = components.host?.lowercased(),
          components.scheme == "rtmps",
          host == "instagram.com" || host.hasSuffix(".instagram.com") || host.hasSuffix(".fbcdn.net"),
          components.user == nil, components.password == nil, components.query == nil, components.fragment == nil,
          components.path == "/rtmp", components.port.map({ (1...65_535).contains($0) }) ?? true else {
        throw DestinationProfileError("instagram_error_server")
    }
    return server
}

/// A real key, not Instagram's masked placeholder or an address.
func validateInstagramKey(_ value: String) throws -> String {
    let key = value.trimmingCharacters(in: .whitespacesAndNewlines)
    do { try validateStreamKey(key) } catch { throw DestinationProfileError("instagram_error_key") }
    guard !key.contains("*"), key.contains(where: { $0.isLetter || $0.isNumber }) else { throw DestinationProfileError("instagram_error_key") }
    return key
}

/// What the capture script returns once the Producer shows both values; anything else is nil.
func decodeInstagramCapture(_ raw: Any?) -> InstagramCredentials? {
    guard let text = raw as? String, text.utf8.count <= 8_192, let data = text.data(using: .utf8),
          let json = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any],
          json["status"] as? String == "ready", json["origin"] as? String == "https://www.instagram.com",
          let server = json["serverUrl"] as? String, let key = json["streamKey"] as? String,
          let serverUrl = try? validateInstagramServer(server), let streamKey = try? validateInstagramKey(key) else { return nil }
    return InstagramCredentials(serverUrl: serverUrl, streamKey: streamKey,
                                titleMatches: json["titleMatches"] as? Bool ?? false,
                                audienceMatches: json["audienceMatches"] as? Bool ?? false)
}

/// One explicit app Start action; never persisted or retried after a document change.
final class InstagramPublishRequest {
    let profileId: String
    let document: Int
    let url: URL
    /// System uptime, in seconds.
    let deadline: TimeInterval
    private(set) var clickDispatched = false

    init(profileId: String, document: Int, url: URL, deadline: TimeInterval) {
        self.profileId = profileId
        self.document = document
        self.url = url
        self.deadline = deadline
    }

    func valid(now: TimeInterval, active: Bool, document: Int, url: URL?) -> Bool {
        active && now < deadline && self.document == document &&
            (self.url == url || (clickDispatched && instagramCaptureOrigin(url)))
    }

    /// Claimed before the script runs: a lost callback must never cause a second click.
    func claimClick() -> Bool {
        if clickDispatched { return false }
        clickDispatched = true
        return true
    }
}

/// Two independent one-shot claims: opening Instagram's end dialog and confirming it.
final class InstagramEndRequest {
    let profileId: String
    let document: Int
    let deadline: TimeInterval
    private var dispatched = Set<String>()

    init(profileId: String, document: Int, deadline: TimeInterval) {
        self.profileId = profileId
        self.document = document
        self.deadline = deadline
    }

    func valid(now: TimeInterval, document: Int, url: URL?) -> Bool {
        now < deadline && self.document == document && instagramCaptureOrigin(url)
    }

    func claim(_ step: String) -> Bool {
        guard ["end", "confirm"].contains(step), !(step == "end" && dispatched.contains("confirm")) else { return false }
        return dispatched.insert(step).inserted
    }
}

/// Positive evidence from the retained Producer, separate from RTMP delivery. Never persisted.
struct InstagramPublicationConfirmation: Equatable {
    let profileId: String
    /// System uptime, in seconds.
    let since: TimeInterval
}

func instagramConfirmedOutput(profileId: String, output: OutputSnapshot?, confirmation: InstagramPublicationConfirmation?) -> Bool {
    guard confirmation?.profileId == profileId, let output, output.id == profileId else { return false }
    return liveOutputStatuses.contains(output.status) && output.outboundBytes > 0
}

/// A Swift string as a JavaScript string literal, for the scripts' placeholders.
func javaScriptLiteral(_ value: String) -> String {
    let data = (try? JSONSerialization.data(withJSONObject: value, options: .fragmentsAllowed)) ?? Data("\"\"".utf8)
    return String(decoding: data, as: UTF8.self)
}
