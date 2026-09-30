import Foundation
import Observation

/// A language the app is translated into, named in that language.
struct AppLanguage: Identifiable, Hashable {
    let tag: String
    let name: String
    var id: String { tag }
}

/// The desktop app's languages, in its order and with its names. The strings come from the
/// Android app (scripts/import-android-strings.mjs), one `<tag>.lproj` per language.
let appLanguages: [AppLanguage] = [
    AppLanguage(tag: "en", name: "English"),
    AppLanguage(tag: "es", name: "Español"),
    AppLanguage(tag: "zh-Hans", name: "中文（简体）"),
    AppLanguage(tag: "zh-Hant", name: "中文（繁體）"),
    AppLanguage(tag: "ar", name: "العربية"),
    AppLanguage(tag: "hi", name: "हिन्दी"),
    AppLanguage(tag: "pt", name: "Português"),
    AppLanguage(tag: "ru", name: "Русский"),
    AppLanguage(tag: "fr", name: "Français"),
    AppLanguage(tag: "de", name: "Deutsch"),
    AppLanguage(tag: "ja", name: "日本語"),
    AppLanguage(tag: "ko", name: "한국어"),
    AppLanguage(tag: "id", name: "Bahasa Indonesia"),
    AppLanguage(tag: "it", name: "Italiano"),
    AppLanguage(tag: "tr", name: "Türkçe"),
]

/// The listed language a locale identifier falls under; Chinese goes by script, everything else
/// by language.
func appLanguage(forIdentifier identifier: String) -> AppLanguage? {
    let locale = Locale(identifier: identifier)
    let code = locale.language.languageCode?.identifier ?? identifier
    let tag: String
    if code == "zh" {
        let script = locale.language.script?.identifier
        let region = locale.region?.identifier
        let traditional = script == "Hant" || (script == nil && ["TW", "HK", "MO"].contains(region ?? ""))
        tag = traditional ? "zh-Hant" : "zh-Hans"
    } else {
        tag = code == "in" ? "id" : code
    }
    return appLanguages.first { $0.tag == tag }
}

/**
 * Puts the app's strings into words in the language the app runs in. The user can pick one of
 * the app's languages in the app, like the Android 13+ language button, and it applies at once;
 * otherwise the app follows the phone (or the language picked for it in iOS Settings).
 */
@Observable
final class Localizer {
    static let shared = Localizer()

    private static let storageKey = "app_language"

    /// The chosen language, or nil when the app follows the phone.
    private(set) var language: AppLanguage?
    private(set) var locale: Locale
    @ObservationIgnored private var bundle: Bundle
    @ObservationIgnored private let english: Bundle

    init(defaults: UserDefaults = .standard) {
        english = Self.bundle(for: "en") ?? .main
        let chosen = defaults.string(forKey: Self.storageKey).flatMap { tag in appLanguages.first { $0.tag == tag } }
        language = chosen
        bundle = chosen.flatMap { Self.bundle(for: $0.tag) } ?? .main
        locale = Self.locale(for: chosen)
    }

    /// The language the strings are in right now.
    var effectiveLanguage: AppLanguage {
        language ?? appLanguage(forIdentifier: Bundle.main.preferredLocalizations.first ?? "en") ?? appLanguages[0]
    }

    var isRightToLeft: Bool {
        Locale.Language(identifier: effectiveLanguage.tag).characterDirection == .rightToLeft
    }

    func setLanguage(_ language: AppLanguage?, defaults: UserDefaults = .standard) {
        self.language = language
        bundle = language.flatMap { Self.bundle(for: $0.tag) } ?? .main
        locale = Self.locale(for: language)
        if let language {
            defaults.set(language.tag, forKey: Self.storageKey)
        } else {
            defaults.removeObject(forKey: Self.storageKey)
        }
    }

    /// The string `key` in the app's language, or in English when that language lacks it.
    func string(_ key: String) -> String {
        let missing = "\u{1}"
        let value = bundle.localizedString(forKey: key, value: missing, table: nil)
        if value != missing { return value }
        return english.localizedString(forKey: key, value: key, table: nil)
    }

    func string(_ key: String, _ arguments: [CVarArg]) -> String {
        String(format: string(key), locale: locale, arguments: arguments)
    }

    /// A plural from Localizable.stringsdict; `count` picks the form and is its first number.
    func plural(_ key: String, count: Int, _ arguments: [CVarArg] = []) -> String {
        let format = bundle.localizedString(forKey: key, value: nil, table: nil)
        return String(format: format, locale: locale, arguments: [count] + arguments)
    }

    private static func bundle(for tag: String) -> Bundle? {
        Bundle.main.path(forResource: tag, ofType: "lproj").flatMap(Bundle.init(path:))
    }

    private static func locale(for language: AppLanguage?) -> Locale {
        if let language { return Locale(identifier: language.tag) }
        // The phone's region formats numbers; the language is the one the strings are in.
        let tag = Bundle.main.preferredLocalizations.first ?? "en"
        let region = Locale.current.region?.identifier
        return Locale(identifier: region.map { "\(tag)_\($0)" } ?? tag)
    }
}

/// The string `key` in the app's language.
func tr(_ key: String, _ arguments: CVarArg...) -> String {
    arguments.isEmpty ? Localizer.shared.string(key) : Localizer.shared.string(key, arguments)
}

/// The plural `key` for `count`, with any further arguments after it.
func trPlural(_ key: String, _ count: Int, _ arguments: CVarArg...) -> String {
    Localizer.shared.plural(key, count: count, arguments)
}

/// Upper-cases the first letter of a sentence that may start with a phrase such as "on your
/// custom server", which the translations keep in lower case for use mid-sentence.
func sentenceStart(_ text: String, locale: Locale = Localizer.shared.locale) -> String {
    guard let first = text.first else { return text }
    return String(first).uppercased(with: locale) + text.dropFirst()
}
