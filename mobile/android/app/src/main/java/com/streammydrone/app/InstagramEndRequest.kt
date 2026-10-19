package com.streammydrone.app

/** Two independent one-shot claims: opening Instagram's end dialog and confirming it. */
internal class InstagramEndRequest(val profileId: String, val document: Int, val deadline: Long) {
    private val dispatched = mutableSetOf<String>()

    fun valid(now: Long, document: Int, url: String?): Boolean =
        now < deadline && this.document == document && instagramCaptureOrigin(url)

    fun claim(step: String): Boolean {
        if (step !in setOf("end", "confirm") || (step == "end" && "confirm" in dispatched)) return false
        return dispatched.add(step)
    }
}

/** Ending other platforms while Instagram finishes must not send the native stop-all command. */
internal fun relayEndTargets(ended: List<String>, remaining: List<String>): List<String> =
    if (ended.isEmpty()) emptyList() else if (remaining.isEmpty()) listOf("") else ended
