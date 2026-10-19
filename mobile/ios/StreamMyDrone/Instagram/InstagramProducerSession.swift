import Foundation
import Observation
import os
import UIKit
import WebKit

/// Who a captured Instagram connection is for: an editor's profile, and the Go live it resumes.
final class InstagramCaptureTarget {
    let profile: DestinationProfile?
    /// Only a Go live request, never an ordinary connection edit, resumes after capture.
    var startRequest: BroadcastStartRequest?

    init(profile: DestinationProfile?, startRequest: BroadcastStartRequest? = nil) {
        self.profile = profile
        self.startRequest = startRequest
    }
}

/**
 * Instagram's own Live Producer page, kept in a web view for the app's lifetime so capturing a key
 * does not close its tab. The iOS counterpart of the Android app's InstagramProducerSession: the
 * same scripts prepare a draft with the chosen title and audience, read its ingest, press
 * Instagram's Go live only after the app's explicit Start, and End before the ingest is cut.
 * Signing in, verification and every consent stay with the user in Instagram's page.
 */
@MainActor
@Observable
final class InstagramProducerSession {
    private(set) var visible = false
    private(set) var loading = true
    private(set) var pageError = false
    private(set) var host = "instagram.com"
    private(set) var producerUnavailable = false
    private(set) var hasPage = false
    /// A string key: what Instagram's start or end is doing, or nil.
    private(set) var publishStatus: String?
    private(set) var publishing = false
    private(set) var ending = false
    private(set) var preparing = false
    private(set) var titleTooLong = false
    private(set) var publicationConfirmation: InstagramPublicationConfirmation?
    private(set) var captureTarget: InstagramCaptureTarget?

    /// The relay's state, for publishing only while video reaches Instagram.
    @ObservationIgnored var relayState: () -> RelayUiState = { RelayUiState() }
    /// A captured connection; the app saves it, then calls `connected`.
    @ObservationIgnored var onCaptured: ((InstagramCaptureTarget, InstagramCredentials) -> Void)?

    @ObservationIgnored private var web: WKWebView?
    @ObservationIgnored private var delegate: WebDelegate?
    @ObservationIgnored private var scripts: [String: String] = [:]
    @ObservationIgnored private var document = 0
    @ObservationIgnored private var generation = 0
    @ObservationIgnored private var automaticSetup = false
    @ObservationIgnored private var setupDeadline: TimeInterval = 0
    @ObservationIgnored private var setupEvaluating = false
    @ObservationIgnored private var diagnosed = false
    @ObservationIgnored private var lastSetupState: String?
    @ObservationIgnored private var desiredAudience: InstagramAudience = .practice
    @ObservationIgnored private var preparedAudience: InstagramAudience?
    @ObservationIgnored private var desiredTitle = "Stream My Drone"
    @ObservationIgnored private var titleRevision = UUID().uuidString
    @ObservationIgnored private var forceTitle = false
    @ObservationIgnored private var preparedTitle: String?
    @ObservationIgnored private var fillingTitle = false
    @ObservationIgnored private var evaluating = false
    @ObservationIgnored private var opening = false
    @ObservationIgnored private var openAttempts = 0
    @ObservationIgnored private var producerOpened = false
    @ObservationIgnored private var createOpened = false
    @ObservationIgnored private var lastOpenState: String?
    @ObservationIgnored private var connectedProfileId: String?
    @ObservationIgnored private var automation: Task<Void, Never>?

    @ObservationIgnored private var pendingPublish: InstagramPublishRequest?
    @ObservationIgnored private var lastPublishState: String?
    @ObservationIgnored private var publishEvaluationSerial = 0
    @ObservationIgnored private var publishEvaluationDeadline: TimeInterval = 0
    @ObservationIgnored private var publishEvaluating = false
    @ObservationIgnored private var publishTask: Task<Void, Never>?

    @ObservationIgnored private var pendingEnd: InstagramEndRequest?
    @ObservationIgnored private var endEvaluating = false
    @ObservationIgnored private var endFinished: ((Bool) -> Void)?
    @ObservationIgnored private var lastEndProfileId: String?
    @ObservationIgnored private let endToken = UUID().uuidString
    @ObservationIgnored private var endAttempt = 0
    @ObservationIgnored private var endTask: Task<Void, Never>?

    private static let home = URL(string: "https://www.instagram.com/")!
    private static let log = Logger(subsystem: "com.streammydrone.app", category: "instagram")
    /// WebKit's own engine, named as the desktop Safari Instagram's Live Producer supports.
    private static let userAgent = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Safari/605.1.15"

    private var now: TimeInterval { ProcessInfo.processInfo.systemUptime }

    // MARK: Title and audience

    func setupTitle(_ title: String, force: Bool = false) {
        guard let normalized = try? validateBroadcastTitle(title) else { return }
        if normalized != desiredTitle || force != forceTitle {
            desiredTitle = normalized
            titleRevision = UUID().uuidString
            forceTitle = force
            titleTooLong = false
        }
    }

    func rejectsTitle(_ title: String) -> Bool { titleTooLong && desiredTitle == title }

    /// True when the retained draft already has this title and audience.
    func prepareForTitle(profileId: String, title: String, audience: InstagramAudience) -> Bool {
        guard let normalized = try? validateBroadcastTitle(title) else { return false }
        if hasConnection(profileId) && preparedTitle == normalized && preparedAudience == audience { return true }
        desiredAudience = audience
        setupTitle(normalized, force: true)
        return false
    }

    func hasConnection(_ profileId: String) -> Bool { connectedProfileId == profileId && web != nil && !pageError }

    func connected(_ profileId: String) {
        connectedProfileId = profileId
        publicationConfirmation = nil
        lastEndProfileId = nil
        publishStatus = nil
    }

    var connectedProfile: String? { connectedProfileId }

    // MARK: Setup

    /// Hidden preparation starts only after the app's own Connect or Start confirmation.
    func prepare(_ target: InstagramCaptureTarget, title: String, audience: InstagramAudience) {
        setupTitle(title, force: true)
        desiredAudience = audience
        // Every confirmation owns fresh title/audience proof and one-shot setup actions.
        titleRevision = UUID().uuidString
        resetSetup(target)
        automaticSetup = true
        preparing = true
        visible = false
        setupDeadline = now + 60
        // A canceled or old draft may still show its key; sign-in cookies survive a fresh draft.
        connectedProfileId = nil
        preparedTitle = nil
        preparedAudience = nil
        // A new page opens on the start page by itself; a retained one starts over there.
        if web != nil { web?.load(URLRequest(url: Self.home)) } else { _ = webView() }
        startAutomation()
    }

    private func resetSetup(_ target: InstagramCaptureTarget?) {
        captureTarget = target
        diagnosed = false
        lastSetupState = nil
        setupEvaluating = false
        evaluating = false
        opening = false
        fillingTitle = false
        openAttempts = 0
        producerOpened = false
        createOpened = false
        producerUnavailable = false
        lastOpenState = nil
        generation += 1
    }

    /// Shows Instagram's page: to resolve a problem, sign in, or look at the running broadcast.
    func show(_ target: InstagramCaptureTarget? = nil) {
        _ = webView()
        if preparing && (target == nil || target === captureTarget) { visible = true; return }
        resetSetup(target)
        automaticSetup = false
        preparing = target != nil
        visible = true
        if preparing { startAutomation() }
    }

    func cancelSetup() {
        captureTarget?.startRequest = nil
        hide()
    }

    func hide() {
        visible = false
        preparing = false
        automaticSetup = false
        captureTarget = nil
        generation += 1
        automation?.cancel()
        automation = nil
    }

    private func startAutomation() {
        automation?.cancel()
        automation = Task { [weak self] in
            while let self, self.preparing, !Task.isCancelled {
                self.fillTitle()
                self.advanceSetup()
                self.capture()
                self.openProducer()
                try? await Task.sleep(for: .seconds(1))
            }
        }
    }

    // MARK: Web view

    /// The retained page, created on first use. Instagram's page needs JavaScript; no native
    /// script bridge, message handler or content blocker is installed.
    func webView() -> WKWebView {
        if let web { return web }
        for name in ["capture", "open", "end", "publish", "diagnose", "setup", "title", "controls"] {
            if let url = Bundle.main.url(forResource: "instagram-producer-\(name)", withExtension: "js"),
               let source = try? String(contentsOf: url, encoding: .utf8) {
                scripts[name] = source
            }
        }
        let configuration = WKWebViewConfiguration()
        // Sign-in cookies persist like a browser's; "Clear session" removes them.
        configuration.websiteDataStore = .default()
        configuration.defaultWebpagePreferences.preferredContentMode = .desktop
        configuration.preferences.javaScriptCanOpenWindowsAutomatically = false
        configuration.mediaTypesRequiringUserActionForPlayback = .all
        configuration.dataDetectorTypes = []
        let view = WKWebView(frame: UIScreen.main.bounds, configuration: configuration)
        view.customUserAgent = Self.userAgent
        view.isInspectable = false
        view.allowsLinkPreview = false
        view.autoresizingMask = [.flexibleWidth, .flexibleHeight]
        let delegate = WebDelegate(session: self)
        view.navigationDelegate = delegate
        view.uiDelegate = delegate
        self.delegate = delegate
        web = view
        hasPage = true
        view.load(URLRequest(url: Self.home))
        return view
    }

    /// Moves the page into `container`; the hidden host never takes touches or VoiceOver focus.
    func place(in container: UIView, interactive: Bool) {
        let view = webView()
        if view.superview !== container {
            view.removeFromSuperview()
            view.frame = container.bounds
            container.addSubview(view)
        }
        view.isUserInteractionEnabled = interactive
        view.accessibilityElementsHidden = !interactive
        if !interactive { view.endEditing(true) }
    }

    fileprivate func pageStarted(_ url: URL?) {
        if pendingPublish != nil || publicationConfirmation != nil { cancelPublish(failed: true) }
        document += 1
        connectedProfileId = nil
        producerOpened = false
        createOpened = false
        openAttempts = 0
        loading = true
        pageError = false
        // A new page, such as Instagram's own after signing in, gets fresh time and another try:
        // signing in is all the user has to do.
        if preparing && automaticSetup {
            producerUnavailable = false
            diagnosed = false
            setupDeadline = now + 60
        }
        host = url?.host ?? "instagram.com"
    }

    fileprivate func pageFinished(_ view: WKWebView) {
        loading = false
        host = view.url?.host ?? host
        // The desktop layout without touching sign-in or 2FA documents.
        guard instagramCaptureOrigin(view.url) else { return }
        let scale = min(max(view.bounds.width / 1100, 0.2), 1)
        let viewport = javaScriptLiteral("width=1100, initial-scale=\(scale), minimum-scale=0.2, maximum-scale=5, user-scalable=yes")
        view.evaluateJavaScript("(() => { let v = document.querySelector('meta[name=viewport]'); if (!v) { v = document.createElement('meta'); v.name = 'viewport'; document.head.appendChild(v); } v.content = \(viewport); })()")
    }

    fileprivate func pageFailed() {
        pageError = true
        loading = false
        cancelPublish(failed: true)
    }

    /// A crashed page must not take the relay down; it starts over on the next use.
    fileprivate func pageTerminated() {
        cancelPublish(failed: true)
        if pendingEnd != nil { finishEnd(false) }
        web?.removeFromSuperview()
        web = nil
        delegate = nil
        hasPage = false
        hide()
        pageError = true
        loading = false
    }

    private func evaluate(_ source: String, _ completion: @escaping @MainActor (Any?) -> Void) {
        guard let web else { return completion(nil) }
        web.evaluateJavaScript(source) { result, error in completion(error == nil ? result : nil) }
    }

    // MARK: Publishing

    /// Called only by the app's explicit Start, never by opening or restoring a page.
    func requestPublish(_ profileId: String) {
        if ending || pendingPublish?.profileId == profileId { return }
        guard hasConnection(profileId), let url = web?.url, instagramCaptureOrigin(url) else {
            publishStatus = "instagram_publish_failed"
            return
        }
        cancelPublish()
        hide()
        pendingPublish = InstagramPublishRequest(profileId: profileId, document: document, url: url, deadline: now + 90)
        publishing = true
        lastPublishState = nil
        publicationConfirmation = nil
        publishState("request")
        publishStatus = "instagram_waiting_video"
        schedulePublish(after: 0)
    }

    func cancelPublish(_ profileId: String? = nil, failed: Bool = false) {
        if let profileId, pendingPublish?.profileId != profileId { return }
        pendingPublish = nil
        publicationConfirmation = nil
        publishing = false
        publishEvaluationSerial += 1
        publishEvaluating = false
        publishStatus = failed ? "instagram_publish_failed" : nil
        publishTask?.cancel()
        publishTask = nil
    }

    private func schedulePublish(after seconds: Double) {
        publishTask?.cancel()
        publishTask = Task { [weak self] in
            if seconds > 0 { try? await Task.sleep(for: .seconds(seconds)) }
            while let self, !Task.isCancelled {
                self.pollPublish()
                guard self.pendingPublish != nil || self.observableProfileId() != nil else { return }
                try? await Task.sleep(for: .seconds(self.pendingPublish != nil ? 1 : 3))
            }
        }
    }

    private func isLive(_ profileId: String) -> Bool { relayState().liveProfileIds.contains(profileId) }

    private func validPublish(_ request: InstagramPublishRequest) -> Bool {
        pendingPublish === request && !pageError &&
            request.valid(now: now, active: isLive(request.profileId), document: document, url: web?.url)
    }

    /// Read-only confirmation continues after the one-shot Start, including a manual Go live.
    private func observableProfileId() -> String? {
        guard let id = connectedProfileId, !ending, !pageError, web != nil, instagramCaptureOrigin(web?.url), isLive(id) else { return nil }
        return id
    }

    private func publishState(_ state: String) {
        guard state != lastPublishState else { return }
        Self.log.info("Instagram publication: \(state, privacy: .public)")
        lastPublishState = state
    }

    private func pollPublish() {
        var request = pendingPublish
        if let armed = request, !validPublish(armed) {
            publishState("expired")
            cancelPublish(failed: isLive(armed.profileId))
            request = nil
        }
        guard let profileId = request?.profileId ?? observableProfileId() else {
            if publicationConfirmation != nil {
                publicationConfirmation = nil
                publishStatus = "instagram_publish_failed"
            }
            return
        }
        if loading { return }
        if publishEvaluating {
            if now < publishEvaluationDeadline { return }
            publishEvaluationSerial += 1
            publishEvaluating = false
            publicationConfirmation = nil
            publishStatus = "instagram_publish_failed"
            publishState("evaluation-timeout")
        }
        let armed = request
        let output = relayState().snapshot.output(profileId)
        if armed != nil && !(output.map { liveOutputStatuses.contains($0.status) && $0.outboundBytes > 0 } ?? false) {
            publishState("video-waiting")
            return
        }
        guard let view = web, let script = scripts["publish"] else { return }
        let currentDocument = document
        let url = view.url
        publishEvaluationSerial += 1
        let ticket = publishEvaluationSerial
        publishEvaluating = true
        publishEvaluationDeadline = now + 5
        if armed != nil { publishStatus = "instagram_publish_starting" }
        evaluate(script) { [weak self] raw in
            guard let self, ticket == self.publishEvaluationSerial else { return }
            self.publishEvaluating = false
            guard self.document == currentDocument, view.url == url, self.observableProfileId() == profileId else { return }
            let state = raw as? String
            if let state, ["live", "ready", "waiting", "blocked"].contains(state) { self.publishState(state) }
            if state == "blocked" || (armed == nil && state != "live") { self.diagnoseSetup() }
            if state == "live" {
                let since = self.publicationConfirmation.flatMap { $0.profileId == profileId ? $0.since : nil } ?? self.now
                if self.pendingPublish != nil { self.cancelPublish() }
                self.publicationConfirmation = InstagramPublicationConfirmation(profileId: profileId, since: since)
                self.publishStatus = "instagram_publish_live"
                self.continuePublicationObservation()
                return
            }
            // An absent or failed answer invalidates the last confirmation; it never means live.
            self.publicationConfirmation = nil
            guard let armed else {
                self.publishStatus = "instagram_publish_failed"
                return
            }
            guard self.validPublish(armed) else { return }
            switch state {
            case "ready":
                // Only the still-valid explicit Start can click. The observer never does.
                guard armed.claimClick(), let url else { return }
                self.publishEvaluationSerial += 1
                let clickTicket = self.publishEvaluationSerial
                self.publishEvaluating = true
                self.publishEvaluationDeadline = self.now + 5
                let expiresAt = Int64((Date().timeIntervalSince1970 + max(armed.deadline - self.now, 0)) * 1_000)
                let click = script.replacingOccurrences(of: "/*CLICK*/false", with: "true")
                    .replacingOccurrences(of: "/*TOKEN*/''", with: javaScriptLiteral(UUID().uuidString))
                    .replacingOccurrences(of: "/*DEADLINE*/0", with: String(expiresAt))
                    .replacingOccurrences(of: "/*EXPECTED_HREF*/''", with: javaScriptLiteral(url.absoluteString))
                self.evaluate(click) { [weak self] result in
                    guard let self, clickTicket == self.publishEvaluationSerial, self.pendingPublish === armed else { return }
                    self.publishEvaluating = false
                    if let result = result as? String, ["clicked", "live", "dispatched"].contains(result) {
                        self.publishState("dispatched")
                    } else if self.validPublish(armed) {
                        self.cancelPublish(failed: true)
                        self.continuePublicationObservation()
                    }
                }
            case "blocked":
                self.cancelPublish(failed: true)
                self.continuePublicationObservation()
            default:
                break
            }
        }
    }

    private func continuePublicationObservation() {
        publishTask?.cancel()
        publishTask = nil
        if observableProfileId() != nil { schedulePublish(after: 3) }
    }

    // MARK: Ending

    /// Instagram's End and its confirmation run before the ingest is cut, within 8 seconds.
    func requestEnd(_ profileId: String, onFinished: @escaping (Bool) -> Void) {
        guard pendingEnd == nil else { return }
        cancelPublish()
        lastEndProfileId = profileId
        endFinished = onFinished
        pendingEnd = InstagramEndRequest(profileId: profileId, document: document, deadline: now + 8)
        ending = true
        endAttempt += 1
        publishStatus = "instagram_end_starting"
        endTask?.cancel()
        endTask = Task { [weak self] in
            while let self, self.pendingEnd != nil, !Task.isCancelled {
                self.pollEnd()
                try? await Task.sleep(for: .milliseconds(250))
            }
        }
    }

    func retryEnd() {
        guard let profileId = lastEndProfileId else { return }
        requestEnd(profileId) { _ in }
    }

    /// The profile among `profileIds` whose broadcast this page runs, if any.
    func profileToEnd(_ profileIds: [String]) -> String? {
        connectedProfileId.flatMap { profileIds.contains($0) ? $0 : nil }
    }

    private func validEnd(_ request: InstagramEndRequest) -> Bool {
        pendingEnd === request && !pageError && request.valid(now: now, document: document, url: web?.url)
    }

    private func pollEnd() {
        guard let request = pendingEnd else { return }
        guard validEnd(request) else { return finishEnd(false) }
        guard !loading, !endEvaluating, let script = scripts["end"] else { return }
        endEvaluating = true
        evaluate(script) { [weak self] raw in
            guard let self, self.pendingEnd === request else { return }
            self.endEvaluating = false
            guard self.validEnd(request) else { return }
            switch raw as? String {
            case "ended", "idle": self.finishEnd(true)
            case "blocked": self.finishEnd(false)
            case let step? where (step == "end" || step == "confirm") && request.claim(step):
                self.endEvaluating = true
                let expiresAt = Int64((Date().timeIntervalSince1970 + max(request.deadline - self.now, 0)) * 1_000)
                let click = script.replacingOccurrences(of: "/*ACTION*/''", with: javaScriptLiteral(step))
                    .replacingOccurrences(of: "/*TOKEN*/''", with: javaScriptLiteral("\(self.endToken):\(self.endAttempt)"))
                    .replacingOccurrences(of: "/*DEADLINE*/0", with: String(expiresAt))
                self.evaluate(click) { [weak self] result in
                    guard let self, self.pendingEnd === request else { return }
                    self.endEvaluating = false
                    let accepted = (result as? String).map { ["clicked", "dispatched"].contains($0) } ?? false
                    if self.validEnd(request) && !accepted { self.finishEnd(false) }
                }
            default:
                break
            }
        }
    }

    private func finishEnd(_ success: Bool) {
        guard pendingEnd != nil else { return }
        pendingEnd = nil
        ending = false
        endEvaluating = false
        endTask?.cancel()
        endTask = nil
        publishStatus = success ? "instagram_end_done" : "instagram_end_failed"
        let callback = endFinished
        endFinished = nil
        if success {
            connectedProfileId = nil
            // A finished Producer key cannot be reused. Sign-in cookies stay.
            web?.load(URLRequest(url: Self.home))
        }
        callback?(success)
    }

    // MARK: Setup steps

    private func capture() {
        guard let view = web, let target = captureTarget, let url = view.url, let script = scripts["capture"],
              preparing, !loading, !pageError, !evaluating, instagramCaptureOrigin(url) else { return }
        let ticket = generation
        evaluating = true
        let source = script.replacingOccurrences(of: "/*TITLE_REVISION*/''", with: javaScriptLiteral(titleRevision))
            .replacingOccurrences(of: "/*AUDIENCE*/'PRACTICE'", with: javaScriptLiteral(desiredAudience.rawValue))
        evaluate(source) { [weak self] raw in
            guard let self else { return }
            self.evaluating = false
            guard ticket == self.generation, self.preparing, self.captureTarget === target, view.url == url,
                  instagramCaptureOrigin(view.url), let credentials = decodeInstagramCapture(raw) else { return }
            if self.automaticSetup && (!credentials.titleMatches || !credentials.audienceMatches) {
                self.producerUnavailable = true
                self.visible = true
                return
            }
            self.preparedTitle = credentials.titleMatches ? self.desiredTitle : nil
            self.preparedAudience = credentials.audienceMatches ? self.desiredAudience : nil
            Self.log.info("Instagram title: \(credentials.titleMatches ? "verified" : "unverified", privacy: .public)")
            self.hide()
            self.onCaptured?(target, credentials)
        }
    }

    func reload() {
        if automaticSetup, let target = captureTarget { prepare(target, title: desiredTitle, audience: desiredAudience) }
        else { web?.reload() }
    }

    /// Fills an empty Live video title once per input; the script never returns its value.
    private func fillTitle() {
        guard let view = web, let script = scripts["title"], preparing, captureTarget != nil, !loading, !pageError,
              !fillingTitle, instagramCaptureOrigin(view.url) else { return }
        let url = view.url
        let ticket = generation
        fillingTitle = true
        let fill = script.replacingOccurrences(of: "/*TITLE*/'Stream My Drone'", with: javaScriptLiteral(desiredTitle))
            .replacingOccurrences(of: "/*REVISION*/'default'", with: javaScriptLiteral(titleRevision))
            .replacingOccurrences(of: "/*FORCE*/false", with: forceTitle ? "true" : "false")
        evaluate(fill) { [weak self] raw in
            guard let self else { return }
            self.fillingTitle = false
            guard self.preparing, ticket == self.generation, view.url == url else { return }
            switch raw as? String {
            case "too-long": self.titleTooLong = true
            case "filled", "matches": self.titleTooLong = false
            default: break
            }
            if raw as? String == "filled" { Self.log.info("Instagram title: filled") }
        }
    }

    /// Bounded draft navigation; signing in stays manual and publishing needs an explicit Start.
    private func openProducer() {
        guard let view = web, let script = scripts["open"], preparing, captureTarget != nil, !loading, !pageError,
              !opening, !producerOpened, openAttempts < 12, instagramCaptureOrigin(view.url) else { return }
        let url = view.url
        let ticket = generation
        opening = true
        evaluate(script.replacingOccurrences(of: "/*ALLOW_CREATE*/true", with: createOpened ? "false" : "true")) { [weak self] raw in
            guard let self else { return }
            self.opening = false
            guard self.preparing, ticket == self.generation, view.url == url else { return }
            let state = raw as? String
            if let state, ["blocked", "opened", "menu", "waiting", "covered", "unavailable", "ambiguous"].contains(state), state != self.lastOpenState {
                // Only the script's fixed state code; never page text, addresses, keys or sign-in data.
                Self.log.info("Instagram setup: \(state, privacy: .public)")
                self.lastOpenState = state
            }
            // Signing in does not use up the retries; preparation resumes after it.
            if state == "blocked" { if self.automaticSetup { self.visible = true }; return }
            if state == "waiting" || state == "covered" { return }
            self.openAttempts += 1
            if state == "ambiguous" { self.openAttempts = 12 }
            if state == "menu" { self.createOpened = true }
            if state == "opened" { self.producerOpened = true }
            self.producerUnavailable = state == "ambiguous" || (self.openAttempts >= 12 && !self.producerOpened)
        }
    }

    /// Runs whether the page shows or not. Signing in is always done by the user.
    private func advanceSetup() {
        guard preparing, automaticSetup else { return }
        if now >= setupDeadline || pageError || producerUnavailable || titleTooLong {
            if !pageError && !titleTooLong { producerUnavailable = true }
            visible = true
            diagnoseSetup()
            return
        }
        guard !loading, !setupEvaluating, let view = web, let url = view.url, let script = scripts["setup"] else { return }
        guard instagramCaptureOrigin(url) else {
            visible = true
            setupDeadline = now + 60
            return
        }
        let ticket = generation
        setupEvaluating = true
        let source = script.replacingOccurrences(of: "/*REVISION*/''", with: javaScriptLiteral(titleRevision))
            .replacingOccurrences(of: "/*AUDIENCE*/'PRACTICE'", with: javaScriptLiteral(desiredAudience.rawValue))
        evaluate(source) { [weak self] raw in
            guard let self else { return }
            self.setupEvaluating = false
            guard self.preparing, ticket == self.generation, view.url == url else { return }
            let state = raw as? String
            if let state, ["auth", "blocked", "ambiguous", "unavailable", "waiting", "covered", "selected", "audience-opened", "next"].contains(state),
               state != self.lastSetupState {
                Self.log.info("Instagram preparation: \(state, privacy: .public)")
                self.lastSetupState = state
            }
            if state == "auth" {
                self.visible = true
                self.setupDeadline = self.now + 60
            } else if state == "blocked" || state == "ambiguous" || self.now >= self.setupDeadline {
                self.producerUnavailable = true
                self.visible = true
            } else {
                self.visible = false
            }
        }
    }

    private func diagnoseSetup() {
        guard !diagnosed, !loading, instagramCaptureOrigin(web?.url), let script = scripts["diagnose"] else { return }
        diagnosed = true
        evaluate(script) { raw in
            guard let code = raw as? String, code.count <= 3_000 else { return }
            // Defense in depth: only the probe's finite structural vocabulary reaches the log.
            let allowed: Set<String> = ["practice", "public", "next", "audience", "start", "end", "button", "div", "span", "label",
                "select", "input", "option", "other", "none", "combobox", "menuitem", "radio", "listbox", "dialog", "disabled",
                "enabled", "blocked", "no-controls"]
            let parts = code.split(whereSeparator: { $0 == ":" || $0 == "," }).map(String.init)
            if parts.allSatisfy(allowed.contains) { Self.log.info("Instagram structure: \(code, privacy: .public)") }
        }
    }

    func retryOpenProducer() {
        if automaticSetup, let target = captureTarget { return prepare(target, title: desiredTitle, audience: desiredAudience) }
        openAttempts = 0
        producerOpened = false
        createOpened = false
        producerUnavailable = false
        setupDeadline = now + 60
        openProducer()
    }

    /// Brings Instagram's own control into view; never publishes or changes Practice by itself.
    func showLiveControl(_ result: @escaping (Bool) -> Void) {
        guard let view = web, visible, !loading, !pageError, instagramCaptureOrigin(view.url), let script = scripts["controls"] else {
            return result(false)
        }
        let url = view.url
        let ticket = generation
        view.scrollView.setZoomScale(view.scrollView.zoomScale * 0.5, animated: true)
        evaluate(script) { [weak self] raw in
            guard let self, self.visible, ticket == self.generation, view.url == url else { return }
            result(raw as? Bool ?? false)
        }
    }

    /// Signs out of Instagram in this app: cookies, storage and the retained page all go.
    func clearWebSession() {
        hide()
        destroy()
        let store = WKWebsiteDataStore.default()
        store.removeData(ofTypes: WKWebsiteDataStore.allWebsiteDataTypes(), modifiedSince: .distantPast) {}
    }

    func destroy() {
        if pendingEnd != nil { finishEnd(false) }
        cancelPublish()
        document += 1
        hasPage = false
        generation += 1
        evaluating = false
        opening = false
        fillingTitle = false
        connectedProfileId = nil
        preparedTitle = nil
        preparedAudience = nil
        preparing = false
        automaticSetup = false
        setupEvaluating = false
        captureTarget = nil
        automation?.cancel()
        web?.stopLoading()
        web?.removeFromSuperview()
        web = nil
        delegate = nil
    }

    /// The navigation and UI callbacks, kept apart so the observable session stays a plain class.
    @MainActor
    private final class WebDelegate: NSObject, WKNavigationDelegate, WKUIDelegate {
        weak var session: InstagramProducerSession?

        init(session: InstagramProducerSession) { self.session = session }

        func webView(_ webView: WKWebView, decidePolicyFor navigationAction: WKNavigationAction) async -> WKNavigationActionPolicy {
            // Instagram's own frames load what they need; the page itself stays on Instagram.
            guard navigationAction.targetFrame?.isMainFrame ?? true else { return .allow }
            return instagramNavigationAllowed(navigationAction.request.url) ? .allow : .cancel
        }

        func webView(_ webView: WKWebView, didStartProvisionalNavigation navigation: WKNavigation!) {
            session?.pageStarted(webView.url)
        }

        func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {
            session?.pageFinished(webView)
        }

        func webView(_ webView: WKWebView, didFail navigation: WKNavigation!, withError error: Error) {
            if !Self.superseded(error) { session?.pageFailed() }
        }

        func webView(_ webView: WKWebView, didFailProvisionalNavigation navigation: WKNavigation!, withError error: Error) {
            if !Self.superseded(error) { session?.pageFailed() }
        }

        func webViewWebContentProcessDidTerminate(_ webView: WKWebView) {
            session?.pageTerminated()
        }

        /// A load replaced by another, or one the policy above refused, is not a page error.
        private static func superseded(_ error: Error) -> Bool {
            let error = error as NSError
            return (error.domain == NSURLErrorDomain && error.code == NSURLErrorCancelled) ||
                (error.domain == "WebKitErrorDomain" && error.code == 102)
        }

        // No pop-up windows, camera, microphone or file pickers for the page.
        func webView(_ webView: WKWebView, createWebViewWith configuration: WKWebViewConfiguration,
                     for navigationAction: WKNavigationAction, windowFeatures: WKWindowFeatures) -> WKWebView? { nil }

        func webView(_ webView: WKWebView, decideMediaCapturePermissionsFor origin: WKSecurityOrigin,
                     initiatedBy frame: WKFrameInfo, type: WKMediaCaptureType) async -> WKPermissionDecision { .deny }
    }
}
