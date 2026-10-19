package com.streammydrone.app

import org.junit.Assert.*
import org.junit.Test

class InstagramPublishRequestTest {
    private fun request() = InstagramPublishRequest("instagram", 3, "https://www.instagram.com/", 90_000)

    @Test fun `stopped expired or replaced document cancels publication`() {
        val request = request()
        assertTrue(request.valid(1, true, 3, request.url))
        assertFalse(request.valid(1, false, 3, request.url))
        assertFalse(request.valid(90_000, true, 3, request.url))
        assertFalse(request.valid(1, true, 4, request.url))
        assertFalse(request.valid(1, true, 3, "https://www.instagram.com/accounts/login/"))
    }

    @Test fun `claim before javascript prevents duplicate clicks even without callback`() {
        val request = request()
        assertTrue(request.claimClick())
        repeat(10) { assertFalse(request.claimClick()) }
        // Confirmation polling must remain possible after dispatch.
        assertTrue(request.valid(2, true, 3, request.url))
    }

    @Test fun `same document success navigation can confirm but cannot dispatch again`() {
        val request = request()
        val liveUrl = "https://www.instagram.com/live/producer/"
        assertFalse(request.valid(1, true, 3, liveUrl))
        request.claimClick()
        assertTrue(request.valid(2, true, 3, liveUrl))
        assertFalse(request.claimClick())
        assertFalse(request.valid(2, true, 4, liveUrl))
        assertFalse(request.valid(2, true, 3, "https://www.instagram.com/accounts/login/"))
        assertFalse(request.valid(2, true, 3, "https://evil.example/"))
    }
}
