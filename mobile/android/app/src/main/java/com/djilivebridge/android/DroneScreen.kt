package com.djilivebridge.android

import android.os.Build
import androidx.annotation.StringRes
import androidx.compose.animation.AnimatedVisibility
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
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.rounded.HelpOutline
import androidx.compose.material.icons.automirrored.rounded.KeyboardArrowRight
import androidx.compose.material.icons.rounded.Add
import androidx.compose.material.icons.rounded.CheckCircle
import androidx.compose.material.icons.rounded.Close
import androidx.compose.material.icons.rounded.ErrorOutline
import androidx.compose.material.icons.rounded.Fullscreen
import androidx.compose.material.icons.rounded.FullscreenExit
import androidx.compose.material.icons.rounded.GridView
import androidx.compose.material.icons.rounded.Hd
import androidx.compose.material.icons.rounded.Image
import androidx.compose.material.icons.rounded.Info
import androidx.compose.material.icons.rounded.Lan
import androidx.compose.material.icons.rounded.Language
import androidx.compose.material.icons.rounded.Lightbulb
import androidx.compose.material.icons.rounded.MoreHoriz
import androidx.compose.material.icons.rounded.Movie
import androidx.compose.material.icons.rounded.Palette
import androidx.compose.material.icons.rounded.Sensors
import androidx.compose.material.icons.rounded.Settings
import androidx.compose.material.icons.rounded.SignalCellularAlt
import androidx.compose.material.icons.rounded.Stop
import androidx.compose.material.icons.rounded.WarningAmber
import androidx.compose.material.icons.rounded.Wifi
import androidx.compose.material.icons.rounded.WifiTethering
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.IconButtonDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Switch
import androidx.compose.material3.SwitchDefaults
import androidx.compose.material3.Text
import androidx.compose.material3.rememberModalBottomSheetState
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
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.res.painterResource
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
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextDirection
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlinx.coroutines.delay

/** Which sheet is open over the picture. */
private enum class DroneSheet { PLATFORMS, DETAILS }

/**
 * The drone's picture over the whole screen once it arrives, with the controls around it like a
 * camera app: what is connected and the stream's state at the top, the picture's numbers under
 * them, a switch per platform and the one big button at the bottom. "Preview" hides everything
 * but the picture; a double tap switches between the whole picture and a screen-filling one.
 */
@Composable
internal fun DroneScreen(
    phase: BridgePhase,
    serviceState: RelayServiceUiState,
    destinations: DestinationProfiles,
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
    onStopTestVideo: () -> Unit,
    onShowLanguagePicker: () -> Unit,
    onShowThemePicker: () -> Unit,
    onShowGuide: () -> Unit,
) {
    val snapshot = serviceState.snapshot
    val liveDestinations = serviceState.liveProfileIds.mapNotNull { id -> destinations.profiles.firstOrNull { it.id == id } }
    val pictureState = rememberDronePictureState()
    var sheet by rememberSaveable { mutableStateOf<DroneSheet?>(null) }
    var pictureOnly by rememberSaveable { mutableStateOf(false) }
    val otherFit = if (pictureState.shownFit == PictureFit.WHOLE) PictureFit.FILL else PictureFit.WHOLE
    KeepScreenOn()

    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(Color.Black),
    ) {
        DronePicture(state = pictureState, fit = pictureFit, modifier = Modifier.fillMaxSize())
        Box(
            modifier = Modifier
                .fillMaxSize()
                // A picture that stopped coming must not look live.
                .background(if (phase.hasPicture) Color.Transparent else Color.Black.copy(alpha = 0.55f))
                .pointerInput(otherFit, pictureOnly) {
                    detectTapGestures(
                        onDoubleTap = { onPictureFitChange(otherFit) },
                        onTap = { if (pictureOnly) pictureOnly = false },
                    )
                },
        )
        AnimatedVisibility(visible = !pictureOnly, enter = fadeIn(), exit = fadeOut()) {
            Box(modifier = Modifier.fillMaxSize()) {
                Scrims()
                Controls(
                    phase = phase,
                    serviceState = serviceState,
                    destinations = destinations,
                    liveDestinations = liveDestinations,
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
                    onStopTestVideo = onStopTestVideo,
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
            PlatformPicker(
                destinations = destinations,
                bitrateKbps = snapshot.bitrateKbps,
                onPlatformClick = onPlatformToggle,
                onPlatformLongClick = onPlatformEdit,
                onEditProfile = onEditProfile,
            )
        }
        DroneSheet.DETAILS -> BridgeSheet(onDismiss = { sheet = null }) {
            LiveDetails(
                phase = phase,
                snapshot = snapshot,
                destinations = liveDestinations,
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
    destinations: DestinationProfiles,
    liveDestinations: List<DestinationProfile>,
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
    onStopTestVideo: () -> Unit,
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
            videoSize = pictureState.videoSize?.let { "${it.width}×${it.height}" },
            bitrateKbps = snapshot.bitrateKbps,
            lan = lan,
        )
    }
    val sideButtons: @Composable () -> Unit = {
        Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
            val whole = pictureState.shownFit == PictureFit.WHOLE
            GlassIconButton(
                icon = if (whole) Icons.Rounded.Fullscreen else Icons.Rounded.FullscreenExit,
                description = stringResource(if (whole) R.string.picture_fill else R.string.picture_fit),
                onClick = onFitToggle,
            )
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
        SnackbarHost(snackbarHostState)
        Message(phase = phase, serviceState = serviceState, liveDestinations = liveDestinations)
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
            testing = testing,
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
            .padding(16.dp),
    ) {
        if (maxWidth > maxHeight) {
            // Sideways: the state along the top, the platforms and the buttons along the bottom.
            Row(modifier = Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                Column(modifier = Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    deviceCard(Modifier.widthIn(max = 340.dp))
                    chips()
                }
                Column(horizontalAlignment = Alignment.End, verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    statusCard(Modifier.widthIn(max = 280.dp))
                    sideButtons()
                }
            }
            Row(
                modifier = Modifier
                    .align(Alignment.BottomStart)
                    .fillMaxWidth(),
                verticalAlignment = Alignment.Bottom,
                horizontalArrangement = Arrangement.spacedBy(16.dp),
            ) {
                Column(
                    modifier = Modifier
                        .weight(1f)
                        .widthIn(max = 520.dp),
                    verticalArrangement = Arrangement.spacedBy(12.dp),
                    content = bottom,
                )
                actions(Modifier.width(LANDSCAPE_ACTIONS_WIDTH))
            }
        } else {
            Column(modifier = Modifier.fillMaxSize(), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                Row(horizontalArrangement = Arrangement.spacedBy(12.dp), verticalAlignment = Alignment.Top) {
                    deviceCard(Modifier.weight(1.15f))
                    statusCard(Modifier.weight(1f))
                }
                chips()
                Box(
                    modifier = Modifier
                        .weight(1f)
                        .fillMaxWidth(),
                ) {
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
                .height(220.dp)
                .background(Brush.verticalGradient(listOf(Color.Black.copy(alpha = 0.55f), Color.Transparent))),
        )
        Box(
            modifier = Modifier
                .align(Alignment.BottomCenter)
                .fillMaxWidth()
                .height(360.dp)
                .background(Brush.verticalGradient(listOf(Color.Transparent, Color.Black.copy(alpha = 0.7f)))),
        )
    }
}

/** What sends the picture, and whether it is connected; opens the connection details. */
@Composable
private fun DeviceCard(phase: BridgePhase, testing: Boolean, onClick: () -> Unit, modifier: Modifier = Modifier) {
    val (dot, state) = when {
        phase == BridgePhase.RECEIVER_ERROR -> DangerColor to R.string.hero_receiver_stopped
        phase.hasPicture || phase == BridgePhase.DRONE_CONNECTED -> ReadyColor to R.string.device_connected
        else -> WarningColor to R.string.device_waiting
    }
    Row(
        modifier = modifier
            .glass(RoundedCornerShape(20.dp))
            .clickable(onClickLabel = stringResource(R.string.show_details), role = Role.Button, onClick = onClick)
            .heightIn(min = 72.dp)
            .padding(horizontal = 14.dp, vertical = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        if (testing) {
            Icon(Icons.Rounded.Movie, contentDescription = null, tint = Color.White, modifier = Modifier.size(34.dp))
        } else {
            Icon(painterResource(R.drawable.ic_drone), contentDescription = null, tint = Color.White, modifier = Modifier.size(34.dp))
        }
        Column(modifier = Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(
                text = stringResource(if (testing) R.string.device_test_video else R.string.device_drone),
                color = Color.White,
                style = MaterialTheme.typography.titleMedium,
                fontWeight = FontWeight.SemiBold,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                Box(
                    modifier = Modifier
                        .size(8.dp)
                        .background(dot, CircleShape),
                )
                Text(
                    text = stringResource(state),
                    color = Color.White.copy(alpha = 0.8f),
                    style = MaterialTheme.typography.bodyMedium,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
        Icon(
            imageVector = Icons.AutoMirrored.Rounded.KeyboardArrowRight,
            contentDescription = null,
            tint = Color.White.copy(alpha = 0.7f),
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
        return if (phase.hasPicture) {
            StatusLook(Tone.READY, Leading.CHECK, stringResource(R.string.status_ready), text(stringResource(R.string.status_not_live)))
        } else {
            StatusLook(Tone.BUSY, Leading.PROGRESS, null, text(stringResource(R.string.status_waiting_for_picture)))
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
                            LiveTimer(since, color = Color.White, style = MaterialTheme.typography.bodySmall)
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
            .clip(RoundedCornerShape(20.dp))
            .background(look.tone.tint.copy(alpha = 0.78f))
            .border(1.5.dp, look.tone.accent.copy(alpha = 0.75f), RoundedCornerShape(20.dp))
            .heightIn(min = 72.dp)
            .padding(horizontal = 14.dp, vertical = 10.dp)
            .semantics(mergeDescendants = true) { liveRegion = LiveRegionMode.Polite },
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        when (look.leading) {
            Leading.CHECK -> Icon(Icons.Rounded.CheckCircle, contentDescription = null, tint = look.tone.accent, modifier = Modifier.size(36.dp))
            Leading.LIVE_DOT -> PulsingDot(color = look.tone.accent, size = 14.dp)
            Leading.PROGRESS -> CircularProgressIndicator(modifier = Modifier.size(24.dp), color = Color.White, strokeWidth = 2.5.dp)
            Leading.WARNING -> Icon(Icons.Rounded.WarningAmber, contentDescription = null, tint = look.tone.accent, modifier = Modifier.size(30.dp))
            Leading.ERROR -> Icon(Icons.Rounded.ErrorOutline, contentDescription = null, tint = look.tone.accent, modifier = Modifier.size(30.dp))
        }
        Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
            look.headline?.let {
                // One line that shrinks to fit: "RECONNECTING" is long in most languages.
                BasicText(
                    text = it.uppercase(locale),
                    style = MaterialTheme.typography.titleLarge.copy(color = look.tone.accent, fontWeight = FontWeight.Bold),
                    maxLines = 1,
                    autoSize = TextAutoSize.StepBased(minFontSize = 11.sp, maxFontSize = 22.sp),
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
        style = MaterialTheme.typography.bodySmall,
        maxLines = 2,
        overflow = TextOverflow.Ellipsis,
    )
}

/** The picture's size, the incoming bitrate and how the phone is connected. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun InfoChips(videoSize: String?, bitrateKbps: Double, lan: LanAddress?) {
    FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
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
            .padding(horizontal = 14.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Icon(icon, contentDescription = null, tint = Color.White, modifier = Modifier.size(18.dp))
        Text(
            text = text,
            color = Color.White,
            style = MaterialTheme.typography.labelLarge.let { if (numbers) it.copy(textDirection = TextDirection.Ltr) else it },
            maxLines = 1,
        )
    }
}

/**
 * The one message worth reading now: why going live failed, why the drone's picture is gone, or
 * the tip for the platform (such as pressing "Go live" in Instagram too).
 */
@Composable
private fun Message(phase: BridgePhase, serviceState: RelayServiceUiState, liveDestinations: List<DestinationProfile>) {
    val snapshot = serviceState.snapshot
    val notice = serviceState.notice
    val holding = serviceState.isLive && snapshot.outputStatus == "holding" &&
        (phase == BridgePhase.WAITING_FOR_DRONE || phase == BridgePhase.DRONE_CONNECTED)
    when {
        notice != null -> Banner(
            key = notice,
            title = notice.title.asString(),
            message = notice.message.asString(),
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
        phase == BridgePhase.RECEIVER_ERROR -> Banner(
            key = "receiver",
            title = stringResource(R.string.hero_receiver_stopped),
            message = snapshot.error?.asString(),
            icon = Icons.Rounded.ErrorOutline,
            accent = DangerColor,
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
            .glass(RoundedCornerShape(20.dp))
            .padding(start = 16.dp, top = 12.dp, bottom = 12.dp, end = 4.dp)
            .semantics(mergeDescendants = true) { liveRegion = LiveRegionMode.Polite },
        verticalAlignment = Alignment.Top,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Icon(
            icon,
            contentDescription = null,
            tint = accent,
            modifier = Modifier
                .padding(top = 2.dp)
                .size(20.dp),
        )
        Column(modifier = Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            title?.let { Text(text = it, color = Color.White, style = MaterialTheme.typography.titleSmall) }
            message?.let { Text(text = it, color = Color.White.copy(alpha = 0.85f), style = MaterialTheme.typography.bodySmall) }
        }
        IconButton(onClick = { closed = true }) {
            Icon(Icons.Rounded.Close, contentDescription = stringResource(R.string.close), tint = Color.White.copy(alpha = 0.8f))
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
            .glass(RoundedCornerShape(24.dp))
            .padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(
                modifier = Modifier.weight(1f),
                text = stringResource(R.string.platforms),
                color = Color.White,
                style = MaterialTheme.typography.titleMedium,
                fontWeight = FontWeight.SemiBold,
            )
            Text(
                text = stringResource(R.string.platforms_active, shown.count { it in on }, shown.size),
                color = Color.White.copy(alpha = 0.7f),
                style = MaterialTheme.typography.bodyMedium,
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
            .clip(RoundedCornerShape(18.dp))
            .background(Color.White.copy(alpha = if (checked) 0.12f else 0.05f))
            .border(1.dp, if (checked) GoLiveStart.copy(alpha = 0.8f) else Color.White.copy(alpha = 0.12f), RoundedCornerShape(18.dp))
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
            .padding(vertical = 12.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Box {
            PlatformTile(kind = kind, size = 40.dp)
            state?.let {
                Box(
                    modifier = Modifier
                        .align(Alignment.BottomEnd)
                        .offset(x = 4.dp, y = 4.dp)
                        .size(12.dp)
                        .background(it, CircleShape)
                        .border(2.dp, Color.Black, CircleShape),
                )
            }
        }
        Text(
            text = name,
            color = Color.White.copy(alpha = if (saved) 1f else 0.7f),
            style = MaterialTheme.typography.labelMedium,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.padding(horizontal = 6.dp),
        )
        Switch(
            checked = checked,
            onCheckedChange = null,
            colors = SwitchDefaults.colors(
                checkedThumbColor = Color.White,
                checkedTrackColor = GoLiveStart,
                checkedBorderColor = GoLiveStart,
                uncheckedThumbColor = Color.White.copy(alpha = 0.85f),
                uncheckedTrackColor = Color.White.copy(alpha = 0.18f),
                uncheckedBorderColor = Color.White.copy(alpha = 0.3f),
            ),
        )
    }
}

/** Twitch, Kick and custom servers, until they have a stream key. */
@Composable
private fun OtherPlatforms(onClick: () -> Unit, modifier: Modifier = Modifier) {
    Column(
        modifier = modifier
            .clip(RoundedCornerShape(18.dp))
            .border(1.dp, Color.White.copy(alpha = 0.12f), RoundedCornerShape(18.dp))
            .clickable(role = Role.Button, onClick = onClick)
            .padding(vertical = 12.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Box(
            modifier = Modifier
                .size(40.dp)
                .border(1.5.dp, Color.White.copy(alpha = 0.5f), RoundedCornerShape(11.dp)),
            contentAlignment = Alignment.Center,
        ) {
            Icon(Icons.Rounded.Add, contentDescription = null, tint = Color.White)
        }
        Text(
            text = stringResource(R.string.other_platforms),
            color = Color.White,
            style = MaterialTheme.typography.labelMedium,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.padding(horizontal = 6.dp),
        )
    }
}

/** "Preview", the big button, and "More", like a camera app's shutter row. */
@Composable
private fun ActionRow(
    live: Boolean,
    canGoLive: Boolean,
    testing: Boolean,
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
        horizontalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        SquareAction(icon = Icons.Rounded.Image, label = stringResource(R.string.preview_badge), onClick = onPictureOnly)
        if (live) {
            BigButton(
                text = stringResource(R.string.end_broadcast),
                icon = Icons.Rounded.Stop,
                brush = Brush.horizontalGradient(listOf(Color(0xFFEF4444), Color(0xFFDC2626))),
                onClick = onEndLive,
                modifier = Modifier.weight(1f),
            )
        } else {
            BigButton(
                text = stringResource(R.string.go_live),
                icon = Icons.Rounded.Sensors,
                brush = Brush.horizontalGradient(listOf(GoLiveStart, GoLiveEnd)),
                enabled = canGoLive,
                onClick = onGoLive,
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
private fun SquareAction(icon: ImageVector, label: String, onClick: () -> Unit) {
    Column(
        modifier = Modifier
            .width(SQUARE_ACTION_SIZE)
            .clip(RoundedCornerShape(20.dp))
            .clickable(role = Role.Button, onClick = onClick),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Box(
            modifier = Modifier
                .size(SQUARE_ACTION_SIZE)
                .glass(RoundedCornerShape(20.dp)),
            contentAlignment = Alignment.Center,
        ) {
            Icon(icon, contentDescription = null, tint = Color.White, modifier = Modifier.size(28.dp))
        }
        Text(
            text = label,
            color = Color.White,
            style = MaterialTheme.typography.labelMedium,
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
) {
    Row(
        modifier = modifier
            .height(SQUARE_ACTION_SIZE)
            .alpha(if (enabled) 1f else 0.5f)
            .clip(CircleShape)
            .background(brush)
            .clickable(enabled = enabled, role = Role.Button, onClick = onClick)
            .padding(horizontal = 20.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.Center,
    ) {
        Icon(icon, contentDescription = null, tint = Color.White, modifier = Modifier.size(28.dp))
        Spacer(Modifier.width(10.dp))
        BasicText(
            text = text,
            style = TextStyle(color = Color.White, fontSize = 20.sp, fontWeight = FontWeight.SemiBold),
            maxLines = 1,
            autoSize = TextAutoSize.StepBased(minFontSize = 12.sp, maxFontSize = 20.sp),
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
            style = MaterialTheme.typography.labelLarge,
            modifier = Modifier
                .glass(CircleShape)
                .padding(horizontal = 16.dp, vertical = 10.dp),
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
            .size(52.dp)
            .border(BorderStroke(1.dp, Color.White.copy(alpha = 0.15f)), CircleShape),
        colors = IconButtonDefaults.iconButtonColors(containerColor = OverlayGlass, contentColor = Color.White),
    ) {
        Icon(icon, contentDescription = description, modifier = Modifier.size(26.dp))
    }
}

/** The see-through dark surface with a faint edge that everything over the picture uses. */
private fun Modifier.glass(shape: Shape): Modifier =
    clip(shape)
        .background(OverlayGlass)
        .border(1.dp, Color.White.copy(alpha = 0.15f), shape)

/** A sheet in the app's own colors, scrolling when its content is long. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun BridgeSheet(onDismiss: () -> Unit, content: @Composable () -> Unit) {
    val colors = BridgeTheme.colors
    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true),
        containerColor = colors.background,
        contentColor = colors.text,
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .verticalScroll(rememberScrollState())
                .padding(start = 16.dp, end = 16.dp, bottom = 24.dp),
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            content()
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

private val ReadyColor = Color(0xFF4ADE80)
private val WarningColor = Color(0xFFFBBF24)
private val DangerColor = Color(0xFFF87171)
private val GoLiveStart = Color(0xFFFF8A3D)
private val GoLiveEnd = Color(0xFFEA580C)

private const val VISIBLE_TILES = 4
private val TILE_SPACING = 8.dp
private val MIN_TILE_WIDTH = 76.dp
private val SQUARE_ACTION_SIZE = 64.dp
private val LANDSCAPE_ACTIONS_WIDTH = 400.dp
private const val CONTROLS_HINT_MS = 2_500L
