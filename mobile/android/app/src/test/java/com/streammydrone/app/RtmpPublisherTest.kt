package com.streammydrone.app

import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Test
import java.io.ByteArrayInputStream
import java.io.DataInputStream

class RtmpPublisherTest {
    @Test
    fun `amf0 command values round trip`() {
        val encoded = Amf0.encode(
            "connect",
            1.0,
            mapOf("app" to "drone", "tcUrl" to "rtmp://127.0.0.1:1935/drone", "fpad" to false),
            null,
        )
        assertEquals(
            listOf(
                "connect",
                1.0,
                mapOf("app" to "drone", "tcUrl" to "rtmp://127.0.0.1:1935/drone", "fpad" to false),
                null,
            ),
            Amf0.decode(encoded),
        )
    }

    @Test
    fun `amf0 metadata is an ecma array`() {
        val encoded = Amf0.encode("onMetaData", AmfEcmaArray(mapOf("width" to 1280.0, "stereo" to true)))
        assertEquals(0x08, encoded[13].toInt()) // after the 13-byte "onMetaData" string
        assertEquals(listOf("onMetaData", mapOf("width" to 1280.0, "stereo" to true)), Amf0.decode(encoded))
    }

    @Test
    fun `amf0 decoding stops at an unknown marker but keeps earlier values`() {
        val encoded = Amf0.encode("_result", 2.0) + byteArrayOf(0x11, 0x00)
        assertEquals(listOf("_result", 2.0), Amf0.decode(encoded))
    }

    @Test
    fun `a message that fits one chunk gets a single type 0 header`() {
        val payload = ByteArray(10) { it.toByte() }
        val chunks = encodeRtmpChunks(3, 1, TYPE_COMMAND_AMF0, 0x010203, payload, DEFAULT_CHUNK_SIZE)
        val header = byteArrayOf(0x03, 0x01, 0x02, 0x03, 0x00, 0x00, 0x0A, 0x14, 0x01, 0x00, 0x00, 0x00)
        assertArrayEquals(header + payload, chunks)
    }

    @Test
    fun `long messages continue in type 3 chunks`() {
        val payload = ByteArray(300) { 7 }
        val chunks = encodeRtmpChunks(6, 1, TYPE_VIDEO, 40, payload, 128)
        assertEquals(12 + 128 + 1 + 128 + 1 + 44, chunks.size)
        assertEquals(0xC6, chunks[12 + 128].toInt() and 0xFF)
        assertEquals(0xC6, chunks[12 + 128 + 1 + 128].toInt() and 0xFF)
    }

    @Test
    fun `extended timestamps are repeated on continuation chunks`() {
        val timestamp = 0x01000000L
        val chunks = encodeRtmpChunks(6, 1, TYPE_VIDEO, timestamp, ByteArray(200), 128)
        assertEquals(0xFFFFFF, readUint24(chunks, 1))
        assertEquals(timestamp, readUint32(chunks, 12))
        val continuation = 16 + 128
        assertEquals(0xC6, chunks[continuation].toInt() and 0xFF)
        assertEquals(timestamp, readUint32(chunks, continuation + 1))
        assertEquals(16 + 128 + 5 + 72, chunks.size)
    }

    @Test
    fun `the chunk reader reassembles what the writer produced`() {
        val first = ByteArray(5_000) { (it % 251).toByte() }
        val second = Amf0.encode("onStatus", 0.0, null, mapOf("code" to "NetStream.Publish.Start"))
        val stream = encodeRtmpChunks(6, 1, TYPE_VIDEO, 0x01000000L, first, 4_096) +
            encodeRtmpChunks(3, 1, TYPE_COMMAND_AMF0, 0, second, 4_096)
        val reader = RtmpChunkReader(DataInputStream(ByteArrayInputStream(stream))).apply { chunkSize = 4_096 }

        val video = reader.readMessage()
        assertEquals(TYPE_VIDEO, video.typeId)
        assertArrayEquals(first, video.payload)
        val status = reader.readMessage()
        assertEquals(TYPE_COMMAND_AMF0, status.typeId)
        assertEquals("NetStream.Publish.Start", (Amf0.decode(status.payload)[3] as Map<*, *>)["code"])
    }

    private fun readUint24(bytes: ByteArray, offset: Int): Int =
        (0 until 3).fold(0) { value, index -> (value shl 8) or (bytes[offset + index].toInt() and 0xFF) }

    private fun readUint32(bytes: ByteArray, offset: Int): Long =
        (0 until 4).fold(0L) { value, index -> (value shl 8) or (bytes[offset + index].toLong() and 0xFF) }
}
