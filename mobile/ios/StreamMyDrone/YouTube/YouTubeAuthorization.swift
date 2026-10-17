import GoogleSignIn
import UIKit

/// Google owns its refresh credentials in Keychain. The app never serializes SDK users/tokens.
@MainActor
final class YouTubeAuthorization {
    static let shared = YouTubeAuthorization()
    static let scope = "https://www.googleapis.com/auth/youtube.force-ssl"

    func connect(hint: String? = nil) async throws -> (email: String, api: YouTubeAPI) {
        guard let client = Bundle.main.object(forInfoDictionaryKey: "GIDClientID") as? String,
              client.hasSuffix(".apps.googleusercontent.com") else { throw YouTubeError("youtube_error_setup") }
        guard var presenter = UIApplication.shared.connectedScenes.compactMap({ $0 as? UIWindowScene })
            .first(where: { $0.activationState == .foregroundActive })?.windows.first(where: \.isKeyWindow)?.rootViewController else {
            throw YouTubeError("youtube_error_auth")
        }
        while let presented = presenter.presentedViewController { presenter = presented }
        do {
            let result = try await GIDSignIn.sharedInstance.signIn(withPresenting: presenter, hint: hint, additionalScopes: [Self.scope])
            guard let email = result.user.profile?.email, !email.isEmpty,
                  result.user.grantedScopes?.contains(Self.scope) == true else { throw YouTubeError("youtube_error_auth") }
            let user = try await result.user.refreshTokensIfNeeded()
            return (email, YouTubeAPI(transport: GoogleYouTubeTransport(token: user.accessToken.tokenString)))
        } catch { throw YouTubeError("youtube_error_auth") }
    }

    /// Background work must never open an account/consent screen or use another channel's grant.
    func api(for email: String) async throws -> YouTubeAPI {
        do {
            let user: GIDGoogleUser
            if let current = GIDSignIn.sharedInstance.currentUser { user = current }
            else { user = try await GIDSignIn.sharedInstance.restorePreviousSignIn() }
            guard user.profile?.email.caseInsensitiveCompare(email) == .orderedSame,
                  user.grantedScopes?.contains(Self.scope) == true else { throw YouTubeError("youtube_error_channel_changed") }
            let refreshed = try await user.refreshTokensIfNeeded()
            return YouTubeAPI(transport: GoogleYouTubeTransport(token: refreshed.accessToken.tokenString))
        } catch let error as YouTubeError { throw error }
        catch { throw YouTubeError("youtube_error_auth") }
    }

    func disconnect(email: String) async throws {
        _ = try await api(for: email)
        do { try await GIDSignIn.sharedInstance.disconnect() }
        catch { throw YouTubeError("youtube_error_auth") }
    }
}
