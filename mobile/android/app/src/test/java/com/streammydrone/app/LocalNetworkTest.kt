package com.streammydrone.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class LocalNetworkTest {
    @Test
    fun `wi-fi is preferred over hotspot and wired interfaces`() {
        val address = preferredLanAddress(
            listOf(
                "rndis0" to "192.168.42.129",
                "swlan0" to "192.168.43.1",
                "wlan0" to "192.168.1.101",
            ),
        )
        assertEquals(LanAddress("192.168.1.101", LanKind.WIFI), address)
    }

    @Test
    fun `hotspot is used when there is no wi-fi client connection`() {
        val address = preferredLanAddress(
            listOf(
                "rmnet_data0" to "10.12.34.56",
                "swlan0" to "192.168.43.1",
            ),
        )
        assertEquals(LanAddress("192.168.43.1", LanKind.HOTSPOT), address)
    }

    @Test
    fun `mobile data, vpn and 464xlat addresses are never offered`() {
        val address = preferredLanAddress(
            listOf(
                "rmnet_data0" to "10.12.34.56",
                "ccmni1" to "10.0.0.8",
                "v4-rmnet_data0" to "192.0.0.4",
                "tun0" to "10.8.0.2",
            ),
        )
        assertNull(address)
    }

    @Test
    fun `interface names are classified case-insensitively`() {
        assertEquals(LanKind.WIFI, lanKind("WLAN0"))
        assertEquals(LanKind.HOTSPOT, lanKind("ap0"))
        assertEquals(LanKind.HOTSPOT, lanKind("softap0"))
        assertEquals(LanKind.WIRED, lanKind("eth0"))
        assertEquals(LanKind.OTHER, lanKind("bt-pan"))
        assertNull(lanKind("rmnet_data1"))
    }

    @Test
    fun `publish url matches the desktop ingest path`() {
        assertEquals(
            "rtmp://192.168.1.101:1935/drone",
            LanAddress("192.168.1.101", LanKind.WIFI).publishUrl,
        )
    }
}
