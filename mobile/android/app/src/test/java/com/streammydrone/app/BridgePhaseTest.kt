package com.streammydrone.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class BridgePhaseTest {
    /** The receiver runs; each output status is one platform the stream goes to. */
    private fun receiving(status: String, vararg outputs: String) = RelayServiceUiState(
        isActive = true,
        snapshot = RelaySnapshot(
            status = status,
            outputs = outputs.mapIndexed { index, output -> OutputSnapshot("platform-$index", status = output) },
        ),
    )

    private fun live(status: String, vararg outputs: String) =
        receiving(status, *outputs).copy(liveProfileIds = outputs.indices.map { "platform-$it" }.ifEmpty { listOf("platform-0") })

    @Test
    fun `stopped receiver is idle`() {
        assertEquals(BridgePhase.IDLE, bridgePhase(RelayServiceUiState()))
    }

    @Test
    fun `failed start keeps the setup screen with an error`() {
        val state = RelayServiceUiState(
            isActive = false,
            snapshot = RelaySnapshot(status = "error", error = nativeError("listen_failed: 0.0.0.0:1935: address in use")),
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
    fun `a stale output never looks live after the stream ended`() {
        // The snapshot lags "end live" by up to one poll.
        assertEquals(BridgePhase.PREVIEW, bridgePhase(receiving("publishing", "forwarding")))
    }

    @Test
    fun `going live follows the output pipeline`() {
        assertEquals(BridgePhase.CONNECTING_TARGET, bridgePhase(live("publishing")))
        assertEquals(BridgePhase.CONNECTING_TARGET, bridgePhase(live("publishing", "armed")))
        assertEquals(BridgePhase.CONNECTING_TARGET, bridgePhase(live("publishing", "connecting")))
        assertEquals(BridgePhase.CONNECTING_TARGET, bridgePhase(live("publishing", "ready")))
        assertEquals(BridgePhase.LIVE, bridgePhase(live("publishing", "forwarding")))
        // A slow uplink skips video frames but the broadcast goes on.
        assertEquals(BridgePhase.LIVE, bridgePhase(live("publishing", "congested")))
    }

    @Test
    fun `several platforms are live while any of them receives the stream`() {
        assertEquals(BridgePhase.LIVE, bridgePhase(live("publishing", "reconnecting", "forwarding")))
        assertEquals(BridgePhase.LIVE, bridgePhase(live("publishing", "connecting", "congested", "armed")))
        assertEquals(BridgePhase.CONNECTING_TARGET, bridgePhase(live("publishing", "reconnecting", "connecting")))
        assertEquals(BridgePhase.RECONNECTING, bridgePhase(live("publishing", "reconnecting", "reconnecting")))
        val snapshot = live("publishing", "forwarding", "congested").snapshot
        assertEquals("forwarding", snapshot.outputStatus)
        assertEquals("congested", snapshot.output("platform-1")?.status)
    }

    @Test
    fun `a live stream waits for the drone when it drops`() {
        assertEquals(BridgePhase.WAITING_FOR_DRONE, bridgePhase(live("listening", "holding")))
        assertEquals(BridgePhase.DRONE_CONNECTED, bridgePhase(live("connected", "holding")))
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
