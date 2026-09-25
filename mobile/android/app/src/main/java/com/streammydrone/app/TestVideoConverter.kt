package com.streammydrone.app

import android.content.Context
import android.media.MediaCodecInfo.CodecProfileLevel
import android.media.MediaFormat
import android.media.metrics.LogSessionId
import android.net.Uri
import android.os.Handler
import android.os.Looper
import androidx.annotation.OptIn
import androidx.media3.common.Format
import androidx.media3.common.MediaItem
import androidx.media3.common.MimeTypes
import androidx.media3.common.util.UnstableApi
import androidx.media3.effect.Presentation
import androidx.media3.transformer.Codec
import androidx.media3.transformer.Composition
import androidx.media3.transformer.DefaultEncoderFactory
import androidx.media3.transformer.EditedMediaItem
import androidx.media3.transformer.EditedMediaItemSequence
import androidx.media3.transformer.Effects
import androidx.media3.transformer.ExportException
import androidx.media3.transformer.ExportResult
import androidx.media3.transformer.ProgressHolder
import androidx.media3.transformer.Transformer
import androidx.media3.transformer.VideoEncoderSettings
import java.io.File
import java.security.MessageDigest

/**
 * What DJI Fly streams, and so what a test video is turned into: H.264 with a 720-pixel short
 * side, 30 frames a second, about 4 Mbps and a keyframe every second, with AAC sound. Phone
 * recordings are far heavier (4K at 50–100 Mbps is common), more than the uplink or any platform
 * takes, and the relay forwards a stream as it is.
 */
internal object DroneLikeVideo {
    const val SHORT_SIDE = 720
    const val FRAME_RATE = 30
    const val BITRATE = 4_000_000
    const val KEYFRAME_INTERVAL_SECONDS = 1f

    /**
     * H.264 level 4.0, room for 1080p30. Media3 otherwise asks for the encoder's highest level
     * (6.0 on a Galaxy S23), which a 720p stream has no use for and a platform may refuse.
     */
    const val LEVEL = CodecProfileLevel.AVCLevel4

    /** Only the start is converted: the test video loops, and a long one would take minutes. */
    const val MAX_DURATION_MS = 60_000L

    /** A little above [BITRATE], so a video already like DJI Fly's goes out as it is. */
    const val MAX_BITRATE_AS_IS = 5_000_000L
}

/** The picked video's picture and sound, as far as deciding whether it can go out as it is. */
internal data class VideoFileInfo(
    val mime: String,
    val width: Int,
    val height: Int,
    val rotationDegrees: Int,
    val frameRate: Float?,
    /** Bits per second of the whole file, or null when unknown. */
    val bitrate: Long?,
    val hdr: Boolean,
    /** The first sound track's format, or null for a silent video such as most drone footage. */
    val audioMime: String?,
)

/**
 * Whether the video has to be converted before it can stand in for DJI Fly. FLV carries no
 * rotation, so a video that is displayed rotated is converted too; an unknown bitrate counts as
 * too high. DJI Fly always sends AAC sound, and Instagram and Facebook show nothing of a stream
 * without any, so a silent video gets a silent sound track.
 */
internal fun needsConversion(video: VideoFileInfo): Boolean =
    video.audioMime != MediaFormat.MIMETYPE_AUDIO_AAC ||
        video.mime != MediaFormat.MIMETYPE_VIDEO_AVC ||
        video.rotationDegrees % 360 != 0 ||
        minOf(video.width, video.height) > DroneLikeVideo.SHORT_SIDE ||
        (video.frameRate ?: 0f) > DroneLikeVideo.FRAME_RATE + FRAME_RATE_TOLERANCE ||
        (video.bitrate ?: Long.MAX_VALUE) > DroneLikeVideo.MAX_BITRATE_AS_IS ||
        video.hdr

/** Average bits per second of a file, or null when its size or length is unknown. */
internal fun averageBitrate(sizeBytes: Long?, durationMs: Long?): Long? =
    if (sizeBytes == null || sizeBytes <= 0 || durationMs == null || durationMs <= 0) {
        null
    } else {
        sizeBytes * 8_000 / durationMs
    }

/**
 * Converts a picked video into a [DroneLikeVideo] file in the app's cache. Call it on the main
 * thread; Media3 does the work on its own threads and reports back on the main thread. The last
 * result is kept, so the same video starts at once the next time.
 */
@OptIn(UnstableApi::class)
internal class TestVideoConverter(
    private val context: Context,
    private val onProgress: (percent: Int) -> Unit,
    private val onFinished: (Result<Uri>) -> Unit,
) {
    private val handler = Handler(Looper.getMainLooper())
    private val progress = ProgressHolder()
    private var transformer: Transformer? = null
    private var output: File? = null
    private var cancelled = false

    private val pollProgress = object : Runnable {
        override fun run() {
            val active = transformer ?: return
            if (active.getProgress(progress) == Transformer.PROGRESS_STATE_AVAILABLE) onProgress(progress.progress)
            handler.postDelayed(this, PROGRESS_INTERVAL_MS)
        }
    }

    fun start(source: Uri) {
        val directory = File(context.cacheDir, CACHE_DIRECTORY).apply { mkdirs() }
        val name = cacheName(source)
        val converted = File(directory, "$name.mp4")
        if (converted.isFile) {
            // Posted, so the caller has its reference to this converter before the result.
            handler.post { finish(Result.success(Uri.fromFile(converted))) }
            return
        }
        // One converted video is enough: an older one, or one left half written, goes.
        directory.listFiles()?.forEach(File::delete)
        val partial = File(directory, "$name-partial.mp4").also { output = it }
        val listener = object : Transformer.Listener {
            override fun onCompleted(composition: Composition, exportResult: ExportResult) {
                finish(
                    if (partial.renameTo(converted)) {
                        Result.success(Uri.fromFile(converted))
                    } else {
                        Result.failure(IllegalStateException("The converted video could not be kept"))
                    },
                )
            }

            override fun onError(composition: Composition, exportResult: ExportResult, exportException: ExportException) {
                partial.delete()
                finish(Result.failure(exportException))
            }
        }
        transformer = Transformer.Builder(context)
            .setVideoMimeType(MimeTypes.VIDEO_H264)
            .setAudioMimeType(MimeTypes.AUDIO_AAC)
            // FLV has no rotation field, so a portrait video must be encoded standing up.
            .setPortraitEncodingEnabled(true)
            .setEncoderFactory(
                OutputFrameRateEncoderFactory(
                    DefaultEncoderFactory.Builder(context)
                        .setRequestedVideoEncoderSettings(
                            VideoEncoderSettings.Builder()
                                .setBitrate(DroneLikeVideo.BITRATE)
                                .setiFrameIntervalSeconds(DroneLikeVideo.KEYFRAME_INTERVAL_SECONDS)
                                // Media3 keeps a requested level (and makes the profile High),
                                // or picks its own when the encoder cannot do this one.
                                .setEncodingProfileLevel(CodecProfileLevel.AVCProfileHigh, DroneLikeVideo.LEVEL)
                                .build(),
                        )
                        .build(),
                ),
            )
            .addListener(listener)
            .build()
            .also { it.start(composition(source), partial.path) }
        handler.postDelayed(pollProgress, PROGRESS_INTERVAL_MS)
    }

    /** Stops the conversion; nothing more is reported. */
    fun cancel() {
        cancelled = true
        handler.removeCallbacks(pollProgress)
        transformer?.cancel()
        transformer = null
        output?.delete()
    }

    private fun finish(result: Result<Uri>) {
        handler.removeCallbacks(pollProgress)
        transformer = null
        if (!cancelled) onFinished(result)
    }

    private fun composition(source: Uri): Composition {
        val item = MediaItem.Builder()
            .setUri(source)
            .setClippingConfiguration(
                MediaItem.ClippingConfiguration.Builder().setEndPositionMs(DroneLikeVideo.MAX_DURATION_MS).build(),
            )
            .build()
        val edited = EditedMediaItem.Builder(item)
            // A maximum: frames of a faster video are dropped, a slower one keeps its rate.
            .setFrameRate(DroneLikeVideo.FRAME_RATE)
            .setEffects(Effects(emptyList(), listOf(Presentation.createForShortSide(DroneLikeVideo.SHORT_SIDE))))
            .build()
        // Declaring sound makes Media3 generate silence for a video that has none.
        return Composition.Builder(EditedMediaItemSequence.withAudioAndVideoFrom(listOf(edited)))
            // RTMP carries 8-bit SDR H.264, as DJI Fly sends; an HDR phone video is tone mapped.
            .setHdrMode(Composition.HDR_MODE_TONE_MAP_HDR_TO_SDR_USING_OPEN_GL)
            .build()
    }

    /** The same video with the same settings maps to the same name. */
    private fun cacheName(source: Uri): String {
        val key = listOf(
            source,
            fileSize(context, source),
            DroneLikeVideo.SHORT_SIDE,
            DroneLikeVideo.FRAME_RATE,
            DroneLikeVideo.BITRATE,
            DroneLikeVideo.LEVEL,
            DroneLikeVideo.MAX_DURATION_MS,
            // Earlier conversions kept a silent video silent.
            "with-sound",
        ).joinToString("|")
        val digest = MessageDigest.getInstance("SHA-256").digest(key.toByteArray())
        return digest.take(CACHE_NAME_BYTES).joinToString("") { "%02x".format(it) }
    }
}

/**
 * Tells the video encoder the frame rate it really gets. Media3 drops a 60 fps video's extra
 * frames but configures the encoder with the input's rate, which halves the bitrate and doubles
 * the keyframe interval (measured on the emulator: 2 Mbps and 2 s instead of 4 Mbps and 1 s).
 */
@OptIn(UnstableApi::class)
private class OutputFrameRateEncoderFactory(private val encoders: Codec.EncoderFactory) : Codec.EncoderFactory {
    override fun createForAudioEncoding(format: Format, logSessionId: LogSessionId?): Codec =
        encoders.createForAudioEncoding(format, logSessionId)

    override fun createForVideoEncoding(format: Format, logSessionId: LogSessionId?): Codec {
        val frameRate = DroneLikeVideo.FRAME_RATE.toFloat()
        val output = if (format.frameRate > frameRate) format.buildUpon().setFrameRate(frameRate).build() else format
        return encoders.createForVideoEncoding(output, logSessionId)
    }

    override fun audioNeedsEncoding(): Boolean = encoders.audioNeedsEncoding()

    override fun videoNeedsEncoding(): Boolean = encoders.videoNeedsEncoding()
}

private const val FRAME_RATE_TOLERANCE = 1f
private const val CACHE_DIRECTORY = "test-video"
private const val CACHE_NAME_BYTES = 12
private const val PROGRESS_INTERVAL_MS = 250L
