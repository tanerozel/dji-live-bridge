import AuthenticationServices
import Foundation
import Security
import UIKit

/**
 * Asked for once on Facebook's own login page. `publish_video` creates live videos on the user's
 * profile; the `pages_*` permissions list the user's Pages and publish to them.
 */
enum FacebookPermissions {
    static let all = ["public_profile", "publish_video", "pages_show_list", "pages_read_engagement", "pages_manage_posts"]
    static let profile: Set<String> = ["publish_video"]
    static let page: Set<String> = ["pages_show_list", "pages_read_engagement", "pages_manage_posts"]

    static func allow(_ scopes: [String], page: Bool) -> Bool {
        (page ? Self.page : Self.profile).isSubset(of: Set(scopes))
    }
}

/// The one Facebook login the app holds. Kept only in this device's Keychain, never in files or logs.
struct FacebookLogin: Codable, Equatable {
    let userId: String
    let token: String
    let expires: Date?
    let scopes: [String]

    func usable(at now: Date = Date()) -> Bool { expires.map { $0 > now } ?? true }
}

protocol FacebookLoginStore {
    func load() -> FacebookLogin?
    func save(_ login: FacebookLogin) throws
    func delete()
}

/// After first unlock, so a broadcast can still be ended while the phone is locked in Picture in Picture.
struct KeychainFacebookLoginStore: FacebookLoginStore {
    private let query: [String: Any] = [
        kSecClass as String: kSecClassGenericPassword,
        kSecAttrService as String: "com.streammydrone.app.facebook-login",
        kSecAttrAccount as String: "current",
        kSecAttrSynchronizable as String: false,
    ]

    func load() -> FacebookLogin? {
        var request = query
        request[kSecReturnData as String] = true
        request[kSecMatchLimit as String] = kSecMatchLimitOne
        var result: CFTypeRef?
        guard SecItemCopyMatching(request as CFDictionary, &result) == errSecSuccess, let data = result as? Data else { return nil }
        return try? JSONDecoder().decode(FacebookLogin.self, from: data)
    }

    func save(_ login: FacebookLogin) throws {
        let attributes: [String: Any] = [
            kSecValueData as String: try JSONEncoder().encode(login),
            kSecAttrAccessible as String: kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly,
        ]
        var status = SecItemUpdate(query as CFDictionary, attributes as CFDictionary)
        if status == errSecItemNotFound {
            status = SecItemAdd(query.merging(attributes) { $1 } as CFDictionary, nil)
        }
        guard status == errSecSuccess else { throw KeychainError(status: status) }
    }

    func delete() {
        SecItemDelete(query as CFDictionary)
    }
}

/// What Facebook's login page sends back to `fb<App ID>://authorize/`, in the fragment or, on errors, the query.
func parseFacebookCallback(_ url: URL, state: String, now: Date = Date()) throws -> (token: String, expires: Date?, scopes: [String]) {
    var values: [String: String] = [:]
    for part in [URLComponents(url: url, resolvingAgainstBaseURL: false)?.percentEncodedQuery, url.fragment(percentEncoded: true)] {
        var parser = URLComponents()
        parser.percentEncodedQuery = part
        for item in parser.queryItems ?? [] { values[item.name] = item.value ?? "" }
    }
    guard values["state"] == state else { throw FacebookError("facebook_error_auth") }
    guard values["error"] == nil, let token = values["access_token"], !token.isEmpty else { throw FacebookError("facebook_error_auth") }
    let seconds = Double(values["expires_in"] ?? "") ?? 0
    let scopes = (values["granted_scopes"] ?? "").split(separator: ",").map(String.init)
    return (token, seconds > 0 ? now.addingTimeInterval(seconds) : nil, scopes)
}

/**
 * Facebook's login in the system's secure web authentication window: the Facebook SDK is not used,
 * because without App Tracking Transparency it only gives Limited Login, which cannot publish.
 */
@MainActor
final class FacebookAuthorization: NSObject, ASWebAuthenticationPresentationContextProviding {
    static let shared = FacebookAuthorization()
    private let store: FacebookLoginStore
    private var session: ASWebAuthenticationSession?

    init(store: FacebookLoginStore = KeychainFacebookLoginStore()) {
        self.store = store
    }

    /// From Info.plist; empty in a build without a Facebook app.
    static var appId: String? {
        guard let id = Bundle.main.object(forInfoDictionaryKey: "FacebookAppID") as? String,
              !id.isEmpty, id.allSatisfy(\.isASCII), id.allSatisfy(\.isNumber) else { return nil }
        return id
    }

    /// The Facebook Login for Business configuration that lists the permissions; an app of the
    /// business type rejects a plain `scope` request.
    static var configId: String? {
        guard let id = Bundle.main.object(forInfoDictionaryKey: "FacebookLoginConfigID") as? String,
              !id.isEmpty, id.allSatisfy(\.isASCII), id.allSatisfy(\.isNumber) else { return nil }
        return id
    }

    var current: FacebookLogin? { store.load() }

    /// Opens Facebook's login and returns who logged in. Nil when the window closed without a login.
    func connect() async throws -> (user: FacebookUser, login: FacebookLogin)? {
        guard let appId = Self.appId else { throw FacebookError("facebook_error_setup") }
        let state = UUID().uuidString
        let redirect = "fb\(appId)://authorize/"
        var components = URLComponents(string: "https://www.facebook.com/\(facebookGraphVersion)/dialog/oauth")!
        var items = [
            URLQueryItem(name: "client_id", value: appId),
            URLQueryItem(name: "redirect_uri", value: redirect),
            URLQueryItem(name: "response_type", value: "token"),
            URLQueryItem(name: "state", value: state),
            URLQueryItem(name: "display", value: "touch"),
        ]
        if let configId = Self.configId {
            items.append(URLQueryItem(name: "config_id", value: configId))
        } else {
            items.append(URLQueryItem(name: "scope", value: FacebookPermissions.all.joined(separator: ",")))
            // Asks again for a permission the user declined last time.
            items.append(URLQueryItem(name: "auth_type", value: "rerequest"))
        }
        components.queryItems = items
        guard let callback = try await authenticate(components.url!, scheme: "fb\(appId)") else { return nil }
        let result = try parseFacebookCallback(callback, state: state)
        let api = FacebookAPI(transport: GraphFacebookTransport())
        let user = try await api.me(token: result.token)
        let scopes = try await api.grantedPermissions(token: result.token)
        let login = FacebookLogin(userId: user.id, token: result.token, expires: result.expires, scopes: scopes)
        do { try store.save(login) } catch { throw FacebookError("facebook_error_auth") }
        return (user, login)
    }

    /// For background work: the saved login, if it still belongs to `userId` and may publish there.
    func token(for userId: String, page: Bool) throws -> String {
        guard let login = store.load(), login.usable() else { throw FacebookError("facebook_error_auth") }
        guard login.userId == userId else { throw FacebookError("facebook_error_account") }
        guard FacebookPermissions.allow(login.scopes, page: page) else { throw FacebookError("facebook_error_auth") }
        return login.token
    }

    /// Removes this app's access on Facebook's side, then forgets the login.
    func disconnect() async throws {
        if let login = store.load() {
            do { try await FacebookAPI(transport: GraphFacebookTransport()).revokePermissions(token: login.token) }
            // An already revoked or expired login has no access left to remove.
            catch let error as FacebookError where error.key == "facebook_error_auth" {}
        }
        store.delete()
    }

    /// Forgets the saved login without asking Facebook, so another account can log in.
    func forget() { store.delete() }

    private func authenticate(_ url: URL, scheme: String) async throws -> URL? {
        try await withCheckedThrowingContinuation { continuation in
            let completion: (URL?, Error?) -> Void = { [weak self] url, error in
                Task { @MainActor in self?.session = nil }
                if let url { continuation.resume(returning: url) }
                else if (error as? ASWebAuthenticationSessionError)?.code == .canceledLogin { continuation.resume(returning: nil) }
                else { continuation.resume(throwing: FacebookError("facebook_error_auth")) }
            }
            let session: ASWebAuthenticationSession
            if #available(iOS 17.4, *) {
                session = ASWebAuthenticationSession(url: url, callback: .customScheme(scheme), completionHandler: completion)
            } else {
                session = ASWebAuthenticationSession(url: url, callbackURLScheme: scheme, completionHandler: completion)
            }
            session.presentationContextProvider = self
            // Shares Safari's Facebook login, so a signed-in user only has to confirm.
            session.prefersEphemeralWebBrowserSession = false
            self.session = session
            if !session.start() {
                self.session = nil
                continuation.resume(throwing: FacebookError("facebook_error_auth"))
            }
        }
    }

    nonisolated func presentationAnchor(for session: ASWebAuthenticationSession) -> ASPresentationAnchor {
        MainActor.assumeIsolated {
            UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }
                .first { $0.activationState == .foregroundActive }?.windows.first(where: \.isKeyWindow) ?? ASPresentationAnchor()
        }
    }
}
