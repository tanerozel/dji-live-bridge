import SwiftUI

/**
 * The middle of the screen until the drone's picture comes: the address to type into DJI Fly, or
 * what keeps DJI Fly from connecting (the receiver is off, the phone is not on Wi-Fi).
 */
struct ConnectPanel: View {
    @Environment(AppModel.self) private var model
    let inspectingVideo: Bool
    let onTestVideo: () -> Void

    var body: some View {
        let phase = model.phase
        let state = model.relay.state
        VStack(alignment: .leading, spacing: 10) {
            switch phase {
            case .starting:
                PanelWaiting(text: tr("receiver_starting"))
            case .idle, .startFailed, .receiverError:
                let failed = phase != .idle
                PanelText(text: failed ? (state.snapshot.error?.resolve() ?? "") : tr("receiver_off"),
                          color: failed ? DroneColors.danger : .white.opacity(0.85))
                PanelButton(title: tr(failed ? "retry" : "open_receiver"), systemImage: "arrow.clockwise") {
                    model.startReceiver(restart: phase == .receiverError)
                }
            case .droneConnected, .preview:
                PanelWaiting(text: tr("preview_connected"))
            default:
                if let percent = state.testVideoConverting, state.testVideoName != nil {
                    // A heavy test video being turned into what DJI Fly sends, with how far it got.
                    PanelWaiting(text: tr("test_video_converting"))
                    ProgressView(value: Double(percent), total: 100).tint(DroneColors.goLiveStart)
                } else if state.testVideoName != nil || inspectingVideo {
                    PanelWaiting(text: tr("test_video_opening"))
                } else if let lan = model.lan {
                    HStack(spacing: 8) {
                        Image("drone").resizable().renderingMode(.template).frame(width: 20, height: 20).foregroundStyle(.white)
                        Text(tr("connect_drone")).font(.subheadline.weight(.semibold)).foregroundStyle(.white)
                    }
                    PanelText(text: tr("dji_fly_instructions"))
                    AddressRow(address: lan.publishUrl) { model.copyAddress(lan.publishUrl) }
                    Text(tr("dji_fly_path")).font(.caption2).foregroundStyle(.white.opacity(0.6))
                    NotConnecting()
                    PanelButton(title: tr("try_test_video"), systemImage: "film", action: onTestVideo)
                } else {
                    HStack(spacing: 8) {
                        Image(systemName: "wifi.slash").foregroundStyle(DroneColors.warning)
                        PanelText(text: tr("no_network"))
                    }
                    PanelButton(title: tr("try_test_video"), systemImage: "film", action: onTestVideo)
                }
            }
        }
        .padding(14)
        .frame(maxWidth: .infinity, alignment: .leading)
        .glass(radius: Metrics.panelRadius)
    }
}

private struct PanelText: View {
    let text: String
    var color: Color = .white.opacity(0.85)

    var body: some View {
        Text(text).font(.footnote).foregroundStyle(color).fixedSize(horizontal: false, vertical: true)
    }
}

private struct PanelWaiting: View {
    let text: String

    var body: some View {
        HStack(spacing: 10) {
            ProgressView().tint(.white).controlSize(.small)
            PanelText(text: text)
        }
    }
}

/// A quiet orange text button, the only kind of button inside the connect card.
private struct PanelButton: View {
    let title: String
    let systemImage: String
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Label(title, systemImage: systemImage)
                .font(.subheadline.weight(.semibold))
                .foregroundStyle(DroneColors.goLiveStart)
                .frame(minHeight: 36)
                .padding(.horizontal, 4)
        }
        .buttonStyle(.plain)
    }
}

/**
 * The RTMP address typed into DJI Fly on the remote controller, on one line on any phone and left
 * to right in every language; copying only helps when DJI Fly runs on this phone.
 */
private struct AddressRow: View {
    let address: String
    let onCopy: () -> Void

    var body: some View {
        let shape = RoundedRectangle(cornerRadius: 10, style: .continuous)
        HStack(spacing: 0) {
            Text(address)
                .font(.system(size: 15, weight: .semibold, design: .monospaced))
                .foregroundStyle(.white)
                .lineLimit(1)
                .minimumScaleFactor(0.6)
                .textSelection(.enabled)
                .frame(maxWidth: .infinity, alignment: .leading)
                .environment(\.layoutDirection, .leftToRight)
            Button(action: onCopy) {
                Image(systemName: "doc.on.doc").font(.system(size: 15, weight: .medium)).foregroundStyle(DroneColors.goLiveStart)
                    .frame(width: 44, height: 44)
            }
            .accessibilityLabel(tr("copy_address"))
        }
        .padding(.leading, 12)
        .background(.white.opacity(0.08), in: shape)
        .overlay(shape.strokeBorder(.white.opacity(0.12), lineWidth: 1))
    }
}

/// The usual reasons DJI Fly cannot reach the phone, folded away until asked for.
private struct NotConnecting: View {
    @State private var open = false

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Button {
                withAnimation(.easeInOut(duration: 0.2)) { open.toggle() }
            } label: {
                HStack {
                    Text(tr("not_connecting")).font(.subheadline.weight(.semibold)).foregroundStyle(.white.opacity(0.85))
                    Spacer()
                    Image(systemName: "chevron.down")
                        .font(.caption.weight(.semibold))
                        .foregroundStyle(.white.opacity(0.7))
                        .rotationEffect(.degrees(open ? 180 : 0))
                }
                .frame(minHeight: 32)
                .padding(.horizontal, 4)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityValue(tr(open ? "state_expanded" : "state_collapsed"))
            .accessibilityHint(tr(open ? "collapse" : "expand"))
            if open {
                VStack(alignment: .leading, spacing: 6) {
                    ForEach(["not_connecting_same_network", "not_connecting_exact_address", "not_connecting_guest_vpn",
                             "ios_not_connecting_local_network"], id: \.self) { tip in
                        HStack(alignment: .top, spacing: 8) {
                            Circle().fill(.white.opacity(0.5)).frame(width: 4, height: 4).padding(.top, 6)
                            PanelText(text: tr(tip), color: .white.opacity(0.7))
                        }
                    }
                }
                .padding(.leading, 4)
                .padding(.top, 4)
                .transition(.opacity)
            }
        }
    }
}
