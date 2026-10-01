import SwiftUI

extension DestinationKind {
    /// Instagram and TikTok hand out a new stream key for every broadcast.
    var keyChangesEachStream: Bool { self == .instagram || self == .tiktok }

    /// The platform as people call it; a custom server is named in the app's language.
    var displayName: String { brandName ?? tr("platform_custom") }

    /// The name for a message that is put into words later, in the language shown then.
    var nameText: UiText { brandName.map(UiText.raw) ?? .tr("platform_custom") }

    /**
     * "on Instagram": a phrase for where the stream is, spelled out per platform because some
     * languages (Turkish: Instagram'da, TikTok'ta) change the ending by how the name is pronounced.
     */
    var onPlatformKey: String { "on_\(rawValue)" }

    var keyHelpKey: String { "key_help_\(rawValue)" }

    var serverPlaceholder: String {
        if let defaultServerUrl { return defaultServerUrl }
        switch self {
        case .instagram: return "rtmps://edgetee-upload-….fbcdn.net:443/rtmp"
        case .tiktok: return "rtmp://push-rtmp-….tiktokcdn.com/game"
        default: return tr("server_placeholder")
        }
    }

    /// What to do on the platform once the stream arrives there.
    var liveReminderKey: String? {
        switch self {
        case .instagram, .tiktok, .youtube, .facebook: "live_reminder_\(rawValue)"
        case .twitch, .kick, .custom: nil
        }
    }

    // Tile colors match the desktop platform icons (src/styles.css .platform-icon.*).
    fileprivate var tileBackground: AnyShapeStyle {
        switch self {
        case .instagram:
            AnyShapeStyle(LinearGradient(
                stops: [
                    .init(color: Color(hex: 0xFEDA75), location: 0),
                    .init(color: Color(hex: 0xFA7E1E), location: 0.22),
                    .init(color: Color(hex: 0xD62976), location: 0.5),
                    .init(color: Color(hex: 0x962FBF), location: 0.75),
                    .init(color: Color(hex: 0x4F5BD5), location: 1),
                ],
                startPoint: .bottomLeading,
                endPoint: .topTrailing
            ))
        case .tiktok: AnyShapeStyle(Color.black)
        case .youtube: AnyShapeStyle(Color(hex: 0xFF0000))
        case .facebook: AnyShapeStyle(Color(hex: 0x1877F2))
        case .twitch: AnyShapeStyle(Color(hex: 0x9146FF))
        case .kick: AnyShapeStyle(Color(hex: 0x53FC18))
        case .custom: AnyShapeStyle(Color(hex: 0x5E5CE6))
        }
    }
}

/// The platform's app-icon style tile: brand background with a white glyph.
struct PlatformTile: View {
    let kind: DestinationKind
    let size: CGFloat

    var body: some View {
        let glyph = Image("platform-\(kind.rawValue)").resizable().renderingMode(.template).frame(width: size * 0.6, height: size * 0.6)
        ZStack {
            RoundedRectangle(cornerRadius: size * 0.28, style: .continuous).fill(kind.tileBackground)
            if kind == .tiktok {
                // TikTok's cyan/red echo, as on the desktop.
                let echo = size * 0.035
                glyph.foregroundStyle(Color(hex: 0x25F4EE)).offset(x: -echo, y: -echo)
                glyph.foregroundStyle(Color(hex: 0xFE2C55)).offset(x: echo, y: echo)
            }
            glyph.foregroundStyle(kind == .kick ? Color.black : Color.white)
        }
        .frame(width: size, height: size)
        .accessibilityHidden(true)
    }
}
