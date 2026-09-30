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
    }

    /**
     * The app left the screen. A broadcast keeps going while its picture plays in Picture in
     * Picture; otherwise iOS allows about 30 seconds, after which the broadcast ends cleanly and
     * the receiver closes.
     */
    func enteredBackground() {
        active = false
        networkTask?.cancel()
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
        // Instagram and TikTok hand out a new key for every broadcast: offer to paste it now.
        if select && kind.keyChangesEachStream {
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
        if let profile = profile(for: kind) { editor = ProfileEditorRequest(profile: profile, kind: kind) }
    }

    func editProfile(_ profile: DestinationProfile) {
        editor = ProfileEditorRequest(profile: profile, kind: profile.kind)
    }

    /// Saves the key screen; returns the error for the screen to show, or nil once saved.
    func saveProfile(_ request: ProfileEditorRequest, serverUrl: String, streamKey: String) -> UiText? {
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
            editor = nil
            showMessage(request.profile == nil ? .tr("platform_added", .string(request.kind.displayName)) : .tr("saved"))
        }
        return error
    }

    func deleteProfile(_ profile: DestinationProfile) -> UiText? {
        let error = updateProfiles(announce: false) { try store.delete(profile.id) }
        if error == nil {
            editor = nil
            showMessage(.tr("platform_deleted", .string(profile.kind.displayName)))
        }
        return error
    }

    // MARK: Broadcast

    /// Goes live on every chosen platform, or while live adds `profileIds` to the broadcast.
    func requestGoLive(_ profileIds: [String]? = nil) {
        let ids = profileIds ?? destinations.selectedProfiles.map(\.id)
        guard !ids.isEmpty else {
            showMessage(.tr("pick_platform_first"))
            return
        }
        // The notification warns when iOS is about to stop a broadcast left in the background.
        if !preferences.notificationsAsked {
            preferences.notificationsAsked = true
            Task { [weak self] in
                let granted = (try? await UNUserNotificationCenter.current().requestAuthorization(options: [.alert, .sound])) ?? false
                if !granted { self?.showMessage(.tr("notification_permission_denied")) }
                self?.relay.goLive(ids)
            }
        } else {
            relay.goLive(ids)
        }
        updateIdleTimer()
    }

    /// Asks before ending; ending the last platform ends the whole broadcast.
    func confirmEndLive(_ profileId: String?) {
        endLive = EndLiveRequest(profileId: relay.state.liveProfileIds.count > 1 ? profileId : nil)
    }

    func endLiveConfirmed(_ request: EndLiveRequest) {
        endLive = nil
        relay.endLive(request.profileId)
        updateIdleTimer()
    }

    func startReceiver(restart: Bool) {
        relay.startReceiver(restart: restart)
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
