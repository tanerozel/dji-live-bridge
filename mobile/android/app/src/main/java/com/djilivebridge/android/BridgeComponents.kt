package com.djilivebridge.android

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.requiredSize
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Check
import androidx.compose.material.icons.rounded.ContentCopy
import androidx.compose.material.icons.rounded.ErrorOutline
import androidx.compose.material.icons.rounded.ExpandMore
import androidx.compose.material.icons.rounded.Lightbulb
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

@Composable
internal fun BridgeCard(
    modifier: Modifier = Modifier,
    contentPadding: PaddingValues = PaddingValues(20.dp),
    verticalSpacing: Dp = 16.dp,
    content: @Composable ColumnScope.() -> Unit,
) {
    val colors = BridgeTheme.colors
    Surface(
        modifier = modifier.fillMaxWidth(),
        shape = MaterialTheme.shapes.large,
        color = colors.card,
        contentColor = colors.text,
        border = BorderStroke(1.dp, colors.border),
    ) {
        Column(
            modifier = Modifier.padding(contentPadding),
            verticalArrangement = Arrangement.spacedBy(verticalSpacing),
            content = content,
        )
    }
}

/** The launcher icon drawn in-app: brand gradient with the white drone glyph. */
@Composable
internal fun BrandMark(size: Dp, modifier: Modifier = Modifier) {
    val colors = BridgeTheme.colors
    Box(
        modifier = modifier
            .size(size)
            .clip(RoundedCornerShape(size * 0.28f))
            .background(Brush.verticalGradient(listOf(colors.brandStart, colors.brandEnd))),
        contentAlignment = Alignment.Center,
    ) {
        // The adaptive-icon foreground keeps an 18dp margin around its 108dp canvas; drawing it
        // at 1.5x lets the visible 72dp fill this box the way a launcher mask does.
        Image(
            painter = painterResource(R.drawable.ic_launcher_foreground),
            contentDescription = null,
            modifier = Modifier.requiredSize(size * 1.5f),
        )
    }
}

@Composable
internal fun PulsingDot(color: Color, modifier: Modifier = Modifier, size: Dp = 8.dp) {
    val alpha = rememberInfiniteTransition(label = "pulse").animateFloat(
        initialValue = 1f,
        targetValue = 0.3f,
        animationSpec = infiniteRepeatable(tween(durationMillis = 900), RepeatMode.Reverse),
        label = "pulseAlpha",
    )
    Box(
        modifier = modifier
            .size(size)
            .graphicsLayer { this.alpha = alpha.value }
            .background(color, CircleShape),
    )
}

/** The red badge streaming apps use; only shown while the stream really reaches the platform. */
@Composable
internal fun LiveBadge(modifier: Modifier = Modifier) {
    val colors = BridgeTheme.colors
    Surface(
        modifier = modifier,
        shape = RoundedCornerShape(8.dp),
        color = colors.live,
        contentColor = Color.White,
    ) {
        Row(
            modifier = Modifier.padding(horizontal = 10.dp, vertical = 5.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            PulsingDot(color = Color.White, size = 7.dp)
            Text(text = "CANLI", style = MaterialTheme.typography.labelLarge, letterSpacing = 1.sp)
        }
    }
}

/** A step number that turns into a green check once the step is done (desktop .step-badge). */
@Composable
internal fun StepDot(number: Int, done: Boolean) {
    val colors = BridgeTheme.colors
    Box(
        modifier = Modifier
            .size(24.dp)
            .background(if (done) colors.success else colors.accentSoft, CircleShape)
            .clearAndSetSemantics {
                contentDescription = if (done) "Adım $number tamamlandı" else "Adım $number"
            },
        contentAlignment = Alignment.Center,
    ) {
        if (done) {
            Icon(Icons.Rounded.Check, contentDescription = null, tint = Color.White, modifier = Modifier.size(16.dp))
        } else {
            Text(
                text = number.toString(),
                color = colors.link,
                style = MaterialTheme.typography.labelMedium,
                fontWeight = FontWeight.Bold,
            )
        }
    }
}

@Composable
internal fun SectionHeader(
    title: String,
    modifier: Modifier = Modifier,
    step: Int? = null,
    done: Boolean = false,
    trailing: (@Composable () -> Unit)? = null,
) {
    Row(
        modifier = modifier.fillMaxWidth(),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        step?.let { StepDot(number = it, done = done) }
        Text(
            modifier = Modifier
                .weight(1f)
                .semantics { heading() },
            text = title,
            style = MaterialTheme.typography.titleSmall,
        )
        trailing?.invoke()
    }
}

/** Soft accent box for short explanations (desktop .callout). */
@Composable
internal fun TipBox(text: String, modifier: Modifier = Modifier, icon: ImageVector = Icons.Rounded.Lightbulb) {
    val colors = BridgeTheme.colors
    Surface(
        modifier = modifier.fillMaxWidth(),
        shape = MaterialTheme.shapes.small,
        color = colors.accentSoft,
        contentColor = colors.onAccentSoft,
    ) {
        Row(
            modifier = Modifier.padding(12.dp),
            verticalAlignment = Alignment.Top,
            horizontalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            Icon(icon, contentDescription = null, modifier = Modifier.size(18.dp))
            Text(text = text, style = MaterialTheme.typography.bodyMedium)
        }
    }
}

@Composable
internal fun AlertBanner(title: String, modifier: Modifier = Modifier, message: String? = null) {
    val colors = BridgeTheme.colors
    Surface(
        modifier = modifier
            .fillMaxWidth()
            .semantics(mergeDescendants = true) { liveRegion = LiveRegionMode.Assertive },
        shape = MaterialTheme.shapes.small,
        color = colors.dangerSoft,
        contentColor = colors.dangerText,
    ) {
        Row(
            modifier = Modifier.padding(12.dp),
            verticalAlignment = Alignment.Top,
            horizontalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            Icon(Icons.Rounded.ErrorOutline, contentDescription = null, modifier = Modifier.size(20.dp))
            Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
                Text(text = title, style = MaterialTheme.typography.titleSmall)
                message?.let { Text(text = it, style = MaterialTheme.typography.bodyMedium, color = colors.text) }
            }
        }
    }
}

/**
 * The RTMP address typed into DJI Fly on the RC 2. It is large and monospaced because it is
 * read on one screen and typed on another; copying only helps when DJI Fly runs on this phone.
 */
@Composable
internal fun AddressField(address: String, onCopy: () -> Unit, modifier: Modifier = Modifier) {
    val colors = BridgeTheme.colors
    Surface(
        modifier = modifier.fillMaxWidth(),
        shape = MaterialTheme.shapes.small,
        color = colors.field,
        contentColor = colors.text,
    ) {
        Row(
            modifier = Modifier.padding(start = 14.dp, top = 6.dp, end = 2.dp, bottom = 6.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            SelectionContainer(modifier = Modifier.weight(1f)) {
                Text(
                    text = address,
                    fontFamily = FontFamily.Monospace,
                    fontWeight = FontWeight.SemiBold,
                    fontSize = 16.sp,
                    lineHeight = 22.sp,
                )
            }
            IconButton(onClick = onCopy) {
                Icon(Icons.Rounded.ContentCopy, contentDescription = "Adresi kopyala", tint = colors.link)
            }
        }
    }
}

@Composable
internal fun PrimaryButton(
    text: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    icon: ImageVector? = null,
    enabled: Boolean = true,
) {
    val colors = BridgeTheme.colors
    Button(
        onClick = onClick,
        modifier = modifier
            .fillMaxWidth()
            .heightIn(min = 54.dp),
        enabled = enabled,
        shape = MaterialTheme.shapes.medium,
        colors = ButtonDefaults.buttonColors(
            containerColor = colors.action,
            contentColor = Color.White,
            disabledContainerColor = colors.fieldStrong,
            disabledContentColor = colors.muted,
        ),
    ) {
        icon?.let {
            Icon(it, contentDescription = null, modifier = Modifier.size(20.dp))
            Spacer(Modifier.width(8.dp))
        }
        Text(text = text, style = MaterialTheme.typography.titleMedium)
    }
}

/** Stopping is always confirmed, so the button stays calm instead of alarm red. */
@Composable
internal fun StopButton(text: String, onClick: () -> Unit, icon: ImageVector, modifier: Modifier = Modifier) {
    val colors = BridgeTheme.colors
    Button(
        onClick = onClick,
        modifier = modifier
            .fillMaxWidth()
            .heightIn(min = 54.dp),
        shape = MaterialTheme.shapes.medium,
        colors = ButtonDefaults.buttonColors(
            containerColor = colors.dangerSoft,
            contentColor = colors.dangerText,
        ),
    ) {
        Icon(icon, contentDescription = null, modifier = Modifier.size(20.dp))
        Spacer(Modifier.width(8.dp))
        Text(text = text, style = MaterialTheme.typography.titleMedium)
    }
}

@Composable
internal fun ExpandableSection(
    title: String,
    expanded: Boolean,
    onToggle: () -> Unit,
    modifier: Modifier = Modifier,
    content: @Composable ColumnScope.() -> Unit,
) {
    val colors = BridgeTheme.colors
    val rotation by animateFloatAsState(if (expanded) 180f else 0f, label = "expandRotation")
    Column(modifier = modifier.fillMaxWidth()) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .heightIn(min = 44.dp)
                .clip(MaterialTheme.shapes.small)
                .clickable(onClickLabel = if (expanded) "Kapat" else "Aç", role = Role.Button, onClick = onToggle)
                .semantics { stateDescription = if (expanded) "Açık" else "Kapalı" },
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(
                modifier = Modifier.weight(1f),
                text = title,
                style = MaterialTheme.typography.labelLarge,
                color = colors.link,
            )
            Icon(
                imageVector = Icons.Rounded.ExpandMore,
                contentDescription = null,
                tint = colors.link,
                modifier = Modifier.rotate(rotation),
            )
        }
        AnimatedVisibility(visible = expanded) {
            Column(
                modifier = Modifier.padding(top = 4.dp, bottom = 4.dp),
                verticalArrangement = Arrangement.spacedBy(8.dp),
                content = content,
            )
        }
    }
}

@Composable
internal fun BulletItem(text: String) {
    val colors = BridgeTheme.colors
    Row(horizontalArrangement = Arrangement.spacedBy(10.dp), verticalAlignment = Alignment.Top) {
        Box(
            modifier = Modifier
                .padding(top = 8.dp)
                .size(5.dp)
                .background(colors.faint, CircleShape),
        )
        Text(text = text, style = MaterialTheme.typography.bodyMedium, color = colors.muted)
    }
}

@Composable
internal fun DetailRow(label: String, value: String) {
    val colors = BridgeTheme.colors
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .semantics(mergeDescendants = true) {},
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Text(
            modifier = Modifier.weight(1f),
            text = label,
            style = MaterialTheme.typography.bodyMedium,
            color = colors.muted,
        )
        Text(
            modifier = Modifier.weight(1f),
            text = value,
            style = MaterialTheme.typography.bodyMedium,
            textAlign = TextAlign.End,
        )
    }
}
