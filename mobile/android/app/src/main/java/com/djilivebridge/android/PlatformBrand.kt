package com.djilivebridge.android

import androidx.annotation.DrawableRes
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

/** "Instagram'da" — the suffix depends on how the name is pronounced, so it is spelled out. */
internal val DestinationKind.locative: String
    get() = when (this) {
        DestinationKind.INSTAGRAM -> "Instagram'da"
        DestinationKind.TIKTOK -> "TikTok'ta"
        DestinationKind.YOUTUBE -> "YouTube'da"
        DestinationKind.FACEBOOK -> "Facebook'ta"
        DestinationKind.TWITCH -> "Twitch'te"
        DestinationKind.KICK -> "Kick'te"
        DestinationKind.CUSTOM -> "Özel sunucunda"
    }

internal val DestinationKind.dative: String
    get() = when (this) {
        DestinationKind.INSTAGRAM -> "Instagram'a"
        DestinationKind.TIKTOK -> "TikTok'a"
        DestinationKind.YOUTUBE -> "YouTube'a"
        DestinationKind.FACEBOOK -> "Facebook'a"
        DestinationKind.TWITCH -> "Twitch'e"
        DestinationKind.KICK -> "Kick'e"
        DestinationKind.CUSTOM -> "Özel sunucuna"
    }

internal val DestinationKind.keyHelp: String
    get() = when (this) {
        DestinationKind.INSTAGRAM ->
            "instagram.com'da Oluştur (+) → Canlı video'yu aç ve “Yayın anahtarı”nı kopyala. " +
                "Instagram her yayında yeni bir anahtar verir."
        DestinationKind.TIKTOK ->
            "TikTok LIVE Center'da yayın anahtarı ekranını aç; Sunucu URL'sini ve Yayın anahtarını " +
                "kopyala. Hesabının RTMP erişimi olmalı; TikTok her yayında yeni anahtar verir."
        DestinationKind.YOUTUBE ->
            "YouTube Studio'da Oluştur → Canlı yayına geç → Yayın yazılımı bölümünden “Yayın " +
                "anahtarı”nı kopyala. Anahtar, sen sıfırlayana kadar aynı kalır."
        DestinationKind.FACEBOOK ->
            "facebook.com'da Canlı video → Yayın yazılımı bölümünden “Yayın anahtarı”nı kopyala. " +
                "Kalıcı anahtarı açmadıysan Facebook her yayında yenisini verir."
        DestinationKind.TWITCH ->
            "Twitch Yayıncı Kontrol Paneli'nde Ayarlar → Yayın bölümünden birincil yayın " +
                "anahtarını kopyala."
        DestinationKind.KICK ->
            "Kick'te Creator Dashboard → Settings → Stream URL & Key bölümünden anahtarı kopyala. " +
                "Orada farklı bir Stream URL görürsen sunucu adresini onunla değiştir."
        DestinationKind.CUSTOM ->
            "Sunucunun verdiği RTMP ya da RTMPS adresini ve yayın anahtarını yapıştır."
    }

internal val DestinationKind.serverPlaceholder: String
    get() = defaultServerUrl ?: when (this) {
        DestinationKind.TIKTOK -> "rtmp://push-rtmp-….tiktokcdn.com/game"
        else -> "rtmp://sunucu.adresi/live"
    }

/** What to do on the platform once the stream arrives there. */
internal val DestinationKind.liveReminder: String?
    get() = when (this) {
        DestinationKind.INSTAGRAM ->
            "Şimdi Instagram'da “Canlı yayına geç”e bas; yayın ancak ondan sonra herkese açılır."
        DestinationKind.TIKTOK -> "TikTok LIVE Center'ı kontrol et; yayını orada da başlatman gerekebilir."
        DestinationKind.YOUTUBE -> "Otomatik başlatma kapalıysa YouTube Studio'da “Canlı yayına geç”e bas."
        DestinationKind.FACEBOOK ->
            "Facebook önce önizleme gösterir; herkese açmak için orada “Canlı yayına geç”e bas."
        DestinationKind.TWITCH, DestinationKind.KICK, DestinationKind.CUSTOM -> null
    }
