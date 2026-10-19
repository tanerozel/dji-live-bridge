package com.streammydrone.app

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Close
import androidx.compose.material.icons.rounded.MoreVert
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import androidx.compose.ui.window.SecureFlagPolicy
import kotlinx.coroutines.delay

@Composable
internal fun InstagramProducerDialog(
    session: InstagramProducerSession,
    live: Boolean,
    onExternalBrowser: () -> Unit,
    onDismiss: () -> Unit,
) {
    if (!session.visible) return
    var controlMissing by remember { mutableStateOf(false) }
    var options by remember { mutableStateOf(false) }
    Dialog(
        onDismissRequest = onDismiss,
        properties = DialogProperties(usePlatformDefaultWidth = false, decorFitsSystemWindows = false, securePolicy = SecureFlagPolicy.SecureOn),
    ) {
        BackHandler(onBack = onDismiss)
        Surface(color = BridgeTheme.colors.background, modifier = Modifier.fillMaxSize()) {
            Column(modifier = Modifier.fillMaxSize().windowInsetsPadding(WindowInsets.safeDrawing).imePadding()) {
                Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.fillMaxWidth()) {
                    IconButton(onClick = onDismiss) { Icon(Icons.Rounded.Close, stringResource(R.string.close)) }
                    Text("Instagram · ${session.host}", style = MaterialTheme.typography.titleSmall, modifier = Modifier.weight(1f))
                    Box {
                        IconButton(onClick = { options = true }) { Icon(Icons.Rounded.MoreVert, stringResource(R.string.more)) }
                        DropdownMenu(expanded = options, onDismissRequest = { options = false }) {
                            if (live) DropdownMenuItem(
                                text = { Text(stringResource(R.string.instagram_show_live_control)) },
                                onClick = { options = false; session.showLiveControl { controlMissing = !it } },
                            )
                            DropdownMenuItem(
                                text = { Text(stringResource(R.string.instagram_open_browser)) },
                                onClick = { options = false; session.hide(); onExternalBrowser() },
                            )
                            if (!live) DropdownMenuItem(
                                text = { Text(stringResource(R.string.instagram_clear_session)) },
                                onClick = { options = false; session.clearWebSession() },
                            )
                        }
                    }
                }
                Text(
                    stringResource(if (session.captureTarget != null) R.string.instagram_modal_steps else session.publishStatus ?: R.string.instagram_producer_live),
                    style = MaterialTheme.typography.bodySmall,
                    modifier = Modifier.padding(horizontal = 16.dp, vertical = 8.dp),
                    color = BridgeTheme.colors.muted,
                )
                if (session.titleTooLong) Text(stringResource(R.string.broadcast_title_instagram_error),
                    style = MaterialTheme.typography.bodySmall, modifier = Modifier.padding(horizontal = 16.dp),
                    color = BridgeTheme.colors.dangerText)
                if (session.loading) LinearProgressIndicator(modifier = Modifier.fillMaxWidth())
                if (session.producerUnavailable) {
                    Text(stringResource(R.string.instagram_setup_unavailable), style = MaterialTheme.typography.bodySmall,
                        modifier = Modifier.padding(horizontal = 16.dp), color = BridgeTheme.colors.warningText)
                    TextButton(onClick = session::retryOpenProducer) { Text(stringResource(R.string.retry)) }
                }
                if (live) {
                    if (controlMissing) Text(
                        stringResource(R.string.instagram_control_missing),
                        style = MaterialTheme.typography.bodySmall,
                        modifier = Modifier.padding(horizontal = 16.dp),
                        color = BridgeTheme.colors.warningText,
                    )
                }
                if (session.pageError) {
                    Column(modifier = Modifier.padding(horizontal = 16.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                        Text(stringResource(R.string.instagram_error_page), color = BridgeTheme.colors.dangerText)
                        TextButton(onClick = session::reload) { Text(stringResource(R.string.retry)) }
                    }
                }
                AndroidView(
                    factory = { session.view(it) },
                    onRelease = { session.detach() },
                    modifier = Modifier.weight(1f).fillMaxWidth(),
                )

            }
        }
        LaunchedEffect(session, live, session.loading) {
            if (live && !session.loading) {
                delay(2_000)
                session.showLiveControl { controlMissing = !it }
            }
        }
    }
}

/** A measured, non-interactive host behind the opaque app UI; no Activity launch is needed. */
@Composable
internal fun InstagramProducerBackground(session: InstagramProducerSession) {
    if (session.visible || (!session.hasPage && !session.preparing)) return
    AndroidView(
        factory = { session.view(it, interactive = false) },
        onRelease = { session.detach() },
        modifier = Modifier.fillMaxSize(),
    )
}

/** One setup loop survives moving the retained page between the hidden host and login dialog. */
@Composable
internal fun InstagramProducerAutomation(
    session: InstagramProducerSession,
    onCaptured: (ProfileEditorState, InstagramCredentials) -> Unit,
) {
    val capture by androidx.compose.runtime.rememberUpdatedState(onCaptured)
    LaunchedEffect(session, session.preparing) {
        while (session.preparing) {
            session.fillTitle()
            session.advanceSetup()
            session.capture(capture)
            session.openProducer()
            delay(1_000)
        }
    }
}

@Composable
internal fun InstagramPreparingDialog(session: InstagramProducerSession, onCancel: () -> Unit) {
    if (!session.preparing || session.visible) return
    AlertDialog(
        onDismissRequest = onCancel,
        title = { Text(stringResource(R.string.instagram_preparing)) },
        text = { CircularProgressIndicator() },
        confirmButton = {},
        dismissButton = { TextButton(onClick = onCancel) { Text(stringResource(R.string.cancel)) } },
    )
}
