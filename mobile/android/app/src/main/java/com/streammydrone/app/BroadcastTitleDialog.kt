package com.streammydrone.app

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicText
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.text.TextAutoSize
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Close
import androidx.compose.material.icons.rounded.Info
import androidx.compose.material.icons.rounded.Sensors
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.RadioButton
import androidx.compose.material3.RadioButtonDefaults
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.window.Dialog

/**
 * The last step before going live: where the broadcast goes, its title and, for Instagram, who
 * sees it. Dark like the camera screen it opens over, with the one big orange button.
 */
@Composable
internal fun BroadcastTitleDialog(
    initialTitle: String,
    hasManualDestinations: Boolean,
    /** The platforms this broadcast goes to, in the order they are shown on the screen. */
    platforms: List<DestinationKind> = emptyList(),
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
    LiveDialog(onDismiss = onDismiss) {
        Column(
            modifier = Modifier
                .verticalScroll(rememberScrollState())
                .padding(horizontal = 20.dp, vertical = 22.dp),
            verticalArrangement = Arrangement.spacedBy(18.dp),
        ) {
            Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                Text(
                    text = stringResource(R.string.go_live),
                    style = MaterialTheme.typography.headlineSmall,
                    color = Color.White,
                    modifier = Modifier.semantics { heading() },
                )
                if (platforms.isNotEmpty()) PlatformChips(platforms)
            }
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
                shape = RoundedCornerShape(14.dp),
                colors = liveFieldColors(),
            )
            if (hasInstagram) {
                Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    Text(
                        text = stringResource(R.string.instagram_audience),
                        style = MaterialTheme.typography.labelLarge,
                        color = Color.White.copy(alpha = 0.7f),
                    )
                    Column(modifier = Modifier.selectableGroup(), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        InstagramAudience.entries.forEach { choice ->
                            LiveChoice(
                                label = stringResource(choice.label),
                                hint = stringResource(choice.hint),
                                selected = choice == audience,
                                onSelect = { audienceName = choice.name },
                            )
                        }
                    }
                }
            }
            if (hasManualDestinations) {
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    Icon(
                        Icons.Rounded.Info,
                        contentDescription = null,
                        tint = Color.White.copy(alpha = 0.5f),
                        modifier = Modifier.padding(top = 1.dp).size(16.dp),
                    )
                    Text(
                        text = stringResource(R.string.broadcast_title_manual),
                        style = MaterialTheme.typography.bodySmall,
                        color = Color.White.copy(alpha = 0.6f),
                    )
                }
            }
            Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                LiveButton(
                    text = stringResource(R.string.go_live),
                    icon = Icons.Rounded.Sensors,
                    enabled = title != null,
                    onClick = ::submit,
                )
                LiveTextButton(text = stringResource(R.string.cancel), onClick = onDismiss)
            }
        }
    }
    LaunchedEffect(Unit) { focus.requestFocus() }
}

/** The platforms a broadcast goes to, as small labelled tiles. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun PlatformChips(platforms: List<DestinationKind>) {
    FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
        platforms.forEach { kind ->
            Row(
                modifier = Modifier
                    .background(Color.White.copy(alpha = 0.07f), CircleShape)
                    .border(1.dp, Color.White.copy(alpha = 0.1f), CircleShape)
                    .padding(start = 4.dp, end = 10.dp, top = 4.dp, bottom = 4.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(6.dp),
            ) {
                PlatformTile(kind = kind, size = 20.dp)
                Text(kind.displayName(), style = MaterialTheme.typography.labelMedium, color = Color.White)
            }
        }
    }
}

/** A dialog over the camera screen: always dark, whatever theme the rest of the app uses. */
@Composable
internal fun LiveDialog(onDismiss: () -> Unit, content: @Composable ColumnScope.() -> Unit) {
    Dialog(onDismissRequest = onDismiss) {
        DjiLiveBridgeTheme(ThemeChoice.DARK) {
            Surface(
                modifier = Modifier.fillMaxWidth(),
                shape = RoundedCornerShape(28.dp),
                color = SheetColor,
                contentColor = Color.White,
                border = BorderStroke(1.dp, Color.White.copy(alpha = 0.08f)),
            ) {
                Column(content = content)
            }
        }
    }
}

/** One choice in a list of them, with a line under its name saying what it means. */
@Composable
internal fun LiveChoice(label: String, hint: String?, selected: Boolean, onSelect: () -> Unit, enabled: Boolean = true) {
    val shape = RoundedCornerShape(14.dp)
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clip(shape)
            .background(if (selected) GoLiveStart.copy(alpha = 0.12f) else Color.White.copy(alpha = 0.04f))
            .border(1.dp, if (selected) GoLiveStart.copy(alpha = 0.9f) else Color.White.copy(alpha = 0.12f), shape)
            .selectable(selected = selected, enabled = enabled, role = Role.RadioButton, onClick = onSelect)
            .alpha(if (enabled) 1f else 0.5f)
            .padding(horizontal = 12.dp, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        RadioButton(
            selected = selected,
            onClick = null,
            enabled = enabled,
            colors = RadioButtonDefaults.colors(selectedColor = GoLiveStart, unselectedColor = Color.White.copy(alpha = 0.5f)),
        )
        Column(modifier = Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(label, style = MaterialTheme.typography.titleSmall, color = Color.White)
            hint?.let { Text(it, style = MaterialTheme.typography.bodySmall, color = Color.White.copy(alpha = 0.65f)) }
        }
    }
}

/** The orange button of the camera screen, full width; its text shrinks to fit any language. */
@Composable
internal fun LiveButton(text: String, icon: ImageVector?, onClick: () -> Unit, modifier: Modifier = Modifier, enabled: Boolean = true) {
    Row(
        modifier = modifier
            .fillMaxWidth()
            .height(52.dp)
            .alpha(if (enabled) 1f else 0.45f)
            .clip(CircleShape)
            .background(Brush.horizontalGradient(listOf(GoLiveStart, GoLiveEnd)))
            .clickable(enabled = enabled, role = Role.Button, onClick = onClick)
            .padding(horizontal = 16.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.Center,
    ) {
        icon?.let {
            Icon(it, contentDescription = null, tint = Color.White, modifier = Modifier.size(22.dp))
            Spacer(Modifier.width(8.dp))
        }
        BasicText(
            text = text,
            style = TextStyle(color = Color.White, fontSize = 16.sp, fontWeight = FontWeight.SemiBold),
            maxLines = 1,
            autoSize = TextAutoSize.StepBased(minFontSize = 11.sp, maxFontSize = 16.sp),
        )
    }
}

@Composable
internal fun LiveTextButton(text: String, onClick: () -> Unit, color: Color = Color.White.copy(alpha = 0.8f)) {
    TextButton(onClick = onClick, modifier = Modifier.fillMaxWidth().heightIn(min = 48.dp)) {
        Text(text, color = color, style = MaterialTheme.typography.labelLarge)
    }
}

@Composable
internal fun liveFieldColors() = OutlinedTextFieldDefaults.colors(
    focusedBorderColor = GoLiveStart,
    focusedLabelColor = GoLiveStart,
    cursorColor = GoLiveStart,
    unfocusedBorderColor = Color.White.copy(alpha = 0.2f),
)
