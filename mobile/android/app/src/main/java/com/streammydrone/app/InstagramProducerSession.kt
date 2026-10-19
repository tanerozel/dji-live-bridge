package com.streammydrone.app

import android.annotation.SuppressLint
import android.content.Context
import android.content.MutableContextWrapper
import android.graphics.Bitmap
import android.net.http.SslError
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import android.util.Log
import android.view.View
import android.view.ViewGroup
import android.webkit.ConsoleMessage
import android.webkit.CookieManager
import android.webkit.PermissionRequest
import android.webkit.RenderProcessGoneDetail
import android.webkit.SslErrorHandler
import android.webkit.WebChromeClient
import android.webkit.WebResourceError
import android.webkit.WebResourceRequest
import android.webkit.WebSettings
import android.webkit.WebView
import android.webkit.WebViewClient
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import java.lang.ref.WeakReference
import java.net.URI
import java.util.UUID
import org.json.JSONObject
import org.json.JSONTokener

internal class InstagramCredentials(val serverUrl: String, val streamKey: String, val titleMatches: Boolean = false, val audienceMatches: Boolean = false)

internal fun instagramCaptureOrigin(url: String?): Boolean = try {
    val uri = URI(url.orEmpty())
    uri.scheme == "https" && uri.host == "www.instagram.com" && uri.rawUserInfo == null && uri.port in listOf(-1, 443) &&
        !Regex("^/(accounts|challenge|checkpoint)(/|$)").containsMatchIn(uri.path.orEmpty())
} catch (_: Exception) { false }

internal fun instagramNavigationAllowed(url: String): Boolean = try {
    val uri = URI(url)
    uri.scheme == "https" && uri.rawUserInfo == null && uri.port in listOf(-1, 443) &&
        uri.host in setOf("www.instagram.com", "instagram.com", "www.facebook.com", "m.facebook.com")
} catch (_: Exception) { false }

internal fun decodeInstagramCapture(raw: String?): InstagramCredentials? = try {
    if (raw == null || raw.length > 8192) null else {
        val text = JSONTokener(raw).nextValue() as? String
        val json = text?.let(::JSONObject)
        if (json?.optString("status") != "ready" || json.optString("origin") != "https://www.instagram.com") null
        else InstagramCredentials(validateInstagramServer(json.getString("serverUrl")), validateInstagramKey(json.getString("streamKey")), json.optBoolean("titleMatches"), json.optBoolean("audienceMatches"))
    }
} catch (_: Exception) { null }

/** Retains the Producer page when the modal hides, so capturing a key does not close its tab. */
internal class InstagramProducerSession {
    var visible by mutableStateOf(false)
        private set
    var loading by mutableStateOf(true)
        private set
    var pageError by mutableStateOf(false)
        private set
    var host by mutableStateOf("instagram.com")
        private set
    var producerUnavailable by mutableStateOf(false)
        private set
    var captureTarget: ProfileEditorState? = null
        private set
    var hasPage by mutableStateOf(false)
        private set
    var publishStatus by mutableStateOf<Int?>(null)
        private set
    var publishing by mutableStateOf(false)
        private set
    var ending by mutableStateOf(false)
        private set
    var preparing by mutableStateOf(false)
        private set
    private var automaticSetup = false
    private var setupDeadline = 0L
    private var setupEvaluating = false
    private var setupScript = ""
    private var diagnoseScript = ""
    private var diagnosed = false
    private var lastSetupState: String? = null
    private var desiredAudience = InstagramAudience.PRACTICE
    private var preparedAudience: InstagramAudience? = null
    private var endScript = ""
    private var pendingEnd: InstagramEndRequest? = null
    private var endEvaluating = false
    private var endFinished: ((Boolean) -> Unit)? = null
    private var lastEndProfileId: String? = null
    private val endToken = UUID.randomUUID().toString()
    private var endAttempt = 0
    private val endTick = object : Runnable {
        override fun run() {
            pollEnd()
            if (pendingEnd != null) handler.postDelayed(this, 250)
        }
    }
    private var document = 0
    private var publishScript = ""
    private var pendingPublish: InstagramPublishRequest? = null
    var publicationConfirmation by mutableStateOf<InstagramPublicationConfirmation?>(null)
        private set
    private var lastPublishState: String? = null
    private var publishEvaluationSerial = 0
    private var publishEvaluationDeadline = 0L
    private var publishEvaluating = false
    private val handler by lazy { Handler(Looper.getMainLooper()) }
    private val publishTick = object : Runnable {
        override fun run() {
            pollPublish()
            if (pendingPublish != null || observableProfileId() != null) {
                handler.postDelayed(this, if (pendingPublish != null) 1_000 else 3_000)
            }
        }
    }
    private var web: WebView? = null
    private var wrapper: MutableContextWrapper? = null
    private var application: Context? = null
    private var script = ""
    private var openScript = ""
    private var titleScript = ""
    private var desiredTitle = "Stream My Drone"
    private var titleRevision = UUID.randomUUID().toString()
    private var forceTitle = false
    private var preparedTitle: String? = null
    var titleTooLong by mutableStateOf(false)
        private set
    private var fillingTitle = false
    private var opening = false
    private var openAttempts = 0
    private var producerOpened = false
    private var createOpened = false
    private var lastOpenState: String? = null
    private var connectedProfileId: String? = null

    fun setupTitle(title: String, force: Boolean = false) {
        val normalized = validateBroadcastTitle(title)
        if (normalized != desiredTitle || force != forceTitle) {
            desiredTitle = normalized
            titleRevision = UUID.randomUUID().toString()
            forceTitle = force
            titleTooLong = false
        }
    }

    fun rejectsTitle(title: String): Boolean = titleTooLong && desiredTitle == title

    fun prepareForTitle(profileId: String, title: String, audience: InstagramAudience = InstagramAudience.PRACTICE): Boolean {
        val normalized = validateBroadcastTitle(title)
        if (hasConnection(profileId) && preparedTitle == normalized && preparedAudience == audience) return true
        desiredAudience = audience
        setupTitle(normalized, force = true)
        return false
    }

    fun hasConnection(profileId: String): Boolean = connectedProfileId == profileId && web != null && !pageError
    fun connected(profileId: String) {
        connectedProfileId = profileId
        publicationConfirmation = null
        lastEndProfileId = null
        publishStatus = null
        retained = WeakReference(this)
    }
    private var generation = 0
    private var evaluating = false

    /** Hidden preparation starts only after a native Connect/Start confirmation. */
    fun prepare(target: ProfileEditorState, title: String, audience: InstagramAudience) {
        setupTitle(title, force = true)
        desiredAudience = audience
        // Every native confirmation owns fresh title/audience proof and one-shot setup actions.
        titleRevision = UUID.randomUUID().toString()
        resetSetup(target)
        automaticSetup = true
        preparing = true
        visible = false
        setupDeadline = SystemClock.elapsedRealtime() + 60_000
        // A canceled/old draft may still display its key; login cookies survive this fresh draft.
        connectedProfileId = null
        preparedTitle = null
        preparedAudience = null
        web?.loadUrl("https://www.instagram.com/")
    }

    private fun resetSetup(target: ProfileEditorState?) {
        captureTarget = target
        diagnosed = false
        lastSetupState = null
        setupEvaluating = false
        evaluating = false
        opening = false
        fillingTitle = false
        openAttempts = 0
        producerOpened = false
        createOpened = false
        producerUnavailable = false
        lastOpenState = null
        generation++
    }

    fun show(target: ProfileEditorState?) {
        if (preparing && target === captureTarget) { visible = true; return }
        resetSetup(target)
        automaticSetup = false
        preparing = target != null
        visible = true
    }

    fun cancelSetup() {
        captureTarget?.instagramStartRequest = null
        hide()
    }

    fun hide() {
        visible = false
        preparing = false
        automaticSetup = false
        captureTarget = null
        generation++
    }

    @SuppressLint("SetJavaScriptEnabled") // Instagram's own page requires JS; no native JS bridge is installed.
    fun view(context: Context, interactive: Boolean = true): WebView {
        web?.let { existing ->
            wrapper?.baseContext = context
            (existing.parent as? ViewGroup)?.removeView(existing)
            configureInteraction(existing, interactive)
            return existing
        }
        application = context.applicationContext
        script = context.assets.open("instagram-producer-capture.js").bufferedReader().use { it.readText() }
        openScript = context.assets.open("instagram-producer-open.js").bufferedReader().use { it.readText() }
        endScript = context.assets.open("instagram-producer-end.js").bufferedReader().use { it.readText() }
        publishScript = context.assets.open("instagram-producer-publish.js").bufferedReader().use { it.readText() }
        diagnoseScript = context.assets.open("instagram-producer-diagnose.js").bufferedReader().use { it.readText() }
        setupScript = context.assets.open("instagram-producer-setup.js").bufferedReader().use { it.readText() }
        titleScript = context.assets.open("instagram-producer-title.js").bufferedReader().use { it.readText() }
        val wrapped = MutableContextWrapper(context)
        wrapper = wrapped
        WebView.setWebContentsDebuggingEnabled(false)
        return WebView(wrapped).apply {
            // WebView treats WRAP_CONTENT as an unbounded document: vh/dvh can resolve to zero,
            // collapsing Instagram's live dialog even when Compose gives this view screen space.
            layoutParams = ViewGroup.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT)
            importantForAutofill = View.IMPORTANT_FOR_AUTOFILL_NO_EXCLUDE_DESCENDANTS
            settings.apply {
                javaScriptEnabled = true
                domStorageEnabled = true
                allowFileAccess = false
                allowContentAccess = false
                mixedContentMode = WebSettings.MIXED_CONTENT_NEVER_ALLOW
                cacheMode = WebSettings.LOAD_NO_CACHE
                safeBrowsingEnabled = true
                setGeolocationEnabled(false)
                setSupportMultipleWindows(false)
                javaScriptCanOpenWindowsAutomatically = false
                useWideViewPort = true
                loadWithOverviewMode = true
                builtInZoomControls = true
                displayZoomControls = false
                val version = Regex("Chrome/([0-9.]+)").find(WebSettings.getDefaultUserAgent(context))?.groupValues?.get(1)
                if (version != null) userAgentString = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/$version Safari/537.36"
            }
            CookieManager.getInstance().setAcceptThirdPartyCookies(this, false)
            webChromeClient = object : WebChromeClient() {
                override fun onPermissionRequest(request: PermissionRequest) { request.deny() }
                override fun onConsoleMessage(consoleMessage: ConsoleMessage?): Boolean = true
            }
            webViewClient = object : WebViewClient() {
                override fun shouldOverrideUrlLoading(view: WebView, request: WebResourceRequest): Boolean =
                    !instagramNavigationAllowed(request.url.toString())
                override fun onPageStarted(view: WebView?, url: String?, favicon: Bitmap?) {
                    if (pendingPublish != null || publicationConfirmation != null) cancelPublish(failed = true)
                    document++
                    connectedProfileId = null
                    producerOpened = false
                    createOpened = false
                    openAttempts = 0
                    loading = true
                    pageError = false
                    host = try { URI(url.orEmpty()).host ?: "instagram.com" } catch (_: Exception) { "instagram.com" }
                }
                override fun onPageFinished(view: WebView?, url: String?) {
                    loading = false
                    // A desktop UA alone is insufficient when Instagram keeps width=device-width.
                    // Set a desktop layout viewport without touching login/2FA documents.
                    if (view != null && instagramCaptureOrigin(url) && view.url == url) {
                        val scale = (view.width / view.resources.displayMetrics.density / 1100f).coerceIn(0.2f, 1f)
                        val viewport = JSONObject.quote("width=1100, initial-scale=$scale, minimum-scale=0.2, maximum-scale=5, user-scalable=yes")
                        view.evaluateJavascript("(() => { let v = document.querySelector('meta[name=viewport]'); if (!v) { v = document.createElement('meta'); v.name = 'viewport'; document.head.appendChild(v); } v.content = $viewport; })()", null)
                    }
                }
                override fun onReceivedError(view: WebView?, request: WebResourceRequest, error: WebResourceError?) {
                    if (request.isForMainFrame) { pageError = true; loading = false; cancelPublish(failed = true) }
                }
                override fun onReceivedSslError(view: WebView?, handler: SslErrorHandler, error: SslError?) {
                    handler.cancel()
                    pageError = true
                    loading = false
                    cancelPublish(failed = true)
                }
                override fun onRenderProcessGone(view: WebView?, detail: RenderProcessGoneDetail?): Boolean {
                    // A page-renderer crash must not kill the native relay/app process.
                    cancelPublish(failed = true)
                    if (pendingEnd != null) finishEnd(false)
                    if (web === view) web = null
                    hasPage = false
                    (view?.parent as? ViewGroup)?.removeView(view)
                    view?.destroy()
                    hide()
                    pageError = true
                    loading = false
                    return true
                }
            }
            configureInteraction(this, interactive)
            web = this
            hasPage = true
            loadUrl("https://www.instagram.com/")
        }
    }

    @SuppressLint("ClickableViewAccessibility") // Hidden host consumes touches, has no click action and is excluded from accessibility.
    private fun configureInteraction(view: WebView, interactive: Boolean) {
        view.isFocusable = interactive
        view.isFocusableInTouchMode = interactive
        view.importantForAccessibility = if (interactive) View.IMPORTANT_FOR_ACCESSIBILITY_AUTO else View.IMPORTANT_FOR_ACCESSIBILITY_NO_HIDE_DESCENDANTS
        view.setOnTouchListener { _, _ -> !interactive }
        if (!interactive) view.clearFocus()
    }

    fun detach() {
        // Release of the old Compose host can run after the new host has reparented this view.
        handler.post { if (web?.isAttachedToWindow != true) wrapper?.baseContext = application }
    }

    /** Called only by the app's explicit Start action, never by opening or restoring a page. */
    fun requestPublish(profileId: String) {
        if (ending) return
        if (pendingPublish?.profileId == profileId) return
        val url = web?.url
        if (!hasConnection(profileId) || !instagramCaptureOrigin(url) || url == null) {
            publishStatus = R.string.instagram_publish_failed
            return
        }
        cancelPublish()
        hide()
        pendingPublish = InstagramPublishRequest(profileId, document, url, SystemClock.elapsedRealtime() + 90_000)
        publishing = true
        lastPublishState = null
        publicationConfirmation = null
        publishState("request")
        publishStatus = R.string.instagram_waiting_video
        handler.post(publishTick)
    }

    fun cancelPublish(profileId: String? = null, failed: Boolean = false) {
        if (profileId != null && pendingPublish?.profileId != profileId) return
        pendingPublish = null
        publicationConfirmation = null
        publishing = false
        publishEvaluationSerial++
        publishEvaluating = false
        publishStatus = if (failed) R.string.instagram_publish_failed else null
        handler.removeCallbacks(publishTick)
    }

    private fun validPublish(request: InstagramPublishRequest): Boolean =
        pendingPublish === request && !pageError && request.valid(SystemClock.elapsedRealtime(),
            request.profileId in RelayServiceState.value.liveProfileIds, document, web?.url)

    /** Continue read-only confirmation after the one-shot Start expires, including manual Go live. */
    private fun observableProfileId(): String? = connectedProfileId?.takeIf {
        !ending && !pageError && web != null && instagramCaptureOrigin(web?.url) &&
            it in RelayServiceState.value.liveProfileIds
    }

    private fun publishState(state: String) {
        if (state != lastPublishState) {
            Log.i(LOG_TAG, "Instagram publication: $state")
            lastPublishState = state
        }
    }

    private fun pollPublish() {
        var request = pendingPublish
        if (request != null && !validPublish(request)) {
            publishState("expired")
            cancelPublish(failed = request.profileId in RelayServiceState.value.liveProfileIds)
            request = null
        }
        val profileId = request?.profileId ?: observableProfileId()
        if (profileId == null) {
            if (publicationConfirmation != null) {
                publicationConfirmation = null
                publishStatus = R.string.instagram_publish_failed
            }
            return
        }
        if (loading) return
        if (publishEvaluating) {
            if (SystemClock.elapsedRealtime() < publishEvaluationDeadline) return
            publishEvaluationSerial++
            publishEvaluating = false
            publicationConfirmation = null
            publishStatus = R.string.instagram_publish_failed
            publishState("evaluation-timeout")
        }
        val armed = request
        val output = RelayServiceState.value.snapshot.output(profileId)
        if (armed != null && (output?.status !in LIVE_OUTPUT_STATUSES || output == null || output.outboundBytes <= 0)) {
            publishState("video-waiting")
            return
        }
        val view = web ?: return
        val currentDocument = document
        val url = view.url
        val ticket = ++publishEvaluationSerial
        publishEvaluating = true
        publishEvaluationDeadline = SystemClock.elapsedRealtime() + 5_000
        if (armed != null) publishStatus = R.string.instagram_publish_starting
        view.evaluateJavascript(publishScript) { raw ->
            if (ticket != publishEvaluationSerial) return@evaluateJavascript
            publishEvaluating = false
            if (document != currentDocument || view.url != url || observableProfileId() != profileId) return@evaluateJavascript
            val state = runCatching { JSONTokener(raw).nextValue() as? String }.getOrNull()
            if (state in setOf("live", "ready", "waiting", "blocked")) publishState(state!!)
            if (state == "live") {
                val since = publicationConfirmation?.takeIf { it.profileId == profileId }?.sinceElapsedMillis
                    ?: SystemClock.elapsedRealtime()
                if (pendingPublish != null) cancelPublish()
                publicationConfirmation = InstagramPublicationConfirmation(profileId, since)
                publishStatus = R.string.instagram_publish_live
                continuePublicationObservation()
                return@evaluateJavascript
            }
            // An absent/error response invalidates the last confirmation; it never means live.
            publicationConfirmation = null
            if (armed == null) {
                publishStatus = R.string.instagram_publish_failed
                return@evaluateJavascript
            }
            if (!validPublish(armed)) return@evaluateJavascript
            when (state) {
                "ready" -> if (armed.claimClick()) {
                    // Only the still-valid explicit Start can dispatch. The observer never clicks.
                    val clickTicket = ++publishEvaluationSerial
                    publishEvaluating = true
                    publishEvaluationDeadline = SystemClock.elapsedRealtime() + 5_000
                    val expiresAt = System.currentTimeMillis() + (armed.deadline - SystemClock.elapsedRealtime()).coerceAtLeast(0)
                    val click = publishScript.replace("/*CLICK*/false", "true")
                        .replace("/*TOKEN*/''", JSONObject.quote(UUID.randomUUID().toString()))
                        .replace("/*DEADLINE*/0", expiresAt.toString())
                        .replace("/*EXPECTED_HREF*/''", JSONObject.quote(url))
                    view.evaluateJavascript(click) clickResult@{ result ->
                        if (clickTicket != publishEvaluationSerial || pendingPublish !== armed) return@clickResult
                        publishEvaluating = false
                        if (result in setOf("\"clicked\"", "\"live\"", "\"dispatched\"")) publishState("dispatched")
                        else if (validPublish(armed)) { cancelPublish(failed = true); continuePublicationObservation() }
                    }
                }
                "blocked" -> { cancelPublish(failed = true); continuePublicationObservation() }
            }
        }
    }

    private fun continuePublicationObservation() {
        handler.removeCallbacks(publishTick)
        if (observableProfileId() != null) handler.postDelayed(publishTick, 3_000)
    }

    /** Keep the ingest alive briefly so Instagram can complete its End/confirm sequence first. */
    private fun requestEnd(profileId: String, onFinished: (Boolean) -> Unit) {
        if (pendingEnd != null) return
        cancelPublish()
        lastEndProfileId = profileId
        endFinished = onFinished
        pendingEnd = InstagramEndRequest(profileId, document, SystemClock.elapsedRealtime() + 8_000)
        ending = true
        endAttempt++
        publishStatus = R.string.instagram_end_starting
        handler.post(endTick)
    }

    fun retryEnd() {
        val profileId = lastEndProfileId ?: return
        requestEnd(profileId) { }
    }

    private fun validEnd(request: InstagramEndRequest): Boolean = pendingEnd === request && !pageError &&
        request.valid(SystemClock.elapsedRealtime(), document, web?.url)

    private fun pollEnd() {
        val request = pendingEnd ?: return
        if (!validEnd(request)) { finishEnd(false); return }
        if (loading || endEvaluating) return
        val view = web ?: return
        endEvaluating = true
        view.evaluateJavascript(endScript) { raw ->
            if (pendingEnd !== request) return@evaluateJavascript
            endEvaluating = false
            if (!validEnd(request)) return@evaluateJavascript
            val step = runCatching { JSONTokener(raw).nextValue() as? String }.getOrNull()
            when (step) {
                "ended", "idle" -> finishEnd(true)
                "blocked" -> finishEnd(false)
                "end", "confirm" -> if (request.claim(step)) {
                    endEvaluating = true
                    val expiresAt = System.currentTimeMillis() + (request.deadline - SystemClock.elapsedRealtime()).coerceAtLeast(0)
                    val click = endScript.replace("/*ACTION*/''", JSONObject.quote(step))
                        .replace("/*TOKEN*/''", JSONObject.quote("$endToken:$endAttempt"))
                        .replace("/*DEADLINE*/0", expiresAt.toString())
                    view.evaluateJavascript(click) clickEnd@{ result ->
                        if (pendingEnd !== request) return@clickEnd
                        endEvaluating = false
                        if (validEnd(request) && result !in setOf("\"clicked\"", "\"dispatched\"")) finishEnd(false)
                    }
                }
            }
        }
    }

    private fun finishEnd(success: Boolean) {
        if (pendingEnd == null) return
        pendingEnd = null
        ending = false
        endEvaluating = false
        handler.removeCallbacks(endTick)
        publishStatus = if (success) R.string.instagram_end_done else R.string.instagram_end_failed
        val callback = endFinished
        endFinished = null
        if (success) {
            connectedProfileId = null
            // A finished Producer key cannot be reused for a new broadcast. Keep login cookies.
            web?.loadUrl("https://www.instagram.com/")
        }
        callback?.invoke(success)
    }

    fun capture(onCaptured: (ProfileEditorState, InstagramCredentials) -> Unit) {
        val view = web ?: return
        val target = captureTarget ?: return
        val url = view.url ?: return
        if (!preparing || loading || pageError || evaluating || !instagramCaptureOrigin(url)) return
        val ticket = generation
        evaluating = true
        view.evaluateJavascript(script.replace("/*TITLE_REVISION*/''", JSONObject.quote(titleRevision))
            .replace("/*AUDIENCE*/'PRACTICE'", JSONObject.quote(desiredAudience.name))) { raw ->
            evaluating = false
            if (ticket != generation || !preparing || captureTarget !== target || view.url != url || !instagramCaptureOrigin(view.url)) return@evaluateJavascript
            val credentials = decodeInstagramCapture(raw) ?: return@evaluateJavascript
            if (automaticSetup && (!credentials.titleMatches || !credentials.audienceMatches)) {
                producerUnavailable = true
                visible = true
                return@evaluateJavascript
            }
            preparedTitle = desiredTitle.takeIf { credentials.titleMatches }
            preparedAudience = desiredAudience.takeIf { credentials.audienceMatches }
            Log.i(LOG_TAG, if (credentials.titleMatches) "Instagram title: verified" else "Instagram title: unverified")
            hide()
            onCaptured(target, credentials)
        }
    }

    fun reload() {
        val target = captureTarget
        if (automaticSetup && target != null) prepare(target, desiredTitle, desiredAudience)
        else web?.reload()
    }

    /** Fill an empty Live video title once per input; the script never returns its value. */
    fun fillTitle() {
        val view = web ?: return
        val url = view.url
        if (!preparing || captureTarget == null || loading || pageError || fillingTitle || !instagramCaptureOrigin(url)) return
        val ticket = generation
        fillingTitle = true
        val fill = titleScript.replace("/*TITLE*/'Stream My Drone'", JSONObject.quote(desiredTitle))
            .replace("/*REVISION*/'default'", JSONObject.quote(titleRevision))
            .replace("/*FORCE*/false", forceTitle.toString())
        view.evaluateJavascript(fill) { raw ->
            fillingTitle = false
            if (!preparing || ticket != generation || view.url != url) return@evaluateJavascript
            if (raw == "\"too-long\"") titleTooLong = true
            else if (raw in setOf("\"filled\"", "\"matches\"")) titleTooLong = false
            if (raw == "\"filled\"") Log.i(LOG_TAG, "Instagram title: filled")
        }
    }

    /** Bounded draft navigation; authentication stays manual and publishing requires an explicit Start. */
    fun openProducer() {
        val view = web ?: return
        val url = view.url
        if (!preparing || captureTarget == null || loading || pageError || opening || producerOpened ||
            openAttempts >= 12 || !instagramCaptureOrigin(url)) return
        val ticket = generation
        opening = true
        view.evaluateJavascript(openScript.replace("/*ALLOW_CREATE*/true", (!createOpened).toString())) { raw ->
            opening = false
            if (!preparing || ticket != generation || view.url != url) return@evaluateJavascript
            val state = runCatching { JSONTokener(raw).nextValue() as? String }.getOrNull()
            if (state in setOf("blocked", "opened", "menu", "waiting", "covered", "unavailable", "ambiguous") && state != lastOpenState) {
                // Only our fixed state code; never page text, addresses, keys or auth data.
                Log.i(LOG_TAG, "Instagram setup: $state")
                lastOpenState = state
            }
            // Login/2FA does not consume the retry budget; resume after the user signs in.
            // An existing title/audience/consent dialog also needs the user's input, not an error.
            if (state == "blocked") { if (automaticSetup) visible = true; return@evaluateJavascript }
            if (state == "waiting" || state == "covered") return@evaluateJavascript
            openAttempts++
            if (state == "ambiguous") openAttempts = 12
            if (state == "menu") createOpened = true
            if (state == "opened") producerOpened = true
            producerUnavailable = state == "ambiguous" || (openAttempts >= 12 && !producerOpened)
        }
    }

    /** Runs for both visible and hidden hosts. Authentication is always completed by the user. */
    fun advanceSetup() {
        if (!preparing || !automaticSetup) return
        if (SystemClock.elapsedRealtime() >= setupDeadline || pageError || producerUnavailable || titleTooLong) {
            if (!pageError && !titleTooLong) producerUnavailable = true
            visible = true
            diagnoseSetup()
            return
        }
        if (loading || setupEvaluating) return
        val view = web ?: return
        val url = view.url ?: return
        if (!instagramCaptureOrigin(url)) { visible = true; setupDeadline = SystemClock.elapsedRealtime() + 60_000; return }
        val ticket = generation
        setupEvaluating = true
        val source = setupScript.replace("/*REVISION*/''", JSONObject.quote(titleRevision))
            .replace("/*AUDIENCE*/'PRACTICE'", JSONObject.quote(desiredAudience.name))
        view.evaluateJavascript(source) { raw ->
            setupEvaluating = false
            if (!preparing || ticket != generation || view.url != url) return@evaluateJavascript
            val state = runCatching { JSONTokener(raw).nextValue() as? String }.getOrNull()
            if (state in setOf("auth", "blocked", "ambiguous", "unavailable", "waiting", "covered", "selected", "audience-opened", "next") && state != lastSetupState) {
                Log.i(LOG_TAG, "Instagram preparation: $state")
                lastSetupState = state
            }
            when {
                state == "auth" -> { visible = true; setupDeadline = SystemClock.elapsedRealtime() + 60_000 }
                state in setOf("blocked", "ambiguous") -> { producerUnavailable = true; visible = true }
                SystemClock.elapsedRealtime() >= setupDeadline -> { producerUnavailable = true; visible = true }
                else -> visible = false
            }
        }
    }

    private fun diagnoseSetup() {
        val view = web ?: return
        if (diagnosed || !instagramCaptureOrigin(view.url) || loading) return
        diagnosed = true
        view.evaluateJavascript(diagnoseScript) { raw ->
            val code = runCatching { JSONTokener(raw).nextValue() as? String }.getOrNull() ?: return@evaluateJavascript
            // Defense in depth: only the probe's finite structural vocabulary reaches logs.
            val allowed = setOf("practice", "public", "next", "audience", "button", "div", "span", "label", "select", "input", "option",
                "other", "none", "combobox", "menuitem", "radio", "listbox", "dialog", "disabled", "enabled", "blocked", "no-controls")
            if (code.length <= 3000 && code.split(':', ',').all { it in allowed }) Log.i(LOG_TAG, "Instagram structure: $code")
        }
    }

    fun retryOpenProducer() {
        val target = captureTarget
        if (automaticSetup && target != null) { prepare(target, desiredTitle, desiredAudience); return }
        openAttempts = 0
        producerOpened = false
        createOpened = false
        producerUnavailable = false
        setupDeadline = SystemClock.elapsedRealtime() + 60_000
        openProducer()
    }

    /** Bring Instagram's own control into view; never publish or change Practice automatically. */
    fun showLiveControl(result: (Boolean) -> Unit) {
        val view = web ?: return result(false)
        val url = view.url
        if (!visible || loading || pageError || !instagramCaptureOrigin(url)) return result(false)
        val ticket = generation
        val controls = view.context.assets.open("instagram-producer-controls.js").bufferedReader().use { it.readText() }
        view.zoomBy(0.5f)
        view.evaluateJavascript(controls) { raw ->
            if (visible && ticket == generation && view.url == url) result(raw == "true")
        }
    }

    fun clearWebSession() {
        hide()
        web?.clearCache(true)
        destroy()
        CookieManager.getInstance().removeAllCookies { CookieManager.getInstance().flush() }
        android.webkit.WebStorage.getInstance().deleteAllData()
    }

    fun destroy() {
        if (pendingEnd != null) finishEnd(false)
        if (retained?.get() === this) retained = null
        cancelPublish()
        document++
        hasPage = false
        generation++
        evaluating = false
        opening = false
        fillingTitle = false
        connectedProfileId = null
        preparedTitle = null
        preparedAudience = null
        preparing = false
        automaticSetup = false
        setupEvaluating = false
        captureTarget = null
        web?.apply { (parent as? ViewGroup)?.removeView(this); stopLoading(); destroy() }
        web = null
        wrapper?.baseContext = application
        wrapper = null
    }
    companion object {
        // A notification may finish the retained page; no Activity or destroyed ViewModel is retained.
        private var retained: WeakReference<InstagramProducerSession>? = null

        fun profileToEnd(profileIds: List<String>): String? = retained?.get()?.connectedProfileId?.takeIf { it in profileIds }

        fun endConnected(profileId: String, onFinished: (Boolean) -> Unit) {
            val session = retained?.get()
            if (session?.connectedProfileId != profileId) onFinished(false)
            else session.requestEnd(profileId, onFinished)
        }
    }

}
