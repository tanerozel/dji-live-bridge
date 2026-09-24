package com.djilivebridge.android

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.lifecycle.ViewModel

internal class ProfileEditorViewModel : ViewModel() {
    var state by mutableStateOf<ProfileEditorState?>(null)
        private set

    /** Opens the editor for [profile], or for a new profile of [kind] when [profile] is null. */
    fun open(profile: DestinationProfile?, kind: DestinationKind = DestinationKind.CUSTOM) {
        state?.clearSecret()
        state = ProfileEditorState(profile, kind)
    }

    fun close() {
        state?.clearSecret()
        state = null
    }

    override fun onCleared() {
        state?.clearSecret()
        state = null
        super.onCleared()
    }
}

internal class ProfileEditorState(
    val profile: DestinationProfile?,
    newProfileKind: DestinationKind = DestinationKind.CUSTOM,
) {
    private val suggestedName = if (profile == null) newProfileKind.label else null
    private val suggestedServerUrl = if (profile == null) newProfileKind.defaultServerUrl else null

    var name by mutableStateOf(profile?.name ?: suggestedName.orEmpty())
    var kind by mutableStateOf(profile?.kind ?: newProfileKind)
    var serverUrl by mutableStateOf(profile?.serverUrl ?: suggestedServerUrl.orEmpty())
    var targetStreamKey by mutableStateOf("")
    var passwordVisible by mutableStateOf(false)
    var editorError by mutableStateOf<String?>(null)
    var nameTouched by mutableStateOf(false)
    var serverTouched by mutableStateOf(false)
    var keyTouched by mutableStateOf(false)
    var showDiscardConfirmation by mutableStateOf(false)
    var showDeleteConfirmation by mutableStateOf(false)
    var autoFilledName by mutableStateOf(suggestedName)
    var autoFilledServerUrl by mutableStateOf(suggestedServerUrl)

    internal fun clearSecret() {
        targetStreamKey = ""
        passwordVisible = false
    }
}

/** The fixed ingest address for platforms that have one; others hand out per-account URLs. */
internal val DestinationKind.defaultServerUrl: String?
    get() = when (this) {
        DestinationKind.YOUTUBE -> "rtmps://a.rtmps.youtube.com/live2"
        DestinationKind.TIKTOK, DestinationKind.CUSTOM -> null
    }
