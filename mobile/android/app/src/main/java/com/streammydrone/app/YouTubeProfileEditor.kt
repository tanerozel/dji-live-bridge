package com.streammydrone.app

import android.app.Activity
import androidx.activity.compose.BackHandler
import androidx.activity.compose.LocalActivity
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.IntentSenderRequest
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.WindowInsetsSides
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.only
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Close
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
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
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
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
    /** Only a fallback for the first broadcast's title: Go live asks for the title every time. */
    val title = connection?.title ?: "Stream My Drone"
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
    val connected = editor.email != null && editor.channels.isNotEmpty()
    val channel = editor.channels.firstOrNull { it.id == editor.channelId }
    Scaffold(
        containerColor = colors.background,
        contentWindowInsets = WindowInsets.safeDrawing,
        topBar = {
            Column(Modifier.fillMaxWidth().windowInsetsPadding(WindowInsets.safeDrawing.only(WindowInsetsSides.Top + WindowInsetsSides.Horizontal))) {
                Row(Modifier.heightIn(min = 56.dp).padding(horizontal = 4.dp), verticalAlignment = Alignment.CenterVertically) {
                    IconButton(onClick = ::dismiss, enabled = !editor.busy) {
                        Icon(Icons.Rounded.Close, stringResource(R.string.close), tint = colors.muted)
                    }
                }
                // Talking to Google can take a moment; the bar says the tap was taken.
                if (editor.busy) LinearProgressIndicator(Modifier.fillMaxWidth(), color = colors.accent, trackColor = colors.field)
            }
        },
        bottomBar = {
            Box(
                modifier = Modifier
                    .fillMaxWidth()
                    .windowInsetsPadding(WindowInsets.safeDrawing.only(WindowInsetsSides.Bottom + WindowInsetsSides.Horizontal))
                    .padding(horizontal = 20.dp, vertical = 12.dp),
                contentAlignment = Alignment.Center,
            ) {
                if (connected) {
                    PrimaryButton(
                        modifier = Modifier.widthIn(max = 560.dp),
                        text = stringResource(R.string.save),
                        onClick = ::save,
                        enabled = !editor.busy && editor.madeForKids != null,
                    )
                } else {
                    PrimaryButton(
                        modifier = Modifier.widthIn(max = 560.dp),
                        text = stringResource(R.string.youtube_connect),
                        onClick = ::connect,
                        enabled = !editor.busy,
                    )
                }
            }
        },
    ) { padding ->
        Box(Modifier.fillMaxSize().padding(padding), contentAlignment = Alignment.TopCenter) {
            Column(
                modifier = Modifier
                    .widthIn(max = 560.dp)
                    .fillMaxSize()
                    .verticalScroll(rememberScrollState())
                    .padding(horizontal = 20.dp, vertical = 8.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.spacedBy(16.dp),
            ) {
                PlatformTile(DestinationKind.YOUTUBE, size = 64.dp)
                Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(6.dp)) {
                    Text("YouTube", style = MaterialTheme.typography.headlineSmall, color = colors.text,
                        modifier = Modifier.semantics { heading() })
                    Text(stringResource(R.string.youtube_connection_help), style = MaterialTheme.typography.bodyMedium,
                        color = colors.muted, textAlign = TextAlign.Center)
                }
                editor.email?.let { email ->
                    AccountCard(
                        email = email,
                        channel = channel?.title?.takeIf { editor.channels.size == 1 },
                        enabled = !editor.busy,
                        onChange = ::connect,
                    )
                }
                if (editor.email == null) {
                    BridgeCard(verticalSpacing = 14.dp) {
                        SectionHeader(title = stringResource(R.string.how_it_works))
                        NumberedSteps(
                            listOf(
                                stringResource(R.string.youtube_step_account),
                                stringResource(R.string.youtube_step_prepare),
                                stringResource(R.string.youtube_auto_hint),
                            ),
                        )
                    }
                }
                if (connected) {
                    if (editor.channels.size > 1) {
                        SettingsSection(stringResource(R.string.youtube_channel)) {
                            Column(Modifier.selectableGroup(), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                                editor.channels.forEach { option ->
                                    ChoiceRow(option.title, null, editor.channelId == option.id, !editor.busy) {
                                        editor.channelId = option.id; editor.touched = true
                                    }
                                }
                            }
                        }
                    }
                    SettingsSection(stringResource(R.string.youtube_privacy)) {
                        Column(Modifier.selectableGroup(), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                            YouTubePrivacy.entries.forEach { privacy ->
                                val (label, hint) = when (privacy) {
                                    YouTubePrivacy.PRIVATE -> R.string.youtube_private to R.string.youtube_private_hint
                                    YouTubePrivacy.UNLISTED -> R.string.youtube_unlisted to R.string.youtube_unlisted_hint
                                    YouTubePrivacy.PUBLIC -> R.string.youtube_public to R.string.youtube_public_hint
                                }
                                ChoiceRow(stringResource(label), stringResource(hint), editor.privacy == privacy.apiValue, !editor.busy) {
                                    editor.privacy = privacy.apiValue; editor.touched = true
                                }
                            }
                        }
                    }
                    SettingsSection(stringResource(R.string.youtube_audience)) {
                        Column(Modifier.selectableGroup(), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                            ChoiceRow(stringResource(R.string.youtube_not_kids), null, editor.madeForKids == false, !editor.busy) {
                                editor.madeForKids = false; editor.touched = true
                            }
                            ChoiceRow(stringResource(R.string.youtube_kids), null, editor.madeForKids == true, !editor.busy) {
                                editor.madeForKids = true; editor.touched = true
                            }
                        }
                    }
                }
                editor.error?.let { AlertBanner(title = it.asString()) }
                if (original == null) {
                    TextButton(onClick = { editor.clearSecret(); onManual() }, enabled = !editor.busy) {
                        Text(stringResource(R.string.youtube_manual))
                    }
                }
                if (original != null && onDelete != null) {
                    TextButton(
                        onClick = { editor.confirmDisconnect = true },
                        enabled = !editor.busy,
                        colors = ButtonDefaults.textButtonColors(contentColor = colors.dangerText),
                    ) {
                        Text(stringResource(R.string.youtube_disconnect))
                    }
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

/** The account in use (a Google e-mail or a Facebook name), with a way to pick another. */
@Composable
internal fun AccountCard(email: String, channel: String?, enabled: Boolean, onChange: () -> Unit) {
    val colors = BridgeTheme.colors
    BridgeCard(contentPadding = PaddingValues(start = 16.dp, end = 8.dp, top = 12.dp, bottom = 12.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            Box(
                modifier = Modifier.size(40.dp).background(colors.accentSoft, CircleShape),
                contentAlignment = Alignment.Center,
            ) {
                Text(
                    text = email.firstOrNull()?.uppercase().orEmpty(),
                    style = MaterialTheme.typography.titleMedium,
                    color = colors.onAccentSoft,
                )
            }
            Column(modifier = Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                channel?.let { Text(it, style = MaterialTheme.typography.titleSmall, color = colors.text, maxLines = 1, overflow = TextOverflow.Ellipsis) }
                Text(email, style = MaterialTheme.typography.bodySmall, color = colors.muted, maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
            TextButton(onClick = onChange, enabled = enabled) { Text(stringResource(R.string.change)) }
        }
    }
}

/** A heading over one group of settings. */
@Composable
internal fun SettingsSection(title: String, content: @Composable () -> Unit) {
    Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Text(
            text = title,
            style = MaterialTheme.typography.labelLarge,
            color = BridgeTheme.colors.muted,
            modifier = Modifier.semantics { heading() },
        )
        content()
    }
}
