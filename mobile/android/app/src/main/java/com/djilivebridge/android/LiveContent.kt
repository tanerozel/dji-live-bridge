package com.djilivebridge.android

import android.os.SystemClock
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
import androidx.compose.material.icons.rounded.ChevronRight
import androidx.compose.material.icons.rounded.ErrorOutline
import androidx.compose.material.icons.rounded.Movie
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
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
    testVideoName: String?,
    destination: DestinationProfile?,
    lan: LanAddress?,
    onCopyAddress: (String) -> Unit,
    modifier: Modifier = Modifier,
) {
    val kind = destination?.kind ?: DestinationKind.CUSTOM
    Column(
        modifier = modifier,
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        StatusHero(
            phase = phase,
            snapshot = snapshot,
            liveSinceElapsedMillis = liveSinceElapsedMillis,
            kind = kind,
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
        when {
            phase == BridgePhase.RECONNECTING && snapshot.outputReconnectAttempts >= RECONNECT_HINT_THRESHOLD ->
                TipBox(
                    "Sürekli tekrarlanıyorsa yayın anahtarını, sunucu adresini ve platformda canlı yayının " +
                        "açık olduğunu kontrol et.",
                )
            phase == BridgePhase.RECEIVER_ERROR ->
                TipBox("Yayını bitirip alıcıyı yeniden aç. Sorun sürerse telefonu yeniden başlatmayı dene.")
            phase == BridgePhase.LIVE && snapshot.outputStatus == "congested" ->
                TipBox(
                    "Yükleme hızı yetmiyor; yayın sürüyor ama bazı görüntü kareleri atlanıyor. Daha güçlü " +
                        "bir bağlantıya geç ya da DJI Fly'da yayın kalitesini düşür.",
                )
            phase == BridgePhase.LIVE -> kind.liveReminder?.let { TipBox(it) }
        }
        TechnicalDetails(snapshot)
    }
}

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
    kind: DestinationKind,
    testVideoName: String?,
) {
    val colors = BridgeTheme.colors
    val testing = testVideoName != null
    val waitingForSource = phase == BridgePhase.WAITING_FOR_DRONE || phase == BridgePhase.DRONE_CONNECTED
    val look = if (testing && waitingForSource) {
        HeroLook(colors.accent, colors.accentSoft, "Test videosu hazırlanıyor", "Video birazdan gönderilmeye başlar.")
    } else if (waitingForSource && snapshot.outputStatus == "holding") {
        HeroLook(
            colors.warningText,
            colors.warningSoft,
            "Drone bağlantısı koptu",
            "Yayın açık tutuluyor; kumanda yeniden bağlanınca kaldığı yerden sürer.",
        )
    } else when (phase) {
        BridgePhase.STARTING -> HeroLook(colors.accent, colors.accentSoft, "Başlatılıyor", "Köprü birkaç saniye içinde hazır olur.")
        BridgePhase.WAITING_FOR_DRONE -> HeroLook(
            colors.warningText,
            colors.warningSoft,
            "Kumanda bekleniyor",
            "DJI Fly'da yayını başlat; görüntü gelince ${kind.dative} kendiliğinden bağlanır.",
        )
        BridgePhase.DRONE_CONNECTED ->
            HeroLook(colors.accent, colors.accentSoft, "Kumanda bağlandı", "Görüntü birazdan gelir.")
        // Only for a moment: the live screen shows once the stream is sent somewhere.
        BridgePhase.PREVIEW, BridgePhase.CONNECTING_TARGET ->
            HeroLook(colors.accent, colors.accentSoft, "${kind.dative} bağlanılıyor", "")
        BridgePhase.LIVE -> HeroLook(colors.live, colors.dangerSoft, "${kind.locative} yayındasın", "")
        BridgePhase.RECONNECTING ->
            HeroLook(colors.warningText, colors.warningSoft, "Bağlantı koptu", "${kind.dative} yeniden bağlanılıyor…")
        BridgePhase.RECEIVER_ERROR ->
            HeroLook(colors.dangerText, colors.dangerSoft, "Alıcı durdu", snapshot.detail, ping = false)
        BridgePhase.IDLE, BridgePhase.START_FAILED ->
            HeroLook(colors.faint, colors.field, "Yayın kapalı", snapshot.detail, ping = false)
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
                else -> PlatformTile(kind = kind, size = 52.dp)
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
        HopLine(phase = phase, kind = kind, sourceLabel = if (testing) "Test videosu" else "Kumanda")
        testVideoName?.let { name ->
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(6.dp),
            ) {
                Icon(Icons.Rounded.Movie, contentDescription = null, tint = colors.faint, modifier = Modifier.size(16.dp))
                Text(
                    text = name,
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
            BridgePhase.CONNECTING_TARGET -> OverlayChip { OverlayText("Bağlanıyor") }
            BridgePhase.RECONNECTING -> OverlayChip { OverlayText("Yeniden bağlanıyor") }
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
    Text(
        modifier = Modifier.semantics { contentDescription = "Yayın süresi ${formatDuration(now - sinceElapsedMillis)}" },
        text = formatDuration(now - sinceElapsedMillis),
        color = color,
        style = style.copy(fontFeatureSettings = "tnum"),
    )
}

/** Source → phone → platform in one line; each dot is colored by that hop's state. */
@Composable
private fun HopLine(phase: BridgePhase, kind: DestinationKind, sourceLabel: String) {
    val colors = BridgeTheme.colors
    val (sourceColor, sourceState) = when (phase) {
        BridgePhase.WAITING_FOR_DRONE -> colors.warningText to "bekleniyor"
        BridgePhase.DRONE_CONNECTED -> colors.accent to "bağlandı"
        BridgePhase.PREVIEW, BridgePhase.CONNECTING_TARGET, BridgePhase.LIVE, BridgePhase.RECONNECTING ->
            colors.success to "gönderiyor"
        else -> colors.faint to "bağlı değil"
    }
    val (relayColor, relayState) = when (phase) {
        BridgePhase.STARTING -> colors.accent to "açılıyor"
        BridgePhase.RECEIVER_ERROR -> colors.dangerText to "hata"
        else -> colors.success to "hazır"
    }
    val (targetColor, targetState) = when (phase) {
        BridgePhase.LIVE -> colors.success to "yayında"
        BridgePhase.CONNECTING_TARGET -> colors.accent to "bağlanıyor"
        BridgePhase.RECONNECTING -> colors.warningText to "yeniden bağlanıyor"
        else -> colors.faint to "beklemede"
    }
    Row(
        modifier = Modifier.clearAndSetSemantics {
            contentDescription = "$sourceLabel $sourceState, telefon $relayState, ${kind.label} $targetState"
        },
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Hop(sourceLabel, sourceColor)
        HopArrow()
        Hop("Telefon", relayColor)
        HopArrow()
        Hop(kind.label, targetColor)
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
        imageVector = Icons.Rounded.ChevronRight,
        contentDescription = null,
        tint = BridgeTheme.colors.faint,
        modifier = Modifier.size(16.dp),
    )
}

@Composable
private fun DjiFlyCard(lan: LanAddress?, onCopyAddress: (String) -> Unit) {
    var troubleshootingOpen by rememberSaveable { mutableStateOf(false) }
    BridgeCard(verticalSpacing = 12.dp) {
        SectionHeader(title = "Kumandada yayını başlat")
        Text(text = DJI_FLY_PATH, style = MaterialTheme.typography.bodySmall, color = BridgeTheme.colors.muted)
        if (lan != null) {
            AddressField(address = lan.publishUrl, onCopy = { onCopyAddress(lan.publishUrl) })
        } else {
            AlertBanner(
                title = "Telefon ağdan koptu",
                message = "Wi-Fi'a yeniden bağlan ya da hotspot'u aç; adres burada yeniden görünür.",
            )
        }
        ExpandableSection(
            title = "Bağlanmıyor mu?",
            expanded = troubleshootingOpen,
            onToggle = { troubleshootingOpen = !troubleshootingOpen },
        ) {
            BulletItem("Kumanda ile telefon aynı Wi-Fi'da olmalı ya da kumanda bu telefonun hotspot'una bağlı olmalı.")
            BulletItem("Adresi harfi harfine yaz; sonu :1935/drone ile bitmeli.")
            BulletItem("Misafir ağları ve telefondaki VPN, cihazların birbirini görmesini engelleyebilir.")
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
            Stat(label = "Bit hızı", value = formatBitrate(snapshot.bitrateKbps), modifier = Modifier.weight(1f))
            VerticalDivider(color = BridgeTheme.colors.border)
            Stat(label = "Gönderilen", value = formatBytes(snapshot.outboundBytes), modifier = Modifier.weight(1f))
            VerticalDivider(color = BridgeTheme.colors.border)
            Stat(label = "Kodek", value = codecs, modifier = Modifier.weight(1f))
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

@Composable
private fun TechnicalDetails(snapshot: RelaySnapshot) {
    var expanded by rememberSaveable { mutableStateOf(false) }
    BridgeCard(contentPadding = PaddingValues(horizontal = 16.dp, vertical = 4.dp), verticalSpacing = 0.dp) {
        ExpandableSection(title = "Teknik ayrıntılar", expanded = expanded, onToggle = { expanded = !expanded }) {
            DetailRow("Kaynak", snapshot.remoteAddress ?: "Bağlantı bekleniyor")
            DetailRow("Alınan veri", formatBytes(snapshot.receivedBytes))
            DetailRow("Hedefe iletilen", formatBytes(snapshot.outboundBytes))
            DetailRow("Video / ses paketleri", "${snapshot.videoFrames} / ${snapshot.audioFrames}")
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
            HorizontalDivider(color = BridgeTheme.colors.border)
            Text(
                text = "Ekranı kilitleyebilirsin; yayın arka planda sürer. Android 15 ve üzeri arka plan " +
                    "aktarımını 24 saatte toplam 6 saatle sınırlar.",
                style = MaterialTheme.typography.bodySmall,
                color = BridgeTheme.colors.muted,
            )
        }
    }
}

private const val RECONNECT_HINT_THRESHOLD = 3
