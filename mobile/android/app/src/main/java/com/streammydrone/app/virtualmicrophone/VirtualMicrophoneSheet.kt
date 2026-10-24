package com.streammydrone.app.virtualmicrophone

import android.Manifest
import android.annotation.SuppressLint
import android.bluetooth.BluetoothAdapter
import android.content.ActivityNotFoundException
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.os.Build
import android.provider.Settings
import androidx.activity.compose.ManagedActivityResultLauncher
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.ActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.annotation.StringRes
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Bluetooth
import androidx.compose.material.icons.rounded.BluetoothDisabled
import androidx.compose.material.icons.rounded.CheckCircle
import androidx.compose.material.icons.rounded.ContentCopy
import androidx.compose.material.icons.rounded.ExpandMore
import androidx.compose.material.icons.rounded.LinkOff
import androidx.compose.material.icons.rounded.Mic
import androidx.compose.material.icons.rounded.MicOff
import androidx.compose.material.icons.rounded.Refresh
import androidx.compose.material.icons.rounded.Search
import androidx.compose.material.icons.rounded.Settings
import androidx.compose.material.icons.rounded.SwapHoriz
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
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
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.compose.LocalLifecycleOwner
import com.streammydrone.app.BigButton
import com.streammydrone.app.DangerColor
import com.streammydrone.app.GoLiveEnd
import com.streammydrone.app.GoLiveStart
import com.streammydrone.app.PanelButton
import com.streammydrone.app.R
import com.streammydrone.app.ReadyColor
import com.streammydrone.app.WarningColor
import com.streammydrone.app.numberedLines

/**
 * The phone as RC 2's Bluetooth microphone, in the dark style of the screen under it: what is
 * going on, the controller, and the one button that does the next step. How it works and the
 * developer diagnostics are folded away. The HFP control connection is held by
 * [VirtualMicrophoneControlService], so it stays open when this sheet closes.
 */
@Composable
internal fun VirtualMicrophoneSheet() {
    val context = LocalContext.current
    val lifecycleOwner = LocalLifecycleOwner.current
    val manager = remember(context) { BluetoothDiagnosticsManager(context) }
    val control = VirtualMicrophoneControlState.value
    var settingsUnavailable by remember { mutableStateOf(false) }
    val openSettings = { settingsUnavailable = !openBluetoothSettings(context) }
    val permissionRequest = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) {
        manager.refresh()
    }
    val enableRequest = rememberLauncherForActivityResult(ActivityResultContracts.StartActivityForResult()) {
        manager.refresh()
    }
    DisposableEffect(manager, lifecycleOwner) {
        val observer = LifecycleEventObserver { _, event ->
            when (event) {
                Lifecycle.Event.ON_START -> manager.start()
                Lifecycle.Event.ON_RESUME -> manager.refresh()
                Lifecycle.Event.ON_STOP -> manager.stop()
                else -> Unit
            }
        }
        lifecycleOwner.lifecycle.addObserver(observer)
        if (lifecycleOwner.lifecycle.currentState.isAtLeast(Lifecycle.State.STARTED)) manager.start()
        onDispose {
            lifecycleOwner.lifecycle.removeObserver(observer)
            manager.stop()
        }
    }
    val selected = manager.pairedDevices.firstOrNull { it.key == manager.selectedKey }
    val connected = control.phase == VirtualMicrophoneControlPhase.CONNECTED

    Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
        Text(
            text = stringResource(R.string.vmic_title),
            color = Color.White,
            style = MaterialTheme.typography.titleSmall,
            fontWeight = FontWeight.SemiBold,
            modifier = Modifier.semantics { heading() },
        )
        Text(
            text = stringResource(R.string.vmic_control_purpose),
            color = Color.White.copy(alpha = 0.6f),
            style = MaterialTheme.typography.bodySmall,
        )
    }
    StatusCard(statusLook(manager.adapterStatus, control.phase, selected?.label), busy = control.isActive && !connected)
    when (manager.adapterStatus) {
        AdapterStatus.PERMISSION_REQUIRED -> MainButton(R.string.vmic_allow_bluetooth, Icons.Rounded.Bluetooth) {
            if (Build.VERSION.SDK_INT >= 31) permissionRequest.launch(Manifest.permission.BLUETOOTH_CONNECT) else manager.refresh()
        }
        AdapterStatus.OFF -> MainButton(R.string.vmic_turn_on_bluetooth, Icons.Rounded.Bluetooth) {
            if (!requestBluetoothOn(enableRequest)) openSettings()
        }
        AdapterStatus.ON -> when {
            manager.pairedDevices.isEmpty() -> {
                SheetText(stringResource(R.string.vmic_no_paired_devices))
                PanelButton(text = stringResource(R.string.vmic_bluetooth_settings), icon = Icons.Rounded.Settings, onClick = openSettings)
            }
            // The status card names the device while connected; it cannot change then.
            control.isActive -> BigButton(
                text = stringResource(R.string.vmic_disconnect),
                icon = Icons.Rounded.LinkOff,
                brush = SolidColor(Color.White.copy(alpha = 0.12f)),
                onClick = manager::disconnect,
                modifier = Modifier.fillMaxWidth(),
            )
            else -> {
                DevicePicker(
                    devices = manager.pairedDevices,
                    selectedKey = manager.selectedKey,
                    onSelect = manager::selectDevice,
                    onOpenSettings = openSettings,
                )
                MainButton(
                    text = if (control.phase == VirtualMicrophoneControlPhase.FAILED) R.string.retry else R.string.vmic_connect,
                    icon = Icons.Rounded.Mic,
                    enabled = manager.canConnect,
                    onClick = manager::connect,
                )
            }
        }
        AdapterStatus.UNAVAILABLE, AdapterStatus.UNKNOWN -> Unit
    }
    if (connected) ConnectedNote()
    if (settingsUnavailable) SheetText(stringResource(R.string.vmic_settings_unavailable), color = DangerColor)
    Fold(stringResource(R.string.vmic_how_it_works)) {
        numberedLines(stringResource(R.string.vmic_steps)).forEachIndexed { index, step -> Step(index + 1, step) }
        // Pairing is the first step; with nothing paired the link is already on screen.
        if (manager.pairedDevices.isNotEmpty()) {
            PanelButton(text = stringResource(R.string.vmic_bluetooth_settings), icon = Icons.Rounded.Settings, onClick = openSettings)
        }
    }
    HorizontalDivider(color = Color.White.copy(alpha = 0.1f))
    Fold(stringResource(R.string.vmic_developer_mode), onToggle = { open -> if (!open) manager.cancelQuery() }) {
        DeveloperDiagnostics(manager, control)
    }
}

private class StatusLook(val icon: ImageVector, val color: Color, @StringRes val title: Int, val detail: String?)

/** The connection first, since it outlives this sheet; then whatever Bluetooth needs before it. */
@Composable
private fun statusLook(adapter: AdapterStatus, phase: VirtualMicrophoneControlPhase, device: String?): StatusLook {
    val idle = Color.White.copy(alpha = 0.6f)
    return when {
        phase == VirtualMicrophoneControlPhase.CONNECTED -> StatusLook(Icons.Rounded.Mic, ReadyColor, R.string.vmic_state_connected, device)
        phase == VirtualMicrophoneControlPhase.CONNECTING || phase == VirtualMicrophoneControlPhase.NEGOTIATING ->
            StatusLook(Icons.Rounded.Mic, WarningColor, R.string.vmic_state_connecting, device)
        adapter == AdapterStatus.PERMISSION_REQUIRED -> StatusLook(
            Icons.Rounded.BluetoothDisabled, WarningColor, R.string.vmic_bluetooth_permission, stringResource(R.string.vmic_permission_reason),
        )
        adapter == AdapterStatus.OFF -> StatusLook(Icons.Rounded.BluetoothDisabled, WarningColor, R.string.vmic_bluetooth_off, null)
        adapter == AdapterStatus.UNAVAILABLE -> StatusLook(Icons.Rounded.BluetoothDisabled, DangerColor, R.string.vmic_bluetooth_unavailable, null)
        adapter == AdapterStatus.UNKNOWN -> StatusLook(Icons.Rounded.BluetoothDisabled, idle, R.string.vmic_bluetooth_unknown, null)
        phase == VirtualMicrophoneControlPhase.FAILED ->
            StatusLook(Icons.Rounded.MicOff, DangerColor, R.string.vmic_state_failed, stringResource(R.string.vmic_failed_hint))
        else -> StatusLook(Icons.Rounded.MicOff, idle, R.string.vmic_state_not_connected, null)
    }
}

@Composable
private fun StatusCard(look: StatusLook, busy: Boolean) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clip(CARD_SHAPE)
            .background(look.color.copy(alpha = 0.1f))
            .border(1.dp, look.color.copy(alpha = 0.35f), CARD_SHAPE)
            .semantics(mergeDescendants = true) { liveRegion = LiveRegionMode.Polite }
            .padding(12.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Box(
            modifier = Modifier
                .size(40.dp)
                .background(look.color.copy(alpha = 0.18f), CircleShape),
            contentAlignment = Alignment.Center,
        ) {
            if (busy) {
                CircularProgressIndicator(modifier = Modifier.size(20.dp), color = look.color, strokeWidth = 2.dp)
            } else {
                Icon(look.icon, contentDescription = null, tint = look.color, modifier = Modifier.size(22.dp))
            }
        }
        Column(modifier = Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(
                text = stringResource(look.title),
                color = Color.White,
                style = MaterialTheme.typography.labelLarge,
            )
            look.detail?.let {
                Text(text = it, color = Color.White.copy(alpha = 0.6f), style = MaterialTheme.typography.labelSmall)
            }
        }
    }
}

/** The orange button, the next step; its name is the action. */
@Composable
private fun MainButton(@StringRes text: Int, icon: ImageVector, enabled: Boolean = true, onClick: () -> Unit) {
    BigButton(
        text = stringResource(text),
        icon = icon,
        brush = Brush.horizontalGradient(listOf(GoLiveStart, GoLiveEnd)),
        enabled = enabled,
        onClick = onClick,
        modifier = Modifier.fillMaxWidth(),
    )
}

/**
 * The paired controller to connect to. One chosen device shows alone with "Change"; with none
 * chosen yet, every paired device is listed.
 */
@Composable
private fun DevicePicker(
    devices: List<PairedDiagnosticDevice>,
    selectedKey: String?,
    onSelect: (String) -> Unit,
    onOpenSettings: () -> Unit,
) {
    var open by rememberSaveable { mutableStateOf(false) }
    val selected = devices.firstOrNull { it.key == selectedKey }
    val listed = selected == null || open
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .clip(CARD_SHAPE)
            .background(Color.White.copy(alpha = 0.06f))
            .border(1.dp, Color.White.copy(alpha = 0.1f), CARD_SHAPE)
            .padding(vertical = 4.dp),
    ) {
        if (selected != null) {
            DeviceRow(label = selected.label, chosen = true) {
                if (devices.size > 1) {
                    PanelButton(text = stringResource(R.string.change), icon = Icons.Rounded.SwapHoriz, onClick = { open = !open })
                }
            }
        }
        AnimatedVisibility(visible = listed) {
            Column {
                if (selected == null) {
                    SheetText(
                        text = stringResource(R.string.vmic_pick_controller),
                        color = Color.White.copy(alpha = 0.6f),
                        modifier = Modifier.padding(horizontal = 12.dp, vertical = 6.dp),
                    )
                } else {
                    HorizontalDivider(color = Color.White.copy(alpha = 0.08f), modifier = Modifier.padding(horizontal = 12.dp))
                }
                devices.filter { it.key != selectedKey }.forEach { device ->
                    DeviceRow(
                        label = device.label,
                        chosen = false,
                        onClick = {
                            onSelect(device.key)
                            open = false
                        },
                    )
                }
                Box(modifier = Modifier.padding(horizontal = 8.dp)) {
                    PanelButton(text = stringResource(R.string.vmic_bluetooth_settings), icon = Icons.Rounded.Settings, onClick = onOpenSettings)
                }
            }
        }
    }
}

@Composable
private fun DeviceRow(
    label: String,
    chosen: Boolean,
    onClick: (() -> Unit)? = null,
    trailing: @Composable () -> Unit = {},
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .then(if (onClick != null) Modifier.clickable(role = Role.RadioButton, onClick = onClick) else Modifier)
            .heightIn(min = 44.dp)
            .padding(horizontal = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Icon(
            imageVector = if (chosen) Icons.Rounded.CheckCircle else Icons.Rounded.Bluetooth,
            contentDescription = null,
            tint = if (chosen) GoLiveStart else Color.White.copy(alpha = 0.6f),
            modifier = Modifier.size(18.dp),
        )
        Text(
            text = label,
            color = Color.White,
            style = MaterialTheme.typography.labelLarge,
            fontWeight = if (chosen) FontWeight.SemiBold else FontWeight.Normal,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.weight(1f),
        )
        trailing()
    }
}

/** What to look at on RC 2 now, and that closing this sheet does not end the connection. */
@Composable
private fun ConnectedNote() {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .clip(CARD_SHAPE)
            .background(Color.White.copy(alpha = 0.06f))
            .padding(12.dp),
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        SheetText(stringResource(R.string.vmic_check_controller), color = Color.White.copy(alpha = 0.85f))
        SheetText(stringResource(R.string.vmic_connection_kept_open), color = Color.White.copy(alpha = 0.6f))
    }
}

@Composable
private fun Step(number: Int, text: String) {
    Row(horizontalArrangement = Arrangement.spacedBy(10.dp), verticalAlignment = Alignment.Top) {
        Box(
            modifier = Modifier
                .size(20.dp)
                .background(Color.White.copy(alpha = 0.12f), CircleShape),
            contentAlignment = Alignment.Center,
        ) {
            Text("$number", color = Color.White, style = MaterialTheme.typography.labelSmall)
        }
        SheetText(text, color = Color.White.copy(alpha = 0.8f), modifier = Modifier.weight(1f).padding(top = 1.dp))
    }
}

/** A line that opens what is under it, like "Not connecting?" on the connect card. */
@Composable
private fun Fold(title: String, onToggle: (open: Boolean) -> Unit = {}, content: @Composable ColumnScope.() -> Unit) {
    var open by rememberSaveable { mutableStateOf(false) }
    val rotation by animateFloatAsState(if (open) 180f else 0f, label = "fold")
    val expandLabel = stringResource(if (open) R.string.collapse else R.string.expand)
    val state = stringResource(if (open) R.string.state_expanded else R.string.state_collapsed)
    Column {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .clip(RoundedCornerShape(8.dp))
                .clickable(onClickLabel = expandLabel, role = Role.Button) {
                    open = !open
                    onToggle(open)
                }
                .semantics { stateDescription = state }
                .heightIn(min = 40.dp)
                .padding(horizontal = 4.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(
                text = title,
                color = Color.White.copy(alpha = 0.85f),
                style = MaterialTheme.typography.labelLarge,
                modifier = Modifier.weight(1f),
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
                modifier = Modifier.padding(start = 4.dp, top = 6.dp),
                verticalArrangement = Arrangement.spacedBy(8.dp),
                content = content,
            )
        }
    }
}

/** Remote services and the real HFP stages, for working out why a controller does not connect. */
@Composable
private fun DeveloperDiagnostics(manager: BluetoothDiagnosticsManager, control: VirtualMicrophoneControlSnapshot) {
    val context = LocalContext.current
    var copied by remember { mutableStateOf(false) }
    val muted = Color.White.copy(alpha = 0.6f)
    SheetText(stringResource(R.string.vmic_diagnostics_scope), color = muted)
    SheetText(stringResource(controlLabel(control.phase)), color = Color.White.copy(alpha = 0.85f))
    SheetText(stringResource(R.string.vmic_fly_unverified), color = muted)
    if (manager.adapterStatus == AdapterStatus.ON && manager.selectedKey != null) {
        SheetText(stringResource(R.string.vmic_sdp_warning), color = muted)
        SheetText(stringResource(sdpLabel(manager.attempt.status)), color = Color.White.copy(alpha = 0.85f))
        if (manager.attempt.status == SdpStatus.PENDING) {
            PanelButton(text = stringResource(R.string.cancel), icon = Icons.Rounded.LinkOff, onClick = manager::cancelQuery)
        } else if (!control.isActive) {
            PanelButton(
                text = stringResource(R.string.vmic_query_services),
                icon = Icons.Rounded.Search,
                onClick = {
                    manager.queryServices()
                    copied = false
                },
            )
        }
    }
    SelectionContainer {
        Text(
            text = manager.report().sanitizedText(),
            color = Color.White.copy(alpha = 0.85f),
            style = MaterialTheme.typography.labelSmall,
            fontFamily = FontFamily.Monospace,
            modifier = Modifier
                .fillMaxWidth()
                .clip(RoundedCornerShape(10.dp))
                .background(Color.White.copy(alpha = 0.06f))
                .padding(10.dp),
        )
    }
    SheetText(stringResource(R.string.vmic_report_privacy), color = muted)
    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        PanelButton(
            text = stringResource(if (copied) R.string.vmic_copied else R.string.vmic_copy_report),
            icon = Icons.Rounded.ContentCopy,
            onClick = {
                context.getSystemService(ClipboardManager::class.java).setPrimaryClip(
                    ClipData.newPlainText("Stream My Drone Bluetooth diagnostics", manager.report().sanitizedText()),
                )
                copied = true
            },
        )
        PanelButton(
            text = stringResource(R.string.vmic_refresh),
            icon = Icons.Rounded.Refresh,
            onClick = {
                manager.refresh()
                copied = false
            },
        )
    }
}

@Composable
private fun SheetText(text: String, modifier: Modifier = Modifier, color: Color = Color.White.copy(alpha = 0.85f)) {
    Text(text = text, color = color, style = MaterialTheme.typography.bodySmall, modifier = modifier)
}

private fun openBluetoothSettings(context: Context): Boolean = try {
    context.startActivity(Intent(Settings.ACTION_BLUETOOTH_SETTINGS))
    true
} catch (_: ActivityNotFoundException) {
    false
}

/** Android's own "Turn on Bluetooth?" prompt; false sends the user to the settings instead. */
@SuppressLint("MissingPermission") // Offered only once BLUETOOTH_CONNECT is granted; a revoked permission falls back.
private fun requestBluetoothOn(launcher: ManagedActivityResultLauncher<Intent, ActivityResult>): Boolean = try {
    launcher.launch(Intent(BluetoothAdapter.ACTION_REQUEST_ENABLE))
    true
} catch (_: ActivityNotFoundException) {
    false
} catch (_: SecurityException) {
    false
}

private fun sdpLabel(status: SdpStatus): Int = when (status) {
    SdpStatus.IDLE -> R.string.vmic_sdp_idle
    SdpStatus.PENDING -> R.string.vmic_sdp_pending
    SdpStatus.RECEIVED -> R.string.vmic_sdp_received
    SdpStatus.EMPTY -> R.string.vmic_sdp_empty
    SdpStatus.TIMED_OUT -> R.string.vmic_sdp_timeout
    SdpStatus.FAILED -> R.string.vmic_sdp_failed
    SdpStatus.CANCELLED -> R.string.vmic_sdp_cancelled
}

private fun controlLabel(phase: VirtualMicrophoneControlPhase): Int = when (phase) {
    VirtualMicrophoneControlPhase.IDLE -> R.string.vmic_control_idle
    VirtualMicrophoneControlPhase.CONNECTING -> R.string.vmic_control_connecting
    VirtualMicrophoneControlPhase.NEGOTIATING -> R.string.vmic_control_negotiating
    VirtualMicrophoneControlPhase.CONNECTED -> R.string.vmic_control_connected
    VirtualMicrophoneControlPhase.DISCONNECTED -> R.string.vmic_control_disconnected
    VirtualMicrophoneControlPhase.FAILED -> R.string.vmic_control_failed
}

private val CARD_SHAPE = RoundedCornerShape(14.dp)
