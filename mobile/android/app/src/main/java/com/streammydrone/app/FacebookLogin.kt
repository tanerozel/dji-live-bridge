package com.streammydrone.app

import android.app.Activity
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Bundle
import android.util.AtomicFile
import android.util.Base64
import androidx.browser.customtabs.CustomTabsIntent
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.core.net.toUri
import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.net.URI
import java.net.URLDecoder
import java.util.UUID

/** The one Facebook login the app holds. Deliberately has no generated toString: the token stays out of logs. */
internal class FacebookLogin(val userId: String, val token: String, val expiresAtMillis: Long?, val scopes: List<String>) {
    fun usable(nowMillis: Long = System.currentTimeMillis()): Boolean = expiresAtMillis?.let { it > nowMillis } ?: true

    internal fun toJson(): JSONObject = JSONObject().put("userId", userId).put("token", token)
        .put("expiresAt", expiresAtMillis ?: JSONObject.NULL).put("scopes", JSONArray(scopes))

    companion object {
        internal fun fromJson(value: JSONObject): FacebookLogin = FacebookLogin(
            value.getString("userId"), value.getString("token"),
            if (value.isNull("expiresAt")) null else value.getLong("expiresAt"),
            value.getJSONArray("scopes").let { list -> List(list.length()) { list.getString(it) } },
        )
    }
}

/** What Facebook's login page sent back to `fb<App ID>://authorize/`. */
internal class FacebookCallback(val token: String, val expiresAtMillis: Long?)

/** Reads the redirect's fragment (or, on errors, its query); refuses a different [state]. */
internal fun parseFacebookCallback(url: String, state: String, nowMillis: Long = System.currentTimeMillis()): FacebookCallback {
    val uri = try { URI(url) } catch (_: Exception) { throw FacebookLiveException(FacebookFailure.AUTH) }
    val values = listOfNotNull(uri.rawQuery, uri.rawFragment).flatMap { it.split('&') }.mapNotNull { pair ->
        val name = pair.substringBefore('=')
        if (name.isEmpty()) null else URLDecoder.decode(name, "UTF-8") to URLDecoder.decode(pair.substringAfter('=', ""), "UTF-8")
    }.toMap()
    if (values["state"] != state || "error" in values) throw FacebookLiveException(FacebookFailure.AUTH)
    val token = values["access_token"]?.takeIf(String::isNotEmpty) ?: throw FacebookLiveException(FacebookFailure.AUTH)
    val seconds = values["expires_in"]?.toLongOrNull() ?: 0
    return FacebookCallback(token, if (seconds > 0) nowMillis + seconds * 1000 else null)
}

/** The login, AES-GCM encrypted with its own Android Keystore key, in no-backup private storage. */
internal class FacebookLoginStore(context: Context) {
    private val storage = AtomicFile(File(context.noBackupFilesDir, "facebook-login-v1.json"))
    private val cipher = AndroidKeystoreProfileCipher("dji-live-bridge-facebook-login-v1", "dji-live-bridge-facebook-login")

    /** Null when there is none, or it can no longer be read (e.g. the key was reset). */
    fun load(): FacebookLogin? = synchronized(LOCK) {
        if (!storage.baseFile.exists()) return null
        runCatching {
            val stored = JSONObject(storage.openRead().bufferedReader().use { it.readText() })
            val secret = EncryptedSecret(Base64.decode(stored.getString("iv"), Base64.NO_WRAP), Base64.decode(stored.getString("ciphertext"), Base64.NO_WRAP))
            FacebookLogin.fromJson(JSONObject(cipher.decrypt("current", secret)))
        }.getOrNull()
    }

    fun save(login: FacebookLogin) = synchronized(LOCK) {
        val secret = cipher.encrypt("current", login.toJson().toString())
        val bytes = JSONObject().put("iv", Base64.encodeToString(secret.iv, Base64.NO_WRAP))
            .put("ciphertext", Base64.encodeToString(secret.ciphertext, Base64.NO_WRAP)).toString().toByteArray(Charsets.UTF_8)
        val output = storage.startWrite()
        try {
            output.write(bytes)
            storage.finishWrite(output)
        } catch (error: Exception) {
            storage.failWrite(output)
            throw error
        }
    }

    fun delete() = synchronized(LOCK) {
        storage.delete()
        runCatching { cipher.deleteKey() }
    }

    companion object { private val LOCK = Any() }
}

/**
 * One login at a time, in process memory: the editor starts it, [FacebookRedirectActivity] hands
 * back the redirect, the editor finishes it. Nothing here survives process death, so a redirect
 * arriving for a lost login is ignored.
 */
internal object FacebookLoginFlow {
    /** The `state` sent with the open login; main thread only. */
    var pendingState: String? = null
        private set
    /** The redirect Facebook sent back, until the editor takes it. */
    var callback by mutableStateOf<String?>(null)
        private set

    /** Opens Facebook's login in a Custom Tab. False when no browser could open it. */
    fun start(context: Context): Boolean {
        if (!facebookConfigured) throw FacebookLiveException(FacebookFailure.SETUP)
        val state = UUID.randomUUID().toString()
        val url = Uri.Builder().scheme("https").authority("www.facebook.com").path("/$FACEBOOK_GRAPH_VERSION/dialog/oauth")
            .appendQueryParameter("client_id", BuildConfig.FACEBOOK_APP_ID)
            .appendQueryParameter("redirect_uri", "fb${BuildConfig.FACEBOOK_APP_ID}://authorize/")
            .appendQueryParameter("response_type", "token")
            .appendQueryParameter("config_id", BuildConfig.FACEBOOK_LOGIN_CONFIG_ID)
            .appendQueryParameter("state", state)
            .appendQueryParameter("display", "touch")
            .build()
        callback = null
        pendingState = state
        return try {
            CustomTabsIntent.Builder().setShowTitle(true).build().launchUrl(context, url)
            true
        } catch (_: Exception) {
            pendingState = null
            false
        }
    }

    fun receive(url: String) {
        if (pendingState != null) callback = url
    }

    /** Ends the open login and returns its state and redirect; the redirect is null when it was abandoned. */
    fun finish(): Pair<String, String?>? {
        val state = pendingState ?: return null
        val url = callback
        pendingState = null
        callback = null
        return state to url
    }
}

/** Receives `fb<App ID>://authorize/` from the Custom Tab and returns to the app, closing the tab. */
class FacebookRedirectActivity : Activity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        intent?.data?.let { FacebookLoginFlow.receive(it.toString()) }
        startActivity(Intent(this, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_CLEAR_TOP or Intent.FLAG_ACTIVITY_SINGLE_TOP))
        finish()
    }
}

/** A broadcast's page in a Custom Tab, which shares the browser's login, above the app. */
internal fun openInApp(context: Context, url: String) {
    val uri = url.toUri()
    if (uri.scheme != "https") throw IllegalArgumentException()
    CustomTabsIntent.Builder().setShowTitle(true).build().launchUrl(context, uri)
}
