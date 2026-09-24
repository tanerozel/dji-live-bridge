package com.djilivebridge.android

import android.Manifest
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import android.provider.Settings
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.WindowInsetsSides
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.only
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.rounded.HelpOutline
import androidx.compose.material.icons.rounded.Add
import androidx.compose.material.icons.rounded.PlayArrow
import androidx.compose.material.icons.rounded.Wifi
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import androidx.core.content.ContextCompat
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.repeatOnLifecycle
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

private const val NETWORK_REFRESH_INTERVAL_MS = 3_000L

private sealed interface BridgeScreen {
    data object Home : BridgeScreen
    data object Guide : BridgeScreen
    data class Editor(val state: ProfileEditorState) : BridgeScreen
}

@Composable
internal fun RelayScreen(profileEditorViewModel: ProfileEditorViewModel) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val snackbarHostState = remember { SnackbarHostState() }
    val uiPreferences = remember(context.applicationContext) { UiPreferences(context.applicationContext) }
    var showGuide by rememberSaveable { mutableStateOf(!uiPreferences.guideCompleted) }
    var lan by remember { mutableStateOf(findLocalLanAddress()) }
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
    var showStopConfirmation by rememberSaveable { mutableStateOf(false) }
    val serviceState = RelayServiceState.value
    val phase = bridgePhase(serviceState)

    // Keeps the network step and the DJI Fly address current: the user usually leaves the app to
    // join a Wi-Fi network or turn the hotspot on and expects to see it when they come back.
    val lifecycleOwner = LocalLifecycleOwner.current
    LaunchedEffect(lifecycleOwner) {
        lifecycleOwner.repeatOnLifecycle(Lifecycle.State.STARTED) {
            while (true) {
                lan = withContext(Dispatchers.IO) { findLocalLanAddress() }
                delay(NETWORK_REFRESH_INTERVAL_MS)
            }
        }
    }

    fun showMessage(message: String) {
        scope.launch {
            snackbarHostState.currentSnackbarData?.dismiss()
            snackbarHostState.showSnackbar(message)
        }
    }

    /** Runs a store action; the error is returned and, unless the editor shows it, announced. */
    fun updateProfiles(announce: Boolean = true, action: () -> DestinationProfiles): String? = runCatching {
        action().also { updated ->
            destinations = updated
            profileError = null
        }
    }.exceptionOrNull()?.let { error ->
        (error.message ?: "Hedef işlemi tamamlanamadı").also {
            if (announce) {
                profileError = it
                showMessage(it)
            }
        }
    }

    fun startRelay() {
        val profile = destinations.selectedProfile
        if (profile == null) {
            showMessage("Önce bir yayın hedefi ekle")
            return
        }
        val refreshedAddress = findLocalLanAddress()
        lan = refreshedAddress
        if (refreshedAddress == null) {
            showMessage("Telefon bir Wi-Fi ağına bağlı değil")
            return
        }
        // Lock profile selection immediately. The service repeats this state transition after
        // entering foreground, but doing it here closes the short launch-time selection race.
        RelayServiceState.starting()
        runCatching {
            RelayForegroundService.start(
                context = context,
                destinationProfileId = profile.id,
            )
        }.onFailure { error ->
            val message = "Köprü başlatılamadı: ${error.message ?: "Bilinmeyen hata"}"
            RelayServiceState.failed(message)
            showMessage(message)
        }
    }

    val notificationPermissionLauncher = rememberLauncherForActivityResult(
        ActivityResultContracts.RequestPermission(),
    ) { granted ->
        if (!granted) {
            showMessage("Bildirim izni verilmedi; köprüyü uygulamadan izleyebilirsin")
        }
        startRelay()
    }

    fun requestStart() {
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

    fun copyAddress(url: String) {
        val clipboard = context.getSystemService(ClipboardManager::class.java)
        clipboard.setPrimaryClip(ClipData.newPlainText("DJI Fly RTMP adresi", url))
        // Android 13+ confirms clipboard writes itself.
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU) showMessage("Adres kopyalandı")
    }

    fun openWifiSettings() {
        runCatching { context.startActivity(Intent(Settings.ACTION_WIFI_SETTINGS)) }
            .onFailure { showMessage("Wi-Fi ayarları açılamadı") }
    }

    val editorState = profileEditorViewModel.state
    val screen = when {
        editorState != null -> BridgeScreen.Editor(editorState)
        showGuide -> BridgeScreen.Guide
        else -> BridgeScreen.Home
    }

    AnimatedContent(
        targetState = screen,
        transitionSpec = { fadeIn(tween(durationMillis = 220)) togetherWith fadeOut(tween(durationMillis = 150)) },
        label = "screen",
    ) { target ->
        when (target) {
            BridgeScreen.Guide -> GuideScreen(
                onFinish = {
                    uiPreferences.guideCompleted = true
                    showGuide = false
                },
            )
            is BridgeScreen.Editor -> DestinationProfileEditor(
                state = target.state,
                onDismiss = profileEditorViewModel::close,
                onSave = { name, kind, serverUrl, targetStreamKey ->
                    val wasEditing = target.state.profile != null
                    updateProfiles(announce = false) {
                        profileStore.save(
                            existingId = target.state.profile?.id,
                            name = name,
                            kind = kind,
                            serverUrl = serverUrl,
                            streamKey = targetStreamKey,
                        )
                    }.also { error ->
                        if (error == null) {
                            profileEditorViewModel.close()
                            showMessage(if (wasEditing) "Yayın hedefi güncellendi" else "Yayın hedefi eklendi")
                        }
                    }
                },
                onDelete = target.state.profile?.let { profile ->
                    {
                        updateProfiles(announce = false) { profileStore.delete(profile.id) }.also { error ->
                            if (error == null) {
                                profileEditorViewModel.close()
                                showMessage("${profile.name} silindi")
                            }
                        }
                    }
                },
            )
            BridgeScreen.Home -> HomeScreen(
                phase = phase,
                serviceState = serviceState,
                destinations = destinations,
                profileError = profileError,
                lan = lan,
                snackbarHostState = snackbarHostState,
                onShowGuide = { showGuide = true },
                onAddDestination = { kind -> profileEditorViewModel.open(null, kind) },
                onSelectDestination = { profile ->
                    if (updateProfiles { profileStore.select(profile.id) } == null) {
                        showMessage("${profile.name} seçildi")
                    }
                },
                onEditDestination = { profile -> profileEditorViewModel.open(profile) },
                onOpenWifiSettings = ::openWifiSettings,
                onStart = ::requestStart,
                onStop = { showStopConfirmation = true },
                onCopyAddress = ::copyAddress,
            )
        }
    }

    if (showStopConfirmation) {
        val streaming = phase.isStreaming
        AlertDialog(
            onDismissRequest = { showStopConfirmation = false },
            containerColor = BridgeTheme.colors.card,
            title = { Text(if (streaming) "Yayın bitirilsin mi?" else "Köprü durdurulsun mu?") },
            text = {
                Text(
                    if (streaming) {
                        "Canlı yayın sona erer ve kumanda bağlantısı kapanır."
                    } else {
                        "Telefon kumandadan yayın beklemeyi bırakır."
                    },
                )
            },
            confirmButton = {
                TextButton(
                    onClick = {
                        showStopConfirmation = false
                        RelayForegroundService.stop(context)
                    },
                ) {
                    Text(
                        text = if (streaming) "Yayını bitir" else "Durdur",
                        color = MaterialTheme.colorScheme.error,
                    )
                }
            },
            dismissButton = {
                TextButton(onClick = { showStopConfirmation = false }) {
                    Text(if (streaming) "Devam et" else "Vazgeç")
                }
            },
        )
    }
}

@Composable
private fun HomeScreen(
    phase: BridgePhase,
    serviceState: RelayServiceUiState,
    destinations: DestinationProfiles,
    profileError: String?,
    lan: LanAddress?,
    snackbarHostState: SnackbarHostState,
    onShowGuide: () -> Unit,
    onAddDestination: (DestinationKind) -> Unit,
    onSelectDestination: (DestinationProfile) -> Unit,
    onEditDestination: (DestinationProfile) -> Unit,
    onOpenWifiSettings: () -> Unit,
    onStart: () -> Unit,
    onStop: () -> Unit,
    onCopyAddress: (String) -> Unit,
) {
    val selectedProfile = destinations.selectedProfile
    val relayRunning = phase != BridgePhase.IDLE && phase != BridgePhase.START_FAILED
    Scaffold(
        containerColor = MaterialTheme.colorScheme.background,
        contentWindowInsets = WindowInsets.safeDrawing,
        topBar = { HomeTopBar(onHelp = onShowGuide) },
        bottomBar = {
            HomeActionBar(
                phase = phase,
                destination = selectedProfile,
                lan = lan,
                onAddDestination = { onAddDestination(DestinationKind.CUSTOM) },
                onOpenWifiSettings = onOpenWifiSettings,
                onStart = onStart,
                onStop = onStop,
            )
        },
        snackbarHost = { SnackbarHost(snackbarHostState) },
    ) { contentPadding ->
        Box(
            modifier = Modifier
                .fillMaxSize()
                .padding(contentPadding),
            contentAlignment = Alignment.TopCenter,
        ) {
            Column(
                modifier = Modifier
                    .widthIn(max = 640.dp)
                    .fillMaxSize()
                    .verticalScroll(rememberScrollState())
                    .padding(horizontal = 16.dp, vertical = 8.dp),
            ) {
                if (relayRunning) {
                    LiveContent(
                        phase = phase,
                        snapshot = serviceState.snapshot,
                        liveSinceElapsedMillis = serviceState.liveSinceElapsedMillis,
                        destination = selectedProfile,
                        lan = lan,
                        onCopyAddress = onCopyAddress,
                    )
                } else {
                    SetupContent(
                        destinations = destinations,
                        profileError = profileError,
                        lan = lan,
                        startError = serviceState.snapshot.detail.takeIf { phase == BridgePhase.START_FAILED },
                        onAddDestination = onAddDestination,
                        onSelectDestination = onSelectDestination,
                        onEditDestination = onEditDestination,
                    )
                }
                Spacer(Modifier.height(16.dp))
            }
        }
    }
}

@Composable
private fun HomeTopBar(onHelp: () -> Unit) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .windowInsetsPadding(WindowInsets.safeDrawing.only(WindowInsetsSides.Top + WindowInsetsSides.Horizontal))
            .heightIn(min = 64.dp)
            .padding(start = 20.dp, end = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        BrandMark(size = 40.dp)
        Column(modifier = Modifier.weight(1f)) {
            Text(
                modifier = Modifier.semantics { heading() },
                text = "DJI Live Bridge",
                style = MaterialTheme.typography.titleMedium,
            )
            Text(
                text = "Drone'dan canlı yayına köprü",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
        IconButton(onClick = onHelp) {
            Icon(Icons.AutoMirrored.Rounded.HelpOutline, contentDescription = "Nasıl kullanılır?")
        }
    }
}

/** The next action in the flow, always under the thumb; the caption says which step it is. */
@Composable
private fun HomeActionBar(
    phase: BridgePhase,
    destination: DestinationProfile?,
    lan: LanAddress?,
    onAddDestination: () -> Unit,
    onOpenWifiSettings: () -> Unit,
    onStart: () -> Unit,
    onStop: () -> Unit,
) {
    val colors = BridgeTheme.colors
    val relayRunning = phase != BridgePhase.IDLE && phase != BridgePhase.START_FAILED
    val caption = when {
        relayRunning -> when (phase) {
            BridgePhase.STARTING -> "Köprü açılıyor…"
            BridgePhase.WAITING_FOR_DRONE, BridgePhase.DRONE_CONNECTED ->
                "Köprü açık · kumandadan yayın bekleniyor"
            BridgePhase.CONNECTING_TARGET -> "Görüntü geliyor · hedefe bağlanılıyor"
            BridgePhase.LIVE -> destination?.let { "Canlı yayındasın · ${it.name}" } ?: "Canlı yayındasın"
            BridgePhase.RECONNECTING -> "Hedefe yeniden bağlanılıyor"
            else -> "Köprüyü durdurup yeniden başlat"
        }
        destination == null -> "Adım 1 / 4 · Yayının gideceği yeri ekle"
        lan == null -> "Adım 2 / 4 · Telefonu Wi-Fi'a bağla ya da hotspot'u aç"
        else -> "Adım 3 / 4 · Hazırsın, köprüyü başlat"
    }
    Surface(color = colors.card) {
        Column {
            HorizontalDivider(color = colors.cardBorder)
            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .windowInsetsPadding(
                        WindowInsets.safeDrawing.only(WindowInsetsSides.Bottom + WindowInsetsSides.Horizontal),
                    )
                    .padding(horizontal = 20.dp, vertical = 12.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.spacedBy(10.dp),
            ) {
                Text(
                    text = caption,
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    textAlign = TextAlign.Center,
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis,
                )
                val buttonModifier = Modifier.widthIn(max = 600.dp)
                when {
                    relayRunning -> StopActionButton(
                        modifier = buttonModifier,
                        text = if (phase.isStreaming) "Yayını bitir" else "Köprüyü durdur",
                        onClick = onStop,
                    )
                    destination == null -> PrimaryActionButton(
                        modifier = buttonModifier,
                        text = "Yayın hedefi ekle",
                        onClick = onAddDestination,
                        icon = Icons.Rounded.Add,
                    )
                    lan == null -> PrimaryActionButton(
                        modifier = buttonModifier,
                        text = "Wi-Fi ayarlarını aç",
                        onClick = onOpenWifiSettings,
                        icon = Icons.Rounded.Wifi,
                    )
                    else -> PrimaryActionButton(
                        modifier = buttonModifier,
                        text = "Köprüyü başlat",
                        onClick = onStart,
                        icon = Icons.Rounded.PlayArrow,
                    )
                }
            }
        }
    }
}

private val PreviewProfile = DestinationProfile("1", "Ana TikTok", DestinationKind.TIKTOK, "rtmps://example/live")

@Preview(name = "Kurulum", widthDp = 360, heightDp = 1100)
@Composable
private fun SetupPreview() {
    DjiLiveBridgeTheme(darkTheme = false) {
        Surface(color = MaterialTheme.colorScheme.background) {
            SetupContent(
                modifier = Modifier.padding(16.dp),
                destinations = DestinationProfiles(listOf(PreviewProfile), PreviewProfile.id),
                profileError = null,
                lan = LanAddress("192.168.1.101", LanKind.WIFI),
                startError = null,
                onAddDestination = {},
                onSelectDestination = {},
                onEditDestination = {},
            )
        }
    }
}

@Preview(name = "Canlı", widthDp = 360, heightDp = 1100)
@Composable
private fun LivePreview() {
    DjiLiveBridgeTheme(darkTheme = true) {
        Surface(color = MaterialTheme.colorScheme.background) {
            LiveContent(
                modifier = Modifier.padding(16.dp),
                phase = BridgePhase.LIVE,
                snapshot = RelaySnapshot(
                    status = "publishing",
                    detail = "RC 2 yayını alınıyor",
                    bitrateKbps = 6_200.0,
                    videoCodec = "H.264",
                    audioCodec = "AAC",
                    outputStatus = "forwarding",
                    outputDetail = "Yayın harici RTMP hedefine aktarılıyor",
                    outboundBytes = 184_000_000,
                ),
                liveSinceElapsedMillis = null,
                destination = PreviewProfile,
                lan = LanAddress("192.168.1.101", LanKind.WIFI),
                onCopyAddress = {},
            )
        }
    }
}
