package com.djilivebridge.android

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import java.util.Locale

class UiTextTest {
    @Test
    fun `relay core codes become the app's own messages`() {
        assertEquals(uiText(R.string.error_receiver_not_running), nativeError("receiver_not_running"))
        assertEquals(uiText(R.string.error_stream_key), nativeError("invalid_stream_key"))
        assertEquals(uiText(R.string.error_internal), nativeError("state_lock_failed"))
    }

    @Test
    fun `the technical detail follows the message in brackets`() {
        assertEquals(
            uiText(R.string.error_with_detail, uiText(R.string.error_listen_failed), "0.0.0.0:1935: address in use"),
            nativeError("listen_failed: 0.0.0.0:1935: address in use"),
        )
    }

    @Test
    fun `an unknown message is shown as it came`() {
        assertEquals(UiText.Raw("something new"), nativeError("something new"))
    }

    @Test
    fun `a failure keeps its cause only when it has one`() {
        assertEquals(uiText(R.string.error_service_start), withCause(R.string.error_service_start, IllegalStateException()))
        assertEquals(
            uiText(R.string.error_with_detail, uiText(R.string.error_service_start), "not allowed"),
            withCause(R.string.error_service_start, IllegalStateException("not allowed")),
        )
    }

    @Test
    fun `phone locales map to the app's languages`() {
        fun tag(languageTag: String) = appLanguageFor(Locale.forLanguageTag(languageTag))?.tag
        assertEquals("tr", tag("tr-TR"))
        assertEquals("pt", tag("pt-BR"))
        assertEquals("id", tag("id-ID"))
        assertEquals("zh-Hans", tag("zh-CN"))
        assertEquals("zh-Hans", tag("zh-Hans-SG"))
        assertEquals("zh-Hant", tag("zh-TW"))
        assertEquals("zh-Hant", tag("zh-HK"))
        assertEquals("zh-Hant", tag("zh-Hant"))
        assertNull(tag("sv-SE"))
    }
}
