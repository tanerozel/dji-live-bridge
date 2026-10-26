import AVFoundation
import Foundation
import Observation
import SwiftUI
import UIKit
import UserNotifications

/// A short message at the bottom of the screen, with an optional action.
struct SnackbarMessage: Identifiable, Equatable {
    let id = UUID()
    let text: UiText
    var actionTitle: String?
    var long = false

    static func == (lhs: SnackbarMessage, rhs: SnackbarMessage) -> Bool { lhs.id == rhs.id }
}

/// What the end-broadcast question ends: one platform, or all of them for nil.
struct EndLiveRequest: Identifiable {
    let id = UUID()
    let profileId: String?
}

/// The key screen for one platform: an existing profile, or a new one on `kind`.
struct ProfileEditorRequest: Identifiable {
    let id = UUID()
    let profile: DestinationProfile?
    let kind: DestinationKind
}

/**
 * The app's screen logic: the saved platforms, which of them the broadcast goes to, and what the
 * buttons do. The receiver opens whenever the app is on screen, so DJI Fly can connect right away
 * and its picture shows before anything goes to a platform.
 */
@MainActor
@Observable
final class AppModel {
    let relay: RelayController
    let picture = DronePictureState()
    private(set) var destinations = DestinationProfiles()
    private(set) var profileError: UiText?
    private(set) var lan: LanAddress?
    var showGuide: Bool
    var editor: ProfileEditorRequest?
    var endLive: EndLiveRequest?
    var snackbar: SnackbarMessage?
    private(set) var theme: ThemeChoice
    private(set) var pictureFit: PictureFit
    /// The drone screen shows the picture, and keeps it through a short drop.
    private(set) var showingPicture = false
    /// Instagram's Live Producer, prepared and started by the app itself.
    let instagram = InstagramProducerSession()
    @ObservationIgnored let instagramPresenter = InstagramPresenter()
    /// The platforms the title dialog is open for.
    var titleRequest: TitleRequest?
    /// Instagram profiles whose key was pasted by hand in this session; the app does not run them.
    private(set) var instagramManualProfiles: Set<String> = []
    /// The current broadcast's title and Instagram audience, kept when a platform is added.
    private(set) var broadcastTitle: String?
    private(set) var broadcastAudience: InstagramAudience = .practice
    /// What YouTube says about each live connected YouTube profile.
    private(set) var youtubeViews: [String: YouTubeBroadcastView] = [:]
    /// What Facebook says about each live connected Facebook profile.
    private(set) var facebookViews: [String: FacebookBroadcastView] = [:]

    @ObservationIgnored private let store: DestinationProfileStore
    @ObservationIgnored private let preferences = UiPreferences()
    @ObservationIgnored private var snackbarAction: (() -> Void)?
    @ObservationIgnored private var snackbarTask: Task<Void, Never>?
    @ObservationIgnored private var pictureGrace: Task<Void, Never>?
    @ObservationIgnored private var networkTask: Task<Void, Never>?
    @ObservationIgnored private let background = BackgroundGuard()
    @ObservationIgnored private var active = false

    init(store: DestinationProfileStore = DestinationProfileStore()) {
        self.store = store
        relay = RelayController(profileStore: store)
        showGuide = !preferences.guideCompleted
        theme = preferences.theme
        pictureFit = preferences.pictureFit
        do {
            destinations = try store.load()
        } catch {
            profileError = profileErrorText(error)
        }
        broadcastAudience = preferences.instagramAudience
        instagram.relayState = { [weak relay] in relay?.state ?? RelayUiState() }
        instagram.onCaptured = { [weak self] target, credentials in self?.instagramCaptured(target, credentials) }
    }

    var phase: BridgePhase { relay.phase }

    // MARK: Lifecycle

    /// The app came on screen: the receiver opens (after the guide) and the address refreshes.
    func becameActive() {
        active = true
        background.end()
        if !showGuide {
            LocalNetworkPermission.request()
            if !relay.state.isActive { relay.startReceiver() }
        }
        networkTask?.cancel()
        // Keeps the DJI Fly address current: the user usually leaves the app to join a Wi-Fi
        // network or turn the hotspot on and expects to see it when they come back.
        networkTask = Task { [weak self] in
            while !Task.isCancelled {
                let address = await Task.detached { findLocalLanAddress() }.value
                self?.lan = address
                try? await Task.sleep(for: .seconds(3))
            }
        }
        updateIdleTimer()
        liveProfilesChanged()
    }

    /**
     * The app left the screen. A broadcast keeps going while its picture plays in Picture in
     * Picture; otherwise iOS allows about 30 seconds, after which the broadcast ends cleanly and
     * the receiver closes.
     */
    func enteredBackground() {
        active = false
        networkTask?.cancel()
        youtubeViews.values.forEach { $0.stop() }
        facebookViews.values.forEach { $0.stop() }
        guard relay.state.isActive else { return }
        let live = relay.state.isLive
        background.begin(warn: live && !picture.pictureInPicture) { [weak self] in
            guard let self, !self.active else { return }
            if self.picture.pictureInPicture { return }
            let notice = self.relay.state.isLive
                ? RelayNotice(title: .tr("ios_background_ended_title"), message: .tr("ios_background_ended"))
                : nil
            self.relay.stop(notice: notice)
        }
    }

    /// Picture in Picture closed while the app is away: it has its grace period from now.
    func pictureInPictureChanged() {
        if !active && !picture.pictureInPicture { enteredBackground() }
    }

    func finishGuide() {
        preferences.guideCompleted = true
        showGuide = false
        if active { becameActive() }
    }

    // MARK: Picture

    /// Called when the phase changes: the picture shows at once and stays 2.5 s after it stops.
    func phaseChanged() {
        pictureGrace?.cancel()
        if phase.hasPicture {
            showingPicture = true
        } else if showingPicture {
            pictureGrace = Task { [weak self] in
                try? await Task.sleep(for: .milliseconds(2_500))
                guard !Task.isCancelled else { return }
                self?.showingPicture = false
            }
        }
        updateIdleTimer()
    }

    var showPicture: Bool { relay.state.isLive || showingPicture }

    /// The phone is a monitor while the picture shows; it should not go dark mid-flight.
    private func updateIdleTimer() {
        UIApplication.shared.isIdleTimerDisabled = active && (showPicture || relay.state.isLive)
    }

    func setPictureFit(_ fit: PictureFit) {
        pictureFit = fit
        preferences.pictureFit = fit
    }

    func setTheme(_ choice: ThemeChoice) {
        theme = choice
        preferences.theme = choice
    }

    // MARK: Messages

    func showMessage(_ text: UiText, actionTitle: String? = nil, long: Bool = false, action: (() -> Void)? = nil) {
        snackbarTask?.cancel()
        snackbarAction = action
        let message = SnackbarMessage(text: text, actionTitle: actionTitle, long: long)
        snackbar = message
        snackbarTask = Task { [weak self] in
            try? await Task.sleep(for: .seconds(long ? 10 : 4))
            guard !Task.isCancelled, self?.snackbar == message else { return }
            self?.snackbar = nil
        }
    }

    func snackbarActionTapped() {
        let action = snackbarAction
        snackbar = nil
        snackbarAction = nil
        action?()
    }

    // MARK: Platforms

    /// Runs a store action; the error is returned and, unless the editor shows it, announced.
    @discardableResult
    func updateProfiles(announce: Bool = true, _ action: () throws -> DestinationProfiles) -> UiText? {
        do {
            destinations = try action()
            profileError = nil
            return nil
        } catch {
            let text = profileErrorText(error)
            if announce {
                profileError = text
                showMessage(text)
            }
            return text
        }
    }

    var liveDestinations: [DestinationProfile] {
        relay.state.liveProfileIds.compactMap { id in destinations.profiles.first { $0.id == id } }
    }

    /// The saved destination a platform tile stands for: a selected one, else the first.
    func profile(for kind: DestinationKind) -> DestinationProfile? {
        destinations.selectedProfiles.first { $0.kind == kind } ?? destinations.profiles.first { $0.kind == kind }
    }

    /// A saved platform goes in or out of the broadcast; a new one is set up first.
    private func platformClicked(_ kind: DestinationKind) {
        guard let profile = profile(for: kind) else {
            editor = ProfileEditorRequest(profile: nil, kind: kind)
            return
        }
        let select = !destinations.isSelected(profile.id)
        updateProfiles { try store.setSelected(profile.id, selected: select) }
        // TikTok, and Instagram set up by hand, hand out a new key for every broadcast: offer to
        // paste it now. The app prepares Instagram's itself.
        if select && renewsKeyByHand(profile) {
            showMessage(.tr("named_error", .text(kind.nameText), .text(.tr("key_changes_each_stream"))),
                        actionTitle: tr("update_key"), long: true) { [weak self] in
                self?.editor = ProfileEditorRequest(profile: profile, kind: kind)
            }
        }
    }

    /**
     * A platform's switch. Before going live it chooses the platform; while live it adds the
     * platform to the broadcast or ends the broadcast there, after asking.
     */
    func platformToggled(_ kind: DestinationKind) {
        let state = relay.state
        guard state.isLive else { return platformClicked(kind) }
        let liveProfile = destinations.profiles.first { $0.kind == kind && state.liveProfileIds.contains($0.id) }
        if let liveProfile {
            confirmEndLive(liveProfile.id)
        } else if let profile = profile(for: kind) {
            if !destinations.isSelected(profile.id) { updateProfiles { try store.setSelected(profile.id, selected: true) } }
            requestGoLive([profile.id])
        } else {
            editor = ProfileEditorRequest(profile: nil, kind: kind)
        }
    }

    func editPlatform(_ kind: DestinationKind) {
        if kind == .youtube && liveYouTube { showMessage(.tr("youtube_error_edit_live")); return }
        if kind == .facebook && liveFacebook { showMessage(.tr("facebook_error_edit_live")); return }
        if let profile = profile(for: kind) { editor = ProfileEditorRequest(profile: profile, kind: kind) }
    }

    func editProfile(_ profile: DestinationProfile) {
        if profile.kind == .youtube && liveYouTube { showMessage(.tr("youtube_error_edit_live")); return }
        if profile.kind == .facebook && liveFacebook { showMessage(.tr("facebook_error_edit_live")); return }
        editor = ProfileEditorRequest(profile: profile, kind: profile.kind)
    }

    /// Saves the key screen; returns the error for the screen to show, or nil once saved.
    func saveProfile(_ request: ProfileEditorRequest, serverUrl: String, streamKey: String) -> UiText? {
        if request.kind == .youtube && liveYouTube { return .tr("youtube_error_edit_live") }
        if request.kind == .facebook && liveFacebook { return .tr("facebook_error_edit_live") }
        // The store adds a new platform to the ones the broadcast goes to.
        let error = updateProfiles(announce: false) {
            try store.save(
                existingId: request.profile?.id,
                name: request.profile?.name ?? request.kind.displayName,
                kind: request.kind,
                serverUrl: serverUrl,
                streamKey: streamKey
            )
        }
        if error == nil {
            if request.kind == .instagram, let id = request.profile?.id ?? destinations.profiles.last?.id {
                // A pasted key: this profile goes out as it is, without the Producer page.
                instagramManualProfiles.insert(id)
                instagram.cancelPublish()
                instagram.hide()
            }
            editor = nil
            showMessage(request.profile == nil ? .tr("platform_added", .string(request.kind.displayName)) : .tr("saved"))
        }
        return error
    }

    /// The key has to be pasted again before each broadcast.
    func renewsKeyByHand(_ profile: DestinationProfile) -> Bool {
        profile.kind == .tiktok || (profile.kind == .instagram && instagramManualProfiles.contains(profile.id))
    }

    /// Instagram the app prepares and starts itself, rather than a pasted key.
    func isAutomaticInstagram(_ profile: DestinationProfile) -> Bool {
        profile.kind == .instagram && !instagramManualProfiles.contains(profile.id)
    }

    // MARK: Instagram

    var liveInstagram: Bool { liveDestinations.contains { $0.kind == .instagram } }

    /// The editor's Connect: prepares Instagram's Producer with the last title and audience.
    func connectInstagram(_ request: ProfileEditorRequest) -> UiText? {
        if liveInstagram { return .tr("instagram_error_edit_live") }
        instagram.prepare(InstagramCaptureTarget(profile: request.profile),
                          title: broadcastTitle ?? initialBroadcastTitle(saved: preferences.lastBroadcastTitle, profiles: destinations.selectedProfiles),
                          audience: broadcastAudience)
        return nil
    }

    /// Instagram's page showed the ingest: it is saved, and a waiting Go live continues.
    private func instagramCaptured(_ target: InstagramCaptureTarget, _ credentials: InstagramCredentials) {
        let existing = target.profile
        var savedId: String?
        let error = updateProfiles(announce: false) {
            let updated = try store.save(existingId: existing?.id, name: existing?.name ?? "Instagram", kind: .instagram,
                                         serverUrl: credentials.serverUrl, streamKey: credentials.streamKey)
            savedId = existing?.id ?? updated.profiles.last?.id
            return updated
        }
        guard error == nil, let savedId else {
            showMessage(error ?? .tr("save_failed"))
            return
        }
        instagram.connected(savedId)
        instagramManualProfiles.remove(savedId)
        if editor?.kind == .instagram { editor = nil }
        showMessage(.tr("instagram_connected"))
        if let start = target.startRequest { goLive(start) }
    }

    func openInstagram() { instagram.show() }

    func closeInstagramWindow() { instagram.cancelSetup() }

    func cancelInstagramSetup() { instagram.cancelSetup() }

    /// Instagram's start or end could not be confirmed: try again, or set it up again.
    func retryInstagram() {
        if instagram.publishStatus == "instagram_end_failed" { return instagram.retryEnd() }
        guard let profile = liveDestinations.first(where: { $0.kind == .instagram }) else { return }
        if instagram.hasConnection(profile.id) { return instagram.requestPublish(profile.id) }
        // The page was lost: its old key cannot be reused, so the platform starts over.
        relay.endLive(profile.id)
        let title = broadcastTitle ?? initialBroadcastTitle(saved: preferences.lastBroadcastTitle, profiles: destinations.selectedProfiles)
        guard let request = try? BroadcastStartRequest(profileIds: [profile.id], title: title, instagramAudience: broadcastAudience) else { return }
        instagram.prepare(InstagramCaptureTarget(profile: profile, startRequest: request), title: title, audience: broadcastAudience)
    }

    // MARK: YouTube status

    /// Starts asking YouTube about each live connected YouTube profile, and stops for the others.
    func liveProfilesChanged() {
        let live = liveDestinations.filter { $0.youtube != nil }
        for (id, view) in youtubeViews where !live.contains(where: { $0.id == id && $0.youtube == view.connection }) {
            view.stop()
            youtubeViews[id] = nil
        }
        for profile in live {
            guard let connection = profile.youtube else { continue }
            let view = youtubeViews[profile.id] ?? YouTubeBroadcastView(profileId: profile.id, connection: connection)
            youtubeViews[profile.id] = view
            if active { view.start(relay.youtube) }
        }
        let liveFacebook = liveDestinations.filter { $0.facebook != nil }
        for (id, view) in facebookViews where !liveFacebook.contains(where: { $0.id == id && $0.facebook == view.connection }) {
            view.stop()
            facebookViews[id] = nil
        }
        for profile in liveFacebook {
            guard let connection = profile.facebook else { continue }
            let view = facebookViews[profile.id] ?? FacebookBroadcastView(profileId: profile.id, connection: connection)
            facebookViews[profile.id] = view
            if active { view.start(relay.facebook) }
        }
    }

    func deleteProfile(_ profile: DestinationProfile) -> UiText? {
        if profile.youtube != nil { return .tr("youtube_error_auth") }
        if profile.facebook != nil { return .tr("facebook_error_auth") }
        let error = updateProfiles(announce: false) { try store.delete(profile.id) }
        if error == nil {
            editor = nil
            showMessage(.tr("platform_deleted", .string(profile.kind.displayName)))
        }
        return error
    }

    var liveYouTube: Bool { liveDestinations.contains { $0.kind == .youtube } }

    func saveYouTube(_ request: ProfileEditorRequest, email: String, channel: YouTubeChannel,
                     title: String, privacy: String, madeForKids: Bool) async -> UiText? {
        guard !liveYouTube else { return .tr("youtube_error_edit_live") }
        var created: (YouTubeAPI, YouTubeStream)?
        do {
            let title = try validateYouTubeTitle(title)
            let old = request.profile?.youtube
            let sameChannel = old?.accountEmail == email && old?.channelId == channel.id
            let stream: YouTubeStream
            if sameChannel, let old, let profile = request.profile {
                let credentials = try store.credentials(profile.id)
                stream = YouTubeStream(id: old.streamId, serverUrl: credentials.serverUrl, key: credentials.streamKey)
            } else {
                let api = try await YouTubeAuthorization.shared.api(for: email)
                try await api.checkChannel(channel.id)
                stream = try await api.createStream()
                created = (api, stream)
            }
            guard !liveYouTube else { throw YouTubeError("youtube_error_edit_live") }
            let connection = YouTubeConnection(accountEmail: email, channelId: channel.id, channelTitle: channel.title,
                streamId: stream.id, title: title, privacy: privacy, madeForKids: madeForKids)
            destinations = try store.save(existingId: request.profile?.id, name: request.profile?.name ?? "YouTube",
                kind: .youtube, serverUrl: stream.serverUrl, streamKey: stream.key, youtube: connection)
            profileError = nil
            editor = nil
            showMessage(.tr("saved"))
            return nil
        } catch {
            if let (api, stream) = created { try? await api.deleteStream(stream.id) }
            return youTubeErrorText(error)
        }
    }

    var liveFacebook: Bool { liveDestinations.contains { $0.kind == .facebook } }

    /// Saves a connected Facebook profile; Facebook's stream URL is fetched on every Go live instead.
    func saveFacebook(_ request: ProfileEditorRequest, connection: FacebookConnection) async -> UiText? {
        guard !liveFacebook else { return .tr("facebook_error_edit_live") }
        do {
            destinations = try store.save(existingId: request.profile?.id, name: request.profile?.name ?? "Facebook",
                kind: .facebook, serverUrl: DestinationKind.facebook.defaultServerUrl!, streamKey: "", facebook: connection)
            profileError = nil
            editor = nil
            showMessage(.tr("saved"))
            return nil
        } catch { return facebookErrorText(error) }
    }

    func disconnectFacebook(_ profile: DestinationProfile) async -> UiText? {
        guard !liveFacebook else { return .tr("facebook_error_edit_live") }
        guard profile.facebook != nil else { return nil }
        do {
            try await relay.facebook.finish(profileId: profile.id)
            try await FacebookAuthorization.shared.disconnect()
            destinations = try store.delete(profile.id)
            editor = nil
            showMessage(.tr("platform_deleted", .string("Facebook")))
            return nil
        } catch { return facebookErrorText(error) }
    }

    func disconnectYouTube(_ profile: DestinationProfile) async -> UiText? {
        guard !liveYouTube else { return .tr("youtube_error_edit_live") }
        guard let connection = profile.youtube else { return nil }
        do {
            try await relay.youtube.finish(profileId: profile.id)
            try await YouTubeAuthorization.shared.disconnect(email: connection.accountEmail)
            destinations = try store.delete(profile.id)
            editor = nil
            showMessage(.tr("platform_deleted", .string("YouTube")))
            return nil
        } catch { return youTubeErrorText(error) }
    }

    // MARK: Broadcast

    /**
     * Goes live on every chosen platform, or while live adds `profileIds` to the broadcast. A new
     * broadcast first asks for its title (and Instagram's audience); a platform added while live
     * keeps the broadcast's, unless it is Instagram, which needs its own preparation.
     */
    func requestGoLive(_ profileIds: [String]? = nil) {
        let ids = profileIds ?? destinations.selectedProfiles.map(\.id)
        guard !ids.isEmpty else {
            showMessage(.tr("pick_platform_first"))
            return
        }
        let addingInstagram = destinations.profiles.contains {
            ids.contains($0.id) && isAutomaticInstagram($0) && !relay.state.liveProfileIds.contains($0.id)
        }
        if relay.state.isLive && !addingInstagram,
           let request = try? BroadcastStartRequest(profileIds: ids, title: broadcastTitle ?? initialTitle, instagramAudience: broadcastAudience) {
            continueStart(request)
        } else {
            titleRequest = TitleRequest(profileIds: ids)
        }
    }

    /// The title the dialog starts with.
    var initialTitle: String {
        initialBroadcastTitle(saved: preferences.lastBroadcastTitle, profiles: destinations.selectedProfiles)
    }

    /// The title dialog's Go live.
    func startBroadcast(_ request: TitleRequest, title: String, audience: InstagramAudience) {
        guard let start = try? BroadcastStartRequest(profileIds: request.profileIds, title: title, instagramAudience: audience) else { return }
        titleRequest = nil
        preferences.lastBroadcastTitle = start.title
        preferences.instagramAudience = audience
        broadcastTitle = start.title
        broadcastAudience = audience
        continueStart(start)
    }

    private func continueStart(_ request: BroadcastStartRequest) {
        // The notification warns when iOS is about to stop a broadcast left in the background.
        if !preferences.notificationsAsked {
            preferences.notificationsAsked = true
            Task { [weak self] in
                let granted = (try? await UNUserNotificationCenter.current().requestAuthorization(options: [.alert, .sound])) ?? false
                if !granted { self?.showMessage(.tr("notification_permission_denied")) }
                self?.goLive(request)
            }
        } else {
            goLive(request)
        }
    }

    /// Starts the broadcast on the request's platforms; Instagram is prepared first when needed.
    private func goLive(_ request: BroadcastStartRequest) {
        let ids = request.profileIds
        let instagramProfile = destinations.profiles.first { ids.contains($0.id) && $0.kind == .instagram }
        if instagramProfile != nil && instagram.ending {
            showMessage(.tr("instagram_end_starting"))
            return
        }
        if instagramProfile != nil && instagram.rejectsTitle(request.title) {
            showMessage(.tr("broadcast_title_instagram_error"))
            titleRequest = TitleRequest(profileIds: ids)
            return
        }
        if let profile = instagramProfile, isAutomaticInstagram(profile), !relay.state.liveProfileIds.contains(profile.id),
           !instagram.prepareForTitle(profileId: profile.id, title: request.title, audience: request.instagramAudience) {
            // A saved key cannot bring back a lost Producer page. A fresh draft is prepared;
            // only this Go live continues once it is saved.
            instagram.prepare(InstagramCaptureTarget(profile: profile, startRequest: request),
                              title: request.title, audience: request.instagramAudience)
            return
        }
        let startInstagram = instagramProfile.flatMap { profile in
            isAutomaticInstagram(profile) && !relay.state.liveProfileIds.contains(profile.id) ? profile.id : nil
        }
        relay.goLive(ids, title: request.title)
        if let startInstagram { instagram.requestPublish(startInstagram) }
        updateIdleTimer()
    }

    /// Asks before ending; ending the last platform ends the whole broadcast.
    func confirmEndLive(_ profileId: String?) {
        endLive = EndLiveRequest(profileId: relay.state.liveProfileIds.count > 1 ? profileId : nil)
    }

    func endLiveConfirmed(_ request: EndLiveRequest) {
        endLive = nil
        let ended = request.profileId.map { [$0] } ?? relay.state.liveProfileIds
        guard let instagramId = instagram.profileToEnd(ended) else {
            relay.endLive(request.profileId)
            // The page that started Instagram is gone, so its End could not be pressed.
            if destinations.profiles.contains(where: { ended.contains($0.id) && isAutomaticInstagram($0) }) {
                showMessage(.tr("instagram_end_failed"))
            }
            updateIdleTimer()
            return
        }
        // Instagram's own End and confirmation run before its ingest is cut; the rest end now.
        for id in ended where id != instagramId { relay.endLive(id) }
        instagram.requestEnd(instagramId) { [weak self] success in
            guard let self else { return }
            self.relay.endLive(instagramId)
            self.showMessage(.tr(success ? "instagram_end_done" : "instagram_end_failed"))
            self.updateIdleTimer()
        }
    }

    func startReceiver(restart: Bool) {
        relay.startReceiver(restart: restart)
    }

    /// Turns the phone's microphone on or off for the broadcast, asking for it the first time.
    func toggleMicrophone() {
        if relay.state.microphoneOn {
            relay.setMicrophone(false)
            return
        }
        switch AVAudioApplication.shared.recordPermission {
        case .granted:
            microphoneOn()
        case .undetermined:
            AVAudioApplication.requestRecordPermission { [weak self] granted in
                Task { @MainActor in
                    if granted { self?.microphoneOn() } else { self?.showMessage(.tr("phone_mic_permission")) }
                }
            }
        default:
            showMessage(.tr("phone_mic_permission"))
        }
    }

    private func microphoneOn() {
        relay.setMicrophone(true)
        if relay.state.microphoneOn { showMessage(.tr("phone_mic_on")) }
    }

    func startTestVideo(_ selection: TestVideoSelection) {
        relay.startReceiver(testVideo: selection)
    }

    func stopTestVideo() {
        // Back to the connect card at once, without waiting out the grace period.
        if !relay.state.isLive {
            pictureGrace?.cancel()
            showingPicture = false
        }
        relay.stopTestVideo()
    }

    func copyAddress(_ url: String) {
        UIPasteboard.general.string = url
        showMessage(.tr("address_copied"))
    }
}

/// A saved-platforms failure in the app's words; an unexpected one keeps its technical cause.
private func profileErrorText(_ error: Error) -> UiText {
    (error as? DestinationProfileError)?.text ?? .withCause("profile_error_generic", error)
}
