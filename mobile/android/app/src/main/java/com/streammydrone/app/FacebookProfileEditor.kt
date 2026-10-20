package com.streammydrone.app

import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.WindowInsetsSides
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.only
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Close
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import com.facebook.AccessToken
import com.facebook.CallbackManager
import com.facebook.FacebookCallback
import com.facebook.FacebookException
import com.facebook.login.LoginManager
import com.facebook.login.LoginResult
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/** Kept by the activity ViewModel; the access token stays in the Facebook SDK's own cache. */
internal class FacebookEditorState(connection: FacebookConnection?) {
    var user by mutableStateOf(connection?.let { FacebookUser(it.userId, it.userName) })
    var pages by mutableStateOf(connection?.takeIf { it.page }?.let { listOf(FacebookPage(it.targetId, it.targetName)) }.orEmpty())
    var targetId by mutableStateOf(connection?.targetId)
    var title by mutableStateOf(connection?.title.orEmpty())
    /** Not chosen yet for a new profile connection: the user decides who can watch. */
    var privacy by mutableStateOf(connection?.takeIf { !it.page }?.privacy)
    var busy by mutableStateOf(false)
    var error by mutableStateOf<UiText?>(null)
    var touched by mutableStateOf(false)
    var confirmDisconnect by mutableStateOf(false)
    var confirmDiscard by mutableStateOf(false)
    var pagesRefreshed = false
}

@Composable
internal fun FacebookProfileEditor(
    state: ProfileEditorState,
    onDismiss: () -> Unit,
    onManual: () -> Unit,
    onSave: (FacebookConnection) -> UiText?,
    onDelete: (() -> UiText?)?,
) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val editor = state.facebook
    val original = state.profile?.facebook
    val colors = BridgeTheme.colors

    fun dismiss() {
        if (editor.busy) return
        if (editor.touched) editor.confirmDiscard = true else onDismiss()
    }

    /** Reads who logged in and which Pages they may publish to. */
    fun loaded(token: AccessToken, userChosen: Boolean) {
        editor.busy = true
        scope.launch {
            try {
                val api = FacebookApi(GraphFacebookTransport())
                val (user, pages) = withContext(Dispatchers.IO) {
                    api.me(token.token) to if (facebookTokenAllows(token, page = true)) api.pages(token.token) else emptyList()
                }
                if (!userChosen && user.id != editor.user?.id) return@launch
                editor.user = user
                editor.pages = pages
                editor.targetId = editor.targetId?.takeIf { id -> id == user.id || pages.any { it.id == id } } ?: user.id
                if (userChosen) editor.touched = true
            } catch (error: Exception) {
                editor.error = facebookError(error)
            } finally {
                editor.busy = false
            }
        }
    }

    val login = rememberFacebookLogin(
        onToken = { loaded(it, userChosen = true) },
        onEnd = { error -> editor.error = error; editor.busy = false },
    )

    // An existing connection lists its Pages again when Facebook still has the same user signed in.
    LaunchedEffect(editor) {
        if (editor.pagesRefreshed || original == null || !facebookConfigured) return@LaunchedEffect
        editor.pagesRefreshed = true
        runCatching { ensureFacebookSdk(context) }
        AccessToken.getCurrentAccessToken()?.takeIf { !it.isExpired && it.userId == original.userId }?.let { loaded(it, userChosen = false) }
    }

    fun connect() {
        if (editor.busy) return
        editor.error = null
        if (login == null) {
            editor.error = uiText(R.string.facebook_error_setup)
            return
        }
        editor.busy = true
        login()
    }

    fun save() {
        val user = editor.user ?: return
        val page = editor.pages.firstOrNull { it.id == editor.targetId }
        if (page == null && editor.targetId != user.id) return
        val privacy = if (page != null) FacebookPrivacy.EVERYONE.apiValue else editor.privacy ?: return
        val title = try { validateBroadcastTitle(editor.title) } catch (error: Exception) {
            editor.error = facebookError(error)
            return
        }
        // Facebook must have granted what this target needs; a declined permission asks again.
        val token = runCatching { ensureFacebookSdk(context); AccessToken.getCurrentAccessToken() }.getOrNull()
        if (token == null || token.userId != user.id || !facebookTokenAllows(token, page != null)) {
            editor.error = uiText(R.string.facebook_error_auth)
            return
        }
        editor.error = onSave(FacebookConnection(user.id, user.name, page?.id ?: user.id, page?.name ?: user.name, page != null, title, privacy))
    }

    SecureWindowEffect()
    BackHandler(onBack = ::dismiss)
    val user = editor.user
    val onPage = user != null && editor.targetId != user.id
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
                // Talking to Facebook can take a moment; the bar says the tap was taken.
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
                if (user != null) {
                    PrimaryButton(
                        modifier = Modifier.widthIn(max = 560.dp),
                        text = stringResource(R.string.save),
                        onClick = ::save,
                        enabled = !editor.busy && editor.title.isNotBlank() && (onPage || editor.privacy != null),
                    )
                } else {
                    PrimaryButton(
                        modifier = Modifier.widthIn(max = 560.dp),
                        text = stringResource(R.string.facebook_connect),
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
                PlatformTile(DestinationKind.FACEBOOK, size = 64.dp)
                Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(6.dp)) {
                    Text("Facebook", style = MaterialTheme.typography.headlineSmall, color = colors.text,
                        modifier = Modifier.semantics { heading() })
                    Text(stringResource(R.string.facebook_connection_help), style = MaterialTheme.typography.bodyMedium,
                        color = colors.muted, textAlign = TextAlign.Center)
                }
                if (user != null) {
                    AccountCard(email = user.name, channel = null, enabled = !editor.busy, onChange = {
                        // Another account: forget this one first, or Facebook offers it again.
                        runCatching { LoginManager.getInstance().logOut() }
                        connect()
                    })
                    SettingsSection(stringResource(R.string.facebook_target)) {
                        Column(Modifier.selectableGroup(), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                            ChoiceRow(user.name, stringResource(R.string.facebook_target_profile), editor.targetId == user.id, !editor.busy) {
                                editor.targetId = user.id; editor.touched = true
                            }
                            editor.pages.forEach { page ->
                                ChoiceRow(page.name, stringResource(R.string.facebook_target_page), editor.targetId == page.id, !editor.busy) {
                                    editor.targetId = page.id; editor.touched = true
                                }
                            }
                        }
                    }
                    SettingsSection(stringResource(R.string.youtube_title)) {
                        OutlinedTextField(
                            value = editor.title,
                            onValueChange = { editor.title = it; editor.touched = true },
                            modifier = Modifier.fillMaxWidth(),
                            placeholder = { Text(stringResource(R.string.youtube_title)) },
                            singleLine = true,
                            enabled = !editor.busy,
                            shape = MaterialTheme.shapes.small,
                        )
                    }
                    SettingsSection(stringResource(R.string.facebook_privacy)) {
                        if (onPage) {
                            Text(stringResource(R.string.facebook_page_public_hint), style = MaterialTheme.typography.bodyMedium, color = colors.muted)
                        } else {
                            Column(Modifier.selectableGroup(), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                                FacebookPrivacy.entries.forEach { privacy ->
                                    val (label, hint) = when (privacy) {
                                        FacebookPrivacy.SELF -> R.string.facebook_only_me to R.string.facebook_only_me_hint
                                        FacebookPrivacy.ALL_FRIENDS -> R.string.facebook_friends to R.string.facebook_friends_hint
                                        FacebookPrivacy.EVERYONE -> R.string.facebook_public to R.string.facebook_public_hint
                                    }
                                    ChoiceRow(stringResource(label), stringResource(hint), editor.privacy == privacy.apiValue, !editor.busy) {
                                        editor.privacy = privacy.apiValue; editor.touched = true
                                    }
                                }
                            }
                        }
                    }
                } else {
                    BridgeCard(verticalSpacing = 14.dp) {
                        SectionHeader(title = stringResource(R.string.how_it_works))
                        NumberedSteps(
                            listOf(
                                stringResource(R.string.facebook_step_account),
                                stringResource(R.string.facebook_step_prepare),
                                stringResource(R.string.facebook_auto_hint),
                            ),
                        )
                    }
                }
                editor.error?.let { AlertBanner(title = it.asString()) }
                if (original == null) {
                    TextButton(onClick = onManual, enabled = !editor.busy) {
                        Text(stringResource(R.string.youtube_manual))
                    }
                }
                if (original != null && onDelete != null) {
                    TextButton(
                        onClick = { editor.confirmDisconnect = true },
                        enabled = !editor.busy,
                        colors = ButtonDefaults.textButtonColors(contentColor = colors.dangerText),
                    ) {
                        Text(stringResource(R.string.facebook_disconnect))
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
            title = { Text(stringResource(R.string.facebook_disconnect)) },
            text = { Text(stringResource(R.string.facebook_disconnect_message)) },
            confirmButton = {
                TextButton(onClick = {
                    editor.confirmDisconnect = false
                    editor.busy = true
                    scope.launch {
                        try {
                            withContext(Dispatchers.IO) {
                                finishStoredFacebookSession(context, state.profile.id)
                                revokeFacebookAccess(context)
                            }
                            editor.error = onDelete()
                        } catch (error: Exception) {
                            editor.error = facebookError(error)
                        } finally { editor.busy = false }
                    }
                }) { Text(stringResource(R.string.facebook_disconnect)) }
            },
            dismissButton = { TextButton(onClick = { editor.confirmDisconnect = false }) { Text(stringResource(R.string.cancel)) } })
    }
}

/**
 * Facebook's own login (its app, or a Custom Tab), never a WebView. Null when this build has no
 * Facebook App ID; the SDK is not touched then.
 */
@Composable
private fun rememberFacebookLogin(onToken: (AccessToken) -> Unit, onEnd: (UiText?) -> Unit): (() -> Unit)? {
    if (!facebookConfigured) return null
    val context = LocalContext.current
    val manager = remember {
        runCatching { ensureFacebookSdk(context); LoginManager.getInstance() }.getOrNull()
    } ?: return null
    val callbacks = remember { CallbackManager.Factory.create() }
    val currentOnToken by rememberUpdatedState(onToken)
    val currentOnEnd by rememberUpdatedState(onEnd)
    DisposableEffect(manager, callbacks) {
        manager.registerCallback(callbacks, object : FacebookCallback<LoginResult> {
            override fun onSuccess(result: LoginResult) = currentOnToken(result.accessToken)
            override fun onCancel() = currentOnEnd(null)
            override fun onError(error: FacebookException) = currentOnEnd(uiText(R.string.facebook_error_auth))
        })
        onDispose { manager.unregisterCallback(callbacks) }
    }
    val contract = remember(manager, callbacks) { manager.createLogInActivityResultContract(callbacks) }
    val launcher = rememberLauncherForActivityResult(contract) { }
    return { launcher.launch(FACEBOOK_PERMISSIONS) }
}
