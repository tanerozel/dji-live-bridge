package com.djilivebridge.android

import android.media.MediaCodec
import android.media.MediaFormat
import android.os.Build
import android.view.Surface
import androidx.compose.foundation.AndroidEmbeddedExternalSurface
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.compose.currentStateAsState
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.launch
import java.nio.ByteBuffer
import kotlin.concurrent.thread

/**
 * The drone's picture, decoded on the phone from the relay's preview tap. It only decodes while
 * the app is visible; the stream to the platform never waits for it.
 */
@Composable
internal fun DronePreview(modifier: Modifier = Modifier, overlay: @Composable BoxScope.() -> Unit = {}) {
    var aspectRatio by remember { mutableFloatStateOf(16f / 9f) }
    var showingPicture by remember { mutableStateOf(false) }
    var unsupported by remember { mutableStateOf(false) }
    val lifecycle by LocalLifecycleOwner.current.lifecycle.currentStateAsState()
    Box(
        modifier = modifier
            .fillMaxWidth()
            .aspectRatio(aspectRatio.coerceIn(MIN_ASPECT_RATIO, MAX_ASPECT_RATIO))
            .clip(MaterialTheme.shapes.large)
            .background(Color.Black)
            .semantics { contentDescription = "Drone görüntüsü" },
        contentAlignment = Alignment.Center,
    ) {
        if (lifecycle.isAtLeast(Lifecycle.State.STARTED)) {
            AndroidEmbeddedExternalSurface(modifier = Modifier.matchParentSize()) {
                onSurface { surface, _, _ ->
                    val decoder = DronePreviewDecoder(
                        surface = surface,
                        onVideoSize = { width, height ->
                            launch {
                                aspectRatio = width.toFloat() / height
                                showingPicture = true
                            }
                        },
                        onUnsupported = { launch { unsupported = true } },
                    )
                    try {
                        awaitCancellation()
                    } finally {
                        decoder.stop()
                    }
                }
            }
        }
        if (!showingPicture) {
            Column(
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                if (!unsupported) {
                    CircularProgressIndicator(modifier = Modifier.size(22.dp), color = Color.White, strokeWidth = 2.dp)
                }
                Text(
                    text = if (unsupported) "Önizleme bu görüntü biçimini gösteremiyor" else "Görüntü bekleniyor…",
                    style = MaterialTheme.typography.bodySmall,
                    color = Color.White.copy(alpha = 0.8f),
                    textAlign = TextAlign.Center,
                )
            }
        }
        overlay()
    }
}

/** A dark label that stays readable over any picture. */
@Composable
internal fun OverlayChip(modifier: Modifier = Modifier, content: @Composable () -> Unit) {
    Box(
        modifier = modifier
            .background(Color.Black.copy(alpha = 0.55f), RoundedCornerShape(8.dp))
            .padding(horizontal = 10.dp, vertical = 5.dp),
    ) {
        content()
    }
}

@Composable
internal fun OverlayText(text: String) {
    Text(text = text, color = Color.White, style = MaterialTheme.typography.labelLarge)
}

/**
 * Feeds the relay's H.264 tags to a hardware decoder that renders straight onto [surface].
 * Frames are shown as soon as they decode; a frame the decoder cannot take in time is skipped
 * and decoding resumes at the next keyframe, so the preview stays live instead of falling behind.
 */
internal class DronePreviewDecoder(
    private val surface: Surface,
    private val onVideoSize: (width: Int, height: Int) -> Unit,
    private val onUnsupported: () -> Unit,
) {
    @Volatile private var running = true
    private val session = NativeRelay.nativePreviewStart()
    private val worker = thread(name = "drone-preview", isDaemon = true) { run() }

    /** Stops decoding before the surface goes away; returns within a fraction of a second. */
    fun stop() {
        running = false
        NativeRelay.nativePreviewStop(session)
        worker.join(STOP_TIMEOUT_MS)
    }

    private fun run() {
        var decoder: MediaCodec? = null
        var config: AvcDecoderConfig? = null
        var configRecord: ByteArray? = null
        var waitingForKeyframe = true
        var unsupportedTags = 0
        val info = MediaCodec.BufferInfo()
        try {
            while (running) {
                val packet = NativeRelay.nativePreviewNext(session, POLL_TIMEOUT_MS)
                if (packet == null || packet.size < 4) {
                    decoder?.let { drain(it, info) }
                    continue
                }
                val timestampMs = ByteBuffer.wrap(packet, 0, 4).int.toLong() and 0xFFFFFFFFL
                when (val tag = parseFlvVideoTag(packet, offset = 4)) {
                    is FlvVideoTag.Config -> {
                        if (configRecord?.contentEquals(tag.record) != true) {
                            val parsed = parseAvcConfigurationRecord(tag.record) ?: continue
                            decoder?.release()
                            decoder = null
                            decoder = createDecoder(parsed)
                            config = parsed
                            configRecord = tag.record
                        }
                        waitingForKeyframe = true
                    }
                    is FlvVideoTag.Picture -> {
                        val codec = decoder ?: continue
                        val nalLengthSize = config?.nalLengthSize ?: continue
                        if (waitingForKeyframe && !tag.keyframe) continue
                        try {
                            waitingForKeyframe = !queue(codec, tag, nalLengthSize, timestampMs)
                            drain(codec, info)
                        } catch (_: IllegalStateException) {
                            // Includes CodecException: rebuild and resume at the next keyframe.
                            codec.release()
                            decoder = null
                            decoder = createDecoder(config)
                            waitingForKeyframe = true
                        }
                    }
                    FlvVideoTag.Unsupported -> {
                        unsupportedTags++
                        if (unsupportedTags == UNSUPPORTED_REPORT_AFTER && decoder == null) onUnsupported()
                    }
                    null -> Unit
                }
            }
        } catch (_: Exception) {
            // A decoder that cannot be created, or a surface that went away, ends the preview only.
        } finally {
            NativeRelay.nativePreviewStop(session)
            decoder?.release()
        }
    }

    private fun createDecoder(config: AvcDecoderConfig): MediaCodec {
        val format = MediaFormat.createVideoFormat(MediaFormat.MIMETYPE_VIDEO_AVC, DEFAULT_WIDTH, DEFAULT_HEIGHT).apply {
            setByteBuffer("csd-0", ByteBuffer.wrap(withStartCodes(config.sps)))
            setByteBuffer("csd-1", ByteBuffer.wrap(withStartCodes(config.pps)))
            setInteger(MediaFormat.KEY_MAX_WIDTH, MAX_WIDTH)
            setInteger(MediaFormat.KEY_MAX_HEIGHT, MAX_HEIGHT)
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) setInteger(MediaFormat.KEY_LOW_LATENCY, 1)
        }
        val codec = MediaCodec.createDecoderByType(MediaFormat.MIMETYPE_VIDEO_AVC)
        try {
            codec.configure(format, surface, null, 0)
            codec.start()
        } catch (error: Exception) {
            codec.release()
            throw error
        }
        return codec
    }

    /** Returns false when the frame had to be skipped. */
    private fun queue(codec: MediaCodec, picture: FlvVideoTag.Picture, nalLengthSize: Int, timestampMs: Long): Boolean {
        val index = codec.dequeueInputBuffer(INPUT_TIMEOUT_US)
        if (index < 0) return false
        val buffer = codec.getInputBuffer(index)
        val data = avccToAnnexB(picture.data, nalLengthSize)
        if (buffer == null || data.isEmpty() || data.size > buffer.capacity()) {
            codec.queueInputBuffer(index, 0, 0, 0, 0)
            return false
        }
        buffer.clear()
        buffer.put(data)
        codec.queueInputBuffer(index, 0, data.size, (timestampMs + picture.compositionTimeMs) * 1_000, 0)
        return true
    }

    private fun drain(codec: MediaCodec, info: MediaCodec.BufferInfo) {
        while (true) {
            val index = codec.dequeueOutputBuffer(info, 0)
            when {
                index >= 0 -> codec.releaseOutputBuffer(index, true)
                index == MediaCodec.INFO_OUTPUT_FORMAT_CHANGED -> reportSize(codec.outputFormat)
                else -> return
            }
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

    private companion object {
        const val POLL_TIMEOUT_MS = 100
        const val STOP_TIMEOUT_MS = 500L
        const val INPUT_TIMEOUT_US = 30_000L
        const val UNSUPPORTED_REPORT_AFTER = 30

        // A starting size only; the decoder takes the real one from the SPS. DJI Fly sends up to 1080p.
        const val DEFAULT_WIDTH = 1920
        const val DEFAULT_HEIGHT = 1080
        const val MAX_WIDTH = 3840
        const val MAX_HEIGHT = 2160
    }
}

private const val MIN_ASPECT_RATIO = 0.5f
private const val MAX_ASPECT_RATIO = 2.4f
