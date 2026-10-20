package com.streammydrone.app

import android.content.Context
import com.facebook.AccessToken
import com.facebook.FacebookSdk
import com.facebook.login.LoginManager

/**
 * Asked for once in Facebook's own login window. `publish_video` creates live videos on the
 * user's profile; the `pages_*` permissions list the user's Pages and publish to them.
 */
internal val FACEBOOK_PERMISSIONS = listOf(
    "public_profile", "publish_video", "pages_show_list", "pages_read_engagement", "pages_manage_posts",
)

private val PROFILE_PERMISSIONS = setOf("publish_video")
private val PAGE_PERMISSIONS = setOf("pages_show_list", "pages_read_engagement", "pages_manage_posts")

/** The App ID and client token come from Gradle properties; see FACEBOOK_SETUP.md. */
internal val facebookConfigured: Boolean
    get() = BuildConfig.FACEBOOK_APP_ID.isNotBlank() && BuildConfig.FACEBOOK_CLIENT_TOKEN.isNotBlank()

/**
 * The SDK is only initialised, never fully: no app events, advertiser ID or automatic start-up
 * requests (see the manifest). It is used for nothing but login and its token cache.
 */
internal fun ensureFacebookSdk(context: Context) {
    if (!facebookConfigured) throw FacebookLiveException(FacebookFailure.SETUP)
    if (!FacebookSdk.isInitialized()) {
        try {
            @Suppress("DEPRECATION")
            FacebookSdk.sdkInitialize(context.applicationContext)
        } catch (_: Exception) {
            throw FacebookLiveException(FacebookFailure.SETUP)
        }
    }
    FacebookSdk.setAutoLogAppEventsEnabled(false)
    FacebookSdk.setAdvertiserIDCollectionEnabled(false)
}

/** For a service: the SDK's cached token, if it still belongs to [userId] and may publish to a profile or [page]. */
internal fun facebookUserToken(context: Context, userId: String, page: Boolean): String {
    ensureFacebookSdk(context)
    val token = AccessToken.getCurrentAccessToken()?.takeIf { !it.isExpired && !it.isDataAccessExpired }
        ?: throw FacebookLiveException(FacebookFailure.AUTH)
    if (token.userId != userId) throw FacebookLiveException(FacebookFailure.WRONG_ACCOUNT)
    if (!facebookTokenAllows(token, page)) throw FacebookLiveException(FacebookFailure.AUTH)
    return token.token
}

internal fun facebookTokenAllows(token: AccessToken, page: Boolean): Boolean =
    token.permissions.containsAll(if (page) PAGE_PERMISSIONS else PROFILE_PERMISSIONS)

/** Removes this app's access on Facebook's side, then forgets the cached token. Blocking. */
internal fun revokeFacebookAccess(context: Context) {
    ensureFacebookSdk(context)
    AccessToken.getCurrentAccessToken()?.let { token ->
        try {
            FacebookApi(GraphFacebookTransport()).revokePermissions(token.token)
        } catch (failure: FacebookLiveException) {
            // An already revoked or expired token has no access left to remove.
            if (failure.failure != FacebookFailure.AUTH) throw failure
        }
    }
    LoginManager.getInstance().logOut()
}

internal fun facebookError(error: Throwable): UiText {
    if (error is DestinationProfileException) return error.text
    if (error is YouTubeException && error.failure == YouTubeFailure.TITLE) return uiText(R.string.youtube_error_title)
    return uiText(when ((error as? FacebookLiveException)?.failure) {
        FacebookFailure.AUTH -> R.string.facebook_error_auth
        FacebookFailure.SETUP -> R.string.facebook_error_setup
        FacebookFailure.WRONG_ACCOUNT -> R.string.facebook_error_account
        FacebookFailure.PAGE -> R.string.facebook_error_page
        FacebookFailure.LIVE_DISABLED -> R.string.facebook_error_live
        FacebookFailure.RATE -> R.string.facebook_error_rate
        FacebookFailure.NETWORK -> R.string.facebook_error_network
        else -> R.string.facebook_error_api
    })
}
