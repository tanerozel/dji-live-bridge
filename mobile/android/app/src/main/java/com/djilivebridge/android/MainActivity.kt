package com.djilivebridge.android

import android.Manifest
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.compose.setContent
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.RadioButton
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.core.content.ContextCompat
import java.net.Inet4Address
import java.net.NetworkInterface
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
}

@Composable
private fun DjiLiveBridgeTheme(content: @Composable () -> Unit) {
    MaterialTheme(colorScheme = BridgeColors, content = content)
}

@Composable
private fun RelayScreen() {
    val context = LocalContext.current
    var localAddress by remember { mutableStateOf(findLocalIpv4Address()) }
    val profileStore = remember(context.applicationContext) {
        DestinationProfileStore(context.applicationContext)
    }
    val initialProfiles = remember(profileStore) { runCatching { profileStore.load() } }
    var destinations by remember(profileStore) {
        mutableStateOf(initialProfiles.getOrDefault(DestinationProfiles()))
    }
    var profileError by remember(profileStore) {
        mutableStateOf(initialProfiles.exceptionOrNull()?.message)
    }
    var showProfileEditor by remember { mutableStateOf(false) }
    var editingProfile by remember { mutableStateOf<DestinationProfile?>(null) }
    var deletingProfile by remember { mutableStateOf<DestinationProfile?>(null) }
    val serviceState = RelayServiceState.value
    val serverRequested = serviceState.isActive
    val relaySnapshot = serviceState.snapshot
    val publishUrl = localAddress?.let { "rtmp://$it:1935/drone" }

    fun updateProfiles(action: () -> DestinationProfiles): String? = runCatching {
        action().also { updated ->
            destinations = updated
            profileError = null
        }
    }.exceptionOrNull()?.let { error ->
        (error.message ?: "Hedef profili işlemi tamamlanamadı").also { profileError = it }
    }

    fun startRelay() {
        val selectedProfile = destinations.selectedProfile
        if (selectedProfile == null) {
            RelayServiceState.failed("Aktarımı başlatmak için bir hedef profili ekleyip seçin")
            return
        }
        localAddress = findLocalIpv4Address()
        runCatching {
            RelayForegroundService.start(
                context = context,
                destinationProfileId = selectedProfile.id,
            )
        }.onFailure { error ->
            RelayServiceState.failed("Arka plan servisi başlatılamadı: ${error.message}")
        }
    }

    val notificationPermissionLauncher = rememberLauncherForActivityResult(
        ActivityResultContracts.RequestPermission(),
    ) {
        // A foreground service may run without drawer permission; Android still shows it in
        // Active apps. Starting here keeps the action tied to the user's button press.
        startRelay()
    }

    Scaffold(
        containerColor = MaterialTheme.colorScheme.background,
        contentWindowInsets = WindowInsets.safeDrawing,
    ) { contentPadding ->
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
            DestinationProfilesCard(
                destinations = destinations,
                errorMessage = profileError,
                enabled = !serverRequested,
                onSelect = { profile ->
                    updateProfiles { profileStore.select(profile.id) }
                },
                onAdd = {
                    editingProfile = null
                    showProfileEditor = true
                },
                onEdit = { profile ->
                    editingProfile = profile
                    showProfileEditor = true
                },
                onDelete = { profile -> deletingProfile = profile },
            )
            OutputStatusCard(relaySnapshot)
            StatsCard(relaySnapshot)
            Button(
                modifier = Modifier.fillMaxWidth(),
                onClick = {
                    if (serverRequested) {
                        RelayForegroundService.stop(context)
                    } else {
                        val needsNotificationPermission =
                            Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU &&
                                ContextCompat.checkSelfPermission(
                                    context,
                                    Manifest.permission.POST_NOTIFICATIONS,
                                ) != PackageManager.PERMISSION_GRANTED
                        if (needsNotificationPermission) {
                            notificationPermissionLauncher.launch(Manifest.permission.POST_NOTIFICATIONS)
                        } else {
                            startRelay()
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
                Text(if (serverRequested) "RTMP aktarımını durdur" else "RTMP aktarımını başlat")
            }
            Text(
                text = "Aktarım başka uygulamaya geçtiğinizde ve ekran kilitlendiğinde foreground service ile devam eder. Android 15+ arka planda dataSync servislerini 24 saatte toplam 6 saatle sınırlar.",
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.55f),
                fontSize = 12.sp,
                lineHeight = 18.sp,
            )
            Surface(
                color = MaterialTheme.colorScheme.primary.copy(alpha = 0.1f),
                shape = RoundedCornerShape(14.dp),
            ) {
                Text(
                    modifier = Modifier.padding(horizontal = 16.dp, vertical = 12.dp),
                    text = "Faz 7 / 8  •  Cihaz ve ağ doğrulaması",
                    color = MaterialTheme.colorScheme.primary,
                    fontSize = 13.sp,
                    fontWeight = FontWeight.Medium,
                )
            }
        }
    }

    if (showProfileEditor) {
        DestinationProfileEditor(
            profile = editingProfile,
            onDismiss = {
                showProfileEditor = false
                editingProfile = null
            },
            onSave = { name, kind, serverUrl, targetStreamKey ->
                updateProfiles {
                    profileStore.save(
                        existingId = editingProfile?.id,
                        name = name,
                        kind = kind,
                        serverUrl = serverUrl,
                        streamKey = targetStreamKey,
                    )
                }.also { error ->
                    if (error == null) {
                        showProfileEditor = false
                        editingProfile = null
                    }
                }
            },
        )
    }

    deletingProfile?.let { profile ->
        AlertDialog(
            onDismissRequest = { deletingProfile = null },
            title = { Text("Hedef profilini sil") },
            text = { Text("${profile.name} profili ve şifreli yayın anahtarı silinecek.") },
            confirmButton = {
                TextButton(
                    onClick = {
                        updateProfiles { profileStore.delete(profile.id) }
                        deletingProfile = null
                    },
                ) {
                    Text("Sil", color = MaterialTheme.colorScheme.error)
                }
            },
            dismissButton = {
                TextButton(onClick = { deletingProfile = null }) { Text("Vazgeç") }
            },
        )
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
        Text(text = "Android RTMP relay", fontSize = 30.sp, fontWeight = FontWeight.SemiBold)
        Text(
            text = "Telefonu ve DJI RC 2’yi aynı Wi-Fi ağına bağla. Aşağıdaki adresi DJI Fly özel RTMP alanına gir.",
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.68f),
            fontSize = 15.sp,
            lineHeight = 22.sp,
        )
    }
}

@Composable
private fun DestinationProfilesCard(
    destinations: DestinationProfiles,
    errorMessage: String?,
    enabled: Boolean,
    onSelect: (DestinationProfile) -> Unit,
    onAdd: () -> Unit,
    onEdit: (DestinationProfile) -> Unit,
    onDelete: (DestinationProfile) -> Unit,
) {
    Card(
        modifier = Modifier.fillMaxWidth(),
        shape = RoundedCornerShape(20.dp),
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface),
    ) {
        Column(modifier = Modifier.padding(20.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Text("Hedef profilleri", fontWeight = FontWeight.SemiBold)
            Text(
                text = "TikTok, YouTube veya özel RTMP hedefini kaydet. Yayın anahtarları Android Keystore ile şifrelenir.",
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.62f),
                fontSize = 13.sp,
                lineHeight = 19.sp,
            )
            if (destinations.profiles.isEmpty()) {
                Surface(
                    modifier = Modifier.fillMaxWidth(),
                    color = MaterialTheme.colorScheme.background,
                    shape = RoundedCornerShape(14.dp),
                ) {
                    Text(
                        modifier = Modifier.padding(16.dp),
                        text = "Henüz hedef yok. Aktarımı başlatmadan önce bir profil ekleyin.",
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.62f),
                        fontSize = 13.sp,
                        lineHeight = 19.sp,
                    )
                }
            } else {
                destinations.profiles.forEach { profile ->
                    DestinationProfileRow(
                        profile = profile,
                        selected = profile.id == destinations.selectedProfileId,
                        enabled = enabled,
                        onSelect = { onSelect(profile) },
                        onEdit = { onEdit(profile) },
                        onDelete = { onDelete(profile) },
                    )
                }
            }
            errorMessage?.let { message ->
                Text(
                    text = message,
                    color = MaterialTheme.colorScheme.error,
                    fontSize = 13.sp,
                    lineHeight = 19.sp,
                )
            }
            FilledTonalButton(onClick = onAdd, enabled = enabled) {
                Text("Hedef profili ekle")
            }
            Text(
                text = if (enabled) {
                    "Aktarımda yalnızca seçili hedef kullanılır. RTMPS sertifika ve hostname doğrulaması zorunludur."
                } else {
                    "Aktarım sürerken hedef seçimi ve profil değişiklikleri kilitlidir."
                },
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.48f),
                fontSize = 12.sp,
            )
        }
    }
}

@Composable
private fun DestinationProfileRow(
    profile: DestinationProfile,
    selected: Boolean,
    enabled: Boolean,
    onSelect: () -> Unit,
    onEdit: () -> Unit,
    onDelete: () -> Unit,
) {
    Surface(
        modifier = Modifier.fillMaxWidth(),
        color = if (selected) {
            MaterialTheme.colorScheme.primary.copy(alpha = 0.11f)
        } else {
            MaterialTheme.colorScheme.background
        },
        shape = RoundedCornerShape(14.dp),
    ) {
        Column(
            modifier = Modifier.padding(horizontal = 12.dp, vertical = 10.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                RadioButton(selected = selected, onClick = onSelect, enabled = enabled)
                Column(modifier = Modifier.fillMaxWidth()) {
                    Text(profile.name, fontWeight = FontWeight.Medium)
                    Text(
                        text = "${profile.kind.label} · ${profile.serverUrl}",
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.55f),
                        fontSize = 12.sp,
                        lineHeight = 17.sp,
                    )
                    Text(
                        text = "Yayın anahtarı güvenli kayıtlı",
                        color = MaterialTheme.colorScheme.primary.copy(alpha = 0.78f),
                        fontSize = 12.sp,
                    )
                }
            }
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                TextButton(onClick = onEdit, enabled = enabled) { Text("Düzenle") }
                TextButton(onClick = onDelete, enabled = enabled) {
                    Text("Sil", color = if (enabled) MaterialTheme.colorScheme.error else Color.Unspecified)
                }
            }
        }
    }
}

@Composable
private fun DestinationProfileEditor(
    profile: DestinationProfile?,
    onDismiss: () -> Unit,
    onSave: (String, DestinationKind, String, String) -> String?,
) {
    var name by remember(profile?.id) { mutableStateOf(profile?.name.orEmpty()) }
    var kind by remember(profile?.id) { mutableStateOf(profile?.kind ?: DestinationKind.CUSTOM) }
    var serverUrl by remember(profile?.id) { mutableStateOf(profile?.serverUrl.orEmpty()) }
    var targetStreamKey by remember(profile?.id) { mutableStateOf("") }
    var editorError by remember(profile?.id) { mutableStateOf<String?>(null) }

    fun selectKind(selected: DestinationKind) {
        kind = selected
        if (profile == null || name.isBlank()) name = selected.label
        if (selected == DestinationKind.YOUTUBE && serverUrl.isBlank()) {
            serverUrl = "rtmps://a.rtmps.youtube.com/live2"
        }
    }

    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(if (profile == null) "Hedef profili ekle" else "Hedef profilini düzenle") },
        text = {
            Column(
                modifier = Modifier
                    .heightIn(max = 480.dp)
                    .verticalScroll(rememberScrollState()),
                verticalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                Text(
                    text = "Hedef türü",
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.65f),
                    fontSize = 13.sp,
                )
                Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                    DestinationKind.entries.forEach { option ->
                        if (kind == option) {
                            Button(
                                onClick = { selectKind(option) },
                                contentPadding = PaddingValues(horizontal = 8.dp),
                            ) {
                                Text(
                                    text = if (option == DestinationKind.CUSTOM) "Özel" else option.label,
                                    maxLines = 1,
                                    softWrap = false,
                                )
                            }
                        } else {
                            FilledTonalButton(
                                onClick = { selectKind(option) },
                                contentPadding = PaddingValues(horizontal = 8.dp),
                            ) {
                                Text(
                                    text = if (option == DestinationKind.CUSTOM) "Özel" else option.label,
                                    maxLines = 1,
                                    softWrap = false,
                                )
                            }
                        }
                    }
                }
                OutlinedTextField(
                    modifier = Modifier.fillMaxWidth(),
                    value = name,
                    onValueChange = { name = it },
                    label = { Text("Profil adı") },
                    placeholder = { Text("Örn. Ana TikTok hesabı") },
                    singleLine = true,
                )
                OutlinedTextField(
                    modifier = Modifier.fillMaxWidth(),
                    value = serverUrl,
                    onValueChange = { serverUrl = it },
                    label = { Text("RTMP sunucu adresi") },
                    placeholder = { Text("rtmps://sunucu.example/live") },
                    singleLine = true,
                )
                OutlinedTextField(
                    modifier = Modifier.fillMaxWidth(),
                    value = targetStreamKey,
                    onValueChange = { targetStreamKey = it },
                    label = {
                        Text(if (profile == null) "Yayın anahtarı" else "Yeni yayın anahtarı")
                    },
                    placeholder = {
                        Text(if (profile == null) "Hedef yayın anahtarı" else "Değiştirmeyecekseniz boş bırakın")
                    },
                    visualTransformation = PasswordVisualTransformation(),
                    singleLine = true,
                )
                Text(
                    text = "Anahtar ekranda açık gösterilmez ve cihaz dışına çıkarılamayan Keystore anahtarıyla şifrelenir.",
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.48f),
                    fontSize = 12.sp,
                    lineHeight = 17.sp,
                )
                editorError?.let { message ->
                    Text(message, color = MaterialTheme.colorScheme.error, fontSize = 13.sp)
                }
            }
        },
        confirmButton = {
            TextButton(
                onClick = {
                    editorError = onSave(name, kind, serverUrl, targetStreamKey)
                    if (editorError == null) targetStreamKey = ""
                },
            ) {
                Text("Kaydet")
            }
        },
        dismissButton = {
            TextButton(
                onClick = {
                    targetStreamKey = ""
                    onDismiss()
                },
            ) {
                Text("Vazgeç")
            }
        },
    )
}

@Composable
private fun OutputStatusCard(snapshot: RelaySnapshot) {
    val statusColor = when (snapshot.outputStatus) {
        "forwarding", "ready" -> MaterialTheme.colorScheme.primary
        "connecting", "reconnecting" -> Color(0xFFFFD166)
        "error" -> MaterialTheme.colorScheme.error
        else -> MaterialTheme.colorScheme.onSurface.copy(alpha = 0.45f)
    }
    val title = when (snapshot.outputStatus) {
        "forwarding" -> "Hedefe aktarılıyor"
        "ready" -> "Hedef hazır"
        "connecting" -> "Hedefe bağlanıyor"
        "reconnecting" -> "Hedefe yeniden bağlanıyor"
        "armed" -> "Hedef beklemede"
        "error" -> "Çıkış hatası"
        "stopped" -> "Aktarım durdu"
        else -> "Hedef kapalı"
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
                text = snapshot.outputDetail,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.65f),
                lineHeight = 20.sp,
            )
        }
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
            Text(
                text = "Sabit /drone adresi yalnızca güvendiğiniz yerel Wi-Fi ağında kullanın.",
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.48f),
                fontSize = 12.sp,
                lineHeight = 17.sp,
            )
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
            MetricRow("Hedefe iletilen", formatBytes(snapshot.outboundBytes))
            MetricRow("Video", "${snapshot.videoCodec ?: "—"} · ${snapshot.videoFrames} paket")
            MetricRow("Ses", "${snapshot.audioCodec ?: "—"} · ${snapshot.audioFrames} paket")
            if (snapshot.rejectedPublishAttempts > 0) {
                MetricRow("Reddedilen yayın", snapshot.rejectedPublishAttempts.toString())
            }
            if (snapshot.droppedOutputFrames > 0) {
                MetricRow("Atlanan çıkış paketi", snapshot.droppedOutputFrames.toString())
            }
            if (snapshot.outputReconnectAttempts > 0) {
                MetricRow("Yeniden bağlantı", snapshot.outputReconnectAttempts.toString())
            }
            MetricRow(
                "Hedef güvenliği",
                when {
                    snapshot.outputSecure && snapshot.outputStatus in setOf("ready", "forwarding") ->
                        "RTMPS · sertifika doğrulandı"
                    snapshot.outputSecure -> "RTMPS · doğrulama bekleniyor"
                    else -> "RTMP · şifresiz"
                },
            )
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
                outputStatus = "forwarding",
                outputDetail = "Yayın harici RTMP hedefine aktarılıyor",
                outboundBytes = 12_100_000,
            ),
        )
    }
}
