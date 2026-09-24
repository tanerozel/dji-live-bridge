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
    testVideoName: String?,
    notice: RelayNotice?,
    destinations: DestinationProfiles,
    profileError: String?,
    lan: LanAddress?,
    onStartReceiver: (restart: Boolean) -> Unit,
    onTestVideo: () -> Unit,
    onStopTestVideo: () -> Unit,
    onPlatformClick: (DestinationKind) -> Unit,
    onPlatformLongClick: (DestinationKind) -> Unit,
    onEditSelected: () -> Unit,
    onCopyAddress: (String) -> Unit,
    onOpenWifiSettings: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val selected = destinations.selectedProfile
    Column(modifier = modifier, verticalArrangement = Arrangement.spacedBy(12.dp)) {
        notice?.let { AlertBanner(title = it.title, message = it.message) }
        profileError?.let { AlertBanner(title = "Hedef kaydı okunamadı", message = it) }

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
            SectionHeader(title = "Nereye yayın yapacaksın?", step = 2, done = selected != null)
            PlatformGrid(
                destinations = destinations,
                onClick = onPlatformClick,
                onLongClick = onPlatformLongClick,
            )
            if (selected != null) {
                HorizontalDivider(color = BridgeTheme.colors.border)
                SelectedDestination(profile = selected, onEdit = onEditSelected)
            } else {
                Text(
                    text = "Bir platforma dokun. Sunucu adresi hazır; yalnızca yayın anahtarını gireceksin.",
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
    testVideoName: String?,
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
        SectionHeader(title = "Drone'u bağla", step = 1, done = phase.hasPicture)
        when (phase) {
            BridgePhase.PREVIEW, BridgePhase.DRONE_CONNECTED -> {
                DronePreview {
                    OverlayChip(modifier = Modifier.align(Alignment.TopStart).padding(12.dp)) {
                        OverlayText(if (testing) "Test videosu · önizleme" else "Önizleme")
                    }
                }
                Text(
                    text = if (phase == BridgePhase.PREVIEW) {
                        listOf(
                            if (testing) "Test videosu geliyor" else "Drone bağlı",
                            formatBitrate(snapshot.bitrateKbps),
                        ).joinToString(" · ") + ". Henüz yayında değilsin."
                    } else {
                        "Kumanda bağlandı; görüntü birazdan gelir."
                    },
                    style = MaterialTheme.typography.bodySmall,
                    color = colors.muted,
                )
                if (testing) {
                    TextButton(onClick = onStopTestVideo) {
                        Icon(Icons.Rounded.Stop, contentDescription = null, modifier = Modifier.size(18.dp))
                        Spacer(Modifier.width(8.dp))
                        Text("Test videosunu durdur")
                    }
                }
            }
            BridgePhase.STARTING -> Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                CircularProgressIndicator(modifier = Modifier.size(20.dp), strokeWidth = 2.dp)
                Text("Alıcı hazırlanıyor…", style = MaterialTheme.typography.bodyMedium)
            }
            BridgePhase.IDLE, BridgePhase.START_FAILED, BridgePhase.RECEIVER_ERROR -> {
                val failed = phase != BridgePhase.IDLE
                Text(
                    text = if (failed) snapshot.detail else "Alıcı kapalı; DJI Fly bu telefona bağlanamaz.",
                    style = MaterialTheme.typography.bodyMedium,
                    color = if (failed) colors.dangerText else colors.text,
                )
                TextButton(onClick = { onStartReceiver(phase == BridgePhase.RECEIVER_ERROR) }) {
                    Text(if (failed) "Yeniden dene" else "Alıcıyı aç")
                }
            }
            else -> {
                if (testing) {
                    Row(
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(12.dp),
                    ) {
                        CircularProgressIndicator(modifier = Modifier.size(20.dp), strokeWidth = 2.dp)
                        Text("Test videosu açılıyor…", style = MaterialTheme.typography.bodyMedium)
                    }
                } else if (lan != null) {
                    Text(
                        text = "DJI Fly'da RTMP adresi olarak bunu yaz ve yayını başlat. Görüntü burada görünür; " +
                            "platforma sen başlatınca gider.",
                        style = MaterialTheme.typography.bodyMedium,
                    )
                    AddressField(address = lan.publishUrl, onCopy = { onCopyAddress(lan.publishUrl) })
                    Text(text = DJI_FLY_PATH, style = MaterialTheme.typography.bodySmall, color = colors.muted)
                    NetworkNote(lan)
                } else {
                    NoNetwork(onOpenWifiSettings)
                }
                if (!testing) {
                    TextButton(onClick = onTestVideo) {
                        Icon(Icons.Rounded.Movie, contentDescription = null, modifier = Modifier.size(18.dp))
                        Spacer(Modifier.width(8.dp))
                        Text("Drone yok mu? Test videosuyla dene")
                    }
                }
            }
        }
    }
}

internal const val DJI_FLY_PATH = "DJI Fly → GO FLY → ••• → Aktarım → Canlı Yayın Platformları → RTMP"

@Composable
private fun PlatformGrid(
    destinations: DestinationProfiles,
    onClick: (DestinationKind) -> Unit,
    onLongClick: (DestinationKind) -> Unit,
) {
    val selectedKind = destinations.selectedProfile?.kind
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
                            selected = kind == selectedKind,
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
    Column(
        modifier = modifier
            .clip(MaterialTheme.shapes.medium)
            .combinedClickable(
                onClickLabel = when {
                    selected -> "Düzenle"
                    configured -> "Seç"
                    else -> "Ekle"
                },
                role = Role.Button,
                onLongClickLabel = if (configured) "Düzenle" else null,
                onLongClick = if (configured) onLongClick else null,
                onClick = onClick,
            )
            .semantics {
                this.selected = selected
                stateDescription = if (configured) "Kayıtlı" else "Eklenmedi"
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
            text = kind.label,
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
    Row(verticalAlignment = Alignment.CenterVertically) {
        Column(modifier = Modifier.weight(1f)) {
            Text(
                text = if (profile.kind == DestinationKind.CUSTOM) profile.name else "${profile.kind.label} seçili",
                style = MaterialTheme.typography.bodyMedium,
                fontWeight = FontWeight.Medium,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            Text(
                text = if (profile.kind.keyChangesEachStream) {
                    "Her yayında yeni anahtar gerekir"
                } else {
                    "Yayın anahtarı kayıtlı"
                },
                style = MaterialTheme.typography.bodySmall,
                color = colors.muted,
            )
        }
        TextButton(onClick = onEdit) {
            Text(if (profile.kind.keyChangesEachStream) "Anahtarı güncelle" else "Düzenle")
        }
    }
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
                LanKind.HOTSPOT -> "Hotspot açık · kumandayı bu telefona bağla"
                LanKind.WIFI -> "Wi-Fi bağlı · kumanda da aynı ağda olmalı"
                LanKind.WIRED, LanKind.OTHER -> "Yerel ağa bağlı · kumanda da aynı ağda olmalı"
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
            text = "Telefon Wi-Fi'a bağlı değil. Wi-Fi'a bağlan ya da hotspot'u aç.",
            style = MaterialTheme.typography.bodyMedium,
        )
    }
    TextButton(modifier = Modifier.padding(start = 20.dp), onClick = onOpenWifiSettings) { Text("Wi-Fi ayarları") }
}
