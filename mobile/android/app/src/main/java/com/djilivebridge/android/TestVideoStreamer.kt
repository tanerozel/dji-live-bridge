package com.djilivebridge.android

import android.content.Context
import android.media.MediaExtractor
import android.media.MediaFormat
import android.net.Uri
import android.os.SystemClock
import android.provider.OpenableColumns
import java.nio.ByteBuffer

/** A test video problem the user can act on, in words for the screen. */
internal class TestVideoException(val text: UiText) : Exception()

/**
 * A video the user picked to stand in for the drone. With [convert] it is first turned into what
 * DJI Fly sends ([TestVideoConverter]); [durationMs] is how much of it plays.
 */
internal class TestVideoSelection(val uri: Uri, val displayName: String?, val durationMs: Long?, val convert: Boolean)

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

/**
 * Checks a picked video before the bridge starts, so a file without a picture is reported right
 * away, and decides whether it must be converted first.
 */
internal fun inspectTestVideo(context: Context, uri: Uri): TestVideoSelection {
    val extractor = MediaExtractor()
    try {
        openVideo(extractor, context, uri)
        val formats = (0 until extractor.trackCount).map(extractor::getTrackFormat)
        fun firstOf(kind: String) = formats.firstOrNull { it.getString(MediaFormat.KEY_MIME).orEmpty().startsWith(kind) }
        val format = firstOf("video/") ?: throw TestVideoException(uiText(R.string.test_video_no_video))
        val durationMs = if (format.containsKey(MediaFormat.KEY_DURATION)) format.getLong(MediaFormat.KEY_DURATION) / 1_000 else null
        val audioMime = firstOf("audio/")?.getString(MediaFormat.KEY_MIME)
        val convert = needsConversion(videoFileInfo(format, averageBitrate(fileSize(context, uri), durationMs), audioMime))
        val playedMs = if (convert) durationMs?.coerceAtMost(DroneLikeVideo.MAX_DURATION_MS) else durationMs
        return TestVideoSelection(uri, displayName(context, uri), playedMs, convert)
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
            ) ?: throw TestVideoException(uiText(R.string.test_video_no_avc_config))
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
        if (videoTimes.isEmpty()) throw TestVideoException(uiText(R.string.test_video_no_frames))
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
                if (extractor.sampleTrackIndex < 0) throw TestVideoException(uiText(R.string.test_video_rewind_failed))
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
        entries["encoder"] = "StreamMyDrone test video"
        return AmfEcmaArray(entries)
    }
}

private fun openVideo(extractor: MediaExtractor, context: Context, uri: Uri) {
    try {
        extractor.setDataSource(context, uri, null)
    } catch (_: Exception) {
        throw TestVideoException(uiText(R.string.test_video_open_failed))
    }
}

/** The H.264 picture and AAC sound to send; a video in any other format is converted first. */
private fun findTracks(extractor: MediaExtractor): TestVideoTracks {
    var video: Int? = null
    var audio: Int? = null
    for (index in 0 until extractor.trackCount) {
        val mime = extractor.getTrackFormat(index).getString(MediaFormat.KEY_MIME).orEmpty()
        when {
            mime == MediaFormat.MIMETYPE_VIDEO_AVC && video == null -> video = index
            mime == MediaFormat.MIMETYPE_AUDIO_AAC && audio == null -> audio = index
        }
    }
    if (video == null) throw TestVideoException(uiText(R.string.test_video_unsupported))
    return TestVideoTracks(
        video = video,
        videoFormat = extractor.getTrackFormat(video),
        audio = audio,
        audioFormat = audio?.let(extractor::getTrackFormat),
    )
}

private fun videoFileInfo(format: MediaFormat, fileBitrate: Long?, audioMime: String?): VideoFileInfo {
    // A key stored with another type means nothing to this check, not a broken file.
    fun integer(key: String) = if (format.containsKey(key)) runCatching { format.getInteger(key) }.getOrNull() else null
    val transfer = integer(MediaFormat.KEY_COLOR_TRANSFER)
    return VideoFileInfo(
        mime = format.getString(MediaFormat.KEY_MIME).orEmpty(),
        width = integer(MediaFormat.KEY_WIDTH) ?: 0,
        height = integer(MediaFormat.KEY_HEIGHT) ?: 0,
        rotationDegrees = integer(MediaFormat.KEY_ROTATION) ?: 0,
        frameRate = format.frameRate()?.toFloat(),
        bitrate = integer(MediaFormat.KEY_BIT_RATE)?.toLong()?.takeIf { it > 0 } ?: fileBitrate,
        hdr = transfer == MediaFormat.COLOR_TRANSFER_ST2084 || transfer == MediaFormat.COLOR_TRANSFER_HLG,
        audioMime = audioMime,
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

/** The file's size in bytes, or null when the provider does not say. */
internal fun fileSize(context: Context, uri: Uri): Long? = runCatching {
    context.contentResolver.query(uri, arrayOf(OpenableColumns.SIZE), null, null, null)?.use { cursor ->
        if (cursor.moveToFirst() && !cursor.isNull(0)) cursor.getLong(0).takeIf { it > 0 } else null
    }
}.getOrNull()

private fun displayName(context: Context, uri: Uri): String? = runCatching {
    context.contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use { cursor ->
        if (cursor.moveToFirst()) cursor.getString(0) else null
    }
}.getOrNull()

/**
 * "drone.mp4 · 00:20 · looping". The photo picker hides real file names behind aliases such as
 * "43.mp4", which mean nothing to the user, so those give way to [fallbackName].
 */
internal fun testVideoLabel(displayName: String?, durationMs: Long?): UiText {
    val name = displayName?.takeUnless { it.substringBeforeLast('.').all(Char::isDigit) }
    return UiText.Joined(
        listOfNotNull(
            name?.let(UiText::Raw) ?: uiText(R.string.test_video_default_name),
            durationMs?.let { UiText.Raw(formatDuration(it)) },
            uiText(R.string.test_video_looping),
        ),
    )
}
