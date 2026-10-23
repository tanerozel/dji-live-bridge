package com.streammydrone.app.virtualmicrophone

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import java.util.UUID

class VirtualMicrophoneDiagnosticsTest {
    @Test
    fun `remote gateway service and privileged permission never imply a microphone audio session`() {
        AdapterStatus.entries.forEach { adapter ->
            listOf(false, true).forEach { privileged ->
                val evidence = MicrophoneEvidence(adapter, privileged, StandardAudioService.entries.toSet())
                assertEquals("NOT_ESTABLISHED", evidence.hfpServiceLevelConnection)
                assertEquals("NOT_STARTED", evidence.scoAudio)
                assertEquals("NOT_NEGOTIATED", evidence.codec)
                assertEquals("NOT_STARTED", evidence.microphoneCapture)
                assertEquals("NOT_STARTED", evidence.audioRouting)
                assertEquals("UNVERIFIED", evidence.djiFlyMicrophone)
            }
        }
    }

    @Test
    fun `established control connection is not audio or DJI Fly recognition evidence`() {
        val evidence = MicrophoneEvidence(
            AdapterStatus.ON, false, emptySet(),
            controlPhase = "CONNECTED", controlStage = "HFP_CONTROL_CONNECTED",
            controlEvents = listOf("BRSF_VALID_RESPONSE_AND_OK", "CIND_DEFINITIONS_VALID_RESPONSE_AND_OK", "CMER_VALID_RESPONSE_AND_OK"),
        )
        assertEquals("ESTABLISHED", evidence.hfpServiceLevelConnection)
        assertEquals("NOT_STARTED", evidence.scoAudio)
        assertEquals("NOT_STARTED", evidence.microphoneCapture)
        assertEquals("UNVERIFIED", evidence.djiFlyMicrophone)
        val report = MicrophoneDiagnosticReport("Test model", "16", 36, evidence, emptySet(), SdpStatus.IDLE, null).sanitizedText()
        assertTrue(report.contains("HFP service level connection: ESTABLISHED"))
        assertTrue(report.contains("Verified HFP stages: BRSF_VALID_RESPONSE_AND_OK, CIND_DEFINITIONS_VALID_RESPONSE_AND_OK, CMER_VALID_RESPONSE_AND_OK"))
        assertTrue(report.contains("DJI Fly microphone recognition: UNVERIFIED"))
        assertFalse(report.contains("UNSUPPORTED_PLATFORM"))
        assertFalse(report.contains("No Bluetooth HFP connection is attempted"))
    }

    @Test
    fun `default peer retains active connection or a unique user identified RC2 label only`() {
        val first = PairedDiagnosticDevice("peer-a", "Headphones")
        val rc2 = PairedDiagnosticDevice("peer-b", "QCOM-BTD")
        assertEquals("peer-b", preferredControlPeer(listOf(first, rc2), null))
        assertEquals("peer-a", preferredControlPeer(listOf(first, rc2), "peer-a"))
        assertEquals(null, preferredControlPeer(listOf(first), null))
        assertEquals(null, preferredControlPeer(listOf(rc2, PairedDiagnosticDevice("peer-c", "qcom-btd")), null))
    }

    @Test
    fun `UUID classification uses the complete Bluetooth base UUID and separates HF from AG`() {
        val uuids = listOf(
            StandardAudioService.HANDSFREE_AUDIO_GATEWAY.uuid,
            StandardAudioService.HEADSET.uuid,
            UUID.fromString("0000111e-1234-1000-8000-00805f9b34fb"),
            UUID.fromString("0000110b-0000-1000-8000-00805f9b34fb"), // A2DP sink is not HFP.
        )
        assertEquals(setOf(StandardAudioService.HANDSFREE_AUDIO_GATEWAY, StandardAudioService.HEADSET), standardAudioServices(uuids))
        assertTrue(standardAudioServices(emptyList()).isEmpty())
    }

    @Test
    fun `SDP timeout rejects late responses and manual retry uses a new generation`() {
        val pending = SdpAttempt().begin("private-peer", 100, 15_000)
        assertEquals(pending, pending.timeout(pending.generation, 15_099))
        val timeout = pending.timeout(pending.generation, 15_100)
        assertEquals(SdpStatus.TIMED_OUT, timeout.status)
        assertEquals(timeout, timeout.receive(pending.generation, "private-peer", 15_101, listOf(StandardAudioService.HANDSFREE.uuid)))
        val retry = timeout.begin("private-peer", 16_000, 15_000)
        assertTrue(retry.generation > pending.generation)
        assertEquals(retry, retry.receive(pending.generation, "private-peer", 16_001, listOf(StandardAudioService.HANDSFREE.uuid)))
        assertEquals(SdpStatus.RECEIVED, retry.receive(retry.generation, "private-peer", 16_001, listOf(StandardAudioService.HANDSFREE.uuid)).status)
    }

    @Test
    fun `cancelled requests wrong peers and deadline boundary cannot become successful`() {
        val pending = SdpAttempt().begin("peer-a", 10, 100)
        assertEquals(pending, pending.receive(pending.generation, "peer-b", 20, emptyList()))
        val cancelled = pending.cancel()
        assertEquals(SdpStatus.CANCELLED, cancelled.status)
        assertEquals(cancelled, cancelled.receive(cancelled.generation, "peer-a", 20, emptyList()))
        assertEquals(SdpStatus.TIMED_OUT, pending.receive(pending.generation, "peer-a", 110, emptyList()).status)
        assertEquals(SdpStatus.EMPTY, pending.receive(pending.generation, "peer-a", 109, null).status)
    }

    @Test
    fun `report includes only allowlisted services and no peer identifiers or arbitrary UUIDs`() {
        val privateUuid = UUID.fromString("b201ba9f-5a90-4f53-9cd2-7886afeaa682")
        val attempt = SdpAttempt().begin("AA:BB:CC:DD:EE:FF", 0, 100)
            .receive(1, "AA:BB:CC:DD:EE:FF", 10, listOf(privateUuid, StandardAudioService.HANDSFREE_AUDIO_GATEWAY.uuid))
        val report = MicrophoneDiagnosticReport(
            "Test manufacturer Test model", "16", 36,
            MicrophoneEvidence(AdapterStatus.ON, false, attempt.services),
            emptySet(), attempt.status, null,
        ).sanitizedText()
        assertFalse(report.contains("AA:BB:CC:DD:EE:FF"))
        assertFalse(report.contains(privateUuid.toString()))
        assertTrue(report.contains("HANDSFREE_AUDIO_GATEWAY (0x111f)"))
        assertTrue(report.contains("SCO/eSCO audio: NOT_STARTED"))
        assertTrue(report.contains("No audio is captured or transmitted"))
    }

    @Test
    fun `control error and stage metadata cannot export arbitrary peer identifiers`() {
        val peer = "AA:BB:CC:DD:EE:FF"
        val evidence = MicrophoneEvidence(
            AdapterStatus.ON, false, emptySet(), controlPhase = "CONNECTED", controlStage = peer,
            controlEvents = listOf("BRSF_VALID_RESPONSE_AND_OK", peer), disconnectReason = peer,
        )
        val report = MicrophoneDiagnosticReport("Test model", "16", 36, evidence, emptySet(), SdpStatus.IDLE, peer).sanitizedText()
        assertFalse(report.contains(peer))
        assertTrue(report.contains("Last error: REDACTED"))
        assertTrue(report.contains("Verified HFP stages: BRSF_VALID_RESPONSE_AND_OK"))
    }
}
