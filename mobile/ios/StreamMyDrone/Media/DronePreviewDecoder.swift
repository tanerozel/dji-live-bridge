import AVFoundation
import CoreMedia
import os
import VideoToolbox

/**
 * Decodes the relay's H.264 tags with as little delay as the phone allows: the hardware decoder
 * in real-time mode, each frame shown the moment it is decoded (the display layer does not wait
 * for a presentation time), and frames far behind the newest input skipped, so after the cached
 * GOP is replayed at start-up the picture jumps straight to live. The relay never waits for it.
 */
final class DronePreviewDecoder {
    private let renderer: AVSampleBufferVideoRenderer
    private let onVideoSize: @MainActor (CGSize) -> Void
    private let onUnsupported: @MainActor () -> Void
    private let lock = NSLock()
    private var running = true
    private var latest: CVPixelBuffer?
    private let session = NativeRelay.previewStart()
    private let finished = DispatchSemaphore(value: 0)
    /// Presentation time of the newest frame given to the decoder.
    private var newestInputMs: Int64 = 0

    init(
        renderer: AVSampleBufferVideoRenderer,
        onVideoSize: @escaping @MainActor (CGSize) -> Void,
        onUnsupported: @escaping @MainActor () -> Void
    ) {
        self.renderer = renderer
        self.onVideoSize = onVideoSize
        self.onUnsupported = onUnsupported
        let thread = Thread { [self] in feed() }
        thread.name = "drone-preview"
        thread.qualityOfService = .userInteractive
        thread.start()
    }

    /// The most recently shown frame, for the blurred backdrop.
    var latestFrame: CVPixelBuffer? { lock.withLock { latest } }

    /// Stops decoding; returns within a fraction of a second.
    func stop() {
        lock.withLock { running = false }
        NativeRelay.previewStop(session: session)
        _ = finished.wait(timeout: .now() + 0.5)
    }

    private var isRunning: Bool { lock.withLock { running } }

    private func feed() {
        var decoder: Decoder?
        var configRecord: Data?
        var waitingForKeyframe = true
        var unsupportedTags = 0
        defer {
            NativeRelay.previewStop(session: session)
            decoder?.invalidate()
            finished.signal()
        }
        while isRunning {
            guard let (timestamp, body) = NativeRelay.previewNext(session: session, timeoutMs: 100) else { continue }
            switch parseFlvVideoTag(body) {
            case let .config(record) where record != configRecord:
                guard let config = parseAvcConfigurationRecord(record), let next = Decoder(config: config, owner: self) else { continue }
                decoder?.invalidate()
                decoder = next
                configRecord = record
                waitingForKeyframe = true
                let size = next.presentationSize
                Task { @MainActor in onVideoSize(size) }
            case let .picture(keyframe, compositionTimeMs, data):
                guard let current = decoder else { continue }
                if waitingForKeyframe && !keyframe { continue }
                let presentationMs = Int64(timestamp) + Int64(compositionTimeMs)
                // Set first: the frame may come out of the decoder before the call returns.
                lock.withLock { newestInputMs = presentationMs }
                if current.decode(data, presentationMs: presentationMs) {
                    waitingForKeyframe = false
                } else {
                    // The session fails after the app was in the background; a fresh one
                    // resumes at the next keyframe.
                    Logger.bridge.info("Preview decoder restarted; it resumes at the next keyframe")
                    decoder = Decoder(config: current.config, owner: self)
                    current.invalidate()
                    waitingForKeyframe = true
                }
            case .unsupported:
                unsupportedTags += 1
                if unsupportedTags == 30 && decoder == nil {
                    Task { @MainActor in onUnsupported() }
                }
            default:
                break
            }
        }
    }

    /// Shows a decoded frame unless a newer one is already on its way.
    fileprivate func show(_ image: CVImageBuffer, presentationMs: Int64) {
        let newest = lock.withLock { newestInputMs }
        // Far behind the newest input means a catch-up burst: skip it, don't replay it.
        guard newest - presentationMs <= 100 else { return }
        var format: CMVideoFormatDescription?
        guard CMVideoFormatDescriptionCreateForImageBuffer(allocator: nil, imageBuffer: image, formatDescriptionOut: &format) == noErr,
              let format else { return }
        var timing = CMSampleTimingInfo(
            duration: .invalid,
            presentationTimeStamp: CMTime(value: presentationMs, timescale: 1_000),
            decodeTimeStamp: .invalid
        )
        var sample: CMSampleBuffer?
        guard CMSampleBufferCreateReadyWithImageBuffer(allocator: nil, imageBuffer: image, formatDescription: format,
                                                       sampleTiming: &timing, sampleBufferOut: &sample) == noErr,
              let sample else { return }
        if let attachments = CMSampleBufferGetSampleAttachmentsArray(sample, createIfNecessary: true),
           CFArrayGetCount(attachments) > 0 {
            let dictionary = unsafeBitCast(CFArrayGetValueAtIndex(attachments, 0), to: CFMutableDictionary.self)
            CFDictionarySetValue(dictionary,
                                 Unmanaged.passUnretained(kCMSampleAttachmentKey_DisplayImmediately).toOpaque(),
                                 Unmanaged.passUnretained(kCFBooleanTrue).toOpaque())
        }
        if renderer.status == .failed || renderer.requiresFlushToResumeDecoding {
            renderer.flush()
        }
        renderer.enqueue(sample)
        lock.withLock { latest = image as CVPixelBuffer }
    }

    /// One VideoToolbox session for one set of parameter sets.
    private final class Decoder {
        let config: AvcDecoderConfig
        let presentationSize: CGSize
        private let format: CMVideoFormatDescription
        private let session: VTDecompressionSession
        private weak var owner: DronePreviewDecoder?

        init?(config: AvcDecoderConfig, owner: DronePreviewDecoder) {
            self.config = config
            self.owner = owner
            let parameterSets = config.sps + config.pps
            var format: CMVideoFormatDescription?
            let created = withPointers(parameterSets) { pointers, sizes in
                CMVideoFormatDescriptionCreateFromH264ParameterSets(
                    allocator: nil, parameterSetCount: parameterSets.count, parameterSetPointers: pointers,
                    parameterSetSizes: sizes, nalUnitHeaderLength: Int32(config.nalLengthSize), formatDescriptionOut: &format
                )
            }
            guard created == noErr, let format else { return nil }
            self.format = format
            presentationSize = CMVideoFormatDescriptionGetPresentationDimensions(format, usePixelAspectRatio: true, useCleanAperture: true)
            let attributes: [CFString: Any] = [
                kCVPixelBufferPixelFormatTypeKey: kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange,
                kCVPixelBufferIOSurfacePropertiesKey: [:] as [CFString: Any],
            ]
            var session: VTDecompressionSession?
            guard VTDecompressionSessionCreate(allocator: nil, formatDescription: format, decoderSpecification: nil,
                                               imageBufferAttributes: attributes as CFDictionary, outputCallback: nil,
                                               decompressionSessionOut: &session) == noErr,
                let session else { return nil }
            VTSessionSetProperty(session, key: kVTDecompressionPropertyKey_RealTime, value: kCFBooleanTrue)
            self.session = session
        }

        /// Decodes one access unit (length-prefixed NAL units, as FLV carries them).
        func decode(_ data: Data, presentationMs: Int64) -> Bool {
            var block: CMBlockBuffer?
            guard CMBlockBufferCreateWithMemoryBlock(allocator: nil, memoryBlock: nil, blockLength: data.count, blockAllocator: nil,
                                                     customBlockSource: nil, offsetToData: 0, dataLength: data.count,
                                                     flags: kCMBlockBufferAssureMemoryNowFlag, blockBufferOut: &block) == noErr,
                let block else { return false }
            let copied = data.withUnsafeBytes { bytes in
                CMBlockBufferReplaceDataBytes(with: bytes.baseAddress!, blockBuffer: block, offsetIntoDestination: 0, dataLength: data.count)
            }
            guard copied == noErr else { return false }
            var timing = CMSampleTimingInfo(duration: .invalid,
                                            presentationTimeStamp: CMTime(value: presentationMs, timescale: 1_000),
                                            decodeTimeStamp: .invalid)
            var size = data.count
            var sample: CMSampleBuffer?
            guard CMSampleBufferCreateReady(allocator: nil, dataBuffer: block, formatDescription: format, sampleCount: 1,
                                            sampleTimingEntryCount: 1, sampleTimingArray: &timing, sampleSizeEntryCount: 1,
                                            sampleSizeArray: &size, sampleBufferOut: &sample) == noErr,
                let sample else { return false }
            let status = VTDecompressionSessionDecodeFrame(session, sampleBuffer: sample, flags: [._1xRealTimePlayback],
                                                           infoFlagsOut: nil) { [weak owner] status, _, image, _, _ in
                guard status == noErr, let image else { return }
                owner?.show(image, presentationMs: presentationMs)
            }
            // A damaged frame is only skipped; a broken session is started again.
            return status == noErr || status == kVTVideoDecoderBadDataErr
        }

        func invalidate() {
            VTDecompressionSessionInvalidate(session)
        }
    }
}

private func withPointers<Result>(_ sets: [Data], _ body: ([UnsafePointer<UInt8>], [Int]) -> Result) -> Result {
    let buffers = sets.map { data -> UnsafeMutablePointer<UInt8> in
        let pointer = UnsafeMutablePointer<UInt8>.allocate(capacity: max(data.count, 1))
        data.copyBytes(to: pointer, count: data.count)
        return pointer
    }
    defer { buffers.forEach { $0.deallocate() } }
    return body(buffers.map { UnsafePointer($0) }, sets.map(\.count))
}

extension Logger {
    /// The app's log lines: Console.app filtered on "DJIBridge" shows how the stream is doing.
    static let bridge = Logger(subsystem: "com.streammydrone.app", category: "DJIBridge")
}
