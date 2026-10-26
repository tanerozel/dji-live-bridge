import AVFoundation
import Observation
import os

/**
 * Records the phone's microphone and hands it to the relay as AAC, which sends it in place of
 * DJI Fly's audio (silence on an RC 2): the pilot talks over the broadcast. 48 kHz mono AAC-LC,
 * as DJI Fly sends, so a platform sees the same audio format whichever of the two plays.
 *
 * The relay puts the frames on the drone's timeline; frames only go out while the drone's
 * picture comes in. The Android app's `PhoneMicrophone` does the same with MediaCodec.
 */
@MainActor
final class PhoneMicrophone {
    private let engine = AVAudioEngine()
    private let onFailed: (UiText) -> Void
    private var observers: [NSObjectProtocol] = []
    private var running = false

    init(onFailed: @escaping (UiText) -> Void) {
        self.onFailed = onFailed
    }

    func start() throws {
        PlaybackSession.recording = true
        try PlaybackSession.configure()
        let input = engine.inputNode
        let format = input.outputFormat(forBus: 0)
        guard format.sampleRate > 0, format.channelCount > 0 else { throw PhoneMicrophoneError("no input") }
        let encoder = try AacEncoder(input: format)
        if let error = NativeRelay.phoneAudioStart(config: AacEncoder.audioSpecificConfig) {
            throw PhoneMicrophoneError(error)
        }
        input.installTap(onBus: 0, bufferSize: 2048, format: format) { buffer, _ in
            encoder.encode(buffer)
        }
        engine.prepare()
        do {
            try engine.start()
        } catch {
            input.removeTap(onBus: 0)
            NativeRelay.phoneAudioStop()
            throw error
        }
        running = true
        let center = NotificationCenter.default
        // A call or Siri stops the engine; it records again once the interruption is over.
        observers.append(center.addObserver(forName: AVAudioSession.interruptionNotification, object: nil, queue: .main) { [weak self] note in
            let ended = (note.userInfo?[AVAudioSessionInterruptionTypeKey] as? UInt).flatMap(AVAudioSession.InterruptionType.init) == .ended
            guard ended else { return }
            MainActor.assumeIsolated { self?.resume() }
        })
        observers.append(center.addObserver(forName: AVAudioSession.mediaServicesWereResetNotification, object: nil, queue: .main) { [weak self] _ in
            MainActor.assumeIsolated { self?.fail(PhoneMicrophoneError("media services were reset")) }
        })
    }

    /// Stops recording and gives the broadcast DJI Fly's audio back.
    func stop() {
        observers.forEach(NotificationCenter.default.removeObserver)
        observers.removeAll()
        if running {
            engine.inputNode.removeTap(onBus: 0)
            engine.stop()
            running = false
        }
        NativeRelay.phoneAudioStop()
        PhoneMicrophoneLevel.shared.value = 0
        PlaybackSession.recording = false
        // Back to playback only, so the app no longer holds the microphone.
        try? PlaybackSession.configure()
    }

    private func resume() {
        guard running, !engine.isRunning else { return }
        do {
            try PlaybackSession.configure()
            try engine.start()
        } catch {
            fail(error)
        }
    }

    private func fail(_ error: Error) {
        guard running else { return }
        Logger.bridge.error("The phone's microphone stopped: \(error.localizedDescription, privacy: .public)")
        stop()
        onFailed(.tr("phone_mic_failed", .string(error.localizedDescription)))
    }
}

struct PhoneMicrophoneError: LocalizedError {
    let detail: String
    init(_ detail: String) { self.detail = detail }
    var errorDescription: String? { detail }
}

/// How loud the phone's microphone is, from 0 to 1, for the meter on screen.
@MainActor
@Observable
final class PhoneMicrophoneLevel {
    static let shared = PhoneMicrophoneLevel()
    var value: Float = 0
}

/**
 * Turns the microphone's PCM into AAC frames for the relay. Runs on the audio tap's thread only.
 * Timestamps count the encoded samples, so encoder bursts cannot move them; the relay realigns
 * them with the drone when the count and the clock drift apart.
 */
final class AacEncoder: @unchecked Sendable {
    static let sampleRate = 48_000.0
    /// AAC-LC (2), 48 kHz (sampling index 3), mono (1): the AudioSpecificConfig of `output`.
    static let audioSpecificConfig = Data([0x11, 0x88])
    private static let framesPerPacket: Int64 = 1024
    private static let levelInterval = 0.1

    private let converter: AVAudioConverter
    private let output: AVAudioFormat
    private var packets: Int64 = 0
    private var peak: Float = 0
    private var levelAt = ProcessInfo.processInfo.systemUptime

    init(input: AVAudioFormat) throws {
        guard let output = AVAudioFormat(settings: [
            AVFormatIDKey: kAudioFormatMPEG4AAC,
            AVSampleRateKey: Self.sampleRate,
            AVNumberOfChannelsKey: 1,
        ]), let converter = AVAudioConverter(from: input, to: output) else {
            throw PhoneMicrophoneError("no AAC encoder for \(input)")
        }
        converter.bitRate = 96_000
        self.output = output
        self.converter = converter
    }

    func encode(_ buffer: AVAudioPCMBuffer) {
        measure(buffer)
        var supplied = false
        while true {
            let packet = AVAudioCompressedBuffer(format: output, packetCapacity: 8, maximumPacketSize: max(converter.maximumOutputPacketSize, 1536))
            var error: NSError?
            let status = converter.convert(to: packet, error: &error) { _, inputStatus in
                if supplied {
                    inputStatus.pointee = .noDataNow
                    return nil
                }
                supplied = true
                inputStatus.pointee = .haveData
                return buffer
            }
            if status == .error {
                Logger.bridge.error("AAC encoding failed: \(error?.localizedDescription ?? "", privacy: .public)")
                return
            }
            send(packet)
            if status != .haveData { return }
        }
    }

    private func send(_ buffer: AVAudioCompressedBuffer) {
        guard let descriptions = buffer.packetDescriptions else { return }
        for index in 0..<Int(buffer.packetCount) {
            let description = descriptions[index]
            let bytes = buffer.data.advanced(by: Int(description.mStartOffset)).assumingMemoryBound(to: UInt8.self)
            let ptsUs = packets * Self.framesPerPacket * 1_000_000 / Int64(Self.sampleRate)
            NativeRelay.phoneAudioFrame(ptsUs: ptsUs, bytes: bytes, length: Int(description.mDataByteSize))
            packets += 1
        }
    }

    private func measure(_ buffer: AVAudioPCMBuffer) {
        if let samples = buffer.floatChannelData?[0] {
            for index in 0..<Int(buffer.frameLength) { peak = max(peak, abs(samples[index])) }
        }
        let now = ProcessInfo.processInfo.systemUptime
        guard now - levelAt >= Self.levelInterval else { return }
        let level = min(peak, 1)
        peak = 0
        levelAt = now
        DispatchQueue.main.async { PhoneMicrophoneLevel.shared.value = level }
    }
}
