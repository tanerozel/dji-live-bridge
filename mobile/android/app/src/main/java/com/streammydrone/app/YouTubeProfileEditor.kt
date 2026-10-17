package com.streammydrone.app

import android.app.Activity
import androidx.activity.compose.BackHandler
import androidx.activity.compose.LocalActivity
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.IntentSenderRequest
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.WindowInsetsSides
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.only
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Close
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.RadioButton
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.dp
import com.google.android.gms.auth.api.identity.AuthorizationResult
import com.google.android.gms.auth.api.identity.Identity
import com.google.android.gms.tasks.Tasks
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.util.concurrent.TimeUnit

/** Kept by the activity ViewModel; no OAuth token is saved to a Bundle or to disk. */
internal class YouTubeEditorState(connection: YouTubeConnection?) {
    var email by mutableStateOf(connection?.accountEmail)
    var channels by mutableStateOf(connection?.let { listOf(YouTubeChannel(it.channelId, it.channelTitle)) }.orEmpty())
    var channelId by mutableStateOf(connection?.channelId)
    var title by mutableStateOf(connection?.title.orEmpty())
    var privacy by mutableStateOf(connection?.privacy ?: YouTubePrivacy.UNLISTED.apiValue)
    var madeForKids by mutableStateOf(connection?.madeForKids)
    var busy by mutableStateOf(false)
    var error by mutableStateOf<UiText?>(null)
    var touched by mutableStateOf(false)
    var confirmDisconnect by mutableStateOf(false)
    var confirmDiscard by mutableStateOf(false)
    var accessToken: String? = null

    fun clearSecret() { accessToken = null }
}

@Composable
internal fun YouTubeProfileEditor(
    state: ProfileEditorState,
    onDismiss: () -> Unit,
    onManual: () -> Unit,
    onSave: (YouTubeConnection, String, String) -> UiText?,
    onDelete: (() -> UiText?)?,
) {
    val context = LocalContext.current
    val activity = LocalActivity.current
    val scope = rememberCoroutineScope()
    val editor = state.youtube
    val original = state.profile?.youtube
    val colors = BridgeTheme.colors

    fun dismiss() {
        if (editor.busy) return
        if (editor.touched) editor.confirmDiscard = true else onDismiss()
    }

    fun authorized(result: AuthorizationResult) {
        scope.launch {
            try {
                val token = result.youTubeAccessToken()
                val api = YouTubeApi(GoogleYouTubeTransport(token))
                val (email, channels) = withContext(Dispatchers.IO) { api.accountEmail() to api.channels() }
                editor.accessToken = token
                editor.email = email
                editor.channels = channels
                editor.channelId = channels.firstOrNull { it.id == editor.channelId }?.id ?: channels.first().id
                editor.touched = true
            } catch (error: Exception) {
                editor.error = youTubeError(error)
            } finally {
                editor.busy = false
            }
        }
    }

    val authorization = rememberLauncherForActivityResult(ActivityResultContracts.StartIntentSenderForResult()) { result ->
        if (result.resultCode != Activity.RESULT_OK || result.data == null) {
            editor.busy = false
        } else {
            try {
                authorized(Identity.getAuthorizationClient(context).getAuthorizationResultFromIntent(result.data))
            } catch (error: Exception) {
                editor.error = youTubeError(error)
                editor.busy = false
            }
        }
    }

    fun connect() {
        if (activity == null || editor.busy) return
        editor.error = null
        editor.busy = true
        // Account selection and permission stay in Google's UI, never in a WebView.
        Identity.getAuthorizationClient(activity).authorize(youTubeAuthorizationRequest(chooseAccount = true))
            .addOnSuccessListener { result ->
                if (result.hasResolution()) {
                    try {
                        authorization.launch(IntentSenderRequest.Builder(result.pendingIntent!!.intentSender).build())
                    } catch (error: Exception) {
                        editor.error = youTubeError(error)
                        editor.busy = false
                    }
                } else {
                    authorized(result)
                }
            }
            .addOnFailureListener { error ->
                editor.error = youTubeError(error)
                editor.busy = false
            }
    }

    fun save() {
        val channel = editor.channels.firstOrNull { it.id == editor.channelId } ?: return
        val email = editor.email ?: return
        val kids = editor.madeForKids ?: return
        val title = try { validateYouTubeTitle(editor.title) } catch (error: YouTubeException) {
            editor.error = youTubeError(error)
            return
        }
        editor.busy = true
        editor.error = null
        scope.launch {
            var created: YouTubeStream? = null
            var saved = false
            var api: YouTubeApi? = null
            try {
                val reuse = original?.takeIf { it.accountEmail == email && it.channelId == channel.id }
                if (reuse == null) {
                    val token = editor.accessToken ?: throw YouTubeException(YouTubeFailure.AUTH)
                    api = YouTubeApi(GoogleYouTubeTransport(token))
                    // Record the result inside NonCancellable so an activity disappearing cannot
                    // lose the ID between the HTTP response and compensation in finally.
                    withContext(Dispatchers.IO + NonCancellable) { created = api.createStream() }
                }
                val connection = YouTubeConnection(email, channel.id, channel.title, reuse?.streamId ?: created!!.id,
                    title, editor.privacy, kids)
                editor.error = onSave(connection, created?.serverUrl ?: state.profile!!.serverUrl, created?.key.orEmpty())
                saved = editor.error == null
            } catch (error: Exception) {
                if (error is YouTubeException && error.failure == YouTubeFailure.AUTH) {
                    editor.accessToken?.let { clearYouTubeToken(context, it) }
                    editor.clearSecret()
                }
                editor.error = youTubeError(error)
            } finally {
                if (!saved) created?.let { stream ->
                    withContext(Dispatchers.IO + NonCancellable) { runCatching { api?.deleteStream(stream.id) } }
                }
                editor.busy = false
            }
        }
    }

    SecureWindowEffect()
    BackHandler(onBack = ::dismiss)
    Scaffold(
        containerColor = colors.background,
        contentWindowInsets = WindowInsets.safeDrawing,
        topBar = {
            Row(Modifier.fillMaxWidth().windowInsetsPadding(WindowInsets.safeDrawing.only(WindowInsetsSides.Top + WindowInsetsSides.Horizontal))) {
                IconButton(onClick = ::dismiss, enabled = !editor.busy) {
                    Icon(Icons.Rounded.Close, stringResource(R.string.close), tint = colors.muted)
                }
            }
        },
    ) { padding ->
        Column(
            modifier = Modifier.fillMaxSize().padding(padding).verticalScroll(rememberScrollState()).padding(20.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(14.dp),
        ) {
            PlatformTile(DestinationKind.YOUTUBE, size = 64.dp)
            Text("YouTube", style = MaterialTheme.typography.headlineSmall, color = colors.text)
            Text(stringResource(R.string.youtube_connection_help), style = MaterialTheme.typography.bodyMedium, color = colors.muted)
            PrimaryButton(text = stringResource(R.string.youtube_connect), onClick = ::connect, enabled = !editor.busy)
            editor.email?.let { Text(it, color = colors.muted, style = MaterialTheme.typography.bodySmall) }
            if (editor.channels.isNotEmpty()) {
                Text(stringResource(R.string.youtube_channel), color = colors.text)
                Column(Modifier.fillMaxWidth().selectableGroup()) {
                    editor.channels.forEach { channel ->
                        YouTubeChoice(channel.title, editor.channelId == channel.id, !editor.busy) {
                            editor.channelId = channel.id; editor.touched = true
                        }
                    }
                }
                OutlinedTextField(
                    value = editor.title,
                    onValueChange = { editor.title = it; editor.touched = true },
                    modifier = Modifier.fillMaxWidth(),
                    label = { Text(stringResource(R.string.youtube_title)) },
                    singleLine = true,
                    enabled = !editor.busy,
                )
                Text(stringResource(R.string.youtube_privacy), color = colors.text)
                Column(Modifier.fillMaxWidth().selectableGroup()) {
                    YouTubePrivacy.entries.forEach { privacy ->
                        val label = when (privacy) {
                            YouTubePrivacy.PRIVATE -> R.string.youtube_private
                            YouTubePrivacy.UNLISTED -> R.string.youtube_unlisted
                            YouTubePrivacy.PUBLIC -> R.string.youtube_public
                        }
                        YouTubeChoice(stringResource(label), editor.privacy == privacy.apiValue, !editor.busy) {
                            editor.privacy = privacy.apiValue; editor.touched = true
                        }
                    }
                }
                Text(stringResource(R.string.youtube_audience), color = colors.text)
                Column(Modifier.fillMaxWidth().selectableGroup()) {
                    YouTubeChoice(stringResource(R.string.youtube_not_kids), editor.madeForKids == false, !editor.busy) {
                        editor.madeForKids = false; editor.touched = true
                    }
                    YouTubeChoice(stringResource(R.string.youtube_kids), editor.madeForKids == true, !editor.busy) {
                        editor.madeForKids = true; editor.touched = true
                    }
                }
                PrimaryButton(text = stringResource(R.string.save), onClick = ::save,
                    enabled = !editor.busy && editor.title.isNotBlank() && editor.madeForKids != null)
            }
            if (editor.busy) CircularProgressIndicator()
            editor.error?.let { Text(it.asString(), color = MaterialTheme.colorScheme.error) }
            if (original == null) {
                TextButton(onClick = { editor.clearSecret(); onManual() }, enabled = !editor.busy) {
                    Text(stringResource(R.string.youtube_manual))
                }
            }
            if (original != null && onDelete != null) {
                TextButton(onClick = { editor.confirmDisconnect = true }, enabled = !editor.busy) {
                    Text(stringResource(R.string.youtube_disconnect), color = MaterialTheme.colorScheme.error)
                }
            }
        }
    }
    if (editor.confirmDiscard) {
        AlertDialog(onDismissRequest = { editor.confirmDiscard = false },
            title = { Text(stringResource(R.string.discard_title)) },
            text = { Text(stringResource(R.string.discard_message)) },
            confirmButton = { TextButton(onClick = onDismiss) { Text(stringResource(R.string.close)) } },
            dismissButton = { TextButton(onClick = { editor.confirmDiscard = false }) { Text(stringResource(R.string.cancel)) } })
    }
    if (editor.confirmDisconnect && original != null && onDelete != null) {
        AlertDialog(onDismissRequest = { editor.confirmDisconnect = false },
            title = { Text(stringResource(R.string.youtube_disconnect)) },
            text = { Text(stringResource(R.string.youtube_disconnect_message)) },
            confirmButton = {
                TextButton(onClick = {
                    editor.confirmDisconnect = false
                    editor.busy = true
                    scope.launch {
                        try {
                            withContext(Dispatchers.IO) {
                                finishStoredYouTubeSession(context, state.profile.id)
                                Tasks.await(revokeYouTubeAccess(context, original.accountEmail), 30, TimeUnit.SECONDS)
                            }
                            editor.error = onDelete()
                        } catch (error: Exception) {
                            editor.error = youTubeError(error)
                        } finally { editor.busy = false }
                    }
                }) { Text(stringResource(R.string.youtube_disconnect)) }
            },
            dismissButton = { TextButton(onClick = { editor.confirmDisconnect = false }) { Text(stringResource(R.string.cancel)) } })
    }
}

@Composable
private fun YouTubeChoice(label: String, selected: Boolean, enabled: Boolean, onSelect: () -> Unit) {
    Row(Modifier.fillMaxWidth().selectable(selected, enabled = enabled, role = Role.RadioButton, onClick = onSelect).padding(vertical = 4.dp),
        verticalAlignment = Alignment.CenterVertically) {
        RadioButton(selected, onClick = null, enabled = enabled)
        Text(label, modifier = Modifier.padding(start = 10.dp), color = BridgeTheme.colors.text)
    }
}
