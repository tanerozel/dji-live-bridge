import SwiftUI
import UIKit

/**
 * Where the retained Instagram page lives. WebKit keeps a page running only while it is in a
 * window, and a full-screen key screen takes the drone screen out of its window, so the hidden
 * page has a window of its own behind the app's (the app's screens are opaque). Instagram's
 * window and the "Preparing Instagram" card go in a window above everything, so they show over
 * the key screen and the sheets as well.
 */
@MainActor
final class InstagramPresenter {
    private var background: UIWindow?
    private var overlay: UIWindow?
    private let backgroundContainer = UIView()

    func update(_ model: AppModel) {
        let session = model.instagram
        guard let scene = UIApplication.shared.connectedScenes.compactMap({ $0 as? UIWindowScene })
            .first(where: { $0.activationState != .unattached }) else { return }
        if session.hasPage || session.preparing {
            if background == nil {
                let window = UIWindow(windowScene: scene)
                window.windowLevel = UIWindow.Level(rawValue: UIWindow.Level.normal.rawValue - 1)
                let controller = UIViewController()
                backgroundContainer.frame = window.bounds
                backgroundContainer.autoresizingMask = [.flexibleWidth, .flexibleHeight]
                controller.view = backgroundContainer
                window.rootViewController = controller
                window.isUserInteractionEnabled = false
                window.accessibilityElementsHidden = true
                window.isHidden = false
                background = window
            }
            if !session.visible { session.place(in: backgroundContainer, interactive: false) }
        }
        let showOverlay = session.visible || session.preparing
        if showOverlay {
            if overlay == nil {
                let window = UIWindow(windowScene: scene)
                window.windowLevel = UIWindow.Level(rawValue: UIWindow.Level.normal.rawValue + 1)
                let controller = UIHostingController(rootView: InstagramOverlay(model: model))
                controller.view.backgroundColor = .clear
                window.rootViewController = controller
                window.backgroundColor = .clear
                overlay = window
            }
            overlay?.isHidden = false
            // Signing in needs the keyboard, which goes to the key window.
            if session.visible { overlay?.makeKey() }
        } else if let overlay, !overlay.isHidden {
            overlay.isHidden = true
            scene.windows.first { $0 !== overlay && $0 !== background }?.makeKey()
        }
    }
}

/// What the top window shows: Instagram's page, or the card while it is prepared.
private struct InstagramOverlay: View {
    let model: AppModel

    var body: some View {
        let session = model.instagram
        let localizer = Localizer.shared
        ZStack {
            if session.visible {
                InstagramWindowView(onDismiss: model.closeInstagramWindow).themed(model.theme)
                    .transition(.move(edge: .bottom))
            } else if session.preparing {
                InstagramPreparingCard(onCancel: model.cancelInstagramSetup)
                    .transition(.opacity)
            }
        }
        .animation(.easeInOut(duration: 0.25), value: session.visible)
        .environment(model)
        .id(localizer.effectiveLanguage.tag)
        .environment(\.locale, localizer.locale)
        .environment(\.layoutDirection, localizer.isRightToLeft ? .rightToLeft : .leftToRight)
    }
}

/// Instagram's own page, for signing in, a setup problem or a look at the running broadcast.
private struct InstagramWindowView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.palette) private var palette
    let onDismiss: () -> Void
    @State private var controlMissing = false

    var body: some View {
        let session = model.instagram
        let live = model.liveDestinations.contains { $0.kind == .instagram }
        VStack(spacing: 0) {
            HStack(spacing: 10) {
                Button(action: onDismiss) {
                    Image(systemName: "xmark").font(.body.weight(.semibold)).foregroundStyle(palette.muted).frame(width: 44, height: 44)
                }
                .accessibilityLabel(tr("close"))
                PlatformTile(kind: .instagram, size: 28)
                VStack(alignment: .leading, spacing: 1) {
                    Text("Instagram").font(.subheadline.weight(.semibold)).lineLimit(1)
                    // The page's address, so it is clear this is Instagram's own site.
                    HStack(spacing: 4) {
                        Image(systemName: "lock.fill").font(.system(size: 9))
                        Text(session.host).font(.caption2).lineLimit(1).truncationMode(.middle)
                            .environment(\.layoutDirection, .leftToRight)
                    }
                    .foregroundStyle(palette.muted)
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                Menu {
                    if live {
                        Button(tr("instagram_show_live_control")) { session.showLiveControl { controlMissing = !$0 } }
                    } else {
                        Button(tr("instagram_clear_session"), role: .destructive) { session.clearWebSession() }
                    }
                } label: {
                    Image(systemName: "ellipsis.circle").font(.body).foregroundStyle(palette.muted).frame(width: 44, height: 44)
                }
                .accessibilityLabel(tr("more"))
            }
            .padding(.horizontal, 4)
            VStack(spacing: 8) {
                TipBox(text: tr(session.captureTarget != nil ? "instagram_modal_steps" : session.publishStatus ?? "instagram_producer_live"))
                if session.titleTooLong { AlertBanner(title: tr("broadcast_title_instagram_error")) }
                if session.producerUnavailable {
                    ProducerNotice(text: tr("instagram_setup_unavailable"), danger: false, onRetry: session.retryOpenProducer)
                }
                if live && controlMissing { ProducerNotice(text: tr("instagram_control_missing"), danger: false, onRetry: nil) }
                if session.pageError { ProducerNotice(text: tr("instagram_error_page"), danger: true, onRetry: session.reload) }
            }
            .padding(.horizontal, 12)
            .padding(.bottom, 8)
            if session.loading { ProgressView().progressViewStyle(.linear).tint(palette.accent) }
            InstagramWebHost(session: session)
        }
        .foregroundStyle(palette.text)
        .background(palette.background.ignoresSafeArea())
        .task(id: live && !session.loading) {
            guard live && !session.loading else { return }
            try? await Task.sleep(for: .seconds(2))
            if !Task.isCancelled { session.showLiveControl { controlMissing = !$0 } }
        }
    }
}

/// A problem in the Instagram window, with a way to try again where there is one.
private struct ProducerNotice: View {
    let text: String
    let danger: Bool
    let onRetry: (() -> Void)?
    @Environment(\.palette) private var palette

    var body: some View {
        let color = danger ? palette.dangerText : palette.warningText
        HStack(spacing: 10) {
            Image(systemName: "exclamationmark.triangle").foregroundStyle(color)
            Text(text).font(.footnote).foregroundStyle(palette.text).frame(maxWidth: .infinity, alignment: .leading)
            if let onRetry {
                Button(tr("retry"), action: onRetry).font(.footnote.weight(.semibold)).foregroundStyle(color)
            }
        }
        .padding(12)
        .background(danger ? palette.dangerSoft : palette.warningText.opacity(0.12), in: RoundedRectangle(cornerRadius: 12, style: .continuous))
        .accessibilityElement(children: .combine)
    }
}

/// Hosts the retained page while the window shows; it goes back behind the app afterwards.
private struct InstagramWebHost: UIViewRepresentable {
    let session: InstagramProducerSession

    func makeUIView(context: Context) -> UIView {
        let container = UIView()
        container.clipsToBounds = true
        session.place(in: container, interactive: true)
        return container
    }

    func updateUIView(_ container: UIView, context: Context) {
        session.place(in: container, interactive: true)
    }
}
