import Foundation

let facebookGraphVersion = "v26.0"

/// Who can watch a broadcast on the user's own profile. Page broadcasts are always public.
enum FacebookPrivacy: String, CaseIterable {
    case onlyMe = "SELF", friends = "ALL_FRIENDS", everyone = "EVERYONE"

    var labelKey: String {
        switch self {
        case .onlyMe: "facebook_only_me"
        case .friends: "facebook_friends"
        case .everyone: "facebook_public"
        }
    }
}

/**
 * Public settings only: where broadcasts go and how they are titled. Facebook gives every live
 * video its own stream URL, so no key is stored; the access token lives in the Keychain.
 */
struct FacebookConnection: Codable, Equatable {
    let userId: String
    let userName: String
    /// The user's own ID for the profile, otherwise a Page ID.
    let targetId: String
    let targetName: String
    let page: Bool
    var title: String
    var privacy: String

    func validate() throws {
        guard !userId.isEmpty, !targetId.isEmpty, page || targetId == userId,
              FacebookPrivacy(rawValue: privacy) != nil else { throw FacebookError() }
        _ = try validateBroadcastTitle(title)
    }

    var privacyKey: String { page ? "facebook_public" : (FacebookPrivacy(rawValue: privacy) ?? .everyone).labelKey }
}

struct FacebookUser: Equatable {
    let id: String
    let name: String
}

struct FacebookPage: Equatable, Identifiable {
    let id: String
    let name: String
}

/// Deliberately not CustomStringConvertible: the key stays out of diagnostics.
struct FacebookLiveVideo {
    let id: String
    let serverUrl: String
    let key: String
}

/// Read-only platform confirmation; the relay's forwarding state cannot supply this.
struct FacebookLiveStatus: Equatable {
    let id: String
    let status: String
    let permalink: URL?
}

/// Never exposes a response body, token, stream URL or underlying network error in diagnostics.
struct FacebookError: Error, Equatable {
    let key: String
    var text: UiText { .tr(key == "facebook_not_found" ? "facebook_error_api" : key) }
    init(_ key: String = "facebook_error_api") { self.key = key }

    static let notFound = FacebookError("facebook_not_found")
}

func facebookErrorText(_ error: Error) -> UiText {
    (error as? FacebookError)?.text ?? (error as? DestinationProfileError)?.text ??
        ((error as? YouTubeError)?.key == "youtube_error_title" ? .tr("youtube_error_title") : .tr("facebook_error_api"))
}

/// Graph API error codes: https://developers.facebook.com/docs/graph-api/guides/error-handling
func facebookFailure(code: Int, subcode: Int) -> FacebookError {
    switch (code, subcode) {
    // 1363120: account younger than 60 days; 1363144: fewer than 100 followers.
    case (_, 1363120), (_, 1363144): FacebookError("facebook_error_live")
    case (190, _), (102, _), (2500, _), (10, _), (200...299, _): FacebookError("facebook_error_auth")
    case (4, _), (17, _), (32, _), (613, _), (80000...80014, _): FacebookError("facebook_error_rate")
    // "Unsupported get/post request": the object is gone or not visible to this token.
    case (100, 33), (803, _): .notFound
    default: FacebookError()
    }
}

/// Splits Facebook's `rtmps://host:443/rtmp/<key>` into the relay's server and key.
func splitFacebookStreamUrl(_ url: String) throws -> (server: String, key: String) {
    guard url.hasPrefix("rtmps://"),
          let marker = url.range(of: "/rtmp/", range: url.index(url.startIndex, offsetBy: "rtmps://".count)..<url.endIndex) else {
        throw FacebookError()
    }
    do {
        let server = try validateServerUrl(String(url[..<marker.lowerBound]) + "/rtmp")
        let key = String(url[marker.upperBound...])
        try validateStreamKey(key)
        return (server, key)
    } catch { throw FacebookError() }
}

/// Facebook's permalink may be relative; only facebook.com addresses are ever opened.
func facebookPermalink(_ value: String?) -> URL? {
    guard let raw = value?.trimmingCharacters(in: .whitespaces), !raw.isEmpty else { return nil }
    guard let url = URL(string: raw.hasPrefix("/") ? "https://www.facebook.com\(raw)" : raw),
          url.scheme == "https", let host = url.host?.lowercased(),
          host == "facebook.com" || host.hasSuffix(".facebook.com") else { return nil }
    return url
}

protocol FacebookTransport {
    func request(_ method: String, _ path: String, params: [String: String], token: String) async throws -> [String: Any]
}

/// Ephemeral HTTPS session; never forwards the token through redirects or disk caches.
final class GraphFacebookTransport: NSObject, FacebookTransport, URLSessionTaskDelegate {
    private func makeSession() -> URLSession {
        let config = URLSessionConfiguration.ephemeral
        config.urlCache = nil
        config.httpCookieStorage = nil
        config.requestCachePolicy = .reloadIgnoringLocalCacheData
        config.timeoutIntervalForRequest = 15
        config.timeoutIntervalForResource = 30
        return URLSession(configuration: config, delegate: self, delegateQueue: nil)
    }

    func urlSession(_ session: URLSession, task: URLSessionTask, willPerformHTTPRedirection response: HTTPURLResponse,
                    newRequest request: URLRequest, completionHandler: @escaping (URLRequest?) -> Void) {
        completionHandler(nil)
    }

    func request(_ method: String, _ path: String, params: [String: String], token: String) async throws -> [String: Any] {
        // Paths are built from IDs Facebook returned; reject anything that could leave the Graph API.
        guard !path.isEmpty, !path.contains(".."), !path.contains("?"), !path.contains("#"), !path.hasPrefix("/") else { throw FacebookError() }
        var components = URLComponents(string: "https://graph.facebook.com/\(facebookGraphVersion)/")!
        components.path += path
        let items = params.sorted { $0.key < $1.key }.map { URLQueryItem(name: $0.key, value: $0.value) }
        if method == "GET" { components.queryItems = items }
        guard let url = components.url, url.host == "graph.facebook.com" else { throw FacebookError() }
        var request = URLRequest(url: url)
        request.httpMethod = method
        request.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
        request.setValue("application/json", forHTTPHeaderField: "Accept")
        if method != "GET" && !items.isEmpty {
            var body = URLComponents()
            body.queryItems = items
            // URLComponents leaves "+" alone, which a form body would read as a space.
            request.httpBody = Data((body.percentEncodedQuery ?? "").replacingOccurrences(of: "+", with: "%2B").utf8)
            request.setValue("application/x-www-form-urlencoded; charset=UTF-8", forHTTPHeaderField: "Content-Type")
        }
        let session = makeSession()
        defer { session.invalidateAndCancel() }
        do {
            let (bytes, response) = try await session.bytes(for: request)
            guard let http = response as? HTTPURLResponse else { throw FacebookError() }
            var data = Data()
            for try await byte in bytes {
                guard data.count < 1_048_576 else { throw FacebookError() }
                data.append(byte)
            }
            let parsed: [String: Any]? = data.isEmpty ? [:] : (try? JSONSerialization.jsonObject(with: data)) as? [String: Any]
            guard let json = parsed else { throw FacebookError() }
            if let error = json["error"] as? [String: Any] {
                throw facebookFailure(code: error["code"] as? Int ?? 0, subcode: error["error_subcode"] as? Int ?? 0)
            }
            guard (200...299).contains(http.statusCode) else {
                throw http.statusCode == 401 ? FacebookError("facebook_error_auth") : FacebookError()
            }
            return json
        } catch let error as FacebookError { throw error }
        catch { throw FacebookError("facebook_error_network") }
    }
}

struct FacebookAPI {
    let transport: FacebookTransport

    func me(token: String) async throws -> FacebookUser {
        let result = try await transport.request("GET", "me", params: ["fields": "id,name"], token: token)
        guard let id = result["id"] as? String, !id.isEmpty else { throw FacebookError("facebook_error_auth") }
        let name = (result["name"] as? String).flatMap { $0.isEmpty ? nil : $0 } ?? id
        return FacebookUser(id: id, name: name)
    }

    /// Pages the user may publish to. A user without Pages simply gets an empty list.
    func pages(token: String) async throws -> [FacebookPage] {
        let result = try await transport.request("GET", "me/accounts", params: ["fields": "id,name,tasks", "limit": "100"], token: token)
        return (result["data"] as? [[String: Any]] ?? []).compactMap { item in
            guard let id = item["id"] as? String, !id.isEmpty else { return nil }
            if let tasks = item["tasks"] as? [String], !tasks.contains("CREATE_CONTENT") { return nil }
            let name = (item["name"] as? String).flatMap { $0.isEmpty ? nil : $0 } ?? id
            return FacebookPage(id: id, name: name)
        }
    }

    /// The same Facebook user must still be signed in; never create a video for someone else.
    func checkAccount(_ userId: String, token: String) async throws {
        guard try await me(token: token).id == userId else { throw FacebookError("facebook_error_account") }
    }

    /// The token that may publish to `targetId`: the user's own, or the Page's.
    func targetToken(_ targetId: String, page: Bool, userToken: String) async throws -> String {
        guard page else { return userToken }
        let result: [String: Any]
        do {
            result = try await transport.request("GET", targetId, params: ["fields": "access_token"], token: userToken)
        } catch let error as FacebookError where error == .notFound || error.key == "facebook_error_auth" {
            throw FacebookError("facebook_error_page")
        }
        guard let token = result["access_token"] as? String, !token.isEmpty else { throw FacebookError("facebook_error_page") }
        return token
    }

    func create(_ connection: FacebookConnection, title: String, token: String) async throws -> FacebookLiveVideo {
        let title = try validateBroadcastTitle(title)
        var params = ["status": "LIVE_NOW", "title": title, "description": title, "fields": "id,secure_stream_url"]
        if !connection.page {
            let privacy = try JSONSerialization.data(withJSONObject: ["value": connection.privacy])
            params["privacy"] = String(decoding: privacy, as: UTF8.self)
        }
        let result = try await transport.request("POST", "\(connection.targetId)/live_videos", params: params, token: token)
        guard let id = result["id"] as? String, !id.isEmpty else { throw FacebookError() }
        do {
            let (server, key) = try splitFacebookStreamUrl(result["secure_stream_url"] as? String ?? "")
            return FacebookLiveVideo(id: id, serverUrl: server, key: key)
        } catch {
            try? await finish(id, token: token)
            throw error
        }
    }

    /// What Facebook says about the live video; nil once Facebook no longer has it.
    func status(_ id: String, token: String) async throws -> FacebookLiveStatus? {
        do {
            let result = try await transport.request("GET", id, params: ["fields": "id,status,permalink_url"], token: token)
            guard result["id"] as? String == id else { throw FacebookError() }
            return FacebookLiveStatus(id: id, status: result["status"] as? String ?? "", permalink: facebookPermalink(result["permalink_url"] as? String))
        } catch let error as FacebookError where error == .notFound {
            return nil
        }
    }

    /// The permissions the user actually granted; a business login does not list them in its redirect.
    func grantedPermissions(token: String) async throws -> [String] {
        let result = try await transport.request("GET", "me/permissions", params: [:], token: token)
        return (result["data"] as? [[String: Any]] ?? []).compactMap { item in
            item["status"] as? String == "granted" ? item["permission"] as? String : nil
        }
    }

    /// Removes every permission this app holds, which also ends its access to the account.
    func revokePermissions(token: String) async throws {
        _ = try await transport.request("DELETE", "me/permissions", params: [:], token: token)
    }

    /// Idempotent: a missing or already ended live video needs no more remote work.
    func finish(_ id: String, token: String) async throws {
        guard let status = try await status(id, token: token)?.status else { return }
        switch status {
        case "LIVE_STOPPED", "VOD", "PROCESSING", "SCHEDULED_CANCELED", "SCHEDULED_EXPIRED":
            return
        // Never shown to anyone: nothing to keep.
        case "UNPUBLISHED", "SCHEDULED_UNPUBLISHED":
            _ = try await transport.request("DELETE", id, params: [:], token: token)
        default:
            _ = try await transport.request("POST", id, params: ["end_live_video": "true"], token: token)
        }
    }
}
