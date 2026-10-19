package com.streammydrone.app

import android.content.ClipboardManager
import android.content.Context
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp

/** Explicit clicks only: no clipboard listener or automatic paste on resume. */
internal fun pasteInstagram(context: Context, state: ProfileEditorState, server: Boolean) {
    val clipboard = context.getSystemService(ClipboardManager::class.java)
    try {
        // Read plain text only. Do not resolve URI clips, intents, or content providers.
        val clip = clipboard.primaryClip
        val text = if (clip?.itemCount == 1) clip.getItemAt(0).text?.toString() else null
        if (text.isNullOrBlank() || text.length > 2048) throw InstagramSetupException(R.string.instagram_error_clipboard)
        if (server) {
            state.serverUrl = validateInstagramServer(text)
            state.serverExpanded = true
            state.serverTouched = true
        } else {
            state.streamKey = validateInstagramKey(text)
            state.keyTouched = true
            state.keyVisible = false
            // Clear only the exact plain-text key the user just imported, never an unrelated clip.
            if (clipboard.primaryClip?.let { it.itemCount == 1 && it.getItemAt(0).text?.toString() == text } == true) {
                clipboard.clearPrimaryClip()
            }
        }
        state.editorError = null
    } catch (error: InstagramSetupException) {
        state.editorError = uiText(error.messageId)
    } catch (_: SecurityException) {
        state.editorError = uiText(R.string.instagram_error_clipboard)
    }
}

@Composable
internal fun InstagramSetupGuide(onOpenBrowser: () -> Unit) {
    Column(modifier = Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Text(stringResource(R.string.instagram_phone_setup), style = MaterialTheme.typography.titleSmall)
        Text(stringResource(R.string.instagram_phone_steps), style = MaterialTheme.typography.bodySmall, color = BridgeTheme.colors.muted)
        TextButton(onClick = onOpenBrowser) { Text(stringResource(R.string.instagram_open_browser)) }
    }
}

/** Shown while forwarding as well: Instagram still needs its own preview/go-live/end actions. */
@Composable
internal fun InstagramProducerActions(onOpenBrowser: () -> Unit) {
    Column(modifier = Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Text(stringResource(R.string.instagram_producer_live), style = MaterialTheme.typography.bodySmall, color = BridgeTheme.colors.muted)
        TextButton(onClick = onOpenBrowser) { Text(stringResource(R.string.instagram_open_producer)) }
    }
}
