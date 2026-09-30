import SwiftUI

@main
struct StreamMyDroneApp: App {
    var body: some Scene {
        WindowGroup {
            RootView()
        }
    }
}

/// The guide on first launch, then the drone screen; the key screen and questions over it.
struct RootView: View {
    @State private var model = AppModel()
    @Environment(\.scenePhase) private var scenePhase
    @Environment(\.colorScheme) private var systemScheme

    var body: some View {
        let localizer = Localizer.shared
        @Bindable var model = model
        let themedScreen = model.showGuide || model.editor != nil
        let palette = BridgePalette.forChoice(model.theme, systemDark: systemScheme == .dark)
        ZStack {
            if model.showGuide {
                GuideScreen(onFinish: model.finishGuide).themed(model.theme)
            } else {
                DroneScreen()
            }
        }
        .animation(.easeInOut(duration: 0.2), value: model.showGuide)
        .fullScreenCover(item: $model.editor) { request in
            ProfileEditorView(request: request).themed(model.theme).environment(model)
        }
        .alert(endLiveTitle, isPresented: endLiveShown, presenting: model.endLive) { request in
            Button(tr("end_broadcast"), role: .destructive) { model.endLiveConfirmed(request) }
            Button(tr("keep_streaming"), role: .cancel) {}
        } message: { _ in
            Text(endLiveMessage)
        }
        .environment(model)
        // Strings are looked up as the views are built; a new language rebuilds them all.
        .id(localizer.effectiveLanguage.tag)
        .environment(\.locale, localizer.locale)
        .environment(\.layoutDirection, localizer.isRightToLeft ? .rightToLeft : .leftToRight)
        // The chosen theme decides the status bar on the guide and key screens; the drone's
        // picture always needs light icons.
        .preferredColorScheme(themedScreen ? (model.theme == .system ? nil : palette.isDark ? .dark : .light) : .dark)
        .onChange(of: scenePhase, initial: true) {
            switch scenePhase {
            case .active: model.becameActive()
            case .background: model.enteredBackground()
            default: break
            }
        }
        .onChange(of: model.phase, initial: true) { model.phaseChanged() }
        .onChange(of: model.relay.state.isLive) { model.phaseChanged() }
        .onChange(of: model.picture.pictureInPicture) { model.pictureInPictureChanged() }
    }

    private var endLiveShown: Binding<Bool> {
        Binding(get: { model.endLive != nil }, set: { if !$0 { model.endLive = nil } })
    }

    private var endingProfile: DestinationProfile? {
        model.endLive?.profileId.flatMap { id in model.destinations.profiles.first { $0.id == id } }
    }

    private var endLiveTitle: String {
        endingProfile.map { tr("end_one_title", $0.kind.displayName) } ?? tr("end_all_title")
    }

    private var endLiveMessage: String {
        let others = model.relay.state.liveProfileIds.count > 1
        if let ending = endingProfile {
            return sentenceStart(tr("end_one_message", tr(ending.kind.onPlatformKey)))
        }
        if model.relay.state.testVideoName != nil {
            return tr(others ? "end_all_test_video_many" : "end_all_test_video")
        }
        return tr(others ? "end_all_drone_many" : "end_all_drone")
    }
}
