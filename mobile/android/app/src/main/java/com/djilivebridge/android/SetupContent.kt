package com.djilivebridge.android

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Add
import androidx.compose.material.icons.rounded.CheckCircle
import androidx.compose.material.icons.rounded.Edit
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp

/** The setup checklist shown while the bridge is off. */
@Composable
internal fun SetupContent(
    destinations: DestinationProfiles,
    profileError: String?,
    lan: LanAddress?,
    startError: String?,
    onAddDestination: (DestinationKind) -> Unit,
    onSelectDestination: (DestinationProfile) -> Unit,
    onEditDestination: (DestinationProfile) -> Unit,
    modifier: Modifier = Modifier,
) {
    val selected = destinations.selectedProfile
    val destinationReady = selected != null
    val networkReady = lan != null
    var destinationsExpanded by rememberSaveable { mutableStateOf(false) }

    Column(modifier = modifier, verticalArrangement = Arrangement.spacedBy(12.dp)) {
        SetupHeader(ready = destinationReady && networkReady)
        startError?.let { AlertBanner(title = "Köprü başlatılamadı", message = it) }

        StepCard(
            number = 1,
            title = "Yayın hedefi",
            state = if (destinationReady) StepState.DONE else StepState.CURRENT,
            summary = selected?.let(::destinationSummary)
                ?: "Görüntü nereye gidecek? Platformunu seç, bilgilerini bir kez kaydet.",
            action = if (destinationReady) {
                {
                    TextButton(onClick = { destinationsExpanded = !destinationsExpanded }) {
                        Text(if (destinationsExpanded) "Kapat" else "Değiştir")
                    }
                }
            } else {
                null
            },
            content = when {
                !destinationReady -> {
                    {
                        PlatformPicker(onPick = onAddDestination)
                        profileError?.let { AlertBanner(title = "Hedefler okunamadı", message = it) }
                    }
                }
                destinationsExpanded || profileError != null -> {
                    {
                        DestinationList(
                            destinations = destinations,
                            onSelect = onSelectDestination,
                            onEdit = onEditDestination,
                            onAdd = { onAddDestination(DestinationKind.CUSTOM) },
                        )
                        profileError?.let { AlertBanner(title = "Hedef işlemi tamamlanamadı", message = it) }
                    }
                }
                else -> null
            },
        )

        StepCard(
            number = 2,
            title = "Ağ bağlantısı",
            state = when {
                networkReady -> StepState.DONE
                destinationReady -> StepState.CURRENT
                else -> StepState.UPCOMING
            },
            summary = lan?.let { "${it.kind.label} · ${it.address}" }
                ?: "Telefon henüz bir Wi-Fi ağına bağlı değil.",
            content = if (networkReady) {
                { InfoNote(text = remoteNetworkHint(lan.kind)) }
            } else if (destinationReady) {
                {
                    Text(
                        text = "Telefonu kumandanın da bağlanacağı Wi-Fi ağına bağla ya da telefonun " +
                            "hotspot'unu açıp kumandayı ona bağla.",
                        style = MaterialTheme.typography.bodyMedium,
                    )
                    Row(
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(10.dp),
                    ) {
                        CircularProgressIndicator(modifier = Modifier.size(16.dp), strokeWidth = 2.dp)
                        Text(
                            text = "Bağlantı gelince bu adım kendiliğinden tamamlanır.",
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                }
            } else {
                null
            },
        )

        StepCard(
            number = 3,
            title = "Köprüyü başlat",
            state = if (destinationReady && networkReady) StepState.CURRENT else StepState.UPCOMING,
            summary = "Telefon, kumandadan gelecek görüntüyü bekler ve seçtiğin hedefe aktarır. " +
                "Ekran kapansa da çalışmaya devam eder.",
        )

        StepCard(
            number = 4,
            title = "DJI Fly'da yayını aç",
            state = StepState.UPCOMING,
            summary = "Köprü çalışınca kumandaya yazacağın adres ve adımlar burada görünür.",
        )
    }
}

@Composable
private fun SetupHeader(ready: Boolean) {
    Column(
        modifier = Modifier.padding(start = 4.dp, top = 4.dp, end = 4.dp, bottom = 4.dp),
        verticalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        Text(
            modifier = Modifier.semantics { heading() },
            text = if (ready) "Her şey hazır" else "Yayına hazırlan",
            style = MaterialTheme.typography.headlineSmall,
        )
        Text(
            text = if (ready) {
                "Köprüyü başlat, ardından kumandada DJI Fly'dan yayını aç."
            } else {
                "Adımları sırayla tamamla. Sıradaki adım mavi çerçeveyle gösterilir."
            },
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}

@Composable
private fun PlatformPicker(onPick: (DestinationKind) -> Unit) {
    Row(
        modifier = Modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        DestinationKind.entries.forEach { kind ->
            PlatformChoice(
                kind = kind,
                onClick = { onPick(kind) },
                modifier = Modifier.weight(1f),
            )
        }
    }
}

@Composable
private fun DestinationList(
    destinations: DestinationProfiles,
    onSelect: (DestinationProfile) -> Unit,
    onEdit: (DestinationProfile) -> Unit,
    onAdd: () -> Unit,
) {
    Column(
        modifier = Modifier.selectableGroup(),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        destinations.profiles.forEach { profile ->
            DestinationRow(
                profile = profile,
                selected = profile.id == destinations.selectedProfileId,
                onSelect = { onSelect(profile) },
                onEdit = { onEdit(profile) },
            )
        }
    }
    OutlinedButton(
        modifier = Modifier
            .fillMaxWidth()
            .heightIn(min = 48.dp),
        onClick = onAdd,
    ) {
        Icon(Icons.Rounded.Add, contentDescription = null, modifier = Modifier.size(20.dp))
        Spacer(Modifier.width(8.dp))
        Text("Yeni hedef ekle")
    }
}

@Composable
private fun DestinationRow(
    profile: DestinationProfile,
    selected: Boolean,
    onSelect: () -> Unit,
    onEdit: () -> Unit,
) {
    val scheme = MaterialTheme.colorScheme
    val shape = MaterialTheme.shapes.medium
    Surface(
        modifier = Modifier
            .fillMaxWidth()
            .clip(shape)
            .selectable(selected = selected, role = Role.RadioButton, onClick = onSelect),
        shape = shape,
        color = if (selected) scheme.primaryContainer else BridgeTheme.colors.card,
        contentColor = if (selected) scheme.onPrimaryContainer else scheme.onSurface,
        border = if (selected) {
            BorderStroke(1.5.dp, scheme.primary)
        } else {
            BorderStroke(1.dp, scheme.outlineVariant)
        },
    ) {
        Row(
            modifier = Modifier.padding(start = 12.dp, top = 8.dp, end = 4.dp, bottom = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            PlatformAvatar(kind = profile.kind, size = 36.dp)
            Column(modifier = Modifier.weight(1f)) {
                Text(
                    text = profile.name,
                    style = MaterialTheme.typography.titleSmall,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                Text(
                    text = profile.kind.label,
                    style = MaterialTheme.typography.bodySmall,
                    color = if (selected) scheme.onPrimaryContainer else scheme.onSurfaceVariant,
                )
            }
            if (selected) {
                Icon(
                    imageVector = Icons.Rounded.CheckCircle,
                    contentDescription = "Seçili",
                    tint = scheme.primary,
                )
            }
            IconButton(onClick = onEdit) {
                Icon(Icons.Rounded.Edit, contentDescription = "${profile.name} hedefini düzenle")
            }
        }
    }
}

private fun destinationSummary(profile: DestinationProfile): String =
    if (profile.name.equals(profile.kind.label, ignoreCase = true)) {
        profile.name
    } else {
        "${profile.name} · ${profile.kind.label}"
    }

internal val LanKind.label: String
    get() = when (this) {
        LanKind.WIFI -> "Wi-Fi"
        LanKind.HOTSPOT -> "Hotspot"
        LanKind.WIRED -> "Kablolu ağ"
        LanKind.OTHER -> "Yerel ağ"
    }

private fun remoteNetworkHint(kind: LanKind): String = when (kind) {
    LanKind.HOTSPOT -> "DJI RC 2 kumandanı bu telefonun hotspot'una bağla."
    LanKind.WIFI -> "DJI RC 2 kumandanı da aynı Wi-Fi ağına bağla."
    LanKind.WIRED, LanKind.OTHER -> "DJI RC 2 kumandanı da bu ağa bağla."
}
