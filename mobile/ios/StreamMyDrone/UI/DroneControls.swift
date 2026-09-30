import SwiftUI

/// Everything over the picture, laid out for an upright phone or one turned sideways.
struct DroneControls: View {
    @Environment(AppModel.self) private var model
    @Binding var sheet: DroneSheet?
    let showPicture: Bool
    let onPictureOnly: () -> Void
    let onFitToggle: () -> Void
    let onTestVideo: () -> Void
    let inspectingVideo: Bool

    var body: some View {
        GeometryReader { geometry in
            let landscape = geometry.size.width > geometry.size.height
            Group {
                if landscape && !showPicture {
                    // Sideways without a picture: the connect card beside the platforms, not
                    // squeezed between the top and bottom rows.
                    HStack(alignment: .top, spacing: Metrics.edge) {
                        VStack(alignment: .leading, spacing: Metrics.gap) {
                            deviceCard.frame(maxWidth: .infinity)
                            chips
                            Spacer(minLength: 0)
                            bottom
                        }
                        VStack(spacing: Metrics.gap) {
                            HStack(alignment: .top, spacing: Metrics.gap) {
                                statusCard.frame(maxWidth: .infinity)
                                sideButtons
                            }
                            CenteredScroll { connect }
                            actions
                        }
                    }
                } else if landscape {
                    // Sideways: the state along the top, the platforms and the buttons along the bottom.
                    VStack(spacing: Metrics.gap) {
                        HStack(alignment: .top, spacing: Metrics.gap) {
                            VStack(alignment: .leading, spacing: Metrics.gap) {
                                deviceCard.frame(maxWidth: 280, alignment: .leading)
                                chips
                            }
                            Spacer(minLength: 0)
                            VStack(alignment: .trailing, spacing: Metrics.gap) {
                                statusCard.frame(maxWidth: 220)
                                sideButtons
                            }
                        }
                        Spacer(minLength: 0)
                        HStack(alignment: .bottom, spacing: Metrics.edge) {
                            VStack(spacing: Metrics.gap) { bottom }.frame(maxWidth: 440)
                            Spacer(minLength: 0)
                            actions.frame(width: Metrics.landscapeActionsWidth)
                        }
                    }
                } else {
                    VStack(spacing: Metrics.gap) {
                        // The device card a little wider than the state card, as on Android.
                        let cards = geometry.size.width - Metrics.edge * 2 - Metrics.gap
                        HStack(alignment: .top, spacing: Metrics.gap) {
                            deviceCard.frame(width: cards * 1.15 / 2.15)
                            statusCard.frame(width: cards / 2.15)
                        }
                        chips
                        ZStack(alignment: .topTrailing) {
                            // The card takes the middle; the settings button sits above its corner.
                            CenteredScroll { connect.padding(.top, Metrics.sideButton + Metrics.gap) }
                            sideButtons
                        }
                        .frame(maxHeight: .infinity)
                        bottom
                        actions
                    }
                }
            }
            .padding(.horizontal, Metrics.edge)
            .padding(.vertical, Metrics.gap)
        }
    }

    private var state: RelayUiState { model.relay.state }
    private var testing: Bool { state.testVideoName != nil }

    private var deviceCard: some View {
        DeviceCard(phase: model.phase, testing: testing) { sheet = .details }
    }

    private var statusCard: some View {
        StatusCard(look: statusLook(phase: model.phase, state: state, target: LiveTarget(model.liveDestinations)))
    }

    private var chips: some View {
        InfoChips(
            videoSize: showPicture ? model.picture.videoSize : nil,
            bitrateKbps: state.snapshot.bitrateKbps,
            lan: model.lan
        )
    }

    @ViewBuilder private var connect: some View {
        if !showPicture {
            ConnectPanel(inspectingVideo: inspectingVideo, onTestVideo: onTestVideo)
                .frame(maxWidth: 420)
                .frame(maxWidth: .infinity)
        }
    }

    private var sideButtons: some View {
        VStack(spacing: Metrics.gap) {
            let whole = model.picture.shownFit == .whole
            if showPicture {
                GlassIconButton(
                    systemImage: whole ? "arrow.up.left.and.arrow.down.right" : "arrow.down.right.and.arrow.up.left",
                    label: tr(whole ? "picture_fill" : "picture_fit"),
                    action: onFitToggle
                )
            }
            Menu {
                Button { sheet = .language } label: { Label(tr("language"), systemImage: "globe") }
                Button { sheet = .theme } label: { Label(tr("theme"), systemImage: "paintpalette") }
                Button { model.showGuide = true } label: { Label(tr("how_to_use"), systemImage: "questionmark.circle") }
            } label: {
                GlassIconLabel(systemImage: "gearshape")
            }
            .accessibilityLabel(tr("settings"))
            if showPicture {
                GlassIconButton(systemImage: "photo", label: tr("preview_badge"), action: onPictureOnly)
            }
            if testing {
                GlassIconButton(systemImage: "stop.fill", label: tr("stop_test_video"), action: model.stopTestVideo)
            }
        }
    }

    @ViewBuilder private var bottom: some View {
        VStack(spacing: Metrics.gap) {
            if let snackbar = model.snackbar {
                Snackbar(message: snackbar, onAction: model.snackbarActionTapped)
                    .transition(.move(edge: .bottom).combined(with: .opacity))
            }
            MessageBanner(showPicture: showPicture)
            StreamToCard(onMore: { sheet = .platforms })
        }
        .animation(.easeInOut(duration: 0.2), value: model.snackbar)
    }

    private var actions: some View {
        ActionRow(
            live: state.isLive,
            canGoLive: model.phase.hasPicture,
            onPlatforms: { sheet = .platforms },
            onDetails: { sheet = .details }
        )
    }
}

/// Centered in the space it has, and scrollable when the text is too large for it.
private struct CenteredScroll<Content: View>: View {
    @ViewBuilder let content: Content

    var body: some View {
        GeometryReader { geometry in
            ScrollView {
                content.frame(maxWidth: .infinity, minHeight: geometry.size.height)
            }
            .scrollBounceBehavior(.basedOnSize)
        }
    }
}

// MARK: - Top

/// What sends the picture, and whether it is connected; opens the connection details.
private struct DeviceCard: View {
    let phase: BridgePhase
    let testing: Bool
    let onTap: () -> Void

    var body: some View {
        let (dot, stateKey): (Color, String) = switch phase {
        case .receiverError, .startFailed: (DroneColors.danger, "hero_receiver_stopped")
        case _ where phase.hasPicture || phase == .droneConnected: (DroneColors.ready, "device_connected")
        default: (DroneColors.warning, "device_waiting")
        }
        Button(action: onTap) {
            HStack(spacing: 8) {
                Group {
                    if testing {
                        Image(systemName: "film").resizable().scaledToFit()
                    } else {
                        Image("drone").resizable().renderingMode(.template).scaledToFit()
                    }
                }
                .frame(width: Metrics.icon, height: Metrics.icon)
                .foregroundStyle(.white)
                VStack(alignment: .leading, spacing: 1) {
                    FittedText(text: tr(testing ? "device_test_video" : "device_drone"), font: .subheadline, weight: .semibold)
                    HStack(spacing: 5) {
                        Circle().fill(dot).frame(width: 6, height: 6)
                        Text(tr(stateKey)).font(.caption).foregroundStyle(.white.opacity(0.8)).lineLimit(1)
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                Image(systemName: "chevron.forward").font(.caption.weight(.semibold)).foregroundStyle(.white.opacity(0.7))
            }
            .padding(.horizontal, 10)
            .padding(.vertical, 8)
            .frame(minHeight: Metrics.cardHeight)
            .glass()
        }
        .buttonStyle(.plain)
        .accessibilityHint(tr("show_details"))
    }
}

/// The colors of a state: ready, live, working on it, a warning or an error.
private enum Tone {
    case ready, live, busy, warning, error

    var accent: Color {
        switch self {
        case .ready: DroneColors.ready
        case .live: Color(hex: 0xF87171)
        case .busy: .white
        case .warning: DroneColors.warning
        case .error: DroneColors.danger
        }
    }

    var tint: Color {
        switch self {
        case .ready: Color(hex: 0x052E16)
        case .live, .error: Color(hex: 0x450A0A)
        case .busy: Color(hex: 0x111827)
        case .warning: Color(hex: 0x451A03)
        }
    }
}

private enum Leading { case check, liveDot, progress, warning, error }

private struct StatusLook {
    let tone: Tone
    let leading: Leading
    let headline: String?
    var caption: String?
    var liveSince: TimeInterval?
}

/// How the screen names where the stream goes: one platform by name, several by their count.
struct LiveTarget {
    let single: DestinationKind?
    let count: Int

    init(_ destinations: [DestinationProfile]) {
        single = destinations.count == 1 ? destinations[0].kind : nil
        count = destinations.count
    }

    var label: String { single?.displayName ?? trPlural("platform_count", count) }
}

/// The stream's state in a word, with a line under it, for the card in the top corner.
private func statusLook(phase: BridgePhase, state: RelayUiState, target: LiveTarget) -> StatusLook {
    let snapshot = state.snapshot
    if !state.isLive {
        switch phase {
        case .preview: return StatusLook(tone: .ready, leading: .check, headline: tr("status_ready"), caption: tr("status_not_live"))
        case .idle, .startFailed, .receiverError: return StatusLook(tone: .error, leading: .error, headline: tr("hero_receiver_stopped"))
        case .starting: return StatusLook(tone: .busy, leading: .progress, headline: nil, caption: tr("receiver_starting"))
        default: return StatusLook(tone: .busy, leading: .progress, headline: nil, caption: tr("status_waiting_for_picture"))
        }
    }
    switch phase {
    case .live:
        let congested = snapshot.outputs.contains { $0.status == "congested" }
        return StatusLook(
            tone: congested ? .warning : .live,
            leading: congested ? .warning : .liveDot,
            headline: tr("live_badge"),
            caption: congested ? tr("status_congested") : formatBitrate(snapshot.bitrateKbps),
            liveSince: state.liveSince
        )
    case .preview, .connectingTarget:
        return StatusLook(tone: .busy, leading: .progress, headline: tr("status_connecting"), caption: target.label)
    case .reconnecting:
        return StatusLook(tone: .warning, leading: .warning, headline: tr("status_reconnecting"), caption: target.label)
    case .starting, .waitingForDrone, .droneConnected:
        return snapshot.outputStatus == "holding"
            ? StatusLook(tone: .warning, leading: .warning, headline: nil, caption: tr("status_holding"))
            : StatusLook(tone: .busy, leading: .progress, headline: nil, caption: tr("status_waiting_for_picture"))
    case .receiverError: return StatusLook(tone: .error, leading: .error, headline: tr("hero_receiver_stopped"))
    case .idle, .startFailed: return StatusLook(tone: .error, leading: .error, headline: tr("status_stopped"))
    }
}

private struct StatusCard: View {
    let look: StatusLook

    var body: some View {
        let shape = RoundedRectangle(cornerRadius: Metrics.cardRadius, style: .continuous)
        HStack(spacing: 8) {
            switch look.leading {
            case .check: Image(systemName: "checkmark.circle.fill").foregroundStyle(look.tone.accent).font(.title3)
            case .liveDot: PulsingDot(color: look.tone.accent, size: 10)
            case .progress: ProgressView().tint(.white).controlSize(.small)
            case .warning: Image(systemName: "exclamationmark.triangle").foregroundStyle(look.tone.accent)
            case .error: Image(systemName: "exclamationmark.circle").foregroundStyle(look.tone.accent)
            }
            VStack(alignment: .leading, spacing: 1) {
                if let headline = look.headline {
                    // One line that shrinks to fit: "RECONNECTING" is long in most languages.
                    Text(headline.uppercased(with: Localizer.shared.locale))
                        .font(.headline.weight(.bold))
                        .foregroundStyle(look.tone.accent)
                        .lineLimit(1)
                        .minimumScaleFactor(0.6)
                }
                HStack(spacing: 0) {
                    if let since = look.liveSince {
                        LiveTimer(since: since)
                        caption(" · ")
                    }
                    if let text = look.caption { caption(text) }
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 8)
        .frame(minHeight: Metrics.cardHeight)
        .background(look.tone.tint.opacity(0.78), in: shape)
        .overlay(shape.strokeBorder(look.tone.accent.opacity(0.75), lineWidth: 1))
        .accessibilityElement(children: .combine)
    }

    private func caption(_ text: String) -> some View {
        Text(text).font(.caption2).foregroundStyle(.white.opacity(0.9)).lineLimit(2)
    }
}

/// The picture's size, the incoming bitrate and how the phone is connected.
private struct InfoChips: View {
    let videoSize: CGSize?
    let bitrateKbps: Double
    let lan: LanAddress?

    var body: some View {
        HStack(spacing: 6) {
            // Numbers read left to right in every language: "1280×720" must not turn into "720×1280".
            if let videoSize { chip("rectangle.on.rectangle", "\(Int(videoSize.width))×\(Int(videoSize.height))", numbers: true) }
            if bitrateKbps > 0 { chip("cellularbars", formatBitrate(bitrateKbps), numbers: true) }
            if let kind = lan?.kind {
                switch kind {
                case .wifi: chip("wifi", tr("net_wifi"))
                case .hotspot: chip("personalhotspot", tr("net_hotspot"))
                case .wired, .other: chip("network", tr("net_local"))
                }
            }
            Spacer(minLength: 0)
        }
    }

    private func chip(_ systemImage: String, _ text: String, numbers: Bool = false) -> some View {
        HStack(spacing: 5) {
            Image(systemName: systemImage).font(.system(size: 11, weight: .semibold))
            Text(text).font(.caption.weight(.medium)).lineLimit(1)
                .environment(\.layoutDirection, numbers ? .leftToRight : Localizer.shared.isRightToLeft ? .rightToLeft : .leftToRight)
        }
        .foregroundStyle(.white)
        .padding(.horizontal, 10)
        .padding(.vertical, 5)
        .glass(Capsule())
    }
}

struct GlassIconLabel: View {
    let systemImage: String

    var body: some View {
        Image(systemName: systemImage)
            .font(.system(size: 17, weight: .medium))
            .foregroundStyle(.white)
            .frame(width: Metrics.sideButton, height: Metrics.sideButton)
            .glass(Circle())
    }
}

private struct GlassIconButton: View {
    let systemImage: String
    let label: String
    let action: () -> Void

    var body: some View {
        Button(action: action) { GlassIconLabel(systemImage: systemImage) }
            .buttonStyle(.plain)
            .accessibilityLabel(label)
    }
}

// MARK: - Bottom

/// A short message above the platforms, with an optional action such as "Update key".
private struct Snackbar: View {
    let message: SnackbarMessage
    let onAction: () -> Void

    var body: some View {
        HStack(spacing: 12) {
            Text(message.text.resolve()).font(.subheadline).foregroundStyle(.white).frame(maxWidth: .infinity, alignment: .leading)
            if let title = message.actionTitle {
                Button(title, action: onAction).font(.subheadline.weight(.semibold)).foregroundStyle(DroneColors.goLiveStart)
            }
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 12)
        .background(DroneColors.sheet, in: RoundedRectangle(cornerRadius: Metrics.cardRadius, style: .continuous))
        .accessibilityElement(children: .combine)
    }
}

/**
 * The one message worth reading now: why going live failed, why the drone's picture is gone, or
 * the tip for the platform (such as pressing "Go live" in Instagram too).
 */
private struct MessageBanner: View {
    @Environment(AppModel.self) private var model
    let showPicture: Bool
    @State private var closed: Set<String> = []

    var body: some View {
        if let content, !closed.contains(content.key) {
            HStack(alignment: .top, spacing: 8) {
                Image(systemName: content.systemImage).font(.footnote).foregroundStyle(content.accent).padding(.top, 1)
                VStack(alignment: .leading, spacing: 1) {
                    if let title = content.title { Text(title).font(.subheadline.weight(.semibold)).foregroundStyle(.white) }
                    if let message = content.message { Text(message).font(.caption).foregroundStyle(.white.opacity(0.85)) }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                Button {
                    closed.insert(content.key)
                    if content.key.hasPrefix("notice:") { model.relay.dismissNotice() }
                } label: {
                    Image(systemName: "xmark").font(.caption.weight(.semibold)).foregroundStyle(.white.opacity(0.8)).frame(width: 32, height: 32)
                }
                .accessibilityLabel(tr("close"))
            }
            .padding(.leading, 12)
            .padding(.vertical, 8)
            .glass()
            .accessibilityElement(children: .combine)
        }
    }

    private struct Content {
        let key: String
        let title: String?
        let message: String?
        let systemImage: String
        let accent: Color
    }

    private var content: Content? {
        let state = model.relay.state
        let phase = model.phase
        let snapshot = state.snapshot
        let holding = state.isLive && snapshot.outputStatus == "holding" && (phase == .waitingForDrone || phase == .droneConnected)
        // A test video starting and stopping counts as reconnects; only a flight says anything.
        let weakLink = showPicture && state.testVideoName == nil && snapshot.recentInterruptions >= 3
        if let notice = state.notice {
            let title = notice.title.resolve()
            let message = notice.message.resolve()
            return Content(key: "notice:\(title)\(message)", title: title, message: message, systemImage: "exclamationmark.circle", accent: DroneColors.danger)
        }
        if let error = model.profileError {
            return Content(key: "profile", title: tr("profile_list_unreadable"), message: error.resolve(), systemImage: "exclamationmark.circle", accent: DroneColors.danger)
        }
        if holding {
            return Content(key: "holding", title: tr("hero_drone_lost"), message: tr("hero_drone_lost_subtitle"), systemImage: "exclamationmark.triangle", accent: DroneColors.warning)
        }
        if phase == .receiverError && showPicture {
            return Content(key: "receiver", title: tr("hero_receiver_stopped"), message: snapshot.error?.resolve(), systemImage: "exclamationmark.circle", accent: DroneColors.danger)
        }
        if weakLink {
            let tip = model.lan?.kind == .hotspot ? "weak_link_tip_hotspot" : "weak_link_tip"
            return Content(key: "weak_link", title: tr("weak_link_title"), message: tr(tip), systemImage: "wifi.slash", accent: DroneColors.warning)
        }
        if let tip = liveTips(phase: phase, snapshot: snapshot, destinations: model.liveDestinations).first {
            return Content(key: tip, title: nil, message: tip, systemImage: "lightbulb", accent: DroneColors.warning)
        }
        return nil
    }
}

/// What the user can do about the stream's state right now, most urgent first.
func liveTips(phase: BridgePhase, snapshot: RelaySnapshot, destinations: [DestinationProfile]) -> [String] {
    let congested = snapshot.outputs.contains { $0.status == "congested" }
    let keys: [String]
    if destinations.isEmpty {
        keys = []
    } else if phase == .reconnecting && snapshot.outputs.contains(where: { $0.reconnectAttempts >= 3 }) {
        keys = ["tip_check_key"]
    } else if phase == .receiverError {
        keys = ["tip_receiver_error"]
    } else if phase == .live && congested {
        keys = ["tip_congested"]
    } else if phase == .live {
        var seen = Set<String>()
        keys = destinations.compactMap(\.kind.liveReminderKey).filter { seen.insert($0).inserted }
    } else {
        keys = []
    }
    return keys.map { tr($0) }
}

/**
 * A switch per platform. Before going live it chooses where the broadcast goes; while live it
 * starts or ends the broadcast on that platform. A platform without a stream key asks for one.
 */
private struct StreamToCard: View {
    @Environment(AppModel.self) private var model
    let onMore: () -> Void

    var body: some View {
        let saved = Set(model.destinations.profiles.map(\.kind))
        // The user's own platforms first, then the four big ones; the others once they have a
        // key. Switching one on or off never moves it.
        let shown = DestinationKind.allCases.filter(saved.contains) +
            DestinationKind.allCases.filter { mainPlatforms.contains($0) && !saved.contains($0) }
        let hidden = DestinationKind.allCases.count > shown.count
        let on = switchedOn(model)
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                Text(tr("platforms")).font(.subheadline.weight(.semibold)).foregroundStyle(.white)
                Spacer()
                Text(tr("platforms_active", shown.filter(on.contains).count, shown.count))
                    .font(.caption.weight(.medium))
                    .foregroundStyle(.white.opacity(0.7))
            }
            .padding(.horizontal, 2)
            GeometryReader { geometry in
                let width = max((geometry.size.width - Metrics.tileSpacing * (Metrics.visibleTiles - 1)) / Metrics.visibleTiles, Metrics.minTileWidth)
                ScrollView(.horizontal, showsIndicators: false) {
                    HStack(spacing: Metrics.tileSpacing) {
                        ForEach(shown) { kind in
                            PlatformSwitch(kind: kind, checked: on.contains(kind), saved: saved.contains(kind), state: liveState(kind))
                                .frame(width: width)
                        }
                        if hidden { OtherPlatforms(onTap: onMore).frame(width: width) }
                    }
                }
            }
            .frame(height: platformSwitchHeight)
        }
        .padding(10)
        .glass(radius: 18)
    }

    private func liveState(_ kind: DestinationKind) -> Color? {
        model.liveDestinations.first { $0.kind == kind }.map { outputTone(model.relay.state.snapshot.output($0.id)?.status, palette: .dark) }
    }
}

private let mainPlatforms: Set<DestinationKind> = [.instagram, .tiktok, .youtube, .facebook]
/// A tile's height: padding, the platform tile, its name and the switch.
private let platformSwitchHeight: CGFloat = 8 + Metrics.tileIcon + 6 + 16 + 6 + 18 + 8

/// The platforms the switches show as on: the live ones while live, else the chosen ones.
@MainActor
func switchedOn(_ model: AppModel) -> Set<DestinationKind> {
    model.relay.state.isLive
        ? Set(model.liveDestinations.map(\.kind))
        : Set(model.destinations.selectedProfiles.map(\.kind))
}

struct PlatformSwitch: View {
    @Environment(AppModel.self) private var model
    let kind: DestinationKind
    let checked: Bool
    let saved: Bool
    /// The platform's state as a dot while live.
    let state: Color?

    var body: some View {
        let shape = RoundedRectangle(cornerRadius: Metrics.tileRadius, style: .continuous)
        VStack(spacing: 6) {
            PlatformTile(kind: kind, size: Metrics.tileIcon)
                .overlay(alignment: .bottomTrailing) {
                    if let state {
                        Circle().fill(state).frame(width: 9, height: 9)
                            .overlay(Circle().strokeBorder(.black, lineWidth: 1.5))
                            .offset(x: 3, y: 3)
                    }
                }
            FittedText(text: kind.displayName, font: .caption2, color: .white.opacity(saved ? 1 : 0.7))
                .padding(.horizontal, 4)
            MiniSwitch(checked: checked)
        }
        .padding(.vertical, 8)
        .frame(maxWidth: .infinity)
        .background(.white.opacity(checked ? 0.12 : 0.05), in: shape)
        .overlay(shape.strokeBorder(checked ? DroneColors.goLiveStart.opacity(0.8) : .white.opacity(0.12), lineWidth: 1))
        .contentShape(shape)
        .onTapGesture { model.platformToggled(kind) }
        .onLongPressGesture { if saved { model.editPlatform(kind) } }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(kind.displayName)
        .accessibilityValue(saved ? "" : tr("state_not_added"))
        .accessibilityAddTraits(.isButton)
        .accessibilityAddTraits(checked ? .isSelected : [])
        .accessibilityAction { model.platformToggled(kind) }
        .accessibilityAction(named: tr("edit")) { if saved { model.editPlatform(kind) } }
    }
}

/// Twitch, Kick and custom servers, until they have a stream key.
private struct OtherPlatforms: View {
    let onTap: () -> Void

    var body: some View {
        let shape = RoundedRectangle(cornerRadius: Metrics.tileRadius, style: .continuous)
        Button(action: onTap) {
            VStack(spacing: 6) {
                RoundedRectangle(cornerRadius: 8, style: .continuous)
                    .strokeBorder(.white.opacity(0.5), lineWidth: 1)
                    .frame(width: Metrics.tileIcon, height: Metrics.tileIcon)
                    .overlay(Image(systemName: "plus").font(.system(size: 15, weight: .semibold)).foregroundStyle(.white))
                FittedText(text: tr("other_platforms"), font: .caption2).padding(.horizontal, 4)
            }
            .padding(.vertical, 8)
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
            .overlay(shape.strokeBorder(.white.opacity(0.12), lineWidth: 1))
            .contentShape(shape)
        }
        .buttonStyle(.plain)
    }
}

/// The big button and "More", like a camera app's shutter row.
private struct ActionRow: View {
    @Environment(AppModel.self) private var model
    let live: Bool
    let canGoLive: Bool
    let onPlatforms: () -> Void
    let onDetails: () -> Void

    var body: some View {
        HStack(alignment: .top, spacing: 10) {
            if live {
                BigButton(title: tr("end_broadcast"), systemImage: "stop.fill",
                          colors: [Color(hex: 0xEF4444), Color(hex: 0xDC2626)]) { model.confirmEndLive(nil) }
            } else {
                BigButton(title: tr("go_live"), systemImage: "dot.radiowaves.left.and.right",
                          colors: [DroneColors.goLiveStart, DroneColors.goLiveEnd], enabled: canGoLive) {
                    if canGoLive {
                        model.requestGoLive()
                    } else {
                        // Says why nothing happens, rather than ignoring the tap.
                        model.showMessage(.tr("go_live_needs_picture"))
                    }
                }
            }
            Menu {
                if !live { Button { onPlatforms() } label: { Label(tr("platforms"), systemImage: "square.grid.2x2") } }
                Button { onDetails() } label: { Label(tr("technical_details"), systemImage: "info.circle") }
            } label: {
                // The label may be wider than the square ("Daha fazla"); the column grows a little.
                VStack(spacing: 3) {
                    Image(systemName: "ellipsis")
                        .font(.system(size: 20, weight: .semibold))
                        .foregroundStyle(.white)
                        .frame(width: Metrics.squareAction, height: Metrics.squareAction)
                        .glass()
                    Text(tr("more")).font(.caption2).foregroundStyle(.white).lineLimit(1)
                }
                .frame(minWidth: Metrics.squareAction, maxWidth: Metrics.squareActionMaxWidth)
            }
        }
    }
}

/// The broadcast button: orange to go live, red to end it. Its text shrinks to fit any language.
private struct BigButton: View {
    let title: String
    let systemImage: String
    let colors: [Color]
    var enabled = true
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack(spacing: 8) {
                Image(systemName: systemImage).font(.system(size: 18, weight: .semibold))
                Text(title).font(.system(size: 16, weight: .semibold)).lineLimit(1).minimumScaleFactor(0.65)
            }
            .foregroundStyle(.white)
            .padding(.horizontal, 16)
            .frame(maxWidth: .infinity, minHeight: Metrics.squareAction, maxHeight: Metrics.squareAction)
            .background(LinearGradient(colors: colors, startPoint: .leading, endPoint: .trailing), in: Capsule())
            .opacity(enabled ? 1 : 0.5)
        }
        .buttonStyle(.plain)
    }
}

/// A platform output's state as a color, for the dots on the tiles and in the details.
func outputTone(_ status: String?, palette: BridgePalette) -> Color {
    switch status {
    case "forwarding": palette.success
    case "congested", "reconnecting", "holding": palette.warningText
    case "ready", "connecting": palette.accent
    case "stopped", "error": palette.dangerText
    default: palette.faint
    }
}

func outputLabelKey(_ status: String?) -> String {
    switch status {
    case "forwarding": "status_live"
    case "congested": "status_congested"
    case "ready", "connecting": "status_connecting"
    case "reconnecting": "status_reconnecting"
    case "holding": "status_holding"
    case "stopped", "error": "status_stopped"
    default: "status_waiting_for_picture"
    }
}
