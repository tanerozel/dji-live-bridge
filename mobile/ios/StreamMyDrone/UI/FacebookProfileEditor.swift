import SwiftUI

struct FacebookProfileEditor: View {
    @Environment(AppModel.self) private var model
    @Environment(\.palette) private var palette
    let request: ProfileEditorRequest
    let onManual: () -> Void
    @State private var user: FacebookUser?
    @State private var pages: [FacebookPage]
    @State private var targetId: String
    /// Only a fallback for the first broadcast's title: Go live asks for the title every time.
    private let title: String
    /// Not chosen yet for a new profile connection: the user decides who can watch.
    @State private var privacy: FacebookPrivacy?
    @State private var busy = false
    @State private var error: UiText?
    @State private var disconnect = false

    init(request: ProfileEditorRequest, onManual: @escaping () -> Void) {
        self.request = request
        self.onManual = onManual
        let old = request.profile?.facebook
        _user = State(initialValue: old.map { FacebookUser(id: $0.userId, name: $0.userName) })
        _pages = State(initialValue: old.flatMap { $0.page ? [FacebookPage(id: $0.targetId, name: $0.targetName)] : nil } ?? [])
        _targetId = State(initialValue: old?.targetId ?? "")
        title = old?.title ?? "Stream My Drone"
        _privacy = State(initialValue: old.flatMap { $0.page ? nil : FacebookPrivacy(rawValue: $0.privacy) })
    }

    var body: some View {
        let onPage = user != nil && targetId != user?.id
        VStack(spacing: 0) {
            HStack {
                Button { model.editor = nil } label: {
                    Image(systemName: "xmark").font(.body.weight(.semibold)).foregroundStyle(palette.muted).frame(width: 44, height: 44)
                }.accessibilityLabel(tr("close")).disabled(busy)
                Spacer()
            }.padding(.horizontal, 4)
            // Talking to Facebook can take a moment; the bar says the tap was taken.
            ProgressView().progressViewStyle(.linear).tint(palette.accent).opacity(busy ? 1 : 0)
            ScrollView {
                VStack(spacing: 16) {
                    PlatformTile(kind: .facebook, size: 64)
                    VStack(spacing: 6) {
                        Text("Facebook").font(.title2.weight(.semibold)).accessibilityAddTraits(.isHeader)
                        Text(tr("facebook_connection_help")).font(.subheadline).foregroundStyle(palette.muted).multilineTextAlignment(.center)
                    }
                    if let user {
                        accountCard(user)
                        section(tr("facebook_target")) {
                            OptionCard(label: user.name, hint: tr("facebook_target_profile"), selected: targetId == user.id) { targetId = user.id }
                            ForEach(pages) { page in
                                OptionCard(label: page.name, hint: tr("facebook_target_page"), selected: targetId == page.id) { targetId = page.id }
                            }
                        }
                        section(tr("facebook_privacy")) {
                            if onPage {
                                Text(tr("facebook_page_public_hint")).font(.subheadline).foregroundStyle(palette.muted)
                            } else {
                                ForEach(FacebookPrivacy.allCases, id: \.self) { value in
                                    OptionCard(label: tr(value.labelKey), hint: tr("\(value.labelKey)_hint"), selected: privacy == value) { privacy = value }
                                }
                            }
                        }
                    } else {
                        HowItWorksCard(steps: [tr("facebook_step_account"), tr("facebook_step_prepare"), tr("facebook_auto_hint")])
                    }
                    if let error { AlertBanner(title: tr("save_failed"), message: error.resolve()) }
                    if request.profile == nil {
                        Button(tr("youtube_manual"), action: onManual).font(.subheadline.weight(.semibold)).foregroundStyle(palette.link).disabled(busy)
                    } else {
                        Button(tr("facebook_disconnect"), role: .destructive) { disconnect = true }
                            .font(.subheadline.weight(.semibold)).foregroundStyle(palette.dangerText).disabled(busy)
                    }
                    Link(tr("privacy_policy"), destination: URL(string: "https://tanerozel.github.io/dji-live-bridge/privacy-policy.html#facebook")!)
                        .font(.caption)
                }
                .padding(.horizontal, 20).frame(maxWidth: 560).frame(maxWidth: .infinity)
                .disabled(busy)
            }.scrollDismissesKeyboard(.interactively)
            Group {
                if user != nil {
                    PrimaryButton(title: tr("save"), enabled: !busy && (onPage || privacy != nil)) { save() }
                } else {
                    PrimaryButton(title: tr("facebook_connect"), enabled: !busy && !model.liveFacebook) { connect() }
                }
            }
            .frame(maxWidth: 560).padding(.horizontal, 20).padding(.vertical, 12)
        }
        .foregroundStyle(palette.text).background(palette.background.ignoresSafeArea())
        .interactiveDismissDisabled(busy)
        .task { await refreshPages() }
        .alert(tr("facebook_disconnect"), isPresented: $disconnect) {
            Button(tr("facebook_disconnect"), role: .destructive) {
                guard let profile = request.profile else { return }
                busy = true
                Task { error = await model.disconnectFacebook(profile); busy = false }
            }
            Button(tr("cancel"), role: .cancel) {}
        } message: { Text(tr("facebook_disconnect_message")) }
    }

    /// The Facebook account in use, with a way to pick another.
    private func accountCard(_ user: FacebookUser) -> some View {
        BridgeCard(padding: EdgeInsets(top: 12, leading: 16, bottom: 12, trailing: 8)) {
            HStack(spacing: 12) {
                Text(user.name.first.map { String($0).uppercased() } ?? "")
                    .font(.headline)
                    .foregroundStyle(palette.onAccentSoft)
                    .frame(width: 40, height: 40)
                    .background(palette.accentSoft, in: Circle())
                Text(user.name).font(.subheadline.weight(.semibold)).lineLimit(1)
                    .frame(maxWidth: .infinity, alignment: .leading)
                Button(tr("change")) {
                    // Another account: forget this one first, or Facebook offers it again.
                    FacebookAuthorization.shared.forget()
                    connect()
                }.font(.subheadline.weight(.semibold)).disabled(busy || model.liveFacebook)
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

    /// Lists the Pages again when the same Facebook user is still logged in.
    private func refreshPages() async {
        guard let user, let login = FacebookAuthorization.shared.current, login.userId == user.id, login.usable(),
              FacebookPermissions.allow(login.scopes, page: true) else { return }
        if let available = try? await FacebookAPI(transport: GraphFacebookTransport()).pages(token: login.token) {
            pages = available
        }
    }

    private func connect() {
        guard !model.liveFacebook else { error = .tr("facebook_error_edit_live"); return }
        busy = true
        error = nil
        Task {
            do {
                guard let connected = try await FacebookAuthorization.shared.connect() else {
                    // Facebook's window closed without handing back a login.
                    error = .tr("facebook_login_incomplete")
                    busy = false
                    return
                }
                let login = connected.login
                let available = FacebookPermissions.allow(login.scopes, page: true)
                    ? try await FacebookAPI(transport: GraphFacebookTransport()).pages(token: login.token) : []
                user = connected.user
                pages = available
                if targetId != connected.user.id && !available.contains(where: { $0.id == targetId }) { targetId = connected.user.id }
            } catch { self.error = facebookErrorText(error) }
            busy = false
        }
    }

    private func save() {
        guard let user else { return }
        let page = pages.first { $0.id == targetId }
        guard page != nil || targetId == user.id else { return }
        guard let login = FacebookAuthorization.shared.current, login.userId == user.id, login.usable(),
              FacebookPermissions.allow(login.scopes, page: page != nil) else {
            error = .tr("facebook_error_auth")
            return
        }
        let privacy = page != nil ? FacebookPrivacy.everyone : self.privacy
        guard let privacy else { return }
        let connection = FacebookConnection(userId: user.id, userName: user.name, targetId: page?.id ?? user.id,
                                            targetName: page?.name ?? user.name, page: page != nil,
                                            title: title.trimmingCharacters(in: .whitespacesAndNewlines), privacy: privacy.rawValue)
        busy = true
        error = nil
        Task {
            error = await model.saveFacebook(request, connection: connection)
            busy = false
        }
    }
}
