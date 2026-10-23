package com.streammydrone.app.virtualmicrophone

import java.io.Closeable
import java.io.IOException
import java.io.InputStream
import java.io.OutputStream
import java.util.concurrent.ArrayBlockingQueue
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicReference

/** Public RFCOMM only. Implementations must unblock connect/read/write when closed. */
internal interface HfpControlTransport : Closeable {
    fun connect()
    val input: InputStream
    val output: OutputStream
}

internal class HfpControlFailure(val code: String) : Exception()

internal data class HfpControlProgress(val code: String, val agFeatures: Long? = null, val connected: Boolean = false)

/**
 * The minimum HF=0 service-level exchange, followed by a held control channel. It never requests
 * SCO, opens a microphone, negotiates a codec, controls calls, or changes a Bluetooth identity.
 * Incoming lines are bounded and discarded after validation; only allowlisted progress codes leave.
 */
internal class HfpControlSession(
    private val transport: HfpControlTransport,
    private val healthCheck: () -> String? = { null },
    private val progress: (HfpControlProgress) -> Unit = {},
    private val connectTimeoutMs: Long = 8_000,
    private val stageTimeoutMs: Long = 4_000,
    private val overallTimeoutMs: Long = 28_000,
    private val now: () -> Long = { System.nanoTime() / 1_000_000 },
) : Closeable {
    private data class ReadEvent(val line: String? = null, val failure: String? = null)
    private val stopped = AtomicBoolean()
    private val failure = AtomicReference<String?>()
    private val lines = ArrayBlockingQueue<ReadEvent>(64)
    private val scheduler = Executors.newSingleThreadScheduledExecutor { runnable ->
        Thread(runnable, "hfp-control-deadlines").apply { isDaemon = true }
    }
    private var reader: Thread? = null

    fun run() {
        checkRunning()
        val overall = scheduler.schedule({ abort("OVERALL_TIMEOUT") }, overallTimeoutMs, TimeUnit.MILLISECONDS)
        try {
            checkedHealth()
            progress(HfpControlProgress("RFCOMM_CONNECT_STARTED"))
            bounded("RFCOMM_CONNECT_TIMEOUT", connectTimeoutMs) { transport.connect() }
            progress(HfpControlProgress("RFCOMM_CONNECTED"))
            startReader()
            var features: Long? = null
            exchange("BRSF", "AT+BRSF=0", true) { line ->
                HfpControlProtocol.features(line)?.also { features = it } != null
            }
            var indicators: List<Set<Int>>? = null
            exchange("CIND_DEFINITIONS", "AT+CIND=?", true) { line ->
                HfpControlProtocol.indicators(line)?.also { indicators = it } != null
            }
            val definitions = checkNotNull(indicators)
            exchange("CIND_VALUES", "AT+CIND?", true) { HfpControlProtocol.statuses(it, definitions) }
            exchange("CMER", "AT+CMER=3,0,0,1", false) { false }
            checkRunning()
            overall.cancel(false)
            // A fired overall deadline wins even if it raced the last OK.
            checkRunning()
            progress(HfpControlProgress("HFP_CONTROL_CONNECTED", checkNotNull(features), connected = true))
            holdControlChannel()
        } catch (error: HfpControlFailure) {
            throw HfpControlFailure(failure.get() ?: error.code)
        } catch (_: SecurityException) {
            throw HfpControlFailure(failure.get() ?: "PERMISSION_DENIED")
        } catch (_: IOException) {
            throw HfpControlFailure(failure.get() ?: "RFCOMM_IO_FAILED")
        } catch (_: InterruptedException) {
            Thread.currentThread().interrupt()
            throw HfpControlFailure(failure.get() ?: "CANCELLED")
        } catch (_: RuntimeException) {
            throw HfpControlFailure(failure.get() ?: "RFCOMM_UNEXPECTED_FAILURE")
        } finally {
            overall.cancel(false)
            close()
        }
    }

    private fun exchange(stage: String, command: String, needsPayload: Boolean, payload: (String) -> Boolean) {
        bounded("${stage}_TIMEOUT", stageTimeoutMs) {
            checkedHealth()
            transport.output.write((command + "\r").toByteArray(Charsets.US_ASCII))
            transport.output.flush()
            progress(HfpControlProgress("${stage}_SENT"))
            var validPayload = false
            var ignored = 0
            while (true) {
                val line = nextLine()
                when {
                    line == command -> Unit // Optional echo never counts as a response.
                    line == "OK" -> {
                        if (needsPayload && !validPayload) throw HfpControlFailure("${stage}_MISSING_VALID_PAYLOAD")
                        progress(HfpControlProgress("${stage}_VALID_RESPONSE_AND_OK"))
                        return@bounded
                    }
                    line == "ERROR" || line.startsWith("+CME ERROR:") -> throw HfpControlFailure("${stage}_AG_ERROR")
                    terminal(line) -> throw HfpControlFailure("${stage}_LINK_TERMINAL_RESPONSE")
                    line.startsWith("+BCS:") -> throw HfpControlFailure("UNEXPECTED_CODEC_NEGOTIATION")
                    line.startsWith("+BRSF:") || line.startsWith("+CIND:") -> {
                        if (validPayload || !payload(line)) throw HfpControlFailure("${stage}_INVALID_OR_DUPLICATE_PAYLOAD")
                        validPayload = true
                    }
                    else -> if (++ignored > 32) throw HfpControlFailure("${stage}_EXCESS_UNSOLICITED_LINES")
                }
            }
        }
    }

    private fun holdControlChannel() {
        var window = now()
        var count = 0
        while (true) {
            val line = nextLine()
            if (terminal(line)) throw HfpControlFailure("RFCOMM_LINK_TERMINAL_RESPONSE")
            if (line.startsWith("+BCS:")) throw HfpControlFailure("UNEXPECTED_CODEC_NEGOTIATION")
            val current = now()
            if (current - window >= 10_000) {
                window = current
                count = 0
            }
            if (++count > 256) throw HfpControlFailure("RFCOMM_EXCESS_UNSOLICITED_LINES")
            // CIEV, RING, volume and other unsolicited lines need no audio or call action from HF=0.
        }
    }

    private fun nextLine(): String {
        while (true) {
            checkedHealth()
            val event = lines.poll(500, TimeUnit.MILLISECONDS) ?: continue
            checkRunning()
            event.failure?.let { throw HfpControlFailure(it) }
            return checkNotNull(event.line)
        }
    }

    private fun terminal(line: String): Boolean = line in setOf("NO CARRIER", "BUSY", "NO ANSWER")

    private fun checkedHealth() {
        checkRunning()
        healthCheck()?.let { abort(it); throw HfpControlFailure(it) }
    }

    private fun checkRunning() {
        failure.get()?.let { throw HfpControlFailure(it) }
        if (stopped.get()) throw HfpControlFailure("CANCELLED")
    }

    private fun <T> bounded(code: String, timeoutMs: Long, operation: () -> T): T {
        checkRunning()
        val deadline = scheduler.schedule({ abort(code) }, timeoutMs, TimeUnit.MILLISECONDS)
        return try {
            operation().also { checkRunning() }
        } finally {
            deadline.cancel(false)
        }
    }

    private fun startReader() {
        reader = Thread({
            try {
                val line = StringBuilder()
                while (!stopped.get()) {
                    when (val byte = transport.input.read()) {
                        -1 -> { readerFailed("RFCOMM_REMOTE_EOF"); return@Thread }
                        10, 13 -> if (line.isNotEmpty()) {
                            if (!lines.offer(ReadEvent(line = line.toString().trim()))) {
                                readerFailed("RFCOMM_RESPONSE_QUEUE_OVERFLOW")
                                return@Thread
                            }
                            line.setLength(0)
                        }
                        in 32..126 -> {
                            line.append(byte.toChar())
                            if (line.length > 2_048) {
                                readerFailed("RFCOMM_LINE_TOO_LONG")
                                return@Thread
                            }
                        }
                        else -> { readerFailed("RFCOMM_NON_ASCII_RESPONSE"); return@Thread }
                    }
                }
            } catch (_: SecurityException) {
                readerFailed("PERMISSION_DENIED")
            } catch (_: IOException) {
                readerFailed("RFCOMM_READ_FAILED")
            } catch (_: InterruptedException) {
                Thread.currentThread().interrupt()
                readerFailed("RFCOMM_READ_FAILED")
            } catch (_: RuntimeException) {
                readerFailed("RFCOMM_READ_FAILED")
            }
        }, "hfp-control-reader").apply { isDaemon = true; start() }
    }

    private fun readerFailed(code: String) {
        if (stopped.get()) return
        // Preserve already queued responses on EOF, but never drop an error behind a full queue.
        if (!lines.offer(ReadEvent(failure = code))) abort("RFCOMM_RESPONSE_QUEUE_OVERFLOW")
    }

    private fun abort(code: String) {
        failure.compareAndSet(null, code)
        closeTransport()
        lines.clear()
        lines.offer(ReadEvent(failure = failure.get()))
    }

    fun cancel(code: String = "CANCELLED") {
        abort(code)
        close()
    }

    override fun close() {
        stopped.set(true)
        closeTransport()
        scheduler.shutdownNow()
        reader?.interrupt()
        if (Thread.currentThread() !== reader) {
            try { reader?.join(300) } catch (_: InterruptedException) { Thread.currentThread().interrupt() }
        }
    }

    private fun closeTransport() {
        try {
            transport.close()
        } catch (_: IOException) {
            // Closing must not replace a timeout/cancellation reason with a raw peer-bearing error.
        } catch (_: RuntimeException) {
            // Permission loss or a platform close failure also must not crash the service/UI.
        }
    }
}
