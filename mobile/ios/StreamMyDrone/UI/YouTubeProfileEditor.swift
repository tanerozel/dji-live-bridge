import GoogleSignInSwift
import SwiftUI

struct DestinationEditorView: View {
    let request: ProfileEditorRequest
    @State private var manual = false
    var body: some View {
        if request.kind != .youtube || manual || (request.profile != nil && request.profile?.youtube == nil) {
            ProfileEditorView(request: request)
        } else {
            YouTubeProfileEditor(request: request, onManual: { manual = true })
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
    @State private var title: String
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
        _title = State(initialValue: old?.title ?? "Stream My Drone")
        _privacy = State(initialValue: old?.privacy ?? "unlisted")
        _kids = State(initialValue: old?.madeForKids)
    }

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Button { model.editor = nil } label: {
                    Image(systemName: "xmark").font(.body.weight(.semibold)).frame(width: 44, height: 44)
                }.accessibilityLabel(tr("close")).disabled(busy)
                Spacer()
            }.padding(.horizontal, 4)
            ScrollView {
                VStack(spacing: 16) {
                    PlatformTile(kind: .youtube, size: 64)
                    Text("YouTube").font(.title2.weight(.semibold)).accessibilityAddTraits(.isHeader)
                    Text(tr("youtube_connection_help")).font(.subheadline).foregroundStyle(palette.muted).multilineTextAlignment(.center)
                    GoogleSignInButton { connect() }.disabled(busy || model.liveYouTube)
                    if !email.isEmpty {
                        Text(email).font(.subheadline).foregroundStyle(palette.muted)
                        Picker(tr("youtube_channel"), selection: $channelId) {
                            ForEach(channels) { Text($0.title).tag($0.id) }
                        }
                        FieldBox(label: tr("youtube_title")) {
                            TextField(tr("youtube_title"), text: $title).submitLabel(.done)
                        }
                        Picker(tr("youtube_privacy"), selection: $privacy) {
                            Text(tr("youtube_private")).tag("private")
                            Text(tr("youtube_unlisted")).tag("unlisted")
                            Text(tr("youtube_public")).tag("public")
                        }
                        VStack(alignment: .leading, spacing: 10) {
                            Text(tr("youtube_audience")).font(.caption.weight(.semibold)).foregroundStyle(palette.muted)
                            audience(false, "youtube_not_kids")
                            audience(true, "youtube_kids")
                        }.frame(maxWidth: .infinity, alignment: .leading)
                        Text(tr("youtube_auto_hint")).font(.caption).foregroundStyle(palette.muted)
                    }
                    if busy { ProgressView() }
                    if let error { AlertBanner(title: tr("save_failed"), message: error.resolve()) }
                    if request.profile == nil { Button(tr("youtube_manual"), action: onManual).disabled(busy) }
                    else {
                        Button(tr("youtube_disconnect"), role: .destructive) { disconnect = true }.disabled(busy)
                    }
                    Link(tr("privacy_policy"), destination: URL(string: "https://tanerozel.github.io/dji-live-bridge/privacy-policy.html#google-youtube")!)
                        .font(.caption)
                }
                .padding(.horizontal, 20).frame(maxWidth: 560).frame(maxWidth: .infinity)
                .disabled(busy)
            }.scrollDismissesKeyboard(.interactively)
            PrimaryButton(title: tr("save"), enabled: !busy && !email.isEmpty && !channelId.isEmpty && kids != nil) { save() }
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

    private func audience(_ value: Bool, _ key: String) -> some View {
        Button { kids = value } label: {
            Label(tr(key), systemImage: kids == value ? "largecircle.fill.circle" : "circle")
                .multilineTextAlignment(.leading)
        }.accessibilityAddTraits(kids == value ? [.isSelected] : [])
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
