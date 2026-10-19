package com.streammydrone.app

import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.ui.unit.dp
import androidx.compose.foundation.layout.Row
import androidx.compose.material3.FilterChip
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Close
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.TextFieldValue

@Composable
internal fun BroadcastTitleDialog(
    initialTitle: String,
    hasManualDestinations: Boolean,
    hasInstagram: Boolean = false,
    initialAudience: InstagramAudience = InstagramAudience.PRACTICE,
    onDismiss: () -> Unit,
    onStart: (String, InstagramAudience) -> Unit,
) {
    var draft by rememberSaveable(stateSaver = TextFieldValue.Saver) {
        mutableStateOf(TextFieldValue(initialTitle, TextRange(0, initialTitle.length)))
    }
    var audienceName by rememberSaveable { mutableStateOf(initialAudience.name) }
    val audience = InstagramAudience.fromStorage(audienceName)
    var touched by rememberSaveable { mutableStateOf(false) }
    val focus = remember { FocusRequester() }
    val title = runCatching { validateBroadcastTitle(draft.text) }.getOrNull()
    fun submit() { title?.let { onStart(it, audience) } }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(stringResource(R.string.broadcast_title)) },
        text = {
            Column(modifier = Modifier.verticalScroll(rememberScrollState())) {
                OutlinedTextField(
                    value = draft,
                    onValueChange = { draft = it; touched = true },
                    modifier = Modifier.fillMaxWidth().focusRequester(focus),
                    label = { Text(stringResource(R.string.broadcast_title)) },
                    singleLine = true,
                    isError = touched && title == null,
                    supportingText = {
                        if (touched && title == null) Text(stringResource(R.string.broadcast_title_error))
                        else Text("${draft.text.codePointCount(0, draft.text.length)}/100")
                    },
                    trailingIcon = {
                        if (draft.text.isNotEmpty()) IconButton(onClick = {
                            draft = TextFieldValue(""); touched = true; focus.requestFocus()
                        }) { Icon(Icons.Rounded.Close, stringResource(R.string.broadcast_title_clear)) }
                    },
                    keyboardOptions = KeyboardOptions(imeAction = ImeAction.Done),
                    keyboardActions = KeyboardActions(onDone = { submit() }),
                )
                if (hasInstagram) {
                    Text(stringResource(R.string.instagram_audience))
                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        InstagramAudience.entries.forEach { choice ->
                            FilterChip(selected = choice == audience, onClick = { audienceName = choice.name },
                                label = { Text(stringResource(choice.label)) })
                        }
                    }
                }
                if (hasManualDestinations) Text(stringResource(R.string.broadcast_title_manual))
            }
        },
        confirmButton = { TextButton(onClick = ::submit, enabled = title != null) { Text(stringResource(R.string.go_live)) } },
        dismissButton = { TextButton(onClick = onDismiss) { Text(stringResource(R.string.cancel)) } },
    )
    LaunchedEffect(Unit) { focus.requestFocus() }
}
