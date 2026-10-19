package com.streammydrone.app

import android.content.ClipboardManager
import android.content.Context
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.rounded.OpenInNew
import androidx.compose.material3.Icon
import androidx.compose.material3.OutlinedButton
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
    BridgeCard(verticalSpacing = 14.dp) {
        SectionHeader(title = stringResource(R.string.instagram_phone_setup))
        NumberedSteps(numberedLines(stringResource(R.string.instagram_phone_steps)))
        OutlinedButton(
            onClick = onOpenBrowser,
            modifier = Modifier.fillMaxWidth().heightIn(min = 48.dp),
            shape = MaterialTheme.shapes.medium,
        ) {
            Icon(Icons.AutoMirrored.Rounded.OpenInNew, contentDescription = null, modifier = Modifier.size(18.dp))
            Spacer(Modifier.width(8.dp))
            Text(stringResource(R.string.instagram_open_browser))
        }
    }
}

/** What happens when Instagram is connected, so nothing about it is a surprise. */
@Composable
internal fun InstagramAutomaticSteps() {
    BridgeCard(verticalSpacing = 14.dp) {
        SectionHeader(title = stringResource(R.string.how_it_works))
        NumberedSteps(
            listOf(
                stringResource(R.string.instagram_step_signin),
                stringResource(R.string.instagram_step_prepare),
                stringResource(R.string.instagram_modal_help),
            ),
        )
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
