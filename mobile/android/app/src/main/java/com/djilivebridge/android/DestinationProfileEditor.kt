package com.djilivebridge.android

import android.view.WindowManager
import androidx.activity.compose.BackHandler
import androidx.activity.compose.LocalActivity
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.WindowInsetsSides
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.only
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Close
import androidx.compose.material.icons.rounded.DeleteOutline
import androidx.compose.material.icons.rounded.Visibility
import androidx.compose.material.icons.rounded.VisibilityOff
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.semantics.error
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.DialogProperties
import androidx.compose.ui.window.SecureFlagPolicy

/**
 * The stream key screen for one platform. The window is FLAG_SECURE while it is open so the key
 * cannot end up in screenshots, screen recordings or the recents thumbnail.
 */
@Composable
internal fun DestinationProfileEditor(
    state: ProfileEditorState,
    onDismiss: () -> Unit,
    onSave: (serverUrl: String, streamKey: String) -> String?,
    onDelete: (() -> String?)?,
) {
    val colors = BridgeTheme.colors
    val focusManager = LocalFocusManager.current
    val keyFocus = remember { FocusRequester() }
    val serverFocus = remember { FocusRequester() }
    val profile = state.profile
    val kind = state.kind

    var serverUrl by state::serverUrl
    var streamKey by state::streamKey
    var keyVisible by state::keyVisible
    var serverExpanded by state::serverExpanded
    var serverTouched by state::serverTouched
    var keyTouched by state::keyTouched
    var editorError by state::editorError
    var showDiscardConfirmation by state::showDiscardConfirmation
    var showDeleteConfirmation by state::showDeleteConfirmation

    val serverError = validationMessage { validateServerUrl(serverUrl) }
    val keyError = when {
        streamKey.isEmpty() && profile != null -> null
        streamKey.isEmpty() -> "Yayın anahtarını yapıştır"
        else -> validationMessage { validateStreamKey(streamKey) }
    }
    val hasChanges = streamKey.isNotEmpty() || !sameServerUrl(serverUrl, state.initialServerUrl)

    fun requestDismiss() {
        focusManager.clearFocus()
        if (hasChanges) {
            showDiscardConfirmation = true
        } else {
            state.clearSecret()
            onDismiss()
        }
    }

    fun save() {
        focusManager.clearFocus()
        serverTouched = true
        keyTouched = true
        if (serverError != null) serverExpanded = true
        if (serverError != null || keyError != null) return
        editorError = onSave(serverUrl, streamKey)
        if (editorError == null) state.clearSecret()
    }

    SecureWindowEffect()
    BackHandler(onBack = ::requestDismiss)
    // Most visits are only for a key, so the keyboard opens straight on it. Platforms without a
    // published address start on the address instead; their field is always shown.
    LaunchedEffect(state) {
        if (kind.defaultServerUrl == null && serverUrl.isBlank()) serverFocus.requestFocus() else keyFocus.requestFocus()
    }

    Scaffold(
        containerColor = colors.background,
        contentWindowInsets = WindowInsets.safeDrawing,
        topBar = {
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .windowInsetsPadding(WindowInsets.safeDrawing.only(WindowInsetsSides.Top + WindowInsetsSides.Horizontal))
                    .heightIn(min = 56.dp)
                    .padding(horizontal = 4.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                IconButton(onClick = ::requestDismiss) {
                    Icon(Icons.Rounded.Close, contentDescription = "Kapat", tint = colors.muted)
                }
            }
        },
        bottomBar = {
            Box(
                modifier = Modifier
                    .fillMaxWidth()
                    .windowInsetsPadding(
                        WindowInsets.safeDrawing.only(WindowInsetsSides.Bottom + WindowInsetsSides.Horizontal),
                    )
                    .padding(horizontal = 20.dp, vertical = 12.dp),
                contentAlignment = Alignment.Center,
            ) {
                PrimaryButton(
                    modifier = Modifier.widthIn(max = 560.dp),
                    text = "Kaydet",
                    onClick = ::save,
                    enabled = profile == null || hasChanges,
                )
            }
        },
    ) { contentPadding ->
        Box(
            modifier = Modifier
                .fillMaxSize()
                .padding(contentPadding),
            contentAlignment = Alignment.TopCenter,
        ) {
            Column(
                modifier = Modifier
                    .widthIn(max = 560.dp)
                    .fillMaxSize()
                    .verticalScroll(rememberScrollState())
                    .padding(horizontal = 20.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.spacedBy(16.dp),
            ) {
                PlatformTile(kind = kind, size = 64.dp)
                Column(
                    horizontalAlignment = Alignment.CenterHorizontally,
                    verticalArrangement = Arrangement.spacedBy(6.dp),
                ) {
                    Text(
                        modifier = Modifier.semantics { heading() },
                        text = if (kind == DestinationKind.CUSTOM) profile?.name ?: kind.label else kind.label,
                        style = MaterialTheme.typography.headlineSmall,
                    )
                    Text(
                        text = kind.keyHelp,
                        style = MaterialTheme.typography.bodyMedium,
                        color = colors.muted,
                        textAlign = TextAlign.Center,
                    )
                }

                if (serverExpanded) {
                    OutlinedTextField(
                        modifier = Modifier
                            .fillMaxWidth()
                            .focusRequester(serverFocus)
                            .semantics { if (serverTouched && serverError != null) this.error(serverError) },
                        value = serverUrl,
                        onValueChange = {
                            serverUrl = it
                            serverTouched = true
                            editorError = null
                        },
                        label = { Text("Sunucu adresi") },
                        placeholder = { Text(kind.serverPlaceholder) },
                        singleLine = true,
                        isError = serverTouched && serverError != null,
                        supportingText = if (serverTouched && serverError != null) {
                            { Text(serverError) }
                        } else {
                            null
                        },
                        keyboardOptions = KeyboardOptions(
                            autoCorrectEnabled = false,
                            keyboardType = KeyboardType.Uri,
                            imeAction = ImeAction.Next,
                        ),
                        keyboardActions = KeyboardActions(onNext = { keyFocus.requestFocus() }),
                        shape = MaterialTheme.shapes.small,
                    )
                }

                OutlinedTextField(
                    modifier = Modifier
                        .fillMaxWidth()
                        .focusRequester(keyFocus)
                        .semantics { if (keyTouched && keyError != null) this.error(keyError) },
                    value = streamKey,
                    onValueChange = {
                        streamKey = it
                        keyTouched = true
                        editorError = null
                    },
                    label = { Text(if (profile == null) "Yayın anahtarı" else "Yeni yayın anahtarı") },
                    placeholder = { Text(if (profile == null) "Buraya yapıştır" else "Değişmeyecekse boş bırak") },
                    visualTransformation = if (keyVisible) VisualTransformation.None else PasswordVisualTransformation(),
                    trailingIcon = {
                        IconButton(onClick = { keyVisible = !keyVisible }) {
                            Icon(
                                imageVector = if (keyVisible) Icons.Rounded.VisibilityOff else Icons.Rounded.Visibility,
                                contentDescription = if (keyVisible) "Anahtarı gizle" else "Anahtarı göster",
                            )
                        }
                    },
                    singleLine = true,
                    isError = keyTouched && keyError != null,
                    supportingText = {
                        Text(
                            when {
                                keyTouched && keyError != null -> keyError
                                profile != null -> "Boş bırakırsan kayıtlı anahtar korunur."
                                else -> "Yalnızca bu telefonda şifreli saklanır."
                            },
                        )
                    },
                    keyboardOptions = KeyboardOptions(
                        autoCorrectEnabled = false,
                        keyboardType = KeyboardType.Password,
                        imeAction = ImeAction.Done,
                    ),
                    keyboardActions = KeyboardActions(onDone = { save() }),
                    shape = MaterialTheme.shapes.small,
                )

                if (!serverExpanded) {
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Column(modifier = Modifier.weight(1f)) {
                            Text(text = "Sunucu adresi", style = MaterialTheme.typography.labelMedium, color = colors.muted)
                            Text(
                                text = serverUrl,
                                style = MaterialTheme.typography.bodySmall,
                                fontFamily = FontFamily.Monospace,
                                color = colors.muted,
                            )
                        }
                        TextButton(onClick = { serverExpanded = true }) { Text("Değiştir") }
                    }
                }

                editorError?.let { AlertBanner(title = "Kaydedilemedi", message = it) }

                if (onDelete != null) {
                    TextButton(
                        onClick = { showDeleteConfirmation = true },
                        colors = ButtonDefaults.textButtonColors(contentColor = colors.dangerText),
                    ) {
                        Icon(Icons.Rounded.DeleteOutline, contentDescription = null, modifier = Modifier.size(18.dp))
                        Spacer(Modifier.width(6.dp))
                        Text("Bu hedefi sil")
                    }
                }
            }
        }
    }

    val secureDialogProperties = DialogProperties(securePolicy = SecureFlagPolicy.SecureOn)

    if (showDiscardConfirmation) {
        AlertDialog(
            onDismissRequest = { showDiscardConfirmation = false },
            properties = secureDialogProperties,
            containerColor = colors.card,
            title = { Text("Kaydedilmedi") },
            text = { Text("Girdiğin bilgiler silinsin mi?") },
            confirmButton = {
                TextButton(
                    onClick = {
                        showDiscardConfirmation = false
                        state.clearSecret()
                        onDismiss()
                    },
                ) {
                    Text("Sil", color = colors.dangerText)
                }
            },
            dismissButton = {
                TextButton(onClick = { showDiscardConfirmation = false }) { Text("Geri dön") }
            },
        )
    }

    if (showDeleteConfirmation && profile != null && onDelete != null) {
        AlertDialog(
            onDismissRequest = { showDeleteConfirmation = false },
            properties = secureDialogProperties,
            containerColor = colors.card,
            title = { Text("${kind.label} silinsin mi?") },
            text = { Text("Bu telefonda şifreli saklanan yayın anahtarı da kalıcı olarak silinir.") },
            confirmButton = {
                TextButton(
                    onClick = {
                        showDeleteConfirmation = false
                        editorError = onDelete()
                    },
                ) {
                    Text("Sil", color = colors.dangerText)
                }
            },
            dismissButton = {
                TextButton(onClick = { showDeleteConfirmation = false }) { Text("Vazgeç") }
            },
        )
    }
}

@Composable
private fun SecureWindowEffect() {
    val window = LocalActivity.current?.window
    DisposableEffect(window) {
        window?.addFlags(WindowManager.LayoutParams.FLAG_SECURE)
        onDispose { window?.clearFlags(WindowManager.LayoutParams.FLAG_SECURE) }
    }
}

private inline fun validationMessage(block: () -> Unit): String? = try {
    block()
    null
} catch (error: DestinationProfileException) {
    error.message ?: "Geçersiz değer"
}
