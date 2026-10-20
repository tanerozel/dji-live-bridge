import SafariServices
import UIKit

/**
 * Opens a platform's page (a broadcast on YouTube or Facebook) in Safari's in-app view above the
 * app, so the drone screen stays where it was. Safari's view keeps its own login, separate from
 * Safari and from the login window: the first visit may ask to log in once. The app cannot see
 * what is typed there.
 */
@MainActor
func openInApp(_ url: URL) {
    guard url.scheme == "https",
          var top = UIApplication.shared.connectedScenes.compactMap({ $0 as? UIWindowScene })
            .first(where: { $0.activationState == .foregroundActive })?.windows.first(where: \.isKeyWindow)?.rootViewController else {
        UIApplication.shared.open(url)
        return
    }
    while let presented = top.presentedViewController { top = presented }
    let safari = SFSafariViewController(url: url)
    safari.dismissButtonStyle = .close
    safari.modalPresentationStyle = .pageSheet
    top.present(safari, animated: true)
}
