package com.djilivebridge.android

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class BridgePhaseTest {
    private fun receiving(status: String, outputStatus: String = "disabled") = RelayServiceUiState(
        isActive = true,
        snapshot = RelaySnapshot(status = status, outputStatus = outputStatus),
    )

    private fun live(status: String, outputStatus: String = "armed") =
        receiving(status, outputStatus).copy(liveProfileId = "profile-1")

    @Test
    fun `stopped receiver is idle`() {
        assertEquals(BridgePhase.IDLE, bridgePhase(RelayServiceUiState()))
    }

    @Test
    fun `failed start keeps the setup screen with an error`() {
        val state = RelayServiceUiState(
            isActive = false,
            snapshot = RelaySnapshot(status = "error", detail = "0.0.0.0:1935 dinlenemedi"),
        )
        assertEquals(BridgePhase.START_FAILED, bridgePhase(state))
    }

    @Test
    fun `the drone connects and shows before anything goes live`() {
        assertEquals(BridgePhase.STARTING, bridgePhase(receiving("starting")))
        assertEquals(BridgePhase.WAITING_FOR_DRONE, bridgePhase(receiving("listening")))
        assertEquals(BridgePhase.DRONE_CONNECTED, bridgePhase(receiving("connected")))
        assertEquals(BridgePhase.PREVIEW, bridgePhase(receiving("publishing")))
    }

    @Test
    fun `a stale output status never looks live after the stream ended`() {
        // The snapshot lags "end live" by up to one poll.
        assertEquals(BridgePhase.PREVIEW, bridgePhase(receiving("publishing", "forwarding")))
    }

    @Test
    fun `going live follows the output pipeline`() {
        assertEquals(BridgePhase.CONNECTING_TARGET, bridgePhase(live("publishing", "disabled")))
        assertEquals(BridgePhase.CONNECTING_TARGET, bridgePhase(live("publishing", "armed")))
        assertEquals(BridgePhase.CONNECTING_TARGET, bridgePhase(live("publishing", "connecting")))
        assertEquals(BridgePhase.CONNECTING_TARGET, bridgePhase(live("publishing", "ready")))
        assertEquals(BridgePhase.LIVE, bridgePhase(live("publishing", "forwarding")))
        // A slow uplink skips video frames but the broadcast goes on.
        assertEquals(BridgePhase.LIVE, bridgePhase(live("publishing", "congested")))
    }

    @Test
    fun `a live stream waits for the drone when it drops`() {
        assertEquals(BridgePhase.WAITING_FOR_DRONE, bridgePhase(live("listening")))
        assertEquals(BridgePhase.DRONE_CONNECTED, bridgePhase(live("connected")))
    }

    @Test
    fun `target reconnect wins over a still publishing source`() {
        assertEquals(BridgePhase.RECONNECTING, bridgePhase(live("publishing", "reconnecting")))
    }

    @Test
    fun `receiver error wins over every output state`() {
        assertEquals(BridgePhase.RECEIVER_ERROR, bridgePhase(live("error", "reconnecting")))
        assertEquals(BridgePhase.RECEIVER_ERROR, bridgePhase(live("error", "forwarding")))
    }

    @Test
    fun `only a stream going out counts as streaming`() {
        val streaming = BridgePhase.entries.filter { it.isStreaming }.toSet()
        assertEquals(
            setOf(BridgePhase.CONNECTING_TARGET, BridgePhase.LIVE, BridgePhase.RECONNECTING),
            streaming,
        )
        assertFalse(BridgePhase.PREVIEW.isStreaming)
        assertTrue(BridgePhase.PREVIEW.hasPicture)
        assertTrue(BridgePhase.LIVE.hasPicture)
        assertFalse(BridgePhase.DRONE_CONNECTED.hasPicture)
    }
}
