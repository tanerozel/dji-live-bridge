package com.streammydrone.app

/** One confirmed title, retained through permissions/setup and reused when adding a platform. */
internal class BroadcastStartRequest(profileIds: List<String>, title: String, val instagramAudience: InstagramAudience = InstagramAudience.PRACTICE) {
    val profileIds = profileIds.distinct().toList()
    val title = validateBroadcastTitle(title)
}

internal fun validateBroadcastTitle(value: String): String = validateYouTubeTitle(value)

internal fun initialBroadcastTitle(saved: String?, profiles: List<DestinationProfile>): String =
    (listOfNotNull(saved) + profiles.mapNotNull { it.youtube?.title ?: it.facebook?.title } + "Stream My Drone")
        .firstNotNullOf { runCatching { validateBroadcastTitle(it) }.getOrNull() }

/** The native Start confirmation always names the audience; first use defaults to Practice. */
internal enum class InstagramAudience(val label: Int, val hint: Int) {
    PRACTICE(R.string.instagram_audience_practice, R.string.instagram_audience_practice_hint),
    PUBLIC(R.string.instagram_audience_public, R.string.instagram_audience_public_hint);

    companion object {
        fun fromStorage(value: String?): InstagramAudience = entries.firstOrNull { it.name == value } ?: PRACTICE
    }
}
