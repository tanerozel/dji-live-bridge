package com.djilivebridge.android

import android.os.SystemClock
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.VerticalDivider
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.produceState
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.withContext

/**
 * Everything about the stream that does not fit over the picture: the numbers, each platform
 * with its own "End", the tips and the technical details. The drone screen shows it in a sheet.
 */
@Composable
internal fun LiveDetails(
    phase: BridgePhase,
    snapshot: RelaySnapshot,
    /** The platforms the stream goes to; empty while the phone only receives. */
    destinations: List<DestinationProfile>,
    lan: LanAddress?,
    onEndPlatform: (DestinationProfile) -> Unit,
    modifier: Modifier = Modifier,
) {
    Column(modifier = modifier, verticalArrangement = Arrangement.spacedBy(12.dp)) {
        if (phase.isStreaming) StatsCard(snapshot)
        if (destinations.size > 1) PlatformsCard(destinations, snapshot, onEndPlatform)
        liveTips(phase, snapshot, destinations).forEach { TipBox(it) }
        BridgeCard(verticalSpacing = 8.dp) {
            SectionHeader(title = stringResource(R.string.technical_details))
            TechnicalDetailRows(snapshot, destinations, lan)
        }
    }
}

/** What the user can do about the stream's state right now, most urgent first. */
@Composable
internal fun liveTips(phase: BridgePhase, snapshot: RelaySnapshot, destinations: List<DestinationProfile>): List<String> {
    val congested = snapshot.outputs.any { it.status == "congested" }
    val tips = when {
        destinations.isEmpty() -> emptyList()
        phase == BridgePhase.RECONNECTING &&
            snapshot.outputs.any { it.reconnectAttempts >= RECONNECT_HINT_THRESHOLD } -> listOf(R.string.tip_check_key)
        phase == BridgePhase.RECEIVER_ERROR -> listOf(R.string.tip_receiver_error)
        phase == BridgePhase.LIVE && congested -> listOf(R.string.tip_congested)
        phase == BridgePhase.LIVE -> destinations.mapNotNull { it.kind.liveReminder }.distinct()
        else -> emptyList()
    }
    return tips.map { stringResource(it) }
}

/** How the screen names where the stream goes: one platform by name, several by their count. */
internal class LiveTarget(destinations: List<DestinationProfile>) {
    /** The only platform, or null when there are several. */
    val single = destinations.singleOrNull()?.kind
    val count = destinations.size

    /** The tile for the orb; the first platform when there are several. */
    val kind = destinations.firstOrNull()?.kind ?: DestinationKind.CUSTOM
}

@Composable
internal fun LiveTarget.label(): String = single?.displayName() ?: pluralStringResource(R.plurals.platform_count, count, count)


@Composable
internal fun LiveTimer(
    sinceElapsedMillis: Long,
    color: Color = Color.Unspecified,
    style: TextStyle = MaterialTheme.typography.titleLarge,
) {
    val now by produceState(SystemClock.elapsedRealtime(), sinceElapsedMillis) {
        while (true) {
            value = SystemClock.elapsedRealtime()
            delay(1_000)
        }
    }
    val elapsed = formatDuration(now - sinceElapsedMillis)
    val description = stringResource(R.string.stream_time, elapsed)
    Text(
        modifier = Modifier.semantics { contentDescription = description },
        text = elapsed,
        color = color,
        style = style.copy(fontFeatureSettings = "tnum"),
    )
}

@Composable
private fun StatsCard(snapshot: RelaySnapshot) {
    val codecs = listOfNotNull(snapshot.videoCodec, snapshot.audioCodec)
        .joinToString(" · ", transform = ::friendlyCodecName)
        .ifEmpty { "—" }
    BridgeCard(contentPadding = PaddingValues(vertical = 16.dp, horizontal = 8.dp)) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .height(IntrinsicSize.Min),
        ) {
            Stat(label = stringResource(R.string.stat_bitrate), value = formatBitrate(snapshot.bitrateKbps), modifier = Modifier.weight(1f))
            VerticalDivider(color = BridgeTheme.colors.border)
            Stat(label = stringResource(R.string.stat_sent), value = formatBytes(snapshot.outboundBytes), modifier = Modifier.weight(1f))
            VerticalDivider(color = BridgeTheme.colors.border)
            Stat(label = stringResource(R.string.stat_codec), value = codecs, modifier = Modifier.weight(1f))
        }
    }
}

@Composable
private fun Stat(label: String, value: String, modifier: Modifier = Modifier) {
    Column(
        modifier = modifier.semantics(mergeDescendants = true) {},
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(2.dp),
    ) {
        Text(
            text = value,
            style = MaterialTheme.typography.titleMedium.copy(fontFeatureSettings = "tnum"),
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
        Text(text = label, style = MaterialTheme.typography.labelSmall, color = BridgeTheme.colors.muted)
    }
}

/** Each platform with its own state, and a way to end just that one. */
@Composable
private fun PlatformsCard(
    destinations: List<DestinationProfile>,
    snapshot: RelaySnapshot,
    onEnd: (DestinationProfile) -> Unit,
) {
    val colors = BridgeTheme.colors
    BridgeCard(verticalSpacing = 10.dp) {
        SectionHeader(title = stringResource(R.string.platforms))
        destinations.forEach { profile ->
            val (label, tone) = outputLabel(snapshot.output(profile.id)?.status, colors)
            val name = profile.kind.displayName()
            Row(
                modifier = Modifier.semantics(mergeDescendants = true) {},
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                PlatformTile(kind = profile.kind, size = 32.dp)
                Column(modifier = Modifier.weight(1f)) {
                    Text(
                        text = name,
                        style = MaterialTheme.typography.bodyMedium,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                    Row(
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(6.dp),
                    ) {
                        Box(
                            modifier = Modifier
                                .size(8.dp)
                                .background(tone, CircleShape),
                        )
                        Text(text = stringResource(label), style = MaterialTheme.typography.labelMedium, color = colors.muted)
                    }
                }
                val endLabel = stringResource(R.string.end_platform, name)
                TextButton(onClick = { onEnd(profile) }, modifier = Modifier.semantics { contentDescription = endLabel }) {
                    Text(stringResource(R.string.end), color = colors.dangerText)
                }
            }
        }
    }
}

internal fun outputLabel(status: String?, colors: BridgePalette): Pair<Int, Color> = when (status) {
    "forwarding" -> R.string.status_live to colors.success
    "congested" -> R.string.status_congested to colors.warningText
    "ready", "connecting" -> R.string.status_connecting to colors.accent
    "reconnecting" -> R.string.status_reconnecting to colors.warningText
    "holding" -> R.string.status_holding to colors.warningText
    "stopped", "error" -> R.string.status_stopped to colors.dangerText
    else -> R.string.status_waiting_for_picture to colors.faint
}

/** What one platform's connection is doing, for the technical details. */
@Composable
private fun outputStateText(output: OutputSnapshot): String = when (output.status) {
    "armed" -> stringResource(R.string.output_armed)
    "connecting" -> stringResource(R.string.output_connecting, if (output.secure) "RTMPS" else "RTMP")
    "ready" -> stringResource(if (output.secure) R.string.output_ready_secure else R.string.output_ready)
    "forwarding" -> stringResource(if (output.secure) R.string.output_forwarding_secure else R.string.output_forwarding)
    "congested" -> stringResource(R.string.output_congested)
    "holding" -> stringResource(R.string.output_holding)
    "reconnecting" -> output.retryInSeconds?.toInt()?.let { pluralStringResource(R.plurals.output_retry, it, it) }
        ?: stringResource(R.string.status_reconnecting)
    "stopped" -> stringResource(R.string.output_stopped)
    else -> output.status
}

/** Why the last attempt failed; the technical cause from the relay stays in English, in brackets. */
@Composable
private fun outputFailureText(output: OutputSnapshot): String? {
    val reason = output.reason ?: return null
    val text = when (reason) {
        "tls_or_network" -> R.string.output_failure_tls
        "connect_failed" -> R.string.output_failure_connect
        "publish_rejected" -> R.string.output_failure_rejected
        "send_failed" -> R.string.output_failure_send
        "network" -> R.string.output_failure_network
        "stalled" -> R.string.output_failure_stalled
        else -> null
    }?.let { stringResource(it) } ?: reason
    return output.reasonDetail?.let { stringResource(R.string.error_with_detail, text, it) } ?: text
}

@Composable
private fun TechnicalDetailRows(snapshot: RelaySnapshot, destinations: List<DestinationProfile>, lan: LanAddress?) {
    DetailRow(stringResource(R.string.detail_source), snapshot.remoteAddress ?: stringResource(R.string.detail_source_waiting))
    DetailRow(stringResource(R.string.detail_received), formatBytes(snapshot.receivedBytes))
    DetailRow(stringResource(R.string.detail_packets), "${snapshot.videoFrames} / ${snapshot.audioFrames}")
    // Where short drops come from: pauses in what arrives, DJI Fly starting over, the phone's Wi-Fi.
    DetailRow(
        stringResource(R.string.detail_stalls),
        if (snapshot.stalls > 0) {
            stringResource(R.string.detail_stalls_value, snapshot.stalls, formatSeconds(snapshot.longestStallMs))
        } else {
            "0"
        },
    )
    DetailRow(stringResource(R.string.detail_source_reconnects), snapshot.sourceReconnects.toString())
    PhoneWifiRow(lan)
    if (snapshot.rejectedPublishAttempts > 0) {
        DetailRow(stringResource(R.string.detail_rejected), snapshot.rejectedPublishAttempts.toString())
    }
    destinations.forEach { profile ->
        val output = snapshot.output(profile.id) ?: return@forEach
        HorizontalDivider(color = BridgeTheme.colors.border)
        Text(
            modifier = Modifier.padding(top = 8.dp),
            text = profile.kind.displayName(),
            style = MaterialTheme.typography.labelLarge,
        )
        DetailRow(stringResource(R.string.detail_forwarded), formatBytes(output.outboundBytes))
        DetailRow(
            stringResource(R.string.detail_connection),
            stringResource(
                when {
                    output.secure && output.status in LIVE_OUTPUT_STATUSES + "ready" -> R.string.connection_rtmps_verified
                    output.secure -> R.string.connection_rtmps_pending
                    else -> R.string.connection_rtmp
                },
            ),
        )
        DetailRow(stringResource(R.string.detail_status), outputStateText(output))
        outputFailureText(output)?.let { DetailRow(stringResource(R.string.detail_last_error), it) }
        if (output.reconnectAttempts > 0) {
            DetailRow(stringResource(R.string.detail_reconnects), output.reconnectAttempts.toString())
        }
        if (output.droppedFrames > 0) {
            DetailRow(stringResource(R.string.detail_dropped), output.droppedFrames.toString())
        }
    }
    HorizontalDivider(color = BridgeTheme.colors.border)
    Text(
        text = stringResource(R.string.background_note),
        style = MaterialTheme.typography.bodySmall,
        color = BridgeTheme.colors.muted,
    )
}

/** The phone's end of the Wi-Fi the picture comes over, refreshed while it shows. */
@Composable
private fun PhoneWifiRow(lan: LanAddress?) {
    val context = LocalContext.current
    val link by produceState<WifiLink?>(initialValue = null, context) {
        while (true) {
            value = withContext(Dispatchers.IO) { currentWifiLink(context) }
            delay(WIFI_REFRESH_MS)
        }
    }
    val value = link?.let { formatWifiLink(it) }
        ?: stringResource(R.string.detail_phone_wifi_hotspot).takeIf { lan?.kind == LanKind.HOTSPOT }
        ?: return
    DetailRow(stringResource(R.string.detail_phone_wifi), value)
}

private const val RECONNECT_HINT_THRESHOLD = 3
private const val WIFI_REFRESH_MS = 2_000L
