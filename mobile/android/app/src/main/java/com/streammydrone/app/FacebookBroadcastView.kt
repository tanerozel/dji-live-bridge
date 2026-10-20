package com.streammydrone.app

import androidx.annotation.StringRes
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.platform.LocalContext
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.repeatOnLifecycle
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.withContext

/** What Facebook last said about one connected profile's live video. */
@Stable
internal class FacebookBroadcastView(val connection: FacebookConnection) {
    var status by mutableStateOf<FacebookLiveStatus?>(null)
    var error by mutableStateOf<UiText?>(null)

    val live: Boolean get() = status?.status == "LIVE"

    @get:StringRes
    val privacy: Int get() = when {
        connection.page -> R.string.facebook_public
        connection.privacy == FacebookPrivacy.SELF.apiValue -> R.string.facebook_only_me
        connection.privacy == FacebookPrivacy.ALL_FRIENDS.apiValue -> R.string.facebook_friends
        else -> R.string.facebook_public
    }

    @get:StringRes
    val lifecycle: Int get() = when (status?.status) {
        "LIVE" -> R.string.facebook_status_live
        "LIVE_STOPPED", "VOD", "PROCESSING" -> R.string.facebook_status_ended
        else -> R.string.facebook_status_pending
    }

    /** The live video once Facebook named it, the profile or Page until then. */
    val url: String get() = status?.permalink ?: "https://www.facebook.com/${connection.targetId}"
}

/** Read-only confirmation from Facebook. Never starts a live video or changes its audience. */
@Composable
internal fun rememberFacebookBroadcast(profile: DestinationProfile, connection: FacebookConnection): FacebookBroadcastView {
    val context = LocalContext.current.applicationContext
    val lifecycle = LocalLifecycleOwner.current
    val view = remember(profile.id, connection) { FacebookBroadcastView(connection) }
    LaunchedEffect(view, lifecycle) {
        lifecycle.repeatOnLifecycle(Lifecycle.State.STARTED) {
            // A previous check must not remain labelled live when resuming after backgrounding.
            view.status = null
            val api = FacebookApi(GraphFacebookTransport())
            // A Page's token is fetched once per screen visit; Graph API limits calls per user and hour.
            var token: String? = null
            while (true) {
                try {
                    val next = withContext(Dispatchers.IO) {
                        val session = FacebookSessionStore(context).load().firstOrNull { it.profileId == profile.id }
                        if (session == null) null else {
                            if (session.userId != connection.userId || session.targetId != connection.targetId) {
                                throw FacebookLiveException(FacebookFailure.WRONG_ACCOUNT)
                            }
                            val current = token ?: api.targetToken(connection.targetId, connection.page,
                                facebookUserToken(context, connection.userId, connection.page)).also { token = it }
                            api.status(session.liveVideoId, current)
                        }
                    }
                    view.status = next
                    view.error = null
                } catch (cancelled: CancellationException) {
                    throw cancelled
                } catch (failure: Exception) {
                    // Do not retain a stale live confirmation after a failed check.
                    view.status = null
                    view.error = facebookError(failure)
                    token = null
                    if (failure is FacebookLiveException && failure.failure in setOf(FacebookFailure.AUTH, FacebookFailure.RATE)) break
                }
                delay(30_000)
            }
        }
    }
    return view
}
