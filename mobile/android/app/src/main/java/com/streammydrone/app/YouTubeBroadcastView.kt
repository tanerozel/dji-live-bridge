package com.streammydrone.app

import android.net.Uri
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

/** What YouTube last said about one connected profile's broadcast. */
@Stable
internal class YouTubeBroadcastView(val connection: YouTubeConnection) {
    var status by mutableStateOf<YouTubeBroadcastStatus?>(null)
    var error by mutableStateOf<UiText?>(null)

    val live: Boolean get() = status?.lifecycle == "live"

    @get:StringRes
    val privacy: Int get() = when (status?.privacy ?: connection.privacy) {
        "public" -> R.string.youtube_public
        "private" -> R.string.youtube_private
        else -> R.string.youtube_unlisted
    }

    @get:StringRes
    val lifecycle: Int get() = when (status?.lifecycle) {
        "live" -> R.string.youtube_status_live
        "testing", "testStarting" -> R.string.youtube_status_testing
        "complete", "revoked" -> R.string.youtube_status_ended
        else -> R.string.youtube_status_pending
    }

    /** The broadcast's watch page once YouTube named it, YouTube Studio until then. */
    val url: String get() = status?.id?.let {
        Uri.Builder().scheme("https").authority("www.youtube.com").path("/watch").appendQueryParameter("v", it).build().toString()
    } ?: "https://studio.youtube.com/"
}

/** Read-only confirmation from YouTube. Never starts a broadcast or changes its audience. */
@Composable
internal fun rememberYouTubeBroadcast(profile: DestinationProfile, connection: YouTubeConnection): YouTubeBroadcastView {
    val context = LocalContext.current.applicationContext
    val lifecycle = LocalLifecycleOwner.current
    val view = remember(profile.id, connection) { YouTubeBroadcastView(connection) }
    LaunchedEffect(view, lifecycle) {
        lifecycle.repeatOnLifecycle(Lifecycle.State.STARTED) {
            // A previous check must not remain labelled live when resuming after backgrounding.
            view.status = null
            while (true) {
                try {
                    val next = withContext(Dispatchers.IO) {
                        val session = YouTubeSessionStore(context).load().firstOrNull { it.profileId == profile.id }
                        if (session == null) null else {
                            if (session.channelId != connection.channelId || session.email != connection.accountEmail) {
                                throw YouTubeException(YouTubeFailure.WRONG_CHANNEL)
                            }
                            authorizedYouTubeApi(context, connection.accountEmail)
                                .broadcastStatus(session.broadcastId, connection.channelId, connection.streamId)
                        }
                    }
                    view.status = next
                    view.error = null
                } catch (cancelled: CancellationException) {
                    throw cancelled
                } catch (failure: Exception) {
                    // Do not retain a stale live confirmation after a failed check.
                    view.status = null
                    view.error = youTubeError(failure)
                    if (failure is YouTubeException && failure.failure in setOf(YouTubeFailure.AUTH, YouTubeFailure.QUOTA)) break
                }
                delay(15_000)
            }
        }
    }
    return view
}
