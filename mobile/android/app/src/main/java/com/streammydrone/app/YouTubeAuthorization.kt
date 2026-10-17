package com.streammydrone.app

import android.accounts.Account
import android.content.Context
import com.google.android.gms.auth.api.identity.AuthorizationRequest
import com.google.android.gms.auth.api.identity.AuthorizationResult
import com.google.android.gms.auth.api.identity.ClearTokenRequest
import com.google.android.gms.auth.api.identity.Identity
import com.google.android.gms.auth.api.identity.RevokeAccessRequest
import com.google.android.gms.common.api.ApiException
import com.google.android.gms.common.api.Scope
import com.google.android.gms.tasks.Task
import com.google.android.gms.tasks.Tasks
import java.util.concurrent.TimeUnit

internal const val YOUTUBE_SCOPE = "https://www.googleapis.com/auth/youtube.force-ssl"
private const val EMAIL_SCOPE = "https://www.googleapis.com/auth/userinfo.email"

internal fun youTubeAuthorizationRequest(email: String? = null, chooseAccount: Boolean = false): AuthorizationRequest =
    AuthorizationRequest.builder()
        .setRequestedScopes(listOf(Scope(YOUTUBE_SCOPE), Scope(EMAIL_SCOPE)))
        .apply {
            if (email != null) setAccount(Account(email, "com.google"))
            if (chooseAccount) setPrompt(AuthorizationRequest.Prompt.SELECT_ACCOUNT)
        }.build()

internal fun AuthorizationResult.youTubeAccessToken(): String {
    if (hasResolution() || YOUTUBE_SCOPE !in grantedScopes) throw YouTubeException(YouTubeFailure.AUTH)
    return accessToken?.takeIf(String::isNotBlank) ?: throw YouTubeException(YouTubeFailure.AUTH)
}

/** For a service: never launches consent or an account picker from the background. */
internal fun authorizedYouTubeApi(context: Context, email: String): YouTubeApi {
    val client = Identity.getAuthorizationClient(context)
    val result = try {
        Tasks.await(client.authorize(youTubeAuthorizationRequest(email)), 30, TimeUnit.SECONDS)
    } catch (_: Exception) {
        throw YouTubeException(YouTubeFailure.AUTH)
    }
    val token = result.youTubeAccessToken()
    return YouTubeApi(GoogleYouTubeTransport(token) { clearYouTubeToken(context, token) })
}

/** Clear only invalid access tokens; Google Play services owns refresh and consent state. */
internal fun clearYouTubeToken(context: Context, token: String) {
    Identity.getAuthorizationClient(context).clearToken(ClearTokenRequest.builder().setToken(token).build())
}

internal fun revokeYouTubeAccess(context: Context, email: String): Task<Void> =
    Identity.getAuthorizationClient(context).revokeAccess(
        RevokeAccessRequest.builder().setAccount(Account(email, "com.google"))
            .setScopes(listOf(Scope(YOUTUBE_SCOPE), Scope(EMAIL_SCOPE))).build(),
    )

internal fun youTubeError(error: Throwable): UiText {
    if (error is DestinationProfileException) return error.text
    return uiText(when ((error as? YouTubeException)?.failure) {
    YouTubeFailure.AUTH -> R.string.youtube_error_auth
    YouTubeFailure.SETUP -> R.string.youtube_error_setup
    YouTubeFailure.CHANNEL -> R.string.youtube_error_channel
    YouTubeFailure.WRONG_CHANNEL -> R.string.youtube_error_channel_changed
    YouTubeFailure.LIVE_DISABLED -> R.string.youtube_error_live
    YouTubeFailure.QUOTA -> R.string.youtube_error_quota
    YouTubeFailure.NETWORK -> R.string.youtube_error_network
    YouTubeFailure.TITLE -> R.string.youtube_error_title
    else -> when {
        error is ApiException && error.statusCode == 10 -> R.string.youtube_error_setup
        error is ApiException -> R.string.youtube_error_auth
        else -> R.string.youtube_error_api
    }
    })
}
