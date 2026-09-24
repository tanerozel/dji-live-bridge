package com.djilivebridge.android

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Check
import androidx.compose.material.icons.rounded.Movie
import androidx.compose.material.icons.rounded.Stop
import androidx.compose.material.icons.rounded.Wifi
import androidx.compose.material.icons.rounded.WifiOff
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp

private const val GRID_COLUMNS = 4

/**
 * Before going live: the drone connects first and its picture shows here, then the user picks
 * where the stream goes.
 */
@Composable
internal fun SetupContent(
    phase: BridgePhase,
    snapshot: RelaySnapshot,
    testVideoName: UiText?,
    notice: RelayNotice?,
    destinations: DestinationProfiles,
    profileError: UiText?,
    lan: LanAddress?,
    onStartReceiver: (restart: Boolean) -> Unit,
    onTestVideo: () -> Unit,
    onStopTestVideo: () -> Unit,
    onPlatformClick: (DestinationKind) -> Unit,
    onPlatformLongClick: (DestinationKind) -> Unit,
    onEditProfile: (DestinationProfile) -> Unit,
    onCopyAddress: (String) -> Unit,
    onOpenWifiSettings: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val selected = destinations.selectedProfiles
    Column(modifier = modifier, verticalArrangement = Arrangement.spacedBy(12.dp)) {
        notice?.let { AlertBanner(title = it.title.asString(), message = it.message.asString()) }
        profileError?.let { AlertBanner(title = stringResource(R.string.profile_list_unreadable), message = it.asString()) }

        DroneCard(
            phase = phase,
            snapshot = snapshot,
            testVideoName = testVideoName,
            lan = lan,
            onStartReceiver = onStartReceiver,
            onTestVideo = onTestVideo,
            onStopTestVideo = onStopTestVideo,
            onCopyAddress = onCopyAddress,
            onOpenWifiSettings = onOpenWifiSettings,
        )

        BridgeCard {
            SectionHeader(title = stringResource(R.string.where_to_stream), step = 2, done = selected.isNotEmpty())
            PlatformGrid(
                destinations = destinations,
                onClick = onPlatformClick,
                onLongClick = onPlatformLongClick,
            )
            if (selected.isNotEmpty()) {
                HorizontalDivider(color = BridgeTheme.colors.border)
                selected.forEach { profile ->
                    SelectedDestination(profile = profile, onEdit = { onEditProfile(profile) })
                }
                if (selected.size > 1) UploadNote(platforms = selected.size, bitrateKbps = snapshot.bitrateKbps)
            } else {
                Text(
                    text = stringResource(R.string.pick_platforms_hint),
                    style = MaterialTheme.typography.bodySmall,
                    color = BridgeTheme.colors.muted,
                )
            }
        }
    }
}

/** Step one: the address for DJI Fly until the drone connects, then the drone's own picture. */
@Composable
private fun DroneCard(
    phase: BridgePhase,
    snapshot: RelaySnapshot,
    testVideoName: UiText?,
    lan: LanAddress?,
    onStartReceiver: (restart: Boolean) -> Unit,
    onTestVideo: () -> Unit,
    onStopTestVideo: () -> Unit,
    onCopyAddress: (String) -> Unit,
    onOpenWifiSettings: () -> Unit,
) {
    val colors = BridgeTheme.colors
    val testing = testVideoName != null
    BridgeCard(verticalSpacing = 12.dp) {
        SectionHeader(title = stringResource(R.string.connect_drone), step = 1, done = phase.hasPicture)
        when (phase) {
            BridgePhase.PREVIEW, BridgePhase.DRONE_CONNECTED -> {
                DronePreview(cornerColor = colors.card) {
                    OverlayChip(modifier = Modifier.align(Alignment.TopStart).padding(12.dp)) {
                        OverlayText(stringResource(if (testing) R.string.preview_badge_test else R.string.preview_badge))
                    }
                }
                Text(
                    text = when {
                        phase != BridgePhase.PREVIEW -> stringResource(R.string.preview_connected)
                        testing -> stringResource(R.string.preview_test_status, formatBitrate(snapshot.bitrateKbps))
                        else -> stringResource(R.string.preview_drone_status, formatBitrate(snapshot.bitrateKbps))
                    },
                    style = MaterialTheme.typography.bodySmall,
                    color = colors.muted,
                )
                if (testing) {
                    TextButton(onClick = onStopTestVideo) {
                        Icon(Icons.Rounded.Stop, contentDescription = null, modifier = Modifier.size(18.dp))
                        Spacer(Modifier.width(8.dp))
                        Text(stringResource(R.string.stop_test_video))
                    }
                }
            }
            BridgePhase.STARTING -> Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                CircularProgressIndicator(modifier = Modifier.size(20.dp), strokeWidth = 2.dp)
                Text(stringResource(R.string.receiver_starting), style = MaterialTheme.typography.bodyMedium)
            }
            BridgePhase.IDLE, BridgePhase.START_FAILED, BridgePhase.RECEIVER_ERROR -> {
                val failed = phase != BridgePhase.IDLE
                Text(
                    text = if (failed) snapshot.error?.asString().orEmpty() else stringResource(R.string.receiver_off),
                    style = MaterialTheme.typography.bodyMedium,
                    color = if (failed) colors.dangerText else colors.text,
                )
                TextButton(onClick = { onStartReceiver(phase == BridgePhase.RECEIVER_ERROR) }) {
                    Text(stringResource(if (failed) R.string.retry else R.string.open_receiver))
                }
            }
            else -> {
                if (testing) {
                    Row(
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(12.dp),
                    ) {
                        CircularProgressIndicator(modifier = Modifier.size(20.dp), strokeWidth = 2.dp)
                        Text(stringResource(R.string.test_video_opening), style = MaterialTheme.typography.bodyMedium)
                    }
                } else if (lan != null) {
                    Text(
                        text = stringResource(R.string.dji_fly_instructions),
                        style = MaterialTheme.typography.bodyMedium,
                    )
                    AddressField(address = lan.publishUrl, onCopy = { onCopyAddress(lan.publishUrl) })
                    Text(text = stringResource(R.string.dji_fly_path), style = MaterialTheme.typography.bodySmall, color = colors.muted)
                    NetworkNote(lan)
                } else {
                    NoNetwork(onOpenWifiSettings)
                }
                if (!testing) {
                    TextButton(onClick = onTestVideo) {
                        Icon(Icons.Rounded.Movie, contentDescription = null, modifier = Modifier.size(18.dp))
                        Spacer(Modifier.width(8.dp))
                        Text(stringResource(R.string.try_test_video))
                    }
                }
            }
        }
    }
}

@Composable
private fun PlatformGrid(
    destinations: DestinationProfiles,
    onClick: (DestinationKind) -> Unit,
    onLongClick: (DestinationKind) -> Unit,
) {
    val selectedKinds = destinations.selectedProfiles.mapTo(mutableSetOf()) { it.kind }
    val configured = destinations.profiles.mapTo(mutableSetOf()) { it.kind }
    BoxWithConstraints(modifier = Modifier.fillMaxWidth()) {
        val cellWidth = maxWidth / GRID_COLUMNS
        Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
            DestinationKind.entries.chunked(GRID_COLUMNS).forEach { row ->
                Row(modifier = Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.Center) {
                    row.forEach { kind ->
                        PlatformCell(
                            kind = kind,
                            configured = kind in configured,
                            selected = kind in selectedKinds,
                            onClick = { onClick(kind) },
                            onLongClick = { onLongClick(kind) },
                            modifier = Modifier.width(cellWidth),
                        )
                    }
                }
            }
        }
    }
}

@Composable
private fun PlatformCell(
    kind: DestinationKind,
    configured: Boolean,
    selected: Boolean,
    onClick: () -> Unit,
    onLongClick: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val colors = BridgeTheme.colors
    val editLabel = stringResource(R.string.edit)
    val savedState = stringResource(R.string.state_saved)
    val notAddedState = stringResource(R.string.state_not_added)
    Column(
        modifier = modifier
            .clip(MaterialTheme.shapes.medium)
            .combinedClickable(
                onClickLabel = stringResource(
                    when {
                        selected -> R.string.deselect
                        configured -> R.string.select
                        else -> R.string.add
                    },
                ),
                role = Role.Button,
                onLongClickLabel = if (configured) editLabel else null,
                onLongClick = if (configured) onLongClick else null,
                onClick = onClick,
            )
            .semantics {
                this.selected = selected
                stateDescription = if (configured) savedState else notAddedState
            }
            .padding(vertical = 6.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Box(modifier = Modifier.size(64.dp), contentAlignment = Alignment.Center) {
            if (selected) {
                Box(
                    modifier = Modifier
                        .matchParentSize()
                        .border(2.dp, colors.accent, RoundedCornerShape(20.dp)),
                )
            }
            PlatformTile(kind = kind, size = 52.dp)
            when {
                selected -> Box(
                    modifier = Modifier
                        .align(Alignment.TopEnd)
                        .offset(x = 2.dp, y = (-2).dp)
                        .size(20.dp)
                        .background(colors.accent, CircleShape)
                        .border(2.dp, colors.card, CircleShape),
                    contentAlignment = Alignment.Center,
                ) {
                    Icon(Icons.Rounded.Check, contentDescription = null, tint = Color.White, modifier = Modifier.size(12.dp))
                }
                // Saved but not selected: a quiet dot instead of a badge on every tile.
                configured -> Box(
                    modifier = Modifier
                        .align(Alignment.BottomEnd)
                        .offset(x = (-1).dp, y = (-1).dp)
                        .size(13.dp)
                        .background(colors.success, CircleShape)
                        .border(2.dp, colors.card, CircleShape),
                )
            }
        }
        Text(
            text = kind.displayName(),
            style = MaterialTheme.typography.labelMedium,
            fontWeight = if (selected) FontWeight.SemiBold else FontWeight.Medium,
            color = if (configured) colors.text else colors.muted,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

@Composable
private fun SelectedDestination(profile: DestinationProfile, onEdit: () -> Unit) {
    val colors = BridgeTheme.colors
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        PlatformTile(kind = profile.kind, size = 32.dp)
        Column(modifier = Modifier.weight(1f)) {
            Text(
                text = stringResource(R.string.platform_selected, profile.kind.displayName()),
                style = MaterialTheme.typography.bodyMedium,
                fontWeight = FontWeight.Medium,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            Text(
                text = if (profile.kind.keyChangesEachStream) {
                    stringResource(R.string.key_changes_each_stream)
                } else {
                    stringResource(R.string.key_saved)
                },
                style = MaterialTheme.typography.bodySmall,
                color = colors.muted,
            )
        }
        TextButton(onClick = onEdit) {
            Text(stringResource(if (profile.kind.keyChangesEachStream) R.string.update_key else R.string.edit))
        }
    }
}

/** Several platforms each upload the whole stream, which the phone's uplink has to carry. */
@Composable
private fun UploadNote(platforms: Int, bitrateKbps: Double) {
    Text(
        text = if (bitrateKbps > 0) {
            pluralStringResource(R.plurals.upload_note_total, platforms, platforms, formatBitrate(bitrateKbps * platforms))
        } else {
            pluralStringResource(R.plurals.upload_note, platforms, platforms)
        },
        style = MaterialTheme.typography.bodySmall,
        color = BridgeTheme.colors.muted,
    )
}

@Composable
private fun NetworkNote(lan: LanAddress) {
    val colors = BridgeTheme.colors
    Row(
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Icon(Icons.Rounded.Wifi, contentDescription = null, tint = colors.successText, modifier = Modifier.size(16.dp))
        Text(
            text = when (lan.kind) {
                LanKind.HOTSPOT -> stringResource(R.string.network_hotspot)
                LanKind.WIFI -> stringResource(R.string.network_wifi)
                LanKind.WIRED, LanKind.OTHER -> stringResource(R.string.network_local)
            },
            style = MaterialTheme.typography.bodySmall,
            color = colors.muted,
        )
    }
}

@Composable
private fun NoNetwork(onOpenWifiSettings: () -> Unit) {
    val colors = BridgeTheme.colors
    Row(
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Icon(Icons.Rounded.WifiOff, contentDescription = null, tint = colors.warningText, modifier = Modifier.size(20.dp))
        Text(
            modifier = Modifier.weight(1f),
            text = stringResource(R.string.no_network),
            style = MaterialTheme.typography.bodyMedium,
        )
    }
    TextButton(modifier = Modifier.padding(start = 20.dp), onClick = onOpenWifiSettings) { Text(stringResource(R.string.wifi_settings)) }
}
