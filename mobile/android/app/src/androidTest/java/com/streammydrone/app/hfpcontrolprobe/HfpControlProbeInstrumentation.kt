package com.streammydrone.app.hfpcontrolprobe

import android.Manifest
import android.app.Activity
import android.app.Instrumentation
import android.bluetooth.BluetoothManager
import android.bluetooth.BluetoothSocket
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
import android.os.Process
import android.os.SystemClock
import com.streammydrone.app.RelayServiceState
import com.streammydrone.app.virtualmicrophone.HfpControlProtocol
import org.json.JSONArray
import org.json.JSONObject
import java.io.IOException
import java.io.InputStream
import java.util.UUID
import java.util.concurrent.ArrayBlockingQueue
import java.util.concurrent.CountDownLatch
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicReference

/**
 * Explicit, test-APK-only HFP AG RFCOMM probe using ordinary target-app permissions.
 * No local HF SDP, SCO, microphone, codec negotiation, call control, identity changes or retries.
 *
 * am instrument -w -e targetDeviceName QCOM-BTD \
 *   com.streammydrone.app.test/com.streammydrone.app.hfpcontrolprobe.HfpControlProbeInstrumentation
 *
 * Instrumentation restarts the app process: the operator must verify the app is idle BEFORE launching.
 * The in-process isLive check is an additional guard, not evidence of its prelaunch state.
 */
class HfpControlProbeInstrumentation : Instrumentation() {
    private var arguments = Bundle()

    override fun onCreate(arguments: Bundle?) {
        super.onCreate(arguments)
        this.arguments = arguments ?: Bundle()
        start()
    }

    override fun onStart() {
        val report = ProbeReport()
        val probe = Probe(report)
        val completed = CountDownLatch(1)
        val worker = Thread({
            try {
                HfpControlProtocol.selfCheck()
                report.event("PARSER_SELF_CHECK_PASSED")
                if (arguments.getString("parserOnly") == "true") {
                    report.result("PARSER_SELF_CHECK_ONLY", true)
                } else {
                    probe.run(arguments.getString("targetDeviceName"))
                }
            } catch (failure: ProbeFailure) {
                report.result(failure.code, false)
            } catch (_: SecurityException) {
                report.result("PERMISSION_DENIED", false)
            } catch (_: IOException) {
                report.result("BLUETOOTH_IO_FAILURE", false)
            } catch (_: InterruptedException) {
                report.result("INTERRUPTED", false)
                Thread.currentThread().interrupt()
            } catch (_: Throwable) {
                // Do not print arbitrary exception messages: Bluetooth failures may include addresses.
                report.result("UNEXPECTED_FAILURE", false)
            } finally {
                probe.close()
                completed.countDown()
            }
        }, "hfp-control-probe").apply { isDaemon = true }
        worker.start()
        if (!completed.await(28, TimeUnit.SECONDS)) {
            report.result("OVERALL_TIMEOUT", false)
            probe.close()
            worker.interrupt()
        }
        // countDown occurs in finally immediately before thread return; join avoids a false
        // cleanup failure if the waiter wakes during those last few instructions.
        worker.join(500)
        report.event(if (worker.isAlive) "WORKER_CLEANUP_INCOMPLETE" else "WORKER_STOPPED")
        if (worker.isAlive) report.result("WORKER_CLEANUP_INCOMPLETE", false)
        val result = report.snapshot()
        finish(
            if (result.optBoolean("passed")) Activity.RESULT_OK else Activity.RESULT_CANCELED,
            Bundle().apply {
                putString("stream", "\n${result.toString(2)}\n")
                putString("probe_report_json", result.toString())
                putInt("probe_exit_code", if (result.optBoolean("passed")) 0 else 2)
            },
        )
    }

    private inner class Probe(private val report: ProbeReport) {
        private val socket = AtomicReference<BluetoothSocket?>()
        private val stopped = AtomicBoolean()
        private val lines = ArrayBlockingQueue<ReadEvent>(64)
        private var reader: Thread? = null
        private val scheduler = Executors.newSingleThreadScheduledExecutor { runnable ->
            Thread(runnable, "hfp-control-deadline").apply { isDaemon = true }
        }

        fun run(targetName: String?) {
            // Deliberately require the exact user-selected peer, never silently choose a nearby device.
            if (targetName != "QCOM-BTD") throw ProbeFailure("EXPECTED_TARGET_NAME_REQUIRED")
            if (Process.myUid() != targetContext.applicationInfo.uid || Process.myUid() in setOf(0, 2000)) {
                throw ProbeFailure("ORDINARY_TARGET_APP_UID_REQUIRED")
            }
            report.event("ORDINARY_TARGET_APP_UID_CONFIRMED")
            report.number("bluetoothPrivilegedGranted", if (
                targetContext.checkSelfPermission("android.permission.BLUETOOTH_PRIVILEGED") == PackageManager.PERMISSION_GRANTED
            ) 1 else 0)
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S &&
                targetContext.checkSelfPermission(Manifest.permission.BLUETOOTH_CONNECT) != PackageManager.PERMISSION_GRANTED
            ) throw ProbeFailure("BLUETOOTH_CONNECT_NOT_GRANTED")
            if (RelayServiceState.value.isLive) throw ProbeFailure("REFUSED_RELAY_IS_LIVE")
            val adapter = targetContext.getSystemService(BluetoothManager::class.java)?.adapter
                ?: throw ProbeFailure("NO_BLUETOOTH_ADAPTER")
            if (!adapter.isEnabled) throw ProbeFailure("BLUETOOTH_OFF")
            val matching = adapter.bondedDevices.filter { it.name == targetName }
            report.number("pairedNameMatchCount", matching.size.toLong())
            if (matching.size != 1) throw ProbeFailure("PAIRED_TARGET_ABSENT_OR_AMBIGUOUS")
            report.event("EXACT_PAIRED_TARGET_SELECTED")
            if (stopped.get()) throw ProbeFailure("CANCELLED_BEFORE_CONNECT")
            if (RelayServiceState.value.isLive) throw ProbeFailure("REFUSED_RELAY_IS_LIVE")
            val connection = matching.single().createRfcommSocketToServiceRecord(HFP_AG)
            socket.set(connection)
            if (stopped.get()) {
                connection.close()
                throw ProbeFailure("CANCELLED_BEFORE_CONNECT")
            }
            val connectExpired = AtomicBoolean()
            val deadline = scheduler.schedule({
                connectExpired.set(true)
                closeSocket()
            }, 8, TimeUnit.SECONDS)
            report.event("HFP_AG_RFCOMM_CONNECT_STARTED")
            try {
                connection.connect()
            } catch (_: IOException) {
                throw ProbeFailure(if (connectExpired.get()) "RFCOMM_CONNECT_TIMEOUT" else "RFCOMM_CONNECT_FAILED")
            } finally {
                deadline.cancel(false)
            }
            if (connectExpired.get()) throw ProbeFailure("RFCOMM_CONNECT_TIMEOUT")
            report.event("HFP_AG_RFCOMM_CONNECTED")
            report.number("hfAdvertisedOptionalFeatures", 0)
            startReader(connection.inputStream)

            // HFP 1.8 section 4.2: HF features=0 has no optional BAC/CHLD/BIND branches.
            // Primary implementation reference: PipeWire spa/plugins/bluez5/backend-native.c.
            var agFeatures: Long? = null
            exchange(connection, "BRSF", "AT+BRSF=0", true) { line ->
                HfpControlProtocol.features(line)?.also { agFeatures = it } != null
            }
            report.number("agSupportedFeatures", checkNotNull(agFeatures))
            var indicators: List<Set<Int>>? = null
            exchange(connection, "CIND_DEFINITIONS", "AT+CIND=?", true) { line ->
                HfpControlProtocol.indicators(line)?.also { indicators = it } != null
            }
            val definitions = checkNotNull(indicators)
            report.number("validatedIndicatorCount", definitions.size.toLong())
            exchange(connection, "CIND_VALUES", "AT+CIND?", true) { line ->
                HfpControlProtocol.statuses(line, definitions)
            }
            exchange(connection, "CMER", "AT+CMER=3,0,0,1", false) { false }
            report.result("MANDATORY_SLC_EXCHANGE_OBSERVED", true)
        }

        private fun exchange(
            connection: BluetoothSocket,
            stage: String,
            command: String,
            requiresPayload: Boolean,
            payload: (String) -> Boolean,
        ) {
            if (stopped.get()) throw ProbeFailure("CANCELLED")
            if (RelayServiceState.value.isLive) throw ProbeFailure("REFUSED_RELAY_IS_LIVE")
            connection.outputStream.write((command + "\r").toByteArray(Charsets.US_ASCII))
            connection.outputStream.flush()
            report.event("${stage}_SENT")
            val end = SystemClock.elapsedRealtime() + 4_000
            var validPayload = false
            var ignoredLines = 0
            while (true) {
                val remaining = end - SystemClock.elapsedRealtime()
                if (remaining <= 0) throw ProbeFailure("${stage}_TIMEOUT")
                val event = lines.poll(remaining, TimeUnit.MILLISECONDS) ?: throw ProbeFailure("${stage}_TIMEOUT")
                if (event.failure != null) throw ProbeFailure(event.failure)
                val line = checkNotNull(event.line)
                when {
                    line == command -> Unit // Optional command echo; never counted as a response.
                    line == "OK" -> {
                        if (requiresPayload && !validPayload) throw ProbeFailure("${stage}_MISSING_VALID_PAYLOAD")
                        report.event("${stage}_VALID_RESPONSE_AND_OK")
                        return
                    }
                    line == "ERROR" || line.startsWith("+CME ERROR:") -> throw ProbeFailure("${stage}_AG_ERROR")
                    line in setOf("NO CARRIER", "BUSY", "NO ANSWER") -> throw ProbeFailure("${stage}_LINK_TERMINAL_RESPONSE")
                    line.startsWith("+BCS:") -> throw ProbeFailure("UNEXPECTED_CODEC_NEGOTIATION")
                    line.startsWith("+BRSF:") || line.startsWith("+CIND:") -> {
                        if (validPayload || !payload(line)) throw ProbeFailure("${stage}_INVALID_OR_DUPLICATE_PAYLOAD")
                        validPayload = true
                    }
                    else -> {
                        // Ignore unsolicited text without storing or printing it; bounded against flooding.
                        if (++ignoredLines > 32) throw ProbeFailure("${stage}_EXCESS_UNSOLICITED_LINES")
                    }
                }
            }
        }

        private fun startReader(input: InputStream) {
            reader = Thread({
                try {
                    val line = StringBuilder()
                    while (!stopped.get()) {
                        val byte = input.read()
                        if (byte < 0) {
                            lines.offer(ReadEvent(failure = "RFCOMM_REMOTE_EOF"))
                            break
                        }
                        when (byte) {
                            10, 13 -> if (line.isNotEmpty()) {
                                if (!lines.offer(ReadEvent(line = line.toString().trim()))) {
                                    lines.clear()
                                    lines.offer(ReadEvent(failure = "RFCOMM_RESPONSE_QUEUE_OVERFLOW"))
                                    break
                                }
                                line.setLength(0)
                            }
                            in 32..126 -> {
                                line.append(byte.toChar())
                                if (line.length > 2_048) {
                                    lines.offer(ReadEvent(failure = "RFCOMM_LINE_TOO_LONG"))
                                    break
                                }
                            }
                            else -> {
                                lines.offer(ReadEvent(failure = "RFCOMM_NON_ASCII_RESPONSE"))
                                break
                            }
                        }
                    }
                } catch (_: IOException) {
                    lines.offer(ReadEvent(failure = "RFCOMM_READ_FAILED"))
                }
            }, "hfp-control-reader").apply { isDaemon = true; start() }
        }

        private fun closeSocket() {
            try { socket.getAndSet(null)?.close() } catch (_: IOException) { /* no raw error logs */ }
        }

        fun close() {
            stopped.set(true)
            closeSocket()
            scheduler.shutdownNow()
            reader?.interrupt()
            if (Thread.currentThread() !== reader) reader?.join(250)
            if (reader?.isAlive == true) {
                report.result("READER_CLEANUP_INCOMPLETE", false)
            }
        }
    }

    private data class ReadEvent(val line: String? = null, val failure: String? = null)
    private class ProbeFailure(val code: String) : Exception()

    private class ProbeReport {
        private val started = SystemClock.elapsedRealtime()
        private val events = mutableListOf<JSONObject>()
        private val numbers = mutableMapOf<String, Long>()
        private var outcome = "NOT_COMPLETED"
        private var passed = false
        private var failed = false

        @Synchronized fun event(code: String) {
            events += JSONObject().put("event", code).put("elapsedMs", SystemClock.elapsedRealtime() - started)
        }
        @Synchronized fun number(key: String, value: Long) { numbers[key] = value }
        @Synchronized fun result(code: String, success: Boolean) {
            // A late worker completion cannot overwrite an overall/cleanup timeout.
            if (success && failed) return
            if (!success) failed = true
            outcome = code
            passed = success
            event(code)
        }
        @Synchronized fun snapshot(): JSONObject = JSONObject()
            .put("schemaVersion", 1).put("outcome", outcome).put("passed", passed)
            .put("elapsedMs", SystemClock.elapsedRealtime() - started)
            .put("scope", "RFCOMM_CONTROL_PLANE_ONLY_NO_AUDIO_NO_LOCAL_HF_SDP")
            .put("audioVerdict", "NOT_TESTED").put("rtmpMicrophoneVerdict", "NOT_TESTED")
            .put("events", JSONArray(events)).also { output -> numbers.forEach { (key, value) -> output.put(key, value) } }
    }

    private companion object {
        val HFP_AG: UUID = UUID.fromString("0000111f-0000-1000-8000-00805f9b34fb")
    }
}
