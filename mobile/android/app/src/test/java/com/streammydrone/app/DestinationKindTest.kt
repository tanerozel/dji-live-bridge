package com.streammydrone.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class DestinationKindTest {
    @Test
    fun `every published default address passes the destination validator unchanged`() {
        DestinationKind.entries.mapNotNull { it.defaultServerUrl }.forEach { url ->
            assertEquals(url, validateServerUrl(url))
        }
    }

    @Test
    fun `platforms that hand out their own address have no default`() {
        assertNull(DestinationKind.TIKTOK.defaultServerUrl)
        assertNull(DestinationKind.CUSTOM.defaultServerUrl)
    }

    @Test
    fun `storage values round trip and unknown values fall back to custom`() {
        DestinationKind.entries.forEach { kind ->
            assertEquals(kind, DestinationKind.fromStorage(kind.storageValue))
        }
        assertEquals(DestinationKind.CUSTOM, DestinationKind.fromStorage("periscope"))
    }

    @Test
    fun `profiles saved by the first release still load as the same platforms`() {
        assertEquals(DestinationKind.TIKTOK, DestinationKind.fromStorage("tiktok"))
        assertEquals(DestinationKind.YOUTUBE, DestinationKind.fromStorage("youtube"))
        assertEquals(DestinationKind.CUSTOM, DestinationKind.fromStorage("custom"))
    }

    @Test
    fun `theme defaults to light like the desktop app`() {
        assertEquals(ThemeChoice.LIGHT, ThemeChoice.fromStorage(null))
        assertEquals(ThemeChoice.LIGHT, ThemeChoice.fromStorage("neon"))
        ThemeChoice.entries.forEach { choice ->
            assertEquals(choice, ThemeChoice.fromStorage(choice.storageValue))
        }
    }
}
