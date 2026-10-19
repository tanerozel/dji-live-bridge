package com.streammydrone.app

import java.net.URI

/** The assisted paste path never mistakes an account/login URL or a full secret URL for a server. */
internal fun validateInstagramServer(value: String): String {
    val server = value.trim().trimEnd('/')
    val uri = try { URI(server) } catch (_: Exception) { throw InstagramSetupException(R.string.instagram_error_server) }
    val host = uri.host?.lowercase().orEmpty()
    val instagramHost = host == "instagram.com" || host.endsWith(".instagram.com") || host.endsWith(".fbcdn.net")
    if (uri.scheme != "rtmps" || !instagramHost || uri.rawUserInfo != null || uri.rawQuery != null ||
        uri.rawFragment != null || uri.rawPath != "/rtmp" || uri.port !in -1..65535 || uri.port == 0
    ) throw InstagramSetupException(R.string.instagram_error_server)
    return server
}

internal fun validateInstagramKey(value: String): String {
    val key = value.trim()
    try { validateStreamKey(key) } catch (_: DestinationProfileException) {
        throw InstagramSetupException(R.string.instagram_error_key)
    }
    if ('*' in key || key.none(Char::isLetterOrDigit)) throw InstagramSetupException(R.string.instagram_error_key)
    return key
}

/** No raw input in exception messages or diagnostics. */
internal class InstagramSetupException(val messageId: Int) : Exception()

/** Retain the successful browser, prefer Android's default, then handle visibility-filtered apps. */
internal fun instagramBrowserPackages(previous: String?, discovered: List<String>): List<String> =
    (listOfNotNull(previous) + discovered + listOf(
        "com.android.chrome", "com.chrome.beta", "com.brave.browser", "com.sec.android.app.sbrowser",
        "org.mozilla.firefox", "com.microsoft.emmx", "com.opera.browser",
    )).filter { it != "android" && it != "com.android.intentresolver" && it != "com.instagram.android" && it.isNotBlank() }.distinct()
