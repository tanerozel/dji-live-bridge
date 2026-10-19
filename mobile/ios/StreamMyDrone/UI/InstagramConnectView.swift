import SwiftUI

/**
 * Instagram connected by the app itself: no key to paste. Connect opens Instagram's page behind
 * the app; it shows only when Instagram needs the user to sign in, verify or fix something.
 */
struct InstagramConnectView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.palette) private var palette
    let request: ProfileEditorRequest
    let onManual: () -> Void
    @State private var error: UiText?
    @State private var showDelete = false

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Button { model.editor = nil } label: {
                    Image(systemName: "xmark").font(.body.weight(.semibold)).foregroundStyle(palette.muted).frame(width: 44, height: 44)
                }
                .accessibilityLabel(tr("close"))
                Spacer()
            }
            .padding(.horizontal, 4)
            ScrollView {
                VStack(spacing: 16) {
                    PlatformTile(kind: .instagram, size: 64)
                    VStack(spacing: 6) {
                        Text("Instagram").font(.title2.weight(.semibold)).accessibilityAddTraits(.isHeader)
                        Text(tr("instagram_automatic_setup")).font(.subheadline).foregroundStyle(palette.muted).multilineTextAlignment(.center)
                    }
                    HowItWorksCard(steps: [tr("instagram_step_signin"), tr("instagram_step_prepare"), tr("instagram_modal_help")])
                    if let error { AlertBanner(title: error.resolve()) }
                    Button(tr("instagram_manual"), action: onManual).font(.subheadline.weight(.semibold)).foregroundStyle(palette.link)
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
            PrimaryButton(title: tr("instagram_connect")) { error = model.connectInstagram(request) }
                .frame(maxWidth: 560)
                .padding(.horizontal, 20)
                .padding(.vertical, 12)
        }
        .foregroundStyle(palette.text)
        .background(palette.background.ignoresSafeArea())
        .alert(tr("delete_platform_title", "Instagram"), isPresented: $showDelete) {
            Button(tr("delete"), role: .destructive) {
                if let profile = request.profile { error = model.deleteProfile(profile) }
            }
            Button(tr("cancel"), role: .cancel) {}
        } message: {
            Text(tr("delete_platform_message"))
        }
    }
}
