package com.streammydrone.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.w3c.dom.Element
import java.io.File
import javax.xml.parsers.DocumentBuilderFactory

/** Every translation has every string, the same placeholders and its language's plural forms. */
class TranslationsTest {
    private val res = File("src/main/res")

    private class Strings(val strings: Map<String, String>, val plurals: Map<String, Map<String, String>>)

    private fun read(folder: String): Strings {
        val document = DocumentBuilderFactory.newInstance().newDocumentBuilder().parse(File(res, "$folder/strings.xml"))
        fun elements(tag: String) = document.getElementsByTagName(tag).let { list -> (0 until list.length).map { list.item(it) as Element } }
        val strings = elements("string")
            .filter { it.getAttribute("translatable") != "false" }
            .associate { it.getAttribute("name") to it.textContent }
        val plurals = elements("plurals").associate { plural ->
            val items = plural.getElementsByTagName("item")
            plural.getAttribute("name") to (0 until items.length).associate { index ->
                val item = items.item(index) as Element
                item.getAttribute("quantity") to item.textContent
            }
        }
        return Strings(strings, plurals)
    }

    private fun placeholders(text: String) = Regex("""%(\d+\$)?[a-z]""").findAll(text).map { it.value }.sorted().toList()

    /** Folder name to the language tag the app and locales_config.xml use. */
    private val translations = mapOf(
        "values-tr" to "tr",
        "values-de" to "de",
        "values-es" to "es",
        "values-fr" to "fr",
        "values-it" to "it",
        "values-pt" to "pt",
        "values-ru" to "ru",
        "values-ar" to "ar",
        "values-hi" to "hi",
        "values-in" to "id",
        "values-ja" to "ja",
        "values-ko" to "ko",
        "values-b+zh+Hans" to "zh-Hans",
        "values-b+zh+Hant" to "zh-Hant",
    )

    /** CLDR plural categories of each language. */
    private val pluralForms = mapOf(
        "tr" to setOf("one", "other"),
        "de" to setOf("one", "other"),
        "hi" to setOf("one", "other"),
        "es" to setOf("one", "many", "other"),
        "fr" to setOf("one", "many", "other"),
        "it" to setOf("one", "many", "other"),
        "pt" to setOf("one", "many", "other"),
        "ru" to setOf("one", "few", "many", "other"),
        "ar" to setOf("zero", "one", "two", "few", "many", "other"),
        "id" to setOf("other"),
        "ja" to setOf("other"),
        "ko" to setOf("other"),
        "zh-Hans" to setOf("other"),
        "zh-Hant" to setOf("other"),
    )

    @Test
    fun `every language folder is listed`() {
        val folders = res.listFiles().orEmpty()
            .filter { it.name.startsWith("values-") && File(it, "strings.xml").exists() }
            .map { it.name }
            .toSet()
        assertEquals(translations.keys, folders)
    }

    @Test
    fun `every translation matches the English strings`() {
        val english = read("values")
        translations.forEach { (folder, language) ->
            val translated = read(folder)
            assertEquals("$folder strings", english.strings.keys, translated.strings.keys)
            assertEquals("$folder plurals", english.plurals.keys, translated.plurals.keys)
            english.strings.forEach { (name, text) ->
                val value = translated.strings.getValue(name)
                assertTrue("$folder $name is empty", value.isNotBlank())
                assertEquals("$folder $name placeholders", placeholders(text), placeholders(value))
            }
            english.plurals.forEach { (name, forms) ->
                val value = translated.plurals.getValue(name)
                assertEquals("$folder $name plural forms", pluralForms.getValue(language), value.keys)
                val wanted = placeholders(forms.getValue("other"))
                value.forEach { (quantity, text) ->
                    assertEquals("$folder $name[$quantity] placeholders", wanted, placeholders(text))
                }
            }
        }
    }

    @Test
    fun `the language picker and Android's settings offer the same languages`() {
        val document = DocumentBuilderFactory.newInstance().newDocumentBuilder().parse(File(res, "xml/locales_config.xml"))
        val locales = document.getElementsByTagName("locale").let { list ->
            (0 until list.length).map { (list.item(it) as Element).getAttribute("android:name") }
        }
        // Android's settings list Indonesian only under its old code, "in".
        val tags = locales.map { if (it == "in") "id" else it }
        assertEquals(APP_LANGUAGES.map { it.tag }, tags)
        assertEquals((translations.values + "en").toSet(), tags.toSet())
    }
}
