package com.streammydrone.app

import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test

class InstagramCaptureTest {
    @Test fun `closing setup cancels queued forwarding while ordinary edits never queue it`() {
        val model = ProfileEditorViewModel()
        model.open(null, DestinationKind.INSTAGRAM)
        val pending = model.state!!
        assertNull(pending.instagramStartRequest)
        pending.instagramStartRequest = BroadcastStartRequest(listOf("instagram", "youtube"), "My flight")
        model.close()
        assertNull(pending.instagramStartRequest)
        assertNull(model.state)
    }

    private fun result(server: String = "rtmps://live-upload.instagram.com:443/rtmp", key: String = "IG_fixture-key-1234", origin: String = "https://www.instagram.com"): String =
        JSONObject.quote(JSONObject().put("status", "ready").put("origin", origin).put("serverUrl", server).put("streamKey", key).toString())

    @Test fun `decode Javascript result without permitting another origin or full secret URL`() {
        assertEquals("IG_fixture-key-1234", decodeInstagramCapture(result())?.streamKey)
        assertNull(decodeInstagramCapture(result(origin = "https://evil.example")))
        assertNull(decodeInstagramCapture(result(server = "rtmps://live-upload.instagram.com/rtmp/secret-key")))
        assertNull(decodeInstagramCapture(result(key = "****abcd")))
    }
    @Test fun `title verification metadata survives the native capture boundary`() {
        val json = JSONObject().put("status", "ready").put("origin", "https://www.instagram.com")
            .put("serverUrl", "rtmps://live-upload.instagram.com:443/rtmp").put("streamKey", "IG_fixture-key-1234")
            .put("titleMatches", true).put("audienceMatches", true)
        assertTrue(decodeInstagramCapture(JSONObject.quote(json.toString()))!!.titleMatches)
        assertTrue(decodeInstagramCapture(JSONObject.quote(json.toString()))!!.audienceMatches)
        json.put("titleMatches", false).put("audienceMatches", false)
        assertFalse(decodeInstagramCapture(JSONObject.quote(json.toString()))!!.titleMatches)
        assertFalse(decodeInstagramCapture(JSONObject.quote(json.toString()))!!.audienceMatches)
    }

    @Test fun `malformed oversized or waiting results carry no credentials`() {
        listOf(null, "null", "{}", JSONObject.quote("{}"), JSONObject.quote("{\"status\":\"waiting\"}"), "x".repeat(8193)).forEach {
            assertNull(decodeInstagramCapture(it))
        }
    }
    @Test fun `only exact HTTPS Instagram capture origin outside login and challenge is accepted`() {
        assertTrue(instagramCaptureOrigin("https://www.instagram.com/"))
        assertTrue(instagramCaptureOrigin("https://www.instagram.com/live/producer/"))
        listOf("http://www.instagram.com/", "https://www.instagram.com.evil.example/", "https://user@www.instagram.com/",
            "https://www.instagram.com/accounts/login/", "https://www.instagram.com/challenge/123", "https://www.instagram.com:8443/").forEach {
            assertFalse(instagramCaptureOrigin(it))
        }
    }
    @Test fun `navigation cannot leave approved HTTPS login hosts or launch external app schemes`() {
        assertTrue(instagramNavigationAllowed("https://www.instagram.com/accounts/login/"))
        assertTrue(instagramNavigationAllowed("https://www.facebook.com/login/"))
        listOf("intent://instagram", "file:///data/data/app", "javascript:alert(1)", "http://www.instagram.com/", "https://evil.example/", "https://www.instagram.com.evil.example/").forEach {
            assertFalse(instagramNavigationAllowed(it))
        }
    }
}
