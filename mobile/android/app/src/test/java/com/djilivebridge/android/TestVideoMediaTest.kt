package com.djilivebridge.android

import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class TestVideoMediaTest {
    private val sps = byteArrayOf(0x67, 0x64, 0x00, 0x1F, 0x11, 0x22)
    private val pps = byteArrayOf(0x68, 0x33, 0x44)
    private val startCode = byteArrayOf(0, 0, 0, 1)

    @Test
    fun `annex b access units split on 3 and 4 byte start codes`() {
        val data = startCode + sps + byteArrayOf(0, 0, 1) + pps + byteArrayOf(0, 0) // trailing zeros
        val units = annexBNalUnits(data)
        assertEquals(2, units.size)
        assertArrayEquals(sps, units[0])
        assertArrayEquals(pps, units[1])
    }

    @Test
    fun `annex b samples become length prefixed`() {
        val slice = byteArrayOf(0x65, 0x01, 0x02)
        val data = startCode + sps + startCode + slice
        val expected = byteArrayOf(0, 0, 0, 6) + sps + byteArrayOf(0, 0, 0, 3) + slice
        assertArrayEquals(expected, toAvcc(data, data.size))
    }

    @Test
    fun `length prefixed samples pass through, even when a length looks like a 3 byte start code`() {
        // A 260-byte NAL unit's length prefix starts with 00 00 01.
        val avcc = byteArrayOf(0, 0, 1, 4) + ByteArray(260) { 0x41 }
        assertArrayEquals(avcc, toAvcc(avcc, avcc.size))
    }

    @Test
    fun `configuration record is built from separate parameter sets`() {
        val record = avcConfigurationRecord(startCode + sps, startCode + pps)
        val expected = byteArrayOf(1, 0x64, 0x00, 0x1F, 0xFF.toByte(), 0xE1.toByte(), 0, 6) + sps +
            byteArrayOf(1, 0, 3) + pps
        assertArrayEquals(expected, record)
    }

    @Test
    fun `configuration record needs both parameter sets and keeps a ready avcC`() {
        assertNull(avcConfigurationRecord(startCode + sps))
        val avcC = byteArrayOf(1, 0x64, 0x00, 0x1F, 0xFF.toByte(), 0xE1.toByte(), 0, 6) + sps
        assertArrayEquals(avcC, avcConfigurationRecord(avcC))
    }

    @Test
    fun `flv video tags carry frame type and composition time`() {
        assertArrayEquals(
            byteArrayOf(0x17, 0x01, 0x00, 0x00, 0x21, 9),
            flvAvcFrame(byteArrayOf(9), keyframe = true, compositionTimeMs = 33),
        )
        assertArrayEquals(
            byteArrayOf(0x27, 0x01, 0x00, 0x00, 0x00, 9),
            flvAvcFrame(byteArrayOf(9), keyframe = false, compositionTimeMs = 0),
        )
        assertArrayEquals(byteArrayOf(0x17, 0x00, 0x00, 0x00, 0x00, 1), flvAvcSequenceHeader(byteArrayOf(1)))
        assertArrayEquals(byteArrayOf(0xAF.toByte(), 0x01, 5), flvAacFrame(byteArrayOf(5, 6), 1))
    }

    @Test
    fun `decode timestamps with b frames stay monotonic and never follow presentation`() {
        // I P B B P B B in decode order, 33 ms apart in presentation.
        val presentation = longArrayOf(0, 99, 33, 66, 198, 132, 165)
        val decode = decodeTimestamps(presentation)
        assertArrayEquals(longArrayOf(-33, 0, 33, 66, 99, 132, 165), decode)
        decode.indices.forEach { index -> assert(decode[index] <= presentation[index]) }
    }

    @Test
    fun `test video label hides photo picker aliases`() {
        assertEquals("Video · 00:20 · döngüde", testVideoLabel("43.mp4", 20_000))
        assertEquals("drone-test.mp4 · 01:05 · döngüde", testVideoLabel("drone-test.mp4", 65_000))
        assertEquals("Video · döngüde", testVideoLabel(null, null))
    }

    @Test
    fun `decode timestamps equal presentation when there are no b frames`() {
        val presentation = longArrayOf(0, 33, 66, 99)
        assertArrayEquals(presentation, decodeTimestamps(presentation))
    }
}
