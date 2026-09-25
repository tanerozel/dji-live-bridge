package com.streammydrone.app

import android.content.Context
import androidx.annotation.PluralsRes
import androidx.annotation.StringRes
import androidx.compose.runtime.Composable
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalContext

/**
 * Text that is put into words only where it is shown, so messages made by the service or the
 * relay core appear in the language the app runs in. Arguments may themselves be [UiText].
 */
sealed interface UiText {
    data class Res(@StringRes val id: Int, val args: List<Any> = emptyList()) : UiText

    data class Plural(@PluralsRes val id: Int, val count: Int, val args: List<Any> = listOf(count)) : UiText

    /** Already in words, such as a technical detail from the operating system. */
    data class Raw(val text: String) : UiText

    /** Parts shown side by side, such as "drone.mp4 · 00:20 · looping". */
    data class Joined(val parts: List<UiText>, val separator: String = " · ") : UiText

    fun resolve(context: Context): String = when (this) {
        is Res -> context.getString(id, *args.map { it.resolved(context) }.toTypedArray())
        is Plural -> context.resources.getQuantityString(id, count, *args.map { it.resolved(context) }.toTypedArray())
        is Raw -> text
        is Joined -> parts.joinToString(separator) { it.resolve(context) }
    }
}

private fun Any.resolved(context: Context): Any = if (this is UiText) resolve(context) else this

internal fun uiText(@StringRes id: Int, vararg args: Any): UiText = UiText.Res(id, args.toList())

/** A message followed by the technical cause of [error], in brackets, when it has one. */
internal fun withCause(@StringRes message: Int, error: Throwable): UiText =
    error.message?.takeIf { it.isNotBlank() }?.let { uiText(R.string.error_with_detail, uiText(message), it) }
        ?: uiText(message)

@Composable
internal fun UiText.asString(): String {
    // Read so a language change recomposes the text.
    LocalConfiguration.current
    return resolve(LocalContext.current)
}

/**
 * A relay core error, sent as "code" or "code: technical detail", in the app's words; the
 * technical detail (English, from the system) follows in brackets.
 */
internal fun nativeError(raw: String): UiText {
    val code = raw.substringBefore(':').trim()
    val detail = raw.substringAfter(':', missingDelimiterValue = "").trim()
    val message = when (code) {
        "receiver_not_running" -> R.string.error_receiver_not_running
        "listen_failed" -> R.string.error_listen_failed
        "server_create_failed", "receiver_failed" -> R.string.error_receiver_failed
        "thread_failed", "state_lock_failed", "snapshot_failed", "jni_argument" -> R.string.error_internal
        "invalid_destination_scheme" -> R.string.error_destination_scheme
        "invalid_destination_path" -> R.string.error_destination_path
        "invalid_destination_server" -> R.string.error_destination_server
        "invalid_stream_key" -> R.string.error_stream_key
        "missing_ca_bundle" -> R.string.error_ca_bundle
        else -> return UiText.Raw(raw)
    }
    return if (detail.isEmpty()) uiText(message) else uiText(R.string.error_with_detail, uiText(message), detail)
}

/**
 * Upper-cases the first letter of a sentence that may start with a phrase such as "on your custom
 * server", which the translations keep in lower case for use mid-sentence.
 */
@Composable
internal fun String.sentenceStart(): String {
    val locale = LocalConfiguration.current.locales[0]
    return replaceFirstChar { it.titlecase(locale) }
}
