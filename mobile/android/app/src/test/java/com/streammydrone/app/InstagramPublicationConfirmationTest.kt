package com.streammydrone.app

import org.junit.Assert.*
import org.junit.Test

class InstagramPublicationConfirmationTest {
    private val profile = DestinationProfile("ig", "Instagram", DestinationKind.INSTAGRAM, "rtmps://live-upload.instagram.com/rtmp")
    private val output = OutputSnapshot("ig", "forwarding", outboundBytes = 123)
    private val confirmation = InstagramPublicationConfirmation("ig", 1234)

    @Test fun `RTMP forwarding alone never means Instagram confirmed live`() {
        assertFalse(instagramPublicationConfirmed(profile, output, null))
        assertTrue(instagramPublicationConfirmed(profile, output, confirmation))
        assertEquals(1234L, confirmation.sinceElapsedMillis)
    }

    @Test fun `a confirmation cannot be reused for another platform or profile`() {
        assertFalse(instagramPublicationConfirmed(profile, output, confirmation.copy(profileId = "old")))
        assertFalse(instagramPublicationConfirmed(profile.copy(kind = DestinationKind.YOUTUBE), output, confirmation))
        assertFalse(instagramPublicationConfirmed(profile, output.copy(id = "other"), confirmation))
    }

    @Test fun `missing interrupted or empty output cannot keep the live badge`() {
        assertFalse(instagramPublicationConfirmed(profile, null, confirmation))
        listOf("reconnecting", "error", "armed", "holding", "stopped").forEach {
            assertFalse(instagramPublicationConfirmed(profile, output.copy(status = it), confirmation))
        }
        assertFalse(instagramPublicationConfirmed(profile, output.copy(outboundBytes = 0), confirmation))
        assertTrue(instagramPublicationConfirmed(profile, output.copy(status = "congested"), confirmation))
    }
}
