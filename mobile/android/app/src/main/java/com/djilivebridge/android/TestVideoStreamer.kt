package com.djilivebridge.android

import android.content.Context
import android.media.MediaExtractor
import android.media.MediaFormat
import android.net.Uri
import android.os.SystemClock
import android.provider.OpenableColumns
import java.nio.ByteBuffer

internal class TestVideoException(message: String) : Exception(message)

/** A video the user picked to stand in for the drone. */
internal class TestVideoSelection(val uri: Uri, val displayName: String, val rotated: Boolean)

private const val LOOPBACK_HOST = "127.0.0.1"
private const val INGEST_PORT = 1935
private const val INGEST_APP = "drone"
private const val SAMPLE_BUFFER_BYTES = 8 * 1024 * 1024
private const val FALLBACK_FRAME_US = 33_333L

private class TestVideoTracks(
    val video: Int,
    val videoFormat: MediaFormat,
    val audio: Int?,
    val audioFormat: MediaFormat?,
)

/** Checks a picked video before the bridge starts, so a bad file is reported right away. */
internal fun inspectTestVideo(context: Context, uri: Uri): TestVideoSelection {
    val extractor = MediaExtractor()
    try {
        openVideo(extractor, context, uri)
        val tracks = findTracks(extractor)
        val format = tracks.videoFormat
        val rotation = if (format.containsKey(MediaFormat.KEY_ROTATION)) format.getInteger(MediaFormat.KEY_ROTATION) else 0
        val durationMs = if (format.containsKey(MediaFormat.KEY_DURATION)) format.getLong(MediaFormat.KEY_DURATION) / 1_000 else null
        return TestVideoSelection(uri, testVideoLabel(displayName(context, uri), durationMs), rotated = rotation % 180 != 0)
    } finally {
        extractor.release()
    }
}

/**
 * Sends a video file to the bridge's own ingest exactly as DJI Fly would: H.264 and AAC copied
 * without re-encoding, paced in real time and looped until the thread is interrupted.
 */
internal class TestVideoStreamer(private val context: Context, private val uri: Uri) {
    private class Timeline(val videoDecodeTimesUs: LongArray, val baseUs: Long, val loopUs: Long)

    fun run() {
        val extractor = MediaExtractor()
        try {
            openVideo(extractor, context, uri)
            val tracks = findTracks(extractor)
            val videoFormat = tracks.videoFormat
            val configuration = avcConfigurationRecord(
                *listOfNotNull(videoFormat.codecData("csd-0"), videoFormat.codecData("csd-1")).toTypedArray(),
            ) ?: throw TestVideoException("Videonun H.264 ayarları okunamadı.")
            val audioConfig = tracks.audioFormat?.codecData("csd-0")
            val audioTrack = tracks.audio.takeIf { audioConfig != null }

            extractor.selectTrack(tracks.video)
            audioTrack?.let(extractor::selectTrack)
            val timeline = scanTimeline(extractor, tracks.video)
            extractor.seekTo(0, MediaExtractor.SEEK_TO_PREVIOUS_SYNC)

            RtmpPublisher(LOOPBACK_HOST, INGEST_PORT, INGEST_APP).use { rtmp ->
                rtmp.connect()
                rtmp.sendMetadata(metadata(videoFormat, tracks.audioFormat.takeIf { audioConfig != null }))
                rtmp.sendVideo(0, flvAvcSequenceHeader(configuration))
                audioConfig?.let { rtmp.sendAudio(0, flvAacSequenceHeader(it)) }
                stream(extractor, rtmp, timeline, tracks.video)
            }
        } finally {
            extractor.release()
        }
    }

    private fun scanTimeline(extractor: MediaExtractor, videoTrack: Int): Timeline {
        val videoTimes = ArrayList<Long>()
        var first = Long.MAX_VALUE
        var last = Long.MIN_VALUE
        while (extractor.sampleTrackIndex >= 0) {
            val time = extractor.sampleTime
            if (extractor.sampleTrackIndex == videoTrack) videoTimes += time
            first = minOf(first, time)
            last = maxOf(last, time)
            if (!extractor.advance()) break
        }
        if (videoTimes.isEmpty()) throw TestVideoException("Videoda gönderilecek kare yok.")
        val decodeTimes = decodeTimestamps(videoTimes.toLongArray())
        val frameUs = if (videoTimes.size > 1) {
            ((videoTimes.max() - videoTimes.min()) / (videoTimes.size - 1)).coerceAtLeast(1_000)
        } else {
            FALLBACK_FRAME_US
        }
        val base = minOf(first, decodeTimes.first())
        // One frame of gap between loops keeps timestamps strictly increasing.
        return Timeline(decodeTimes, base, last - base + frameUs)
    }

    private fun stream(extractor: MediaExtractor, rtmp: RtmpPublisher, timeline: Timeline, videoTrack: Int) {
        val buffer = ByteBuffer.allocate(SAMPLE_BUFFER_BYTES)
        val startedAt = SystemClock.elapsedRealtime()
        var loop = 0L
        var videoIndex = 0
        while (!Thread.currentThread().isInterrupted) {
            val track = extractor.sampleTrackIndex
            val size = if (track >= 0) extractor.readSampleData(buffer, 0) else -1
            if (size < 0) {
                loop++
                videoIndex = 0
                extractor.seekTo(0, MediaExtractor.SEEK_TO_PREVIOUS_SYNC)
                if (extractor.sampleTrackIndex < 0) throw TestVideoException("Video başa sarılamadı.")
                continue
            }
            val presentationUs = extractor.sampleTime
            val offsetUs = loop * timeline.loopUs - timeline.baseUs
            if (track == videoTrack) {
                val decodeUs = timeline.videoDecodeTimesUs.getOrElse(videoIndex++) { presentationUs }
                val timestampMs = (decodeUs + offsetUs) / 1_000
                val compositionMs = ((presentationUs - decodeUs) / 1_000).toInt().coerceAtLeast(0)
                val keyframe = extractor.sampleFlags and MediaExtractor.SAMPLE_FLAG_SYNC != 0
                waitUntil(startedAt + timestampMs)
                rtmp.sendVideo(timestampMs, flvAvcFrame(toAvcc(buffer.array(), size), keyframe, compositionMs))
            } else {
                val timestampMs = (presentationUs + offsetUs) / 1_000
                waitUntil(startedAt + timestampMs)
                rtmp.sendAudio(timestampMs, flvAacFrame(buffer.array(), size))
            }
            extractor.advance()
        }
    }

    private fun waitUntil(elapsedRealtimeMs: Long) {
        val delay = elapsedRealtimeMs - SystemClock.elapsedRealtime()
        if (delay > 0) Thread.sleep(delay)
    }

    private fun metadata(video: MediaFormat, audio: MediaFormat?): AmfEcmaArray {
        val entries = linkedMapOf<String, Any?>(
            "width" to video.getInteger(MediaFormat.KEY_WIDTH).toDouble(),
            "height" to video.getInteger(MediaFormat.KEY_HEIGHT).toDouble(),
            "videocodecid" to 7.0,
        )
        video.frameRate()?.let { entries["framerate"] = it }
        if (audio != null) {
            entries["audiocodecid"] = 10.0
            entries["audiosamplerate"] = audio.getInteger(MediaFormat.KEY_SAMPLE_RATE).toDouble()
            entries["stereo"] = audio.getInteger(MediaFormat.KEY_CHANNEL_COUNT) > 1
        }
        entries["encoder"] = "DJI Live Bridge test video"
        return AmfEcmaArray(entries)
    }
}

private fun openVideo(extractor: MediaExtractor, context: Context, uri: Uri) {
    try {
        extractor.setDataSource(context, uri, null)
    } catch (_: Exception) {
        throw TestVideoException("Video açılamadı. Başka bir video seçmeyi dene.")
    }
}

private fun findTracks(extractor: MediaExtractor): TestVideoTracks {
    var video: Int? = null
    var audio: Int? = null
    var otherVideoMime: String? = null
    for (index in 0 until extractor.trackCount) {
        val mime = extractor.getTrackFormat(index).getString(MediaFormat.KEY_MIME).orEmpty()
        when {
            mime == MediaFormat.MIMETYPE_VIDEO_AVC && video == null -> video = index
            mime.startsWith("video/") && otherVideoMime == null -> otherVideoMime = mime
            mime == MediaFormat.MIMETYPE_AUDIO_AAC && audio == null -> audio = index
        }
    }
    if (video == null) {
        throw TestVideoException(
            when {
                otherVideoMime == MediaFormat.MIMETYPE_VIDEO_HEVC ->
                    "Bu video H.265 (HEVC) ile kaydedilmiş. DJI Fly gibi H.264 gönderebilmek için H.264 bir video seç."
                otherVideoMime != null -> "Bu videonun biçimi desteklenmiyor. H.264 bir video seç."
                else -> "Seçilen dosyada görüntü yok."
            },
        )
    }
    return TestVideoTracks(
        video = video,
        videoFormat = extractor.getTrackFormat(video),
        audio = audio,
        audioFormat = audio?.let(extractor::getTrackFormat),
    )
}

private fun MediaFormat.codecData(name: String): ByteArray? {
    if (!containsKey(name)) return null
    val buffer = getByteBuffer(name)?.duplicate() ?: return null
    return ByteArray(buffer.remaining()).also(buffer::get).takeIf { it.isNotEmpty() }
}

private fun MediaFormat.frameRate(): Double? {
    if (!containsKey(MediaFormat.KEY_FRAME_RATE)) return null
    return runCatching { getInteger(MediaFormat.KEY_FRAME_RATE).toDouble() }.getOrNull()
        ?: runCatching { getFloat(MediaFormat.KEY_FRAME_RATE).toDouble() }.getOrNull()
}

private fun displayName(context: Context, uri: Uri): String? = runCatching {
    context.contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use { cursor ->
        if (cursor.moveToFirst()) cursor.getString(0) else null
    }
}.getOrNull()

/**
 * "drone.mp4 · 00:20 · döngüde". The photo picker hides real file names behind aliases such as
 * "43.mp4", which mean nothing to the user, so those give way to a plain "Video".
 */
internal fun testVideoLabel(displayName: String?, durationMs: Long?): String {
    val name = displayName?.takeUnless { it.substringBeforeLast('.').all(Char::isDigit) } ?: "Video"
    return listOfNotNull(name, durationMs?.let(::formatDuration), "döngüde").joinToString(" · ")
}
