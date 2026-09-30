@preconcurrency import AVFoundation
import CoreMedia
import CryptoKit
import Foundation

/// A test video problem the user can act on, in words for the screen.
struct TestVideoError: Error {
    let text: UiText

    init(_ key: String) { text = .tr(key) }
}

/**
 * A video the user picked to stand in for the drone, copied into the app's caches. With
 * `convert` it is first turned into what DJI Fly sends (`TestVideoConverter`); `durationMs` is
 * how much of it plays.
 */
struct TestVideoSelection {
    let url: URL
    let displayName: String?
    let durationMs: Int64?
    let convert: Bool
}

/**
 * What DJI Fly streams, and so what a test video is turned into: H.264 with a 720-pixel short
 * side, 30 frames a second, about 4 Mbps and a keyframe every second, with AAC sound. Phone
 * recordings are far heavier (4K HEVC at 50–100 Mbps is common), more than the uplink or any
 * platform takes, and the relay forwards a stream as it is.
 */
enum DroneLikeVideo {
    static let shortSide = 720
    static let frameRate = 30
    static let bitrate = 4_000_000
    static let keyframeIntervalSeconds = 1.0
    /// Only the start is converted: the test video loops, and a long one would take minutes.
    static let maxDurationMs: Int64 = 60_000
    /// A little above `bitrate`, so a video already like DJI Fly's goes out as it is.
    static let maxBitrateAsIs: Int64 = 5_000_000
}

/// The picked video's picture and sound, as far as deciding whether it can go out as it is.
struct VideoFileInfo {
    let codec: FourCharCode
    let width: Int
    let height: Int
    let rotationDegrees: Int
    let frameRate: Float?
    /// Bits per second of the video, or nil when unknown.
    let bitrate: Int64?
    let hdr: Bool
    /// The first sound track's format, or nil for a silent video such as most drone footage.
    let audioFormat: AudioFormatID?
}

/**
 * Whether the video has to be converted before it can stand in for DJI Fly. FLV carries no
 * rotation, so a video that is displayed rotated is converted too; an unknown bitrate counts as
 * too high. DJI Fly always sends AAC sound, and Instagram and Facebook show nothing of a stream
 * without any, so a silent video gets a silent sound track.
 */
func needsConversion(_ video: VideoFileInfo) -> Bool {
    video.audioFormat != kAudioFormatMPEG4AAC ||
        video.codec != kCMVideoCodecType_H264 ||
        video.rotationDegrees % 360 != 0 ||
        min(video.width, video.height) > DroneLikeVideo.shortSide ||
        (video.frameRate ?? 0) > Float(DroneLikeVideo.frameRate) + 1 ||
        (video.bitrate ?? .max) > DroneLikeVideo.maxBitrateAsIs ||
        video.hdr
}

/**
 * Checks a picked video before the bridge starts, so a file without a picture is reported right
 * away, and decides whether it must be converted first.
 */
func inspectTestVideo(url: URL, displayName: String?) async throws -> TestVideoSelection {
    let asset = AVURLAsset(url: url)
    let tracks: [AVAssetTrack]
    let duration: CMTime
    do {
        (tracks, duration) = try await asset.load(.tracks, .duration)
    } catch {
        throw TestVideoError("test_video_open_failed")
    }
    guard let video = tracks.first(where: { $0.mediaType == .video }) else {
        throw TestVideoError("test_video_no_video")
    }
    let info = try await videoFileInfo(video: video, audio: tracks.first { $0.mediaType == .audio })
    let durationMs = duration.isNumeric ? Int64(duration.seconds * 1_000) : nil
    let convert = needsConversion(info)
    let playedMs = convert ? durationMs.map { min($0, DroneLikeVideo.maxDurationMs) } : durationMs
    return TestVideoSelection(url: url, displayName: displayName, durationMs: playedMs, convert: convert)
}

private func videoFileInfo(video: AVAssetTrack, audio: AVAssetTrack?) async throws -> VideoFileInfo {
    let (formats, size, transform, frameRate, dataRate) = try await video.load(
        .formatDescriptions, .naturalSize, .preferredTransform, .nominalFrameRate, .estimatedDataRate
    )
    let format = formats.first
    let transfer = format.flatMap {
        CMFormatDescriptionGetExtension($0, extensionKey: kCMFormatDescriptionExtension_TransferFunction) as? String
    }
    let hdr = transfer == (kCMFormatDescriptionTransferFunction_SMPTE_ST_2084_PQ as String) ||
        transfer == (kCMFormatDescriptionTransferFunction_ITU_R_2100_HLG as String)
    let audioFormat: AudioFormatID?
    if let audio, let description = try await audio.load(.formatDescriptions).first {
        audioFormat = CMFormatDescriptionGetMediaSubType(description)
    } else {
        audioFormat = nil
    }
    let degrees = Int((atan2(transform.b, transform.a) * 180 / .pi).rounded())
    return VideoFileInfo(
        codec: format.map(CMFormatDescriptionGetMediaSubType) ?? 0,
        width: Int(size.width),
        height: Int(size.height),
        rotationDegrees: (degrees + 360) % 360,
        frameRate: frameRate > 0 ? frameRate : nil,
        bitrate: dataRate > 0 ? Int64(dataRate) : nil,
        hdr: hdr,
        audioFormat: audioFormat
    )
}

/**
 * "drone.mp4 · 00:20 · looping". A name made of digits only (as some pickers hand out) means
 * nothing to the user, so it gives way to "Video".
 */
func testVideoLabel(displayName: String?, durationMs: Int64?) -> UiText {
    let stem = displayName.map { ($0 as NSString).deletingPathExtension }
    let name = stem.flatMap { $0.isEmpty || $0.allSatisfy(\.isNumber) ? nil : displayName }
    return .joined(
        [name.map(UiText.raw) ?? .tr("test_video_default_name")] +
            (durationMs.map { [UiText.raw(formatDuration(millis: $0))] } ?? []) +
            [.tr("test_video_looping")],
        separator: " · "
    )
}

// MARK: - Conversion

/**
 * Converts a picked video into a `DroneLikeVideo` file in the app's caches: upright, scaled to a
 * 720-pixel short side, at most 30 fps, SDR (HDR is tone mapped), H.264 High 4.0 at 4 Mbps with
 * a keyframe every second and no B-frames, and AAC sound (silence for a silent video). The last
 * result is kept, so the same video starts at once the next time.
 */
final class TestVideoConverter {
    private let lock = NSLock()
    private var reader: AVAssetReader?
    private var writer: AVAssetWriter?
    private var cancelled = false

    /// Converts `source`; `onProgress` gets whole percents on the main thread.
    func convert(source: URL, onProgress: @escaping @MainActor (Int) -> Void) async throws -> URL {
        let directory = FileManager.default.urls(for: .cachesDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("test-video", isDirectory: true)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        let name = try cacheName(source)
        let converted = directory.appendingPathComponent("\(name).mp4")
        if FileManager.default.fileExists(atPath: converted.path) { return converted }
        // One converted video is enough: an older one, or one left half written, goes.
        for file in (try? FileManager.default.contentsOfDirectory(at: directory, includingPropertiesForKeys: nil)) ?? [] {
            try? FileManager.default.removeItem(at: file)
        }
        let partial = directory.appendingPathComponent("\(name)-partial.mp4")
        do {
            try await run(source: source, output: partial, onProgress: onProgress)
            try FileManager.default.moveItem(at: partial, to: converted)
            return converted
        } catch {
            try? FileManager.default.removeItem(at: partial)
            throw error
        }
    }

    func cancel() {
        lock.withLock {
            cancelled = true
            reader?.cancelReading()
            writer?.cancelWriting()
        }
    }

    private var isCancelled: Bool { lock.withLock { cancelled } }

    private func run(source: URL, output: URL, onProgress: @escaping @MainActor (Int) -> Void) async throws {
        let asset = AVURLAsset(url: source)
        let (tracks, assetDuration) = try await asset.load(.tracks, .duration)
        guard let video = tracks.first(where: { $0.mediaType == .video }) else { throw TestVideoError("test_video_no_video") }
        let audio = tracks.first { $0.mediaType == .audio }
        let (naturalSize, transform, nominalFrameRate) = try await video.load(.naturalSize, .preferredTransform, .nominalFrameRate)
        let clip = CMTimeMinimum(assetDuration, CMTime(value: DroneLikeVideo.maxDurationMs, timescale: 1_000))

        // The picture as it is displayed, then scaled so its short side is 720 (never enlarged).
        let displayed = CGRect(origin: .zero, size: naturalSize).applying(transform).standardized
        let scale = min(1, CGFloat(DroneLikeVideo.shortSide) / min(displayed.width, displayed.height))
        let outputSize = CGSize(width: evenFloor(displayed.width * scale), height: evenFloor(displayed.height * scale))
        let placement = transform
            .concatenating(CGAffineTransform(translationX: -displayed.minX, y: -displayed.minY))
            .concatenating(CGAffineTransform(scaleX: scale, y: scale))
        let composition = AVMutableVideoComposition()
        composition.renderSize = outputSize
        let fps = nominalFrameRate > 0 ? min(Double(nominalFrameRate), Double(DroneLikeVideo.frameRate)) : Double(DroneLikeVideo.frameRate)
        composition.frameDuration = CMTime(value: 1_000, timescale: CMTimeScale((fps * 1_000).rounded()))
        // RTMP carries 8-bit SDR H.264, as DJI Fly sends; an HDR phone video is tone mapped.
        composition.colorPrimaries = AVVideoColorPrimaries_ITU_R_709_2
        composition.colorTransferFunction = AVVideoTransferFunction_ITU_R_709_2
        composition.colorYCbCrMatrix = AVVideoYCbCrMatrix_ITU_R_709_2
        let instruction = AVMutableVideoCompositionInstruction()
        instruction.timeRange = CMTimeRange(start: .zero, duration: clip)
        let layer = AVMutableVideoCompositionLayerInstruction(assetTrack: video)
        layer.setTransform(placement, at: .zero)
        instruction.layerInstructions = [layer]
        composition.instructions = [instruction]

        let reader = try AVAssetReader(asset: asset)
        reader.timeRange = CMTimeRange(start: .zero, duration: clip)
        let videoOutput = AVAssetReaderVideoCompositionOutput(
            videoTracks: [video],
            videoSettings: [kCVPixelBufferPixelFormatTypeKey as String: kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange]
        )
        videoOutput.videoComposition = composition
        videoOutput.alwaysCopiesSampleData = false
        reader.add(videoOutput)
        var audioOutput: AVAssetReaderAudioMixOutput?
        if let audio {
            let output = AVAssetReaderAudioMixOutput(audioTracks: [audio], audioSettings: [
                AVFormatIDKey: kAudioFormatLinearPCM,
                AVSampleRateKey: 48_000,
                AVNumberOfChannelsKey: 2,
                AVLinearPCMBitDepthKey: 16,
                AVLinearPCMIsFloatKey: false,
                AVLinearPCMIsBigEndianKey: false,
                AVLinearPCMIsNonInterleaved: false,
            ])
            if reader.canAdd(output) {
                reader.add(output)
                audioOutput = output
            }
        }

        let writer = try AVAssetWriter(outputURL: output, fileType: .mp4)
        let videoInput = AVAssetWriterInput(mediaType: .video, outputSettings: [
            AVVideoCodecKey: AVVideoCodecType.h264,
            AVVideoWidthKey: outputSize.width,
            AVVideoHeightKey: outputSize.height,
            AVVideoColorPropertiesKey: [
                AVVideoColorPrimariesKey: AVVideoColorPrimaries_ITU_R_709_2,
                AVVideoTransferFunctionKey: AVVideoTransferFunction_ITU_R_709_2,
                AVVideoYCbCrMatrixKey: AVVideoYCbCrMatrix_ITU_R_709_2,
            ],
            AVVideoCompressionPropertiesKey: [
                AVVideoAverageBitRateKey: DroneLikeVideo.bitrate,
                AVVideoExpectedSourceFrameRateKey: DroneLikeVideo.frameRate,
                AVVideoMaxKeyFrameIntervalKey: DroneLikeVideo.frameRate,
                AVVideoMaxKeyFrameIntervalDurationKey: DroneLikeVideo.keyframeIntervalSeconds,
                // High 4.0 like DJI Fly's stream; a platform may refuse a far higher level.
                AVVideoProfileLevelKey: AVVideoProfileLevelH264High40,
                AVVideoAllowFrameReorderingKey: false,
                AVVideoH264EntropyModeKey: AVVideoH264EntropyModeCABAC,
            ],
        ])
        videoInput.expectsMediaDataInRealTime = false
        writer.add(videoInput)
        let audioInput = AVAssetWriterInput(mediaType: .audio, outputSettings: [
            AVFormatIDKey: kAudioFormatMPEG4AAC,
            AVSampleRateKey: 48_000,
            AVNumberOfChannelsKey: audioOutput == nil ? 1 : 2,
            AVEncoderBitRateKey: audioOutput == nil ? 64_000 : 128_000,
        ])
        audioInput.expectsMediaDataInRealTime = false
        writer.add(audioInput)

        try lock.withLock {
            if cancelled { throw CancellationError() }
            self.reader = reader
            self.writer = writer
        }
        guard reader.startReading() else { throw reader.error ?? TestVideoError("test_video_convert_failed") }
        guard writer.startWriting() else { throw writer.error ?? TestVideoError("test_video_convert_failed") }
        writer.startSession(atSourceTime: .zero)

        let clipSeconds = max(clip.seconds, 0.001)
        let queue = DispatchQueue(label: "test-video-convert")
        try await withThrowingTaskGroup(of: Void.self) { group in
            group.addTask {
                try await pump(input: videoInput, on: queue) {
                    guard let sample = videoOutput.copyNextSampleBuffer() else { return nil }
                    let seconds = CMSampleBufferGetPresentationTimeStamp(sample).seconds
                    let percent = Int(min(99, max(0, seconds / clipSeconds * 100)))
                    Task { @MainActor in onProgress(percent) }
                    return sample
                }
            }
            group.addTask {
                if let audioOutput {
                    try await pump(input: audioInput, on: queue) { audioOutput.copyNextSampleBuffer() }
                } else {
                    // Declaring sound makes the result carry it; a silent video gets silence.
                    var silence = SilenceGenerator(duration: clip)
                    try await pump(input: audioInput, on: queue) { silence.next() }
                }
            }
            try await group.waitForAll()
        }
        if isCancelled || reader.status == .failed {
            writer.cancelWriting()
            throw isCancelled ? CancellationError() : (reader.error ?? TestVideoError("test_video_convert_failed"))
        }
        await writer.finishWriting()
        guard writer.status == .completed else { throw writer.error ?? TestVideoError("test_video_convert_failed") }
        await onProgress(100)
    }

    /// The same video with the same settings maps to the same name.
    private func cacheName(_ source: URL) throws -> String {
        let handle = try FileHandle(forReadingFrom: source)
        defer { try? handle.close() }
        let head = try handle.read(upToCount: 1 << 20) ?? Data()
        let size = (try? source.resourceValues(forKeys: [.fileSizeKey]).fileSize) ?? 0
        var hasher = SHA256()
        hasher.update(data: head)
        hasher.update(data: Data("|\(size)|\(DroneLikeVideo.shortSide)|\(DroneLikeVideo.frameRate)|\(DroneLikeVideo.bitrate)|high40|\(DroneLikeVideo.maxDurationMs)|with-sound".utf8))
        return hasher.finalize().prefix(12).map { String(format: "%02x", $0) }.joined()
    }
}

/// Feeds `input` from `next` whenever it takes more, until `next` runs dry.
private func pump(input: AVAssetWriterInput, on queue: DispatchQueue, next: @escaping () -> CMSampleBuffer?) async throws {
    await Pump(input: input, next: next).run(on: queue)
}

/// The writer input calls back on one serial queue only, so the state here is never shared.
private final class Pump: @unchecked Sendable {
    private let input: AVAssetWriterInput
    private let next: () -> CMSampleBuffer?
    private var finished = false

    init(input: AVAssetWriterInput, next: @escaping () -> CMSampleBuffer?) {
        self.input = input
        self.next = next
    }

    func run(on queue: DispatchQueue) async {
        await withCheckedContinuation { (continuation: CheckedContinuation<Void, Never>) in
            input.requestMediaDataWhenReady(on: queue) { [self] in
                while !finished && input.isReadyForMoreMediaData {
                    if let sample = next(), input.append(sample) { continue }
                    input.markAsFinished()
                    finished = true
                    continuation.resume()
                }
            }
        }
    }
}

/// 16-bit mono LPCM silence at 48 kHz in 1024-frame buffers, for a video without sound.
private struct SilenceGenerator {
    private static let sampleRate: Int32 = 48_000
    private static let frames = 1_024
    private let duration: CMTime
    private var position: Int64 = 0
    private let format: CMAudioFormatDescription?

    init(duration: CMTime) {
        self.duration = duration
        var description = AudioStreamBasicDescription(
            mSampleRate: Float64(Self.sampleRate),
            mFormatID: kAudioFormatLinearPCM,
            mFormatFlags: kLinearPCMFormatFlagIsSignedInteger | kLinearPCMFormatFlagIsPacked,
            mBytesPerPacket: 2, mFramesPerPacket: 1, mBytesPerFrame: 2, mChannelsPerFrame: 1, mBitsPerChannel: 16,
            mReserved: 0
        )
        var format: CMAudioFormatDescription?
        CMAudioFormatDescriptionCreate(allocator: nil, asbd: &description, layoutSize: 0, layout: nil,
                                       magicCookieSize: 0, magicCookie: nil, extensions: nil, formatDescriptionOut: &format)
        self.format = format
    }

    mutating func next() -> CMSampleBuffer? {
        let time = CMTime(value: position, timescale: Self.sampleRate)
        guard let format, time < duration else { return nil }
        let bytes = Self.frames * 2
        var block: CMBlockBuffer?
        guard CMBlockBufferCreateWithMemoryBlock(allocator: nil, memoryBlock: nil, blockLength: bytes, blockAllocator: nil,
                                                 customBlockSource: nil, offsetToData: 0, dataLength: bytes,
                                                 flags: kCMBlockBufferAssureMemoryNowFlag, blockBufferOut: &block) == noErr,
            let block, CMBlockBufferFillDataBytes(with: 0, blockBuffer: block, offsetIntoDestination: 0, dataLength: bytes) == noErr
        else { return nil }
        var sample: CMSampleBuffer?
        guard CMAudioSampleBufferCreateReadyWithPacketDescriptions(allocator: nil, dataBuffer: block, formatDescription: format,
                                                                   sampleCount: Self.frames, presentationTimeStamp: time,
                                                                   packetDescriptions: nil, sampleBufferOut: &sample) == noErr
        else { return nil }
        position += Int64(Self.frames)
        return sample
    }
}

private func evenFloor(_ value: CGFloat) -> CGFloat {
    max(2, (value / 2).rounded(.down) * 2)
}

// MARK: - Streaming

/**
 * Sends a video file to the bridge's own ingest exactly as DJI Fly would: H.264 and AAC copied
 * without re-encoding, paced in real time and looped until the task is cancelled.
 */
struct TestVideoStreamer {
    let url: URL

    func run() async throws {
        let asset = AVURLAsset(url: url)
        let (tracks, duration) = try await asset.load(.tracks, .duration)
        guard let video = tracks.first(where: { $0.mediaType == .video }),
              let videoFormat = try await video.load(.formatDescriptions).first,
              CMFormatDescriptionGetMediaSubType(videoFormat) == kCMVideoCodecType_H264
        else { throw TestVideoError("test_video_unsupported") }
        guard let configuration = avcConfiguration(videoFormat) else { throw TestVideoError("test_video_no_avc_config") }
        var audio: AVAssetTrack?
        var audioConfig: Data?
        var audioFormat: CMAudioFormatDescription?
        if let track = tracks.first(where: { $0.mediaType == .audio }),
           let format = try await track.load(.formatDescriptions).first,
           CMFormatDescriptionGetMediaSubType(format) == kAudioFormatMPEG4AAC,
           let config = aacConfiguration(format) {
            audio = track
            audioConfig = config
            audioFormat = format
        }
        let (naturalSize, frameRate) = try await video.load(.naturalSize, .nominalFrameRate)
        let frameSeconds = frameRate > 0 ? 1 / Double(frameRate) : 1.0 / 30
        // One frame of gap between loops keeps timestamps strictly increasing.
        let loopMs = Int64(((duration.isNumeric ? duration.seconds : 0) + frameSeconds) * 1_000)
        guard loopMs > 0 else { throw TestVideoError("test_video_no_frames") }

        let rtmp = RtmpPublisher(host: "127.0.0.1", port: 1935, app: "drone")
        defer { rtmp.close() }
        try rtmp.connect()
        var metadata: [(String, AmfValue)] = [
            ("width", .number(Double(naturalSize.width))),
            ("height", .number(Double(naturalSize.height))),
            ("videocodecid", .number(7)),
        ]
        if frameRate > 0 { metadata.append(("framerate", .number(Double(frameRate)))) }
        if let audioFormat, let description = CMAudioFormatDescriptionGetStreamBasicDescription(audioFormat)?.pointee {
            metadata.append(("audiocodecid", .number(10)))
            metadata.append(("audiosamplerate", .number(description.mSampleRate)))
            metadata.append(("stereo", .boolean(description.mChannelsPerFrame > 1)))
        }
        metadata.append(("encoder", .string("StreamMyDrone test video")))
        try rtmp.sendMetadata(metadata)
        try rtmp.sendVideo(timestamp: 0, flvAvcSequenceHeader(configuration))
        if let audioConfig { try rtmp.sendAudio(timestamp: 0, flvAacSequenceHeader(audioConfig)) }

        let started = ContinuousClock.now
        var loop: Int64 = 0
        while !Task.isCancelled {
            try await playOnce(asset: asset, video: video, audio: audio, rtmp: rtmp, offsetMs: loop * loopMs, started: started)
            loop += 1
        }
    }

    private func playOnce(
        asset: AVURLAsset,
        video: AVAssetTrack,
        audio: AVAssetTrack?,
        rtmp: RtmpPublisher,
        offsetMs: Int64,
        started: ContinuousClock.Instant
    ) async throws {
        let reader = try AVAssetReader(asset: asset)
        let videoOutput = AVAssetReaderTrackOutput(track: video, outputSettings: nil)
        videoOutput.alwaysCopiesSampleData = false
        reader.add(videoOutput)
        let audioOutput = audio.map { AVAssetReaderTrackOutput(track: $0, outputSettings: nil) }
        if let audioOutput {
            audioOutput.alwaysCopiesSampleData = false
            reader.add(audioOutput)
        }
        guard reader.startReading() else { throw TestVideoError("test_video_rewind_failed") }
        defer { reader.cancelReading() }

        var pendingVideo = MediaQueue(output: videoOutput, video: true)
        var pendingAudio = audioOutput.map { MediaQueue(output: $0, video: false) }
        var sentAny = false
        while !Task.isCancelled {
            let nextVideo = pendingVideo.peek()
            let nextAudio = pendingAudio?.peek()
            let useVideo: Bool
            switch (nextVideo, nextAudio) {
            case (nil, nil):
                if !sentAny { throw TestVideoError("test_video_no_frames") }
                return
            case (.some, nil): useVideo = true
            case (nil, .some): useVideo = false
            case let (.some(v), .some(a)): useVideo = v.decodeMs <= a.decodeMs
            }
            let unit = useVideo ? pendingVideo.pop()! : pendingAudio!.pop()!
            let timestamp = unit.decodeMs + offsetMs
            let wait = started + .milliseconds(timestamp) - ContinuousClock.now
            if wait > .zero { try await Task.sleep(for: wait) }
            if unit.video {
                try rtmp.sendVideo(timestamp: timestamp, flvAvcFrame(unit.data, keyframe: unit.keyframe,
                                                                     compositionTimeMs: Int32(max(0, unit.compositionMs))))
            } else {
                try rtmp.sendAudio(timestamp: timestamp, flvAacFrame(unit.data))
            }
            sentAny = true
        }
    }
}

/// One access unit or AAC frame with its timing in milliseconds.
private struct MediaUnit {
    let video: Bool
    let decodeMs: Int64
    let compositionMs: Int64
    let keyframe: Bool
    let data: Data
}

/// The samples of one track output, split into single frames (AAC buffers carry many).
private struct MediaQueue {
    let output: AVAssetReaderTrackOutput
    let video: Bool
    private var units: [MediaUnit] = []
    private var finished = false

    init(output: AVAssetReaderTrackOutput, video: Bool) {
        self.output = output
        self.video = video
    }

    mutating func peek() -> MediaUnit? {
        while units.isEmpty && !finished {
            guard let sample = output.copyNextSampleBuffer() else {
                finished = true
                break
            }
            units = split(sample)
        }
        return units.first
    }

    mutating func pop() -> MediaUnit? {
        guard peek() != nil else { return nil }
        return units.removeFirst()
    }

    private func split(_ sample: CMSampleBuffer) -> [MediaUnit] {
        guard let block = CMSampleBufferGetDataBuffer(sample) else { return [] }
        let length = CMBlockBufferGetDataLength(block)
        var bytes = Data(count: length)
        let copied = bytes.withUnsafeMutableBytes {
            CMBlockBufferCopyDataBytes(block, atOffset: 0, dataLength: length, destination: $0.baseAddress!)
        }
        guard copied == noErr, length > 0 else { return [] }
        let count = CMSampleBufferGetNumSamples(sample)
        var timing = [CMSampleTimingInfo](repeating: CMSampleTimingInfo(), count: max(count, 1))
        var timingCount = 0
        CMSampleBufferGetSampleTimingInfoArray(sample, entryCount: timing.count, arrayToFill: &timing, entriesNeededOut: &timingCount)
        var sizes = [Int](repeating: 0, count: max(count, 1))
        var sizeCount = 0
        CMSampleBufferGetSampleSizeArray(sample, entryCount: sizes.count, arrayToFill: &sizes, entriesNeededOut: &sizeCount)
        let attachments = CMSampleBufferGetSampleAttachmentsArray(sample, createIfNecessary: false) as? [[CFString: Any]]

        var units: [MediaUnit] = []
        var offset = 0
        for index in 0..<max(count, 1) {
            let size = sizeCount == 1 ? (count > 1 ? length / count : length) : (index < sizeCount ? sizes[index] : 0)
            guard size > 0, offset + size <= length else { break }
            let info = timingCount == 1 && count > 1 ? extrapolated(timing[0], index: index) : timing[min(index, max(timingCount - 1, 0))]
            let presentation = info.presentationTimeStamp
            let decode = info.decodeTimeStamp.isValid ? info.decodeTimeStamp : presentation
            let keyframe = video && !((attachments?[safe: index]?[kCMSampleAttachmentKey_NotSync] as? Bool) ?? false)
            units.append(MediaUnit(
                video: video,
                decodeMs: milliseconds(decode),
                compositionMs: milliseconds(presentation) - milliseconds(decode),
                keyframe: keyframe,
                data: bytes.subdata(in: offset..<offset + size)
            ))
            offset += size
        }
        return units
    }

    /// Timing of the `index`th of several equally long frames that share one timing entry.
    private func extrapolated(_ first: CMSampleTimingInfo, index: Int) -> CMSampleTimingInfo {
        var info = first
        let step = CMTimeMultiply(first.duration, multiplier: Int32(index))
        info.presentationTimeStamp = CMTimeAdd(first.presentationTimeStamp, step)
        if first.decodeTimeStamp.isValid { info.decodeTimeStamp = CMTimeAdd(first.decodeTimeStamp, step) }
        return info
    }
}

private func milliseconds(_ time: CMTime) -> Int64 {
    time.isNumeric ? Int64((time.seconds * 1_000).rounded()) : 0
}

/// The avcC record MP4 files carry for H.264: the sequence header's payload.
private func avcConfiguration(_ format: CMFormatDescription) -> Data? {
    let atoms = CMFormatDescriptionGetExtension(format, extensionKey: kCMFormatDescriptionExtension_SampleDescriptionExtensionAtoms) as? [String: Any]
    return atoms?["avcC"] as? Data
}

/// The AudioSpecificConfig of an AAC track, from its magic cookie or, lacking one, its format.
private func aacConfiguration(_ format: CMAudioFormatDescription) -> Data? {
    var size = 0
    if let cookie = CMAudioFormatDescriptionGetMagicCookie(format, sizeOut: &size), size > 0,
       let config = audioSpecificConfig(fromMagicCookie: Data(bytes: cookie, count: size)) {
        return config
    }
    guard let description = CMAudioFormatDescriptionGetStreamBasicDescription(format)?.pointee else { return nil }
    let objectType = description.mFormatFlags > 0 ? Int(description.mFormatFlags) : 2
    return audioSpecificConfig(objectType: objectType, sampleRate: description.mSampleRate, channels: Int(description.mChannelsPerFrame))
}

extension Array {
    subscript(safe index: Int) -> Element? {
        indices.contains(index) ? self[index] : nil
    }
}
