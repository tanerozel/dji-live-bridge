package com.djilivebridge.android

import android.graphics.Bitmap
import android.media.MediaCodec
import android.media.MediaCodecInfo
import android.media.MediaCodecList
import android.media.MediaFormat
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.os.Process
import android.view.PixelCopy
import android.view.Surface
import androidx.compose.animation.Crossfade
import androidx.compose.animation.core.tween
import androidx.compose.foundation.AndroidExternalSurface
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.requiredSize
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.BlurredEdgeTreatment
import androidx.compose.ui.draw.blur
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.dp
import androidx.core.graphics.createBitmap
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.suspendCancellableCoroutine
import java.nio.ByteBuffer
import kotlin.concurrent.thread
import kotlin.coroutines.resume

/** How the drone's picture sits on a screen of another shape. */
internal enum class PictureFit(val storageValue: String) {
    /** The whole picture; the rest of the screen shows its colors, blurred. */
    WHOLE("whole"),

    /** Edge to edge, like a camera app; the picture's sides (or top and bottom) are cut off. */
    FILL("fill"),
    ;

    companion object {
        fun fromStorage(value: String?): PictureFit = entries.firstOrNull { it.storageValue == value } ?: WHOLE
    }
}

/** What the decoder found out about the picture, for the screen around it. */
@Stable
internal class DronePictureState {
    /** The decoded picture's size, or null until the first frame. */
    var videoSize by mutableStateOf<IntSize?>(null)
        internal set
    var unsupported by mutableStateOf(false)
        internal set

    /** A tiny copy of a recent frame, blurred into the space around a [PictureFit.WHOLE] picture. */
    internal var backdrop by mutableStateOf<ImageBitmap?>(null)
    internal var sampling = false
}

@Composable
internal fun rememberDronePictureState(): DronePictureState = remember { DronePictureState() }

/**
 * The drone's picture across the whole of [modifier]'s area, decoded on the phone from the relay's
 * preview tap. It only decodes while the app is visible; the stream to the platform never waits
 * for it.
 *
 * The picture is a SurfaceView, which the system composites directly: no extra GPU copy and one
 * frame less delay than a TextureView. It is sized to the video's shape, inside the area or, for
 * [PictureFit.FILL], beyond it; the screen's edges cut off what sticks out, since such a surface
 * ignores clipping.
 */
@Composable
internal fun DronePicture(
    state: DronePictureState,
    fit: PictureFit,
    modifier: Modifier = Modifier,
) {
    val pictureDescription = stringResource(R.string.drone_picture)
    var area by remember { mutableStateOf(IntSize.Zero) }
    val density = LocalDensity.current
    SideEffect { state.sampling = fit == PictureFit.WHOLE }
    Box(
        modifier = modifier
            .background(Color.Black)
            .onSizeChanged { area = it }
            .semantics { contentDescription = pictureDescription },
        contentAlignment = Alignment.Center,
    ) {
        if (fit == PictureFit.WHOLE) AmbientBackdrop(state.backdrop)
        val aspect = state.videoSize?.let { it.width.toFloat() / it.height }
            ?.coerceIn(MIN_ASPECT_RATIO, MAX_ASPECT_RATIO)
            ?: DEFAULT_ASPECT_RATIO
        val (width, height) = pictureSize(area.width.toFloat(), area.height.toFloat(), aspect, fit)
        // The surface goes away while the app is in the background and comes back with it; the
        // decoder follows it. Removing the view instead would leave the new one's surface unplaced.
        if (area != IntSize.Zero) {
            val size = with(density) { Modifier.requiredSize(width.toDp(), height.toDp()) }
            AndroidExternalSurface(modifier = size) {
                onSurface { surface, _, _ ->
                    val decoder = DronePreviewDecoder(
                        surface = surface,
                        onVideoSize = { videoWidth, videoHeight -> launch { state.videoSize = IntSize(videoWidth, videoHeight) } },
                        onUnsupported = { launch { state.unsupported = true } },
                    )
                    val sampler = launch { sampleBackdrop(surface, state) }
                    try {
                        awaitCancellation()
                    } finally {
                        sampler.cancel()
                        decoder.stop()
                    }
                }
            }
        }
        if (state.videoSize == null) {
            Column(
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                if (!state.unsupported) {
                    CircularProgressIndicator(modifier = Modifier.size(22.dp), color = Color.White, strokeWidth = 2.dp)
                }
                Text(
                    text = stringResource(if (state.unsupported) R.string.preview_unsupported else R.string.preview_waiting),
                    style = MaterialTheme.typography.bodySmall,
                    color = Color.White.copy(alpha = 0.8f),
                    textAlign = TextAlign.Center,
                )
            }
        }
    }
}

/**
 * The picture's size in an area of [width] by [height] for a video of [aspect] (width / height):
 * as large as fits for [PictureFit.WHOLE], as small as covers it for [PictureFit.FILL].
 */
internal fun pictureSize(width: Float, height: Float, aspect: Float, fit: PictureFit): Pair<Float, Float> {
    val widerThanArea = aspect > width / height
    val fullWidth = if (fit == PictureFit.WHOLE) widerThanArea else !widerThanArea
    return if (fullWidth) width to width / aspect else height * aspect to height
}

/** The recent frame, blurred and dimmed, so the picture seems to fill the screen. */
@Composable
private fun AmbientBackdrop(frame: ImageBitmap?) {
    Crossfade(targetState = frame, animationSpec = tween(BACKDROP_FADE_MS), label = "backdrop") { image ->
        if (image != null) {
            // A 32×18 copy stretched over the screen is already soft; the blur (Android 12+)
            // smooths it further.
            Image(
                bitmap = image,
                contentDescription = null,
                contentScale = ContentScale.Crop,
                modifier = Modifier
                    .fillMaxSize()
                    .blur(BACKDROP_BLUR, BlurredEdgeTreatment.Rectangle),
            )
        }
    }
    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(Color.Black.copy(alpha = BACKDROP_DIM)),
    )
}

/** Copies a small version of the frame on screen every so often, while a backdrop is shown. */
private suspend fun sampleBackdrop(surface: Surface, state: DronePictureState) {
    val bitmap = createBitmap(BACKDROP_WIDTH, BACKDROP_HEIGHT)
    val handler = Handler(Looper.getMainLooper())
    while (true) {
        delay(BACKDROP_INTERVAL_MS)
        if (!state.sampling || state.videoSize == null || !surface.isValid) continue
        val copied = suspendCancellableCoroutine { continuation ->
            try {
                PixelCopy.request(
                    surface,
                    bitmap,
                    { result -> if (continuation.isActive) continuation.resume(result == PixelCopy.SUCCESS) },
                    handler,
                )
            } catch (_: IllegalArgumentException) {
                // The surface went away between the check and the copy.
                continuation.resume(false)
            }
        }
        if (copied) state.backdrop = bitmap.copy(Bitmap.Config.ARGB_8888, false).asImageBitmap()
    }
}

/** A dark label that stays readable over any picture. */
@Composable
internal fun OverlayChip(modifier: Modifier = Modifier, content: @Composable () -> Unit) {
    Box(
        modifier = modifier
            .background(OverlayGlass, RoundedCornerShape(12.dp))
            .padding(horizontal = 12.dp, vertical = 8.dp),
    ) {
        content()
    }
}

@Composable
internal fun OverlayText(text: String) {
    Text(text = text, color = Color.White, style = MaterialTheme.typography.labelLarge)
}

/** The see-through dark background of everything drawn over the picture. */
internal val OverlayGlass = Color.Black.copy(alpha = 0.55f)

/**
 * Decodes the relay's H.264 tags onto [surface] with as little delay as the phone allows: a
 * hardware decoder in low-latency mode, input and output on separate threads so each frame is
 * shown the moment it is decoded, and frames far behind the newest input skipped, so after the
 * cached GOP is replayed at start-up the picture jumps straight to live.
 */
internal class DronePreviewDecoder(
    private val surface: Surface,
    private val onVideoSize: (width: Int, height: Int) -> Unit,
    private val onUnsupported: () -> Unit,
) {
    @Volatile private var running = true

    /** Presentation time of the newest frame given to the decoder. */
    @Volatile private var newestInputUs = 0L
    private val session = NativeRelay.nativePreviewStart()
    private val feeder = thread(name = "drone-preview-in", isDaemon = true) { feed() }

    /** Stops decoding before the surface goes away; returns within a fraction of a second. */
    fun stop() {
        running = false
        NativeRelay.nativePreviewStop(session)
        feeder.join(STOP_TIMEOUT_MS)
    }

    private fun feed() {
        Process.setThreadPriority(Process.THREAD_PRIORITY_DISPLAY)
        var output: Output? = null
        var config: AvcDecoderConfig? = null
        var configRecord: ByteArray? = null
        var waitingForKeyframe = true
        var unsupportedTags = 0
        try {
            while (running) {
                val packet = NativeRelay.nativePreviewNext(session, POLL_TIMEOUT_MS) ?: continue
                if (packet.size < 4) continue
                val timestampMs = ByteBuffer.wrap(packet, 0, 4).int.toLong() and 0xFFFFFFFFL
                when (val tag = parseFlvVideoTag(packet, offset = 4)) {
                    is FlvVideoTag.Config -> if (configRecord?.contentEquals(tag.record) != true) {
                        val parsed = parseAvcConfigurationRecord(tag.record) ?: continue
                        output?.close()
                        output = null
                        output = Output(createDecoder(parsed))
                        config = parsed
                        configRecord = tag.record
                        waitingForKeyframe = true
                    }
                    is FlvVideoTag.Picture -> {
                        val current = output ?: continue
                        val currentConfig = config ?: continue
                        if (waitingForKeyframe && !tag.keyframe) continue
                        try {
                            waitingForKeyframe = !queue(current.codec, tag, currentConfig.nalLengthSize, timestampMs)
                        } catch (_: IllegalStateException) {
                            // Includes CodecException: a fresh decoder resumes at the next keyframe.
                            current.close()
                            output = null
                            output = Output(createDecoder(currentConfig))
                            waitingForKeyframe = true
                        }
                    }
                    FlvVideoTag.Unsupported -> {
                        unsupportedTags++
                        if (unsupportedTags == UNSUPPORTED_REPORT_AFTER && output == null) onUnsupported()
                    }
                    null -> Unit
                }
            }
        } catch (_: Exception) {
            // A decoder that cannot be created, or a surface that went away, ends the preview only.
        } finally {
            NativeRelay.nativePreviewStop(session)
            output?.close()
        }
    }

    /** Returns false when the frame had to be skipped. */
    private fun queue(codec: MediaCodec, picture: FlvVideoTag.Picture, nalLengthSize: Int, timestampMs: Long): Boolean {
        val index = codec.dequeueInputBuffer(INPUT_TIMEOUT_US)
        if (index < 0) return false
        val size = codec.getInputBuffer(index)?.let { buffer -> putAnnexB(picture.data, nalLengthSize, buffer) } ?: -1
        if (size <= 0) {
            codec.queueInputBuffer(index, 0, 0, 0, 0)
            return false
        }
        val presentationUs = (timestampMs + picture.compositionTimeMs) * 1_000
        // Set first: the frame may come out of the decoder before queueInputBuffer returns.
        newestInputUs = presentationUs
        codec.queueInputBuffer(index, 0, size, presentationUs, 0)
        return true
    }

    private fun createDecoder(config: AvcDecoderConfig): MediaCodec {
        fun format() = MediaFormat.createVideoFormat(MediaFormat.MIMETYPE_VIDEO_AVC, DEFAULT_WIDTH, DEFAULT_HEIGHT).apply {
            setByteBuffer("csd-0", ByteBuffer.wrap(withStartCodes(config.sps)))
            setByteBuffer("csd-1", ByteBuffer.wrap(withStartCodes(config.pps)))
            setInteger(MediaFormat.KEY_MAX_WIDTH, MAX_WIDTH)
            setInteger(MediaFormat.KEY_MAX_HEIGHT, MAX_HEIGHT)
        }
        val decoder = lowLatencyDecoder
        val decodeOrder = config.sps.firstOrNull()?.let(::avcOutputsInDecodeOrder) == true
        val tuned = format().also { tuneForLowLatency(it, decoder, decodeOrder) }
        // A decoder that rejects the tuning still decodes with the plain format.
        return startDecoder(decoder, tuned) ?: startDecoder(decoder, format())
            ?: throw IllegalStateException("AVC decoder could not start")
    }

    private fun startDecoder(decoder: MediaCodecInfo?, format: MediaFormat): MediaCodec? {
        val codec = decoder?.let { runCatching { MediaCodec.createByCodecName(it.name) }.getOrNull() }
            ?: MediaCodec.createDecoderByType(MediaFormat.MIMETYPE_VIDEO_AVC)
        return try {
            codec.configure(format, surface, null, 0)
            codec.start()
            codec
        } catch (_: Exception) {
            codec.release()
            null
        }
    }

    private fun reportSize(format: MediaFormat) {
        val width = if (format.containsKey("crop-left") && format.containsKey("crop-right")) {
            format.getInteger("crop-right") - format.getInteger("crop-left") + 1
        } else {
            format.getInteger(MediaFormat.KEY_WIDTH)
        }
        val height = if (format.containsKey("crop-top") && format.containsKey("crop-bottom")) {
            format.getInteger("crop-bottom") - format.getInteger("crop-top") + 1
        } else {
            format.getInteger(MediaFormat.KEY_HEIGHT)
        }
        if (width > 0 && height > 0) onVideoSize(width, height)
    }

    /** One decoder and the thread that shows its frames as soon as they are decoded. */
    private inner class Output(val codec: MediaCodec) {
        @Volatile private var open = true
        private val renderer = thread(name = "drone-preview-out", isDaemon = true) { render() }

        fun close() {
            open = false
            renderer.join(STOP_TIMEOUT_MS)
            codec.release()
        }

        private fun render() {
            Process.setThreadPriority(Process.THREAD_PRIORITY_URGENT_DISPLAY)
            var info = MediaCodec.BufferInfo()
            var newer = MediaCodec.BufferInfo()
            try {
                while (open) {
                    var index = codec.dequeueOutputBuffer(info, OUTPUT_TIMEOUT_US)
                    if (index == MediaCodec.INFO_OUTPUT_FORMAT_CHANGED) reportSize(codec.outputFormat)
                    if (index < 0) continue
                    // Only the newest decoded frame is worth showing; older ones would only queue
                    // up on the screen behind it.
                    while (true) {
                        val next = codec.dequeueOutputBuffer(newer, 0)
                        if (next == MediaCodec.INFO_OUTPUT_FORMAT_CHANGED) reportSize(codec.outputFormat)
                        if (next < 0) break
                        codec.releaseOutputBuffer(index, false)
                        index = next
                        info = newer.also { newer = info }
                    }
                    // Far behind the newest input means a catch-up burst: skip it, don't replay it.
                    if (newestInputUs - info.presentationTimeUs <= LATE_FRAME_US) {
                        codec.releaseOutputBuffer(index, System.nanoTime())
                    } else {
                        codec.releaseOutputBuffer(index, false)
                    }
                }
            } catch (_: IllegalStateException) {
                // The decoder failed or its surface went away; the feeder starts a new one.
            }
        }
    }

    private companion object {
        const val POLL_TIMEOUT_MS = 100
        const val STOP_TIMEOUT_MS = 500L
        const val INPUT_TIMEOUT_US = 100_000L
        const val OUTPUT_TIMEOUT_US = 10_000L
        const val LATE_FRAME_US = 100_000L
        const val UNSUPPORTED_REPORT_AFTER = 30

        // A starting size only; the decoder takes the real one from the SPS. DJI Fly sends up to 1080p.
        const val DEFAULT_WIDTH = 1920
        const val DEFAULT_HEIGHT = 1080
        const val MAX_WIDTH = 3840
        const val MAX_HEIGHT = 2160

        /** Hardware first, then one with a low-latency mode; the platform's order breaks ties. */
        val lowLatencyDecoder: MediaCodecInfo? by lazy {
            runCatching {
                MediaCodecList(MediaCodecList.REGULAR_CODECS).codecInfos
                    .filter { info ->
                        !info.isEncoder &&
                            info.supportedTypes.any { it.equals(MediaFormat.MIMETYPE_VIDEO_AVC, ignoreCase = true) }
                    }
                    .maxByOrNull { info -> (if (info.isHardware()) 2 else 0) + (if (info.supportsLowLatency()) 1 else 0) }
            }.getOrNull()
        }
    }
}

private fun MediaCodecInfo.isHardware(): Boolean =
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
        isHardwareAccelerated
    } else {
        SOFTWARE_DECODER_PREFIXES.none { name.startsWith(it, ignoreCase = true) }
    }

private fun MediaCodecInfo.supportsLowLatency(): Boolean =
    Build.VERSION.SDK_INT >= Build.VERSION_CODES.R && runCatching {
        getCapabilitiesForType(MediaFormat.MIMETYPE_VIDEO_AVC)
            .isFeatureSupported(MediaCodecInfo.CodecCapabilities.FEATURE_LowLatency)
    }.getOrDefault(false)

/**
 * Asks the decoder to hand out each frame as soon as it is decoded, the way Moonlight's game
 * streaming client does. Codec2 decoders ignore keys they do not declare; an older decoder that
 * rejects one fails configure(), and the caller retries without them. Decode-order output is only
 * safe when the stream never reorders.
 */
private fun tuneForLowLatency(format: MediaFormat, decoder: MediaCodecInfo?, decodeOrder: Boolean) {
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
        format.setInteger(MediaFormat.KEY_LOW_LATENCY, 1)
        // MediaTek's and some Amlogic decoders read their own key when they do not advertise it.
        if (decoder?.supportsLowLatency() != true) format.setInteger("vdec-lowlatency", 1)
    }
    val name = decoder?.name?.lowercase().orEmpty()
    val qualcomm = name.startsWith("c2.qti") || name.startsWith("omx.qcom")
    // Realtime priority, which Moonlight gives every decoder but Qualcomm's: those get a raised
    // operating rate there instead (never both), which some Adreno 620 phones cannot take.
    if (!qualcomm) format.setInteger(MediaFormat.KEY_PRIORITY, 0)
    when {
        qualcomm -> {
            format.setInteger("vendor.qti-ext-dec-low-latency.enable", 1)
            if (decodeOrder) format.setInteger("vendor.qti-ext-dec-picture-order.enable", 1)
        }
        name.startsWith("c2.exynos") || name.startsWith("omx.exynos") ->
            format.setInteger("vendor.rtc-ext-dec-low-latency.enable", 1)
        name.startsWith("c2.hisi") || name.startsWith("omx.hisi") -> {
            format.setInteger("vendor.hisi-ext-low-latency-video-dec.video-scene-for-low-latency-req", 1)
            format.setInteger("vendor.hisi-ext-low-latency-video-dec.video-scene-for-low-latency-rdy", -1)
        }
        name.startsWith("c2.amlogic") || name.startsWith("omx.amlogic") ->
            format.setInteger("vendor.low-latency.enable", 1)
    }
}

private val SOFTWARE_DECODER_PREFIXES = listOf("omx.google.", "c2.android.", "c2.google.")

private const val DEFAULT_ASPECT_RATIO = 16f / 9f
private const val MIN_ASPECT_RATIO = 0.5f
private const val MAX_ASPECT_RATIO = 2.4f

private const val BACKDROP_WIDTH = 32
private const val BACKDROP_HEIGHT = 18
private const val BACKDROP_INTERVAL_MS = 700L
private const val BACKDROP_FADE_MS = 500
private const val BACKDROP_DIM = 0.45f
private val BACKDROP_BLUR = 40.dp
