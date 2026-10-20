package com.streammydrone.app

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.lifecycle.ViewModel

internal class ProfileEditorViewModel : ViewModel() {
    val instagramBrowser = InstagramBrowser()
    val instagramProducer = InstagramProducerSession()
    /** Explicit manual-save fallback is valid for this app session, without a retained WebView. */
    val instagramManualProfiles = mutableSetOf<String>()
    var broadcastTitle: String? = null
    var broadcastInstagramAudience = InstagramAudience.PRACTICE
    var pendingStart: BroadcastStartRequest? = null
    var state by mutableStateOf<ProfileEditorState?>(null)
        private set

    /** Opens the editor for [profile], or for a new destination on [kind] when [profile] is null. */
    fun open(profile: DestinationProfile?, kind: DestinationKind) {
        state?.clearSecret()
        state = ProfileEditorState(profile, profile?.kind ?: kind)
    }

    fun close() {
        state?.clearSecret()
        state = null
    }

    override fun onCleared() {
        instagramProducer.destroy()
        state?.clearSecret()
        state = null
        super.onCleared()
    }
}

internal class ProfileEditorState(
    val profile: DestinationProfile?,
    val kind: DestinationKind,
) {
    var youtubeSetup by mutableStateOf(kind == DestinationKind.YOUTUBE && (profile == null || profile.youtube != null))
    var facebookSetup by mutableStateOf(kind == DestinationKind.FACEBOOK && (profile == null || profile.facebook != null))
    var instagramManual by mutableStateOf(false)
    /** Only a Go live request, never an ordinary connection edit, resumes after capture. */
    var instagramStartRequest: BroadcastStartRequest? = null
    val youtube = YouTubeEditorState(profile?.youtube)
    val facebook = FacebookEditorState(profile?.facebook)
    val initialServerUrl = profile?.serverUrl ?: kind.defaultServerUrl.orEmpty()

    var serverUrl by mutableStateOf(initialServerUrl)
    var streamKey by mutableStateOf("")
    var keyVisible by mutableStateOf(false)

    /** The address stays folded away while it is the platform's published default. */
    var serverExpanded by mutableStateOf(
        kind.defaultServerUrl == null || !sameServerUrl(initialServerUrl, kind.defaultServerUrl),
    )
    var serverTouched by mutableStateOf(false)
    var keyTouched by mutableStateOf(false)
    var editorError by mutableStateOf<UiText?>(null)
    var showDiscardConfirmation by mutableStateOf(false)
    var showDeleteConfirmation by mutableStateOf(false)

    internal fun clearSecret() {
        instagramStartRequest = null
        youtube.clearSecret()
        streamKey = ""
        keyVisible = false
    }
}

internal fun sameServerUrl(first: String, second: String): Boolean =
    first.trim().trimEnd('/') == second.trim().trimEnd('/')
