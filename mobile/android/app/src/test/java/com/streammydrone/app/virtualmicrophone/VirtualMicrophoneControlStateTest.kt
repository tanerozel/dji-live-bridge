package com.streammydrone.app.virtualmicrophone

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class VirtualMicrophoneControlStateTest {
    @Test
    fun `late generation and completed session callbacks cannot reconnect`() {
        val current = VirtualMicrophoneControlSnapshot(2, VirtualMicrophoneControlPhase.CONNECTING, "private-peer")
        val connected = HfpControlProgress("HFP_CONTROL_CONNECTED", 4096, connected = true)
        assertEquals(current, current.progress(1, connected))
        assertEquals(current, current.finish(1, "OLD_FAILURE", failed = true))
        val stopped = current.finish(2, "USER_DISCONNECTED", failed = false)
        assertEquals(stopped, stopped.progress(2, connected))
        assertEquals(stopped, stopped.finish(2, "OLD_FAILURE", failed = true))
        assertEquals(null, stopped.peerAddress)
        assertFalse(stopped.isActive)
    }

    @Test
    fun `HFP control success never becomes an audio capture claim`() {
        val negotiating = VirtualMicrophoneControlSnapshot(5, VirtualMicrophoneControlPhase.CONNECTING)
            .progress(5, HfpControlProgress("RFCOMM_CONNECTED"))
        assertEquals(VirtualMicrophoneControlPhase.NEGOTIATING, negotiating.phase)
        val connected = negotiating.progress(5, HfpControlProgress("HFP_CONTROL_CONNECTED", 4096, connected = true))
        assertEquals(VirtualMicrophoneControlPhase.CONNECTED, connected.phase)
        assertTrue(connected.isActive)
        assertEquals("NOT_STARTED", connected.audioStatus)
        assertEquals(null, connected.errorCode)
        val failed = connected.finish(5, "RFCOMM_READ_FAILED", failed = true)
        assertEquals(VirtualMicrophoneControlPhase.FAILED, failed.phase)
        assertEquals("RFCOMM_READ_FAILED", failed.errorCode)
        assertEquals("NOT_STARTED", failed.audioStatus)
    }
}
