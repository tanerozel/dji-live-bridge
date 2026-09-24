package com.djilivebridge.android

import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Test

class DestinationProfileValidationTest {
    @Test
    fun `profile name is trimmed`() {
        assertEquals("YouTube Ana Yayın", validateName("  YouTube Ana Yayın  "))
    }

    @Test
    fun `profile name accepts boundary lengths`() {
        assertEquals("a", validateName("a"))

        val maximumLengthName = "a".repeat(64)
        assertEquals(maximumLengthName, validateName(maximumLengthName))
    }

    @Test
    fun `profile name rejects blank overlong and control characters`() {
        listOf(
            "",
            "   ",
            "a".repeat(65),
            "Ana\u0000Yayın",
            "Ana\nYayın",
        ).forEach { invalidName ->
            assertThrows(DestinationProfileException::class.java) {
                validateName(invalidName)
            }
        }
    }

    @Test
    fun `server URL normalizes surrounding whitespace and trailing slashes`() {
        assertEquals(
            "rtmp://live.example.com/app",
            validateServerUrl("  rtmp://live.example.com/app///  "),
        )
        assertEquals(
            "rtmps://live.example.com:443/live/channel",
            validateServerUrl("rtmps://live.example.com:443/live/channel/"),
        )
    }

    @Test
    fun `server URL rejects unsupported or incomplete forms`() {
        listOf(
            "https://live.example.com/app",
            "RTMP://live.example.com/app",
            "rtmp:///app",
            "rtmp://live.example.com",
            "rtmp://user@live.example.com/app",
            "rtmp://live.example.com/live app",
            "rtmp://live.example.com/app?token=value",
            "rtmps://live.example.com/app#fragment",
        ).forEach { invalidUrl ->
            assertThrows(DestinationProfileException::class.java) {
                validateServerUrl(invalidUrl)
            }
        }
    }

    @Test
    fun `stream key accepts boundary lengths`() {
        validateStreamKey("a".repeat(4))
        validateStreamKey("a".repeat(512))
    }

    @Test
    fun `stream key rejects values outside length bounds`() {
        listOf("a".repeat(3), "a".repeat(513)).forEach { invalidKey ->
            assertThrows(DestinationProfileException::class.java) {
                validateStreamKey(invalidKey)
            }
        }
    }

    @Test
    fun `stream key rejects forbidden characters`() {
        listOf(
            "key value",
            "key\tvalue",
            "key\u0000value",
            "anahtarç",
            "key/value",
            "key#value",
        ).forEach { invalidKey ->
            assertThrows(DestinationProfileException::class.java) {
                validateStreamKey(invalidKey)
            }
        }
    }
}
