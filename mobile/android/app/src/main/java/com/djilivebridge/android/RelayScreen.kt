package com.djilivebridge.android

import android.Manifest
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import android.provider.Settings
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.PickVisualMediaRequest
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.isSystemInDarkTheme
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
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.rounded.HelpOutline
import androidx.compose.material.icons.rounded.Movie
import androidx.compose.material.icons.rounded.Palette
import androidx.compose.material.icons.rounded.PlayArrow
import androidx.compose.material.icons.rounded.Stop
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.RadioButton
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
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
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
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
internal fun RelayScreen(
    profileEditorViewModel: ProfileEditorViewModel,
    theme: ThemeChoice,
    onThemeChange: (ThemeChoice) -> Unit,
) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val snackbarHostState = remember { SnackbarHostState() }
    val uiPreferences = remember(context.applicationContext) { UiPreferences(context.applicationContext) }
    var showGuide by rememberSaveable { mutableStateOf(!uiPreferences.guideCompleted) }
    var showThemePicker by rememberSaveable { mutableStateOf(false) }
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

    // Keeps the DJI Fly address current: the user usually leaves the app to join a Wi-Fi network
    // or turn the hotspot on and expects to see it when they come back.
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

    /** The saved destination a platform tile stands for: the selected one, else the first. */
    fun profileFor(kind: DestinationKind): DestinationProfile? =
        destinations.selectedProfile?.takeIf { it.kind == kind }
            ?: destinations.profiles.firstOrNull { it.kind == kind }

    fun onPlatformClick(kind: DestinationKind) {
        val profile = profileFor(kind)
        when {
            profile == null -> profileEditorViewModel.open(null, kind)
            profile.id == destinations.selectedProfileId -> profileEditorViewModel.open(profile, kind)
            else -> updateProfiles { profileStore.select(profile.id) }
        }
    }

    fun startRelay(testVideo: TestVideoSelection?) {
        val profile = destinations.selectedProfile
        if (profile == null) {
            showMessage("Önce bir platform seç")
            return
        }
        // Only DJI Fly needs the local network; a test video plays over loopback.
        if (testVideo == null) {
            val refreshedAddress = findLocalLanAddress()
            lan = refreshedAddress
            if (refreshedAddress == null) {
                showMessage("Telefon bir Wi-Fi ağına bağlı değil")
                return
            }
        }
        // Lock profile selection immediately. The service repeats this state transition after
        // entering foreground, but doing it here closes the short launch-time selection race.
        RelayServiceState.starting(testVideo?.displayName)
        runCatching {
            RelayForegroundService.start(
                context = context,
                destinationProfileId = profile.id,
                testVideo = testVideo,
            )
        }.onFailure { error ->
            val message = "Yayın başlatılamadı: ${error.message ?: "Bilinmeyen hata"}"
            RelayServiceState.failed(message)
            showMessage(message)
        }
    }

    // A picked test video waits here while the notification permission prompt is open.
    var pendingTestVideo by remember { mutableStateOf<TestVideoSelection?>(null) }

    val notificationPermissionLauncher = rememberLauncherForActivityResult(
        ActivityResultContracts.RequestPermission(),
    ) { granted ->
        if (!granted) {
            showMessage("Bildirim izni verilmedi; yayını uygulamadan izleyebilirsin")
        }
        startRelay(pendingTestVideo)
        pendingTestVideo = null
    }

    fun requestStart(testVideo: TestVideoSelection? = null) {
        val needsNotificationPermission =
            Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU &&
                ContextCompat.checkSelfPermission(
                    context,
                    Manifest.permission.POST_NOTIFICATIONS,
                ) != PackageManager.PERMISSION_GRANTED
        if (needsNotificationPermission) {
            pendingTestVideo = testVideo
            notificationPermissionLauncher.launch(Manifest.permission.POST_NOTIFICATIONS)
        } else {
            startRelay(testVideo)
        }
    }

    val testVideoPicker = rememberLauncherForActivityResult(ActivityResultContracts.PickVisualMedia()) { uri ->
        if (uri == null) return@rememberLauncherForActivityResult
        scope.launch {
            withContext(Dispatchers.IO) { runCatching { inspectTestVideo(context, uri) } }
                .onSuccess { video ->
                    if (video.rotated) showMessage("Dikey çekilmiş videolar yayında yan görünebilir")
                    requestStart(video)
                }
                .onFailure { error -> showMessage(error.message ?: "Video okunamadı") }
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
            is BridgeScreen.Editor -> {
                val editing = target.state
                DestinationProfileEditor(
                    state = editing,
                    onDismiss = profileEditorViewModel::close,
                    onSave = { serverUrl, streamKey ->
                        updateProfiles(announce = false) {
                            val saved = profileStore.save(
                                existingId = editing.profile?.id,
                                name = editing.profile?.name ?: editing.kind.label,
                                kind = editing.kind,
                                serverUrl = serverUrl,
                                streamKey = streamKey,
                            )
                            // A platform the user just added is where they mean to stream.
                            val added = saved.profiles.lastOrNull { it.kind == editing.kind }
                            if (editing.profile == null && added != null) profileStore.select(added.id) else saved
                        }.also { error ->
                            if (error == null) {
                                profileEditorViewModel.close()
                                showMessage(
                                    if (editing.profile == null) "${editing.kind.label} eklendi" else "Kaydedildi",
                                )
                            }
                        }
                    },
                    onDelete = editing.profile?.let { profile ->
                        {
                            updateProfiles(announce = false) { profileStore.delete(profile.id) }.also { error ->
                                if (error == null) {
                                    profileEditorViewModel.close()
                                    showMessage("${profile.kind.label} silindi")
                                }
                            }
                        }
                    },
                )
            }
            BridgeScreen.Home -> HomeScreen(
                phase = phase,
                serviceState = serviceState,
                destinations = destinations,
                profileError = profileError,
                lan = lan,
                snackbarHostState = snackbarHostState,
                onShowGuide = { showGuide = true },
                onShowThemePicker = { showThemePicker = true },
                onPlatformClick = ::onPlatformClick,
                onPlatformLongClick = { kind -> profileFor(kind)?.let { profileEditorViewModel.open(it, kind) } },
                onEditSelected = {
                    destinations.selectedProfile?.let { profileEditorViewModel.open(it, it.kind) }
                },
                onOpenWifiSettings = ::openWifiSettings,
                onStart = { requestStart() },
                onTestVideo = {
                    testVideoPicker.launch(PickVisualMediaRequest(ActivityResultContracts.PickVisualMedia.VideoOnly))
                },
                onStop = { showStopConfirmation = true },
                onCopyAddress = ::copyAddress,
            )
        }
    }

    if (showThemePicker) {
        ThemePickerDialog(
            current = theme,
            onSelect = onThemeChange,
            onDismiss = { showThemePicker = false },
        )
    }

    if (showStopConfirmation) {
        val streaming = phase.isStreaming
        val colors = BridgeTheme.colors
        AlertDialog(
            onDismissRequest = { showStopConfirmation = false },
            containerColor = colors.card,
            title = { Text(if (streaming) "Yayın bitirilsin mi?" else "Durdurulsun mu?") },
            text = {
                val testing = serviceState.testVideoName != null
                Text(
                    when {
                        streaming && testing -> "Canlı yayın sona erer ve test videosu durur."
                        streaming -> "Canlı yayın sona erer ve kumanda bağlantısı kapanır."
                        testing -> "Test videosu durdurulur."
                        else -> "Telefon kumandadan yayın beklemeyi bırakır."
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
                    Text(if (streaming) "Yayını bitir" else "Durdur", color = colors.dangerText)
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
    onShowThemePicker: () -> Unit,
    onPlatformClick: (DestinationKind) -> Unit,
    onPlatformLongClick: (DestinationKind) -> Unit,
    onEditSelected: () -> Unit,
    onOpenWifiSettings: () -> Unit,
    onStart: () -> Unit,
    onTestVideo: () -> Unit,
    onStop: () -> Unit,
    onCopyAddress: (String) -> Unit,
) {
    val colors = BridgeTheme.colors
    val selected = destinations.selectedProfile
    val running = phase != BridgePhase.IDLE && phase != BridgePhase.START_FAILED
    Scaffold(
        containerColor = colors.background,
        contentWindowInsets = WindowInsets.safeDrawing,
        topBar = { HomeTopBar(onTheme = onShowThemePicker, onHelp = onShowGuide) },
        bottomBar = {
            HomeBottomBar(
                running = running,
                streaming = phase.isStreaming,
                blocker = when {
                    selected == null -> "Önce bir platform seç"
                    lan == null -> "Önce telefonu Wi-Fi'a bağla"
                    else -> null
                },
                canTest = selected != null,
                onStart = onStart,
                onTestVideo = onTestVideo,
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
                    .widthIn(max = 560.dp)
                    .fillMaxSize()
                    .verticalScroll(rememberScrollState())
                    .padding(horizontal = 16.dp, vertical = 4.dp),
            ) {
                if (running) {
                    LiveContent(
                        phase = phase,
                        snapshot = serviceState.snapshot,
                        liveSinceElapsedMillis = serviceState.liveSinceElapsedMillis,
                        testVideoName = serviceState.testVideoName,
                        destination = selected,
                        lan = lan,
                        onCopyAddress = onCopyAddress,
                    )
                } else {
                    SetupContent(
                        destinations = destinations,
                        profileError = profileError,
                        lan = lan,
                        startError = serviceState.snapshot.detail.takeIf { phase == BridgePhase.START_FAILED },
                        onPlatformClick = onPlatformClick,
                        onPlatformLongClick = onPlatformLongClick,
                        onEditSelected = onEditSelected,
                        onCopyAddress = onCopyAddress,
                        onOpenWifiSettings = onOpenWifiSettings,
                    )
                }
                Spacer(Modifier.height(16.dp))
            }
        }
    }
}

@Composable
private fun HomeTopBar(onTheme: () -> Unit, onHelp: () -> Unit) {
    val colors = BridgeTheme.colors
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .windowInsetsPadding(WindowInsets.safeDrawing.only(WindowInsetsSides.Top + WindowInsetsSides.Horizontal))
            .heightIn(min = 60.dp)
            .padding(start = 20.dp, end = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        BrandMark(size = 30.dp)
        Text(
            modifier = Modifier
                .weight(1f)
                .semantics { heading() },
            text = "DJI Live Bridge",
            style = MaterialTheme.typography.titleMedium,
        )
        IconButton(onClick = onTheme) {
            Icon(Icons.Rounded.Palette, contentDescription = "Tema", tint = colors.muted)
        }
        IconButton(onClick = onHelp) {
            Icon(Icons.AutoMirrored.Rounded.HelpOutline, contentDescription = "Nasıl kullanılır?", tint = colors.muted)
        }
    }
}

@Composable
private fun HomeBottomBar(
    running: Boolean,
    streaming: Boolean,
    blocker: String?,
    canTest: Boolean,
    onStart: () -> Unit,
    onTestVideo: () -> Unit,
    onStop: () -> Unit,
) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .windowInsetsPadding(WindowInsets.safeDrawing.only(WindowInsetsSides.Bottom + WindowInsetsSides.Horizontal))
            .padding(horizontal = 16.dp, vertical = 12.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        val buttonModifier = Modifier.widthIn(max = 560.dp)
        if (running) {
            StopButton(
                modifier = buttonModifier,
                text = if (streaming) "Yayını bitir" else "Durdur",
                onClick = onStop,
                icon = Icons.Rounded.Stop,
            )
        } else {
            blocker?.let {
                Text(
                    text = it,
                    style = MaterialTheme.typography.bodySmall,
                    color = BridgeTheme.colors.muted,
                    textAlign = TextAlign.Center,
                )
            }
            PrimaryButton(
                modifier = buttonModifier,
                text = "Yayını başlat",
                onClick = onStart,
                icon = Icons.Rounded.PlayArrow,
                enabled = blocker == null,
            )
            // Stands in for the drone: goes live on the selected platform without DJI Fly.
            TextButton(onClick = onTestVideo, enabled = canTest) {
                Icon(Icons.Rounded.Movie, contentDescription = null, modifier = Modifier.size(18.dp))
                Spacer(Modifier.width(8.dp))
                Text("Test videosuyla dene")
            }
        }
    }
}

@Composable
private fun ThemePickerDialog(current: ThemeChoice, onSelect: (ThemeChoice) -> Unit, onDismiss: () -> Unit) {
    val colors = BridgeTheme.colors
    AlertDialog(
        onDismissRequest = onDismiss,
        containerColor = colors.card,
        title = { Text("Tema") },
        text = {
            Column(modifier = Modifier.selectableGroup()) {
                ThemeChoice.entries.forEach { choice ->
                    Row(
                        modifier = Modifier
                            .fillMaxWidth()
                            .heightIn(min = 52.dp)
                            .clip(MaterialTheme.shapes.small)
                            .selectable(selected = choice == current, role = Role.RadioButton, onClick = { onSelect(choice) })
                            .padding(horizontal = 8.dp),
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(14.dp),
                    ) {
                        ThemeSwatch(choice)
                        Text(
                            modifier = Modifier.weight(1f),
                            text = choice.label,
                            style = MaterialTheme.typography.bodyLarge,
                            color = colors.text,
                        )
                        RadioButton(selected = choice == current, onClick = null)
                    }
                }
            }
        },
        confirmButton = {
            TextButton(onClick = onDismiss) { Text("Tamam") }
        },
    )
}

/** A small preview of a theme: its background with its accent in the middle. */
@Composable
private fun ThemeSwatch(choice: ThemeChoice) {
    val systemDark = isSystemInDarkTheme()
    val outline = BridgeTheme.colors.border
    Canvas(modifier = Modifier.size(28.dp)) {
        if (choice == ThemeChoice.SYSTEM) {
            val light = paletteFor(ThemeChoice.LIGHT, systemDark = false)
            val dark = paletteFor(ThemeChoice.DARK, systemDark = true)
            drawArc(light.background, startAngle = 90f, sweepAngle = 180f, useCenter = true)
            drawArc(dark.background, startAngle = -90f, sweepAngle = 180f, useCenter = true)
            drawCircle(light.accent, radius = 5.dp.toPx())
        } else {
            val palette = paletteFor(choice, systemDark)
            drawCircle(palette.background)
            drawCircle(palette.accent, radius = 5.dp.toPx(), center = Offset(size.width / 2, size.height / 2))
        }
        drawCircle(outline, style = Stroke(width = 1.dp.toPx()))
    }
}

private val PreviewProfile = DestinationProfile(
    "1",
    "Instagram",
    DestinationKind.INSTAGRAM,
    "rtmps://live-upload.instagram.com:443/rtmp",
)

@Preview(name = "Kurulum", widthDp = 360, heightDp = 760)
@Composable
private fun SetupPreview() {
    DjiLiveBridgeTheme(ThemeChoice.LIGHT) {
        SetupContent(
            modifier = Modifier.padding(16.dp),
            destinations = DestinationProfiles(listOf(PreviewProfile), PreviewProfile.id),
            profileError = null,
            lan = LanAddress("192.168.1.101", LanKind.WIFI),
            startError = null,
            onPlatformClick = {},
            onPlatformLongClick = {},
            onEditSelected = {},
            onCopyAddress = {},
            onOpenWifiSettings = {},
        )
    }
}

@Preview(name = "Canlı", widthDp = 360, heightDp = 760)
@Composable
private fun LivePreview() {
    DjiLiveBridgeTheme(ThemeChoice.SAND) {
        LiveContent(
            modifier = Modifier.padding(16.dp),
            phase = BridgePhase.LIVE,
            snapshot = RelaySnapshot(
                status = "publishing",
                bitrateKbps = 6_200.0,
                videoCodec = "avc1",
                audioCodec = "mp4a",
                outputStatus = "forwarding",
                outboundBytes = 184_000_000,
            ),
            liveSinceElapsedMillis = null,
            testVideoName = null,
            destination = PreviewProfile,
            lan = LanAddress("192.168.1.101", LanKind.WIFI),
            onCopyAddress = {},
        )
    }
}
