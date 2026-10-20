package com.streammydrone.app

import android.content.Context
import android.os.Handler
import android.os.Looper
import android.util.AtomicFile
import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.util.UUID
import java.util.concurrent.Executors
import java.util.concurrent.atomic.AtomicBoolean

// Serializes remote session changes across a service being destroyed/recreated and the editor's
// disconnect action. This never holds up the independent native relay executor.
private val FACEBOOK_SESSION_OPERATIONS = Any()

/**
 * Facebook HTTP work never occupies the relay's command executor. Each start creates a live video
 * and hands its one-off stream URL to [prepare]'s caller; a cancelled preparation never does.
 */
internal class FacebookBroadcastManager(context: Context, private val cleanupFailed: () -> Unit) {
    private val context = context.applicationContext
    private val main = Handler(Looper.getMainLooper())
    private val worker = Executors.newSingleThreadExecutor { task -> Thread(task, "facebook-broadcasts") }
    private val sessions = FacebookSessionStore(context)
    // Main thread only. Each start has a unique identity, including stop/start on the same profile.
    private val pending = mutableMapOf<String, AtomicBoolean>()
    private val owner = UUID.randomUUID().toString()
    @Volatile private var closed = false

    fun recover() {
        worker.execute {
            if (closed) return@execute
            runCatching {
                synchronized(FACEBOOK_SESSION_OPERATIONS) {
                    if (!closed) sessions.load().forEach { session -> finishReportingFailure(session.profileId) }
                }
            }
                .onFailure { main.post { if (!closed) cleanupFailed() } }
        }
    }

    fun prepare(profile: DestinationProfile, title: String, ready: (DestinationCredentials) -> Unit, failed: (UiText) -> Unit) {
        if (closed) return
        val connection = profile.facebook ?: return
        val operation = AtomicBoolean(false)
        pending.put(profile.id, operation)?.set(true)
        worker.execute {
            val result = runCatching {
                synchronized(FACEBOOK_SESSION_OPERATIONS) {
                    if (closed || operation.get()) return@execute
                    finishStoredFacebookSession(context, profile.id)
                    val token = facebookUserToken(context, connection.userId, connection.page)
                    if (closed || operation.get()) return@execute
                    prepareFacebookLiveVideo(FacebookApi(GraphFacebookTransport()), connection, title, token) { videoId ->
                        sessions.put(FacebookSession(profile.id, connection.userId, connection.targetId, connection.page, videoId, owner))
                    }
                }
            }
            main.post {
                if (!closed && pending[profile.id] === operation) {
                    pending.remove(profile.id)
                    result.onSuccess { ready(DestinationCredentials(it.serverUrl, it.key)) }.onFailure { failed(facebookError(it)) }
                }
                // Otherwise end() or close() already queued finishing this live video behind this work.
            }
        }
    }

    fun end(profileIds: List<String>) {
        if (closed) return
        profileIds.forEach { pending.remove(it)?.set(true) }
        worker.execute { profileIds.forEach { finishReportingFailure(it, owner) } }
    }

    fun close() {
        if (closed) return
        closed = true
        pending.values.forEach { it.set(true) }
        pending.clear()
        worker.execute {
            // An older service's delayed close must not finish a newer service's broadcast.
            synchronized(FACEBOOK_SESSION_OPERATIONS) {
                runCatching { sessions.load().filter { it.owner == owner }.map { it.profileId } }.getOrDefault(emptyList())
                    .forEach { finishReportingFailure(it, owner) }
            }
        }
        worker.shutdown()
    }

    private fun finishReportingFailure(profileId: String, expectedOwner: String? = null) {
        runCatching { finishStoredFacebookSession(context, profileId, expectedOwner) }.onFailure {
            main.post { if (!closed) cleanupFailed() }
        }
    }
}

internal fun finishStoredFacebookSession(context: Context, profileId: String, expectedOwner: String? = null): Unit = synchronized(FACEBOOK_SESSION_OPERATIONS) {
    val sessions = FacebookSessionStore(context)
    val session = sessions.load().firstOrNull { it.profileId == profileId } ?: return@synchronized
    if (expectedOwner != null && session.owner != expectedOwner) return@synchronized
    val api = FacebookApi(GraphFacebookTransport())
    val token = api.targetToken(session.targetId, session.page, facebookUserToken(context, session.userId, session.page))
    api.finish(session.liveVideoId, token)
    // A failed request leaves its ID in private storage for the next retry, never a token/key.
    sessions.remove(profileId, session.liveVideoId)
}

internal data class FacebookSession(
    val profileId: String,
    val userId: String,
    val targetId: String,
    val page: Boolean,
    val liveVideoId: String,
    val owner: String,
)

/** Only remote IDs and account identity survive process death; the token stays with the Facebook SDK. */
internal class FacebookSessionStore(context: Context) {
    private val storage = AtomicFile(File(context.noBackupFilesDir, "facebook-sessions-v1.json"))

    fun load(): List<FacebookSession> = synchronized(LOCK) { read() }

    fun put(session: FacebookSession) = synchronized(LOCK) { write(read().filterNot { it.profileId == session.profileId } + session) }

    fun remove(profileId: String, liveVideoId: String) = synchronized(LOCK) {
        write(read().filterNot { it.profileId == profileId && it.liveVideoId == liveVideoId })
    }

    private fun read(): List<FacebookSession> {
        if (!storage.baseFile.exists()) return emptyList()
        val items = storage.openRead().bufferedReader().use { JSONArray(it.readText()) }
        return (0 until items.length()).map { index ->
            val item = items.getJSONObject(index)
            FacebookSession(item.getString("profileId"), item.getString("userId"), item.getString("targetId"),
                item.getBoolean("page"), item.getString("liveVideoId"), item.optString("owner"))
        }
    }

    private fun write(sessions: List<FacebookSession>) {
        val items = JSONArray()
        sessions.forEach { session ->
            items.put(JSONObject().put("profileId", session.profileId).put("userId", session.userId)
                .put("targetId", session.targetId).put("page", session.page)
                .put("liveVideoId", session.liveVideoId).put("owner", session.owner))
        }
        val output = storage.startWrite()
        try {
            output.write(items.toString().toByteArray(Charsets.UTF_8))
            storage.finishWrite(output)
        } catch (error: Exception) {
            storage.failWrite(output)
            throw error
        }
    }

    companion object { private val LOCK = Any() }
}
