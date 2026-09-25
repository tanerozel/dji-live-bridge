package com.streammydrone.app

import android.app.LocaleManager
import android.content.Context
import android.os.Build
import android.os.LocaleList
import androidx.annotation.RequiresApi
import java.util.Locale

/** A language the app is translated into, named in that language. */
internal data class AppLanguage(val tag: String, val name: String)

/**
 * The desktop app's languages, in its order and with its names. Keep in step with
 * res/xml/locales_config.xml (which lists Indonesian as "in") and the values-* folders.
 */
internal val APP_LANGUAGES = listOf(
    AppLanguage("en", "English"),
    AppLanguage("es", "Español"),
    AppLanguage("zh-Hans", "中文（简体）"),
    AppLanguage("zh-Hant", "中文（繁體）"),
    AppLanguage("ar", "العربية"),
    AppLanguage("hi", "हिन्दी"),
    AppLanguage("pt", "Português"),
    AppLanguage("ru", "Русский"),
    AppLanguage("fr", "Français"),
    AppLanguage("de", "Deutsch"),
    AppLanguage("ja", "日本語"),
    AppLanguage("ko", "한국어"),
    AppLanguage("id", "Bahasa Indonesia"),
    AppLanguage("it", "Italiano"),
    AppLanguage("tr", "Türkçe"),
)

/** The listed language a locale falls under; Chinese goes by script, everything else by language. */
internal fun appLanguageFor(locale: Locale): AppLanguage? {
    val language = when (locale.language) {
        "in" -> "id"
        else -> locale.language
    }
    val tag = if (language == "zh") {
        val traditional = locale.script == "Hant" ||
            (locale.script.isEmpty() && locale.country in setOf("TW", "HK", "MO"))
        if (traditional) "zh-Hant" else "zh-Hans"
    } else {
        language
    }
    return APP_LANGUAGES.firstOrNull { it.tag == tag }
}

/**
 * The per-app language of Android 13 and later: the system stores it, applies it to the whole app
 * (activity, service and notification) and offers the same choice in its settings. Older Android
 * versions use the phone's language.
 */
@RequiresApi(Build.VERSION_CODES.TIRAMISU)
internal object AppLanguageSetting {
    /** The chosen language, or null when the app follows the phone. */
    fun current(context: Context): AppLanguage? {
        val locales = context.getSystemService(LocaleManager::class.java).applicationLocales
        return if (locales.isEmpty) null else appLanguageFor(locales[0])
    }

    fun set(context: Context, language: AppLanguage?) {
        context.getSystemService(LocaleManager::class.java).applicationLocales =
            if (language == null) LocaleList.getEmptyLocaleList() else LocaleList.forLanguageTags(language.tag)
    }
}
