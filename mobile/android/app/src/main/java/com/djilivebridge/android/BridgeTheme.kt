package com.djilivebridge.android

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Shapes
import androidx.compose.material3.Typography
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.ReadOnlyComposable
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp

/** A status color plus the colors that sit on it and around it. */
@Immutable
data class StatusColors(
    val color: Color,
    val onColor: Color,
    val container: Color,
    val onContainer: Color,
)

/**
 * Colors the app needs beyond Material's roles. The palette follows the desktop app and the
 * website: one brand blue, cool neutrals tinted with the same hue, and green/amber/red reserved
 * for relay states so they always mean something.
 */
@Immutable
data class BridgeColors(
    val success: StatusColors,
    val warning: StatusColors,
    val live: StatusColors,
    val card: Color,
    val cardBorder: Color,
    val tip: Color,
    val onTip: Color,
    /** Filled call-to-action buttons: the vivid brand blue with white text in both themes. */
    val action: Color,
    val onAction: Color,
    val brandStart: Color,
    val brandEnd: Color,
)

private val BrandStart = Color(0xFF4AA8FF)
private val BrandEnd = Color(0xFF0A5CF0)

private val LightColorScheme = lightColorScheme(
    primary = Color(0xFF0A5CF0),
    onPrimary = Color(0xFFFFFFFF),
    primaryContainer = Color(0xFFDBE6FF),
    onPrimaryContainer = Color(0xFF002A70),
    inversePrimary = Color(0xFFA9C5FF),
    secondary = Color(0xFF4B5B79),
    onSecondary = Color(0xFFFFFFFF),
    secondaryContainer = Color(0xFFE2E8F5),
    onSecondaryContainer = Color(0xFF1B2940),
    tertiary = Color(0xFF9A4A00),
    onTertiary = Color(0xFFFFFFFF),
    tertiaryContainer = Color(0xFFFFE3CC),
    onTertiaryContainer = Color(0xFF331400),
    error = Color(0xFFC4281C),
    onError = Color(0xFFFFFFFF),
    errorContainer = Color(0xFFFFE4E1),
    onErrorContainer = Color(0xFF5C0A04),
    background = Color(0xFFF4F6FB),
    onBackground = Color(0xFF121826),
    surface = Color(0xFFF4F6FB),
    onSurface = Color(0xFF121826),
    surfaceVariant = Color(0xFFE6EAF2),
    onSurfaceVariant = Color(0xFF5A6478),
    surfaceTint = Color(0xFF0A5CF0),
    inverseSurface = Color(0xFF252C3A),
    inverseOnSurface = Color(0xFFEEF2F9),
    outline = Color(0xFF8A93A5),
    outlineVariant = Color(0xFFD8DDE7),
    scrim = Color(0xFF000000),
    surfaceBright = Color(0xFFF4F6FB),
    surfaceDim = Color(0xFFD5DAE4),
    surfaceContainerLowest = Color(0xFFFFFFFF),
    surfaceContainerLow = Color(0xFFEFF2F8),
    surfaceContainer = Color(0xFFE9EDF5),
    surfaceContainerHigh = Color(0xFFE3E8F1),
    surfaceContainerHighest = Color(0xFFDDE3ED),
)

private val DarkColorScheme = darkColorScheme(
    primary = Color(0xFF8FB4FF),
    onPrimary = Color(0xFF002B6B),
    primaryContainer = Color(0xFF15428F),
    onPrimaryContainer = Color(0xFFDCE6FF),
    inversePrimary = Color(0xFF0A5CF0),
    secondary = Color(0xFFB7C4DD),
    onSecondary = Color(0xFF1F2B41),
    secondaryContainer = Color(0xFF2B3549),
    onSecondaryContainer = Color(0xFFD9E2F4),
    tertiary = Color(0xFFFFB77A),
    onTertiary = Color(0xFF4D2600),
    tertiaryContainer = Color(0xFF6A3A0B),
    onTertiaryContainer = Color(0xFFFFDCC1),
    error = Color(0xFFFFB4AB),
    onError = Color(0xFF690005),
    errorContainer = Color(0xFF7D1C15),
    onErrorContainer = Color(0xFFFFDAD5),
    background = Color(0xFF0D1118),
    onBackground = Color(0xFFE5E9F1),
    surface = Color(0xFF0D1118),
    onSurface = Color(0xFFE5E9F1),
    surfaceVariant = Color(0xFF283041),
    onSurfaceVariant = Color(0xFFA7B1C3),
    surfaceTint = Color(0xFF8FB4FF),
    inverseSurface = Color(0xFFE5E9F1),
    inverseOnSurface = Color(0xFF1D2432),
    outline = Color(0xFF7B8598),
    outlineVariant = Color(0xFF2E3748),
    scrim = Color(0xFF000000),
    surfaceBright = Color(0xFF313949),
    surfaceDim = Color(0xFF0D1118),
    surfaceContainerLowest = Color(0xFF080B10),
    surfaceContainerLow = Color(0xFF131822),
    surfaceContainer = Color(0xFF171D28),
    surfaceContainerHigh = Color(0xFF1E2532),
    surfaceContainerHighest = Color(0xFF272F3D),
)

// Every text/background pair below meets WCAG AA (4.5:1) in its theme.
private val LightBridgeColors = BridgeColors(
    success = StatusColors(Color(0xFF1B7F46), Color.White, Color(0xFFDBF5E4), Color(0xFF0B3A20)),
    warning = StatusColors(Color(0xFFA35A00), Color.White, Color(0xFFFFEFD6), Color(0xFF3F2400)),
    live = StatusColors(Color(0xFFE0263E), Color.White, Color(0xFFFFE1E4), Color(0xFF5E0414)),
    card = Color(0xFFFFFFFF),
    cardBorder = Color(0xFFE2E7F0),
    tip = Color(0xFFE8F0FF),
    onTip = Color(0xFF0B2A66),
    action = Color(0xFF0A5CF0),
    onAction = Color.White,
    brandStart = BrandStart,
    brandEnd = BrandEnd,
)

private val DarkBridgeColors = BridgeColors(
    success = StatusColors(Color(0xFF5AD58C), Color(0xFF00391D), Color(0xFF123D27), Color(0xFFBDF0CF)),
    warning = StatusColors(Color(0xFFF6BD4F), Color(0xFF3F2400), Color(0xFF3D2B05), Color(0xFFFFE4AD)),
    live = StatusColors(Color(0xFFE0263E), Color.White, Color(0xFF4A0F19), Color(0xFFFFD9DE)),
    card = Color(0xFF171D28),
    cardBorder = Color(0xFF232B3A),
    tip = Color(0xFF1A2742),
    onTip = Color(0xFFD6E2FA),
    action = Color(0xFF1F5BEA),
    onAction = Color.White,
    brandStart = BrandStart,
    brandEnd = BrandEnd,
)

private val LocalBridgeColors = staticCompositionLocalOf { LightBridgeColors }

object BridgeTheme {
    /** App-specific colors paired with the active [DjiLiveBridgeTheme]. */
    val colors: BridgeColors
        @Composable
        @ReadOnlyComposable
        get() = LocalBridgeColors.current
}

private val BridgeTypography = Typography().let { base ->
    base.copy(
        headlineMedium = base.headlineMedium.copy(fontWeight = FontWeight.SemiBold),
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
    large = RoundedCornerShape(24.dp),
    extraLarge = RoundedCornerShape(28.dp),
)

/** Light or dark following the system setting; both are tuned for outdoor readability. */
@Composable
fun DjiLiveBridgeTheme(
    darkTheme: Boolean = isSystemInDarkTheme(),
    content: @Composable () -> Unit,
) {
    CompositionLocalProvider(
        LocalBridgeColors provides if (darkTheme) DarkBridgeColors else LightBridgeColors,
    ) {
        MaterialTheme(
            colorScheme = if (darkTheme) DarkColorScheme else LightColorScheme,
            typography = BridgeTypography,
            shapes = BridgeShapes,
            content = content,
        )
    }
}
