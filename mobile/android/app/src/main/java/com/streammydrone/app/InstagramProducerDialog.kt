package com.streammydrone.app

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Close
import androidx.compose.material.icons.rounded.Lock
import androidx.compose.material.icons.rounded.MoreVert
import androidx.compose.material.icons.rounded.WarningAmber
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
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextDirection
import androidx.compose.ui.text.style.TextOverflow
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
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(10.dp),
                    modifier = Modifier.fillMaxWidth().padding(horizontal = 4.dp, vertical = 4.dp),
                ) {
                    IconButton(onClick = onDismiss) { Icon(Icons.Rounded.Close, stringResource(R.string.close), tint = BridgeTheme.colors.muted) }
                    PlatformTile(kind = DestinationKind.INSTAGRAM, size = 28.dp)
                    Column(modifier = Modifier.weight(1f)) {
                        Text("Instagram", style = MaterialTheme.typography.titleSmall, maxLines = 1)
                        // The page's address, so it is clear this is Instagram's own site.
                        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                            Icon(Icons.Rounded.Lock, contentDescription = null, tint = BridgeTheme.colors.muted, modifier = Modifier.size(12.dp))
                            Text(
                                text = session.host,
                                style = MaterialTheme.typography.labelSmall.copy(textDirection = TextDirection.Ltr),
                                color = BridgeTheme.colors.muted,
                                maxLines = 1,
                                overflow = TextOverflow.Ellipsis,
                            )
                        }
                    }
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
                Column(
                    modifier = Modifier.padding(start = 12.dp, end = 12.dp, bottom = 8.dp),
                    verticalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    TipBox(
                        stringResource(
                            if (session.captureTarget != null) R.string.instagram_modal_steps else session.publishStatus ?: R.string.instagram_producer_live,
                        ),
                    )
                    if (session.titleTooLong) AlertBanner(title = stringResource(R.string.broadcast_title_instagram_error))
                    if (session.producerUnavailable) {
                        ProducerNotice(
                            text = stringResource(R.string.instagram_setup_unavailable),
                            color = BridgeTheme.colors.warningText,
                            background = BridgeTheme.colors.warningSoft,
                            onRetry = session::retryOpenProducer,
                        )
                    }
                    if (live && controlMissing) {
                        ProducerNotice(
                            text = stringResource(R.string.instagram_control_missing),
                            color = BridgeTheme.colors.warningText,
                            background = BridgeTheme.colors.warningSoft,
                            onRetry = null,
                        )
                    }
                    if (session.pageError) {
                        ProducerNotice(
                            text = stringResource(R.string.instagram_error_page),
                            color = BridgeTheme.colors.dangerText,
                            background = BridgeTheme.colors.dangerSoft,
                            onRetry = session::reload,
                        )
                    }
                }
                if (session.loading) LinearProgressIndicator(modifier = Modifier.fillMaxWidth(), color = BridgeTheme.colors.accent)
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

/** A problem in the Instagram window, with a way to try again where there is one. */
@Composable
private fun ProducerNotice(text: String, color: Color, background: Color, onRetry: (() -> Unit)?) {
    Surface(
        modifier = Modifier.fillMaxWidth(),
        shape = MaterialTheme.shapes.small,
        color = background,
        contentColor = color,
    ) {
        Row(
            modifier = Modifier.padding(start = 12.dp, top = 4.dp, bottom = 4.dp, end = 4.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            Icon(Icons.Rounded.WarningAmber, contentDescription = null, modifier = Modifier.size(18.dp))
            Text(text, style = MaterialTheme.typography.bodySmall, color = BridgeTheme.colors.text, modifier = Modifier.weight(1f).padding(vertical = 8.dp))
            onRetry?.let { TextButton(onClick = it) { Text(stringResource(R.string.retry), color = color) } }
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

/** While the app gets Instagram ready by itself; Instagram's own window opens only if it needs the user. */
@Composable
internal fun InstagramPreparingDialog(session: InstagramProducerSession, onCancel: () -> Unit) {
    if (!session.preparing || session.visible) return
    LiveDialog(onDismiss = onCancel) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(start = 24.dp, end = 24.dp, top = 28.dp, bottom = 12.dp)
                .semantics(mergeDescendants = true) { liveRegion = LiveRegionMode.Polite },
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Box(contentAlignment = Alignment.Center) {
                CircularProgressIndicator(
                    modifier = Modifier.size(84.dp),
                    color = GoLiveStart,
                    trackColor = Color.White.copy(alpha = 0.08f),
                    strokeWidth = 3.dp,
                )
                PlatformTile(kind = DestinationKind.INSTAGRAM, size = 56.dp)
            }
            Spacer(Modifier.height(4.dp))
            Text(
                text = stringResource(R.string.instagram_preparing),
                style = MaterialTheme.typography.titleLarge,
                color = Color.White,
                textAlign = TextAlign.Center,
            )
            Text(
                text = stringResource(R.string.instagram_preparing_hint),
                style = MaterialTheme.typography.bodyMedium,
                color = Color.White.copy(alpha = 0.7f),
                textAlign = TextAlign.Center,
            )
        }
        Box(modifier = Modifier.padding(start = 12.dp, end = 12.dp, bottom = 12.dp)) {
            LiveTextButton(text = stringResource(R.string.cancel), onClick = onCancel)
        }
    }
}
