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
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Check
import androidx.compose.material.icons.rounded.Close
import androidx.compose.material.icons.rounded.DeleteOutline
import androidx.compose.material.icons.rounded.Visibility
import androidx.compose.material.icons.rounded.VisibilityOff
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusDirection
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.semantics.error
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.DialogProperties
import androidx.compose.ui.window.SecureFlagPolicy

/**
 * Full-screen, step-by-step destination editor. The window is FLAG_SECURE while it is open so
 * the stream key cannot end up in screenshots, screen recordings or the recents thumbnail.
 */
@Composable
internal fun DestinationProfileEditor(
    state: ProfileEditorState,
    onDismiss: () -> Unit,
    onSave: (String, DestinationKind, String, String) -> String?,
    onDelete: (() -> String?)?,
) {
    val focusManager = LocalFocusManager.current
    val profile = state.profile
    val initialName = profile?.name.orEmpty()
    val initialKind = profile?.kind ?: DestinationKind.CUSTOM
    val initialServerUrl = profile?.serverUrl.orEmpty()

    var name by state::name
    var kind by state::kind
    var serverUrl by state::serverUrl
    var targetStreamKey by state::targetStreamKey
    var passwordVisible by state::passwordVisible
    var editorError by state::editorError
    var nameTouched by state::nameTouched
    var serverTouched by state::serverTouched
    var keyTouched by state::keyTouched
    var showDiscardConfirmation by state::showDiscardConfirmation
    var showDeleteConfirmation by state::showDeleteConfirmation
    var autoFilledName by state::autoFilledName
    var autoFilledServerUrl by state::autoFilledServerUrl

    // The name is optional: an empty one falls back to the platform name.
    val effectiveName = name.ifBlank { kind.label }
    val nameError = validationMessage { validateName(effectiveName) }
    val serverError = validationMessage { validateServerUrl(serverUrl) }
    val keyError = when {
        targetStreamKey.isEmpty() && profile != null -> null
        targetStreamKey.isEmpty() -> "Yayın anahtarını yapıştır"
        else -> validationMessage { validateStreamKey(targetStreamKey) }
    }
    val hasChanges = if (profile == null) {
        targetStreamKey.isNotEmpty() ||
            (name.isNotBlank() && name != autoFilledName) ||
            (serverUrl.isNotBlank() && serverUrl != autoFilledServerUrl)
    } else {
        effectiveName.trim() != initialName.trim() ||
            kind != initialKind ||
            serverUrl.trim().trimEnd('/') != initialServerUrl.trim().trimEnd('/') ||
            targetStreamKey.isNotEmpty()
    }

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
        nameTouched = true
        serverTouched = true
        keyTouched = true
        if (nameError != null || serverError != null || keyError != null) return
        editorError = onSave(effectiveName, kind, serverUrl, targetStreamKey)
        if (editorError == null) state.clearSecret()
    }

    fun selectKind(selected: DestinationKind) {
        if (selected == kind) return
        if (name.isBlank() || name == autoFilledName) {
            name = selected.label
            autoFilledName = selected.label
        }
        val suggestedUrl = selected.defaultServerUrl.orEmpty()
        if (serverUrl.isBlank() || serverUrl == autoFilledServerUrl) {
            serverUrl = suggestedUrl
            autoFilledServerUrl = suggestedUrl.takeIf(String::isNotEmpty)
        }
        kind = selected
        editorError = null
    }

    SecureWindowEffect()
    BackHandler(onBack = ::requestDismiss)

    Scaffold(
        containerColor = MaterialTheme.colorScheme.background,
        contentWindowInsets = WindowInsets.safeDrawing,
        topBar = {
            EditorTopBar(
                title = if (profile == null) "Yeni yayın hedefi" else "Hedefi düzenle",
                onClose = ::requestDismiss,
            )
        },
        bottomBar = {
            EditorSaveBar(enabled = profile == null || hasChanges, onSave = ::save)
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
                    .widthIn(max = 640.dp)
                    .fillMaxSize()
                    .verticalScroll(rememberScrollState())
                    .padding(horizontal = 16.dp, vertical = 8.dp),
                verticalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                Text(
                    modifier = Modifier.padding(horizontal = 4.dp),
                    text = if (profile == null) {
                        "Yayının gideceği yeri kaydet. Bu bilgileri yalnızca bir kez girmen yeterli."
                    } else {
                        "Değiştirmek istediğin alanları güncelle."
                    },
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )

                BridgeCard {
                    FieldHeader(number = 1, title = "Platformu seç")
                    Row(
                        modifier = Modifier
                            .fillMaxWidth()
                            .selectableGroup(),
                        horizontalArrangement = Arrangement.spacedBy(8.dp),
                    ) {
                        DestinationKind.entries.forEach { option ->
                            PlatformChoice(
                                kind = option,
                                onClick = { selectKind(option) },
                                modifier = Modifier.weight(1f),
                                selected = kind == option,
                                selectable = true,
                            )
                        }
                    }
                    TipBox(title = "Bilgileri nereden bulurum?", text = kind.helpText)
                }

                BridgeCard {
                    FieldHeader(number = 2, title = "Sunucu adresini yapıştır")
                    OutlinedTextField(
                        modifier = Modifier
                            .fillMaxWidth()
                            .semantics {
                                if (serverTouched && serverError != null) this.error(serverError)
                            },
                        value = serverUrl,
                        onValueChange = {
                            serverUrl = it
                            serverTouched = true
                            autoFilledServerUrl = null
                            editorError = null
                        },
                        label = { Text("Sunucu adresi (URL)") },
                        placeholder = { Text(kind.serverPlaceholder) },
                        singleLine = true,
                        isError = serverTouched && serverError != null,
                        supportingText = {
                            Text(
                                if (serverTouched && serverError != null) {
                                    serverError
                                } else {
                                    "rtmp:// ya da rtmps:// ile başlar."
                                },
                            )
                        },
                        keyboardOptions = KeyboardOptions(
                            autoCorrectEnabled = false,
                            keyboardType = KeyboardType.Uri,
                            imeAction = ImeAction.Next,
                        ),
                        keyboardActions = KeyboardActions(
                            onNext = { focusManager.moveFocus(FocusDirection.Down) },
                        ),
                        shape = MaterialTheme.shapes.small,
                    )
                }

                BridgeCard {
                    FieldHeader(
                        number = 3,
                        title = if (profile == null) "Yayın anahtarını yapıştır" else "Yayın anahtarı",
                    )
                    OutlinedTextField(
                        modifier = Modifier
                            .fillMaxWidth()
                            .semantics {
                                if (keyTouched && keyError != null) this.error(keyError)
                            },
                        value = targetStreamKey,
                        onValueChange = {
                            targetStreamKey = it
                            keyTouched = true
                            editorError = null
                        },
                        label = { Text(if (profile == null) "Yayın anahtarı" else "Yeni yayın anahtarı") },
                        placeholder = {
                            Text(if (profile == null) "Anahtarı buraya yapıştır" else "Değişmeyecekse boş bırak")
                        },
                        visualTransformation = if (passwordVisible) {
                            VisualTransformation.None
                        } else {
                            PasswordVisualTransformation()
                        },
                        trailingIcon = {
                            IconButton(onClick = { passwordVisible = !passwordVisible }) {
                                Icon(
                                    imageVector = if (passwordVisible) {
                                        Icons.Rounded.VisibilityOff
                                    } else {
                                        Icons.Rounded.Visibility
                                    },
                                    contentDescription = if (passwordVisible) {
                                        "Yayın anahtarını gizle"
                                    } else {
                                        "Yayın anahtarını göster"
                                    },
                                )
                            }
                        },
                        singleLine = true,
                        isError = keyTouched && keyError != null,
                        supportingText = {
                            Text(
                                when {
                                    keyTouched && keyError != null -> keyError
                                    profile != null ->
                                        "Boş bırakırsan kayıtlı anahtar korunur. Anahtar yalnızca bu " +
                                            "telefonda şifreli saklanır."
                                    else -> "Anahtar yalnızca bu telefonda, Android Keystore ile şifreli saklanır."
                                },
                            )
                        },
                        keyboardOptions = KeyboardOptions(
                            autoCorrectEnabled = false,
                            keyboardType = KeyboardType.Password,
                            imeAction = ImeAction.Next,
                        ),
                        keyboardActions = KeyboardActions(
                            onNext = { focusManager.moveFocus(FocusDirection.Down) },
                        ),
                        shape = MaterialTheme.shapes.small,
                    )
                }

                BridgeCard {
                    FieldHeader(number = 4, title = "Bir ad ver", subtitle = "İsteğe bağlı")
                    OutlinedTextField(
                        modifier = Modifier
                            .fillMaxWidth()
                            .semantics {
                                if (nameTouched && nameError != null) this.error(nameError)
                            },
                        value = name,
                        onValueChange = {
                            name = it
                            nameTouched = true
                            autoFilledName = null
                            editorError = null
                        },
                        label = { Text("Hedefin adı") },
                        placeholder = { Text(kind.label) },
                        singleLine = true,
                        isError = nameTouched && nameError != null,
                        supportingText = {
                            Text(
                                if (nameTouched && nameError != null) {
                                    nameError
                                } else {
                                    "Listede bu adla görünür. Boş bırakırsan platform adı kullanılır."
                                },
                            )
                        },
                        keyboardOptions = KeyboardOptions(
                            capitalization = KeyboardCapitalization.Words,
                            imeAction = ImeAction.Done,
                        ),
                        keyboardActions = KeyboardActions(onDone = { focusManager.clearFocus() }),
                        shape = MaterialTheme.shapes.small,
                    )
                }

                editorError?.let { AlertBanner(title = "Kaydedilemedi", message = it) }

                if (onDelete != null) {
                    TextButton(
                        modifier = Modifier
                            .fillMaxWidth()
                            .heightIn(min = 48.dp),
                        onClick = { showDeleteConfirmation = true },
                        colors = ButtonDefaults.textButtonColors(
                            contentColor = MaterialTheme.colorScheme.error,
                        ),
                    ) {
                        Icon(Icons.Rounded.DeleteOutline, contentDescription = null, modifier = Modifier.size(20.dp))
                        Spacer(Modifier.width(8.dp))
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
            containerColor = BridgeTheme.colors.card,
            title = { Text("Değişiklikler kaydedilmedi") },
            text = { Text("Bu ekranda yaptığın değişiklikler silinsin mi?") },
            confirmButton = {
                TextButton(
                    onClick = {
                        showDiscardConfirmation = false
                        state.clearSecret()
                        onDismiss()
                    },
                ) {
                    Text("Değişiklikleri sil", color = MaterialTheme.colorScheme.error)
                }
            },
            dismissButton = {
                TextButton(onClick = { showDiscardConfirmation = false }) {
                    Text("Düzenlemeye dön")
                }
            },
        )
    }

    if (showDeleteConfirmation && profile != null && onDelete != null) {
        AlertDialog(
            onDismissRequest = { showDeleteConfirmation = false },
            properties = secureDialogProperties,
            containerColor = BridgeTheme.colors.card,
            title = { Text("Hedef silinsin mi?") },
            text = {
                Text("${profile.name} ve bu telefonda şifreli saklanan yayın anahtarı kalıcı olarak silinecek.")
            },
            confirmButton = {
                TextButton(
                    onClick = {
                        showDeleteConfirmation = false
                        editorError = onDelete()
                    },
                ) {
                    Text("Sil", color = MaterialTheme.colorScheme.error)
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

@Composable
private fun EditorTopBar(title: String, onClose: () -> Unit) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .windowInsetsPadding(WindowInsets.safeDrawing.only(WindowInsetsSides.Top + WindowInsetsSides.Horizontal))
            .heightIn(min = 64.dp)
            .padding(start = 4.dp, end = 16.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        IconButton(onClick = onClose) {
            Icon(Icons.Rounded.Close, contentDescription = "Kapat")
        }
        Text(
            modifier = Modifier.semantics { heading() },
            text = title,
            style = MaterialTheme.typography.titleLarge,
        )
    }
}

@Composable
private fun EditorSaveBar(enabled: Boolean, onSave: () -> Unit) {
    val colors = BridgeTheme.colors
    Surface(color = colors.card) {
        Column {
            HorizontalDivider(color = colors.cardBorder)
            Box(
                modifier = Modifier
                    .fillMaxWidth()
                    .windowInsetsPadding(
                        WindowInsets.safeDrawing.only(WindowInsetsSides.Bottom + WindowInsetsSides.Horizontal),
                    )
                    .padding(horizontal = 20.dp, vertical = 12.dp),
                contentAlignment = Alignment.Center,
            ) {
                PrimaryActionButton(
                    modifier = Modifier.widthIn(max = 600.dp),
                    text = "Kaydet",
                    onClick = onSave,
                    enabled = enabled,
                    icon = Icons.Rounded.Check,
                )
            }
        }
    }
}

@Composable
private fun FieldHeader(number: Int, title: String, subtitle: String? = null) {
    val scheme = MaterialTheme.colorScheme
    Row(
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Surface(
            modifier = Modifier.size(26.dp),
            shape = CircleShape,
            color = scheme.primaryContainer,
            contentColor = scheme.onPrimaryContainer,
        ) {
            Box(contentAlignment = Alignment.Center) {
                Text(text = number.toString(), style = MaterialTheme.typography.labelLarge)
            }
        }
        Column {
            Text(
                modifier = Modifier.semantics { heading() },
                text = title,
                style = MaterialTheme.typography.titleSmall,
            )
            subtitle?.let {
                Text(text = it, style = MaterialTheme.typography.bodySmall, color = scheme.onSurfaceVariant)
            }
        }
    }
}

private val DestinationKind.helpText: String
    get() = when (this) {
        DestinationKind.TIKTOK ->
            "TikTok LIVE Center'da (ya da LIVE Studio'da) yayın anahtarı ekranını aç. Sunucu " +
                "URL'sini ve yayın anahtarını kopyalayıp buraya yapıştır. Hesabının RTMP ile " +
                "yayın izni olmalı."
        DestinationKind.YOUTUBE ->
            "YouTube Studio'da Oluştur → Canlı yayına geç → Yayın yazılımı bölümünü aç. Sunucu " +
                "adresi hazır; yalnızca yayın anahtarını kopyalayıp yapıştır."
        DestinationKind.CUSTOM ->
            "Yayın yaptığın platformun verdiği RTMP ya da RTMPS sunucu adresini ve yayın " +
                "anahtarını yapıştır."
    }

private val DestinationKind.serverPlaceholder: String
    get() = when (this) {
        DestinationKind.TIKTOK -> "rtmp://push-rtmp-….tiktokcdn.com/game"
        DestinationKind.YOUTUBE -> "rtmps://a.rtmps.youtube.com/live2"
        DestinationKind.CUSTOM -> "rtmp://sunucu.adresi/live"
    }

private inline fun validationMessage(block: () -> Unit): String? = try {
    block()
    null
} catch (error: DestinationProfileException) {
    error.message ?: "Geçersiz değer"
}
