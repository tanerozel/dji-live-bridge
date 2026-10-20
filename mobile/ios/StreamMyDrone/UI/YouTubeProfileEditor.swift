import GoogleSignInSwift
import SwiftUI

/// The screen for one platform: Instagram, YouTube and Facebook connect themselves, the others take a key.
struct DestinationEditorView: View {
    @Environment(AppModel.self) private var model
    let request: ProfileEditorRequest
    /// Chosen on this screen; until then a key pasted earlier in this session keeps the key screen.
    @State private var manual: Bool?

    var body: some View {
        let manual = manual ?? request.profile.map { model.instagramManualProfiles.contains($0.id) } ?? false
        switch request.kind {
        case .instagram:
            if manual {
                ProfileEditorView(request: request, onAutomatic: { self.manual = false })
            } else {
                InstagramConnectView(request: request, onManual: { self.manual = true })
            }
        case .youtube where !manual && (request.profile == nil || request.profile?.youtube != nil):
            YouTubeProfileEditor(request: request, onManual: { self.manual = true })
        case .facebook where !manual && (request.profile == nil || request.profile?.facebook != nil):
            FacebookProfileEditor(request: request, onManual: { self.manual = true })
        default:
            ProfileEditorView(request: request)
        }
    }
}

struct YouTubeProfileEditor: View {
    @Environment(AppModel.self) private var model
    @Environment(\.palette) private var palette
    let request: ProfileEditorRequest
    let onManual: () -> Void
    @State private var email: String
    @State private var channels: [YouTubeChannel]
    @State private var channelId: String
    /// Only a fallback for the first broadcast's title: Go live asks for the title every time.
    private let title: String
    @State private var privacy: String
    @State private var kids: Bool?
    @State private var busy = false
    @State private var error: UiText?
    @State private var disconnect = false

    init(request: ProfileEditorRequest, onManual: @escaping () -> Void) {
        self.request = request
        self.onManual = onManual
        let old = request.profile?.youtube
        _email = State(initialValue: old?.accountEmail ?? "")
        _channels = State(initialValue: old.map { [YouTubeChannel(id: $0.channelId, title: $0.channelTitle)] } ?? [])
        _channelId = State(initialValue: old?.channelId ?? "")
        title = old?.title ?? "Stream My Drone"
        _privacy = State(initialValue: old?.privacy ?? "unlisted")
        _kids = State(initialValue: old?.madeForKids)
    }

    var body: some View {
        let connected = !email.isEmpty && !channels.isEmpty
        VStack(spacing: 0) {
            HStack {
                Button { model.editor = nil } label: {
                    Image(systemName: "xmark").font(.body.weight(.semibold)).foregroundStyle(palette.muted).frame(width: 44, height: 44)
                }.accessibilityLabel(tr("close")).disabled(busy)
                Spacer()
            }.padding(.horizontal, 4)
            // Talking to Google can take a moment; the bar says the tap was taken.
            ProgressView().progressViewStyle(.linear).tint(palette.accent).opacity(busy ? 1 : 0)
            ScrollView {
                VStack(spacing: 16) {
                    PlatformTile(kind: .youtube, size: 64)
                    VStack(spacing: 6) {
                        Text("YouTube").font(.title2.weight(.semibold)).accessibilityAddTraits(.isHeader)
                        Text(tr("youtube_connection_help")).font(.subheadline).foregroundStyle(palette.muted).multilineTextAlignment(.center)
                    }
                    if !connected {
                        HowItWorksCard(steps: [tr("youtube_step_account"), tr("youtube_step_prepare"), tr("youtube_auto_hint")])
                    } else {
                        accountCard
                        if channels.count > 1 {
                            section(tr("youtube_channel")) {
                                ForEach(channels) { channel in
                                    OptionCard(label: channel.title, selected: channelId == channel.id) { channelId = channel.id }
                                }
                            }
                        }
                        section(tr("youtube_privacy")) {
                            ForEach(["private", "unlisted", "public"], id: \.self) { value in
                                OptionCard(label: tr("youtube_\(value)"), hint: tr("youtube_\(value)_hint"), selected: privacy == value) { privacy = value }
                            }
                        }
                        section(tr("youtube_audience")) {
                            OptionCard(label: tr("youtube_not_kids"), selected: kids == false) { kids = false }
                            OptionCard(label: tr("youtube_kids"), selected: kids == true) { kids = true }
                        }
                    }
                    if let error { AlertBanner(title: tr("save_failed"), message: error.resolve()) }
                    if request.profile == nil {
                        Button(tr("youtube_manual"), action: onManual).font(.subheadline.weight(.semibold)).foregroundStyle(palette.link).disabled(busy)
                    } else {
                        Button(tr("youtube_disconnect"), role: .destructive) { disconnect = true }
                            .font(.subheadline.weight(.semibold)).foregroundStyle(palette.dangerText).disabled(busy)
                    }
                    Link(tr("privacy_policy"), destination: URL(string: "https://tanerozel.github.io/dji-live-bridge/privacy-policy.html#google-youtube")!)
                        .font(.caption)
                }
                .padding(.horizontal, 20).frame(maxWidth: 560).frame(maxWidth: .infinity)
                .disabled(busy)
            }.scrollDismissesKeyboard(.interactively)
            Group {
                if connected {
                    PrimaryButton(title: tr("save"), enabled: !busy && !channelId.isEmpty && kids != nil) { save() }
                } else {
                    // Google's own button, as its sign-in guidelines ask.
                    GoogleSignInButton(scheme: palette.isDark ? .dark : .light, style: .wide) { connect() }
                        .disabled(busy || model.liveYouTube)
                }
            }
            .frame(maxWidth: 560).padding(.horizontal, 20).padding(.vertical, 12)
        }
        .foregroundStyle(palette.text).background(palette.background.ignoresSafeArea())
        .interactiveDismissDisabled(busy)
        .alert(tr("youtube_disconnect"), isPresented: $disconnect) {
            Button(tr("youtube_disconnect"), role: .destructive) {
                guard let profile = request.profile else { return }
                busy = true
                Task { error = await model.disconnectYouTube(profile); busy = false }
            }
            Button(tr("cancel"), role: .cancel) {}
        } message: { Text(tr("youtube_disconnect_message")) }
    }

    /// The Google account in use, with a way to pick another.
    private var accountCard: some View {
        BridgeCard(padding: EdgeInsets(top: 12, leading: 16, bottom: 12, trailing: 8)) {
            HStack(spacing: 12) {
                Text(email.first.map { String($0).uppercased() } ?? "")
                    .font(.headline)
                    .foregroundStyle(palette.onAccentSoft)
                    .frame(width: 40, height: 40)
                    .background(palette.accentSoft, in: Circle())
                VStack(alignment: .leading, spacing: 2) {
                    if channels.count == 1, let channel = channels.first {
                        Text(channel.title).font(.subheadline.weight(.semibold)).lineLimit(1)
                    }
                    Text(email).font(.footnote).foregroundStyle(palette.muted).lineLimit(1).truncationMode(.middle)
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                Button(tr("change")) { connect() }.font(.subheadline.weight(.semibold)).disabled(busy || model.liveYouTube)
            }
        }
    }

    /// A heading over one group of settings.
    private func section<Content: View>(_ title: String, @ViewBuilder content: () -> Content) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(title).font(.subheadline.weight(.semibold)).foregroundStyle(palette.muted).accessibilityAddTraits(.isHeader)
            content()
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private func connect() {
        guard !model.liveYouTube else { error = .tr("youtube_error_edit_live"); return }
        busy = true
        error = nil
        Task {
            do {
                let connected = try await YouTubeAuthorization.shared.connect(hint: email.isEmpty ? nil : email)
                let available = try await connected.api.channels()
                email = connected.email
                channels = available
                if !available.contains(where: { $0.id == channelId }) { channelId = available[0].id }
            } catch { self.error = youTubeErrorText(error) }
            busy = false
        }
    }

    private func save() {
        guard let channel = channels.first(where: { $0.id == channelId }), let kids else { return }
        busy = true
        error = nil
        Task {
            error = await model.saveYouTube(request, email: email, channel: channel, title: title, privacy: privacy, madeForKids: kids)
            busy = false
        }
    }
}
