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
import androidx.annotation.RequiresApi
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
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.rounded.HelpOutline
import androidx.compose.material.icons.rounded.Language
import androidx.compose.material.icons.rounded.Palette
import androidx.compose.material.icons.rounded.PlayArrow
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
import androidx.compose.runtime.SideEffect
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
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
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
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

private const val NETWORK_REFRESH_INTERVAL_MS = 3_000L

/** How long the drone screen stays after the picture stops, so a short drop does not flip screens. */
private const val PICTURE_GRACE_MS = 2_500L

private sealed interface BridgeScreen {
    data object Home : BridgeScreen

    /** The drone's picture over the whole screen. */
    data object Drone : BridgeScreen
    data object Guide : BridgeScreen
    data class Editor(val state: ProfileEditorState) : BridgeScreen
}

@Composable
internal fun RelayScreen(
    profileEditorViewModel: ProfileEditorViewModel,
    theme: ThemeChoice,
    onThemeChange: (ThemeChoice) -> Unit,
    /** True while the drone screen shows, whose dark picture needs light system bar icons. */
    onDroneScreenChange: (Boolean) -> Unit,
) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val snackbarHostState = remember { SnackbarHostState() }
    val uiPreferences = remember(context.applicationContext) { UiPreferences(context.applicationContext) }
    var showGuide by rememberSaveable { mutableStateOf(!uiPreferences.guideCompleted) }
    var showThemePicker by rememberSaveable { mutableStateOf(false) }
    var showLanguagePicker by rememberSaveable { mutableStateOf(false) }
    var lan by remember { mutableStateOf(findLocalLanAddress()) }
    val profileStore = remember(context.applicationContext) {
        DestinationProfileStore(context.applicationContext)
    }
    val initialProfiles = remember(profileStore) { runCatching { profileStore.load() } }
    var destinations by remember(profileStore) {
        mutableStateOf(initialProfiles.getOrDefault(DestinationProfiles()))
    }
    var profileError by remember(profileStore) {
        mutableStateOf(initialProfiles.exceptionOrNull()?.let(::profileErrorText))
    }
    var showEndLiveConfirmation by rememberSaveable { mutableStateOf(false) }
    // The platform whose broadcast the dialog ends, or null for all of them.
    var endLiveProfileId by rememberSaveable { mutableStateOf<String?>(null) }
    val serviceState = RelayServiceState.value
    val phase = bridgePhase(serviceState)
    var pictureFit by remember { mutableStateOf(uiPreferences.pictureFit) }
    // The drone screen opens with the picture and stays through a short drop.
    var showingPicture by remember { mutableStateOf(phase.hasPicture) }
    LaunchedEffect(phase.hasPicture) {
        if (phase.hasPicture) {
            showingPicture = true
        } else {
            delay(PICTURE_GRACE_MS)
            showingPicture = false
        }
    }

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

    fun showMessage(message: UiText) {
        scope.launch {
            snackbarHostState.currentSnackbarData?.dismiss()
            snackbarHostState.showSnackbar(message.resolve(context))
        }
    }

    /** Runs a store action; the error is returned and, unless the editor shows it, announced. */
    fun updateProfiles(announce: Boolean = true, action: () -> DestinationProfiles): UiText? = runCatching {
        action().also { updated ->
            destinations = updated
            profileError = null
        }
    }.exceptionOrNull()?.let { error ->
        profileErrorText(error).also {
            if (announce) {
                profileError = it
                showMessage(it)
            }
        }
    }

    /** The saved destination a platform tile stands for: a selected one, else the first. */
    fun profileFor(kind: DestinationKind): DestinationProfile? =
        destinations.selectedProfiles.firstOrNull { it.kind == kind }
            ?: destinations.profiles.firstOrNull { it.kind == kind }

    /** A saved platform goes in or out of the broadcast; a new one is set up first. */
    fun onPlatformClick(kind: DestinationKind) {
        val profile = profileFor(kind)
        if (profile == null) {
            profileEditorViewModel.open(null, kind)
        } else {
            updateProfiles { profileStore.setSelected(profile.id, !destinations.isSelected(profile.id)) }
        }
    }

    fun startReceiver(testVideo: TestVideoSelection? = null, restart: Boolean = false) {
        if (!RelayServiceState.value.isActive) RelayServiceState.starting()
        runCatching { RelayForegroundService.startReceiver(context, testVideo, restart) }
            .onFailure { error -> RelayServiceState.failed(serviceStartError(error)) }
    }

    // The receiver opens whenever the app is on screen, so DJI Fly can connect right away and
    // its picture shows before anything goes to a platform.
    LaunchedEffect(lifecycleOwner, showGuide) {
        if (showGuide) return@LaunchedEffect
        lifecycleOwner.repeatOnLifecycle(Lifecycle.State.STARTED) {
            if (!RelayServiceState.value.isActive) startReceiver()
            awaitCancellation()
        }
    }

    fun goLive() {
        val profileIds = destinations.selectedProfiles.map { it.id }
        if (profileIds.isEmpty()) {
            showMessage(uiText(R.string.pick_platform_first))
            return
        }
        // Switch to the live screen now; the service confirms or reports a failure.
        RelayServiceState.goingLive(profileIds)
        runCatching { RelayForegroundService.goLive(context, profileIds) }
            .onFailure { error ->
                RelayServiceState.notLive(profileIds, RelayNotice(uiText(R.string.go_live_failed), serviceStartError(error)))
            }
    }

    val notificationPermissionLauncher = rememberLauncherForActivityResult(
        ActivityResultContracts.RequestPermission(),
    ) { granted ->
        if (!granted) {
            showMessage(uiText(R.string.notification_permission_denied))
        }
        goLive()
    }

    fun requestGoLive() {
        val needsNotificationPermission =
            Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU &&
                ContextCompat.checkSelfPermission(
                    context,
                    Manifest.permission.POST_NOTIFICATIONS,
                ) != PackageManager.PERMISSION_GRANTED
        if (needsNotificationPermission) {
            notificationPermissionLauncher.launch(Manifest.permission.POST_NOTIFICATIONS)
        } else {
            goLive()
        }
    }

    val testVideoPicker = rememberLauncherForActivityResult(ActivityResultContracts.PickVisualMedia()) { uri ->
        if (uri == null) return@rememberLauncherForActivityResult
        scope.launch {
            withContext(Dispatchers.IO) { runCatching { inspectTestVideo(context, uri) } }
                .onSuccess { video ->
                    if (video.rotated) showMessage(uiText(R.string.test_video_rotated))
                    startReceiver(video)
                }
                .onFailure { error -> showMessage((error as? TestVideoException)?.text ?: uiText(R.string.test_video_unreadable)) }
        }
    }

    val clipLabel = stringResource(R.string.clip_address_label)

    fun copyAddress(url: String) {
        val clipboard = context.getSystemService(ClipboardManager::class.java)
        clipboard.setPrimaryClip(ClipData.newPlainText(clipLabel, url))
        // Android 13+ confirms clipboard writes itself.
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU) showMessage(uiText(R.string.address_copied))
    }

    fun openWifiSettings() {
        runCatching { context.startActivity(Intent(Settings.ACTION_WIFI_SETTINGS)) }
            .onFailure { showMessage(uiText(R.string.wifi_settings_failed)) }
    }

    val editorState = profileEditorViewModel.state
    val screen = when {
        editorState != null -> BridgeScreen.Editor(editorState)
        showGuide -> BridgeScreen.Guide
        serviceState.isLive || showingPicture -> BridgeScreen.Drone
        else -> BridgeScreen.Home
    }
    SideEffect { onDroneScreenChange(screen == BridgeScreen.Drone) }

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
                        // The store adds a new platform to the ones the broadcast goes to.
                        updateProfiles(announce = false) {
                            profileStore.save(
                                existingId = editing.profile?.id,
                                name = editing.profile?.name ?: editing.kind.displayName(context),
                                kind = editing.kind,
                                serverUrl = serverUrl,
                                streamKey = streamKey,
                            )
                        }.also { error ->
                            if (error == null) {
                                profileEditorViewModel.close()
                                showMessage(
                                    if (editing.profile == null) {
                                        uiText(R.string.platform_added, editing.kind.displayName(context))
                                    } else {
                                        uiText(R.string.saved)
                                    },
                                )
                            }
                        }
                    },
                    onDelete = editing.profile?.let { profile ->
                        {
                            updateProfiles(announce = false) { profileStore.delete(profile.id) }.also { error ->
                                if (error == null) {
                                    profileEditorViewModel.close()
                                    showMessage(uiText(R.string.platform_deleted, profile.kind.displayName(context)))
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
                onShowLanguagePicker = { showLanguagePicker = true },
                onPlatformClick = ::onPlatformClick,
                onPlatformLongClick = { kind -> profileFor(kind)?.let { profileEditorViewModel.open(it, kind) } },
                onEditProfile = { profile -> profileEditorViewModel.open(profile, profile.kind) },
                onOpenWifiSettings = ::openWifiSettings,
                onStartReceiver = { restart -> startReceiver(restart = restart) },
                onTestVideo = {
                    testVideoPicker.launch(PickVisualMediaRequest(ActivityResultContracts.PickVisualMedia.VideoOnly))
                },
                onStopTestVideo = { RelayForegroundService.stopTestVideo(context) },
                onCopyAddress = ::copyAddress,
            )
            BridgeScreen.Drone -> DroneScreen(
                phase = phase,
                serviceState = serviceState,
                destinations = destinations,
                lan = lan,
                pictureFit = pictureFit,
                snackbarHostState = snackbarHostState,
                onPictureFitChange = { fit ->
                    pictureFit = fit
                    uiPreferences.pictureFit = fit
                },
                onGoLive = ::requestGoLive,
                onEndLive = { profileId ->
                    endLiveProfileId = profileId
                    showEndLiveConfirmation = true
                },
                onPlatformClick = ::onPlatformClick,
                onPlatformLongClick = { kind -> profileFor(kind)?.let { profileEditorViewModel.open(it, kind) } },
                onEditProfile = { profile -> profileEditorViewModel.open(profile, profile.kind) },
                onStopTestVideo = {
                    // Back to the setup screen at once, without waiting out the grace period.
                    if (!serviceState.isLive) showingPicture = false
                    RelayForegroundService.stopTestVideo(context)
                },
                onShowLanguagePicker = { showLanguagePicker = true },
                onShowThemePicker = { showThemePicker = true },
                onShowGuide = { showGuide = true },
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

    if (showLanguagePicker && Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
        LanguagePickerDialog(onDismiss = { showLanguagePicker = false })
    }

    if (showEndLiveConfirmation) {
        val colors = BridgeTheme.colors
        val ending = endLiveProfileId?.let { id -> destinations.profiles.firstOrNull { it.id == id } }
        val others = serviceState.liveProfileIds.size > 1
        AlertDialog(
            onDismissRequest = { showEndLiveConfirmation = false },
            containerColor = colors.card,
            title = {
                Text(
                    if (ending != null) {
                        stringResource(R.string.end_one_title, ending.kind.displayName())
                    } else {
                        stringResource(R.string.end_all_title)
                    },
                )
            },
            text = {
                Text(
                    when {
                        ending != null ->
                            stringResource(R.string.end_one_message, stringResource(ending.kind.onPlatform)).sentenceStart()
                        serviceState.testVideoName != null ->
                            stringResource(if (others) R.string.end_all_test_video_many else R.string.end_all_test_video)
                        else -> stringResource(if (others) R.string.end_all_drone_many else R.string.end_all_drone)
                    },
                )
            },
            confirmButton = {
                TextButton(
                    onClick = {
                        showEndLiveConfirmation = false
                        val profileId = endLiveProfileId
                        RelayServiceState.notLive(profileId?.let(::listOf))
                        RelayForegroundService.endLive(context, profileId)
                    },
                ) {
                    Text(stringResource(R.string.end_broadcast), color = colors.dangerText)
                }
            },
            dismissButton = {
                TextButton(onClick = { showEndLiveConfirmation = false }) { Text(stringResource(R.string.keep_streaming)) }
            },
        )
    }
}

@Composable
private fun HomeScreen(
    phase: BridgePhase,
    serviceState: RelayServiceUiState,
    destinations: DestinationProfiles,
    profileError: UiText?,
    lan: LanAddress?,
    snackbarHostState: SnackbarHostState,
    onShowGuide: () -> Unit,
    onShowThemePicker: () -> Unit,
    onShowLanguagePicker: () -> Unit,
    onPlatformClick: (DestinationKind) -> Unit,
    onPlatformLongClick: (DestinationKind) -> Unit,
    onEditProfile: (DestinationProfile) -> Unit,
    onOpenWifiSettings: () -> Unit,
    onStartReceiver: (restart: Boolean) -> Unit,
    onTestVideo: () -> Unit,
    onStopTestVideo: () -> Unit,
    onCopyAddress: (String) -> Unit,
) {
    val colors = BridgeTheme.colors
    val selected = destinations.selectedProfiles
    Scaffold(
        containerColor = colors.background,
        contentWindowInsets = WindowInsets.safeDrawing,
        topBar = { HomeTopBar(onLanguage = onShowLanguagePicker, onTheme = onShowThemePicker, onHelp = onShowGuide) },
        bottomBar = {
            // Going live happens on the drone screen; here the button shows what comes next.
            HomeBottomBar(
                platforms = selected.size,
                hint = stringResource(if (selected.isEmpty()) R.string.pick_platform_first else R.string.go_live_needs_picture),
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
                SetupContent(
                    phase = phase,
                    snapshot = serviceState.snapshot,
                    testVideoName = serviceState.testVideoName,
                    notice = serviceState.notice,
                    destinations = destinations,
                    profileError = profileError,
                    lan = lan,
                    onStartReceiver = onStartReceiver,
                    onTestVideo = onTestVideo,
                    onStopTestVideo = onStopTestVideo,
                    onPlatformClick = onPlatformClick,
                    onPlatformLongClick = onPlatformLongClick,
                    onEditProfile = onEditProfile,
                    onCopyAddress = onCopyAddress,
                    onOpenWifiSettings = onOpenWifiSettings,
                )
                Spacer(Modifier.height(16.dp))
            }
        }
    }
}

@Composable
private fun HomeTopBar(onLanguage: () -> Unit, onTheme: () -> Unit, onHelp: () -> Unit) {
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
            text = stringResource(R.string.app_name),
            style = MaterialTheme.typography.titleMedium,
        )
        // Android 13 and later keep a language per app; older versions follow the phone.
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            IconButton(onClick = onLanguage) {
                Icon(Icons.Rounded.Language, contentDescription = stringResource(R.string.language), tint = colors.muted)
            }
        }
        IconButton(onClick = onTheme) {
            Icon(Icons.Rounded.Palette, contentDescription = stringResource(R.string.theme), tint = colors.muted)
        }
        IconButton(onClick = onHelp) {
            Icon(Icons.AutoMirrored.Rounded.HelpOutline, contentDescription = stringResource(R.string.how_to_use), tint = colors.muted)
        }
    }
}

@Composable
private fun HomeBottomBar(platforms: Int, hint: String) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .windowInsetsPadding(WindowInsets.safeDrawing.only(WindowInsetsSides.Bottom + WindowInsetsSides.Horizontal))
            .padding(horizontal = 16.dp, vertical = 12.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Text(
            text = hint,
            style = MaterialTheme.typography.bodySmall,
            color = BridgeTheme.colors.muted,
            textAlign = TextAlign.Center,
        )
        PrimaryButton(
            modifier = Modifier.widthIn(max = 560.dp),
            text = if (platforms > 1) {
                pluralStringResource(R.plurals.go_live_many, platforms, platforms)
            } else {
                stringResource(R.string.go_live)
            },
            onClick = {},
            icon = Icons.Rounded.PlayArrow,
            enabled = false,
        )
    }
}

@Composable
private fun ThemePickerDialog(current: ThemeChoice, onSelect: (ThemeChoice) -> Unit, onDismiss: () -> Unit) {
    val colors = BridgeTheme.colors
    AlertDialog(
        onDismissRequest = onDismiss,
        containerColor = colors.card,
        title = { Text(stringResource(R.string.theme)) },
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
                            text = stringResource(choice.label),
                            style = MaterialTheme.typography.bodyLarge,
                            color = colors.text,
                        )
                        RadioButton(selected = choice == current, onClick = null)
                    }
                }
            }
        },
        confirmButton = {
            TextButton(onClick = onDismiss) { Text(stringResource(R.string.ok)) }
        },
    )
}

/** The phone's language or one of the app's; the system applies it at once. */
@RequiresApi(Build.VERSION_CODES.TIRAMISU)
@Composable
private fun LanguagePickerDialog(onDismiss: () -> Unit) {
    val context = LocalContext.current
    val colors = BridgeTheme.colors
    // Read again after the system applies a choice and the configuration changes.
    val current = remember(LocalConfiguration.current) { AppLanguageSetting.current(context) }
    val options = listOf<AppLanguage?>(null) + APP_LANGUAGES
    AlertDialog(
        onDismissRequest = onDismiss,
        containerColor = colors.card,
        title = { Text(stringResource(R.string.language)) },
        text = {
            Column(
                modifier = Modifier
                    .selectableGroup()
                    .verticalScroll(rememberScrollState()),
            ) {
                options.forEach { language ->
                    val selected = language == current
                    Row(
                        modifier = Modifier
                            .fillMaxWidth()
                            .heightIn(min = 48.dp)
                            .clip(MaterialTheme.shapes.small)
                            .selectable(
                                selected = selected,
                                role = Role.RadioButton,
                                onClick = { if (!selected) AppLanguageSetting.set(context, language) },
                            )
                            .padding(horizontal = 8.dp),
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(14.dp),
                    ) {
                        Text(
                            modifier = Modifier.weight(1f),
                            text = language?.name ?: stringResource(R.string.language_system),
                            style = MaterialTheme.typography.bodyLarge,
                            color = colors.text,
                        )
                        RadioButton(selected = selected, onClick = null)
                    }
                }
            }
        },
        confirmButton = {
            TextButton(onClick = onDismiss) { Text(stringResource(R.string.ok)) }
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

private val PreviewTwitch = DestinationProfile("2", "Twitch", DestinationKind.TWITCH, "rtmp://live.twitch.tv/app")

private val PreviewProfile = DestinationProfile(
    "1",
    "Instagram",
    DestinationKind.INSTAGRAM,
    "rtmps://live-upload.instagram.com:443/rtmp",
)

@Preview(name = "Setup", widthDp = 360, heightDp = 760)
@Composable
private fun SetupPreview() {
    DjiLiveBridgeTheme(ThemeChoice.LIGHT) {
        SetupContent(
            modifier = Modifier.padding(16.dp),
            phase = BridgePhase.WAITING_FOR_DRONE,
            snapshot = RelaySnapshot(status = "listening"),
            testVideoName = null,
            notice = null,
            destinations = DestinationProfiles(listOf(PreviewProfile), listOf(PreviewProfile.id)),
            profileError = null,
            lan = LanAddress("192.168.1.101", LanKind.WIFI),
            onStartReceiver = {},
            onTestVideo = {},
            onStopTestVideo = {},
            onPlatformClick = {},
            onPlatformLongClick = {},
            onEditProfile = {},
            onCopyAddress = {},
            onOpenWifiSettings = {},
        )
    }
}

@Preview(name = "Live", widthDp = 360, heightDp = 760)
@Composable
private fun LivePreview() {
    DjiLiveBridgeTheme(ThemeChoice.LIGHT) {
        DroneScreen(
            phase = BridgePhase.LIVE,
            serviceState = RelayServiceUiState(
                isActive = true,
                snapshot = RelaySnapshot(
                    status = "publishing",
                    bitrateKbps = 6_200.0,
                    videoCodec = "avc1",
                    audioCodec = "mp4a",
                    outputs = listOf(
                        OutputSnapshot(PreviewProfile.id, status = "forwarding", outboundBytes = 184_000_000),
                        OutputSnapshot(PreviewTwitch.id, status = "congested", outboundBytes = 150_000_000),
                    ),
                ),
                liveProfileIds = listOf(PreviewProfile.id, PreviewTwitch.id),
            ),
            destinations = DestinationProfiles(listOf(PreviewProfile, PreviewTwitch), listOf(PreviewProfile.id, PreviewTwitch.id)),
            lan = LanAddress("192.168.1.101", LanKind.WIFI),
            pictureFit = PictureFit.WHOLE,
            snackbarHostState = remember { SnackbarHostState() },
            onPictureFitChange = {},
            onGoLive = {},
            onEndLive = {},
            onPlatformClick = {},
            onPlatformLongClick = {},
            onEditProfile = {},
            onStopTestVideo = {},
            onShowLanguagePicker = {},
            onShowThemePicker = {},
            onShowGuide = {},
        )
    }
}

/** A saved-platforms failure in the app's words; an unexpected one keeps its technical cause. */
private fun profileErrorText(error: Throwable): UiText = when (error) {
    is DestinationProfileException -> error.text
    else -> withCause(R.string.profile_error_generic, error)
}

/** The background service could not be started or reached. */
private fun serviceStartError(error: Throwable): UiText = withCause(R.string.error_service_start, error)
