package com.streammydrone.app.virtualmicrophone

import android.Manifest
import android.content.ActivityNotFoundException
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Intent
import android.os.Build
import android.provider.Settings
import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.ArrowDropDown
import androidx.compose.material.icons.rounded.Close
import androidx.compose.material3.Button
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.compose.LocalLifecycleOwner
import com.streammydrone.app.BridgeTheme
import com.streammydrone.app.R

/** Keeps an explicit HFP control connection; microphone recognition is checked on the controller. */
@Composable
internal fun VirtualMicrophoneDiagnosticsDialog(onDismiss: () -> Unit) {
    val context = LocalContext.current
    val lifecycleOwner = LocalLifecycleOwner.current
    val manager = remember(context) { BluetoothDiagnosticsManager(context) }
    val control = VirtualMicrophoneControlState.value
    var developerMode by remember { mutableStateOf(false) }
    var copied by remember { mutableStateOf(false) }
    var settingsUnavailable by remember { mutableStateOf(false) }
    var deviceMenuOpen by remember { mutableStateOf(false) }
    val permissionRequest = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) {
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
    Dialog(
        onDismissRequest = onDismiss,
        properties = DialogProperties(usePlatformDefaultWidth = false, decorFitsSystemWindows = false),
    ) {
        BackHandler(onBack = onDismiss)
        Surface(color = BridgeTheme.colors.background, modifier = Modifier.fillMaxSize()) {
            Column(Modifier.fillMaxSize().windowInsetsPadding(WindowInsets.safeDrawing)) {
                Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.fillMaxWidth().padding(4.dp)) {
                    IconButton(onClick = onDismiss) { Icon(Icons.Rounded.Close, stringResource(R.string.close)) }
                    Text(stringResource(R.string.vmic_title), style = MaterialTheme.typography.titleMedium, modifier = Modifier.weight(1f))
                }
                Column(
                    Modifier.weight(1f).verticalScroll(rememberScrollState()).padding(16.dp),
                    verticalArrangement = Arrangement.spacedBy(14.dp),
                ) {
                    Text(stringResource(R.string.vmic_control_purpose), style = MaterialTheme.typography.bodyMedium)
                    Surface(color = BridgeTheme.colors.accentSoft, shape = MaterialTheme.shapes.medium) {
                        Column(Modifier.fillMaxWidth().padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                            Text(stringResource(controlLabel(control.phase)), style = MaterialTheme.typography.titleSmall)
                            Text(stringResource(R.string.vmic_fly_unverified), style = MaterialTheme.typography.bodySmall)
                            if (control.phase == VirtualMicrophoneControlPhase.CONNECTED) {
                                Text(stringResource(R.string.vmic_check_controller), style = MaterialTheme.typography.bodyMedium)
                            }
                        }
                    }
                    Text(stringResource(adapterLabel(manager.adapterStatus)), style = MaterialTheme.typography.titleSmall)
                    if (manager.adapterStatus == AdapterStatus.PERMISSION_REQUIRED) {
                        Text(stringResource(R.string.vmic_permission_reason), style = MaterialTheme.typography.bodySmall)
                        Button(onClick = {
                            if (Build.VERSION.SDK_INT >= 31) permissionRequest.launch(Manifest.permission.BLUETOOTH_CONNECT)
                            else manager.refresh()
                        }) { Text(stringResource(R.string.vmic_allow_bluetooth)) }
                    }
                    if (manager.adapterStatus == AdapterStatus.ON) {
                        Text(stringResource(R.string.vmic_paired_devices), style = MaterialTheme.typography.titleSmall)
                        if (manager.pairedDevices.isEmpty()) Text(stringResource(R.string.vmic_no_paired_devices), color = BridgeTheme.colors.muted)
                        Box {
                            OutlinedButton(
                                onClick = { deviceMenuOpen = true },
                                enabled = !control.isActive && manager.pairedDevices.isNotEmpty(),
                                modifier = Modifier.fillMaxWidth(),
                            ) {
                                Text(
                                    manager.pairedDevices.firstOrNull { it.key == manager.selectedKey }?.label
                                        ?: stringResource(R.string.vmic_paired_devices),
                                    modifier = Modifier.weight(1f),
                                    maxLines = 1,
                                    overflow = TextOverflow.Ellipsis,
                                )
                                Icon(Icons.Rounded.ArrowDropDown, contentDescription = null)
                            }
                            DropdownMenu(
                                expanded = deviceMenuOpen && !control.isActive,
                                onDismissRequest = { deviceMenuOpen = false },
                                modifier = Modifier.heightIn(max = 280.dp),
                            ) {
                                manager.pairedDevices.forEach { device ->
                                    DropdownMenuItem(
                                        text = { Text(device.label, maxLines = 1, overflow = TextOverflow.Ellipsis) },
                                        onClick = {
                                            manager.selectDevice(device.key)
                                            deviceMenuOpen = false
                                            copied = false
                                        },
                                    )
                                }
                            }
                        }
                    }
                    if (control.isActive) {
                        Button(onClick = manager::disconnect) { Text(stringResource(R.string.vmic_disconnect)) }
                        Text(stringResource(R.string.vmic_connection_kept_open), style = MaterialTheme.typography.bodySmall, color = BridgeTheme.colors.muted)
                    } else {
                        Button(onClick = { manager.connect(); copied = false }, enabled = manager.canConnect) {
                            Text(stringResource(R.string.vmic_connect))
                        }
                    }
                    Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                        TextButton(onClick = { manager.refresh(); copied = false }) { Text(stringResource(R.string.vmic_refresh)) }
                        TextButton(onClick = {
                            try {
                                context.startActivity(Intent(Settings.ACTION_BLUETOOTH_SETTINGS))
                                settingsUnavailable = false
                            } catch (_: ActivityNotFoundException) {
                                settingsUnavailable = true
                            }
                        }) { Text(stringResource(R.string.vmic_bluetooth_settings)) }
                    }
                    if (settingsUnavailable) Text(stringResource(R.string.vmic_settings_unavailable), color = BridgeTheme.colors.dangerText)
                    HorizontalDivider()
                    Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.fillMaxWidth()) {
                        Text(stringResource(R.string.vmic_developer_mode), modifier = Modifier.weight(1f))
                        Switch(checked = developerMode, onCheckedChange = {
                            developerMode = it
                            if (!it) manager.cancelQuery()
                        })
                    }
                    if (developerMode) {
                        HorizontalDivider()
                        Text(stringResource(R.string.vmic_diagnostics_scope), style = MaterialTheme.typography.bodySmall, color = BridgeTheme.colors.muted)
                        if (manager.adapterStatus == AdapterStatus.ON) {
                            if (manager.selectedKey != null) {
                                Text(stringResource(R.string.vmic_sdp_warning), style = MaterialTheme.typography.bodySmall, color = BridgeTheme.colors.muted)
                                Text(stringResource(sdpLabel(manager.attempt.status)))
                                if (manager.attempt.status == SdpStatus.PENDING) {
                                    TextButton(onClick = manager::cancelQuery) { Text(stringResource(R.string.cancel)) }
                                } else {
                                    Button(onClick = { manager.queryServices(); copied = false }, enabled = !control.isActive) {
                                        Text(stringResource(R.string.vmic_query_services))
                                    }
                                }
                            }
                        }
                        HorizontalDivider()
                        SelectionContainer {
                            Text(manager.report().sanitizedText(), style = MaterialTheme.typography.bodySmall, fontFamily = FontFamily.Monospace)
                        }
                        Text(stringResource(R.string.vmic_report_privacy), style = MaterialTheme.typography.bodySmall, color = BridgeTheme.colors.muted)
                        OutlinedButton(onClick = {
                            context.getSystemService(ClipboardManager::class.java).setPrimaryClip(
                                ClipData.newPlainText("Stream My Drone Bluetooth diagnostics", manager.report().sanitizedText()),
                            )
                            copied = true
                        }) { Text(stringResource(if (copied) R.string.vmic_copied else R.string.vmic_copy_report)) }
                    }
                }
            }
        }
    }
}

private fun adapterLabel(status: AdapterStatus): Int = when (status) {
    AdapterStatus.PERMISSION_REQUIRED -> R.string.vmic_bluetooth_permission
    AdapterStatus.UNAVAILABLE -> R.string.vmic_bluetooth_unavailable
    AdapterStatus.OFF -> R.string.vmic_bluetooth_off
    AdapterStatus.ON -> R.string.vmic_bluetooth_on
    AdapterStatus.UNKNOWN -> R.string.vmic_bluetooth_unknown
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
