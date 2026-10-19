package com.streammydrone.app

import org.json.JSONObject
import java.io.IOException
import java.io.ByteArrayOutputStream
import java.net.HttpURLConnection
import java.net.URI
import java.net.URLEncoder
import java.time.Instant

internal enum class YouTubePrivacy(val apiValue: String) {
    PRIVATE("private"), UNLISTED("unlisted"), PUBLIC("public");
}

/** Public metadata only. Tokens and stream keys must never be added to this record. */
data class YouTubeConnection(
    val accountEmail: String,
    val channelId: String,
    val channelTitle: String,
    val streamId: String,
    val title: String,
    val privacy: String,
    val madeForKids: Boolean,
) {
    internal fun toJson(): JSONObject = JSONObject()
        .put("accountEmail", accountEmail).put("channelId", channelId).put("channelTitle", channelTitle)
        .put("streamId", streamId).put("title", title).put("privacy", privacy).put("madeForKids", madeForKids)

    companion object {
        internal fun fromJson(value: JSONObject): YouTubeConnection = YouTubeConnection(
            value.getString("accountEmail"), value.getString("channelId"), value.getString("channelTitle"),
            value.getString("streamId"), value.getString("title"), value.getString("privacy"), value.getBoolean("madeForKids"),
        ).also {
            require(it.accountEmail.isNotBlank() && it.channelId.isNotBlank() && it.streamId.isNotBlank())
            require(it.privacy in YouTubePrivacy.entries.map(YouTubePrivacy::apiValue))
            validateYouTubeTitle(it.title)
        }
    }
}

internal data class YouTubeChannel(val id: String, val title: String)

/** Read-only platform confirmation; the relay's forwarding state cannot supply this. */
internal data class YouTubeBroadcastStatus(val id: String, val lifecycle: String, val privacy: String)

/** Deliberately has no generated toString: the key stays out of diagnostics. */
internal class YouTubeStream(val id: String, val serverUrl: String, val key: String)

internal enum class YouTubeFailure { AUTH, SETUP, CHANNEL, WRONG_CHANNEL, LIVE_DISABLED, QUOTA, NETWORK, API, TITLE }

internal class YouTubeException(val failure: YouTubeFailure) : Exception()

internal fun validateYouTubeTitle(value: String): String = value.trim().also {
    if (it.isEmpty() || it.codePointCount(0, it.length) > 100 || it.any(Char::isISOControl) || '<' in it || '>' in it) {
        throw YouTubeException(YouTubeFailure.TITLE)
    }
}

/** Small injectable boundary, also used to test lifecycle failures without a Google account. */
internal fun interface YouTubeTransport {
    fun request(method: String, path: String, query: Map<String, String>, body: JSONObject?): JSONObject
}

internal class GoogleYouTubeTransport(private val token: String, private val invalidToken: () -> Unit = {}) : YouTubeTransport {
    override fun request(method: String, path: String, query: Map<String, String>, body: JSONObject?): JSONObject {
        // Paths are application constants. Never follow a redirect with the Authorization header.
        val encoded = query.entries.joinToString("&") { (key, value) ->
            "${URLEncoder.encode(key, "UTF-8")}=${URLEncoder.encode(value, "UTF-8")}"
        }
        val connection = URI("https://www.googleapis.com/$path${if (encoded.isEmpty()) "" else "?$encoded"}")
            .toURL().openConnection() as HttpURLConnection
        try {
            connection.requestMethod = method
            connection.instanceFollowRedirects = false
            connection.useCaches = false
            connection.connectTimeout = 15_000
            connection.readTimeout = 15_000
            connection.setRequestProperty("Authorization", "Bearer $token")
            connection.setRequestProperty("Accept", "application/json")
            if (body != null) {
                connection.doOutput = true
                connection.setRequestProperty("Content-Type", "application/json; charset=UTF-8")
                connection.outputStream.use { it.write(body.toString().toByteArray(Charsets.UTF_8)) }
            }
            val code = connection.responseCode
            val stream = if (code in 200..299) connection.inputStream else connection.errorStream
            val raw = stream?.use { input ->
                val bytes = ByteArrayOutputStream()
                val buffer = ByteArray(8_192)
                while (true) {
                    val count = input.read(buffer)
                    if (count < 0) break
                    if (bytes.size() + count > 1_048_576) throw YouTubeException(YouTubeFailure.API)
                    bytes.write(buffer, 0, count)
                }
                bytes.toString("UTF-8")
            }.orEmpty()
            val json = if (raw.isBlank()) JSONObject() else try {
                JSONObject(raw)
            } catch (_: Exception) {
                throw YouTubeException(YouTubeFailure.API)
            }
            if (code !in 200..299) {
                if (code == 401) runCatching(invalidToken)
                val errors = json.optJSONObject("error")?.optJSONArray("errors")
                val reasons = errors?.let { list ->
                    (0 until list.length()).map { list.optJSONObject(it)?.optString("reason").orEmpty() }
                }.orEmpty()
                throw YouTubeException(when {
                    code == 401 -> YouTubeFailure.AUTH
                    reasons.any { it in setOf("accessNotConfigured", "serviceDisabled") } -> YouTubeFailure.SETUP
                    reasons.any { it in setOf("liveStreamingNotEnabled", "livePermissionBlocked", "insufficientLivePermissions") } -> YouTubeFailure.LIVE_DISABLED
                    code == 429 || reasons.any { it in setOf("quotaExceeded", "dailyLimitExceeded", "rateLimitExceeded", "userRequestsExceedRateLimit") } -> YouTubeFailure.QUOTA
                    reasons.any { it in setOf("insufficientPermissions", "forbidden") } -> YouTubeFailure.AUTH
                    else -> YouTubeFailure.API
                })
            }
            return json
        } catch (_: IOException) {
            // Neither the URL, token, response body nor exception text reaches logs or the UI.
            throw YouTubeException(YouTubeFailure.NETWORK)
        } finally {
            connection.disconnect()
        }
    }
}

internal class YouTubeApi(private val transport: YouTubeTransport) {
    fun accountEmail(): String = transport.request("GET", "oauth2/v2/userinfo", emptyMap(), null)
        .getString("email").also { if (it.isBlank()) throw YouTubeException(YouTubeFailure.AUTH) }

    fun channels(): List<YouTubeChannel> {
        val items = transport.request("GET", "youtube/v3/channels", mapOf("part" to "snippet", "mine" to "true"), null)
            .getJSONArray("items")
        return (0 until items.length()).map { index ->
            val item = items.getJSONObject(index)
            YouTubeChannel(item.getString("id"), item.getJSONObject("snippet").getString("title"))
        }.ifEmpty { throw YouTubeException(YouTubeFailure.CHANNEL) }
    }

    fun checkChannel(channelId: String) {
        if (channels().none { it.id == channelId }) throw YouTubeException(YouTubeFailure.WRONG_CHANNEL)
    }

    fun createStream(): YouTubeStream {
        val result = transport.request("POST", "youtube/v3/liveStreams", mapOf("part" to "id,cdn,snippet,contentDetails"),
            JSONObject().put("snippet", JSONObject().put("title", "Stream My Drone"))
                .put("cdn", JSONObject().put("ingestionType", "rtmp").put("frameRate", "variable").put("resolution", "variable"))
                .put("contentDetails", JSONObject().put("isReusable", true)))
        val id = result.getString("id")
        try {
            val ingest = result.getJSONObject("cdn").getJSONObject("ingestionInfo")
            val url = validateServerUrl(ingest.getString("rtmpsIngestionAddress"))
            if (!url.startsWith("rtmps://")) throw YouTubeException(YouTubeFailure.API)
            val key = ingest.getString("streamName").also(::validateStreamKey)
            return YouTubeStream(id, url, key)
        } catch (failure: Exception) {
            runCatching { deleteStream(id) }
            throw failure
        }
    }

    fun deleteStream(id: String) {
        transport.request("DELETE", "youtube/v3/liveStreams", mapOf("id" to id), null)
    }

    fun createBroadcast(connection: YouTubeConnection): String = transport.request(
        "POST", "youtube/v3/liveBroadcasts", mapOf("part" to "id,snippet,status,contentDetails"),
        JSONObject().put("snippet", JSONObject().put("title", validateYouTubeTitle(connection.title))
            .put("scheduledStartTime", Instant.now().plusSeconds(10).toString()))
            .put("status", JSONObject().put("privacyStatus", connection.privacy).put("selfDeclaredMadeForKids", connection.madeForKids))
            .put("contentDetails", JSONObject().put("enableAutoStart", true).put("enableAutoStop", true)
                .put("monitorStream", JSONObject().put("enableMonitorStream", false))),
    ).getString("id")

    fun bind(broadcastId: String, streamId: String) {
        transport.request("POST", "youtube/v3/liveBroadcasts/bind", mapOf("part" to "id", "id" to broadcastId, "streamId" to streamId), null)
    }

    fun broadcastStatus(id: String, channelId: String, streamId: String): YouTubeBroadcastStatus? {
        val items = transport.request("GET", "youtube/v3/liveBroadcasts",
            mapOf("part" to "id,snippet,status,contentDetails", "id" to id), null).getJSONArray("items")
        if (items.length() == 0) return null
        val item = items.getJSONObject(0)
        if (items.length() != 1 || item.getString("id") != id ||
            item.getJSONObject("snippet").getString("channelId") != channelId ||
            item.getJSONObject("contentDetails").optString("boundStreamId") != streamId
        ) throw YouTubeException(YouTubeFailure.WRONG_CHANNEL)
        val status = item.getJSONObject("status")
        return YouTubeBroadcastStatus(id, status.getString("lifeCycleStatus"), status.getString("privacyStatus"))
    }

    /** Idempotent: a missing or already completed broadcast needs no more remote work. */
    fun finish(broadcastId: String) {
        val items = transport.request("GET", "youtube/v3/liveBroadcasts", mapOf("part" to "status", "id" to broadcastId), null)
            .getJSONArray("items")
        if (items.length() == 0) return
        when (items.getJSONObject(0).getJSONObject("status").getString("lifeCycleStatus")) {
            "complete", "revoked" -> return
            "created", "ready" -> transport.request("DELETE", "youtube/v3/liveBroadcasts", mapOf("id" to broadcastId), null)
            else -> transport.request("POST", "youtube/v3/liveBroadcasts/transition",
                mapOf("part" to "id,status", "id" to broadcastId, "broadcastStatus" to "complete"), null)
        }
    }
}

/** Persist the ID before binding, so interruption or a bind failure can be recovered later. */
internal fun prepareYouTubeBroadcast(api: YouTubeApi, connection: YouTubeConnection, created: (String) -> Unit): String {
    api.checkChannel(connection.channelId)
    val id = api.createBroadcast(connection)
    try {
        created(id)
        api.bind(id, connection.streamId)
    } catch (failure: Exception) {
        runCatching { api.finish(id) }
        throw failure
    }
    return id
}
