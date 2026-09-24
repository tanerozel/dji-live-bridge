package com.djilivebridge.android

import android.os.SystemClock
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.size
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.rounded.HelpOutline
import androidx.compose.material.icons.rounded.CloudUpload
import androidx.compose.material.icons.rounded.GraphicEq
import androidx.compose.material.icons.rounded.Lock
import androidx.compose.material.icons.rounded.PhoneAndroid
import androidx.compose.material.icons.rounded.Speed
import androidx.compose.material.icons.rounded.Timer
import androidx.compose.material.icons.rounded.Tune
import androidx.compose.material.icons.rounded.Videocam
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.rememberVectorPainter
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.delay

/** Everything shown while the bridge runs: status, what to do on the RC 2, and live numbers. */
@Composable
internal fun LiveContent(
    phase: BridgePhase,
    snapshot: RelaySnapshot,
    liveSinceElapsedMillis: Long?,
    destination: DestinationProfile?,
    lan: LanAddress?,
    onCopyAddress: (String) -> Unit,
    modifier: Modifier = Modifier,
) {
    Column(modifier = modifier, verticalArrangement = Arrangement.spacedBy(12.dp)) {
        StatusCard(
            phase = phase,
            snapshot = snapshot,
            liveSinceElapsedMillis = liveSinceElapsedMillis,
            destination = destination,
        )
        if (phase == BridgePhase.STARTING ||
            phase == BridgePhase.WAITING_FOR_DRONE ||
            phase == BridgePhase.DRONE_CONNECTED
        ) {
            DjiFlyCard(
                lan = lan,
                onCopyAddress = onCopyAddress,
                highlighted = phase == BridgePhase.WAITING_FOR_DRONE,
            )
        }
        if (phase.isStreaming) {
            MetricsCard(snapshot)
        }
        if (phase == BridgePhase.LIVE) {
            destination?.kind?.liveReminder?.let { reminder ->
                TipBox(title = "Platformu da kontrol et", text = reminder)
            }
        }
        TechnicalDetailsCard(snapshot)
        InfoNote(
            text = "Ekranı kilitleyebilir ya da başka bir uygulamaya geçebilirsin; köprü arka planda " +
                "çalışmaya devam eder. Bildirimden de durdurabilirsin.",
            icon = Icons.Rounded.Lock,
        )
    }
}

private class StatusCopy(
    val pill: String,
    val tone: StatusColors,
    val title: String,
    val body: String,
    val hint: String? = null,
    val detail: String? = null,
    val pulsing: Boolean = false,
)

@Composable
private fun statusCopy(phase: BridgePhase, snapshot: RelaySnapshot, destinationName: String): StatusCopy {
    val scheme = MaterialTheme.colorScheme
    val colors = BridgeTheme.colors
    val info = StatusColors(scheme.primary, scheme.onPrimary, scheme.primaryContainer, scheme.onPrimaryContainer)
    val error = StatusColors(scheme.error, scheme.onError, scheme.errorContainer, scheme.onErrorContainer)
    return when (phase) {
        BridgePhase.STARTING -> StatusCopy(
            pill = "Hazırlanıyor",
            tone = info,
            title = "Köprü açılıyor",
            body = "Birkaç saniye içinde hazır olacak.",
            pulsing = true,
        )
        BridgePhase.WAITING_FOR_DRONE -> StatusCopy(
            pill = "Bekleniyor",
            tone = colors.warning,
            title = "Kumandadan yayın bekleniyor",
            body = "Köprü hazır. Şimdi aşağıdaki adımlarla kumandada DJI Fly'dan yayını başlat.",
            pulsing = true,
        )
        BridgePhase.DRONE_CONNECTED -> StatusCopy(
            pill = "Bağlandı",
            tone = info,
            title = "Kumanda bağlandı",
            body = "DJI Fly yayını başlatıyor; görüntü birazdan gelir.",
            pulsing = true,
        )
        BridgePhase.CONNECTING_TARGET -> StatusCopy(
            pill = "Bağlanıyor",
            tone = info,
            title = "Hedefe bağlanılıyor",
            body = "Görüntü geliyor. $destinationName hedefine bağlanılıyor…",
            pulsing = true,
        )
        BridgePhase.LIVE -> StatusCopy(
            pill = "Canlı",
            tone = colors.live,
            title = "Yayındasın",
            body = "Görüntü olduğu gibi $destinationName hedefine aktarılıyor.",
        )
        BridgePhase.RECONNECTING -> StatusCopy(
            pill = "Yeniden bağlanıyor",
            tone = colors.warning,
            title = "Hedef bağlantısı koptu",
            body = "Görüntü gelmeye devam ediyor; köprü $destinationName hedefine kendiliğinden " +
                "yeniden bağlanıyor.",
            hint = if (snapshot.outputReconnectAttempts >= RECONNECT_HINT_THRESHOLD) {
                "Sürekli tekrarlanıyorsa yayın anahtarını, sunucu adresini ve platformda canlı " +
                    "yayının açık olduğunu kontrol et."
            } else {
                null
            },
            detail = snapshot.outputDetail,
            pulsing = true,
        )
        BridgePhase.RECEIVER_ERROR -> StatusCopy(
            pill = "Hata",
            tone = error,
            title = "Alıcı durdu",
            body = snapshot.detail,
            hint = "Köprüyü durdurup yeniden başlat. Sorun sürerse telefonu yeniden başlatmayı dene.",
        )
        BridgePhase.IDLE, BridgePhase.START_FAILED -> StatusCopy(
            pill = "Kapalı",
            tone = StatusColors(
                scheme.onSurfaceVariant,
                scheme.surface,
                scheme.surfaceContainerHigh,
                scheme.onSurface,
            ),
            title = "Köprü kapalı",
            body = snapshot.detail,
        )
    }
}

@Composable
private fun StatusCard(
    phase: BridgePhase,
    snapshot: RelaySnapshot,
    liveSinceElapsedMillis: Long?,
    destination: DestinationProfile?,
) {
    val copy = statusCopy(phase, snapshot, destination?.name ?: "Yayın")
    BridgeCard {
        Row(verticalAlignment = Alignment.CenterVertically) {
            if (phase == BridgePhase.LIVE) {
                LiveBadge()
            } else {
                StatusPill(text = copy.pill, colors = copy.tone, pulsing = copy.pulsing)
            }
            Spacer(Modifier.weight(1f))
            liveSinceElapsedMillis?.let { LiveTimer(it) }
        }
        Column(
            modifier = Modifier.semantics(mergeDescendants = true) {
                liveRegion = LiveRegionMode.Polite
            },
            verticalArrangement = Arrangement.spacedBy(4.dp),
        ) {
            Text(
                modifier = Modifier.semantics { heading() },
                text = copy.title,
                style = MaterialTheme.typography.headlineSmall,
            )
            Text(
                text = copy.body,
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        copy.hint?.let { TipBox(text = it) }
        copy.detail?.let {
            Text(
                text = it,
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        PhasePipeline(phase = phase, destination = destination)
    }
}

@Composable
private fun LiveTimer(sinceElapsedMillis: Long) {
    val now by produceState(SystemClock.elapsedRealtime(), sinceElapsedMillis) {
        while (true) {
            value = SystemClock.elapsedRealtime()
            delay(1_000)
        }
    }
    Row(
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Icon(
            imageVector = Icons.Rounded.Timer,
            contentDescription = "Yayın süresi",
            tint = MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = Modifier.size(18.dp),
        )
        Text(
            text = formatDuration(now - sinceElapsedMillis),
            style = MaterialTheme.typography.titleMedium.copy(fontFeatureSettings = "tnum"),
        )
    }
}

@Composable
private fun PhasePipeline(phase: BridgePhase, destination: DestinationProfile?) {
    val (sourceTone, sourceStatus) = when (phase) {
        BridgePhase.WAITING_FOR_DRONE -> NodeTone.WAITING to "Bekleniyor"
        BridgePhase.DRONE_CONNECTED -> NodeTone.BUSY to "Bağlandı"
        BridgePhase.CONNECTING_TARGET, BridgePhase.LIVE, BridgePhase.RECONNECTING -> NodeTone.ACTIVE to "Gönderiyor"
        else -> NodeTone.IDLE to "Bağlı değil"
    }
    val (relayTone, relayStatus) = when (phase) {
        BridgePhase.STARTING -> NodeTone.BUSY to "Açılıyor"
        BridgePhase.RECEIVER_ERROR -> NodeTone.ERROR to "Hata"
        BridgePhase.LIVE -> NodeTone.ACTIVE to "Aktarıyor"
        BridgePhase.IDLE, BridgePhase.START_FAILED -> NodeTone.IDLE to "Kapalı"
        else -> NodeTone.ACTIVE to "Hazır"
    }
    val (targetTone, targetStatus) = when (phase) {
        BridgePhase.LIVE -> NodeTone.ACTIVE to "Yayında"
        BridgePhase.CONNECTING_TARGET -> NodeTone.BUSY to "Bağlanıyor"
        BridgePhase.RECONNECTING -> NodeTone.WAITING to "Tekrar deniyor"
        else -> NodeTone.IDLE to "Beklemede"
    }
    PipelineView(
        source = PipelineNode("Kumanda", sourceStatus, sourceTone, painterResource(R.drawable.ic_drone)),
        relay = PipelineNode(
            "Bu telefon",
            relayStatus,
            relayTone,
            rememberVectorPainter(Icons.Rounded.PhoneAndroid),
        ),
        target = PipelineNode(
            destination?.kind?.label ?: "Hedef",
            targetStatus,
            targetTone,
            rememberVectorPainter((destination?.kind ?: DestinationKind.CUSTOM).icon),
        ),
        sourceLinkActive = phase.isStreaming,
        targetLinkActive = phase == BridgePhase.LIVE,
    )
}

@Composable
private fun DjiFlyCard(lan: LanAddress?, onCopyAddress: (String) -> Unit, highlighted: Boolean) {
    var troubleshootingOpen by rememberSaveable { mutableStateOf(false) }
    BridgeCard(highlighted = highlighted) {
        Text(
            modifier = Modifier.semantics { heading() },
            text = "Kumandada DJI Fly'dan yayını başlat",
            style = MaterialTheme.typography.titleMedium,
        )
        NumberedItem(number = 1, text = "DJI Fly'ı aç ve GO FLY'a dokunarak kamera görünümüne geç.")
        NumberedItem(
            number = 2,
            text = "Sağ üstteki ••• menüsünden Aktarım → Canlı Yayın Platformları → RTMP'yi seç.",
        )
        NumberedItem(number = 3, text = "Bu adresi RTMP adresi alanına yaz ve yayını başlat:")
        if (lan != null) {
            AddressBox(address = lan.publishUrl, onCopy = { onCopyAddress(lan.publishUrl) })
        } else {
            AlertBanner(
                title = "Telefon ağdan koptu",
                message = "Telefonu yeniden Wi-Fi'a bağla ya da hotspot'u aç; adres burada yeniden görünür.",
            )
        }
        HorizontalDivider(color = BridgeTheme.colors.cardBorder)
        ExpandableSection(
            title = "Bağlanmıyor mu?",
            expanded = troubleshootingOpen,
            onToggle = { troubleshootingOpen = !troubleshootingOpen },
            icon = Icons.AutoMirrored.Rounded.HelpOutline,
        ) {
            BulletItem(
                "Kumanda ile telefon aynı Wi-Fi ağında olmalı ya da kumanda bu telefonun hotspot'una " +
                    "bağlı olmalı. Misafir ağları cihazların birbirini görmesini engelleyebilir.",
            )
            BulletItem("Adresi harfi harfine yaz; sonu :1935/drone ile bitmeli.")
            BulletItem(
                "Telefon başka bir ağa geçerse adres değişir; DJI Fly'daki adresi de güncelle.",
            )
            BulletItem("Telefonda VPN açıksa kapatıp yeniden dene.")
        }
    }
}

@Composable
private fun MetricsCard(snapshot: RelaySnapshot) {
    BridgeCard {
        Text(
            modifier = Modifier.semantics { heading() },
            text = "Yayın bilgileri",
            style = MaterialTheme.typography.titleMedium,
        )
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            MetricTile(
                modifier = Modifier.weight(1f),
                label = "Bit hızı",
                value = formatBitrate(snapshot.bitrateKbps),
                icon = Icons.Rounded.Speed,
            )
            MetricTile(
                modifier = Modifier.weight(1f),
                label = "Hedefe gönderilen",
                value = formatBytes(snapshot.outboundBytes),
                icon = Icons.Rounded.CloudUpload,
            )
        }
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            MetricTile(
                modifier = Modifier.weight(1f),
                label = "Görüntü",
                value = snapshot.videoCodec?.let(::friendlyCodecName) ?: "Bekleniyor",
                icon = Icons.Rounded.Videocam,
            )
            MetricTile(
                modifier = Modifier.weight(1f),
                label = "Ses",
                value = snapshot.audioCodec?.let(::friendlyCodecName) ?: "Bekleniyor",
                icon = Icons.Rounded.GraphicEq,
            )
        }
    }
}

@Composable
private fun TechnicalDetailsCard(snapshot: RelaySnapshot) {
    var expanded by rememberSaveable { mutableStateOf(false) }
    BridgeCard(
        contentPadding = PaddingValues(horizontal = 16.dp, vertical = 8.dp),
        verticalSpacing = 0.dp,
    ) {
        ExpandableSection(
            title = "Teknik ayrıntılar",
            expanded = expanded,
            onToggle = { expanded = !expanded },
            icon = Icons.Rounded.Tune,
        ) {
            DetailRow("Kaynak", snapshot.remoteAddress ?: "Bağlantı bekleniyor")
            DetailRow("Alınan veri", formatBytes(snapshot.receivedBytes))
            DetailRow("Hedefe iletilen", formatBytes(snapshot.outboundBytes))
            DetailRow("Video paketleri", snapshot.videoFrames.toString())
            DetailRow("Ses paketleri", snapshot.audioFrames.toString())
            DetailRow(
                "Hedef bağlantısı",
                when {
                    snapshot.outputSecure && snapshot.outputStatus in setOf("ready", "forwarding") ->
                        "RTMPS · sertifika doğrulandı"
                    snapshot.outputSecure -> "RTMPS · doğrulama bekleniyor"
                    else -> "RTMP · şifresiz"
                },
            )
            DetailRow("Hedef durumu", snapshot.outputDetail)
            if (snapshot.outputReconnectAttempts > 0) {
                DetailRow("Yeniden bağlanma", snapshot.outputReconnectAttempts.toString())
            }
            if (snapshot.droppedOutputFrames > 0) {
                DetailRow("Atlanan çıkış paketi", snapshot.droppedOutputFrames.toString())
            }
            if (snapshot.rejectedPublishAttempts > 0) {
                DetailRow("Reddedilen yayın", snapshot.rejectedPublishAttempts.toString())
            }
            HorizontalDivider(color = BridgeTheme.colors.cardBorder)
            Text(
                text = "Android 15 ve üzeri, arka plan veri aktarımını 24 saatte toplam 6 saatle " +
                    "sınırlar. Cihazın pil ayarları ek kısıt uygulayabilir.",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}

private val DestinationKind.liveReminder: String?
    get() = when (this) {
        DestinationKind.TIKTOK -> "TikTok LIVE Center'ı kontrol et; yayını orada da başlatman gerekebilir."
        DestinationKind.YOUTUBE ->
            "YouTube önce önizleme gösterir. Otomatik başlatma kapalıysa YouTube Studio'da " +
                "“Canlı yayına geç”e bas."
        DestinationKind.CUSTOM -> null
    }

private const val RECONNECT_HINT_THRESHOLD = 3
