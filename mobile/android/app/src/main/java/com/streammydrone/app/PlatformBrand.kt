package com.streammydrone.app

import android.content.Context
import androidx.annotation.DrawableRes
import androidx.annotation.StringRes
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Icon
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.Dp

// Tile colors match the desktop platform icons (src/styles.css .platform-icon.*).
private val InstagramGradient = Brush.linearGradient(
    0f to Color(0xFFFEDA75),
    0.22f to Color(0xFFFA7E1E),
    0.5f to Color(0xFFD62976),
    0.75f to Color(0xFF962FBF),
    1f to Color(0xFF4F5BD5),
    start = Offset(0f, Float.POSITIVE_INFINITY),
    end = Offset(Float.POSITIVE_INFINITY, 0f),
)
private val TikTokCyan = Color(0xFF25F4EE)
private val TikTokRed = Color(0xFFFE2C55)

private val DestinationKind.tileBrush: Brush
    get() = when (this) {
        DestinationKind.INSTAGRAM -> InstagramGradient
        DestinationKind.TIKTOK -> SolidColor(Color.Black)
        DestinationKind.YOUTUBE -> SolidColor(Color(0xFFFF0000))
        DestinationKind.FACEBOOK -> SolidColor(Color(0xFF1877F2))
        DestinationKind.TWITCH -> SolidColor(Color(0xFF9146FF))
        DestinationKind.KICK -> SolidColor(Color(0xFF53FC18))
        DestinationKind.CUSTOM -> SolidColor(Color(0xFF5E5CE6))
    }

@get:DrawableRes
private val DestinationKind.glyph: Int
    get() = when (this) {
        DestinationKind.INSTAGRAM -> R.drawable.ic_platform_instagram
        DestinationKind.TIKTOK -> R.drawable.ic_platform_tiktok
        DestinationKind.YOUTUBE -> R.drawable.ic_platform_youtube
        DestinationKind.FACEBOOK -> R.drawable.ic_platform_facebook
        DestinationKind.TWITCH -> R.drawable.ic_platform_twitch
        DestinationKind.KICK -> R.drawable.ic_platform_kick
        DestinationKind.CUSTOM -> R.drawable.ic_platform_custom
    }

/** The platform's app-icon style tile: brand background with a white glyph. */
@Composable
internal fun PlatformTile(kind: DestinationKind, size: Dp, modifier: Modifier = Modifier) {
    val glyph = painterResource(kind.glyph)
    val glyphSize = size * 0.6f
    val glyphColor = if (kind == DestinationKind.KICK) Color.Black else Color.White
    Box(
        modifier = modifier
            .size(size)
            .clip(RoundedCornerShape(size * 0.28f))
            .background(kind.tileBrush),
        contentAlignment = Alignment.Center,
    ) {
        if (kind == DestinationKind.TIKTOK) {
            // TikTok's cyan/red echo, as on the desktop.
            val echo = size * 0.035f
            Icon(glyph, null, Modifier.size(glyphSize).offset(-echo, -echo), tint = TikTokCyan)
            Icon(glyph, null, Modifier.size(glyphSize).offset(echo, echo), tint = TikTokRed)
        }
        Icon(glyph, null, Modifier.size(glyphSize), tint = glyphColor)
    }
}

/** Instagram and TikTok hand out a new stream key for every broadcast. */
internal val DestinationKind.keyChangesEachStream: Boolean
    get() = this == DestinationKind.INSTAGRAM || this == DestinationKind.TIKTOK

/** The platform as people call it; a custom server is named in the app's language. */
internal fun DestinationKind.displayName(context: Context): String =
    brandName ?: context.getString(R.string.platform_custom)

@Composable
internal fun DestinationKind.displayName(): String = brandName ?: stringResource(R.string.platform_custom)

/** The name for a message that is put into words later, in the language shown then. */
internal val DestinationKind.nameText: UiText
    get() = brandName?.let(UiText::Raw) ?: uiText(R.string.platform_custom)

/**
 * "on Instagram": a phrase for where the stream is, spelled out per platform because some
 * languages (Turkish: Instagram'da, TikTok'ta) change the ending by how the name is pronounced.
 */
@get:StringRes
internal val DestinationKind.onPlatform: Int
    get() = when (this) {
        DestinationKind.INSTAGRAM -> R.string.on_instagram
        DestinationKind.TIKTOK -> R.string.on_tiktok
        DestinationKind.YOUTUBE -> R.string.on_youtube
        DestinationKind.FACEBOOK -> R.string.on_facebook
        DestinationKind.TWITCH -> R.string.on_twitch
        DestinationKind.KICK -> R.string.on_kick
        DestinationKind.CUSTOM -> R.string.on_custom
    }

@get:StringRes
internal val DestinationKind.keyHelp: Int
    get() = when (this) {
        DestinationKind.INSTAGRAM -> R.string.key_help_instagram
        DestinationKind.TIKTOK -> R.string.key_help_tiktok
        DestinationKind.YOUTUBE -> R.string.key_help_youtube
        DestinationKind.FACEBOOK -> R.string.key_help_facebook
        DestinationKind.TWITCH -> R.string.key_help_twitch
        DestinationKind.KICK -> R.string.key_help_kick
        DestinationKind.CUSTOM -> R.string.key_help_custom
    }

@Composable
internal fun DestinationKind.serverPlaceholder(): String = defaultServerUrl ?: when (this) {
    DestinationKind.TIKTOK -> "rtmp://push-rtmp-….tiktokcdn.com/game"
    else -> stringResource(R.string.server_placeholder)
}

/** What to do on the platform once the stream arrives there. */
@get:StringRes
internal val DestinationKind.liveReminder: Int?
    get() = when (this) {
        DestinationKind.INSTAGRAM -> R.string.live_reminder_instagram
        DestinationKind.TIKTOK -> R.string.live_reminder_tiktok
        DestinationKind.YOUTUBE -> R.string.live_reminder_youtube
        DestinationKind.FACEBOOK -> R.string.live_reminder_facebook
        DestinationKind.TWITCH, DestinationKind.KICK, DestinationKind.CUSTOM -> null
    }
