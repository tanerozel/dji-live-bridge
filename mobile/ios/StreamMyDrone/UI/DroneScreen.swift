import SwiftUI

/// Which sheet is open over the picture.
enum DroneSheet: String, Identifiable {
    case platforms, details, theme, language
    var id: String { rawValue }
}

/**
 * The app's one screen, laid out like a camera app: what is connected and the stream's state at
 * the top, a switch per platform and the one big button at the bottom. Until the drone's picture
 * comes, a small card in the middle says how to connect DJI Fly; then the picture fills the screen.
 * "Preview" hides everything but the picture; a double tap switches between the whole picture and
 * a screen-filling one.
 */
struct DroneScreen: View {
    @Environment(AppModel.self) private var model
    @State private var sheet: DroneSheet?
    @State private var pictureOnly = false
    @State private var stopped = false
    @State private var pickingVideo = false
    @State private var inspecting = false

    var body: some View {
        let state = model.relay.state
        let showPicture = model.showPicture
        let otherFit: PictureFit = model.picture.shownFit == .whole ? .fill : .whole
        ZStack {
            if showPicture {
                DronePicture(state: model.picture, fit: model.pictureFit, pictureInPicture: state.isLive)
                // A picture that stopped coming must not look live, but a gap of a moment, such as
                // DJI Fly reconnecting at once, should not flash the screen.
                Color.black.opacity(stopped ? 0.55 : 0)
                    .ignoresSafeArea()
                    .contentShape(Rectangle())
                    .onTapGesture(count: 2) { model.setPictureFit(otherFit) }
                    .onTapGesture { if pictureOnly { withAnimation { pictureOnly = false } } }
                    .animation(.easeInOut, value: stopped)
            } else {
                DroneColors.waitingBackground.ignoresSafeArea()
            }
            if !pictureOnly {
                if showPicture { Scrims() }
                DroneControls(
                    sheet: $sheet,
                    showPicture: showPicture,
                    onPictureOnly: { withAnimation { pictureOnly = true } },
                    onFitToggle: { model.setPictureFit(otherFit) },
                    onTestVideo: { pickingVideo = true },
                    inspectingVideo: inspecting
                )
                .transition(.opacity)
            } else {
                ControlsHint()
            }
        }
        .environment(\.colorScheme, .dark)
        .onChange(of: showPicture) { if !showPicture { pictureOnly = false } }
        .task(id: model.phase.hasPicture) {
            if model.phase.hasPicture {
                stopped = false
            } else {
                try? await Task.sleep(for: .seconds(1))
                if !Task.isCancelled { stopped = true }
            }
        }
        .sheet(item: $sheet) { sheet in
            switch sheet {
            case .platforms: DarkSheet { PlatformsSheet() }
            case .details: DarkSheet { LiveDetails(onEndPlatform: { model.confirmEndLive($0.id) }) }
            case .theme: ThemePicker()
            case .language: LanguagePicker()
            }
        }
        .sheet(isPresented: $pickingVideo) {
            VideoPicker {
                pickingVideo = false
                inspecting = true
            } onPicked: { picked in
                pickingVideo = false
                guard let picked else {
                    inspecting = false
                    return
                }
                Task {
                    defer { inspecting = false }
                    do {
                        let selection = try await inspectTestVideo(url: picked.url, displayName: picked.name)
                        model.startTestVideo(selection)
                    } catch {
                        model.showMessage((error as? TestVideoError)?.text ?? .tr("test_video_unreadable"))
                    }
                }
            }
            .ignoresSafeArea()
        }
    }
}

/// Darkens the top and bottom edges so the controls over the picture stay readable.
private struct Scrims: View {
    var body: some View {
        VStack(spacing: 0) {
            LinearGradient(colors: [.black.opacity(0.55), .clear], startPoint: .top, endPoint: .bottom).frame(height: 180)
            Spacer(minLength: 0)
            LinearGradient(colors: [.clear, .black.opacity(0.7)], startPoint: .top, endPoint: .bottom).frame(height: 300)
        }
        .ignoresSafeArea()
        .allowsHitTesting(false)
    }
}

/// The one line shown while only the picture is on screen.
private struct ControlsHint: View {
    @State private var visible = true

    var body: some View {
        VStack {
            Spacer()
            if visible {
                Text(tr("controls_hint"))
                    .font(.caption)
                    .foregroundStyle(.white)
                    .padding(.horizontal, 12)
                    .padding(.vertical, 6)
                    .glass(Capsule())
                    .padding(.bottom, 24)
                    .transition(.opacity)
            }
        }
        .allowsHitTesting(false)
        .task {
            try? await Task.sleep(for: .milliseconds(2_500))
            withAnimation { visible = false }
        }
    }
}

/// A dark sheet like the screen under it, scrolling when its content is long.
struct DarkSheet<Content: View>: View {
    @ViewBuilder let content: Content

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 12) { content }
                .padding(.horizontal, Metrics.edge)
                .padding(.top, 24)
                .padding(.bottom, 24)
        }
        .foregroundStyle(.white)
        // The cards inside use the app's dark theme, whatever theme the user picked.
        .themed(.dark)
        .presentationDetents([.medium, .large])
        .presentationDragIndicator(.visible)
        .presentationBackground(DroneColors.sheet)
    }
}
