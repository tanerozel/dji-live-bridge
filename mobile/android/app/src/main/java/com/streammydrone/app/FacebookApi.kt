package com.streammydrone.app

import org.json.JSONObject
import java.io.ByteArrayOutputStream
import java.io.IOException
import java.net.HttpURLConnection
import java.net.URI
import java.net.URLEncoder

internal const val FACEBOOK_GRAPH_VERSION = "v26.0"

/** Who can watch a broadcast on the user's own profile. Page broadcasts are always public. */
internal enum class FacebookPrivacy(val apiValue: String) {
    SELF("SELF"), ALL_FRIENDS("ALL_FRIENDS"), EVERYONE("EVERYONE");
}

/**
 * Public metadata only: where broadcasts go and how they are titled. Facebook gives every live
 * video its own stream URL, so no key is stored; the access token stays with the Facebook SDK.
 */
data class FacebookConnection(
    val userId: String,
    val userName: String,
    /** The user's own ID for the profile, otherwise a Page ID. */
    val targetId: String,
    val targetName: String,
    val page: Boolean,
    val title: String,
    val privacy: String,
) {
    internal fun toJson(): JSONObject = JSONObject()
        .put("userId", userId).put("userName", userName).put("targetId", targetId).put("targetName", targetName)
        .put("page", page).put("title", title).put("privacy", privacy)

    companion object {
        internal fun fromJson(value: JSONObject): FacebookConnection = FacebookConnection(
            value.getString("userId"), value.getString("userName"), value.getString("targetId"),
            value.getString("targetName"), value.getBoolean("page"), value.getString("title"), value.getString("privacy"),
        ).also {
            require(it.userId.isNotBlank() && it.targetId.isNotBlank())
            require(it.page || it.targetId == it.userId)
            require(it.privacy in FacebookPrivacy.entries.map(FacebookPrivacy::apiValue))
            validateBroadcastTitle(it.title)
        }
    }
}

internal data class FacebookUser(val id: String, val name: String)

internal data class FacebookPage(val id: String, val name: String)

/** Deliberately has no generated toString: the key stays out of diagnostics. */
internal class FacebookLiveVideo(val id: String, val serverUrl: String, val key: String)

/** Read-only platform confirmation; the relay's forwarding state cannot supply this. */
internal data class FacebookLiveStatus(val id: String, val status: String, val permalink: String?)

internal enum class FacebookFailure { AUTH, SETUP, WRONG_ACCOUNT, PAGE, LIVE_DISABLED, RATE, NETWORK, NOT_FOUND, API }

internal class FacebookLiveException(val failure: FacebookFailure) : Exception()

/** Small injectable boundary, also used to test lifecycle failures without a Facebook account. */
internal fun interface FacebookTransport {
    fun request(method: String, path: String, params: Map<String, String>, token: String): JSONObject
}

internal class GraphFacebookTransport : FacebookTransport {
    override fun request(method: String, path: String, params: Map<String, String>, token: String): JSONObject {
        // Paths are built from IDs Facebook returned. Never follow a redirect with the token.
        val encoded = params.entries.joinToString("&") { (key, value) ->
            "${URLEncoder.encode(key, "UTF-8")}=${URLEncoder.encode(value, "UTF-8")}"
        }
        val query = if (method == "GET" && encoded.isNotEmpty()) "?$encoded" else ""
        val connection = URI("https://graph.facebook.com/$FACEBOOK_GRAPH_VERSION/$path$query")
            .toURL().openConnection() as HttpURLConnection
        try {
            connection.requestMethod = method
            connection.instanceFollowRedirects = false
            connection.useCaches = false
            connection.connectTimeout = 15_000
            connection.readTimeout = 15_000
            connection.setRequestProperty("Authorization", "Bearer $token")
            connection.setRequestProperty("Accept", "application/json")
            if (method != "GET" && encoded.isNotEmpty()) {
                connection.doOutput = true
                connection.setRequestProperty("Content-Type", "application/x-www-form-urlencoded; charset=UTF-8")
                connection.outputStream.use { it.write(encoded.toByteArray(Charsets.UTF_8)) }
            }
            val code = connection.responseCode
            val stream = if (code in 200..299) connection.inputStream else connection.errorStream
            val raw = stream?.use { input ->
                val bytes = ByteArrayOutputStream()
                val buffer = ByteArray(8_192)
                while (true) {
                    val count = input.read(buffer)
                    if (count < 0) break
                    if (bytes.size() + count > 1_048_576) throw FacebookLiveException(FacebookFailure.API)
                    bytes.write(buffer, 0, count)
                }
                bytes.toString("UTF-8")
            }.orEmpty()
            val json = if (raw.isBlank()) JSONObject() else try {
                // DELETE and some POSTs answer {"success":true}; anything else must be an object too.
                JSONObject(raw)
            } catch (_: Exception) {
                throw FacebookLiveException(FacebookFailure.API)
            }
            json.optJSONObject("error")?.let { throw FacebookLiveException(facebookFailure(it)) }
            if (code !in 200..299) throw FacebookLiveException(if (code == 401) FacebookFailure.AUTH else FacebookFailure.API)
            return json
        } catch (_: IOException) {
            // Neither the URL, token, response body nor exception text reaches logs or the UI.
            throw FacebookLiveException(FacebookFailure.NETWORK)
        } finally {
            connection.disconnect()
        }
    }
}

/** Graph API error codes: https://developers.facebook.com/docs/graph-api/guides/error-handling */
internal fun facebookFailure(error: JSONObject): FacebookFailure {
    val code = error.optInt("code")
    val subcode = error.optInt("error_subcode")
    return when {
        // 1363120: account younger than 60 days; 1363144: fewer than 100 followers.
        subcode == 1363120 || subcode == 1363144 -> FacebookFailure.LIVE_DISABLED
        code == 190 || code == 102 || code == 2500 -> FacebookFailure.AUTH
        code == 10 || code in 200..299 -> FacebookFailure.AUTH
        code == 4 || code == 17 || code == 32 || code == 613 || code in 80_000..80_014 -> FacebookFailure.RATE
        // "Unsupported get/post request": the object is gone or not visible to this token.
        code == 100 && subcode == 33 -> FacebookFailure.NOT_FOUND
        code == 803 -> FacebookFailure.NOT_FOUND
        else -> FacebookFailure.API
    }
}

/** Splits Facebook's `rtmps://host:443/rtmp/<key>` into the relay's server and key. */
internal fun splitFacebookStreamUrl(url: String): Pair<String, String> {
    if (!url.startsWith("rtmps://")) throw FacebookLiveException(FacebookFailure.API)
    val marker = url.indexOf("/rtmp/", "rtmps://".length)
    if (marker < 0) throw FacebookLiveException(FacebookFailure.API)
    val server = try {
        validateServerUrl(url.substring(0, marker + "/rtmp".length))
    } catch (_: DestinationProfileException) {
        throw FacebookLiveException(FacebookFailure.API)
    }
    val key = url.substring(marker + "/rtmp/".length)
    try {
        validateStreamKey(key)
    } catch (_: DestinationProfileException) {
        throw FacebookLiveException(FacebookFailure.API)
    }
    return server to key
}

/** Facebook's permalink may be relative; only facebook.com addresses are ever opened. */
internal fun facebookPermalink(value: String?): String? {
    val raw = value?.trim()?.takeIf(String::isNotEmpty) ?: return null
    val url = if (raw.startsWith("/")) "https://www.facebook.com$raw" else raw
    val host = runCatching { URI(url).takeIf { it.scheme == "https" }?.host }.getOrNull() ?: return null
    return url.takeIf { host == "facebook.com" || host.endsWith(".facebook.com") }
}

internal class FacebookApi(private val transport: FacebookTransport) {
    fun me(token: String): FacebookUser {
        val result = transport.request("GET", "me", mapOf("fields" to "id,name"), token)
        val id = result.optString("id").takeIf(String::isNotBlank) ?: throw FacebookLiveException(FacebookFailure.AUTH)
        return FacebookUser(id, result.optString("name").ifBlank { id })
    }

    /** Pages the user may publish to. A user without Pages simply gets an empty list. */
    fun pages(token: String): List<FacebookPage> {
        val items = transport.request("GET", "me/accounts", mapOf("fields" to "id,name,tasks", "limit" to "100"), token)
            .optJSONArray("data") ?: return emptyList()
        return (0 until items.length()).mapNotNull { index ->
            val item = items.optJSONObject(index) ?: return@mapNotNull null
            val tasks = item.optJSONArray("tasks")
            val canPost = tasks == null || (0 until tasks.length()).any { tasks.optString(it) == "CREATE_CONTENT" }
            val id = item.optString("id")
            if (!canPost || id.isBlank()) null else FacebookPage(id, item.optString("name").ifBlank { id })
        }
    }

    /** The same Facebook user must still be signed in; never create a video for someone else. */
    fun checkAccount(userId: String, token: String) {
        if (me(token).id != userId) throw FacebookLiveException(FacebookFailure.WRONG_ACCOUNT)
    }

    /** The token that may publish to [targetId]: the user's own, or the Page's. */
    fun targetToken(targetId: String, page: Boolean, userToken: String): String {
        if (!page) return userToken
        val result = try {
            transport.request("GET", targetId, mapOf("fields" to "access_token"), userToken)
        } catch (failure: FacebookLiveException) {
            throw if (failure.failure in setOf(FacebookFailure.NOT_FOUND, FacebookFailure.AUTH)) FacebookLiveException(FacebookFailure.PAGE) else failure
        }
        return result.optString("access_token").takeIf(String::isNotBlank) ?: throw FacebookLiveException(FacebookFailure.PAGE)
    }

    fun create(connection: FacebookConnection, title: String, token: String): FacebookLiveVideo {
        val params = mutableMapOf(
            "status" to "LIVE_NOW",
            "title" to validateBroadcastTitle(title),
            "description" to validateBroadcastTitle(title),
            "fields" to "id,secure_stream_url",
        )
        if (!connection.page) params["privacy"] = JSONObject().put("value", connection.privacy).toString()
        val result = transport.request("POST", "${connection.targetId}/live_videos", params, token)
        val id = result.optString("id").takeIf(String::isNotBlank) ?: throw FacebookLiveException(FacebookFailure.API)
        try {
            val (server, key) = splitFacebookStreamUrl(result.optString("secure_stream_url"))
            return FacebookLiveVideo(id, server, key)
        } catch (failure: Exception) {
            runCatching { finish(id, token) }
            throw failure
        }
    }

    fun status(id: String, token: String): FacebookLiveStatus? = try {
        val result = transport.request("GET", id, mapOf("fields" to "id,status,permalink_url"), token)
        if (result.optString("id") != id) throw FacebookLiveException(FacebookFailure.API)
        FacebookLiveStatus(id, result.optString("status"), facebookPermalink(result.optString("permalink_url")))
    } catch (failure: FacebookLiveException) {
        if (failure.failure == FacebookFailure.NOT_FOUND) null else throw failure
    }

    /** The permissions the user actually granted; a business login does not list them in its redirect. */
    fun grantedPermissions(token: String): List<String> {
        val items = transport.request("GET", "me/permissions", emptyMap(), token).optJSONArray("data") ?: return emptyList()
        return (0 until items.length()).mapNotNull { index ->
            items.optJSONObject(index)?.takeIf { it.optString("status") == "granted" }?.optString("permission")?.takeIf(String::isNotEmpty)
        }
    }

    /** Removes every permission this app holds, which also ends its access to the account. */
    fun revokePermissions(token: String) {
        transport.request("DELETE", "me/permissions", emptyMap(), token)
    }

    /** Idempotent: a missing or already ended live video needs no more remote work. */
    fun finish(id: String, token: String) {
        val status = status(id, token)?.status ?: return
        when (status) {
            "LIVE_STOPPED", "VOD", "PROCESSING", "SCHEDULED_CANCELED", "SCHEDULED_EXPIRED" -> return
            // Never shown to anyone: nothing to keep.
            "UNPUBLISHED", "SCHEDULED_UNPUBLISHED" -> transport.request("DELETE", id, emptyMap(), token)
            else -> transport.request("POST", id, mapOf("end_live_video" to "true"), token)
        }
    }
}

/** Persist the ID before handing out the stream, so interruption can be recovered later. */
internal fun prepareFacebookLiveVideo(
    api: FacebookApi,
    connection: FacebookConnection,
    title: String,
    userToken: String,
    created: (String) -> Unit,
): FacebookLiveVideo {
    api.checkAccount(connection.userId, userToken)
    val token = api.targetToken(connection.targetId, connection.page, userToken)
    val video = api.create(connection, title, token)
    try {
        created(video.id)
    } catch (failure: Exception) {
        runCatching { api.finish(video.id, token) }
        throw failure
    }
    return video
}
