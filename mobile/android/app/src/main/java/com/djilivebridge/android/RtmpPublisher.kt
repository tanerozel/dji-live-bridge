package com.djilivebridge.android

import java.io.BufferedOutputStream
import java.io.ByteArrayOutputStream
import java.io.Closeable
import java.io.DataInputStream
import java.io.IOException
import java.io.OutputStream
import java.net.InetSocketAddress
import java.net.Socket
import java.nio.ByteBuffer
import kotlin.concurrent.thread
import kotlin.random.Random

/**
 * Just enough of an RTMP publisher to feed the bridge's own ingest over loopback the way DJI Fly
 * does: legacy handshake, connect → createStream → publish, then FLV-tag media messages. It
 * answers the server's pings (librtmp2 drops peers that stay silent for 10 seconds); everything
 * else the server sends is read and ignored.
 */
internal class RtmpPublisher(
    private val host: String,
    private val port: Int,
    private val app: String,
) : Closeable {
    private val socket = Socket()
    private val writeLock = Any()
    private lateinit var input: DataInputStream
    private lateinit var output: OutputStream
    private lateinit var reader: RtmpChunkReader
    private var outChunkSize = DEFAULT_CHUNK_SIZE
    private var streamId = 0
    private var readerThread: Thread? = null

    @Volatile private var closed = false

    @Volatile private var failure: IOException? = null

    fun connect() {
        socket.connect(InetSocketAddress(host, port), SETUP_TIMEOUT_MS)
        socket.tcpNoDelay = true
        socket.soTimeout = SETUP_TIMEOUT_MS
        input = DataInputStream(socket.getInputStream().buffered())
        output = BufferedOutputStream(socket.getOutputStream(), OUTPUT_BUFFER_BYTES)
        reader = RtmpChunkReader(input)
        handshake()

        send(CSID_CONTROL, TYPE_SET_CHUNK_SIZE, 0, 0, int32(PUBLISH_CHUNK_SIZE))
        outChunkSize = PUBLISH_CHUNK_SIZE
        val connect = mapOf(
            "app" to app,
            "type" to "nonprivate",
            "flashVer" to "FMLE/3.0 (compatible; DJI Live Bridge)",
            "tcUrl" to "rtmp://$host:$port/$app",
        )
        sendCommand(0, "connect", 1.0, connect)
        awaitResult(1.0)
        sendCommand(0, "createStream", 2.0, null)
        streamId = (awaitResult(2.0).getOrNull(3) as? Double)?.toInt()
            ?: throw IOException("Yerel alıcı yayın akışı açmadı")
        sendCommand(streamId, "publish", 3.0, null, "", "live")
        awaitPublishStart()

        socket.soTimeout = 0
        readerThread = thread(name = "rtmp-test-reader", isDaemon = true) { drainServer() }
    }

    fun sendMetadata(metadata: AmfEcmaArray) {
        sendMedia(CSID_DATA, TYPE_DATA_AMF0, 0, Amf0.encode("@setDataFrame", "onMetaData", metadata))
    }

    fun sendVideo(timestampMs: Long, payload: ByteArray) = sendMedia(CSID_VIDEO, TYPE_VIDEO, timestampMs, payload)

    fun sendAudio(timestampMs: Long, payload: ByteArray) = sendMedia(CSID_AUDIO, TYPE_AUDIO, timestampMs, payload)

    override fun close() {
        if (closed) return
        if (streamId != 0) runCatching { sendCommand(streamId, "deleteStream", 4.0, null, streamId.toDouble()) }
        closed = true
        runCatching { socket.close() }
        readerThread?.join(READER_JOIN_TIMEOUT_MS)
    }

    private fun sendMedia(csid: Int, type: Int, timestampMs: Long, payload: ByteArray) {
        failure?.let { throw it }
        send(csid, type, streamId, timestampMs, payload)
    }

    private fun handshake() {
        val c1 = ByteArray(HANDSHAKE_SIZE)
        Random.nextBytes(c1, fromIndex = 8)
        synchronized(writeLock) {
            output.write(RTMP_VERSION)
            output.write(c1)
            output.flush()
        }
        val version = input.readUnsignedByte()
        if (version != RTMP_VERSION) throw IOException("Beklenmeyen RTMP sürümü: $version")
        val s1 = ByteArray(HANDSHAKE_SIZE).also(input::readFully)
        input.readFully(ByteArray(HANDSHAKE_SIZE))
        synchronized(writeLock) {
            output.write(s1)
            output.flush()
        }
    }

    private fun sendCommand(messageStreamId: Int, vararg values: Any?) {
        send(CSID_COMMAND, TYPE_COMMAND_AMF0, messageStreamId, 0, Amf0.encode(*values))
    }

    private fun send(csid: Int, type: Int, messageStreamId: Int, timestampMs: Long, payload: ByteArray) {
        val bytes = encodeRtmpChunks(csid, messageStreamId, type, timestampMs, payload, outChunkSize)
        synchronized(writeLock) {
            output.write(bytes)
            output.flush()
        }
    }

    /** Reads until the reply to [transaction]; control messages met on the way are handled. */
    private fun awaitResult(transaction: Double): List<Any?> {
        while (true) {
            val values = nextCommand() ?: continue
            if (values.getOrNull(1) != transaction) continue
            when (values.firstOrNull()) {
                "_result" -> return values
                "_error" -> throw IOException("Yerel alıcı isteği reddetti: ${statusDescription(values)}")
            }
        }
    }

    private fun awaitPublishStart() {
        while (true) {
            val values = nextCommand() ?: continue
            if (values.firstOrNull() != "onStatus") continue
            val info = values.getOrNull(3) as? Map<*, *>
            val code = info?.get("code") as? String
            when {
                code == "NetStream.Publish.Start" -> return
                info?.get("level") == "error" || code?.contains("Publish") == true ->
                    throw IOException("Yerel alıcı test yayınını kabul etmedi: ${code ?: "bilinmeyen durum"}")
            }
        }
    }

    private fun nextCommand(): List<Any?>? {
        val message = reader.readMessage()
        return if (message.typeId == TYPE_COMMAND_AMF0) Amf0.decode(message.payload) else {
            handleControl(message)
            null
        }
    }

    private fun drainServer() {
        try {
            while (!closed) {
                val message = reader.readMessage()
                if (message.typeId != TYPE_COMMAND_AMF0) {
                    handleControl(message)
                    continue
                }
                val values = Amf0.decode(message.payload)
                val info = values.getOrNull(3) as? Map<*, *>
                if (values.firstOrNull() == "onStatus" && info?.get("level") == "error") {
                    failure = IOException("Yerel alıcı test yayınını durdurdu: ${info["code"]}")
                }
            }
        } catch (error: IOException) {
            if (!closed) failure = error
        }
    }

    private fun handleControl(message: RtmpMessage) {
        val payload = message.payload
        when (message.typeId) {
            TYPE_SET_CHUNK_SIZE -> if (payload.size >= 4) {
                reader.chunkSize = ByteBuffer.wrap(payload).int and 0x7FFFFFFF
            }
            TYPE_USER_CONTROL -> if (payload.size >= 6 && readUint16(payload) == USER_CONTROL_PING_REQUEST) {
                val response = ByteArray(6)
                response[1] = USER_CONTROL_PING_RESPONSE.toByte()
                payload.copyInto(response, destinationOffset = 2, startIndex = 2, endIndex = 6)
                send(CSID_CONTROL, TYPE_USER_CONTROL, 0, 0, response)
            }
        }
    }

    private fun statusDescription(values: List<Any?>): String {
        val info = values.getOrNull(3) as? Map<*, *>
        return (info?.get("description") ?: info?.get("code") ?: "bilinmeyen hata").toString()
    }

    private companion object {
        const val RTMP_VERSION = 3
        const val HANDSHAKE_SIZE = 1536
        const val SETUP_TIMEOUT_MS = 5_000
        const val READER_JOIN_TIMEOUT_MS = 1_000L
        const val OUTPUT_BUFFER_BYTES = 64 * 1024
        const val PUBLISH_CHUNK_SIZE = 4_096
        const val CSID_CONTROL = 2
        const val CSID_COMMAND = 3
        const val CSID_AUDIO = 4
        const val CSID_DATA = 5
        const val CSID_VIDEO = 6
        const val USER_CONTROL_PING_REQUEST = 6
        const val USER_CONTROL_PING_RESPONSE = 7
    }
}

internal const val DEFAULT_CHUNK_SIZE = 128
internal const val TYPE_SET_CHUNK_SIZE = 1
internal const val TYPE_USER_CONTROL = 4
internal const val TYPE_AUDIO = 8
internal const val TYPE_VIDEO = 9
internal const val TYPE_DATA_AMF0 = 18
internal const val TYPE_COMMAND_AMF0 = 20
private const val MAX_MESSAGE_BYTES = 16 * 1024 * 1024
private const val EXTENDED_TIMESTAMP = 0xFFFFFF

internal class RtmpMessage(val typeId: Int, val payload: ByteArray)

/**
 * Splits one message into chunks. Every message starts with a full (type 0) header, which costs
 * a few bytes per frame but keeps the writer stateless. Continuation chunks repeat the extended
 * timestamp, as librtmp2's reader expects.
 */
internal fun encodeRtmpChunks(
    chunkStreamId: Int,
    messageStreamId: Int,
    typeId: Int,
    timestampMs: Long,
    payload: ByteArray,
    chunkSize: Int,
): ByteArray {
    require(chunkStreamId in 2..63) { "Chunk stream id $chunkStreamId needs a longer basic header" }
    val timestamp = timestampMs and 0xFFFFFFFFL
    val extended = timestamp >= EXTENDED_TIMESTAMP
    val out = ByteArrayOutputStream(payload.size + 16 + (payload.size / chunkSize) * 5)
    out.write(chunkStreamId)
    out.writeUint24(if (extended) EXTENDED_TIMESTAMP else timestamp.toInt())
    out.writeUint24(payload.size)
    out.write(typeId)
    // The message stream id is the one little-endian field in the header.
    out.write(messageStreamId and 0xFF)
    out.write(messageStreamId ushr 8 and 0xFF)
    out.write(messageStreamId ushr 16 and 0xFF)
    out.write(messageStreamId ushr 24 and 0xFF)
    if (extended) out.writeUint32(timestamp)
    var offset = 0
    while (true) {
        val length = minOf(chunkSize, payload.size - offset)
        out.write(payload, offset, length)
        offset += length
        if (offset >= payload.size) break
        out.write(0xC0 or chunkStreamId)
        if (extended) out.writeUint32(timestamp)
    }
    return out.toByteArray()
}

/** Reassembles the server's chunked messages; only their type and payload matter here. */
internal class RtmpChunkReader(private val input: DataInputStream) {
    var chunkSize = DEFAULT_CHUNK_SIZE

    private class ChunkStream {
        var length = 0
        var typeId = 0
        var extended = false
        var buffer: ByteArray? = null
        var received = 0
    }

    private val streams = HashMap<Int, ChunkStream>()

    fun readMessage(): RtmpMessage {
        while (true) {
            val first = input.readUnsignedByte()
            val format = first ushr 6
            val csid = when (val id = first and 0x3F) {
                0 -> 64 + input.readUnsignedByte()
                1 -> 64 + input.readUnsignedByte() + (input.readUnsignedByte() shl 8)
                else -> id
            }
            val stream = streams.getOrPut(csid) { ChunkStream() }
            if (format <= 2) {
                val timestampField = input.readUint24()
                if (format <= 1) {
                    stream.length = input.readUint24()
                    stream.typeId = input.readUnsignedByte()
                }
                if (format == 0) input.readInt() // message stream id; unused here
                stream.extended = timestampField == EXTENDED_TIMESTAMP
            }
            // skipBytes() may skip less than asked on a buffered stream; readInt() never does.
            if (stream.extended) input.readInt()
            if (stream.length > MAX_MESSAGE_BYTES) throw IOException("RTMP mesajı çok büyük")

            val buffer = stream.buffer ?: ByteArray(stream.length).also {
                stream.buffer = it
                stream.received = 0
            }
            val count = minOf(chunkSize, stream.length - stream.received)
            input.readFully(buffer, stream.received, count)
            stream.received += count
            if (stream.received >= stream.length) {
                stream.buffer = null
                return RtmpMessage(stream.typeId, buffer)
            }
        }
    }
}

/** An AMF0 ECMA array, the shape FLV metadata travels in. */
internal class AmfEcmaArray(val entries: Map<String, Any?>)

/** The AMF0 subset RTMP commands and metadata use. */
internal object Amf0 {
    private const val NUMBER = 0x00
    private const val BOOLEAN = 0x01
    private const val STRING = 0x02
    private const val OBJECT = 0x03
    private const val NULL = 0x05
    private const val UNDEFINED = 0x06
    private const val ECMA_ARRAY = 0x08
    private const val OBJECT_END = 0x09
    private const val STRICT_ARRAY = 0x0A
    private const val DATE = 0x0B
    private const val LONG_STRING = 0x0C

    fun encode(vararg values: Any?): ByteArray {
        val out = ByteArrayOutputStream()
        values.forEach { writeValue(out, it) }
        return out.toByteArray()
    }

    /** Decodes values in order, stopping at the first type it does not know. */
    fun decode(bytes: ByteArray): List<Any?> {
        val buffer = ByteBuffer.wrap(bytes)
        val values = mutableListOf<Any?>()
        try {
            while (buffer.hasRemaining()) values += readValue(buffer)
        } catch (_: RuntimeException) {
            // An unknown marker or a truncated value ends decoding; what came before still counts.
        }
        return values
    }

    private fun writeValue(out: ByteArrayOutputStream, value: Any?) {
        when (value) {
            null -> out.write(NULL)
            is Boolean -> {
                out.write(BOOLEAN)
                out.write(if (value) 1 else 0)
            }
            is Number -> {
                out.write(NUMBER)
                out.writeUint64(value.toDouble().toRawBits())
            }
            is String -> {
                val bytes = value.encodeToByteArray()
                if (bytes.size <= 0xFFFF) {
                    out.write(STRING)
                    out.writeUint16(bytes.size)
                } else {
                    out.write(LONG_STRING)
                    out.writeUint32(bytes.size.toLong())
                }
                out.write(bytes)
            }
            is AmfEcmaArray -> {
                out.write(ECMA_ARRAY)
                out.writeUint32(value.entries.size.toLong())
                writeProperties(out, value.entries)
            }
            is Map<*, *> -> {
                out.write(OBJECT)
                writeProperties(out, value)
            }
            else -> throw IllegalArgumentException("AMF0 cannot encode ${value::class.java.simpleName}")
        }
    }

    private fun writeProperties(out: ByteArrayOutputStream, properties: Map<*, *>) {
        properties.forEach { (key, value) ->
            val name = key.toString().encodeToByteArray()
            out.writeUint16(name.size)
            out.write(name)
            writeValue(out, value)
        }
        out.writeUint16(0)
        out.write(OBJECT_END)
    }

    private fun readValue(buffer: ByteBuffer): Any? = when (val marker = buffer.get().toInt() and 0xFF) {
        NUMBER -> Double.fromBits(buffer.long)
        BOOLEAN -> buffer.get().toInt() != 0
        STRING -> readUtf8(buffer, buffer.short.toInt() and 0xFFFF)
        OBJECT -> readProperties(buffer)
        NULL, UNDEFINED -> null
        ECMA_ARRAY -> {
            buffer.int
            readProperties(buffer)
        }
        STRICT_ARRAY -> List(buffer.int) { readValue(buffer) }
        DATE -> Double.fromBits(buffer.long).also { buffer.short }
        LONG_STRING -> readUtf8(buffer, buffer.int)
        else -> throw IllegalArgumentException("Unsupported AMF0 marker $marker")
    }

    private fun readProperties(buffer: ByteBuffer): Map<String, Any?> {
        val properties = LinkedHashMap<String, Any?>()
        while (true) {
            val nameLength = buffer.short.toInt() and 0xFFFF
            if (nameLength == 0 && (buffer.get(buffer.position()).toInt() and 0xFF) == OBJECT_END) {
                buffer.get()
                return properties
            }
            val name = readUtf8(buffer, nameLength)
            properties[name] = readValue(buffer)
        }
    }

    private fun readUtf8(buffer: ByteBuffer, length: Int): String {
        val bytes = ByteArray(length)
        buffer.get(bytes)
        return bytes.decodeToString()
    }
}

private fun int32(value: Int): ByteArray = ByteBuffer.allocate(4).putInt(value).array()

private fun readUint16(bytes: ByteArray): Int = ((bytes[0].toInt() and 0xFF) shl 8) or (bytes[1].toInt() and 0xFF)

private fun DataInputStream.readUint24(): Int =
    (readUnsignedByte() shl 16) or (readUnsignedByte() shl 8) or readUnsignedByte()

private fun ByteArrayOutputStream.writeUint16(value: Int) {
    write(value ushr 8 and 0xFF)
    write(value and 0xFF)
}

private fun ByteArrayOutputStream.writeUint24(value: Int) {
    write(value ushr 16 and 0xFF)
    write(value ushr 8 and 0xFF)
    write(value and 0xFF)
}

private fun ByteArrayOutputStream.writeUint32(value: Long) {
    write((value ushr 24).toInt() and 0xFF)
    write((value ushr 16).toInt() and 0xFF)
    write((value ushr 8).toInt() and 0xFF)
    write(value.toInt() and 0xFF)
}

private fun ByteArrayOutputStream.writeUint64(value: Long) {
    writeUint32(value ushr 32)
    writeUint32(value and 0xFFFFFFFFL)
}
