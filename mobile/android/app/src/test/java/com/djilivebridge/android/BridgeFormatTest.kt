package com.djilivebridge.android

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
