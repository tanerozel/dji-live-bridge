package com.djilivebridge.android

import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.nio.ByteBuffer

class FlvMediaTest {
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
    fun `the preview reads back the configuration record the test video writes`() {
        val record = avcConfigurationRecord(startCode + sps, startCode + pps)!!
        val config = parseAvcConfigurationRecord(record)!!
        assertEquals(4, config.nalLengthSize)
        assertArrayEquals(sps, config.sps.single())
        assertArrayEquals(pps, config.pps.single())
        assertArrayEquals(startCode + sps, withStartCodes(config.sps))
    }

    @Test
    fun `the preview reads DJI Fly's configuration record`() {
        // The video sequence header an RC 2 sent: H.264 High 3.1, 1280x720.
        val record = hex("0164001fffe100126764001facb402802dd2905060506d0a135001000568ee06f2c0")
        val config = parseAvcConfigurationRecord(record)!!
        assertEquals(4, config.nalLengthSize)
        assertArrayEquals(hex("6764001facb402802dd2905060506d0a1350"), config.sps.single())
        assertArrayEquals(hex("68ee06f2c0"), config.pps.single())
    }

    @Test
    fun `truncated configuration records are rejected`() {
        val record = avcConfigurationRecord(startCode + sps, startCode + pps)!!
        assertNull(parseAvcConfigurationRecord(record.copyOf(record.size - 1)))
        assertNull(parseAvcConfigurationRecord(byteArrayOf(0, 1, 2, 3, 4, 5, 6)))
    }

    @Test
    fun `flv video tags parse into config, pictures and unsupported codecs`() {
        val config = parseFlvVideoTag(flvAvcSequenceHeader(byteArrayOf(1, 2, 3)))
        assertArrayEquals(byteArrayOf(1, 2, 3), (config as FlvVideoTag.Config).record)

        val picture = parseFlvVideoTag(byteArrayOf(9, 9) + flvAvcFrame(byteArrayOf(7), keyframe = true, compositionTimeMs = 66), offset = 2)
        picture as FlvVideoTag.Picture
        assertEquals(true, picture.keyframe)
        assertEquals(66, picture.compositionTimeMs)
        assertArrayEquals(byteArrayOf(7), picture.data)

        // Composition time is a signed 24-bit value.
        val negative = parseFlvVideoTag(byteArrayOf(0x27, 0x01, 0xFF.toByte(), 0xFF.toByte(), 0xDF.toByte(), 1))
        assertEquals(-33, (negative as FlvVideoTag.Picture).compositionTimeMs)
        assertEquals(false, negative.keyframe)

        assertEquals(FlvVideoTag.Unsupported, parseFlvVideoTag(byteArrayOf(0x1C, 0x01, 0, 0, 0))) // legacy HEVC id
        assertEquals(FlvVideoTag.Unsupported, parseFlvVideoTag(byteArrayOf(0x90.toByte(), 0x68, 0x76, 0x63, 0x31))) // E-RTMP
        assertNull(parseFlvVideoTag(byteArrayOf(0x17, 0x01, 0)))
        assertNull(parseFlvVideoTag(byteArrayOf(0x17, 0x02, 0, 0, 0))) // end of sequence
    }

    @Test
    fun `length prefixed units become annex b and a bad length stops conversion`() {
        val slice = byteArrayOf(0x65, 0x01)
        val avcc = byteArrayOf(0, 0, 0, 6) + sps + byteArrayOf(0, 0, 0, 2) + slice
        assertArrayEquals(startCode + sps + startCode + slice, annexB(avcc))
        assertArrayEquals(startCode + sps, annexB(byteArrayOf(0, 0, 0, 6) + sps + byteArrayOf(0, 0, 0, 9, 1)))
        // A decoder input buffer that is too small takes nothing.
        assertEquals(-1, putAnnexB(avcc, 4, ByteBuffer.allocate(8)))
    }

    @Test
    fun `decode timestamps equal presentation when there are no b frames`() {
        val presentation = longArrayOf(0, 33, 66, 99)
        assertArrayEquals(presentation, decodeTimestamps(presentation))
    }

    @Test
    fun `decode order is display order only for pic order count type 2`() {
        // DJI Fly's SPS (RC 2): High 3.1, pic_order_cnt_type 2, no reordering.
        assertTrue(avcOutputsInDecodeOrder(hex("6764001facb402802dd2905060506d0a1350")))
        // x264 with B-frames: pic_order_cnt_type 0.
        assertFalse(avcOutputsInDecodeOrder(hex("6764001facd9405005bb0110000003001000000303c0f1831960")))
        assertFalse(avcOutputsInDecodeOrder(byteArrayOf(0x67, 0x64)))
    }

    private fun annexB(avcc: ByteArray): ByteArray {
        val buffer = ByteBuffer.allocate(avcc.size + 16)
        val size = putAnnexB(avcc, 4, buffer)
        return buffer.array().copyOf(size)
    }

    private fun hex(value: String): ByteArray =
        value.chunked(2).map { it.toInt(16).toByte() }.toByteArray()
}
