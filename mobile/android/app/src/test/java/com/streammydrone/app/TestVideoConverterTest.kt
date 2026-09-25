package com.streammydrone.app

import android.media.MediaFormat
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class TestVideoConverterTest {
    private val droneLike = VideoFileInfo(
        mime = MediaFormat.MIMETYPE_VIDEO_AVC,
        width = 1280,
        height = 720,
        rotationDegrees = 0,
        frameRate = 30f,
        bitrate = 4_200_000,
        hdr = false,
        audioMime = MediaFormat.MIMETYPE_AUDIO_AAC,
    )

    @Test
    fun `a video like DJI Fly's goes out as it is`() {
        assertFalse(needsConversion(droneLike))
        assertFalse(needsConversion(droneLike.copy(width = 720, height = 1280)))
        assertFalse(needsConversion(droneLike.copy(frameRate = 29.97f)))
        assertFalse(needsConversion(droneLike.copy(frameRate = null)))
    }

    @Test
    fun `a phone recording is converted`() {
        // The video that stalled a live test: 4K portrait at 90 Mbps.
        assertTrue(needsConversion(droneLike.copy(width = 2160, height = 3840, bitrate = 90_700_000)))
        assertTrue(needsConversion(droneLike.copy(width = 1920, height = 1080)))
        assertTrue(needsConversion(droneLike.copy(bitrate = 12_000_000)))
        assertTrue(needsConversion(droneLike.copy(frameRate = 60f)))
    }

    @Test
    fun `what DJI Fly cannot send is converted`() {
        assertTrue(needsConversion(droneLike.copy(mime = MediaFormat.MIMETYPE_VIDEO_HEVC)))
        assertTrue(needsConversion(droneLike.copy(hdr = true)))
        // FLV has no rotation, so a video displayed rotated would go out sideways or upside down.
        assertTrue(needsConversion(droneLike.copy(rotationDegrees = 90)))
        assertTrue(needsConversion(droneLike.copy(rotationDegrees = 180)))
        assertTrue(needsConversion(droneLike.copy(bitrate = null)))
    }

    @Test
    fun `a video without AAC sound gets it`() {
        // Drone footage is usually silent; Instagram and Facebook show nothing of such a stream.
        assertTrue(needsConversion(droneLike.copy(audioMime = null)))
        assertTrue(needsConversion(droneLike.copy(audioMime = MediaFormat.MIMETYPE_AUDIO_OPUS)))
    }

    @Test
    fun `the bitrate comes from the file's size and length`() {
        assertEquals(4_000_000L, averageBitrate(sizeBytes = 5_000_000, durationMs = 10_000))
        assertNull(averageBitrate(sizeBytes = null, durationMs = 10_000))
        assertNull(averageBitrate(sizeBytes = 5_000_000, durationMs = null))
        assertNull(averageBitrate(sizeBytes = 5_000_000, durationMs = 0))
    }
}
