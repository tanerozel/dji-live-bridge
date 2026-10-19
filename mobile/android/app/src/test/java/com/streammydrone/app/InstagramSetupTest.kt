package com.streammydrone.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertThrows
import org.junit.Test

class InstagramSetupTest {
    @Test fun `accept Instagram and regional RTMPS hosts without adding a secret to the address`() {
        assertEquals("rtmps://live-upload.instagram.com:443/rtmp", validateInstagramServer(" rtmps://live-upload.instagram.com:443/rtmp/\n"))
        assertEquals("rtmps://edgetee-upload-ist1-2.xx.fbcdn.net:443/rtmp", validateInstagramServer("rtmps://edgetee-upload-ist1-2.xx.fbcdn.net:443/rtmp/"))
    }

    @Test fun `reject account URLs plaintext RTMP lookalike hosts and other platforms`() {
        listOf("https://www.instagram.com/", "rtmp://live-upload.instagram.com/rtmp", "rtmps://instagram.com.evil.example/rtmp",
            "rtmps://evilinstagram.com/rtmp", "rtmps://a.rtmps.youtube.com/live2", "rtmps://user@live-upload.instagram.com/rtmp",
            "rtmps://live-upload.instagram.com:0/rtmp", "rtmps://live-upload.instagram.com:99999/rtmp").forEach {
            assertThrows(InstagramSetupException::class.java) { validateInstagramServer(it) }
        }
    }

    @Test fun `a full secret URL can never be stored as a public server address`() {
        listOf("rtmps://live-upload.instagram.com/rtmp/private-key-123", "rtmps://live-upload.instagram.com/rtmp?key=private-key-123",
            "rtmps://live-upload.instagram.com/rtmp#private-key-123", "rtmps://live-upload.instagram.com/rtmp%2Fprivate-key-123").forEach {
            val failure = assertThrows(InstagramSetupException::class.java) { validateInstagramServer(it) }
            assertFalse(failure.toString().contains("private-key-123"))
        }
    }

    @Test fun `reject masked key placeholders addresses whitespace and oversize input`() {
        listOf("****", "****abcd", "••••••••", "rtmps://live-upload.instagram.com/rtmp", "key with spaces", "key\nnew", "a".repeat(513)).forEach {
            assertThrows(InstagramSetupException::class.java) { validateInstagramKey(it) }
        }
    }

    @Test fun `trim copied key without changing its signed query parameters`() {
        assertEquals("IG_1234?s_sw=0&s_vt=ig", validateInstagramKey(" IG_1234?s_sw=0&s_vt=ig\n"))
    }
    @Test fun `browser discovery filtering cannot hide installed Chrome or Brave fallbacks`() {
        val packages = instagramBrowserPackages(null, emptyList())
        org.junit.Assert.assertTrue("com.android.chrome" in packages)
        org.junit.Assert.assertTrue("com.brave.browser" in packages)
    }

    @Test fun `resume successful browser then default without Instagram or duplicate resolvers`() {
        val packages = instagramBrowserPackages("com.brave.browser", listOf("com.android.chrome", "com.brave.browser", "com.instagram.android", "android"))
        assertEquals(listOf("com.brave.browser", "com.android.chrome"), packages.take(2))
        assertEquals(1, packages.count { it == "com.brave.browser" })
        assertFalse("com.instagram.android" in packages)
        assertFalse("android" in packages)
    }
}
