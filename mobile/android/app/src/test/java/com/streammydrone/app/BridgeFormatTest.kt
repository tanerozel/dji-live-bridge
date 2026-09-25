package com.streammydrone.app

import org.junit.Assert.assertEquals
import org.junit.Test
import java.util.Locale

class BridgeFormatTest {
    private val turkish = Locale.forLanguageTag("tr-TR")

    @Test
    fun `bitrate switches to megabits with a local decimal separator`() {
        assertEquals("850 kbps", formatBitrate(850.0, turkish))
        assertEquals("6,2 Mbps", formatBitrate(6_200.0, turkish))
        assertEquals("6.2 Mbps", formatBitrate(6_200.0, Locale.US))
    }

    @Test
    fun `bytes use decimal units`() {
        assertEquals("999 B", formatBytes(999, turkish))
        assertEquals("12 KB", formatBytes(12_000, turkish))
        assertEquals("184,0 MB", formatBytes(184_000_000, turkish))
        assertEquals("1,25 GB", formatBytes(1_250_000_000, turkish))
    }

    @Test
    fun `a pause is shown in seconds with one decimal`() {
        assertEquals("0.7", formatSeconds(700, Locale.US))
        assertEquals("1,8", formatSeconds(1_750, turkish))
        assertEquals("12.0", formatSeconds(12_000, Locale.US))
    }

    @Test
    fun `the phone's wi-fi shows its signal, band and speed`() {
        // Each number keeps its unit on the same line.
        assertEquals("-58\u00A0dBm · 5\u00A0GHz · 866\u00A0Mbps", formatWifiLink(WifiLink(-58, 5_180, 866), Locale.US))
        assertEquals("-71\u00A0dBm · 2,4\u00A0GHz · 72\u00A0Mbps", formatWifiLink(WifiLink(-71, 2_437, 72), turkish))
        // A speed the phone does not know is left out.
        assertEquals("-49\u00A0dBm · 6\u00A0GHz", formatWifiLink(WifiLink(-49, 5_955, -1), Locale.US))
    }

    @Test
    fun `relay fourcc labels become familiar codec names`() {
        assertEquals("H.264", friendlyCodecName("avc1"))
        assertEquals("H.265", friendlyCodecName("hvc1"))
        assertEquals("AV1", friendlyCodecName("av01"))
        assertEquals("AAC", friendlyCodecName("mp4a"))
        assertEquals("Opus", friendlyCodecName("Opus"))
        assertEquals("MP3", friendlyCodecName(".mp3"))
        assertEquals("legacy:2", friendlyCodecName("legacy:2"))
    }

    @Test
    fun `duration shows hours only when needed`() {
        assertEquals("00:00", formatDuration(0))
        assertEquals("00:00", formatDuration(-5_000))
        assertEquals("01:05", formatDuration(65_000))
        assertEquals("59:59", formatDuration(3_599_000))
        assertEquals("1:00:00", formatDuration(3_600_000))
        assertEquals("2:03:04", formatDuration(7_384_000))
    }
}
