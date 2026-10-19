package com.streammydrone.app

import org.junit.Assert.*
import org.junit.Test

class BroadcastStartRequestTest {
    private val connection = YouTubeConnection("owner@example.test", "channel", "Channel", "stream", "Previous flight", "unlisted", false)
    private val profile = DestinationProfile("youtube", "YouTube", DestinationKind.YOUTUBE, "rtmps://a.rtmps.youtube.com/live2", connection)

    @Test fun `last confirmed title wins with existing profile and default as migration fallbacks`() {
        assertEquals("Latest flight", initialBroadcastTitle("Latest flight", listOf(profile)))
        assertEquals("Previous flight", initialBroadcastTitle(null, listOf(profile)))
        assertEquals("Previous flight", initialBroadcastTitle("   ", listOf(profile)))
        assertEquals("Stream My Drone", initialBroadcastTitle(null, emptyList()))
    }

    @Test fun `confirmed start snapshots destinations and title through permission and Instagram setup`() {
        val selected = mutableListOf("youtube", "instagram", "youtube")
        val request = BroadcastStartRequest(selected, "  İstanbul uçuşu  ", InstagramAudience.PUBLIC)
        selected.clear()
        assertEquals(listOf("youtube", "instagram"), request.profileIds)
        assertEquals("İstanbul uçuşu", request.title)
        assertEquals(InstagramAudience.PUBLIC, request.instagramAudience)
    }

    @Test fun `first use or unknown stored audience falls back to Practice while explicit Public is restored`() {
        assertEquals(InstagramAudience.PRACTICE, InstagramAudience.fromStorage(null))
        assertEquals(InstagramAudience.PRACTICE, InstagramAudience.fromStorage("unknown"))
        assertEquals(InstagramAudience.PUBLIC, InstagramAudience.fromStorage("PUBLIC"))
    }

    @Test fun `cleared control and overlong titles cannot start a broadcast`() {
        listOf("", "  ", "a".repeat(101), "flight\nnew line", "<flight>").forEach {
            assertThrows(YouTubeException::class.java) { BroadcastStartRequest(listOf("youtube"), it) }
        }
        assertEquals("🚁".repeat(100), validateBroadcastTitle("🚁".repeat(100)))
    }

    @Test fun `per-broadcast title override preserves account audience and saved default`() {
        val launch = BroadcastStartRequest(listOf(profile.id), "New shared title")
        val prepared = connection.copy(title = launch.title)
        assertEquals("Previous flight", connection.title)
        assertEquals("New shared title", prepared.title)
        assertEquals(connection.accountEmail, prepared.accountEmail)
        assertEquals(connection.channelId, prepared.channelId)
        assertEquals(connection.privacy, prepared.privacy)
        assertEquals(connection.madeForKids, prepared.madeForKids)
    }
}
