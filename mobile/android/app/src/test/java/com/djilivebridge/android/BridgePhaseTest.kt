package com.djilivebridge.android

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class BridgePhaseTest {
    private fun active(status: String, outputStatus: String = "armed") = RelayServiceUiState(
        isActive = true,
        snapshot = RelaySnapshot(status = status, outputStatus = outputStatus),
    )

    @Test
    fun `stopped relay is idle`() {
        assertEquals(BridgePhase.IDLE, bridgePhase(RelayServiceUiState()))
    }

    @Test
    fun `failed start keeps the setup screen with an error`() {
        val state = RelayServiceUiState(
            isActive = false,
            snapshot = RelaySnapshot(status = "error", detail = "Aktif hedef profili seçilmedi"),
        )
        assertEquals(BridgePhase.START_FAILED, bridgePhase(state))
    }

    @Test
    fun `active states follow the ingest and output pipeline`() {
        assertEquals(BridgePhase.STARTING, bridgePhase(active("starting", "starting")))
        assertEquals(BridgePhase.WAITING_FOR_DRONE, bridgePhase(active("listening")))
        assertEquals(BridgePhase.DRONE_CONNECTED, bridgePhase(active("connected")))
        assertEquals(BridgePhase.CONNECTING_TARGET, bridgePhase(active("publishing", "armed")))
        assertEquals(BridgePhase.CONNECTING_TARGET, bridgePhase(active("publishing", "connecting")))
        assertEquals(BridgePhase.CONNECTING_TARGET, bridgePhase(active("publishing", "ready")))
        assertEquals(BridgePhase.LIVE, bridgePhase(active("publishing", "forwarding")))
    }

    @Test
    fun `target reconnect wins over a still publishing source`() {
        assertEquals(BridgePhase.RECONNECTING, bridgePhase(active("publishing", "reconnecting")))
    }

    @Test
    fun `receiver error wins over every output state`() {
        assertEquals(BridgePhase.RECEIVER_ERROR, bridgePhase(active("error", "reconnecting")))
        assertEquals(BridgePhase.RECEIVER_ERROR, bridgePhase(active("error", "forwarding")))
    }

    @Test
    fun `only phases with a drone stream count as streaming`() {
        val streaming = BridgePhase.entries.filter { it.isStreaming }.toSet()
        assertEquals(
            setOf(BridgePhase.CONNECTING_TARGET, BridgePhase.LIVE, BridgePhase.RECONNECTING),
            streaming,
        )
        assertFalse(BridgePhase.WAITING_FOR_DRONE.isStreaming)
        assertTrue(BridgePhase.LIVE.isStreaming)
    }
}
