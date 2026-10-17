import Foundation

/// Public account/broadcast settings only; credentials never belong in this record.
struct YouTubeConnection: Codable, Equatable {
    let accountEmail: String
    let channelId: String
    let channelTitle: String
    let streamId: String
    var title: String
    var privacy: String
    var madeForKids: Bool

    func validate() throws {
        guard !accountEmail.isEmpty, !channelId.isEmpty, !streamId.isEmpty,
              ["private", "unlisted", "public"].contains(privacy) else { throw YouTubeError("youtube_error_api") }
        _ = try validateYouTubeTitle(title)
    }
}

struct YouTubeChannel: Equatable, Identifiable {
    let id: String
    let title: String
}

struct YouTubeStream {
    let id: String
    let serverUrl: String
    let key: String
}

/// Never exposes a response body, token, key, URL or underlying network error in diagnostics.
struct YouTubeError: Error {
    let key: String
    var text: UiText { .tr(key) }
    init(_ key: String = "youtube_error_api") { self.key = key }
}

func youTubeErrorText(_ error: Error) -> UiText {
    (error as? YouTubeError)?.text ?? (error as? DestinationProfileError)?.text ?? .tr("youtube_error_api")
}

func validateYouTubeTitle(_ value: String) throws -> String {
    let title = value.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !title.isEmpty, title.unicodeScalars.count <= 100,
          !title.contains("<"), !title.contains(">"),
          !title.unicodeScalars.contains(where: CharacterSet.controlCharacters.contains) else {
        throw YouTubeError("youtube_error_title")
    }
    return title
}

protocol YouTubeTransport {
    func request(_ method: String, _ path: String, query: [String: String], body: [String: Any]?) async throws -> [String: Any]
}

/// Ephemeral HTTPS session; never forwards the bearer token through redirects or disk caches.
final class GoogleYouTubeTransport: NSObject, YouTubeTransport, URLSessionTaskDelegate {
    private let token: String
    private func makeSession() -> URLSession {
        let config = URLSessionConfiguration.ephemeral
        config.urlCache = nil
        config.httpCookieStorage = nil
        config.requestCachePolicy = .reloadIgnoringLocalCacheData
        config.timeoutIntervalForRequest = 15
        config.timeoutIntervalForResource = 30
        return URLSession(configuration: config, delegate: self, delegateQueue: nil)
    }

    init(token: String) { self.token = token }

    func urlSession(_ session: URLSession, task: URLSessionTask, willPerformHTTPRedirection response: HTTPURLResponse,
                    newRequest request: URLRequest, completionHandler: @escaping (URLRequest?) -> Void) {
        completionHandler(nil)
    }

    func request(_ method: String, _ path: String, query: [String: String], body: [String: Any]?) async throws -> [String: Any] {
        // Paths are fixed application constants. Reject malformed paths even in future callers.
        guard path.hasPrefix("youtube/v3/"), !path.contains(".."), !path.contains("?") else { throw YouTubeError() }
        var components = URLComponents(string: "https://www.googleapis.com/\(path)")!
        components.queryItems = query.sorted { $0.key < $1.key }.map { URLQueryItem(name: $0.key, value: $0.value) }
        var request = URLRequest(url: components.url!)
        request.httpMethod = method
        request.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
        request.setValue("application/json", forHTTPHeaderField: "Accept")
        if let body {
            request.setValue("application/json; charset=UTF-8", forHTTPHeaderField: "Content-Type")
            request.httpBody = try JSONSerialization.data(withJSONObject: body)
        }
        let session = makeSession()
        defer { session.invalidateAndCancel() }
        do {
            let (bytes, response) = try await session.bytes(for: request)
            guard let http = response as? HTTPURLResponse else { throw YouTubeError() }
            var data = Data()
            for try await byte in bytes {
                guard data.count < 1_048_576 else { throw YouTubeError() }
                data.append(byte)
            }
            let json = data.isEmpty ? [:] : (try JSONSerialization.jsonObject(with: data) as? [String: Any] ?? [:])
            guard (200...299).contains(http.statusCode) else { throw Self.failure(status: http.statusCode, json: json) }
            return json
        } catch let error as YouTubeError { throw error }
        catch { throw YouTubeError("youtube_error_network") }
    }

    static func failure(status: Int, json: [String: Any]) -> YouTubeError {
        let errors = (json["error"] as? [String: Any])?["errors"] as? [[String: Any]] ?? []
        let reasons = Set(errors.compactMap { $0["reason"] as? String })
        let key: String
        if status == 401 { key = "youtube_error_auth" }
        else if !reasons.isDisjoint(with: ["accessNotConfigured", "serviceDisabled"]) { key = "youtube_error_setup" }
        else if !reasons.isDisjoint(with: ["liveStreamingNotEnabled", "livePermissionBlocked", "insufficientLivePermissions"]) { key = "youtube_error_live" }
        else if status == 429 || !reasons.isDisjoint(with: ["quotaExceeded", "dailyLimitExceeded", "rateLimitExceeded", "userRequestsExceedRateLimit"]) { key = "youtube_error_quota" }
        else if !reasons.isDisjoint(with: ["insufficientPermissions", "forbidden"]) { key = "youtube_error_auth" }
        else { key = "youtube_error_api" }
        return YouTubeError(key)
    }
}

struct YouTubeAPI {
    let transport: YouTubeTransport

    func channels() async throws -> [YouTubeChannel] {
        let result = try await transport.request("GET", "youtube/v3/channels", query: ["part": "snippet", "mine": "true", "maxResults": "50"], body: nil)
        let channels = (result["items"] as? [[String: Any]] ?? []).compactMap { item -> YouTubeChannel? in
            guard let id = item["id"] as? String, let title = (item["snippet"] as? [String: Any])?["title"] as? String else { return nil }
            return YouTubeChannel(id: id, title: title)
        }
        guard !channels.isEmpty else { throw YouTubeError("youtube_error_channel") }
        return channels
    }

    func checkChannel(_ id: String) async throws {
        guard try await channels().contains(where: { $0.id == id }) else { throw YouTubeError("youtube_error_channel_changed") }
    }

    func createStream() async throws -> YouTubeStream {
        let result = try await transport.request("POST", "youtube/v3/liveStreams", query: ["part": "id,cdn,snippet,contentDetails"], body: [
            "snippet": ["title": "Stream My Drone"],
            "cdn": ["ingestionType": "rtmp", "frameRate": "variable", "resolution": "variable"],
            "contentDetails": ["isReusable": true],
        ])
        let id = try identifier(result)
        do {
            guard let ingest = (result["cdn"] as? [String: Any])?["ingestionInfo"] as? [String: Any],
                  let address = ingest["rtmpsIngestionAddress"] as? String, address.hasPrefix("rtmps://"),
                  let key = ingest["streamName"] as? String else { throw YouTubeError() }
            let server = try validateServerUrl(address)
            try validateStreamKey(key)
            return YouTubeStream(id: id, serverUrl: server, key: key)
        } catch {
            try? await deleteStream(id)
            throw YouTubeError()
        }
    }

    func deleteStream(_ id: String) async throws {
        _ = try await transport.request("DELETE", "youtube/v3/liveStreams", query: ["id": id], body: nil)
    }

    func createBroadcast(_ connection: YouTubeConnection) async throws -> String {
        try connection.validate()
        let result = try await transport.request("POST", "youtube/v3/liveBroadcasts", query: ["part": "id,snippet,status,contentDetails"], body: [
            "snippet": ["title": try validateYouTubeTitle(connection.title), "scheduledStartTime": ISO8601DateFormatter().string(from: Date().addingTimeInterval(10))],
            "status": ["privacyStatus": connection.privacy, "selfDeclaredMadeForKids": connection.madeForKids],
            "contentDetails": ["enableAutoStart": true, "enableAutoStop": true, "monitorStream": ["enableMonitorStream": false]],
        ])
        return try identifier(result)
    }

    func bind(_ broadcastId: String, streamId: String) async throws {
        _ = try await transport.request("POST", "youtube/v3/liveBroadcasts/bind", query: ["part": "id", "id": broadcastId, "streamId": streamId], body: nil)
    }

    /// Deletes only an unstarted event; completed/live videos are never deleted.
    func finish(_ id: String) async throws {
        let result = try await transport.request("GET", "youtube/v3/liveBroadcasts", query: ["part": "status", "id": id], body: nil)
        guard let items = result["items"] as? [[String: Any]] else { throw YouTubeError() }
        guard let item = items.first else { return }
        guard let status = (item["status"] as? [String: Any])?["lifeCycleStatus"] as? String else { throw YouTubeError() }
        if ["complete", "revoked"].contains(status) { return }
        if ["created", "ready"].contains(status) {
            _ = try await transport.request("DELETE", "youtube/v3/liveBroadcasts", query: ["id": id], body: nil)
        } else {
            _ = try await transport.request("POST", "youtube/v3/liveBroadcasts/transition", query: ["part": "id,status", "id": id, "broadcastStatus": "complete"], body: nil)
        }
    }

    private func identifier(_ json: [String: Any]) throws -> String {
        guard let id = json["id"] as? String, !id.isEmpty else { throw YouTubeError() }
        return id
    }
}
