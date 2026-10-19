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
private val YOUTUBE_SESSION_OPERATIONS = Any()

/**
 * YouTube HTTP work never occupies the relay's command executor. Start/end are serialized here;
 * a cancelled preparation can never attach a destination after the user's End broadcast.
 */
internal class YouTubeBroadcastManager(context: Context, private val cleanupFailed: () -> Unit) {
    private val context = context.applicationContext
    private val main = Handler(Looper.getMainLooper())
    private val worker = Executors.newSingleThreadExecutor { task -> Thread(task, "youtube-broadcasts") }
    private val sessions = YouTubeSessionStore(context)
    // Main thread only. Each start has a unique identity, including stop/start on the same profile.
    private val pending = mutableMapOf<String, AtomicBoolean>()
    private val owner = UUID.randomUUID().toString()
    @Volatile private var closed = false

    fun recover() {
        worker.execute {
            if (closed) return@execute
            runCatching {
                synchronized(YOUTUBE_SESSION_OPERATIONS) {
                    if (!closed) sessions.load().forEach { session -> finishReportingFailure(session.profileId) }
                }
            }
                .onFailure { main.post { if (!closed) cleanupFailed() } }
        }
    }

    fun prepare(profile: DestinationProfile, title: String, ready: () -> Unit, failed: (UiText) -> Unit) {
        if (closed) return
        val connection = profile.youtube?.copy(title = validateBroadcastTitle(title)) ?: return
        val operation = AtomicBoolean(false)
        pending.put(profile.id, operation)?.set(true)
        worker.execute {
            val result = runCatching {
                synchronized(YOUTUBE_SESSION_OPERATIONS) {
                    if (closed || operation.get()) return@execute
                    finishSession(profile.id)
                    val api = authorizedYouTubeApi(context, connection.accountEmail)
                    if (closed || operation.get()) return@execute
                    prepareYouTubeBroadcast(api, connection) { broadcastId ->
                        sessions.put(YouTubeSession(profile.id, connection.accountEmail, connection.channelId, broadcastId, owner))
                    }
                }
            }
            main.post {
                if (!closed && pending[profile.id] === operation) {
                    pending.remove(profile.id)
                    result.onSuccess { ready() }.onFailure { failed(youTubeError(it)) }
                }
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
            synchronized(YOUTUBE_SESSION_OPERATIONS) {
                runCatching { sessions.load().filter { it.owner == owner }.map { it.profileId } }.getOrDefault(emptyList())
                    .forEach { finishReportingFailure(it, owner) }
            }
        }
        worker.shutdown()
    }

    private fun finishReportingFailure(profileId: String, expectedOwner: String? = null) {
        runCatching { finishStoredYouTubeSession(context, profileId, expectedOwner) }.onFailure {
            main.post { if (!closed) cleanupFailed() }
        }
    }

    private fun finishSession(profileId: String) {
        finishStoredYouTubeSession(context, profileId)
    }
}

internal fun finishStoredYouTubeSession(context: Context, profileId: String, expectedOwner: String? = null): Unit = synchronized(YOUTUBE_SESSION_OPERATIONS) {
    val sessions = YouTubeSessionStore(context)
    val session = sessions.load().firstOrNull { it.profileId == profileId } ?: return@synchronized
    if (expectedOwner != null && session.owner != expectedOwner) return@synchronized
    val api = authorizedYouTubeApi(context, session.email)
    api.checkChannel(session.channelId)
    api.finish(session.broadcastId)
    // A failed request leaves its ID in private storage for the next retry, never a token/key.
    sessions.remove(profileId, session.broadcastId)
}

internal data class YouTubeSession(val profileId: String, val email: String, val channelId: String, val broadcastId: String, val owner: String)

/** Only remote IDs and account identity survive process death; secrets remain with Google/Keystore. */
internal class YouTubeSessionStore(context: Context) {
    private val storage = AtomicFile(File(context.noBackupFilesDir, "youtube-sessions-v1.json"))

    fun load(): List<YouTubeSession> = synchronized(LOCK) { read() }

    fun put(session: YouTubeSession) = synchronized(LOCK) { write(read().filterNot { it.profileId == session.profileId } + session) }

    fun remove(profileId: String, broadcastId: String) = synchronized(LOCK) {
        write(read().filterNot { it.profileId == profileId && it.broadcastId == broadcastId })
    }

    private fun read(): List<YouTubeSession> {
        if (!storage.baseFile.exists()) return emptyList()
        val items = storage.openRead().bufferedReader().use { JSONArray(it.readText()) }
        return (0 until items.length()).map { index ->
            val item = items.getJSONObject(index)
            YouTubeSession(item.getString("profileId"), item.getString("email"), item.getString("channelId"), item.getString("broadcastId"), item.optString("owner"))
        }
    }

    private fun write(sessions: List<YouTubeSession>) {
        val items = JSONArray()
        sessions.forEach { session ->
            items.put(JSONObject().put("profileId", session.profileId).put("email", session.email)
                .put("channelId", session.channelId).put("broadcastId", session.broadcastId).put("owner", session.owner))
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
