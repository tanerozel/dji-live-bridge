package com.streammydrone.app

import android.annotation.SuppressLint
import android.media.AudioFormat
import android.media.AudioRecord
import android.media.MediaCodec
import android.media.MediaCodecInfo
import android.media.MediaFormat
import android.media.MediaRecorder
import android.os.SystemClock
import android.util.Log
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.setValue
import kotlin.concurrent.thread
import kotlin.math.abs
import kotlin.math.max

/**
 * Records the phone's microphone and hands it to the relay as AAC, which sends it in place of
 * DJI Fly's audio (silence on an RC 2): the pilot talks over the broadcast. 48 kHz mono AAC-LC,
 * as DJI Fly sends, so a platform sees the same audio format whichever of the two plays.
 *
 * The relay puts the frames on the drone's timeline; frames only go out while the drone's
 * picture comes in.
 */
internal class PhoneMicrophone(private val onFailed: (UiText) -> Unit) {
    @Volatile private var running = true
    private val worker = thread(name = "dji-phone-microphone") { run() }

    /** Stops recording and gives the broadcast DJI Fly's audio back. */
    fun stop() {
        running = false
        worker.join(STOP_TIMEOUT_MS)
    }

    // The service only starts this with RECORD_AUDIO granted; a revoked permission throws below.
    @SuppressLint("MissingPermission")
    private fun run() {
        var record: AudioRecord? = null
        var encoder: MediaCodec? = null
        try {
            val bufferBytes = max(
                AudioRecord.getMinBufferSize(SAMPLE_RATE, AudioFormat.CHANNEL_IN_MONO, AudioFormat.ENCODING_PCM_16BIT),
                FRAME_BYTES * 8,
            )
            record = AudioRecord(
                MediaRecorder.AudioSource.MIC,
                SAMPLE_RATE,
                AudioFormat.CHANNEL_IN_MONO,
                AudioFormat.ENCODING_PCM_16BIT,
                bufferBytes,
            ).also { check(it.state == AudioRecord.STATE_INITIALIZED) { "AudioRecord not initialized" } }
            val format = MediaFormat.createAudioFormat(MediaFormat.MIMETYPE_AUDIO_AAC, SAMPLE_RATE, 1).apply {
                setInteger(MediaFormat.KEY_AAC_PROFILE, MediaCodecInfo.CodecProfileLevel.AACObjectLC)
                setInteger(MediaFormat.KEY_BIT_RATE, BIT_RATE)
                setInteger(MediaFormat.KEY_MAX_INPUT_SIZE, FRAME_BYTES * 4)
            }
            encoder = MediaCodec.createEncoderByType(MediaFormat.MIMETYPE_AUDIO_AAC).apply {
                configure(format, null, null, MediaCodec.CONFIGURE_FLAG_ENCODE)
                start()
            }
            record.startRecording()
            check(record.recordingState == AudioRecord.RECORDSTATE_RECORDING) { "the microphone is in use" }
            encode(record, encoder)
        } catch (error: Exception) {
            Log.w(LOG_TAG, "The phone's microphone stopped", error)
            if (running) onFailed(uiText(R.string.phone_mic_failed, error.message ?: error.javaClass.simpleName))
        } finally {
            NativeRelay.nativePhoneAudioStop()
            PhoneMicrophoneLevel.value = 0f
            runCatching { record?.stop() }
            record?.release()
            runCatching { encoder?.stop() }
            encoder?.release()
        }
    }

    private fun encode(record: AudioRecord, encoder: MediaCodec) {
        val pcm = ByteArray(FRAME_BYTES)
        val info = MediaCodec.BufferInfo()
        var output = ByteArray(FRAME_BYTES)
        var configured = false
        // Timestamps count samples, so encoder bursts cannot move them; the relay realigns
        // when the count and the clock drift apart (a dropped buffer, a stalled recording).
        var samples = 0L
        var peak = 0
        var levelAt = SystemClock.elapsedRealtime()

        fun configure(config: ByteArray) {
            val error = NativeRelay.nativePhoneAudioStart(config)
            check(error.isEmpty()) { error }
            configured = true
        }

        while (running) {
            val read = record.read(pcm, 0, pcm.size)
            check(read >= 0) { "AudioRecord.read: $read" }
            for (index in 0 until read / 2) {
                val sample = (pcm[index * 2].toInt() and 0xFF) or (pcm[index * 2 + 1].toInt() shl 8)
                peak = max(peak, abs(sample.toShort().toInt()))
            }
            val now = SystemClock.elapsedRealtime()
            if (now - levelAt >= LEVEL_INTERVAL_MS) {
                PhoneMicrophoneLevel.value = peak / Short.MAX_VALUE.toFloat()
                peak = 0
                levelAt = now
            }

            val input = encoder.dequeueInputBuffer(INPUT_TIMEOUT_US)
            if (input >= 0) {
                encoder.getInputBuffer(input)!!.apply {
                    clear()
                    put(pcm, 0, read)
                }
                encoder.queueInputBuffer(input, 0, read, samples * 1_000_000L / SAMPLE_RATE, 0)
                samples += read / 2
            }

            while (true) {
                val index = encoder.dequeueOutputBuffer(info, 0)
                if (index == MediaCodec.INFO_OUTPUT_FORMAT_CHANGED) {
                    encoder.outputFormat.getByteBuffer("csd-0")?.let { csd ->
                        if (!configured) configure(ByteArray(csd.remaining()).also(csd::get))
                    }
                    continue
                }
                if (index < 0) break
                val buffer = encoder.getOutputBuffer(index)!!
                if (output.size < info.size) output = ByteArray(info.size)
                buffer.position(info.offset)
                buffer.get(output, 0, info.size)
                encoder.releaseOutputBuffer(index, false)
                when {
                    info.flags and MediaCodec.BUFFER_FLAG_CODEC_CONFIG != 0 -> {
                        if (!configured) configure(output.copyOf(info.size))
                    }
                    configured && info.size > 0 -> NativeRelay.nativePhoneAudioFrame(info.presentationTimeUs, output, info.size)
                }
            }
        }
    }

    private companion object {
        const val SAMPLE_RATE = 48_000
        const val BIT_RATE = 96_000
        /** One AAC frame of 16-bit mono samples. */
        const val FRAME_BYTES = 1024 * 2
        const val INPUT_TIMEOUT_US = 10_000L
        const val LEVEL_INTERVAL_MS = 100L
        const val STOP_TIMEOUT_MS = 1_000L
    }
}

/** How loud the phone's microphone is, from 0 to 1, for the meter on screen. */
internal object PhoneMicrophoneLevel {
    var value by mutableFloatStateOf(0f)
}
