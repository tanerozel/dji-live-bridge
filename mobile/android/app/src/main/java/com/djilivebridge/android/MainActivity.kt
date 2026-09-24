package com.djilivebridge.android

import android.graphics.Color
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.SystemBarStyle
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.viewModels
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue

class MainActivity : ComponentActivity() {
    private val profileEditorViewModel by viewModels<ProfileEditorViewModel>()

    override fun onCreate(savedInstanceState: Bundle?) {
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)
        val preferences = UiPreferences(applicationContext)
        setContent {
            var theme by remember { mutableStateOf(preferences.theme) }
            var droneScreen by remember { mutableStateOf(false) }
            DjiLiveBridgeTheme(theme) {
                // The chosen theme, not the system setting, decides the status bar icon color; the
                // drone's picture always needs light icons.
                val dark = BridgeTheme.colors.isDark || droneScreen
                LaunchedEffect(dark) {
                    val style = if (dark) {
                        SystemBarStyle.dark(Color.TRANSPARENT)
                    } else {
                        SystemBarStyle.light(Color.TRANSPARENT, Color.TRANSPARENT)
                    }
                    enableEdgeToEdge(statusBarStyle = style, navigationBarStyle = style)
                }
                RelayScreen(
                    profileEditorViewModel = profileEditorViewModel,
                    theme = theme,
                    onThemeChange = { choice ->
                        theme = choice
                        preferences.theme = choice
                    },
                    onDroneScreenChange = { droneScreen = it },
                )
            }
        }
    }
}
