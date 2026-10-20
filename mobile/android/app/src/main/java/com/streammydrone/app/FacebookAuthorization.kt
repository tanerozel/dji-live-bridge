package com.streammydrone.app

import android.content.Context

/**
 * Granted through the Facebook Login for Business configuration (`facebookLoginConfigId`).
 * `publish_video` creates live videos on the user's profile; the `pages_*` permissions list the
 * user's Pages and publish to them.
 */
private val PROFILE_PERMISSIONS = setOf("publish_video")
private val PAGE_PERMISSIONS = setOf("pages_show_list", "pages_read_engagement", "pages_manage_posts")

/** The App ID and login configuration come from Gradle properties; see FACEBOOK_SETUP.md. */
internal val facebookConfigured: Boolean
    get() = BuildConfig.FACEBOOK_APP_ID.isNotBlank() && BuildConfig.FACEBOOK_LOGIN_CONFIG_ID.isNotBlank()

internal fun facebookLoginAllows(login: FacebookLogin, page: Boolean): Boolean =
    login.scopes.containsAll(if (page) PAGE_PERMISSIONS else PROFILE_PERMISSIONS)

/**
 * Finishes a login from Facebook's redirect: checks the state, asks Facebook who logged in and
 * what was granted (a business login does not list it in the redirect), then saves it. Blocking.
 */
internal fun completeFacebookLogin(context: Context, callbackUrl: String, state: String): Pair<FacebookUser, FacebookLogin> {
    val callback = parseFacebookCallback(callbackUrl, state)
    val api = FacebookApi(GraphFacebookTransport())
    val user = api.me(callback.token)
    val login = FacebookLogin(user.id, callback.token, callback.expiresAtMillis, api.grantedPermissions(callback.token))
    try {
        FacebookLoginStore(context).save(login)
    } catch (_: Exception) {
        throw FacebookLiveException(FacebookFailure.AUTH)
    }
    return user to login
}

/** The saved login, if it is still valid. */
internal fun currentFacebookLogin(context: Context): FacebookLogin? = FacebookLoginStore(context).load()?.takeIf { it.usable() }

/** For a service: the saved login's token, if it still belongs to [userId] and may publish to a profile or [page]. */
internal fun facebookUserToken(context: Context, userId: String, page: Boolean): String {
    val login = currentFacebookLogin(context) ?: throw FacebookLiveException(FacebookFailure.AUTH)
    if (login.userId != userId) throw FacebookLiveException(FacebookFailure.WRONG_ACCOUNT)
    if (!facebookLoginAllows(login, page)) throw FacebookLiveException(FacebookFailure.AUTH)
    return login.token
}

/** Removes this app's access on Facebook's side, then forgets the login. Blocking. */
internal fun revokeFacebookAccess(context: Context) {
    val store = FacebookLoginStore(context)
    store.load()?.let { login ->
        try {
            FacebookApi(GraphFacebookTransport()).revokePermissions(login.token)
        } catch (failure: FacebookLiveException) {
            // An already revoked or expired login has no access left to remove.
            if (failure.failure != FacebookFailure.AUTH) throw failure
        }
    }
    store.delete()
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
