package com.djilivebridge.android

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.viewModels

class MainActivity : ComponentActivity() {
    private val profileEditorViewModel by viewModels<ProfileEditorViewModel>()

    override fun onCreate(savedInstanceState: Bundle?) {
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)
        setContent {
            DjiLiveBridgeTheme {
                RelayScreen(profileEditorViewModel)
            }
        }
    }
}
