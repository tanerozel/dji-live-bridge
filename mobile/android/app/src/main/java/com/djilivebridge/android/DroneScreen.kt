package com.djilivebridge.android

import android.os.Build
import androidx.annotation.StringRes
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
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
import androidx.compose.material.icons.rounded.Info
import androidx.compose.material.icons.rounded.Language
import androidx.compose.material.icons.rounded.Lightbulb
import androidx.compose.material.icons.rounded.MoreVert
import androidx.compose.material.icons.rounded.Movie
import androidx.compose.material.icons.rounded.Palette
import androidx.compose.material.icons.rounded.Sensors
import androidx.compose.material.icons.rounded.Stop
import androidx.compose.material.icons.rounded.Videocam
import androidx.compose.material.icons.rounded.WarningAmber
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
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
import androidx.compose.material3.Text
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

/** Which sheet is open over the picture. */
private enum class DroneSheet { PLATFORMS, DETAILS }

/**
 * The drone's picture over the whole screen, once it arrives, the way a camera app shows its
 * viewfinder: what the picture is and the stream's state at the top, what is going on and the one
 * next action at the bottom. Platforms and details open in sheets, so nothing else covers the
 * picture. A double tap switches between the whole picture and a screen-filling one.
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
    onGoLive: () -> Unit,
    /** Ends the broadcast on one platform, or on all of them for null. */
    onEndLive: (profileId: String?) -> Unit,
    onPlatformClick: (DestinationKind) -> Unit,
    onPlatformLongClick: (DestinationKind) -> Unit,
    onEditProfile: (DestinationProfile) -> Unit,
    onStopTestVideo: () -> Unit,
    onShowLanguagePicker: () -> Unit,
    onShowThemePicker: () -> Unit,
    onShowGuide: () -> Unit,
) {
    val snapshot = serviceState.snapshot
    val live = serviceState.isLive
    val liveDestinations = serviceState.liveProfileIds.mapNotNull { id -> destinations.profiles.firstOrNull { it.id == id } }
    val pictureState = rememberDronePictureState()
    var sheet by rememberSaveable { mutableStateOf<DroneSheet?>(null) }
    val otherFit = if (pictureFit == PictureFit.WHOLE) PictureFit.FILL else PictureFit.WHOLE
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
                .pointerInput(pictureFit) { detectTapGestures(onDoubleTap = { onPictureFitChange(otherFit) }) },
        )
        Scrims()
        Box(
            modifier = Modifier
                .fillMaxSize()
                .windowInsetsPadding(WindowInsets.safeDrawing)
                .padding(16.dp),
        ) {
            Row(
                modifier = Modifier
                    .align(Alignment.TopStart)
                    .fillMaxWidth(),
                verticalAlignment = Alignment.Top,
                horizontalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                Box(modifier = Modifier.weight(1f)) {
                    SourceChip(
                        phase = phase,
                        live = live,
                        liveSinceElapsedMillis = serviceState.liveSinceElapsedMillis,
                        testing = serviceState.testVideoName != null,
                        onStopTestVideo = onStopTestVideo,
                    )
                }
                Column(
                    horizontalAlignment = Alignment.End,
                    verticalArrangement = Arrangement.spacedBy(12.dp),
                ) {
                    StatusChip(chipStatus(phase, snapshot, live, LiveTarget(liveDestinations)))
                    GlassIconButton(
                        icon = if (pictureFit == PictureFit.WHOLE) Icons.Rounded.Fullscreen else Icons.Rounded.FullscreenExit,
                        description = stringResource(if (pictureFit == PictureFit.WHOLE) R.string.picture_fill else R.string.picture_fit),
                        onClick = { onPictureFitChange(otherFit) },
                    )
                    DroneMenu(
                        live = live,
                        testing = serviceState.testVideoName != null,
                        onPlatforms = { sheet = DroneSheet.PLATFORMS },
                        onDetails = { sheet = DroneSheet.DETAILS },
                        onStopTestVideo = onStopTestVideo,
                        onLanguage = onShowLanguagePicker,
                        onTheme = onShowThemePicker,
                        onHelp = onShowGuide,
                    )
                }
            }
            Column(
                modifier = Modifier
                    .align(Alignment.BottomCenter)
                    .fillMaxWidth(),
                verticalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                SnackbarHost(snackbarHostState)
                serviceState.notice?.let { notice -> NoticeCard(notice) }
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    verticalAlignment = Alignment.Bottom,
                    horizontalArrangement = Arrangement.spacedBy(12.dp),
                ) {
                    Box(modifier = Modifier.weight(1f)) {
                        InfoCard(
                            modifier = Modifier.widthIn(max = INFO_CARD_MAX_WIDTH),
                            phase = phase,
                            serviceState = serviceState,
                            selected = destinations.selectedProfiles,
                            liveDestinations = liveDestinations,
                            videoSizeLabel = pictureState.videoSize?.let { "${it.width}×${it.height}" },
                            lan = lan,
                            onPlatforms = { sheet = if (live) DroneSheet.DETAILS else DroneSheet.PLATFORMS },
                        )
                    }
                    when {
                        live -> ActionPill(
                            text = stringResource(R.string.end_broadcast),
                            icon = Icons.Rounded.Stop,
                            color = BridgeTheme.colors.live,
                            onClick = { onEndLive(null) },
                        )
                        destinations.selectedProfiles.isEmpty() -> ActionPill(
                            text = stringResource(R.string.choose_platform),
                            icon = Icons.Rounded.Add,
                            color = BridgeTheme.colors.action,
                            onClick = { sheet = DroneSheet.PLATFORMS },
                        )
                        // The card shows where it goes, so the button stays short in every language.
                        else -> ActionPill(
                            text = stringResource(R.string.go_live),
                            icon = Icons.Rounded.Sensors,
                            color = BridgeTheme.colors.action,
                            enabled = phase.hasPicture,
                            onClick = onGoLive,
                        )
                    }
                }
            }
        }
    }

    when (sheet) {
        DroneSheet.PLATFORMS -> BridgeSheet(onDismiss = { sheet = null }) {
            PlatformPicker(
                destinations = destinations,
                bitrateKbps = snapshot.bitrateKbps,
                onPlatformClick = onPlatformClick,
                onPlatformLongClick = onPlatformLongClick,
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

/** Darkens the top and bottom edges so the labels over the picture stay readable. */
@Composable
private fun Scrims() {
    Box(modifier = Modifier.fillMaxSize()) {
        Box(
            modifier = Modifier
                .align(Alignment.TopCenter)
                .fillMaxWidth()
                .height(160.dp)
                .background(Brush.verticalGradient(listOf(Color.Black.copy(alpha = 0.5f), Color.Transparent))),
        )
        Box(
            modifier = Modifier
                .align(Alignment.BottomCenter)
                .fillMaxWidth()
                .height(280.dp)
                .background(Brush.verticalGradient(listOf(Color.Transparent, Color.Black.copy(alpha = 0.6f)))),
        )
    }
}

/**
 * What the picture is: a preview before going live, the live badge and running time after. A
 * test video can be closed right here.
 */
@Composable
private fun SourceChip(
    phase: BridgePhase,
    live: Boolean,
    liveSinceElapsedMillis: Long?,
    testing: Boolean,
    onStopTestVideo: () -> Unit,
) {
    when {
        live -> Row(
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            if (phase == BridgePhase.LIVE) LiveBadge()
            // A reconnect keeps the running time, so it stays up while the platform comes back.
            liveSinceElapsedMillis?.let { since ->
                OverlayChip { LiveTimer(since, color = Color.White, style = MaterialTheme.typography.labelLarge) }
            }
        }
        else -> Row(
            modifier = Modifier
                .background(OverlayGlass, RoundedCornerShape(12.dp))
                .heightIn(min = 36.dp)
                .padding(start = 12.dp, end = if (testing) 0.dp else 12.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Icon(
                imageVector = if (testing) Icons.Rounded.Movie else Icons.Rounded.Videocam,
                contentDescription = null,
                tint = Color.White,
                modifier = Modifier.size(20.dp),
            )
            Text(
                modifier = Modifier.weight(1f, fill = false),
                text = stringResource(if (testing) R.string.preview_badge_test else R.string.preview_badge),
                color = Color.White,
                style = MaterialTheme.typography.labelLarge,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            if (testing) {
                IconButton(onClick = onStopTestVideo) {
                    Icon(
                        imageVector = Icons.Rounded.Close,
                        contentDescription = stringResource(R.string.stop_test_video),
                        tint = Color.White,
                        modifier = Modifier.size(20.dp),
                    )
                }
            }
        }
    }
}

private class ChipStatus(
    val icon: ImageVector?,
    val tint: Color,
    val headline: String?,
    val caption: String?,
)

/** The stream's state in a word or two, for the chip in the top corner. */
@Composable
private fun chipStatus(phase: BridgePhase, snapshot: RelaySnapshot, live: Boolean, target: LiveTarget): ChipStatus {
    val ok = Color(0xFF4ADE80)
    val warning = Color(0xFFFBBF24)
    val danger = Color(0xFFF87171)
    if (!live) {
        return if (phase.hasPicture) {
            ChipStatus(Icons.Rounded.CheckCircle, ok, stringResource(R.string.status_ready), stringResource(R.string.status_not_live))
        } else {
            ChipStatus(null, Color.White, null, stringResource(R.string.status_waiting_for_picture))
        }
    }
    return when (phase) {
        BridgePhase.LIVE -> if (snapshot.outputs.any { it.status == "congested" }) {
            ChipStatus(Icons.Rounded.WarningAmber, warning, stringResource(R.string.status_live), stringResource(R.string.status_congested))
        } else {
            ChipStatus(Icons.Rounded.CheckCircle, ok, stringResource(R.string.status_live), formatBitrate(snapshot.bitrateKbps))
        }
        BridgePhase.PREVIEW, BridgePhase.CONNECTING_TARGET ->
            ChipStatus(null, Color.White, stringResource(R.string.status_connecting), target.label())
        BridgePhase.RECONNECTING ->
            ChipStatus(Icons.Rounded.WarningAmber, warning, stringResource(R.string.status_reconnecting), target.label())
        BridgePhase.STARTING, BridgePhase.WAITING_FOR_DRONE, BridgePhase.DRONE_CONNECTED ->
            if (snapshot.outputStatus == "holding") {
                ChipStatus(Icons.Rounded.WarningAmber, warning, null, stringResource(R.string.status_holding))
            } else {
                ChipStatus(null, Color.White, null, stringResource(R.string.status_waiting_for_picture))
            }
        BridgePhase.RECEIVER_ERROR -> ChipStatus(Icons.Rounded.ErrorOutline, danger, stringResource(R.string.hero_receiver_stopped), null)
        BridgePhase.IDLE, BridgePhase.START_FAILED ->
            ChipStatus(Icons.Rounded.ErrorOutline, danger, stringResource(R.string.status_stopped), null)
    }
}

@Composable
private fun StatusChip(status: ChipStatus) {
    val locale = LocalConfiguration.current.locales[0]
    Column(
        modifier = Modifier
            .widthIn(max = STATUS_CHIP_MAX_WIDTH)
            .background(OverlayGlass, RoundedCornerShape(16.dp))
            .padding(horizontal = 14.dp, vertical = 10.dp)
            .semantics(mergeDescendants = true) { liveRegion = LiveRegionMode.Polite },
        horizontalAlignment = Alignment.End,
        verticalArrangement = Arrangement.spacedBy(2.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            if (status.icon != null) {
                Icon(status.icon, contentDescription = null, tint = status.tint, modifier = Modifier.size(18.dp))
            } else {
                CircularProgressIndicator(modifier = Modifier.size(14.dp), color = Color.White, strokeWidth = 2.dp)
            }
            if (status.headline != null) {
                Text(
                    modifier = Modifier.weight(1f, fill = false),
                    text = status.headline.uppercase(locale),
                    color = Color.White,
                    style = MaterialTheme.typography.labelLarge,
                    fontWeight = FontWeight.Bold,
                    letterSpacing = 1.sp,
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis,
                )
            } else {
                status.caption?.let { StatusCaption(it) }
            }
        }
        if (status.headline != null) status.caption?.let { StatusCaption(it) }
    }
}

@Composable
private fun StatusCaption(text: String) {
    Text(
        text = text,
        color = Color.White.copy(alpha = 0.85f),
        style = MaterialTheme.typography.bodySmall,
        maxLines = 2,
        overflow = TextOverflow.Ellipsis,
    )
}

/** What is going on, in a sentence, with the platforms the stream goes (or will go) to. */
@Composable
private fun InfoCard(
    phase: BridgePhase,
    serviceState: RelayServiceUiState,
    selected: List<DestinationProfile>,
    liveDestinations: List<DestinationProfile>,
    videoSizeLabel: String?,
    lan: LanAddress?,
    onPlatforms: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val snapshot = serviceState.snapshot
    val live = serviceState.isLive
    val testVideo = serviceState.testVideoName
    val target = LiveTarget(liveDestinations)
    val colors = BridgeTheme.colors
    val title: String
    val subtitle: String?
    if (!live && !phase.hasPicture) {
        // The picture stopped; the home screen comes back unless it returns within moments.
        title = stringResource(R.string.preview_waiting)
        subtitle = null
    } else if (!live) {
        title = stringResource(if (testVideo != null) R.string.preview_ready_title_test else R.string.preview_ready_title)
        subtitle = if (testVideo != null) {
            testVideo.asString()
        } else {
            listOfNotNull(
                videoSizeLabel,
                snapshot.bitrateKbps.takeIf { it > 0 }?.let { formatBitrate(it) },
                lan?.kind?.let { networkName(it) },
            ).joinToString(" · ").ifEmpty { null }
        }
    } else {
        val holding = snapshot.outputStatus == "holding"
        val waiting = phase == BridgePhase.WAITING_FOR_DRONE || phase == BridgePhase.DRONE_CONNECTED
        when {
            waiting && holding -> {
                title = stringResource(R.string.hero_drone_lost)
                subtitle = stringResource(R.string.hero_drone_lost_subtitle)
            }
            waiting && testVideo != null -> {
                title = stringResource(R.string.hero_test_video)
                subtitle = stringResource(R.string.hero_test_video_subtitle)
            }
            phase == BridgePhase.DRONE_CONNECTED -> {
                title = stringResource(R.string.hero_connected)
                subtitle = stringResource(R.string.hero_connected_subtitle)
            }
            phase == BridgePhase.WAITING_FOR_DRONE -> {
                title = stringResource(R.string.hero_waiting)
                subtitle = target.toSentence(R.string.hero_waiting_subtitle_one, R.string.hero_waiting_subtitle_many)
            }
            phase == BridgePhase.STARTING -> {
                title = stringResource(R.string.hero_starting)
                subtitle = stringResource(R.string.hero_starting_subtitle)
            }
            phase == BridgePhase.LIVE -> {
                title = target.single?.let { stringResource(R.string.hero_live_one, stringResource(it.onPlatform)).sentenceStart() }
                    ?: pluralStringResource(R.plurals.hero_live_many, target.count, target.count)
                subtitle = null
            }
            phase == BridgePhase.RECONNECTING -> {
                title = stringResource(R.string.hero_connection_lost)
                subtitle = target.toSentence(R.string.hero_reconnecting_one, R.string.hero_reconnecting_many)
            }
            phase == BridgePhase.RECEIVER_ERROR -> {
                title = stringResource(R.string.hero_receiver_stopped)
                subtitle = snapshot.error?.asString()
            }
            phase == BridgePhase.IDLE || phase == BridgePhase.START_FAILED -> {
                title = stringResource(R.string.hero_off)
                subtitle = snapshot.error?.asString()
            }
            else -> {
                title = target.toSentence(R.string.hero_connecting_one, R.string.hero_connecting_many)
                subtitle = null
            }
        }
    }
    val tip = liveTips(phase, snapshot, liveDestinations).firstOrNull()
    Column(
        modifier = modifier
            .background(OverlayGlass, RoundedCornerShape(24.dp))
            .padding(horizontal = 18.dp, vertical = 16.dp),
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Text(
            modifier = Modifier.semantics { liveRegion = LiveRegionMode.Polite },
            text = title,
            color = Color.White,
            style = MaterialTheme.typography.titleLarge,
            fontWeight = FontWeight.SemiBold,
        )
        subtitle?.let {
            Text(text = it, color = Color.White.copy(alpha = 0.75f), style = MaterialTheme.typography.bodyMedium)
        }
        tip?.let {
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.Top) {
                Icon(
                    Icons.Rounded.Lightbulb,
                    contentDescription = null,
                    tint = Color(0xFFFBBF24),
                    modifier = Modifier
                        .padding(top = 2.dp)
                        .size(16.dp),
                )
                Text(text = it, color = Color.White.copy(alpha = 0.9f), style = MaterialTheme.typography.bodySmall)
            }
        }
        if (live) {
            PlatformRow(
                platforms = liveDestinations.map { profile ->
                    profile.kind to outputLabel(snapshot.output(profile.id)?.status, colors).second
                },
                label = target.label(),
                clickLabel = stringResource(R.string.show_details),
                onClick = onPlatforms,
            )
        } else {
            PlatformRow(
                platforms = selected.map { it.kind to null },
                label = selected.singleOrNull()?.kind?.displayName()
                    ?: if (selected.isEmpty()) stringResource(R.string.no_platform) else pluralStringResource(R.plurals.platform_count, selected.size, selected.size),
                clickLabel = stringResource(R.string.change_platforms),
                onClick = onPlatforms,
            )
        }
    }
}

/**
 * The platforms' tiles, each with its state as a dot once live, and a way to see more. Several
 * tiles speak for themselves; their count is only read out.
 */
@Composable
private fun PlatformRow(
    platforms: List<Pair<DestinationKind, Color?>>,
    label: String,
    clickLabel: String,
    onClick: () -> Unit,
) {
    val several = platforms.size > 1
    Row(
        modifier = Modifier
            .padding(top = 4.dp)
            .clip(RoundedCornerShape(12.dp))
            .clickable(onClickLabel = clickLabel, role = Role.Button, onClick = onClick)
            .semantics { if (several) contentDescription = label }
            .heightIn(min = 40.dp)
            .padding(end = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        if (platforms.isNotEmpty()) {
            Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                platforms.take(MAX_TILES).forEach { (kind, state) ->
                    Box {
                        PlatformTile(kind = kind, size = 28.dp)
                        state?.let {
                            Box(
                                modifier = Modifier
                                    .align(Alignment.BottomEnd)
                                    .offset(x = 3.dp, y = 3.dp)
                                    .size(11.dp)
                                    .background(it, CircleShape)
                                    .border(1.5.dp, Color.Black, CircleShape),
                            )
                        }
                    }
                }
            }
        }
        if (!several) {
            Text(
                modifier = Modifier.weight(1f, fill = false),
                text = label,
                color = Color.White,
                style = MaterialTheme.typography.labelLarge,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
        Icon(
            imageVector = Icons.AutoMirrored.Rounded.KeyboardArrowRight,
            contentDescription = null,
            tint = Color.White.copy(alpha = 0.7f),
            modifier = Modifier.size(20.dp),
        )
    }
}

/** The one big button: go live, choose a platform first, or end the broadcast. */
@Composable
private fun ActionPill(
    text: String,
    icon: ImageVector,
    color: Color,
    onClick: () -> Unit,
    enabled: Boolean = true,
) {
    Button(
        onClick = onClick,
        enabled = enabled,
        modifier = Modifier
            .heightIn(min = 64.dp)
            .widthIn(max = ACTION_MAX_WIDTH),
        shape = RoundedCornerShape(percent = 50),
        colors = ButtonDefaults.buttonColors(
            containerColor = color,
            contentColor = Color.White,
            disabledContainerColor = color.copy(alpha = 0.45f),
            disabledContentColor = Color.White.copy(alpha = 0.7f),
        ),
        contentPadding = PaddingValues(horizontal = 22.dp, vertical = 12.dp),
    ) {
        Icon(icon, contentDescription = null, modifier = Modifier.size(26.dp))
        Spacer(Modifier.width(10.dp))
        Text(text = text, style = MaterialTheme.typography.titleMedium, maxLines = 2, overflow = TextOverflow.Ellipsis)
    }
}

/** Why going live or the test video failed, until the next attempt or until closed. */
@Composable
private fun NoticeCard(notice: RelayNotice) {
    var closed by remember(notice) { mutableStateOf(false) }
    if (closed) return
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .background(Color(0xE6431216), RoundedCornerShape(20.dp))
            .padding(start = 16.dp, top = 12.dp, bottom = 12.dp, end = 4.dp)
            .semantics(mergeDescendants = true) { liveRegion = LiveRegionMode.Assertive },
        verticalAlignment = Alignment.Top,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Icon(Icons.Rounded.ErrorOutline, contentDescription = null, tint = Color(0xFFFCA5A5), modifier = Modifier.size(20.dp))
        Column(modifier = Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(text = notice.title.asString(), color = Color.White, style = MaterialTheme.typography.titleSmall)
            Text(text = notice.message.asString(), color = Color.White.copy(alpha = 0.85f), style = MaterialTheme.typography.bodySmall)
        }
        IconButton(onClick = { closed = true }) {
            Icon(Icons.Rounded.Close, contentDescription = stringResource(R.string.close), tint = Color.White)
        }
    }
}

@Composable
private fun GlassIconButton(icon: ImageVector, description: String, onClick: () -> Unit) {
    IconButton(
        onClick = onClick,
        colors = IconButtonDefaults.iconButtonColors(containerColor = OverlayGlass, contentColor = Color.White),
    ) {
        Icon(icon, contentDescription = description)
    }
}

/** Everything that is not about the picture: sheets, settings and help. */
@Composable
private fun DroneMenu(
    live: Boolean,
    testing: Boolean,
    onPlatforms: () -> Unit,
    onDetails: () -> Unit,
    onStopTestVideo: () -> Unit,
    onLanguage: () -> Unit,
    onTheme: () -> Unit,
    onHelp: () -> Unit,
) {
    var open by remember { mutableStateOf(false) }
    Box {
        GlassIconButton(icon = Icons.Rounded.MoreVert, description = stringResource(R.string.more_options), onClick = { open = true })
        DropdownMenu(expanded = open, onDismissRequest = { open = false }) {
            val closing: (() -> Unit) -> () -> Unit = { action ->
                {
                    open = false
                    action()
                }
            }
            if (!live) MenuItem(R.string.platforms, Icons.Rounded.GridView, closing(onPlatforms))
            MenuItem(R.string.technical_details, Icons.Rounded.Info, closing(onDetails))
            if (testing) MenuItem(R.string.stop_test_video, Icons.Rounded.Stop, closing(onStopTestVideo))
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) MenuItem(R.string.language, Icons.Rounded.Language, closing(onLanguage))
            MenuItem(R.string.theme, Icons.Rounded.Palette, closing(onTheme))
            MenuItem(R.string.how_to_use, Icons.AutoMirrored.Rounded.HelpOutline, closing(onHelp))
        }
    }
}

@Composable
private fun MenuItem(@StringRes text: Int, icon: ImageVector, onClick: () -> Unit) {
    DropdownMenuItem(
        text = { Text(stringResource(text)) },
        leadingIcon = { Icon(icon, contentDescription = null) },
        onClick = onClick,
    )
}

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

private val INFO_CARD_MAX_WIDTH = 420.dp
private val STATUS_CHIP_MAX_WIDTH = 240.dp
private val ACTION_MAX_WIDTH = 200.dp
private const val MAX_TILES = 3
