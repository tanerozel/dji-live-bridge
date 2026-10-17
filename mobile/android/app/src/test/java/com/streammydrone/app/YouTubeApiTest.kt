package com.streammydrone.app

import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test

/** Exercises real request construction and compensation without publishing to a real channel. */
class YouTubeApiTest {
    private val connection = YouTubeConnection("owner@example.test", "channel-a", "My channel", "stream-a", "Drone flight", "unlisted", false)

    private class Request(val method: String, val path: String, val query: Map<String, String>, val body: JSONObject?)

    private class FakeTransport(val answer: (Request) -> JSONObject) : YouTubeTransport {
        val requests = mutableListOf<Request>()
        override fun request(method: String, path: String, query: Map<String, String>, body: JSONObject?): JSONObject =
            Request(method, path, query, body).let { request -> requests += request; answer(request) }
    }

    private fun channels(id: String = "channel-a") = JSONObject().put("items", JSONArray().put(
        JSONObject().put("id", id).put("snippet", JSONObject().put("title", "My channel")),
    ))

    private fun broadcast(status: String) = JSONObject().put("items", JSONArray().put(
        JSONObject().put("status", JSONObject().put("lifeCycleStatus", status)),
    ))

    @Test fun `prepare binds a fresh broadcast after recording its recovery ID`() {
        var recorded: String? = null
        val transport = FakeTransport { request ->
            when (request.path) {
                "youtube/v3/channels" -> channels()
                "youtube/v3/liveBroadcasts/bind" -> {
                    assertEquals("broadcast-a", recorded)
                    assertEquals("stream-a", request.query["streamId"])
                    JSONObject()
                }
                else -> JSONObject().put("id", "broadcast-a")
            }
        }
        assertEquals("broadcast-a", prepareYouTubeBroadcast(YouTubeApi(transport), connection) { recorded = it })
        val body = transport.requests[1].body!!
        assertEquals("Drone flight", body.getJSONObject("snippet").getString("title"))
        assertEquals("unlisted", body.getJSONObject("status").getString("privacyStatus"))
        assertFalse(body.getJSONObject("status").getBoolean("selfDeclaredMadeForKids"))
        assertTrue(body.getJSONObject("contentDetails").getBoolean("enableAutoStart"))
        assertTrue(body.getJSONObject("contentDetails").getBoolean("enableAutoStop"))
        assertFalse(body.getJSONObject("contentDetails").getJSONObject("monitorStream").getBoolean("enableMonitorStream"))
    }

    @Test fun `a different authorized channel can never create a broadcast`() {
        val transport = FakeTransport { channels("different-channel") }
        val error = assertThrows(YouTubeException::class.java) {
            prepareYouTubeBroadcast(YouTubeApi(transport), connection) { error("Must not persist a session") }
        }
        assertEquals(YouTubeFailure.WRONG_CHANNEL, error.failure)
        assertEquals(listOf("GET"), transport.requests.map { it.method })
    }

    @Test fun `a bind failure deletes the unstarted remote broadcast`() {
        val transport = FakeTransport { request -> when {
            request.path.endsWith("channels") -> channels()
            request.path.endsWith("bind") -> throw YouTubeException(YouTubeFailure.API)
            request.method == "POST" -> JSONObject().put("id", "broadcast-a")
            request.method == "GET" -> broadcast("ready")
            else -> JSONObject()
        } }
        assertThrows(YouTubeException::class.java) { prepareYouTubeBroadcast(YouTubeApi(transport), connection) {} }
        val cleanup = transport.requests.last()
        assertEquals("DELETE", cleanup.method)
        assertEquals("broadcast-a", cleanup.query["id"])
    }

    @Test fun `a persistence failure also compensates before binding`() {
        val transport = FakeTransport { request -> when {
            request.path.endsWith("channels") -> channels()
            request.method == "POST" -> JSONObject().put("id", "broadcast-a")
            request.method == "GET" -> broadcast("created")
            else -> JSONObject()
        } }
        assertThrows(IllegalStateException::class.java) {
            prepareYouTubeBroadcast(YouTubeApi(transport), connection) { throw IllegalStateException("disk unavailable") }
        }
        assertFalse(transport.requests.any { it.path.endsWith("bind") })
        assertEquals("DELETE", transport.requests.last().method)
    }

    @Test fun `cleanup failure preserves the original preparation error`() {
        val failure = YouTubeException(YouTubeFailure.LIVE_DISABLED)
        val transport = FakeTransport { request -> when {
            request.path.endsWith("channels") -> channels()
            request.path.endsWith("bind") -> throw failure
            request.method == "POST" -> JSONObject().put("id", "broadcast-a")
            else -> throw YouTubeException(YouTubeFailure.NETWORK)
        } }
        assertEquals(failure, assertThrows(YouTubeException::class.java) {
            prepareYouTubeBroadcast(YouTubeApi(transport), connection) {}
        })
    }

    @Test fun `ending a live broadcast transitions it to complete`() {
        val transport = FakeTransport { if (it.method == "GET") broadcast("live") else JSONObject() }
        YouTubeApi(transport).finish("broadcast-a")
        val request = transport.requests.last()
        assertEquals("youtube/v3/liveBroadcasts/transition", request.path)
        assertEquals("complete", request.query["broadcastStatus"])
    }

    @Test fun `ending a broadcast that never received video deletes it`() {
        val transport = FakeTransport { if (it.method == "GET") broadcast("ready") else JSONObject() }
        YouTubeApi(transport).finish("broadcast-a")
        assertEquals("DELETE", transport.requests.last().method)
    }

    @Test fun `ending an already completed or missing broadcast is harmless`() {
        listOf(broadcast("complete"), JSONObject().put("items", JSONArray())).forEach { response ->
            val transport = FakeTransport { response }
            YouTubeApi(transport).finish("broadcast-a")
            assertEquals(1, transport.requests.size)
        }
    }

    @Test fun `stream creation uses encrypted RTMPS and a reusable input`() {
        val transport = FakeTransport { JSONObject().put("id", "stream-a").put("cdn", JSONObject().put("ingestionInfo",
            JSONObject().put("rtmpsIngestionAddress", "rtmps://a.rtmps.youtube.com/live2").put("streamName", "test-stream-key"))) }
        val stream = YouTubeApi(transport).createStream()
        assertEquals("rtmps://a.rtmps.youtube.com/live2", stream.serverUrl)
        assertEquals("test-stream-key", stream.key)
        val body = transport.requests.single().body!!
        assertEquals("rtmp", body.getJSONObject("cdn").getString("ingestionType"))
        assertTrue(body.getJSONObject("contentDetails").getBoolean("isReusable"))
    }

    @Test fun `a malformed ingest response cleans up the allocated stream`() {
        val transport = FakeTransport { request ->
            if (request.method == "DELETE") JSONObject() else JSONObject().put("id", "stream-a").put("cdn", JSONObject())
        }
        assertThrows(Exception::class.java) { YouTubeApi(transport).createStream() }
        assertEquals("DELETE", transport.requests.last().method)
        assertEquals("stream-a", transport.requests.last().query["id"])
    }

    @Test fun `public and child-directed settings are preserved explicitly`() {
        val transport = FakeTransport { JSONObject().put("id", "broadcast-a") }
        YouTubeApi(transport).createBroadcast(connection.copy(privacy = "public", madeForKids = true))
        val status = transport.requests.single().body!!.getJSONObject("status")
        assertEquals("public", status.getString("privacyStatus"))
        assertTrue(status.getBoolean("selfDeclaredMadeForKids"))
    }

    @Test fun `connection metadata roundtrips without credentials`() {
        val value = connection.toJson()
        assertEquals(connection, YouTubeConnection.fromJson(value))
        assertFalse(value.has("streamKey"))
        assertFalse(value.has("accessToken"))
        assertThrows(IllegalArgumentException::class.java) {
            YouTubeConnection.fromJson(connection.copy(privacy = "unexpected").toJson())
        }
    }

    @Test fun `title validation handles blank invalid and overlong Unicode titles`() {
        listOf("", "   ", "a".repeat(101), "line\nbreak", "<title>").forEach { title ->
            assertThrows(YouTubeException::class.java) { validateYouTubeTitle(title) }
        }
        assertEquals("Flight", validateYouTubeTitle(" Flight "))
        assertEquals("🚁".repeat(100), validateYouTubeTitle("🚁".repeat(100)))
    }
}
