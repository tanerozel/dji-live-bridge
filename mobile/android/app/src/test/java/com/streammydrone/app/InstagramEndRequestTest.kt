package com.streammydrone.app

import org.junit.Assert.*
import org.junit.Test

class InstagramEndRequestTest {
    @Test fun `end and confirmation are each dispatched at most once`() {
        val request = InstagramEndRequest("instagram", 4, 8_000)
        assertTrue(request.claim("end"))
        assertFalse(request.claim("end"))
        assertTrue(request.claim("confirm"))
        assertFalse(request.claim("confirm"))
        assertFalse(request.claim("end"))
        assertFalse(request.claim("start"))
    }

    @Test fun `already open confirmation can be completed without reopening end`() {
        val request = InstagramEndRequest("instagram", 4, 8_000)
        assertTrue(request.claim("confirm"))
        assertFalse(request.claim("end"))
    }

    @Test fun `deadline document replacement and authentication invalidate end`() {
        val request = InstagramEndRequest("instagram", 4, 8_000)
        assertTrue(request.valid(1, 4, "https://www.instagram.com/"))
        assertTrue(request.valid(2, 4, "https://www.instagram.com/live/producer/"))
        assertFalse(request.valid(8_000, 4, "https://www.instagram.com/"))
        assertFalse(request.valid(1, 5, "https://www.instagram.com/"))
        assertFalse(request.valid(1, 4, "https://www.instagram.com/accounts/login/"))
        assertFalse(request.valid(1, 4, "https://evil.example/"))
    }

    @Test fun `ending other platforms leaves Instagram ingest alive until its own end finishes`() {
        assertEquals(listOf("youtube", "twitch"), relayEndTargets(listOf("youtube", "twitch"), listOf("instagram")))
        assertEquals(listOf("instagram"), relayEndTargets(listOf("instagram"), listOf("youtube")))
        assertEquals(emptyList<String>(), relayEndTargets(emptyList(), emptyList()))
        assertEquals(listOf(""), relayEndTargets(listOf("instagram"), emptyList()))
    }
}
