package com.djilivebridge.android

import android.os.Build
import androidx.annotation.StringRes
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.animateDpAsState
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicText
import androidx.compose.foundation.text.TextAutoSize
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.rounded.HelpOutline
import androidx.compose.material.icons.automirrored.rounded.KeyboardArrowRight
import androidx.compose.material.icons.rounded.Add
import androidx.compose.material.icons.rounded.CheckCircle
import androidx.compose.material.icons.rounded.Close
import androidx.compose.material.icons.rounded.ContentCopy
import androidx.compose.material.icons.rounded.ErrorOutline
import androidx.compose.material.icons.rounded.ExpandMore
import androidx.compose.material.icons.rounded.Fullscreen
import androidx.compose.material.icons.rounded.FullscreenExit
import androidx.compose.material.icons.rounded.GridView
import androidx.compose.material.icons.rounded.Hd
import androidx.compose.material.icons.rounded.Image
import androidx.compose.material.icons.rounded.Info
import androidx.compose.material.icons.rounded.Key
import androidx.compose.material.icons.rounded.Lan
import androidx.compose.material.icons.rounded.Language
import androidx.compose.material.icons.rounded.Lightbulb
import androidx.compose.material.icons.rounded.MoreHoriz
import androidx.compose.material.icons.rounded.Movie
import androidx.compose.material.icons.rounded.Palette
import androidx.compose.material.icons.rounded.Refresh
import androidx.compose.material.icons.rounded.Sensors
import androidx.compose.material.icons.rounded.Settings
import androidx.compose.material.icons.rounded.SignalCellularAlt
import androidx.compose.material.icons.rounded.Stop
import androidx.compose.material.icons.rounded.WarningAmber
import androidx.compose.material.icons.rounded.Wifi
import androidx.compose.material.icons.rounded.WifiOff
import androidx.compose.material.icons.rounded.WifiTethering
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.IconButtonDefaults
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Snackbar
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Text
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.semantics.toggleableState
import androidx.compose.ui.state.ToggleableState
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextDirection
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

/** Which sheet is open over the picture. */
private enum class DroneSheet { PLATFORMS, DETAILS }

/**
 * The app's one screen, laid out like a camera app: what is connected and the stream's state at
 * the top, a switch per platform and the one big button at the bottom. Until the drone's picture
 * comes, a small card in the middle says how to connect DJI Fly; then the picture fills the screen.
 * "Preview" hides everything but the picture; a double tap switches between the whole picture and
 * a screen-filling one.
 */
@Composable
internal fun DroneScreen(
    phase: BridgePhase,
    serviceState: RelayServiceUiState,
    /** The picture is up (or was, moments ago); otherwise the connect card takes its place. */
    showPicture: Boolean,
    destinations: DestinationProfiles,
    /** Why the saved platforms could not be read, if they could not. */
    profileError: UiText?,
    lan: LanAddress?,
    pictureFit: PictureFit,
    snackbarHostState: SnackbarHostState,
    onPictureFitChange: (PictureFit) -> Unit,
    /** Goes live on every chosen platform. */
    onGoLive: () -> Unit,
    /** Ends the broadcast on one platform, or on all of them for null. */
    onEndLive: (profileId: String?) -> Unit,
    /** A platform's switch: choose it for the next broadcast, or while live start or end it there. */
    onPlatformToggle: (DestinationKind) -> Unit,
    onPlatformEdit: (DestinationKind) -> Unit,
    onEditProfile: (DestinationProfile) -> Unit,
    onStartReceiver: (restart: Boolean) -> Unit,
    onTestVideo: () -> Unit,
    onStopTestVideo: () -> Unit,
    onCopyAddress: (String) -> Unit,
    onOpenWifiSettings: () -> Unit,
    onShowLanguagePicker: () -> Unit,
    onShowThemePicker: () -> Unit,
    onShowGuide: () -> Unit,
) {
    val snapshot = serviceState.snapshot
    val liveDestinations = serviceState.liveProfileIds.mapNotNull { id -> destinations.profiles.firstOrNull { it.id == id } }
    val pictureState = rememberDronePictureState()
    var sheet by rememberSaveable { mutableStateOf<DroneSheet?>(null) }
    var pictureOnly by rememberSaveable { mutableStateOf(false) }
    if (!showPicture) pictureOnly = false
    val otherFit = if (pictureState.shownFit == PictureFit.WHOLE) PictureFit.FILL else PictureFit.WHOLE
    if (showPicture) KeepScreenOn()
    // A picture that stopped coming must not look live, but a gap of a moment, such as DJI Fly
    // reconnecting at once, should not flash the screen.
    val stopped by produceState(initialValue = !phase.hasPicture, phase.hasPicture) {
        if (phase.hasPicture) {
            value = false
        } else {
            delay(DIM_DELAY_MS)
            value = true
        }
    }
    val dim by animateFloatAsState(if (stopped) DIM_ALPHA else 0f, label = "dim")

    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(if (showPicture) SolidColor(Color.Black) else WaitingBackground),
    ) {
        if (showPicture) {
            DronePicture(state = pictureState, fit = pictureFit, modifier = Modifier.fillMaxSize())
            Box(
                modifier = Modifier
                    .fillMaxSize()
                    .background(Color.Black.copy(alpha = dim))
                    .pointerInput(otherFit, pictureOnly) {
                        detectTapGestures(
                            onDoubleTap = { onPictureFitChange(otherFit) },
                            onTap = { if (pictureOnly) pictureOnly = false },
                        )
                    },
            )
        }
        AnimatedVisibility(visible = !pictureOnly, enter = fadeIn(), exit = fadeOut()) {
            Box(modifier = Modifier.fillMaxSize()) {
                if (showPicture) Scrims()
                Controls(
                    phase = phase,
                    serviceState = serviceState,
                    showPicture = showPicture,
                    destinations = destinations,
                    liveDestinations = liveDestinations,
                    profileError = profileError,
                    lan = lan,
                    pictureState = pictureState,
                    snackbarHostState = snackbarHostState,
                    onDetails = { sheet = DroneSheet.DETAILS },
                    onPlatforms = { sheet = DroneSheet.PLATFORMS },
                    onFitToggle = { onPictureFitChange(otherFit) },
                    onPictureOnly = { pictureOnly = true },
                    onGoLive = onGoLive,
                    onEndLive = onEndLive,
                    onPlatformToggle = onPlatformToggle,
                    onPlatformEdit = onPlatformEdit,
                    onStartReceiver = onStartReceiver,
                    onTestVideo = onTestVideo,
                    onStopTestVideo = onStopTestVideo,
                    onCopyAddress = onCopyAddress,
                    onOpenWifiSettings = onOpenWifiSettings,
                    onShowLanguagePicker = onShowLanguagePicker,
                    onShowThemePicker = onShowThemePicker,
                    onShowGuide = onShowGuide,
                )
            }
        }
        if (pictureOnly) ControlsHint(modifier = Modifier.align(Alignment.BottomCenter))
    }

    when (sheet) {
        DroneSheet.PLATFORMS -> BridgeSheet(onDismiss = { sheet = null }) {
            PlatformsSheet(
                destinations = destinations,
                liveDestinations = liveDestinations,
                live = serviceState.isLive,
                snapshot = snapshot,
                onToggle = onPlatformToggle,
                onEdit = onPlatformEdit,
                onEditProfile = onEditProfile,
            )
        }
        DroneSheet.DETAILS -> BridgeSheet(onDismiss = { sheet = null }) {
            LiveDetails(
                phase = phase,
                snapshot = snapshot,
                destinations = liveDestinations,
                lan = lan,
                onEndPlatform = { profile -> onEndLive(profile.id) },
            )
        }
        null -> Unit
    }
}

/** Everything over the picture, laid out for an upright phone or one turned sideways. */
@Composable
private fun Controls(
    phase: BridgePhase,
    serviceState: RelayServiceUiState,
    showPicture: Boolean,
    destinations: DestinationProfiles,
    liveDestinations: List<DestinationProfile>,
    profileError: UiText?,
    lan: LanAddress?,
    pictureState: DronePictureState,
    snackbarHostState: SnackbarHostState,
    onDetails: () -> Unit,
    onPlatforms: () -> Unit,
    onFitToggle: () -> Unit,
    onPictureOnly: () -> Unit,
    onGoLive: () -> Unit,
    onEndLive: (profileId: String?) -> Unit,
    onPlatformToggle: (DestinationKind) -> Unit,
    onPlatformEdit: (DestinationKind) -> Unit,
    onStartReceiver: (restart: Boolean) -> Unit,
    onTestVideo: () -> Unit,
    onStopTestVideo: () -> Unit,
    onCopyAddress: (String) -> Unit,
    onOpenWifiSettings: () -> Unit,
    onShowLanguagePicker: () -> Unit,
    onShowThemePicker: () -> Unit,
    onShowGuide: () -> Unit,
) {
    val snapshot = serviceState.snapshot
    val live = serviceState.isLive
    val testing = serviceState.testVideoName != null
    val deviceCard: @Composable (Modifier) -> Unit = { modifier ->
        DeviceCard(phase = phase, testing = testing, onClick = onDetails, modifier = modifier)
    }
    val statusCard: @Composable (Modifier) -> Unit = { modifier ->
        StatusCard(
            look = statusLook(phase, snapshot, live, serviceState.liveSinceElapsedMillis, LiveTarget(liveDestinations)),
            modifier = modifier,
        )
    }
    val chips: @Composable () -> Unit = {
        InfoChips(
            videoSize = pictureState.videoSize?.takeIf { showPicture }?.let { "${it.width}×${it.height}" },
            bitrateKbps = snapshot.bitrateKbps,
            lan = lan,
        )
    }
    val connect: @Composable (Modifier) -> Unit = { modifier ->
        if (!showPicture) {
            ConnectPanel(
                phase = phase,
                snapshot = snapshot,
                testing = testing,
                converting = serviceState.testVideoConverting,
                lan = lan,
                onStartReceiver = onStartReceiver,
                onTestVideo = onTestVideo,
                onCopyAddress = onCopyAddress,
                onOpenWifiSettings = onOpenWifiSettings,
                modifier = modifier,
            )
        }
    }
    val sideButtons: @Composable () -> Unit = {
        Column(verticalArrangement = Arrangement.spacedBy(GAP)) {
            val whole = pictureState.shownFit == PictureFit.WHOLE
            if (showPicture) {
                GlassIconButton(
                    icon = if (whole) Icons.Rounded.Fullscreen else Icons.Rounded.FullscreenExit,
                    description = stringResource(if (whole) R.string.picture_fill else R.string.picture_fit),
                    onClick = onFitToggle,
                )
            }
            MenuButton(
                items = buildList {
                    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
                        add(MenuEntry(R.string.language, Icons.Rounded.Language, onShowLanguagePicker))
                    }
                    add(MenuEntry(R.string.theme, Icons.Rounded.Palette, onShowThemePicker))
                    add(MenuEntry(R.string.how_to_use, Icons.AutoMirrored.Rounded.HelpOutline, onShowGuide))
                },
            ) { open -> GlassIconButton(icon = Icons.Rounded.Settings, description = stringResource(R.string.settings), onClick = open) }
        }
    }
    val bottom: @Composable ColumnScope.() -> Unit = {
        SnackbarHost(snackbarHostState) { data ->
            Snackbar(
                snackbarData = data,
                shape = CARD_SHAPE,
                containerColor = SheetColor,
                contentColor = Color.White,
                actionColor = GoLiveStart,
            )
        }
        Message(
            phase = phase,
            serviceState = serviceState,
            showPicture = showPicture,
            profileError = profileError,
            liveDestinations = liveDestinations,
            lan = lan,
        )
        StreamToCard(
            destinations = destinations,
            liveDestinations = liveDestinations,
            live = live,
            snapshot = snapshot,
            onToggle = onPlatformToggle,
            onEdit = onPlatformEdit,
            onMore = onPlatforms,
        )
    }
    val actions: @Composable (Modifier) -> Unit = { modifier ->
        ActionRow(
            live = live,
            canGoLive = phase.hasPicture,
            showPicture = showPicture,
            testing = testing,
            snackbarHostState = snackbarHostState,
            onPictureOnly = onPictureOnly,
            onGoLive = onGoLive,
            onEndLive = { onEndLive(null) },
            onPlatforms = onPlatforms,
            onDetails = onDetails,
            onStopTestVideo = onStopTestVideo,
            modifier = modifier,
        )
    }

    BoxWithConstraints(
        modifier = Modifier
            .fillMaxSize()
            .windowInsetsPadding(WindowInsets.safeDrawing)
            .padding(horizontal = EDGE, vertical = GAP),
    ) {
        if (maxWidth > maxHeight && !showPicture) {
            // Sideways without a picture: the connect card beside the platforms, not squeezed
            // between the top and bottom rows.
            Row(modifier = Modifier.fillMaxSize(), horizontalArrangement = Arrangement.spacedBy(EDGE)) {
                Column(
                    modifier = Modifier
                        .weight(1f)
                        .fillMaxHeight(),
                    verticalArrangement = Arrangement.spacedBy(GAP),
                ) {
                    deviceCard(Modifier.fillMaxWidth())
                    chips()
                    Spacer(modifier = Modifier.weight(1f))
                    bottom()
                }
                Column(
                    modifier = Modifier
                        .weight(1f)
                        .fillMaxHeight(),
                    verticalArrangement = Arrangement.spacedBy(GAP),
                ) {
                    Row(verticalAlignment = Alignment.Top, horizontalArrangement = Arrangement.spacedBy(GAP)) {
                        statusCard(Modifier.weight(1f))
                        sideButtons()
                    }
                    Box(
                        modifier = Modifier
                            .weight(1f)
                            .fillMaxWidth(),
                        contentAlignment = Alignment.Center,
                    ) {
                        connect(Modifier.verticalScroll(rememberScrollState()))
                    }
                    actions(Modifier.fillMaxWidth())
                }
            }
        } else if (maxWidth > maxHeight) {
            // Sideways: the state along the top, the platforms and the buttons along the bottom.
            Column(modifier = Modifier.fillMaxSize(), verticalArrangement = Arrangement.spacedBy(GAP)) {
                Row(modifier = Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(GAP)) {
                    Column(modifier = Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(GAP)) {
                        deviceCard(Modifier.widthIn(max = 280.dp))
                        chips()
                    }
                    Column(horizontalAlignment = Alignment.End, verticalArrangement = Arrangement.spacedBy(GAP)) {
                        statusCard(Modifier.widthIn(max = 220.dp))
                        sideButtons()
                    }
                }
                Spacer(modifier = Modifier.weight(1f))
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    verticalAlignment = Alignment.Bottom,
                    horizontalArrangement = Arrangement.spacedBy(EDGE),
                ) {
                    Column(
                        modifier = Modifier
                            .weight(1f)
                            .widthIn(max = 440.dp),
                        verticalArrangement = Arrangement.spacedBy(GAP),
                        content = bottom,
                    )
                    actions(Modifier.width(LANDSCAPE_ACTIONS_WIDTH))
                }
            }
        } else {
            Column(modifier = Modifier.fillMaxSize(), verticalArrangement = Arrangement.spacedBy(GAP)) {
                Row(horizontalArrangement = Arrangement.spacedBy(GAP), verticalAlignment = Alignment.Top) {
                    deviceCard(Modifier.weight(1.15f))
                    statusCard(Modifier.weight(1f))
                }
                chips()
                Box(
                    modifier = Modifier
                        .weight(1f)
                        .fillMaxWidth(),
                    contentAlignment = Alignment.Center,
                ) {
                    // The card takes the middle; the settings button sits above its corner.
                    connect(
                        Modifier
                            .padding(top = SIDE_BUTTON + GAP)
                            .verticalScroll(rememberScrollState()),
                    )
                    Box(modifier = Modifier.align(Alignment.TopEnd)) { sideButtons() }
                }
                bottom()
                actions(Modifier.fillMaxWidth())
            }
        }
    }
}

/** Darkens the top and bottom edges so the controls over the picture stay readable. */
@Composable
private fun Scrims() {
    Box(modifier = Modifier.fillMaxSize()) {
        Box(
            modifier = Modifier
                .align(Alignment.TopCenter)
                .fillMaxWidth()
                .height(180.dp)
                .background(Brush.verticalGradient(listOf(Color.Black.copy(alpha = 0.55f), Color.Transparent))),
        )
        Box(
            modifier = Modifier
                .align(Alignment.BottomCenter)
                .fillMaxWidth()
                .height(300.dp)
                .background(Brush.verticalGradient(listOf(Color.Transparent, Color.Black.copy(alpha = 0.7f)))),
        )
    }
}

/** What sends the picture, and whether it is connected; opens the connection details. */
@Composable
private fun DeviceCard(phase: BridgePhase, testing: Boolean, onClick: () -> Unit, modifier: Modifier = Modifier) {
    val (dot, state) = when {
        phase == BridgePhase.RECEIVER_ERROR || phase == BridgePhase.START_FAILED ->
            DangerColor to R.string.hero_receiver_stopped
        phase.hasPicture || phase == BridgePhase.DRONE_CONNECTED -> ReadyColor to R.string.device_connected
        else -> WarningColor to R.string.device_waiting
    }
    Row(
        modifier = modifier
            .glass(CARD_SHAPE)
            .clickable(onClickLabel = stringResource(R.string.show_details), role = Role.Button, onClick = onClick)
            .heightIn(min = CARD_HEIGHT)
            .padding(horizontal = 10.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        if (testing) {
            Icon(Icons.Rounded.Movie, contentDescription = null, tint = Color.White, modifier = Modifier.size(ICON))
        } else {
            Icon(painterResource(R.drawable.ic_drone), contentDescription = null, tint = Color.White, modifier = Modifier.size(ICON))
        }
        Column(modifier = Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(1.dp)) {
            FittedText(
                text = stringResource(if (testing) R.string.device_test_video else R.string.device_drone),
                style = MaterialTheme.typography.titleSmall.copy(color = Color.White, fontWeight = FontWeight.SemiBold),
            )
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(5.dp)) {
                Box(
                    modifier = Modifier
                        .size(6.dp)
                        .background(dot, CircleShape),
                )
                Text(
                    text = stringResource(state),
                    color = Color.White.copy(alpha = 0.8f),
                    style = MaterialTheme.typography.bodySmall,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
        Icon(
            imageVector = Icons.AutoMirrored.Rounded.KeyboardArrowRight,
            contentDescription = null,
            tint = Color.White.copy(alpha = 0.7f),
            modifier = Modifier.size(18.dp),
        )
    }
}

/** The colors of a state: ready, live, working on it, a warning or an error. */
private enum class Tone(val accent: Color, val tint: Color) {
    READY(ReadyColor, Color(0xFF052E16)),
    LIVE(Color(0xFFF87171), Color(0xFF450A0A)),
    BUSY(Color.White, Color(0xFF111827)),
    WARNING(WarningColor, Color(0xFF451A03)),
    ERROR(DangerColor, Color(0xFF450A0A)),
}

private enum class Leading { CHECK, LIVE_DOT, PROGRESS, WARNING, ERROR }

private class StatusLook(
    val tone: Tone,
    val leading: Leading,
    val headline: String?,
    val caption: (@Composable () -> Unit)?,
)

/** The stream's state in a word, with a line under it, for the card in the top corner. */
@Composable
private fun statusLook(
    phase: BridgePhase,
    snapshot: RelaySnapshot,
    live: Boolean,
    liveSinceElapsedMillis: Long?,
    target: LiveTarget,
): StatusLook {
    fun text(value: String): @Composable () -> Unit = { StatusCaption(value) }
    if (!live) {
        return when (phase) {
            BridgePhase.PREVIEW ->
                StatusLook(Tone.READY, Leading.CHECK, stringResource(R.string.status_ready), text(stringResource(R.string.status_not_live)))
            BridgePhase.IDLE, BridgePhase.START_FAILED, BridgePhase.RECEIVER_ERROR ->
                StatusLook(Tone.ERROR, Leading.ERROR, stringResource(R.string.hero_receiver_stopped), null)
            BridgePhase.STARTING -> StatusLook(Tone.BUSY, Leading.PROGRESS, null, text(stringResource(R.string.receiver_starting)))
            else -> StatusLook(Tone.BUSY, Leading.PROGRESS, null, text(stringResource(R.string.status_waiting_for_picture)))
        }
    }
    return when (phase) {
        BridgePhase.LIVE -> {
            val congested = snapshot.outputs.any { it.status == "congested" }
            val detail = if (congested) stringResource(R.string.status_congested) else formatBitrate(snapshot.bitrateKbps)
            StatusLook(
                tone = if (congested) Tone.WARNING else Tone.LIVE,
                leading = if (congested) Leading.WARNING else Leading.LIVE_DOT,
                headline = stringResource(R.string.live_badge),
                caption = {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        liveSinceElapsedMillis?.let { since ->
                            LiveTimer(since, color = Color.White, style = MaterialTheme.typography.labelSmall)
                            StatusCaption(" · ")
                        }
                        StatusCaption(detail)
                    }
                },
            )
        }
        BridgePhase.PREVIEW, BridgePhase.CONNECTING_TARGET ->
            StatusLook(Tone.BUSY, Leading.PROGRESS, stringResource(R.string.status_connecting), text(target.label()))
        BridgePhase.RECONNECTING ->
            StatusLook(Tone.WARNING, Leading.WARNING, stringResource(R.string.status_reconnecting), text(target.label()))
        BridgePhase.STARTING, BridgePhase.WAITING_FOR_DRONE, BridgePhase.DRONE_CONNECTED ->
            if (snapshot.outputStatus == "holding") {
                StatusLook(Tone.WARNING, Leading.WARNING, null, text(stringResource(R.string.status_holding)))
            } else {
                StatusLook(Tone.BUSY, Leading.PROGRESS, null, text(stringResource(R.string.status_waiting_for_picture)))
            }
        BridgePhase.RECEIVER_ERROR -> StatusLook(Tone.ERROR, Leading.ERROR, stringResource(R.string.hero_receiver_stopped), null)
        BridgePhase.IDLE, BridgePhase.START_FAILED -> StatusLook(Tone.ERROR, Leading.ERROR, stringResource(R.string.status_stopped), null)
    }
}

@Composable
private fun StatusCard(look: StatusLook, modifier: Modifier = Modifier) {
    val locale = LocalConfiguration.current.locales[0]
    Row(
        modifier = modifier
            .clip(CARD_SHAPE)
            .background(look.tone.tint.copy(alpha = 0.78f))
            .border(1.dp, look.tone.accent.copy(alpha = 0.75f), CARD_SHAPE)
            .heightIn(min = CARD_HEIGHT)
            .padding(horizontal = 10.dp, vertical = 8.dp)
            .semantics(mergeDescendants = true) { liveRegion = LiveRegionMode.Polite },
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        when (look.leading) {
            Leading.CHECK -> Icon(Icons.Rounded.CheckCircle, contentDescription = null, tint = look.tone.accent, modifier = Modifier.size(ICON))
            Leading.LIVE_DOT -> PulsingDot(color = look.tone.accent, size = 10.dp)
            Leading.PROGRESS -> CircularProgressIndicator(modifier = Modifier.size(16.dp), color = Color.White, strokeWidth = 2.dp)
            Leading.WARNING -> Icon(Icons.Rounded.WarningAmber, contentDescription = null, tint = look.tone.accent, modifier = Modifier.size(20.dp))
            Leading.ERROR -> Icon(Icons.Rounded.ErrorOutline, contentDescription = null, tint = look.tone.accent, modifier = Modifier.size(20.dp))
        }
        Column(verticalArrangement = Arrangement.spacedBy(1.dp)) {
            look.headline?.let {
                // One line that shrinks to fit: "RECONNECTING" is long in most languages.
                BasicText(
                    text = it.uppercase(locale),
                    style = MaterialTheme.typography.titleMedium.copy(color = look.tone.accent, fontWeight = FontWeight.Bold),
                    maxLines = 1,
                    autoSize = TextAutoSize.StepBased(minFontSize = 10.sp, maxFontSize = 16.sp),
                )
            }
            look.caption?.invoke()
        }
    }
}

@Composable
private fun StatusCaption(text: String) {
    Text(
        text = text,
        color = Color.White.copy(alpha = 0.9f),
        style = MaterialTheme.typography.labelSmall,
        maxLines = 2,
        overflow = TextOverflow.Ellipsis,
    )
}

/** The picture's size, the incoming bitrate and how the phone is connected. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun InfoChips(videoSize: String?, bitrateKbps: Double, lan: LanAddress?) {
    FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
        // Numbers read left to right in every language: "1280×720" must not turn into "720×1280".
        videoSize?.let { InfoChip(Icons.Rounded.Hd, it, numbers = true) }
        if (bitrateKbps > 0) InfoChip(Icons.Rounded.SignalCellularAlt, formatBitrate(bitrateKbps), numbers = true)
        lan?.kind?.let { kind ->
            val icon = when (kind) {
                LanKind.WIFI -> Icons.Rounded.Wifi
                LanKind.HOTSPOT -> Icons.Rounded.WifiTethering
                LanKind.WIRED, LanKind.OTHER -> Icons.Rounded.Lan
            }
            InfoChip(icon, networkName(kind))
        }
    }
}

@Composable
private fun InfoChip(icon: ImageVector, text: String, numbers: Boolean = false) {
    Row(
        modifier = Modifier
            .glass(CircleShape)
            .padding(horizontal = 10.dp, vertical = 5.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(5.dp),
    ) {
        Icon(icon, contentDescription = null, tint = Color.White, modifier = Modifier.size(14.dp))
        Text(
            text = text,
            color = Color.White,
            style = MaterialTheme.typography.labelMedium.let { if (numbers) it.copy(textDirection = TextDirection.Ltr) else it },
            maxLines = 1,
        )
    }
}

/**
 * The one message worth reading now: why going live failed, why the drone's picture is gone, or
 * the tip for the platform (such as pressing "Go live" in Instagram too).
 */
@Composable
private fun Message(
    phase: BridgePhase,
    serviceState: RelayServiceUiState,
    showPicture: Boolean,
    profileError: UiText?,
    liveDestinations: List<DestinationProfile>,
    lan: LanAddress?,
) {
    val snapshot = serviceState.snapshot
    val notice = serviceState.notice
    val holding = serviceState.isLive && snapshot.outputStatus == "holding" &&
        (phase == BridgePhase.WAITING_FOR_DRONE || phase == BridgePhase.DRONE_CONNECTED)
    // A test video starting and stopping counts as reconnects; only a flight says anything.
    val weakLink = showPicture && serviceState.testVideoName == null &&
        snapshot.recentInterruptions >= WEAK_LINK_INTERRUPTIONS
    when {
        notice != null -> Banner(
            key = notice,
            title = notice.title.asString(),
            message = notice.message.asString(),
            icon = Icons.Rounded.ErrorOutline,
            accent = DangerColor,
        )
        profileError != null -> Banner(
            key = profileError,
            title = stringResource(R.string.profile_list_unreadable),
            message = profileError.asString(),
            icon = Icons.Rounded.ErrorOutline,
            accent = DangerColor,
        )
        holding -> Banner(
            key = "holding",
            title = stringResource(R.string.hero_drone_lost),
            message = stringResource(R.string.hero_drone_lost_subtitle),
            icon = Icons.Rounded.WarningAmber,
            accent = WarningColor,
        )
        phase == BridgePhase.RECEIVER_ERROR && showPicture -> Banner(
            key = "receiver",
            title = stringResource(R.string.hero_receiver_stopped),
            message = snapshot.error?.asString(),
            icon = Icons.Rounded.ErrorOutline,
            accent = DangerColor,
        )
        weakLink -> Banner(
            key = "weak_link",
            title = stringResource(R.string.weak_link_title),
            message = stringResource(
                if (lan?.kind == LanKind.HOTSPOT) R.string.weak_link_tip_hotspot else R.string.weak_link_tip,
            ),
            icon = Icons.Rounded.WifiOff,
            accent = WarningColor,
        )
        else -> liveTips(phase, snapshot, liveDestinations).firstOrNull()?.let { tip ->
            Banner(key = tip, title = null, message = tip, icon = Icons.Rounded.Lightbulb, accent = WarningColor)
        }
    }
}

/** A note over the picture that can be closed; a different message shows again. */
@Composable
private fun Banner(key: Any, title: String?, message: String?, icon: ImageVector, accent: Color) {
    var closed by remember(key) { mutableStateOf(false) }
    if (closed) return
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .glass(CARD_SHAPE)
            .padding(start = 12.dp, top = 8.dp, bottom = 8.dp)
            .semantics(mergeDescendants = true) { liveRegion = LiveRegionMode.Polite },
        verticalAlignment = Alignment.Top,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Icon(
            icon,
            contentDescription = null,
            tint = accent,
            modifier = Modifier
                .padding(top = 1.dp)
                .size(16.dp),
        )
        Column(modifier = Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(1.dp)) {
            title?.let { Text(text = it, color = Color.White, style = MaterialTheme.typography.labelLarge) }
            message?.let { Text(text = it, color = Color.White.copy(alpha = 0.85f), style = MaterialTheme.typography.bodySmall) }
        }
        IconButton(onClick = { closed = true }, modifier = Modifier.size(32.dp)) {
            Icon(
                Icons.Rounded.Close,
                contentDescription = stringResource(R.string.close),
                tint = Color.White.copy(alpha = 0.8f),
                modifier = Modifier.size(16.dp),
            )
        }
    }
}

/**
 * A switch per platform. Before going live it chooses where the broadcast goes; while live it
 * starts or ends the broadcast on that platform. A platform without a stream key asks for one.
 */
@Composable
private fun StreamToCard(
    destinations: DestinationProfiles,
    liveDestinations: List<DestinationProfile>,
    live: Boolean,
    snapshot: RelaySnapshot,
    onToggle: (DestinationKind) -> Unit,
    onEdit: (DestinationKind) -> Unit,
    onMore: () -> Unit,
) {
    val saved = destinations.profiles.mapTo(mutableSetOf()) { it.kind }
    // The user's own platforms first, then the four big ones; the others once they have a key.
    // Switching one on or off never moves it.
    val shown = DestinationKind.entries.filter { it in saved } +
        DestinationKind.entries.filter { it in MAIN_PLATFORMS && it !in saved }
    val hidden = DestinationKind.entries.size > shown.size
    val on = if (live) {
        liveDestinations.mapTo(mutableSetOf()) { it.kind }
    } else {
        destinations.selectedProfiles.mapTo(mutableSetOf()) { it.kind }
    }
    val colors = BridgeTheme.colors
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .glass(RoundedCornerShape(18.dp))
            .padding(10.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Row(modifier = Modifier.padding(horizontal = 2.dp), verticalAlignment = Alignment.CenterVertically) {
            Text(
                modifier = Modifier.weight(1f),
                text = stringResource(R.string.platforms),
                color = Color.White,
                style = MaterialTheme.typography.labelLarge,
                fontWeight = FontWeight.SemiBold,
            )
            Text(
                text = stringResource(R.string.platforms_active, shown.count { it in on }, shown.size),
                color = Color.White.copy(alpha = 0.7f),
                style = MaterialTheme.typography.labelMedium,
            )
        }
        BoxWithConstraints(modifier = Modifier.fillMaxWidth()) {
            val tileWidth = ((maxWidth - TILE_SPACING * (VISIBLE_TILES - 1)) / VISIBLE_TILES).coerceAtLeast(MIN_TILE_WIDTH)
            Row(
                modifier = Modifier.horizontalScroll(rememberScrollState()),
                horizontalArrangement = Arrangement.spacedBy(TILE_SPACING),
            ) {
                shown.forEach { kind ->
                    val liveProfile = liveDestinations.firstOrNull { it.kind == kind }
                    PlatformSwitch(
                        kind = kind,
                        checked = kind in on,
                        saved = kind in saved,
                        state = liveProfile?.let { outputLabel(snapshot.output(it.id)?.status, colors).second },
                        onToggle = { onToggle(kind) },
                        onEdit = { onEdit(kind) },
                        modifier = Modifier.width(tileWidth),
                    )
                }
                if (hidden) OtherPlatforms(onClick = onMore, modifier = Modifier.width(tileWidth))
            }
        }
    }
}

@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun PlatformSwitch(
    kind: DestinationKind,
    checked: Boolean,
    saved: Boolean,
    /** The platform's state as a dot while live. */
    state: Color?,
    onToggle: () -> Unit,
    onEdit: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val name = kind.displayName()
    val notAdded = stringResource(R.string.state_not_added)
    val editLabel = stringResource(R.string.edit)
    Column(
        modifier = modifier
            .clip(TILE_SHAPE)
            .background(Color.White.copy(alpha = if (checked) 0.12f else 0.05f))
            .border(1.dp, if (checked) GoLiveStart.copy(alpha = 0.8f) else Color.White.copy(alpha = 0.12f), TILE_SHAPE)
            .combinedClickable(
                onLongClickLabel = if (saved) editLabel else null,
                onLongClick = if (saved) onEdit else null,
                onClick = onToggle,
            )
            .semantics(mergeDescendants = true) {
                role = Role.Switch
                toggleableState = ToggleableState(checked)
                contentDescription = name
                if (!saved) stateDescription = notAdded
            }
            .padding(vertical = 8.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Box {
            PlatformTile(kind = kind, size = TILE_ICON)
            state?.let {
                Box(
                    modifier = Modifier
                        .align(Alignment.BottomEnd)
                        .offset(x = 3.dp, y = 3.dp)
                        .size(9.dp)
                        .background(it, CircleShape)
                        .border(1.5.dp, Color.Black, CircleShape),
                )
            }
        }
        FittedText(
            text = name,
            style = MaterialTheme.typography.labelSmall.copy(color = Color.White.copy(alpha = if (saved) 1f else 0.7f)),
            modifier = Modifier.padding(horizontal = 4.dp),
        )
        MiniSwitch(checked = checked)
    }
}

/** One line that shrinks rather than cutting off a name at a large font size. */
@Composable
private fun FittedText(text: String, style: TextStyle, modifier: Modifier = Modifier) {
    BasicText(
        text = text,
        modifier = modifier,
        style = style,
        maxLines = 1,
        autoSize = TextAutoSize.StepBased(minFontSize = 8.sp, maxFontSize = style.fontSize),
    )
}

/** A small switch that fits a platform tile; the whole tile is the control. */
@Composable
private fun MiniSwitch(checked: Boolean) {
    val thumbOffset by animateDpAsState(if (checked) MINI_SWITCH_TRAVEL else 0.dp, label = "miniSwitch")
    Box(
        modifier = Modifier
            .size(width = 32.dp, height = 18.dp)
            .clip(CircleShape)
            .background(if (checked) GoLiveStart else Color.White.copy(alpha = 0.18f))
            .border(1.dp, if (checked) GoLiveStart else Color.White.copy(alpha = 0.3f), CircleShape)
            .padding(3.dp),
        contentAlignment = Alignment.CenterStart,
    ) {
        Box(
            modifier = Modifier
                .offset { IntOffset(thumbOffset.roundToPx(), 0) }
                .size(12.dp)
                .background(Color.White.copy(alpha = if (checked) 1f else 0.85f), CircleShape),
        )
    }
}

/** Twitch, Kick and custom servers, until they have a stream key. */
@Composable
private fun OtherPlatforms(onClick: () -> Unit, modifier: Modifier = Modifier) {
    Column(
        modifier = modifier
            .clip(TILE_SHAPE)
            .border(1.dp, Color.White.copy(alpha = 0.12f), TILE_SHAPE)
            .clickable(role = Role.Button, onClick = onClick)
            .padding(vertical = 8.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Box(
            modifier = Modifier
                .size(TILE_ICON)
                .border(1.dp, Color.White.copy(alpha = 0.5f), RoundedCornerShape(8.dp)),
            contentAlignment = Alignment.Center,
        ) {
            Icon(Icons.Rounded.Add, contentDescription = null, tint = Color.White, modifier = Modifier.size(18.dp))
        }
        Text(
            text = stringResource(R.string.other_platforms),
            color = Color.White,
            style = MaterialTheme.typography.labelSmall,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.padding(horizontal = 4.dp),
        )
    }
}

/**
 * The middle of the screen until the drone's picture comes: the address to type into DJI Fly, or
 * what keeps DJI Fly from connecting (the receiver is off, the phone is not on Wi-Fi).
 */
@Composable
private fun ConnectPanel(
    phase: BridgePhase,
    snapshot: RelaySnapshot,
    testing: Boolean,
    /** How far the test video's conversion is, in percent, while it runs. */
    converting: Int?,
    lan: LanAddress?,
    onStartReceiver: (restart: Boolean) -> Unit,
    onTestVideo: () -> Unit,
    onCopyAddress: (String) -> Unit,
    onOpenWifiSettings: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Column(
        modifier = modifier
            .widthIn(max = 420.dp)
            .glass(PANEL_SHAPE)
            .padding(14.dp),
        verticalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        when {
            phase == BridgePhase.STARTING -> PanelWaiting(stringResource(R.string.receiver_starting))
            phase == BridgePhase.IDLE || phase == BridgePhase.START_FAILED || phase == BridgePhase.RECEIVER_ERROR -> {
                val failed = phase != BridgePhase.IDLE
                PanelText(
                    text = if (failed) snapshot.error?.asString().orEmpty() else stringResource(R.string.receiver_off),
                    color = if (failed) DangerColor else Color.White.copy(alpha = 0.85f),
                )
                PanelButton(
                    text = stringResource(if (failed) R.string.retry else R.string.open_receiver),
                    icon = Icons.Rounded.Refresh,
                    onClick = { onStartReceiver(phase == BridgePhase.RECEIVER_ERROR) },
                )
            }
            phase == BridgePhase.DRONE_CONNECTED || phase == BridgePhase.PREVIEW ->
                PanelWaiting(stringResource(R.string.preview_connected))
            testing && converting != null -> PanelConverting(converting)
            testing -> PanelWaiting(stringResource(R.string.test_video_opening))
            lan == null -> {
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    Icon(Icons.Rounded.WifiOff, contentDescription = null, tint = WarningColor, modifier = Modifier.size(18.dp))
                    PanelText(stringResource(R.string.no_network), modifier = Modifier.weight(1f))
                }
                PanelButton(text = stringResource(R.string.wifi_settings), icon = Icons.Rounded.Wifi, onClick = onOpenWifiSettings)
            }
            else -> {
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    Icon(painterResource(R.drawable.ic_drone), contentDescription = null, tint = Color.White, modifier = Modifier.size(20.dp))
                    Text(
                        text = stringResource(R.string.connect_drone),
                        color = Color.White,
                        style = MaterialTheme.typography.titleSmall,
                        fontWeight = FontWeight.SemiBold,
                    )
                }
                PanelText(stringResource(R.string.dji_fly_instructions))
                AddressRow(address = lan.publishUrl, onCopy = { onCopyAddress(lan.publishUrl) })
                Text(
                    text = stringResource(R.string.dji_fly_path),
                    color = Color.White.copy(alpha = 0.6f),
                    style = MaterialTheme.typography.labelSmall,
                )
                NotConnecting()
                PanelButton(text = stringResource(R.string.try_test_video), icon = Icons.Rounded.Movie, onClick = onTestVideo)
            }
        }
    }
}

@Composable
private fun PanelText(text: String, modifier: Modifier = Modifier, color: Color = Color.White.copy(alpha = 0.85f)) {
    Text(text = text, color = color, style = MaterialTheme.typography.bodySmall, modifier = modifier)
}

@Composable
private fun PanelWaiting(text: String) {
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
        CircularProgressIndicator(modifier = Modifier.size(16.dp), color = Color.White, strokeWidth = 2.dp)
        PanelText(text)
    }
}

/** A heavy test video being turned into what DJI Fly sends, with how far it got. */
@Composable
private fun PanelConverting(percent: Int) {
    Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
        PanelWaiting(stringResource(R.string.test_video_converting))
        LinearProgressIndicator(
            progress = { percent / 100f },
            modifier = Modifier.fillMaxWidth(),
            color = GoLiveStart,
            trackColor = Color.White.copy(alpha = 0.15f),
        )
    }
}

/** A quiet orange text button, the only kind of button inside the connect card. */
@Composable
private fun PanelButton(text: String, icon: ImageVector, onClick: () -> Unit) {
    Row(
        modifier = Modifier
            .clip(RoundedCornerShape(8.dp))
            .clickable(role = Role.Button, onClick = onClick)
            .heightIn(min = 36.dp)
            .padding(horizontal = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Icon(icon, contentDescription = null, tint = GoLiveStart, modifier = Modifier.size(16.dp))
        Text(text = text, color = GoLiveStart, style = MaterialTheme.typography.labelLarge)
    }
}

/**
 * The RTMP address typed into DJI Fly on the remote controller, on one line on any phone and left
 * to right in every language; copying only helps when DJI Fly runs on this phone.
 */
@Composable
private fun AddressRow(address: String, onCopy: () -> Unit) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(10.dp))
            .background(Color.White.copy(alpha = 0.08f))
            .border(1.dp, Color.White.copy(alpha = 0.12f), RoundedCornerShape(10.dp))
            .padding(start = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        SelectionContainer(modifier = Modifier.weight(1f)) {
            BasicText(
                text = address,
                style = TextStyle(
                    color = Color.White,
                    fontFamily = FontFamily.Monospace,
                    fontWeight = FontWeight.SemiBold,
                    fontSize = 15.sp,
                    textDirection = TextDirection.Ltr,
                ),
                maxLines = 1,
                autoSize = TextAutoSize.StepBased(minFontSize = 9.sp, maxFontSize = 15.sp),
            )
        }
        IconButton(onClick = onCopy) {
            Icon(
                Icons.Rounded.ContentCopy,
                contentDescription = stringResource(R.string.copy_address),
                tint = GoLiveStart,
                modifier = Modifier.size(18.dp),
            )
        }
    }
}

/** The usual reasons DJI Fly cannot reach the phone, folded away until asked for. */
@Composable
private fun NotConnecting() {
    var open by rememberSaveable { mutableStateOf(false) }
    val rotation by animateFloatAsState(if (open) 180f else 0f, label = "notConnecting")
    val expandLabel = stringResource(if (open) R.string.collapse else R.string.expand)
    val state = stringResource(if (open) R.string.state_expanded else R.string.state_collapsed)
    Column {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .clip(RoundedCornerShape(8.dp))
                .clickable(onClickLabel = expandLabel, role = Role.Button) { open = !open }
                .semantics { stateDescription = state }
                .heightIn(min = 32.dp)
                .padding(horizontal = 4.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(
                modifier = Modifier.weight(1f),
                text = stringResource(R.string.not_connecting),
                color = Color.White.copy(alpha = 0.85f),
                style = MaterialTheme.typography.labelLarge,
            )
            Icon(
                Icons.Rounded.ExpandMore,
                contentDescription = null,
                tint = Color.White.copy(alpha = 0.7f),
                modifier = Modifier
                    .size(18.dp)
                    .rotate(rotation),
            )
        }
        AnimatedVisibility(visible = open) {
            Column(
                modifier = Modifier.padding(start = 4.dp, top = 4.dp),
                verticalArrangement = Arrangement.spacedBy(6.dp),
            ) {
                listOf(
                    R.string.not_connecting_same_network,
                    R.string.not_connecting_exact_address,
                    R.string.not_connecting_guest_vpn,
                ).forEach { tip ->
                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        Box(
                            modifier = Modifier
                                .padding(top = 6.dp)
                                .size(4.dp)
                                .background(Color.White.copy(alpha = 0.5f), CircleShape),
                        )
                        PanelText(stringResource(tip), color = Color.White.copy(alpha = 0.7f))
                    }
                }
            }
        }
    }
}

/**
 * Where the stream goes, in the style of the screen: every platform with its switch, then the saved
 * ones with their stream keys. Instagram and TikTok need a new key for every broadcast.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun PlatformsSheet(
    destinations: DestinationProfiles,
    liveDestinations: List<DestinationProfile>,
    live: Boolean,
    snapshot: RelaySnapshot,
    onToggle: (DestinationKind) -> Unit,
    onEdit: (DestinationKind) -> Unit,
    onEditProfile: (DestinationProfile) -> Unit,
) {
    val saved = destinations.profiles.mapTo(mutableSetOf()) { it.kind }
    val on = if (live) {
        liveDestinations.mapTo(mutableSetOf()) { it.kind }
    } else {
        destinations.selectedProfiles.mapTo(mutableSetOf()) { it.kind }
    }
    val colors = BridgeTheme.colors
    Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
        Text(
            text = stringResource(R.string.where_to_stream),
            color = Color.White,
            style = MaterialTheme.typography.titleSmall,
            fontWeight = FontWeight.SemiBold,
        )
        Text(
            text = stringResource(R.string.pick_platforms_hint),
            color = Color.White.copy(alpha = 0.6f),
            style = MaterialTheme.typography.bodySmall,
        )
    }
    BoxWithConstraints(modifier = Modifier.fillMaxWidth()) {
        val tileWidth = (maxWidth - TILE_SPACING * (VISIBLE_TILES - 1)) / VISIBLE_TILES
        FlowRow(
            horizontalArrangement = Arrangement.spacedBy(TILE_SPACING),
            verticalArrangement = Arrangement.spacedBy(TILE_SPACING),
        ) {
            DestinationKind.entries.forEach { kind ->
                val liveProfile = liveDestinations.firstOrNull { it.kind == kind }
                PlatformSwitch(
                    kind = kind,
                    checked = kind in on,
                    saved = kind in saved,
                    state = liveProfile?.let { outputLabel(snapshot.output(it.id)?.status, colors).second },
                    onToggle = { onToggle(kind) },
                    onEdit = { onEdit(kind) },
                    modifier = Modifier.width(tileWidth),
                )
            }
        }
    }
    if (destinations.profiles.isNotEmpty()) {
        HorizontalDivider(color = Color.White.copy(alpha = 0.1f))
        destinations.profiles.forEach { profile ->
            SavedPlatform(profile = profile, onEdit = { onEditProfile(profile) })
        }
    }
    val selected = destinations.selectedProfiles.size
    if (!live && selected > 1) {
        Text(
            text = if (snapshot.bitrateKbps > 0) {
                pluralStringResource(R.plurals.upload_note_total, selected, selected, formatBitrate(snapshot.bitrateKbps * selected))
            } else {
                pluralStringResource(R.plurals.upload_note, selected, selected)
            },
            color = Color.White.copy(alpha = 0.6f),
            style = MaterialTheme.typography.bodySmall,
        )
    }
}

/** A saved platform with the state of its stream key and the way to change it. */
@Composable
private fun SavedPlatform(profile: DestinationProfile, onEdit: () -> Unit) {
    val renewKey = profile.kind.keyChangesEachStream
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
        PlatformTile(kind = profile.kind, size = 28.dp)
        Column(modifier = Modifier.weight(1f)) {
            Text(
                text = profile.kind.displayName(),
                color = Color.White,
                style = MaterialTheme.typography.labelLarge,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            Text(
                text = stringResource(if (renewKey) R.string.key_changes_each_stream else R.string.key_saved),
                color = if (renewKey) WarningColor else Color.White.copy(alpha = 0.6f),
                style = MaterialTheme.typography.labelSmall,
            )
        }
        PanelButton(
            text = stringResource(if (renewKey) R.string.update_key else R.string.edit),
            icon = Icons.Rounded.Key,
            onClick = onEdit,
        )
    }
}

/** "Preview", the big button, and "More", like a camera app's shutter row. */
@Composable
private fun ActionRow(
    live: Boolean,
    canGoLive: Boolean,
    showPicture: Boolean,
    testing: Boolean,
    snackbarHostState: SnackbarHostState,
    onPictureOnly: () -> Unit,
    onGoLive: () -> Unit,
    onEndLive: () -> Unit,
    onPlatforms: () -> Unit,
    onDetails: () -> Unit,
    onStopTestVideo: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Row(
        modifier = modifier,
        verticalAlignment = Alignment.Top,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        SquareAction(
            icon = Icons.Rounded.Image,
            label = stringResource(R.string.preview_badge),
            onClick = onPictureOnly,
            enabled = showPicture,
        )
        if (live) {
            BigButton(
                text = stringResource(R.string.end_broadcast),
                icon = Icons.Rounded.Stop,
                brush = Brush.horizontalGradient(listOf(Color(0xFFEF4444), Color(0xFFDC2626))),
                onClick = onEndLive,
                modifier = Modifier.weight(1f),
            )
        } else {
            val scope = rememberCoroutineScope()
            val needsPicture = stringResource(R.string.go_live_needs_picture)
            BigButton(
                text = stringResource(R.string.go_live),
                icon = Icons.Rounded.Sensors,
                brush = Brush.horizontalGradient(listOf(GoLiveStart, GoLiveEnd)),
                enabled = canGoLive,
                onClick = onGoLive,
                onDisabledClick = {
                    scope.launch {
                        snackbarHostState.currentSnackbarData?.dismiss()
                        snackbarHostState.showSnackbar(needsPicture)
                    }
                },
                modifier = Modifier.weight(1f),
            )
        }
        MenuButton(
            items = buildList {
                if (!live) add(MenuEntry(R.string.platforms, Icons.Rounded.GridView, onPlatforms))
                add(MenuEntry(R.string.technical_details, Icons.Rounded.Info, onDetails))
                if (testing) add(MenuEntry(R.string.stop_test_video, Icons.Rounded.Stop, onStopTestVideo))
            },
        ) { open -> SquareAction(icon = Icons.Rounded.MoreHoriz, label = stringResource(R.string.more), onClick = open) }
    }
}

@Composable
private fun SquareAction(icon: ImageVector, label: String, onClick: () -> Unit, enabled: Boolean = true) {
    // The label may be wider than the square ("Daha fazla"); the column grows a little for it.
    Column(
        modifier = Modifier
            .widthIn(min = SQUARE_ACTION_SIZE, max = SQUARE_ACTION_MAX_WIDTH)
            .alpha(if (enabled) 1f else 0.4f)
            .clip(CARD_SHAPE)
            .clickable(enabled = enabled, role = Role.Button, onClick = onClick),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(3.dp),
    ) {
        Box(
            modifier = Modifier
                .size(SQUARE_ACTION_SIZE)
                .glass(CARD_SHAPE),
            contentAlignment = Alignment.Center,
        ) {
            Icon(icon, contentDescription = null, tint = Color.White, modifier = Modifier.size(ICON))
        }
        Text(
            text = label,
            color = Color.White,
            style = MaterialTheme.typography.labelSmall,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

/** The broadcast button: orange to go live, red to end it. Its text shrinks to fit any language. */
@Composable
private fun BigButton(
    text: String,
    icon: ImageVector,
    brush: Brush,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    /** Says why nothing happens, rather than ignoring the tap. */
    onDisabledClick: () -> Unit = {},
) {
    Row(
        modifier = modifier
            .height(SQUARE_ACTION_SIZE)
            .alpha(if (enabled) 1f else 0.5f)
            .clip(CircleShape)
            .background(brush)
            .clickable(role = Role.Button, onClick = if (enabled) onClick else onDisabledClick)
            .padding(horizontal = 16.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.Center,
    ) {
        Icon(icon, contentDescription = null, tint = Color.White, modifier = Modifier.size(ICON))
        Spacer(Modifier.width(8.dp))
        BasicText(
            text = text,
            style = TextStyle(color = Color.White, fontSize = 16.sp, fontWeight = FontWeight.SemiBold),
            maxLines = 1,
            autoSize = TextAutoSize.StepBased(minFontSize = 11.sp, maxFontSize = 16.sp),
        )
    }
}

/** The one line shown while only the picture is on screen. */
@Composable
private fun ControlsHint(modifier: Modifier = Modifier) {
    var visible by remember { mutableStateOf(true) }
    LaunchedEffect(Unit) {
        delay(CONTROLS_HINT_MS)
        visible = false
    }
    AnimatedVisibility(
        visible = visible,
        enter = fadeIn(),
        exit = fadeOut(),
        modifier = modifier
            .windowInsetsPadding(WindowInsets.safeDrawing)
            .padding(bottom = 24.dp),
    ) {
        Text(
            text = stringResource(R.string.controls_hint),
            color = Color.White,
            style = MaterialTheme.typography.labelMedium,
            modifier = Modifier
                .glass(CircleShape)
                .padding(horizontal = 12.dp, vertical = 6.dp),
        )
    }
}

private class MenuEntry(@StringRes val text: Int, val icon: ImageVector, val action: () -> Unit)

/** A button that opens a short menu; [button] gets the function that opens it. */
@Composable
private fun MenuButton(items: List<MenuEntry>, button: @Composable (open: () -> Unit) -> Unit) {
    var open by remember { mutableStateOf(false) }
    Box {
        button { open = true }
        DropdownMenu(expanded = open, onDismissRequest = { open = false }) {
            items.forEach { item ->
                DropdownMenuItem(
                    text = { Text(stringResource(item.text)) },
                    leadingIcon = { Icon(item.icon, contentDescription = null) },
                    onClick = {
                        open = false
                        item.action()
                    },
                )
            }
        }
    }
}

@Composable
private fun GlassIconButton(icon: ImageVector, description: String, onClick: () -> Unit) {
    IconButton(
        onClick = onClick,
        modifier = Modifier
            .size(SIDE_BUTTON)
            .border(BorderStroke(1.dp, Color.White.copy(alpha = 0.15f)), CircleShape),
        colors = IconButtonDefaults.iconButtonColors(containerColor = OverlayGlass, contentColor = Color.White),
    ) {
        Icon(icon, contentDescription = description, modifier = Modifier.size(20.dp))
    }
}

/** The see-through dark surface with a faint edge that everything over the picture uses. */
private fun Modifier.glass(shape: Shape): Modifier =
    clip(shape)
        .background(OverlayGlass)
        .border(1.dp, Color.White.copy(alpha = 0.15f), shape)

/** A dark sheet like the screen under it, scrolling when its content is long. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun BridgeSheet(onDismiss: () -> Unit, content: @Composable () -> Unit) {
    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true),
        containerColor = SheetColor,
        contentColor = Color.White,
    ) {
        // The cards inside use the app's dark theme, whatever theme the user picked.
        DjiLiveBridgeTheme(ThemeChoice.DARK) {
            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .verticalScroll(rememberScrollState())
                    .padding(start = EDGE, end = EDGE, bottom = 24.dp),
                verticalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                content()
            }
        }
    }
}

@Composable
private fun networkName(kind: LanKind): String = stringResource(
    when (kind) {
        LanKind.WIFI -> R.string.net_wifi
        LanKind.HOTSPOT -> R.string.net_hotspot
        LanKind.WIRED, LanKind.OTHER -> R.string.net_local
    },
)

/** The phone is a monitor while the picture shows; it should not go dark mid-flight. */
@Composable
private fun KeepScreenOn() {
    val view = LocalView.current
    DisposableEffect(view) {
        view.keepScreenOn = true
        onDispose { view.keepScreenOn = false }
    }
}

private val MAIN_PLATFORMS = setOf(
    DestinationKind.INSTAGRAM,
    DestinationKind.TIKTOK,
    DestinationKind.YOUTUBE,
    DestinationKind.FACEBOOK,
)

/** Behind the connect card until the picture comes: the brand's blue, nearly black. */
private val WaitingBackground = Brush.verticalGradient(listOf(Color(0xFF101A2A), Color(0xFF05070B)))
private val SheetColor = Color(0xFF14171C)
private val ReadyColor = Color(0xFF4ADE80)
private val WarningColor = Color(0xFFFBBF24)
private val DangerColor = Color(0xFFF87171)
private val GoLiveStart = Color(0xFFFF8A3D)
private val GoLiveEnd = Color(0xFFEA580C)

// Sized for a 360 dp wide phone at a larger font size: four platform tiles fit in a row.
private val EDGE = 12.dp
private val GAP = 8.dp
private val ICON = 22.dp
private val CARD_HEIGHT = 52.dp
private val CARD_SHAPE = RoundedCornerShape(14.dp)
private val PANEL_SHAPE = RoundedCornerShape(18.dp)
private val SIDE_BUTTON = 40.dp
private val TILE_SHAPE = RoundedCornerShape(12.dp)
private val TILE_ICON = 30.dp
private val MINI_SWITCH_TRAVEL = 14.dp
private const val VISIBLE_TILES = 4
private val TILE_SPACING = 6.dp
private val MIN_TILE_WIDTH = 62.dp
private val SQUARE_ACTION_SIZE = 52.dp
private val SQUARE_ACTION_MAX_WIDTH = 76.dp
private val LANDSCAPE_ACTIONS_WIDTH = 320.dp
private const val CONTROLS_HINT_MS = 2_500L
private const val DIM_DELAY_MS = 1_000L
private const val DIM_ALPHA = 0.55f

/** Pauses and reconnects of the drone's stream within a minute before the screen says why. */
private const val WEAK_LINK_INTERRUPTIONS = 3
