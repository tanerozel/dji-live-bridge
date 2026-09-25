package com.streammydrone.app

import androidx.annotation.StringRes
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.ColorScheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Shapes
import androidx.compose.material3.Typography
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.ReadOnlyComposable
import androidx.compose.runtime.remember
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp

/** The desktop app's themes (src/theme.ts); light is the default there too. */
enum class ThemeChoice(val storageValue: String, @StringRes val label: Int) {
    SYSTEM("system", R.string.theme_system),
    LIGHT("light", R.string.theme_light),
    DARK("dark", R.string.theme_dark),
    MIDNIGHT("midnight", R.string.theme_midnight),
    SAND("sand", R.string.theme_sand),
    ;

    companion object {
        val DEFAULT = LIGHT

        fun fromStorage(value: String?): ThemeChoice = entries.firstOrNull { it.storageValue == value } ?: DEFAULT
    }
}

/**
 * One theme's colors, named after the desktop tokens in src/styles.css. Translucent desktop
 * tints are pre-blended onto [card]. Where a desktop pair misses WCAG AA for small text,
 * [link] and [action] use the theme's deeper accent instead.
 */
@Immutable
data class BridgePalette(
    val isDark: Boolean,
    val background: Color,
    val card: Color,
    val field: Color,
    val fieldStrong: Color,
    val border: Color,
    val text: Color,
    val muted: Color,
    val faint: Color,
    /** Selection rings, icons and focus. */
    val accent: Color,
    /** Accent for text and small icons. */
    val link: Color,
    /** Filled buttons; always carries white text. */
    val action: Color,
    val accentSoft: Color,
    val onAccentSoft: Color,
    val success: Color,
    val successText: Color,
    val successSoft: Color,
    val warningText: Color,
    val warningSoft: Color,
    val dangerText: Color,
    val dangerSoft: Color,
    val live: Color = Color(0xFFE0263E),
    val brandStart: Color = Color(0xFF4AA8FF),
    val brandEnd: Color = Color(0xFF0A5CF0),
)

private val LightPalette = BridgePalette(
    isDark = false,
    background = Color(0xFFF5F5F7),
    card = Color(0xFFFFFFFF),
    field = Color(0xFFF2F2F5),
    fieldStrong = Color(0xFFE8E8ED),
    border = Color(0xFFE3E3E8),
    text = Color(0xFF1D1D1F),
    muted = Color(0xFF6E6E73),
    faint = Color(0xFF8E8E93),
    accent = Color(0xFF0A84FF),
    link = Color(0xFF006AD6),
    action = Color(0xFF0071E3),
    accentSoft = Color(0xFFEAF3FF),
    onAccentSoft = Color(0xFF004A99),
    success = Color(0xFF34C759),
    successText = Color(0xFF1A7F37),
    successSoft = Color(0xFFEAF8EE),
    warningText = Color(0xFF9A5B00),
    warningSoft = Color(0xFFFFF5E0),
    dangerText = Color(0xFFD70015),
    dangerSoft = Color(0xFFFFF0EF),
)

private val SandPalette = BridgePalette(
    isDark = false,
    background = Color(0xFFF5F0E8),
    card = Color(0xFFFFFDF9),
    field = Color(0xFFF4EDE3),
    fieldStrong = Color(0xFFEAE1D4),
    border = Color(0xFFE7DDCF),
    text = Color(0xFF2B2118),
    muted = Color(0xFF76675A),
    faint = Color(0xFF9A8A78),
    accent = Color(0xFFC8581C),
    link = Color(0xFFB04B14),
    action = Color(0xFFB04B14),
    accentSoft = Color(0xFFFBEADF),
    onAccentSoft = Color(0xFF7A3510),
    success = Color(0xFF3F9B5A),
    successText = Color(0xFF2C7A44),
    successSoft = Color(0xFFE7F3E9),
    warningText = Color(0xFF8A5A00),
    warningSoft = Color(0xFFFBF0D6),
    dangerText = Color(0xFFB42318),
    dangerSoft = Color(0xFFFCEBE8),
)

private val DarkPalette = BridgePalette(
    isDark = true,
    background = Color(0xFF161618),
    card = Color(0xFF1F1F22),
    field = Color(0xFF27272B),
    fieldStrong = Color(0xFF2F2F34),
    border = Color(0xFF2E2E33),
    text = Color(0xFFF2F2F5),
    muted = Color(0xFFA1A1A8),
    faint = Color(0xFF76767E),
    accent = Color(0xFF0A84FF),
    link = Color(0xFF339AFF),
    action = Color(0xFF0071E3),
    accentSoft = Color(0xFF1C2F45),
    onAccentSoft = Color(0xFF9CCBFF),
    success = Color(0xFF30D158),
    successText = Color(0xFF5EE08A),
    successSoft = Color(0xFF223A2A),
    warningText = Color(0xFFFFCC66),
    warningSoft = Color(0xFF3C3222),
    dangerText = Color(0xFFFF8A80),
    dangerSoft = Color(0xFF3E2425),
)

private val MidnightPalette = BridgePalette(
    isDark = true,
    background = Color(0xFF0C1120),
    card = Color(0xFF131A2C),
    field = Color(0xFF1A2238),
    fieldStrong = Color(0xFF222B45),
    border = Color(0xFF222B44),
    text = Color(0xFFE9EDF8),
    muted = Color(0xFF9BA5C4),
    faint = Color(0xFF6B7596),
    accent = Color(0xFF5B6CF0),
    link = Color(0xFF6F7FF5),
    action = Color(0xFF4F5FE8),
    accentSoft = Color(0xFF1F274B),
    onAccentSoft = Color(0xFFC3CBFF),
    success = Color(0xFF2FD28A),
    successText = Color(0xFF5FE3A8),
    successSoft = Color(0xFF173439),
    warningText = Color(0xFFFFCF70),
    warningSoft = Color(0xFF2F2D2C),
    dangerText = Color(0xFFFF8F9B),
    dangerSoft = Color(0xFF342133),
)

internal fun paletteFor(choice: ThemeChoice, systemDark: Boolean): BridgePalette = when (choice) {
    ThemeChoice.SYSTEM -> if (systemDark) DarkPalette else LightPalette
    ThemeChoice.LIGHT -> LightPalette
    ThemeChoice.DARK -> DarkPalette
    ThemeChoice.MIDNIGHT -> MidnightPalette
    ThemeChoice.SAND -> SandPalette
}

private fun BridgePalette.toColorScheme(): ColorScheme {
    val base = if (isDark) darkColorScheme() else lightColorScheme()
    return base.copy(
        primary = link,
        onPrimary = if (isDark) background else Color.White,
        primaryContainer = accentSoft,
        onPrimaryContainer = onAccentSoft,
        inversePrimary = accent,
        secondary = muted,
        onSecondary = card,
        secondaryContainer = field,
        onSecondaryContainer = text,
        tertiary = successText,
        onTertiary = card,
        tertiaryContainer = successSoft,
        onTertiaryContainer = successText,
        error = dangerText,
        onError = if (isDark) background else Color.White,
        errorContainer = dangerSoft,
        onErrorContainer = dangerText,
        background = background,
        onBackground = text,
        surface = background,
        onSurface = text,
        surfaceVariant = field,
        onSurfaceVariant = muted,
        surfaceTint = accent,
        inverseSurface = text,
        inverseOnSurface = card,
        outline = faint,
        outlineVariant = border,
        scrim = Color.Black,
        surfaceBright = card,
        surfaceDim = background,
        surfaceContainerLowest = card,
        surfaceContainerLow = card,
        surfaceContainer = field,
        surfaceContainerHigh = card,
        surfaceContainerHighest = fieldStrong,
    )
}

private val LocalBridgeColors = staticCompositionLocalOf { LightPalette }

object BridgeTheme {
    val colors: BridgePalette
        @Composable
        @ReadOnlyComposable
        get() = LocalBridgeColors.current
}

private val BridgeTypography = Typography().let { base ->
    base.copy(
        headlineSmall = base.headlineSmall.copy(fontWeight = FontWeight.SemiBold),
        titleLarge = base.titleLarge.copy(fontWeight = FontWeight.SemiBold),
        titleMedium = base.titleMedium.copy(fontWeight = FontWeight.SemiBold),
        titleSmall = base.titleSmall.copy(fontWeight = FontWeight.SemiBold),
        labelLarge = base.labelLarge.copy(fontWeight = FontWeight.SemiBold),
    )
}

private val BridgeShapes = Shapes(
    extraSmall = RoundedCornerShape(8.dp),
    small = RoundedCornerShape(12.dp),
    medium = RoundedCornerShape(16.dp),
    large = RoundedCornerShape(20.dp),
    extraLarge = RoundedCornerShape(28.dp),
)

@Composable
fun DjiLiveBridgeTheme(choice: ThemeChoice, content: @Composable () -> Unit) {
    val palette = paletteFor(choice, isSystemInDarkTheme())
    val colorScheme = remember(palette) { palette.toColorScheme() }
    CompositionLocalProvider(LocalBridgeColors provides palette) {
        MaterialTheme(
            colorScheme = colorScheme,
            typography = BridgeTypography,
            shapes = BridgeShapes,
            content = content,
        )
    }
}
