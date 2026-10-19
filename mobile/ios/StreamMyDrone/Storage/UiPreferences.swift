import Foundation

/// Non-sensitive UI settings. Stream keys never go here; they stay in the Keychain.
struct UiPreferences {
    private let defaults: UserDefaults

    init(defaults: UserDefaults = .standard) {
        self.defaults = defaults
    }

    // Bumped when the guide changes so everyone sees the new one once.
    private static let guideCompletedKey = "guide_completed_v3"
    private static let themeKey = "theme"
    private static let pictureFitKey = "picture_fit"
    private static let notificationsAskedKey = "notifications_asked"
    private static let lastBroadcastTitleKey = "last_broadcast_title"
    private static let instagramAudienceKey = "instagram_audience"

    var guideCompleted: Bool {
        get { defaults.bool(forKey: Self.guideCompletedKey) }
        nonmutating set { defaults.set(newValue, forKey: Self.guideCompletedKey) }
    }

    var theme: ThemeChoice {
        get { ThemeChoice.fromStorage(defaults.string(forKey: Self.themeKey)) }
        nonmutating set { defaults.set(newValue.rawValue, forKey: Self.themeKey) }
    }

    var pictureFit: PictureFit {
        get { PictureFit.fromStorage(defaults.string(forKey: Self.pictureFitKey)) }
        nonmutating set { defaults.set(newValue.rawValue, forKey: Self.pictureFitKey) }
    }

    /// The title the last broadcast went out with, offered again for the next one.
    var lastBroadcastTitle: String? {
        get { defaults.string(forKey: Self.lastBroadcastTitleKey) }
        nonmutating set { defaults.set(newValue, forKey: Self.lastBroadcastTitleKey) }
    }

    /// The audience last chosen for Instagram; Practice until the user picks Public.
    var instagramAudience: InstagramAudience {
        get { InstagramAudience.fromStorage(defaults.string(forKey: Self.instagramAudienceKey)) }
        nonmutating set { defaults.set(newValue.rawValue, forKey: Self.instagramAudienceKey) }
    }

    /// Whether the notification permission was asked for, which iOS allows only once.
    var notificationsAsked: Bool {
        get { defaults.bool(forKey: Self.notificationsAskedKey) }
        nonmutating set { defaults.set(newValue, forKey: Self.notificationsAskedKey) }
    }
}
