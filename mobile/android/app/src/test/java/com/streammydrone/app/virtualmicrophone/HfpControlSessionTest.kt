package com.streammydrone.app.virtualmicrophone

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.InputStream
import java.io.OutputStream
import java.util.concurrent.CopyOnWriteArrayList
import java.util.concurrent.CountDownLatch
import java.util.concurrent.LinkedBlockingQueue
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicReference

class HfpControlSessionTest {
    @Test
    fun `shared parser rejects malformed and deceptive success payloads`() {
        HfpControlProtocol.selfCheck()
    }

    @Test
    fun `connected requires all mandatory replies and holds the socket without requesting audio`() {
        val transport = ScriptedTransport(response = { command -> if (command == "AT+CMER=3,0,0,1") null else reply(command) })
        val running = Running(transport, stageTimeoutMs = 1_000)
        assertTrue(transport.cmerSent.await(2, TimeUnit.SECONDS))
        assertFalse(running.connected.await(40, TimeUnit.MILLISECONDS))
        transport.receive("OK\r\n")
        assertTrue(running.connected.await(2, TimeUnit.SECONDS))
        assertEquals(listOf("AT+BRSF=0", "AT+CIND=?", "AT+CIND?", "AT+CMER=3,0,0,1"), transport.commands)
        assertEquals(4096L, running.events.last().agFeatures)
        assertFalse(running.done.await(100, TimeUnit.MILLISECONDS))
        running.cancel()
        assertEquals("CANCELLED", running.outcome.get())
        assertTrue(transport.closed)
    }

    @Test
    fun `successful SLC cancels handshake deadlines while unsolicited indicators are drained`() {
        val transport = ScriptedTransport(::reply)
        val running = Running(transport, stageTimeoutMs = 100, overallTimeoutMs = 200)
        assertTrue(running.connected.await(2, TimeUnit.SECONDS))
        repeat(100) { transport.receive("+CIEV: 1,1\r\n") }
        assertFalse(running.done.await(250, TimeUnit.MILLISECONDS))
        transport.eof()
        running.awaitDone()
        assertEquals("RFCOMM_REMOTE_EOF", running.outcome.get())
    }

    @Test
    fun `OK alone never establishes HFP control`() {
        val running = Running(ScriptedTransport(response = { "OK\r\n" }))
        running.awaitDone()
        assertEquals("BRSF_MISSING_VALID_PAYLOAD", running.outcome.get())
        assertFalse(running.events.any { it.connected })
    }

    @Test
    fun `invalid indicator value stops before CMER`() {
        val transport = ScriptedTransport(response = { command ->
            if (command == "AT+CIND?") "+CIND: 1,0,0,0,9,0,5\r\nOK\r\n" else reply(command)
        })
        val running = Running(transport)
        running.awaitDone()
        assertEquals("CIND_VALUES_INVALID_OR_DUPLICATE_PAYLOAD", running.outcome.get())
        assertFalse(transport.commands.contains("AT+CMER=3,0,0,1"))
    }

    @Test
    fun `negotiation timeout closes the transport and cannot report connected`() {
        val transport = ScriptedTransport(response = { null })
        val running = Running(transport, stageTimeoutMs = 60)
        running.awaitDone()
        assertEquals("BRSF_TIMEOUT", running.outcome.get())
        assertTrue(transport.closed)
        assertFalse(running.events.any { it.connected })
    }

    @Test
    fun `blocked connect is bounded by closing the transport`() {
        val transport = ScriptedTransport(::reply, blockConnect = true)
        val running = Running(transport, connectTimeoutMs = 60)
        running.awaitDone()
        assertEquals("RFCOMM_CONNECT_TIMEOUT", running.outcome.get())
        assertTrue(transport.commands.isEmpty())
        assertTrue(transport.closed)
    }

    @Test
    fun `permission loss closes held control without starting audio`() {
        val health = AtomicReference<String?>()
        val transport = ScriptedTransport(::reply)
        val running = Running(transport, health = { health.get() })
        assertTrue(running.connected.await(2, TimeUnit.SECONDS))
        health.set("PERMISSION_DENIED")
        transport.receive("+CIEV: 1,0\r\n")
        running.awaitDone()
        assertEquals("PERMISSION_DENIED", running.outcome.get())
        assertTrue(transport.closed)
        assertEquals(4, transport.commands.size)
    }

    @Test
    fun `codec request is rejected without responding or claiming audio`() {
        val transport = ScriptedTransport(::reply)
        val running = Running(transport)
        assertTrue(running.connected.await(2, TimeUnit.SECONDS))
        transport.receive("+BCS: 1\r\n")
        running.awaitDone()
        assertEquals("UNEXPECTED_CODEC_NEGOTIATION", running.outcome.get())
        assertEquals(4, transport.commands.size)
    }

    @Test
    fun `non ASCII and oversized lines have sanitized failures`() {
        for ((response, expected) in listOf(
            "\u0001\r\n" to "RFCOMM_NON_ASCII_RESPONSE",
            "x".repeat(2_049) + "\r\n" to "RFCOMM_LINE_TOO_LONG",
        )) {
            val running = Running(ScriptedTransport(response = { response }))
            running.awaitDone()
            assertEquals(expected, running.outcome.get())
            assertFalse(running.events.any { it.connected })
        }
    }

    private class Running(
        transport: ScriptedTransport,
        stageTimeoutMs: Long = 1_000,
        connectTimeoutMs: Long = 1_000,
        overallTimeoutMs: Long = 5_000,
        health: () -> String? = { null },
    ) {
        val events = CopyOnWriteArrayList<HfpControlProgress>()
        val connected = CountDownLatch(1)
        val done = CountDownLatch(1)
        val outcome = AtomicReference<String?>()
        private val session = HfpControlSession(
            transport, health,
            progress = { event -> events += event; if (event.connected) connected.countDown() },
            stageTimeoutMs = stageTimeoutMs, connectTimeoutMs = connectTimeoutMs, overallTimeoutMs = overallTimeoutMs,
        )
        private val worker = Thread {
            try {
                session.run()
            } catch (error: HfpControlFailure) {
                outcome.set(error.code)
            } finally {
                done.countDown()
            }
        }.apply { isDaemon = true; start() }

        fun cancel() { session.cancel(); awaitDone() }
        fun awaitDone() {
            assertTrue("session worker did not finish", done.await(2, TimeUnit.SECONDS))
            worker.join(300)
            assertFalse(worker.isAlive)
        }
    }

    private class ScriptedTransport(
        private val response: (String) -> String?,
        private val blockConnect: Boolean = false,
    ) : HfpControlTransport {
        private val bytes = LinkedBlockingQueue<Int>()
        private val connectRelease = CountDownLatch(1)
        val commands = CopyOnWriteArrayList<String>()
        val cmerSent = CountDownLatch(1)
        @Volatile var closed = false
            private set
        override fun connect() { if (blockConnect) connectRelease.await() }
        override val input = object : InputStream() {
            override fun read(): Int = bytes.take()
        }
        override val output = object : OutputStream() {
            private val command = StringBuilder()
            override fun write(value: Int) {
                if (value == 13) {
                    val text = command.toString()
                    command.setLength(0)
                    commands += text
                    if (text == "AT+CMER=3,0,0,1") cmerSent.countDown()
                    response(text)?.let(::receive)
                } else command.append(value.toChar())
            }
        }
        fun receive(text: String) { text.forEach { bytes.offer(it.code) } }
        fun eof() { bytes.offer(-1) }
        override fun close() { closed = true; connectRelease.countDown(); eof() }
    }

    private companion object {
        fun reply(command: String): String = when (command) {
            "AT+BRSF=0" -> "+BRSF: 4096\r\nOK\r\n"
            "AT+CIND=?" -> "+CIND: (\"service\",(0,1)),(\"call\",(0-1)),(\"callsetup\",(0-3))," +
                "(\"callheld\",(0-2)),(\"signal\",(0-5)),(\"roam\",(0-1)),(\"battchg\",(0-5))\r\nOK\r\n"
            "AT+CIND?" -> "+CIND: 1,0,0,0,5,0,5\r\nOK\r\n"
            "AT+CMER=3,0,0,1" -> "OK\r\n"
            else -> error("Unexpected command")
        }
    }
}
