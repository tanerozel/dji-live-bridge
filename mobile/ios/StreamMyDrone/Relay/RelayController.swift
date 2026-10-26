import Foundation
import Observation
import os
import UIKit
import UserNotifications

/**
 * Keeps the receiver open so DJI Fly can connect and its picture shows on the phone, and sends
 * the stream to a platform only between "go live" and "end live". The iOS counterpart of the
 * Android app's foreground service: the native calls run on one serial queue, in order, and the
 * state the screen shows is published on the main actor.
 */
@MainActor
@Observable
final class RelayController {
    private(set) var state = RelayUiState()

    /// Native start, go-live, end-live and stop calls run here, one at a time and in order.
    @ObservationIgnored private let commands = DispatchQueue(label: "dji-relay-commands", qos: .userInitiated)
    @ObservationIgnored private var receiverStarted = false
    @ObservationIgnored private var receiverListening = false
    @ObservationIgnored private var pollTask: Task<Void, Never>?
    @ObservationIgnored private var testVideoTask: Task<Void, Never>?
    @ObservationIgnored private var converter: TestVideoConverter?
    /// Records while the pilot talks over the broadcast.
    @ObservationIgnored private var microphone: PhoneMicrophone?
    @ObservationIgnored let youtube = YouTubeBroadcastManager.shared
    @ObservationIgnored let facebook = FacebookBroadcastManager.shared
    @ObservationIgnored private var liveAttempts: [String: UUID] = [:]
    @ObservationIgnored private var liveFailures: [String: UiText] = [:]
    @ObservationIgnored private let profileStore: DestinationProfileStore

    init(profileStore: DestinationProfileStore = DestinationProfileStore()) {
        self.profileStore = profileStore
        Task { [weak self] in
            guard let self else { return }
            do { try await youtube.recoverOnce() }
            catch { state.notice = RelayNotice(title: .tr("youtube_error_end"), message: .tr("youtube_error_end")) }
        }
        Task { [weak self] in
            guard let self else { return }
            do { try await facebook.recoverOnce() }
            catch { state.notice = RelayNotice(title: .tr("facebook_error_end"), message: .tr("facebook_error_end")) }
        }
    }

    var phase: BridgePhase { bridgePhase(state) }

    // MARK: Receiver

    /**
     * Opens the receiver; with `testVideo` the picked file plays in place of DJI Fly, and with
     * `restart` a receiver that stopped on an error opens again (never while a broadcast is on).
     */
    func startReceiver(testVideo: TestVideoSelection? = nil, restart: Bool = false) {
        if restart && receiverStarted {
            if state.isLive { return }
            stopTestVideoWork()
            state.snapshot = RelaySnapshot(status: "starting")
            commands.async { _ = NativeRelay.startReceiver() }
        } else if !receiverStarted {
            receiverStarted = true
            // A note from before, such as a broadcast iOS ended in the background, stays up.
            state = RelayUiState(isActive: true, snapshot: RelaySnapshot(status: "starting"), notice: state.notice)
            commands.async { [weak self] in
                let error = NativeRelay.startReceiver()
                Task { @MainActor in
                    guard let self else { return }
                    if let error {
                        self.receiverStarted = false
                        self.state = RelayUiState(isActive: false, snapshot: RelaySnapshot(status: "error", error: nativeError(error)))
                    } else {
                        self.startPolling()
                    }
                }
            }
        }
        if let testVideo { startTestVideo(testVideo) }
    }

    /// Closes the receiver and ends any broadcast; `notice` says why, when it was not asked for.
    func stop(notice: RelayNotice? = nil) {
        let attempts = liveAttempts
        liveAttempts.removeAll()
        liveFailures.removeAll()
        finishRemote(attempts)
        stopTestVideoWork()
        stopMicrophone()
        pollTask?.cancel()
        pollTask = nil
        let wasStarted = receiverStarted
        receiverStarted = false
        receiverListening = false
        state = RelayUiState(
            isActive: false,
            snapshot: RelaySnapshot(status: "stopped", receivedBytes: state.snapshot.receivedBytes),
            notice: notice
        )
        if wasStarted { commands.async { NativeRelay.stop() } }
    }

    private func startPolling() {
        pollTask?.cancel()
        pollTask = Task { [weak self] in
            var previous = RelaySnapshot()
            while !Task.isCancelled {
                let snapshot: RelaySnapshot
                do {
                    snapshot = try RelaySnapshot.fromJson(NativeRelay.snapshotJson())
                } catch {
                    snapshot = RelaySnapshot(status: "error", error: .withCause("error_internal", error))
                }
                guard let self, !Task.isCancelled else { return }
                logInterruptions(previous, snapshot)
                previous = snapshot
                running(snapshot)
                try? await Task.sleep(for: .milliseconds(500))
            }
        }
    }

    private func running(_ snapshot: RelaySnapshot) {
        receiverListening = ["listening", "connected", "publishing"].contains(snapshot.status)
        var next = state
        next.isActive = true
        next.snapshot = snapshot
        if !state.isLive || snapshot.status != "publishing" {
            next.liveSince = nil
        } else if state.liveSince == nil && liveOutputStatuses.contains(snapshot.outputStatus) {
            next.liveSince = ProcessInfo.processInfo.systemUptime
        }
        if next != state { state = next }
    }

    /**
     * Leaves a line in the log for each pause or reconnect of the drone's stream and each change
     * of a platform's state, so a capture shows which side a gap in the broadcast came from.
     * Only codes and counts: never an address or a key.
     */
    private func logInterruptions(_ previous: RelaySnapshot, _ snapshot: RelaySnapshot) {
        if snapshot.stalls > previous.stalls {
            Logger.bridge.info("Drone video paused (\(snapshot.stalls) so far, longest \(snapshot.longestStallMs) ms, \(snapshot.recentInterruptions) in the last minute)")
        }
        if snapshot.sourceReconnects > previous.sourceReconnects {
            Logger.bridge.info("DJI Fly reconnected (\(snapshot.sourceReconnects) so far)")
        }
        for output in snapshot.outputs {
            let before = previous.output(output.id)?.status
            if output.status != before {
                let reason = output.reason.map { " (\($0))" } ?? ""
                Logger.bridge.info("Platform \(String(output.id.prefix(8)), privacy: .public): \(before ?? "added", privacy: .public) -> \(output.status, privacy: .public)\(reason, privacy: .public), \(output.droppedFrames) frames skipped so far")
            }
        }
    }

    // MARK: Broadcast

    /// Adds `profileIds` to the broadcast. A platform that fails is reported; the others go on.
    /// `title` is the broadcast's, for the platforms the app sets up itself.
    func goLive(_ profileIds: [String], title: String? = nil) {
        guard !profileIds.isEmpty else {
            state.notice = RelayNotice(title: .tr("go_live_failed"), message: .tr("pick_platform_first"))
            return
        }
        if !receiverStarted { startReceiver() }
        let added = profileIds.filter { !state.liveProfileIds.contains($0) }
        guard !added.isEmpty else { return }
        state.liveProfileIds += added
        state.notice = nil
        liveFailures.removeAll()
        for profileId in added {
            let owner = UUID()
            liveAttempts[profileId] = owner
            Task { [weak self] in
                guard let self else { return }
                do {
                    guard let profile = try profileStore.load().profiles.first(where: { $0.id == profileId }) else {
                        throw DestinationProfileError("profile_error_not_found")
                    }
                    let credentials: DestinationCredentials
                    if profile.facebook != nil {
                        // Facebook's stream URL belongs to this one live video; it is never stored.
                        let broadcastTitle = title ?? initialBroadcastTitle(saved: nil, profiles: [profile])
                        credentials = try await facebook.prepare(profile, title: broadcastTitle, owner: owner)
                    } else {
                        try await youtube.prepare(profile, title: title, owner: owner)
                        credentials = try profileStore.credentials(profileId)
                    }
                    guard liveAttempts[profileId] == owner else {
                        finishRemote([profileId: owner])
                        return
                    }
                    commands.async { [weak self] in
                        let problem = NativeRelay.goLive(outputId: profileId, serverUrl: credentials.serverUrl, streamKey: credentials.streamKey).map(nativeError)
                        if let problem {
                            Task { @MainActor in self?.goLiveFailed(profileId, owner: owner, error: problem) }
                        }
                    }
                } catch {
                    goLiveFailed(profileId, owner: owner, error: error is FacebookError ? facebookErrorText(error) : youTubeErrorText(error))
                }
            }
        }
    }

    private func goLiveFailed(_ profileId: String, owner: UUID, error: UiText) {
        finishRemote([profileId: owner])
        guard liveAttempts[profileId] == owner else { return }
        liveAttempts[profileId] = nil
        state.liveProfileIds.removeAll { $0 == profileId }
        if !state.isLive { state.liveSince = nil }
        let kind = (try? profileStore.load().profiles.first { $0.id == profileId })?.kind
        liveFailures[profileId] = kind.map { .tr("named_error", .text($0.nameText), .text(error)) } ?? error
        state.notice = RelayNotice(title: .tr("go_live_failed"),
            message: .joined(liveFailures.keys.sorted().compactMap { liveFailures[$0] }, separator: "\n"))
    }

    /// Ends the broadcast on `profileId`, or on every platform when nil; the drone stays connected.
    func endLive(_ profileId: String? = nil, notice: RelayNotice? = nil) {
        let ended = profileId.map { id in state.liveProfileIds.filter { $0 == id } } ?? state.liveProfileIds
        guard !ended.isEmpty else { return }
        var attempts: [String: UUID] = [:]
        for id in ended { attempts[id] = liveAttempts.removeValue(forKey: id) }
        state.liveProfileIds.removeAll(where: ended.contains)
        if !state.isLive { state.liveSince = nil }
        state.notice = notice
        let stillLive = state.isLive
        commands.async { NativeRelay.endLive(outputId: stillLive ? profileId : nil) }
        finishRemote(attempts)
    }

    /// Ends what the app created on YouTube or Facebook for these attempts; each has at most one.
    private func finishRemote(_ attempts: [String: UUID]) {
        for (id, owner) in attempts {
            Task { [weak self] in
                guard let self else { return }
                do { try await youtube.finish(profileId: id, owner: owner) }
                catch { state.notice = RelayNotice(title: .tr("youtube_error_end"), message: .tr("youtube_error_end")) }
            }
            Task { [weak self] in
                guard let self else { return }
                do { try await facebook.finish(profileId: id, owner: owner) }
                catch { state.notice = RelayNotice(title: .tr("facebook_error_end"), message: .tr("facebook_error_end")) }
            }
        }
    }

    // MARK: Microphone

    /// The pilot's voice from the phone in place of DJI Fly's audio, or DJI Fly's audio again.
    /// The caller has the microphone permission.
    func setMicrophone(_ on: Bool) {
        guard on != (microphone != nil), receiverStarted || !on else { return }
        guard on else {
            stopMicrophone()
            return
        }
        var started: PhoneMicrophone?
        let phoneMicrophone = PhoneMicrophone { [weak self] message in
            guard let self, self.microphone === started else { return }
            self.microphone = nil
            self.state.microphoneOn = false
            self.state.notice = RelayNotice(title: .tr("phone_mic"), message: message)
        }
        started = phoneMicrophone
        do {
            try phoneMicrophone.start()
        } catch {
            Logger.bridge.error("The phone's microphone could not start: \(error.localizedDescription, privacy: .public)")
            phoneMicrophone.stop()
            state.notice = RelayNotice(title: .tr("phone_mic"), message: .tr("phone_mic_failed", .string(error.localizedDescription)))
            return
        }
        microphone = phoneMicrophone
        state.microphoneOn = true
    }

    private func stopMicrophone() {
        microphone?.stop()
        microphone = nil
        state.microphoneOn = false
    }

    func dismissNotice() {
        state.notice = nil
    }

    // MARK: Test video

    /// Plays the picked video in place of DJI Fly, converted first to what DJI Fly sends when needed.
    private func startTestVideo(_ video: TestVideoSelection) {
        stopTestVideoWork()
        state.testVideoName = testVideoLabel(displayName: video.displayName, durationMs: video.durationMs)
        state.testVideoConverting = video.convert ? 0 : nil
        state.notice = nil
        let converter = video.convert ? TestVideoConverter() : nil
        self.converter = converter
        testVideoTask = Task { [weak self] in
            do {
                var url = video.url
                if let converter {
                    do {
                        url = try await converter.convert(source: video.url) { percent in
                            self?.state.testVideoConverting = percent
                        }
                    } catch is CancellationError {
                        return
                    } catch {
                        Logger.bridge.warning("The test video could not be converted: \(error.localizedDescription, privacy: .public)")
                        throw TestVideoError("test_video_convert_failed")
                    }
                    self?.state.testVideoConverting = nil
                }
                try await self?.awaitReceiver()
                try await TestVideoStreamer(url: url).run()
            } catch is CancellationError {
                return
            } catch {
                guard !Task.isCancelled, let self else { return }
                let detail = (error as? TestVideoError)?.text ?? .tr("test_video_failed", .string(error.localizedDescription))
                testVideoFailed(RelayNotice(title: .tr("test_video_stopped"), message: detail))
            }
        }
    }

    private func testVideoFailed(_ notice: RelayNotice) {
        testVideoTask = nil
        converter = nil
        state.testVideoName = nil
        state.testVideoConverting = nil
        state.notice = notice
    }

    private func awaitReceiver() async throws {
        let deadline = ContinuousClock.now + .seconds(5)
        while !receiverListening {
            if ContinuousClock.now > deadline { throw TestVideoError("test_video_receiver_not_ready") }
            try await Task.sleep(for: .milliseconds(50))
        }
    }

    func stopTestVideo() {
        stopTestVideoWork()
        state.testVideoName = nil
        state.testVideoConverting = nil
    }

    /// Ends what the test video is doing, converting or playing.
    private func stopTestVideoWork() {
        converter?.cancel()
        converter = nil
        testVideoTask?.cancel()
        testVideoTask = nil
    }
}

// MARK: - Background

/**
 * iOS suspends an app shortly after it leaves the screen, which would freeze the receiver and
 * every platform connection. While the picture plays in Picture in Picture the app keeps
 * running; otherwise it gets the usual short grace period, warns with a notification before it
 * runs out, and then ends the broadcast cleanly rather than letting the platforms time out.
 */
@MainActor
final class BackgroundGuard {
    private var task: UIBackgroundTaskIdentifier = .invalid
    private var warning: Task<Void, Never>?

    /// The app went to the background; `onExpire` runs when iOS is about to suspend it.
    func begin(warn: Bool, onExpire: @escaping @MainActor () -> Void) {
        end()
        task = UIApplication.shared.beginBackgroundTask(withName: "relay") { [weak self] in
            MainActor.assumeIsolated {
                onExpire()
                self?.end()
            }
        }
        guard warn else { return }
        // iOS grants about 30 seconds; the warning goes out while there is time to come back.
        warning = Task {
            try? await Task.sleep(for: .seconds(max(1, min(UIApplication.shared.backgroundTimeRemaining, 30) - 12)))
            guard !Task.isCancelled else { return }
            let content = UNMutableNotificationContent()
            content.title = tr("ios_background_warning_title")
            content.body = tr("ios_background_warning_body")
            content.interruptionLevel = .timeSensitive
            try? await UNUserNotificationCenter.current().add(
                UNNotificationRequest(identifier: "background-warning", content: content, trigger: nil)
            )
        }
    }

    func end() {
        warning?.cancel()
        warning = nil
        UNUserNotificationCenter.current().removeDeliveredNotifications(withIdentifiers: ["background-warning"])
        if task != .invalid {
            UIApplication.shared.endBackgroundTask(task)
            task = .invalid
        }
    }
}
