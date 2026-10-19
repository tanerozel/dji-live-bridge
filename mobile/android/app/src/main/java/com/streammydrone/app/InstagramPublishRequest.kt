package com.streammydrone.app

/** One explicit app Start action; never persisted or retried after a document change. */
internal class InstagramPublishRequest(
    val profileId: String,
    val document: Int,
    val url: String,
    val deadline: Long,
) {
    var clickDispatched = false
        private set

    fun valid(now: Long, active: Boolean, document: Int, url: String?): Boolean =
        active && now < deadline && this.document == document &&
            (this.url == url || (clickDispatched && instagramCaptureOrigin(url)))

    /** Claim before evaluating JS: a missing callback must never cause a second click. */
    fun claimClick(): Boolean {
        if (clickDispatched) return false
        clickDispatched = true
        return true
    }
}

/** Positive evidence from the retained Producer, separate from RTMP delivery. Never persisted. */
internal data class InstagramPublicationConfirmation(val profileId: String, val sinceElapsedMillis: Long)

internal fun instagramPublicationConfirmed(
    profile: DestinationProfile,
    output: OutputSnapshot?,
    confirmation: InstagramPublicationConfirmation?,
): Boolean = profile.kind == DestinationKind.INSTAGRAM && instagramConfirmedOutput(profile.id, output, confirmation)

internal fun instagramConfirmedOutput(
    profileId: String,
    output: OutputSnapshot?,
    confirmation: InstagramPublicationConfirmation?,
): Boolean = confirmation?.profileId == profileId && output?.id == profileId &&
    output.status in LIVE_OUTPUT_STATUSES && output.outboundBytes > 0
