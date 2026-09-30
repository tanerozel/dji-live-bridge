import SwiftUI

/// The desktop app's themes (src/theme.ts); light is the default there too.
enum ThemeChoice: String, CaseIterable, Identifiable {
    case system, light, dark, midnight, sand

    static let defaultChoice = ThemeChoice.light

    var id: String { rawValue }

    var labelKey: String { "theme_\(rawValue)" }

    static func fromStorage(_ value: String?) -> ThemeChoice {
        value.flatMap(ThemeChoice.init(rawValue:)) ?? defaultChoice
    }
}

/**
 * One theme's colors, named after the desktop tokens in src/styles.css, as on Android. The
 * themes color the guide, the key screen and the dialogs; the main screen is always dark.
 */
struct BridgePalette {
    let isDark: Bool
    let background: Color
    let card: Color
    let field: Color
    let fieldStrong: Color
    let border: Color
    let text: Color
    let muted: Color
    let faint: Color
    /// Selection rings, icons and focus.
    let accent: Color
    /// Accent for text and small icons.
    let link: Color
    /// Filled buttons; always carries white text.
    let action: Color
    let accentSoft: Color
    let onAccentSoft: Color
    let success: Color
    let successText: Color
    let warningText: Color
    let dangerText: Color
    let dangerSoft: Color

    static let light = BridgePalette(
        isDark: false, background: Color(hex: 0xF5F5F7), card: Color(hex: 0xFFFFFF), field: Color(hex: 0xF2F2F5),
        fieldStrong: Color(hex: 0xE8E8ED), border: Color(hex: 0xE3E3E8), text: Color(hex: 0x1D1D1F), muted: Color(hex: 0x6E6E73),
        faint: Color(hex: 0x8E8E93), accent: Color(hex: 0x0A84FF), link: Color(hex: 0x006AD6), action: Color(hex: 0x0071E3),
        accentSoft: Color(hex: 0xEAF3FF), onAccentSoft: Color(hex: 0x004A99), success: Color(hex: 0x34C759),
        successText: Color(hex: 0x1A7F37), warningText: Color(hex: 0x9A5B00), dangerText: Color(hex: 0xD70015),
        dangerSoft: Color(hex: 0xFFF0EF)
    )

    static let sand = BridgePalette(
        isDark: false, background: Color(hex: 0xF5F0E8), card: Color(hex: 0xFFFDF9), field: Color(hex: 0xF4EDE3),
        fieldStrong: Color(hex: 0xEAE1D4), border: Color(hex: 0xE7DDCF), text: Color(hex: 0x2B2118), muted: Color(hex: 0x76675A),
        faint: Color(hex: 0x9A8A78), accent: Color(hex: 0xC8581C), link: Color(hex: 0xB04B14), action: Color(hex: 0xB04B14),
        accentSoft: Color(hex: 0xFBEADF), onAccentSoft: Color(hex: 0x7A3510), success: Color(hex: 0x3F9B5A),
        successText: Color(hex: 0x2C7A44), warningText: Color(hex: 0x8A5A00), dangerText: Color(hex: 0xB42318),
        dangerSoft: Color(hex: 0xFCEBE8)
    )

    static let dark = BridgePalette(
        isDark: true, background: Color(hex: 0x161618), card: Color(hex: 0x1F1F22), field: Color(hex: 0x27272B),
        fieldStrong: Color(hex: 0x2F2F34), border: Color(hex: 0x2E2E33), text: Color(hex: 0xF2F2F5), muted: Color(hex: 0xA1A1A8),
        faint: Color(hex: 0x76767E), accent: Color(hex: 0x0A84FF), link: Color(hex: 0x339AFF), action: Color(hex: 0x0071E3),
        accentSoft: Color(hex: 0x1C2F45), onAccentSoft: Color(hex: 0x9CCBFF), success: Color(hex: 0x30D158),
        successText: Color(hex: 0x5EE08A), warningText: Color(hex: 0xFFCC66), dangerText: Color(hex: 0xFF8A80),
        dangerSoft: Color(hex: 0x3E2425)
    )

    static let midnight = BridgePalette(
        isDark: true, background: Color(hex: 0x0C1120), card: Color(hex: 0x131A2C), field: Color(hex: 0x1A2238),
        fieldStrong: Color(hex: 0x222B45), border: Color(hex: 0x222B44), text: Color(hex: 0xE9EDF8), muted: Color(hex: 0x9BA5C4),
        faint: Color(hex: 0x6B7596), accent: Color(hex: 0x5B6CF0), link: Color(hex: 0x6F7FF5), action: Color(hex: 0x4F5FE8),
        accentSoft: Color(hex: 0x1F274B), onAccentSoft: Color(hex: 0xC3CBFF), success: Color(hex: 0x2FD28A),
        successText: Color(hex: 0x5FE3A8), warningText: Color(hex: 0xFFCF70), dangerText: Color(hex: 0xFF8F9B),
        dangerSoft: Color(hex: 0x342133)
    )

    static func forChoice(_ choice: ThemeChoice, systemDark: Bool) -> BridgePalette {
        switch choice {
        case .system: systemDark ? .dark : .light
        case .light: .light
        case .dark: .dark
        case .midnight: .midnight
        case .sand: .sand
        }
    }
}

private struct PaletteKey: EnvironmentKey {
    static let defaultValue = BridgePalette.light
}

extension EnvironmentValues {
    var palette: BridgePalette {
        get { self[PaletteKey.self] }
        set { self[PaletteKey.self] = newValue }
    }
}

/// Gives the content the chosen theme's palette and matching light or dark system controls.
struct Themed: ViewModifier {
    let choice: ThemeChoice
    @Environment(\.colorScheme) private var systemScheme

    func body(content: Content) -> some View {
        let palette = BridgePalette.forChoice(choice, systemDark: systemScheme == .dark)
        content
            .environment(\.palette, palette)
            .tint(palette.link)
            .environment(\.colorScheme, palette.isDark ? .dark : .light)
    }
}

extension View {
    func themed(_ choice: ThemeChoice) -> some View { modifier(Themed(choice: choice)) }
}

extension Color {
    init(hex: UInt32, opacity: Double = 1) {
        self.init(
            .sRGB,
            red: Double(hex >> 16 & 0xFF) / 255,
            green: Double(hex >> 8 & 0xFF) / 255,
            blue: Double(hex & 0xFF) / 255,
            opacity: opacity
        )
    }
}

/// The drone screen's own colors: it is always dark, like a camera app.
enum DroneColors {
    /// Behind the connect card until the picture comes: the brand's blue, nearly black.
    static let waitingBackground = LinearGradient(colors: [Color(hex: 0x101A2A), Color(hex: 0x05070B)], startPoint: .top, endPoint: .bottom)
    static let sheet = Color(hex: 0x14171C)
    static let ready = Color(hex: 0x4ADE80)
    static let warning = Color(hex: 0xFBBF24)
    static let danger = Color(hex: 0xF87171)
    static let goLiveStart = Color(hex: 0xFF8A3D)
    static let goLiveEnd = Color(hex: 0xEA580C)
    /// The see-through dark background of everything drawn over the picture.
    static let glass = Color.black.opacity(0.55)
}
