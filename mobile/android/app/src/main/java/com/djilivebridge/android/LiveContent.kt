package com.djilivebridge.android

import android.os.SystemClock
import androidx.annotation.StringRes
import androidx.compose.animation.core.LinearOutSlowInEasing
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.rounded.KeyboardArrowRight
import androidx.compose.material.icons.rounded.ErrorOutline
import androidx.compose.material.icons.rounded.Movie
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.VerticalDivider
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.vector.rememberVectorPainter
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.delay

/** The screen while the bridge runs: one status, the next action, and the numbers that matter. */
@Composable
internal fun LiveContent(
    phase: BridgePhase,
    snapshot: RelaySnapshot,
    liveSinceElapsedMillis: Long?,
    testVideoName: UiText?,
    /** The platforms the stream goes to. */
    destinations: List<DestinationProfile>,
    lan: LanAddress?,
    onCopyAddress: (String) -> Unit,
    onEndPlatform: (DestinationProfile) -> Unit,
    modifier: Modifier = Modifier,
) {
    val target = LiveTarget(destinations)
    Column(
        modifier = modifier,
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        StatusHero(
            phase = phase,
            snapshot = snapshot,
            liveSinceElapsedMillis = liveSinceElapsedMillis,
            target = target,
            testVideoName = testVideoName,
        )
        val waitingForSource = phase == BridgePhase.STARTING ||
            phase == BridgePhase.WAITING_FOR_DRONE ||
            phase == BridgePhase.DRONE_CONNECTED
        when {
            // A test video connects by itself; DJI Fly instructions would only confuse.
            waitingForSource && testVideoName == null -> DjiFlyCard(lan = lan, onCopyAddress = onCopyAddress)
            phase.isStreaming -> StatsCard(snapshot)
        }
        if (destinations.size > 1) PlatformsCard(destinations, snapshot, onEndPlatform)
        val congested = snapshot.outputs.any { it.status == "congested" }
        when {
            phase == BridgePhase.RECONNECTING &&
                snapshot.outputs.any { it.reconnectAttempts >= RECONNECT_HINT_THRESHOLD } ->
                TipBox(stringResource(R.string.tip_check_key))
            phase == BridgePhase.RECEIVER_ERROR ->
                TipBox(stringResource(R.string.tip_receiver_error))
            phase == BridgePhase.LIVE && congested ->
                TipBox(stringResource(R.string.tip_congested))
            phase == BridgePhase.LIVE ->
                destinations.mapNotNull { it.kind.liveReminder }.distinct().forEach { TipBox(stringResource(it)) }
        }
        TechnicalDetails(snapshot, destinations)
    }
}

/** How the screen names where the stream goes: one platform by name, several by their count. */
private class LiveTarget(destinations: List<DestinationProfile>) {
    /** The only platform, or null when there are several. */
    val single = destinations.singleOrNull()?.kind
    val count = destinations.size

    /** The tile for the orb; the first platform when there are several. */
    val kind = destinations.firstOrNull()?.kind ?: DestinationKind.CUSTOM
}

@Composable
private fun LiveTarget.label(): String = single?.displayName() ?: pluralStringResource(R.plurals.platform_count, count, count)

/** A sentence about going to the one platform ("to Instagram"), or its version for several. */
@Composable
private fun LiveTarget.toSentence(@StringRes one: Int, @StringRes many: Int): String =
    single?.let { stringResource(one, stringResource(it.toPlatform)).sentenceStart() } ?: stringResource(many)

private class HeroLook(
    val tone: Color,
    val halo: Color,
    val title: String,
    val subtitle: String,
    val ping: Boolean = true,
)

@Composable
private fun StatusHero(
    phase: BridgePhase,
    snapshot: RelaySnapshot,
    liveSinceElapsedMillis: Long?,
    target: LiveTarget,
    testVideoName: UiText?,
) {
    val colors = BridgeTheme.colors
    val testing = testVideoName != null
    val waitingForSource = phase == BridgePhase.WAITING_FOR_DRONE || phase == BridgePhase.DRONE_CONNECTED
    val error = snapshot.error?.asString().orEmpty()
    val look = if (testing && waitingForSource) {
        HeroLook(
            colors.accent,
            colors.accentSoft,
            stringResource(R.string.hero_test_video),
            stringResource(R.string.hero_test_video_subtitle),
        )
    } else if (waitingForSource && snapshot.outputStatus == "holding") {
        HeroLook(
            colors.warningText,
            colors.warningSoft,
            stringResource(R.string.hero_drone_lost),
            stringResource(R.string.hero_drone_lost_subtitle),
        )
    } else when (phase) {
        BridgePhase.STARTING -> HeroLook(
            colors.accent,
            colors.accentSoft,
            stringResource(R.string.hero_starting),
            stringResource(R.string.hero_starting_subtitle),
        )
        BridgePhase.WAITING_FOR_DRONE -> HeroLook(
            colors.warningText,
            colors.warningSoft,
            stringResource(R.string.hero_waiting),
            target.toSentence(R.string.hero_waiting_subtitle_one, R.string.hero_waiting_subtitle_many),
        )
        BridgePhase.DRONE_CONNECTED -> HeroLook(
            colors.accent,
            colors.accentSoft,
            stringResource(R.string.hero_connected),
            stringResource(R.string.hero_connected_subtitle),
        )
        // Only for a moment: the live screen shows once the stream is sent somewhere.
        BridgePhase.PREVIEW, BridgePhase.CONNECTING_TARGET -> HeroLook(
            colors.accent,
            colors.accentSoft,
            target.toSentence(R.string.hero_connecting_one, R.string.hero_connecting_many),
            "",
        )
        BridgePhase.LIVE -> HeroLook(
            colors.live,
            colors.dangerSoft,
            target.single?.let { stringResource(R.string.hero_live_one, stringResource(it.onPlatform)).sentenceStart() }
                ?: pluralStringResource(R.plurals.hero_live_many, target.count, target.count),
            "",
        )
        BridgePhase.RECONNECTING -> HeroLook(
            colors.warningText,
            colors.warningSoft,
            stringResource(R.string.hero_connection_lost),
            target.toSentence(R.string.hero_reconnecting_one, R.string.hero_reconnecting_many),
        )
        BridgePhase.RECEIVER_ERROR ->
            HeroLook(colors.dangerText, colors.dangerSoft, stringResource(R.string.hero_receiver_stopped), error, ping = false)
        BridgePhase.IDLE, BridgePhase.START_FAILED ->
            HeroLook(colors.faint, colors.field, stringResource(R.string.hero_off), error, ping = false)
    }
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(top = 8.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        // Once the drone's video arrives, the picture itself is the status.
        if (phase.isStreaming) {
            DronePreview(cornerColor = colors.background, modifier = Modifier.padding(bottom = 4.dp)) {
                PreviewOverlay(phase = phase, liveSinceElapsedMillis = liveSinceElapsedMillis)
            }
        } else StatusOrb(tone = look.tone, halo = look.halo, ping = look.ping) {
            when (phase) {
                BridgePhase.STARTING -> CircularProgressIndicator(
                    modifier = Modifier.size(32.dp),
                    color = look.tone,
                    strokeWidth = 3.dp,
                )
                BridgePhase.WAITING_FOR_DRONE, BridgePhase.DRONE_CONNECTED -> Icon(
                    painter = if (testing) rememberVectorPainter(Icons.Rounded.Movie) else painterResource(R.drawable.ic_drone),
                    contentDescription = null,
                    tint = look.tone,
                    modifier = Modifier.size(40.dp),
                )
                BridgePhase.RECEIVER_ERROR -> Icon(
                    imageVector = Icons.Rounded.ErrorOutline,
                    contentDescription = null,
                    tint = look.tone,
                    modifier = Modifier.size(40.dp),
                )
                else -> PlatformTile(kind = target.kind, size = 52.dp)
            }
        }
        // Only the status line is announced; the ticking timer would talk every second.
        Text(
            modifier = Modifier.semantics { liveRegion = LiveRegionMode.Polite },
            text = look.title,
            style = if (phase.isStreaming) MaterialTheme.typography.titleMedium else MaterialTheme.typography.titleLarge,
            textAlign = TextAlign.Center,
        )
        if (look.subtitle.isNotEmpty()) {
            Text(
                text = look.subtitle,
                style = MaterialTheme.typography.bodyMedium,
                color = colors.muted,
                textAlign = TextAlign.Center,
            )
        }
        HopLine(
            phase = phase,
            targetLabel = target.label(),
            sourceLabel = stringResource(if (testing) R.string.hop_test_video else R.string.hop_controller),
        )
        testVideoName?.let { name ->
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(6.dp),
            ) {
                Icon(Icons.Rounded.Movie, contentDescription = null, tint = colors.faint, modifier = Modifier.size(16.dp))
                Text(
                    text = name.asString(),
                    style = MaterialTheme.typography.labelMedium,
                    color = colors.muted,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
    }
}

/** A soft halo with an expanding ring while something is in progress or live. */
@Composable
private fun StatusOrb(tone: Color, halo: Color, ping: Boolean, content: @Composable BoxScope.() -> Unit) {
    val colors = BridgeTheme.colors
    val progress = if (ping) {
        rememberInfiniteTransition(label = "orb").animateFloat(
            initialValue = 0f,
            targetValue = 1f,
            animationSpec = infiniteRepeatable(tween(durationMillis = 2_000, easing = LinearOutSlowInEasing)),
            label = "orbPing",
        )
    } else {
        null
    }
    Box(modifier = Modifier.size(140.dp), contentAlignment = Alignment.Center) {
        if (progress != null) {
            Canvas(modifier = Modifier.size(140.dp)) {
                val start = 52.dp.toPx()
                val end = size.minDimension / 2 - 1.dp.toPx()
                val value = progress.value
                drawCircle(
                    color = tone.copy(alpha = 0.5f * (1f - value)),
                    radius = start + (end - start) * value,
                    style = Stroke(width = 2.dp.toPx()),
                )
            }
        }
        Box(
            modifier = Modifier
                .size(104.dp)
                .background(halo, CircleShape),
            contentAlignment = Alignment.Center,
        ) {
            Box(
                modifier = Modifier
                    .size(78.dp)
                    .background(colors.card, CircleShape),
                contentAlignment = Alignment.Center,
                content = content,
            )
        }
    }
}

/** Live badge and running time over the top-left corner of the picture. */
@Composable
private fun BoxScope.PreviewOverlay(phase: BridgePhase, liveSinceElapsedMillis: Long?) {
    Row(
        modifier = Modifier
            .align(Alignment.TopStart)
            .padding(12.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        when (phase) {
            BridgePhase.LIVE -> {
                LiveBadge()
                liveSinceElapsedMillis?.let { since ->
                    OverlayChip { LiveTimer(since, color = Color.White, style = MaterialTheme.typography.labelLarge) }
                }
            }
            BridgePhase.CONNECTING_TARGET -> OverlayChip { OverlayText(stringResource(R.string.status_connecting)) }
            BridgePhase.RECONNECTING -> OverlayChip { OverlayText(stringResource(R.string.status_reconnecting)) }
            else -> Unit
        }
    }
}

@Composable
private fun LiveTimer(
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

/** Source → phone → platform in one line; each dot is colored by that hop's state. */
@Composable
private fun HopLine(phase: BridgePhase, targetLabel: String, sourceLabel: String) {
    val colors = BridgeTheme.colors
    val (sourceColor, sourceState) = when (phase) {
        BridgePhase.WAITING_FOR_DRONE -> colors.warningText to R.string.hop_waiting
        BridgePhase.DRONE_CONNECTED -> colors.accent to R.string.hop_connected
        BridgePhase.PREVIEW, BridgePhase.CONNECTING_TARGET, BridgePhase.LIVE, BridgePhase.RECONNECTING ->
            colors.success to R.string.hop_sending
        else -> colors.faint to R.string.hop_not_connected
    }
    val (relayColor, relayState) = when (phase) {
        BridgePhase.STARTING -> colors.accent to R.string.hop_starting
        BridgePhase.RECEIVER_ERROR -> colors.dangerText to R.string.hop_error
        else -> colors.success to R.string.hop_ready
    }
    val (targetColor, targetState) = when (phase) {
        BridgePhase.LIVE -> colors.success to R.string.hop_live
        BridgePhase.CONNECTING_TARGET -> colors.accent to R.string.hop_connecting
        BridgePhase.RECONNECTING -> colors.warningText to R.string.hop_reconnecting
        else -> colors.faint to R.string.hop_standing_by
    }
    val phoneLabel = stringResource(R.string.hop_phone)
    val description = stringResource(
        R.string.hop_description,
        sourceLabel,
        stringResource(sourceState),
        phoneLabel,
        stringResource(relayState),
        targetLabel,
        stringResource(targetState),
    )
    Row(
        modifier = Modifier.clearAndSetSemantics { contentDescription = description },
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Hop(sourceLabel, sourceColor)
        HopArrow()
        Hop(phoneLabel, relayColor)
        HopArrow()
        Hop(targetLabel, targetColor)
    }
}

@Composable
private fun Hop(label: String, color: Color) {
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
        Box(
            modifier = Modifier
                .size(8.dp)
                .background(color, CircleShape),
        )
        Text(
            text = label,
            style = MaterialTheme.typography.labelMedium,
            color = BridgeTheme.colors.muted,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

@Composable
private fun HopArrow() {
    Icon(
        imageVector = Icons.AutoMirrored.Rounded.KeyboardArrowRight,
        contentDescription = null,
        tint = BridgeTheme.colors.faint,
        modifier = Modifier.size(16.dp),
    )
}

@Composable
private fun DjiFlyCard(lan: LanAddress?, onCopyAddress: (String) -> Unit) {
    var troubleshootingOpen by rememberSaveable { mutableStateOf(false) }
    BridgeCard(verticalSpacing = 12.dp) {
        SectionHeader(title = stringResource(R.string.start_on_controller))
        Text(text = stringResource(R.string.dji_fly_path), style = MaterialTheme.typography.bodySmall, color = BridgeTheme.colors.muted)
        if (lan != null) {
            AddressField(address = lan.publishUrl, onCopy = { onCopyAddress(lan.publishUrl) })
        } else {
            AlertBanner(
                title = stringResource(R.string.phone_offline),
                message = stringResource(R.string.phone_offline_message),
            )
        }
        ExpandableSection(
            title = stringResource(R.string.not_connecting),
            expanded = troubleshootingOpen,
            onToggle = { troubleshootingOpen = !troubleshootingOpen },
        ) {
            BulletItem(stringResource(R.string.not_connecting_same_network))
            BulletItem(stringResource(R.string.not_connecting_exact_address))
            BulletItem(stringResource(R.string.not_connecting_guest_vpn))
        }
    }
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

private fun outputLabel(status: String?, colors: BridgePalette): Pair<Int, Color> = when (status) {
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
private fun TechnicalDetails(snapshot: RelaySnapshot, destinations: List<DestinationProfile>) {
    var expanded by rememberSaveable { mutableStateOf(false) }
    BridgeCard(contentPadding = PaddingValues(horizontal = 16.dp, vertical = 4.dp), verticalSpacing = 0.dp) {
        ExpandableSection(title = stringResource(R.string.technical_details), expanded = expanded, onToggle = { expanded = !expanded }) {
            DetailRow(stringResource(R.string.detail_source), snapshot.remoteAddress ?: stringResource(R.string.detail_source_waiting))
            DetailRow(stringResource(R.string.detail_received), formatBytes(snapshot.receivedBytes))
            DetailRow(stringResource(R.string.detail_packets), "${snapshot.videoFrames} / ${snapshot.audioFrames}")
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
    }
}

private const val RECONNECT_HINT_THRESHOLD = 3
