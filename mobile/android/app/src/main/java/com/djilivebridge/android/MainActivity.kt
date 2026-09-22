package com.djilivebridge.android

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.os.Bundle
import android.util.Base64
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlinx.coroutines.delay
import org.json.JSONObject
import java.net.Inet4Address
import java.net.NetworkInterface
import java.security.SecureRandom
import java.util.Locale

private val BridgeColors = darkColorScheme(
    primary = Color(0xFF55D6BE),
    onPrimary = Color(0xFF00201A),
    background = Color(0xFF0B1014),
    onBackground = Color(0xFFE7EEF2),
    surface = Color(0xFF131A20),
    onSurface = Color(0xFFE7EEF2),
    error = Color(0xFFFFB4AB),
)

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent {
            DjiLiveBridgeTheme {
                RelayScreen()
            }
        }
    }

    override fun onDestroy() {
        NativeRelay.nativeStop()
        super.onDestroy()
    }
}

object NativeRelay {
    init {
        System.loadLibrary("dji_relay_core")
    }

    external fun nativeStart(streamKey: String): String
    external fun nativeSnapshot(): String
    external fun nativeStop()
}

private data class RelaySnapshot(
    val status: String = "stopped",
    val detail: String = "RTMP alıcısı kapalı",
    val remoteAddress: String? = null,
    val receivedBytes: Long = 0,
    val bitrateKbps: Double = 0.0,
    val videoCodec: String? = null,
    val audioCodec: String? = null,
    val videoFrames: Long = 0,
    val audioFrames: Long = 0,
    val rejectedPublishAttempts: Long = 0,
) {
    companion object {
        fun fromJson(raw: String): RelaySnapshot {
            val value = JSONObject(raw)
            return RelaySnapshot(
                status = value.optString("status", "error"),
                detail = value.optString("detail", "Durum okunamadı"),
                remoteAddress = value.optionalString("remoteAddress"),
                receivedBytes = value.optLong("receivedBytes"),
                bitrateKbps = value.optDouble("bitrateKbps"),
                videoCodec = value.optionalString("videoCodec"),
                audioCodec = value.optionalString("audioCodec"),
                videoFrames = value.optLong("videoFrames"),
                audioFrames = value.optLong("audioFrames"),
                rejectedPublishAttempts = value.optLong("rejectedPublishAttempts"),
            )
        }
    }
}

private fun JSONObject.optionalString(name: String): String? =
    if (isNull(name)) null else optString(name).takeIf(String::isNotBlank)

@Composable
private fun DjiLiveBridgeTheme(content: @Composable () -> Unit) {
    MaterialTheme(colorScheme = BridgeColors, content = content)
}

@Composable
private fun RelayScreen() {
    val context = LocalContext.current
    val streamKey = rememberSaveable { generateStreamKey() }
    var localAddress by remember { mutableStateOf(findLocalIpv4Address()) }
    var serverRequested by remember { mutableStateOf(false) }
    var relaySnapshot by remember { mutableStateOf(RelaySnapshot()) }
    val publishUrl = localAddress?.let { "rtmp://$it:1935/live/$streamKey" }

    DisposableEffect(Unit) {
        onDispose { NativeRelay.nativeStop() }
    }

    LaunchedEffect(serverRequested) {
        while (serverRequested) {
            relaySnapshot = runCatching {
                RelaySnapshot.fromJson(NativeRelay.nativeSnapshot())
            }.getOrElse { error ->
                RelaySnapshot(status = "error", detail = "Durum okunamadı: ${error.message}")
            }
            delay(500)
        }
    }

    Scaffold(containerColor = MaterialTheme.colorScheme.background) { contentPadding ->
        Column(
            modifier = Modifier
                .fillMaxSize()
                .verticalScroll(rememberScrollState())
                .padding(contentPadding)
                .padding(horizontal = 20.dp, vertical = 24.dp),
            verticalArrangement = Arrangement.spacedBy(18.dp),
        ) {
            Header()
            StatusCard(relaySnapshot)
            PublishAddressCard(
                publishUrl = publishUrl,
                onCopy = {
                    if (publishUrl != null) {
                        val clipboard = context.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
                        clipboard.setPrimaryClip(ClipData.newPlainText("DJI Live Bridge RTMP", publishUrl))
                    }
                },
                onRefreshAddress = { localAddress = findLocalIpv4Address() },
            )
            StatsCard(relaySnapshot)
            Button(
                modifier = Modifier.fillMaxWidth(),
                onClick = {
                    if (serverRequested) {
                        NativeRelay.nativeStop()
                        serverRequested = false
                        relaySnapshot = RelaySnapshot()
                    } else {
                        localAddress = findLocalIpv4Address()
                        val error = NativeRelay.nativeStart(streamKey)
                        if (error.isEmpty()) {
                            serverRequested = true
                        } else {
                            relaySnapshot = RelaySnapshot(status = "error", detail = error)
                        }
                    }
                },
                colors = if (serverRequested) {
                    ButtonDefaults.buttonColors(
                        containerColor = MaterialTheme.colorScheme.error,
                        contentColor = Color(0xFF330400),
                    )
                } else {
                    ButtonDefaults.buttonColors()
                },
            ) {
                Text(if (serverRequested) "RTMP alıcısını durdur" else "RTMP alıcısını başlat")
            }
            Surface(
                color = MaterialTheme.colorScheme.primary.copy(alpha = 0.1f),
                shape = RoundedCornerShape(14.dp),
            ) {
                Text(
                    modifier = Modifier.padding(horizontal = 16.dp, vertical = 12.dp),
                    text = "Faz 2 / 8  •  RC 2 → Android RTMP ingest",
                    color = MaterialTheme.colorScheme.primary,
                    fontSize = 13.sp,
                    fontWeight = FontWeight.Medium,
                )
            }
        }
    }
}

@Composable
private fun Header() {
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Text(
            text = "DJI LIVE BRIDGE",
            color = MaterialTheme.colorScheme.primary,
            fontSize = 12.sp,
            fontWeight = FontWeight.Bold,
            letterSpacing = 1.6.sp,
        )
        Text(text = "Android RTMP alıcısı", fontSize = 30.sp, fontWeight = FontWeight.SemiBold)
        Text(
            text = "Telefonu ve DJI RC 2’yi aynı Wi-Fi ağına bağla. Aşağıdaki adresi DJI Fly özel RTMP alanına gir.",
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.68f),
            fontSize = 15.sp,
            lineHeight = 22.sp,
        )
    }
}

@Composable
private fun StatusCard(snapshot: RelaySnapshot) {
    val statusColor = when (snapshot.status) {
        "publishing" -> MaterialTheme.colorScheme.primary
        "connected" -> Color(0xFFFFD166)
        "error" -> MaterialTheme.colorScheme.error
        else -> MaterialTheme.colorScheme.onSurface.copy(alpha = 0.45f)
    }
    val title = when (snapshot.status) {
        "publishing" -> "Yayın alınıyor"
        "connected" -> "RC 2 bağlandı"
        "listening" -> "Yayın bekleniyor"
        "error" -> "RTMP hatası"
        else -> "Alıcı kapalı"
    }

    Card(
        modifier = Modifier.fillMaxWidth(),
        shape = RoundedCornerShape(20.dp),
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface),
    ) {
        Column(modifier = Modifier.padding(20.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(10.dp),
            ) {
                Box(modifier = Modifier.size(10.dp).background(statusColor, CircleShape))
                Text(title, fontWeight = FontWeight.SemiBold, fontSize = 18.sp)
            }
            Text(
                text = snapshot.detail,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.65f),
                lineHeight = 20.sp,
            )
            snapshot.remoteAddress?.let { remote ->
                Text(
                    text = "Kaynak: $remote",
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.55f),
                    fontSize = 13.sp,
                    fontFamily = FontFamily.Monospace,
                )
            }
        }
    }
}

@Composable
private fun PublishAddressCard(
    publishUrl: String?,
    onCopy: () -> Unit,
    onRefreshAddress: () -> Unit,
) {
    Card(
        modifier = Modifier.fillMaxWidth(),
        shape = RoundedCornerShape(20.dp),
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface),
    ) {
        Column(modifier = Modifier.padding(20.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Text("DJI Fly yayın adresi", fontWeight = FontWeight.SemiBold)
            Surface(
                modifier = Modifier.fillMaxWidth(),
                color = MaterialTheme.colorScheme.background,
                shape = RoundedCornerShape(12.dp),
            ) {
                Text(
                    modifier = Modifier.padding(14.dp),
                    text = publishUrl ?: "Yerel IPv4 adresi bulunamadı",
                    color = if (publishUrl == null) MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.primary,
                    fontFamily = FontFamily.Monospace,
                    fontSize = 13.sp,
                    lineHeight = 19.sp,
                )
            }
            Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                FilledTonalButton(onClick = onCopy, enabled = publishUrl != null) {
                    Text("Adresi kopyala")
                }
                FilledTonalButton(onClick = onRefreshAddress) {
                    Text("IP’yi yenile")
                }
            }
        }
    }
}

@Composable
private fun StatsCard(snapshot: RelaySnapshot) {
    Card(
        modifier = Modifier.fillMaxWidth(),
        shape = RoundedCornerShape(20.dp),
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface),
    ) {
        Column(modifier = Modifier.padding(20.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Text("Canlı veri", fontWeight = FontWeight.SemiBold)
            MetricRow("Anlık bitrate", "${formatDecimal(snapshot.bitrateKbps)} kbps")
            MetricRow("Alınan veri", formatBytes(snapshot.receivedBytes))
            MetricRow("Video", "${snapshot.videoCodec ?: "—"} · ${snapshot.videoFrames} paket")
            MetricRow("Ses", "${snapshot.audioCodec ?: "—"} · ${snapshot.audioFrames} paket")
            if (snapshot.rejectedPublishAttempts > 0) {
                MetricRow("Reddedilen yayın", snapshot.rejectedPublishAttempts.toString())
            }
        }
    }
}

@Composable
private fun MetricRow(label: String, value: String) {
    Row(
        modifier = Modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.SpaceBetween,
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(label, color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.58f))
        Text(value, fontWeight = FontWeight.Medium)
    }
}

private fun generateStreamKey(): String {
    val bytes = ByteArray(18)
    SecureRandom().nextBytes(bytes)
    return Base64.encodeToString(bytes, Base64.URL_SAFE or Base64.NO_WRAP or Base64.NO_PADDING)
}

private fun findLocalIpv4Address(): String? = runCatching {
    NetworkInterface.getNetworkInterfaces()?.toList()
        ?.filter { network -> network.isUp && !network.isLoopback }
        ?.flatMap { network -> network.inetAddresses.toList().map { address -> network.name to address } }
        ?.filter { (_, address) ->
            address is Inet4Address && !address.isLoopbackAddress && !address.isLinkLocalAddress
        }
        ?.sortedBy { (name, _) ->
            when {
                name.startsWith("wlan") -> 0
                name.startsWith("ap") -> 1
                else -> 2
            }
        }
        ?.firstOrNull()
        ?.second
        ?.hostAddress
}.getOrNull()

private fun formatDecimal(value: Double): String = String.format(Locale.US, "%.0f", value)

private fun formatBytes(bytes: Long): String = when {
    bytes >= 1_000_000_000 -> "${formatDecimal(bytes / 1_000_000_000.0)} GB"
    bytes >= 1_000_000 -> "${formatDecimal(bytes / 1_000_000.0)} MB"
    bytes >= 1_000 -> "${formatDecimal(bytes / 1_000.0)} KB"
    else -> "$bytes B"
}

@Preview(showBackground = true, backgroundColor = 0xFF0B1014)
@Composable
private fun RelayScreenPreview() {
    DjiLiveBridgeTheme {
        StatusCard(
            RelaySnapshot(
                status = "publishing",
                detail = "RC 2 yayını alınıyor",
                remoteAddress = "192.168.1.42:54321",
                receivedBytes = 12_400_000,
                bitrateKbps = 6_120.0,
                videoCodec = "H264",
                audioCodec = "AAC",
                videoFrames = 1_248,
                audioFrames = 2_010,
            ),
        )
    }
}
