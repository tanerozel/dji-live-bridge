import Foundation

func formatBitrate(_ kbps: Double, locale: Locale = Localizer.shared.locale) -> String {
    kbps >= 1_000
        ? String(format: "%.1f Mbps", locale: locale, kbps / 1_000)
        : String(format: "%.0f kbps", locale: locale, kbps)
}

func formatBytes(_ bytes: Int64, locale: Locale = Localizer.shared.locale) -> String {
    switch bytes {
    case 1_000_000_000...: String(format: "%.2f GB", locale: locale, Double(bytes) / 1_000_000_000)
    case 1_000_000...: String(format: "%.1f MB", locale: locale, Double(bytes) / 1_000_000)
    case 1_000...: String(format: "%.0f KB", locale: locale, Double(bytes) / 1_000)
    default: "\(bytes) B"
    }
}

/// Seconds with one decimal, for how long the picture paused.
func formatSeconds(millis: Int64, locale: Locale = Localizer.shared.locale) -> String {
    String(format: "%.1f", locale: locale, Double(millis) / 1_000)
}

/// The relay reports codecs as FourCC labels ("avc1", "mp4a"); show the names people know.
func friendlyCodecName(_ label: String) -> String {
    switch label.trimmingCharacters(in: .whitespaces).lowercased() {
    case "avc1": "H.264"
    case "hvc1", "hev1": "H.265"
    case "vvc1": "H.266"
    case "av01": "AV1"
    case "vp08": "VP8"
    case "vp09": "VP9"
    case "mp4a": "AAC"
    case "opus": "Opus"
    case "mp3", ".mp3": "MP3"
    case "ac-3": "AC-3"
    case "ec-3": "E-AC-3"
    case "flac": "FLAC"
    default: label
    }
}

/// `mm:ss`, or `h:mm:ss` once the broadcast passes an hour.
func formatDuration(millis: Int64) -> String {
    let totalSeconds = max(millis / 1_000, 0)
    let hours = totalSeconds / 3_600
    let minutes = totalSeconds % 3_600 / 60
    let seconds = totalSeconds % 60
    return hours > 0
        ? String(format: "%d:%02d:%02d", hours, minutes, seconds)
        : String(format: "%02d:%02d", minutes, seconds)
}
