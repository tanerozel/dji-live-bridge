import SwiftUI

/**
 * The stream key screen for one platform. iOS cannot keep a screen out of screenshots the way
 * Android's FLAG_SECURE does, so the key stays masked unless the user shows it, and the whole
 * screen is covered while the screen is recorded or mirrored and in the app switcher.
 */
struct ProfileEditorView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.palette) private var palette
    @Environment(\.scenePhase) private var scenePhase
    let request: ProfileEditorRequest

    @State private var serverUrl: String
    @State private var streamKey = ""
    @State private var keyVisible = false
    /// The address stays folded away while it is the platform's published default.
    @State private var serverExpanded: Bool
    @State private var serverTouched = false
    @State private var keyTouched = false
    @State private var editorError: UiText?
    @State private var showDiscard = false
    @State private var showDelete = false
    @State private var captured = UIScreen.main.isCaptured
    @FocusState private var focus: Field?

    private enum Field { case server, key }

    init(request: ProfileEditorRequest) {
        self.request = request
        let initial = request.profile?.serverUrl ?? request.kind.defaultServerUrl ?? ""
        _serverUrl = State(initialValue: initial)
        _serverExpanded = State(initialValue: request.kind.defaultServerUrl.map { !sameServerUrl(initial, $0) } ?? true)
    }

    private var initialServerUrl: String { request.profile?.serverUrl ?? request.kind.defaultServerUrl ?? "" }

    private var serverError: String? {
        do {
            _ = try validateServerUrl(serverUrl)
            return nil
        } catch {
            return (error as? DestinationProfileError)?.text.resolve()
        }
    }

    private var keyError: String? {
        if streamKey.isEmpty { return request.profile == nil ? tr("key_paste_prompt") : nil }
        do {
            try validateStreamKey(streamKey)
            return nil
        } catch {
            return (error as? DestinationProfileError)?.text.resolve()
        }
    }

    private var hasChanges: Bool { !streamKey.isEmpty || !sameServerUrl(serverUrl, initialServerUrl) }

    var body: some View {
        let kind = request.kind
        VStack(spacing: 0) {
            HStack {
                Button(action: requestDismiss) {
                    Image(systemName: "xmark").font(.body.weight(.semibold)).foregroundStyle(palette.muted).frame(width: 44, height: 44)
                }
                .accessibilityLabel(tr("close"))
                Spacer()
            }
            .padding(.horizontal, 4)
            ScrollView {
                VStack(spacing: 16) {
                    PlatformTile(kind: kind, size: 64)
                    VStack(spacing: 6) {
                        Text(kind.displayName).font(.title2.weight(.semibold)).accessibilityAddTraits(.isHeader)
                        Text(tr(kind.keyHelpKey)).font(.subheadline).foregroundStyle(palette.muted).multilineTextAlignment(.center)
                    }
                    if serverExpanded {
                        FieldBox(label: tr("server_address"), error: serverTouched ? serverError : nil) {
                            TextField(kind.serverPlaceholder, text: $serverUrl)
                                .keyboardType(.URL)
                                .textInputAutocapitalization(.never)
                                .autocorrectionDisabled()
                                .focused($focus, equals: .server)
                                .submitLabel(.next)
                                .onSubmit { focus = .key }
                                .onChange(of: serverUrl) { serverTouched = true; editorError = nil }
                        }
                    }
                    FieldBox(
                        label: tr(request.profile == nil ? "stream_key" : "stream_key_new"),
                        error: keyTouched ? keyError : nil,
                        supporting: tr(request.profile == nil ? "stream_key_stored_encrypted" : "stream_key_kept")
                    ) {
                        HStack {
                            Group {
                                let placeholder = tr(request.profile == nil ? "stream_key_paste_here" : "stream_key_keep_hint")
                                if keyVisible {
                                    TextField(placeholder, text: $streamKey)
                                } else {
                                    SecureField(placeholder, text: $streamKey)
                                }
                            }
                            .textInputAutocapitalization(.never)
                            .autocorrectionDisabled()
                            .textContentType(.password)
                            .focused($focus, equals: .key)
                            .submitLabel(.done)
                            .onSubmit(save)
                            .onChange(of: streamKey) { keyTouched = true; editorError = nil }
                            Button { keyVisible.toggle() } label: {
                                Image(systemName: keyVisible ? "eye.slash" : "eye").foregroundStyle(palette.muted)
                            }
                            .accessibilityLabel(tr(keyVisible ? "key_hide" : "key_show"))
                        }
                    }
                    if !serverExpanded {
                        HStack {
                            VStack(alignment: .leading, spacing: 2) {
                                Text(tr("server_address")).font(.caption).foregroundStyle(palette.muted)
                                Text(serverUrl).font(.caption.monospaced()).foregroundStyle(palette.muted)
                                    .environment(\.layoutDirection, .leftToRight)
                            }
                            .frame(maxWidth: .infinity, alignment: .leading)
                            Button(tr("change")) { serverExpanded = true; focus = .server }
                        }
                    }
                    if let editorError { AlertBanner(title: tr("save_failed"), message: editorError.resolve()) }
                    if request.profile != nil {
                        Button(role: .destructive) { showDelete = true } label: {
                            Label(tr("delete_platform"), systemImage: "trash").foregroundStyle(palette.dangerText)
                        }
                    }
                }
                .frame(maxWidth: 560)
                .padding(.horizontal, 20)
                .frame(maxWidth: .infinity)
            }
            .scrollDismissesKeyboard(.interactively)
            PrimaryButton(title: tr("save"), enabled: request.profile == nil || hasChanges, action: save)
                .frame(maxWidth: 560)
                .padding(.horizontal, 20)
                .padding(.vertical, 12)
        }
        .foregroundStyle(palette.text)
        .background(palette.background.ignoresSafeArea())
        .overlay { if captured || scenePhase != .active { privacyCover } }
        .onReceive(NotificationCenter.default.publisher(for: UIScreen.capturedDidChangeNotification)) { _ in
            captured = UIScreen.main.isCaptured
        }
        .interactiveDismissDisabled(hasChanges)
        .onAppear {
            // Most visits are only for a key, so the keyboard opens straight on it. Platforms
            // without a published address start on the address instead.
            focus = kind.defaultServerUrl == nil && serverUrl.isEmpty ? .server : .key
        }
        .alert(tr("discard_title"), isPresented: $showDiscard) {
            Button(tr("delete"), role: .destructive) { clearAndClose() }
            Button(tr("go_back"), role: .cancel) {}
        } message: {
            Text(tr("discard_message"))
        }
        .alert(tr("delete_platform_title", kind.displayName), isPresented: $showDelete) {
            Button(tr("delete"), role: .destructive) {
                if let profile = request.profile { editorError = model.deleteProfile(profile) }
            }
            Button(tr("cancel"), role: .cancel) {}
        } message: {
            Text(tr("delete_platform_message"))
        }
    }

    private var privacyCover: some View {
        palette.background.ignoresSafeArea().overlay(PlatformTile(kind: request.kind, size: 64))
    }

    private func requestDismiss() {
        focus = nil
        if hasChanges { showDiscard = true } else { clearAndClose() }
    }

    private func clearAndClose() {
        streamKey = ""
        keyVisible = false
        model.editor = nil
    }

    private func save() {
        focus = nil
        serverTouched = true
        keyTouched = true
        if serverError != nil { serverExpanded = true }
        guard serverError == nil, keyError == nil else { return }
        editorError = model.saveProfile(request, serverUrl: serverUrl, streamKey: streamKey)
        if editorError == nil {
            streamKey = ""
            keyVisible = false
        }
    }
}

/// A labeled text field in the theme's colors, with its error or a note under it.
private struct FieldBox<Field: View>: View {
    let label: String
    var error: String?
    var supporting: String?
    @ViewBuilder let field: Field
    @Environment(\.palette) private var palette

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(label).font(.caption.weight(.semibold)).foregroundStyle(error == nil ? palette.muted : palette.dangerText)
            field
                .environment(\.layoutDirection, .leftToRight)
                .padding(.horizontal, 12)
                .frame(minHeight: 50)
                .background(palette.field, in: RoundedRectangle(cornerRadius: 12, style: .continuous))
                .overlay(RoundedRectangle(cornerRadius: 12, style: .continuous)
                    .strokeBorder(error == nil ? palette.border : palette.dangerText, lineWidth: 1))
            if let note = error ?? supporting {
                Text(note).font(.caption).foregroundStyle(error == nil ? palette.muted : palette.dangerText)
            }
        }
    }
}
