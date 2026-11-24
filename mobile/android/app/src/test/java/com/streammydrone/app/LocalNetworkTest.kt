package com.streammydrone.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
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

    @Test
    fun `bands follow the frequency`() {
        assertEquals(2, wifiBand(2_437))
        assertEquals(5, wifiBand(5_500))
        assertEquals(6, wifiBand(6_115))
    }

    @Test
    fun `two band switches in five minutes count as roaming`() {
        val watch = WifiWatch()
        watch.add(0, WifiLink(-60, 5_500, 400))
        watch.add(2_000, WifiLink(-55, 2_437, 72))
        assertFalse(watch.isRoaming())
        // The moment of the switch reports no link; it is skipped.
        watch.add(60_000, null)
        watch.add(62_000, WifiLink(-66, 5_500, 300))
        assertTrue(watch.isRoaming())
        // Five quiet minutes later the old switches no longer count.
        watch.add(62_000 + 5 * 60_000 + 1, WifiLink(-64, 5_500, 300))
        assertFalse(watch.isRoaming())
    }

    @Test
    fun `a signal is weak only when it stayed weak`() {
        val watch = WifiWatch()
        watch.add(0, WifiLink(-74, 5_500, 100))
        // One sample is not enough to judge.
        assertNull(watch.weakSignalDbm(0))
        (2_000L..30_000L step 2_000).forEach { watch.add(it, WifiLink(-75, 5_500, 100)) }
        assertEquals(-75, watch.weakSignalDbm(30_000))
        // One good moment in the last 30 seconds clears it.
        watch.add(32_000, WifiLink(-58, 5_500, 400))
        assertNull(watch.weakSignalDbm(32_000))
    }
}
