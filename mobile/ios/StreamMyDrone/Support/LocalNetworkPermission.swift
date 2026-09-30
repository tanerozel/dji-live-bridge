import Network

/**
 * Asks for local network access once, at the first launch. DJI Fly reaches the phone over the
 * local network, and iOS may keep other devices' traffic away from an app until the user has
 * allowed it. Browsing for the app's own Bonjour type (listed in NSBonjourServices) is the usual
 * way to bring up the question; nothing is found or advertised.
 */
@MainActor
enum LocalNetworkPermission {
    private static var browser: NWBrowser?

    static func request() {
        guard browser == nil else { return }
        let browser = NWBrowser(for: .bonjour(type: "_streammydrone._tcp", domain: nil), using: .tcp)
        browser.stateUpdateHandler = { _ in }
        browser.start(queue: .main)
        self.browser = browser
        Task {
            try? await Task.sleep(for: .seconds(60))
            browser.cancel()
        }
    }
}
