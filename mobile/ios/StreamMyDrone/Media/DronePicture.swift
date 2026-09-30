import AVFoundation
import AVKit
import CoreImage
import Observation
import SwiftUI

/// What the decoder found out about the picture, for the screen around it.
@Observable
final class DronePictureState {
    /// The decoded picture's size, or nil until the first frame.
    var videoSize: CGSize?
    var unsupported = false
    /// How the picture is shown right now: `PictureFit.auto` resolved for the screen's shape.
    var shownFit = PictureFit.whole
    /// A tiny copy of a recent frame, blurred into the space around a whole picture.
    var backdrop: CGImage?
    /// The picture plays in a Picture in Picture window, which keeps the app running in the
    /// background.
    var pictureInPicture = false
}

/**
 * The drone's picture across the whole of the view's area, decoded on the phone from the relay's
 * preview tap. The stream to the platform never waits for it. While live, leaving the app moves
 * it into Picture in Picture, which is what lets iOS keep the broadcast running.
 */
struct DronePicture: View {
    let state: DronePictureState
    let fit: PictureFit
    /// Leaving the app starts Picture in Picture.
    let pictureInPicture: Bool

    var body: some View {
        GeometryReader { geometry in
            let aspect = state.videoSize.map { min(max($0.width / max($0.height, 1), 0.5), 2.4) } ?? 16 / 9
            let shown = shownFit(width: geometry.size.width, height: geometry.size.height, aspect: aspect, fit: fit)
            ZStack {
                Color.black
                if shown == .whole { AmbientBackdrop(image: state.backdrop) }
                // Sized to the video's shape, inside the area or, filling it, beyond it; the
                // screen's edges cut off what sticks out.
                let size = pictureSize(width: geometry.size.width, height: geometry.size.height, aspect: aspect, fit: shown)
                DronePictureLayer(state: state, pictureInPicture: pictureInPicture)
                    .frame(width: size.width, height: size.height)
                if state.videoSize == nil {
                    VStack(spacing: 8) {
                        if !state.unsupported { ProgressView().tint(.white) }
                        Text(tr(state.unsupported ? "preview_unsupported" : "preview_waiting"))
                            .font(.footnote)
                            .foregroundStyle(.white.opacity(0.8))
                            .multilineTextAlignment(.center)
                    }
                    .padding()
                }
            }
            .frame(width: geometry.size.width, height: geometry.size.height)
            .clipped()
            .onChange(of: shown, initial: true) { state.shownFit = shown }
        }
        .ignoresSafeArea()
        .accessibilityElement()
        .accessibilityLabel(tr("drone_picture"))
    }
}

/// The recent frame, blurred and dimmed, so the picture seems to fill the screen.
private struct AmbientBackdrop: View {
    let image: CGImage?

    var body: some View {
        ZStack {
            if let image {
                // A 32×18 copy stretched over the screen is already soft; the blur smooths it further.
                Image(decorative: image, scale: 1)
                    .resizable()
                    .scaledToFill()
                    .blur(radius: 40)
                    .transition(.opacity)
            }
            Color.black.opacity(0.45)
        }
        .animation(.easeInOut(duration: 0.5), value: image.map(ObjectIdentifier.init))
        .clipped()
    }
}

final class DisplayLayerView: UIView {
    override class var layerClass: AnyClass { AVSampleBufferDisplayLayer.self }
    var displayLayer: AVSampleBufferDisplayLayer { layer as! AVSampleBufferDisplayLayer }
}

private struct DronePictureLayer: UIViewRepresentable {
    let state: DronePictureState
    let pictureInPicture: Bool

    func makeCoordinator() -> Coordinator { Coordinator(state: state) }

    func makeUIView(context: Context) -> DisplayLayerView {
        let view = DisplayLayerView()
        view.backgroundColor = .clear
        view.displayLayer.videoGravity = .resize
        context.coordinator.start(view: view)
        context.coordinator.setPictureInPicture(pictureInPicture)
        return view
    }

    func updateUIView(_ view: DisplayLayerView, context: Context) {
        context.coordinator.setPictureInPicture(pictureInPicture)
    }

    static func dismantleUIView(_ view: DisplayLayerView, coordinator: Coordinator) {
        coordinator.stop()
    }

    @MainActor
    final class Coordinator: NSObject, AVPictureInPictureControllerDelegate, AVPictureInPictureSampleBufferPlaybackDelegate {
        private let state: DronePictureState
        private var decoder: DronePreviewDecoder?
        private var pictureInPicture: AVPictureInPictureController?
        private var sampler: Timer?
        private let context = CIContext(options: [.cacheIntermediates: false])

        init(state: DronePictureState) {
            self.state = state
        }

        func start(view: DisplayLayerView) {
            let state = state
            decoder = DronePreviewDecoder(
                renderer: view.displayLayer.sampleBufferRenderer,
                onVideoSize: { size in state.videoSize = size },
                onUnsupported: { state.unsupported = true }
            )
            if AVPictureInPictureController.isPictureInPictureSupported() {
                let source = AVPictureInPictureController.ContentSource(sampleBufferDisplayLayer: view.displayLayer, playbackDelegate: self)
                let controller = AVPictureInPictureController(contentSource: source)
                controller.delegate = self
                controller.requiresLinearPlayback = true
                pictureInPicture = controller
            }
            sampler = Timer.scheduledTimer(withTimeInterval: 0.7, repeats: true) { [weak self] _ in
                MainActor.assumeIsolated { self?.sampleBackdrop() }
            }
        }

        func stop() {
            sampler?.invalidate()
            sampler = nil
            pictureInPicture?.stopPictureInPicture()
            pictureInPicture = nil
            decoder?.stop()
            decoder = nil
            state.pictureInPicture = false
        }

        func setPictureInPicture(_ enabled: Bool) {
            guard let pictureInPicture, pictureInPicture.canStartPictureInPictureAutomaticallyFromInline != enabled else { return }
            if enabled { PlaybackSession.activate() }
            pictureInPicture.canStartPictureInPictureAutomaticallyFromInline = enabled
        }

        /// Copies a small version of the frame on screen, while a backdrop is shown.
        private func sampleBackdrop() {
            guard state.shownFit == .whole, let frame = decoder?.latestFrame else { return }
            let image = CIImage(cvPixelBuffer: frame)
            let scale = 32 / max(image.extent.width, 1)
            let small = image.transformed(by: CGAffineTransform(scaleX: scale, y: scale))
            state.backdrop = context.createCGImage(small, from: small.extent)
        }

        // MARK: Picture in Picture

        nonisolated func pictureInPictureControllerDidStartPictureInPicture(_ controller: AVPictureInPictureController) {
            Task { @MainActor in state.pictureInPicture = true }
        }

        nonisolated func pictureInPictureControllerDidStopPictureInPicture(_ controller: AVPictureInPictureController) {
            Task { @MainActor in state.pictureInPicture = false }
        }

        // A live picture: nothing to pause, seek or skip.
        nonisolated func pictureInPictureController(_ controller: AVPictureInPictureController, setPlaying playing: Bool) {}

        nonisolated func pictureInPictureControllerTimeRangeForPlayback(_ controller: AVPictureInPictureController) -> CMTimeRange {
            CMTimeRange(start: .negativeInfinity, duration: .positiveInfinity)
        }

        nonisolated func pictureInPictureControllerIsPlaybackPaused(_ controller: AVPictureInPictureController) -> Bool { false }

        nonisolated func pictureInPictureController(_ controller: AVPictureInPictureController, didTransitionToRenderSize newRenderSize: CMVideoDimensions) {}

        nonisolated func pictureInPictureController(
            _ controller: AVPictureInPictureController,
            skipByInterval skipInterval: CMTime,
            completion completionHandler: @escaping () -> Void
        ) {
            completionHandler()
        }
    }
}

/// Picture in Picture needs a playback audio session. The app plays no sound, so it mixes with
/// whatever else plays instead of interrupting it.
enum PlaybackSession {
    static func activate() {
        let session = AVAudioSession.sharedInstance()
        try? session.setCategory(.playback, mode: .moviePlayback, options: [.mixWithOthers])
        try? session.setActive(true)
    }
}
