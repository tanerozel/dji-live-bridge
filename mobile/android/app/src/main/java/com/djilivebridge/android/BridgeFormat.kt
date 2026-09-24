package com.djilivebridge.android

import java.util.Locale

internal fun formatBitrate(kbps: Double, locale: Locale = Locale.getDefault()): String = when {
    kbps >= 1_000 -> String.format(locale, "%.1f Mbps", kbps / 1_000)
    else -> String.format(locale, "%.0f kbps", kbps)
}

internal fun formatBytes(bytes: Long, locale: Locale = Locale.getDefault()): String = when {
    bytes >= 1_000_000_000 -> String.format(locale, "%.2f GB", bytes / 1_000_000_000.0)
    bytes >= 1_000_000 -> String.format(locale, "%.1f MB", bytes / 1_000_000.0)
    bytes >= 1_000 -> String.format(locale, "%.0f KB", bytes / 1_000.0)
    else -> "$bytes B"
}

/** The relay reports codecs as FourCC labels ("avc1", "mp4a"); show the names people know. */
internal fun friendlyCodecName(label: String): String = when (label.trim().lowercase(Locale.ROOT)) {
    "avc1" -> "H.264"
    "hvc1", "hev1" -> "H.265"
    "vvc1" -> "H.266"
    "av01" -> "AV1"
    "vp08" -> "VP8"
    "vp09" -> "VP9"
    "mp4a" -> "AAC"
    "opus" -> "Opus"
    "mp3", ".mp3" -> "MP3"
    "ac-3" -> "AC-3"
    "ec-3" -> "E-AC-3"
    "flac" -> "FLAC"
    else -> label
}

/** `mm:ss`, or `h:mm:ss` once the broadcast passes an hour. */
internal fun formatDuration(millis: Long): String {
    val totalSeconds = (millis / 1_000).coerceAtLeast(0)
    val hours = totalSeconds / 3_600
    val minutes = totalSeconds % 3_600 / 60
    val seconds = totalSeconds % 60
    return if (hours > 0) {
        String.format(Locale.ROOT, "%d:%02d:%02d", hours, minutes, seconds)
    } else {
        String.format(Locale.ROOT, "%02d:%02d", minutes, seconds)
    }
}
