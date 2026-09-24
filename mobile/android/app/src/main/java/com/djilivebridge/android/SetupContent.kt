package com.djilivebridge.android

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Check
import androidx.compose.material.icons.rounded.Wifi
import androidx.compose.material.icons.rounded.WifiOff
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

/** What the user sets up before starting: where to stream, and the address DJI Fly needs. */
@Composable
internal fun SetupContent(
    destinations: DestinationProfiles,
    profileError: String?,
    lan: LanAddress?,
    startError: String?,
    onPlatformClick: (DestinationKind) -> Unit,
    onPlatformLongClick: (DestinationKind) -> Unit,
    onEditSelected: () -> Unit,
    onCopyAddress: (String) -> Unit,
    onOpenWifiSettings: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val selected = destinations.selectedProfile
    Column(modifier = modifier, verticalArrangement = Arrangement.spacedBy(12.dp)) {
        startError?.let { AlertBanner(title = "Yayın başlatılamadı", message = it) }
        profileError?.let { AlertBanner(title = "Hedef kaydı okunamadı", message = it) }

        BridgeCard {
            SectionHeader(title = "Nereye yayın yapacaksın?", step = 1, done = selected != null)
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

        BridgeCard(verticalSpacing = 12.dp) {
            // Stays a number: the app cannot know whether the address is in DJI Fly yet.
            SectionHeader(title = "DJI Fly'a bu adresi gir", step = 2)
            if (lan != null) {
                AddressField(address = lan.publishUrl, onCopy = { onCopyAddress(lan.publishUrl) })
                Text(
                    text = DJI_FLY_PATH,
                    style = MaterialTheme.typography.bodySmall,
                    color = BridgeTheme.colors.muted,
                )
                NetworkNote(lan)
            } else {
                NoNetwork(onOpenWifiSettings)
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
