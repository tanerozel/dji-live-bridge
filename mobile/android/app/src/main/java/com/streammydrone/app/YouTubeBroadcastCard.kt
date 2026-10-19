package com.streammydrone.app

import android.net.Uri
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.repeatOnLifecycle
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.withContext

/** Read-only confirmation from YouTube. Never starts a broadcast or changes its audience. */
@Composable
internal fun YouTubeBroadcastCard(profile: DestinationProfile) {
    val connection = profile.youtube ?: return
    val context = LocalContext.current.applicationContext
    val lifecycle = LocalLifecycleOwner.current
    val uriHandler = LocalUriHandler.current
    var status by remember(profile.id) { mutableStateOf<YouTubeBroadcastStatus?>(null) }
    var error by remember(profile.id) { mutableStateOf<UiText?>(null) }
    LaunchedEffect(profile.id, connection, lifecycle) {
        lifecycle.repeatOnLifecycle(Lifecycle.State.STARTED) {
            // A previous check must not remain labelled live when resuming after backgrounding.
            status = null
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
                    status = next
                    error = null
                } catch (cancelled: CancellationException) {
                    throw cancelled
                } catch (failure: Exception) {
                    // Do not retain a stale live confirmation after a failed check.
                    status = null
                    error = youTubeError(failure)
                    if (failure is YouTubeException && failure.failure in setOf(YouTubeFailure.AUTH, YouTubeFailure.QUOTA)) break
                }
                delay(15_000)
            }
        }
    }
    val current = status
    val privacy = when (current?.privacy ?: connection.privacy) {
        "public" -> R.string.youtube_public
        "private" -> R.string.youtube_private
        else -> R.string.youtube_unlisted
    }
    BridgeCard {
        Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
            Text("YouTube · ${stringResource(privacy)}", style = MaterialTheme.typography.titleSmall)
            Text(stringResource(when (current?.lifecycle) {
                "live" -> R.string.youtube_status_live
                "testing", "testStarting" -> R.string.youtube_status_testing
                "complete", "revoked" -> R.string.youtube_status_ended
                else -> R.string.youtube_status_pending
            }), style = MaterialTheme.typography.bodySmall)
            error?.let { Text(it.asString(), style = MaterialTheme.typography.bodySmall, color = BridgeTheme.colors.dangerText) }
            TextButton(onClick = {
                val url = current?.id?.let {
                    Uri.Builder().scheme("https").authority("www.youtube.com").path("/watch").appendQueryParameter("v", it).build().toString()
                } ?: "https://studio.youtube.com/"
                runCatching { uriHandler.openUri(url) }.onFailure { error = uiText(R.string.youtube_error_api) }
            }) { Text(stringResource(R.string.youtube_open_broadcast)) }
        }
    }
}
