package com.streammydrone.app

import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test

/** Exercises real request construction and compensation without publishing to a real account. */
class FacebookApiTest {
    private val profile = FacebookConnection("user-a", "Ayşe", "user-a", "Ayşe", false, "Drone flight", "SELF")
    private val page = FacebookConnection("user-a", "Ayşe", "page-a", "Farm Page", true, "Drone flight", "EVERYONE")
    private val streamUrl = "rtmps://live-api-s.facebook.com:443/rtmp/FB-123-0-AbC?s_bl=1&s_psm=1&s_sw=0&s_vt=api-s&a=XyZ"

    private class Request(val method: String, val path: String, val params: Map<String, String>, val token: String)

    private class FakeTransport(val answer: (Request) -> JSONObject) : FacebookTransport {
        val requests = mutableListOf<Request>()
        override fun request(method: String, path: String, params: Map<String, String>, token: String): JSONObject =
            Request(method, path, params, token).let { request -> requests += request; answer(request) }
    }

    private fun me(id: String = "user-a") = JSONObject().put("id", id).put("name", "Ayşe")

    private fun created(url: String = streamUrl) = JSONObject().put("id", "video-a").put("secure_stream_url", url)

    private fun status(value: String) = JSONObject().put("id", "video-a").put("status", value)

    private fun graphError(code: Int, subcode: Int = 0): FacebookLiveException =
        FacebookLiveException(facebookFailure(JSONObject().put("code", code).put("error_subcode", subcode)))

    @Test fun `prepare creates a profile live video and records its ID before handing out the stream`() {
        var recorded: String? = null
        val transport = FakeTransport { request ->
            when (request.path) {
                "me" -> me()
                "user-a/live_videos" -> created()
                else -> error("Unexpected ${request.path}")
            }
        }
        val video = prepareFacebookLiveVideo(FacebookApi(transport), profile, "Morning flight", "user-token") { recorded = it }
        assertEquals("video-a", recorded)
        assertEquals("rtmps://live-api-s.facebook.com:443/rtmp", video.serverUrl)
        assertEquals("FB-123-0-AbC?s_bl=1&s_psm=1&s_sw=0&s_vt=api-s&a=XyZ", video.key)
        val create = transport.requests.single { it.method == "POST" }
        assertEquals("user-token", create.token)
        assertEquals("LIVE_NOW", create.params["status"])
        assertEquals("Morning flight", create.params["title"])
        assertEquals("SELF", JSONObject(create.params.getValue("privacy")).getString("value"))
    }

    @Test fun `a Page live video uses the Page token and never sends a profile audience`() {
        val transport = FakeTransport { request ->
            when (request.path) {
                "me" -> me()
                "page-a" -> JSONObject().put("access_token", "page-token")
                "page-a/live_videos" -> created()
                else -> error("Unexpected ${request.path}")
            }
        }
        prepareFacebookLiveVideo(FacebookApi(transport), page, "Drone flight", "user-token") { }
        val create = transport.requests.single { it.method == "POST" }
        assertEquals("page-token", create.token)
        assertFalse("privacy" in create.params)
        assertEquals("user-token", transport.requests.single { it.path == "page-a" }.token)
    }

    @Test fun `another signed-in Facebook user can never create a live video`() {
        val transport = FakeTransport { me("user-b") }
        val error = assertThrows(FacebookLiveException::class.java) {
            prepareFacebookLiveVideo(FacebookApi(transport), profile, "Drone flight", "user-token") { error("Must not persist a session") }
        }
        assertEquals(FacebookFailure.WRONG_ACCOUNT, error.failure)
        assertEquals(listOf("GET"), transport.requests.map { it.method })
    }

    @Test fun `a lost Page reports the Page, not a broken login`() {
        val transport = FakeTransport { request ->
            if (request.path == "me") me() else throw graphError(100, 33)
        }
        val error = assertThrows(FacebookLiveException::class.java) {
            prepareFacebookLiveVideo(FacebookApi(transport), page, "Drone flight", "user-token") { }
        }
        assertEquals(FacebookFailure.PAGE, error.failure)
    }

    @Test fun `an unusable stream URL deletes the live video nobody saw`() {
        val transport = FakeTransport { request ->
            when {
                request.path == "me" -> me()
                request.method == "POST" -> created("rtmp://insecure.example/rtmp/key")
                request.method == "GET" -> status("UNPUBLISHED")
                else -> JSONObject().put("success", true)
            }
        }
        val error = assertThrows(FacebookLiveException::class.java) {
            prepareFacebookLiveVideo(FacebookApi(transport), profile, "Drone flight", "user-token") { error("Must not persist a session") }
        }
        assertEquals(FacebookFailure.API, error.failure)
        assertEquals("video-a", transport.requests.single { it.method == "DELETE" }.path)
    }

    @Test fun `failing to record the ID ends the live video instead of leaving it behind`() {
        val transport = FakeTransport { request ->
            when {
                request.path == "me" -> me()
                request.method == "POST" && request.path.endsWith("live_videos") -> created()
                request.method == "GET" -> status("LIVE")
                else -> JSONObject().put("success", true)
            }
        }
        assertThrows(IllegalStateException::class.java) {
            prepareFacebookLiveVideo(FacebookApi(transport), profile, "Drone flight", "user-token") { error("disk full") }
        }
        val end = transport.requests.last()
        assertEquals("POST", end.method)
        assertEquals("video-a", end.path)
        assertEquals("true", end.params["end_live_video"])
    }

    @Test fun `finish is idempotent for ended and missing live videos`() {
        for (ended in listOf("LIVE_STOPPED", "VOD", "PROCESSING")) {
            val transport = FakeTransport { status(ended) }
            FacebookApi(transport).finish("video-a", "token")
            assertEquals(listOf("GET"), transport.requests.map { it.method })
        }
        val missing = FakeTransport { throw graphError(100, 33) }
        FacebookApi(missing).finish("video-a", "token")
        assertEquals(1, missing.requests.size)
    }

    @Test fun `status reads only, and names the live video's Facebook page`() {
        val transport = FakeTransport { status("LIVE").put("permalink_url", "/user-a/videos/video-a/") }
        val result = FacebookApi(transport).status("video-a", "token")!!
        assertEquals("LIVE", result.status)
        assertEquals("https://www.facebook.com/user-a/videos/video-a/", result.permalink)
        assertEquals(listOf("GET"), transport.requests.map { it.method })
    }

    @Test fun `only facebook addresses are ever opened`() {
        assertEquals("https://m.facebook.com/x", facebookPermalink("https://m.facebook.com/x"))
        assertNull(facebookPermalink("https://facebook.com.evil.example/x"))
        assertNull(facebookPermalink("http://www.facebook.com/x"))
        assertNull(facebookPermalink(""))
    }

    @Test fun `graph errors map to what the user can do about them`() {
        assertEquals(FacebookFailure.LIVE_DISABLED, graphError(200, 1363144).failure)
        assertEquals(FacebookFailure.LIVE_DISABLED, graphError(200, 1363120).failure)
        assertEquals(FacebookFailure.AUTH, graphError(190).failure)
        assertEquals(FacebookFailure.AUTH, graphError(200).failure)
        assertEquals(FacebookFailure.RATE, graphError(613).failure)
        assertEquals(FacebookFailure.RATE, graphError(4).failure)
        assertEquals(FacebookFailure.NOT_FOUND, graphError(100, 33).failure)
        assertEquals(FacebookFailure.API, graphError(368).failure)
    }

    @Test fun `stream URLs split into the relay's server and key`() {
        val (server, key) = splitFacebookStreamUrl(streamUrl)
        assertEquals("rtmps://live-api-s.facebook.com:443/rtmp", server)
        assertTrue(key.startsWith("FB-123-0-AbC"))
        assertThrows(FacebookLiveException::class.java) { splitFacebookStreamUrl("rtmps://live-api-s.facebook.com:443/live/key") }
        assertThrows(FacebookLiveException::class.java) { splitFacebookStreamUrl("rtmps://live-api-s.facebook.com:443/rtmp/") }
    }

    @Test fun `saved connections round-trip and reject a profile aimed at someone else`() {
        assertEquals(page, FacebookConnection.fromJson(page.toJson()))
        assertEquals(profile, FacebookConnection.fromJson(profile.toJson()))
        assertThrows(IllegalArgumentException::class.java) {
            FacebookConnection.fromJson(profile.copy(targetId = "user-b").toJson())
        }
        assertThrows(IllegalArgumentException::class.java) {
            FacebookConnection.fromJson(profile.copy(privacy = "CUSTOM").toJson())
        }
    }
}
