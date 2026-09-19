import CoreMedia
import CoreMediaIO
import CoreVideo
import Darwin
import Foundation

private let cameraName = "DJI Live Bridge Camera"
private let cameraModel = "DJI Live Bridge Virtual Camera"
private let cameraWidth = 1080
private let cameraHeight = 1920
private let cameraFPS: Int32 = 30
private let frameSize = cameraWidth * cameraHeight * 3 / 2
private let frameServerPort: UInt16 = 49213

final class CameraExtensionProviderSource: NSObject, CMIOExtensionProviderSource {
    private(set) var provider: CMIOExtensionProvider!
    private let deviceSource: CameraExtensionDeviceSource

    override init() {
        deviceSource = CameraExtensionDeviceSource()
        super.init()
        provider = CMIOExtensionProvider(source: self, clientQueue: nil)
        try? provider.addDevice(deviceSource.device)
    }

    var availableProperties: Set<CMIOExtensionProperty> {
        [.providerManufacturer]
    }

    func providerProperties(forProperties properties: Set<CMIOExtensionProperty>) throws -> CMIOExtensionProviderProperties {
        let result = CMIOExtensionProviderProperties(dictionary: [:])
        if properties.contains(.providerManufacturer) {
            result.manufacturer = "DJI Live Bridge"
        }
        return result
    }

    func setProviderProperties(_ providerProperties: CMIOExtensionProviderProperties) throws {}

    func connect(to client: CMIOExtensionClient) throws {}

    func disconnect(from client: CMIOExtensionClient) {}
}

final class CameraExtensionDeviceSource: NSObject, CMIOExtensionDeviceSource {
    private(set) var device: CMIOExtensionDevice!
    private let streamSource: CameraExtensionStreamSource

    override init() {
        streamSource = CameraExtensionStreamSource()
        super.init()
        device = CMIOExtensionDevice(
            localizedName: cameraName,
            deviceID: UUID(uuidString: "2C488C22-BD91-4EB4-A500-00D152F91A3D")!,
            legacyDeviceID: "com.djilivebridge.camera",
            source: self
        )
        streamSource.attach(to: device)
    }

    var availableProperties: Set<CMIOExtensionProperty> {
        [.deviceModel]
    }

    func deviceProperties(forProperties properties: Set<CMIOExtensionProperty>) throws -> CMIOExtensionDeviceProperties {
        let result = CMIOExtensionDeviceProperties(dictionary: [:])
        if properties.contains(.deviceModel) {
            result.model = cameraModel
        }
        return result
    }

    func setDeviceProperties(_ deviceProperties: CMIOExtensionDeviceProperties) throws {}
}

final class CameraExtensionStreamSource: NSObject, CMIOExtensionStreamSource {
    private(set) var stream: CMIOExtensionStream!
    private let format: CMIOExtensionStreamFormat
    private let videoFormatDescription: CMVideoFormatDescription
    private let pixelBufferPool: CVPixelBufferPool
    private let fallbackFrame: [UInt8]
    private let queue = DispatchQueue(label: "com.djilivebridge.camera.frames", qos: .userInteractive)
    private var running = false
    private var frameIndex: Int64 = 0

    override init() {
        var description: CMFormatDescription?
        CMVideoFormatDescriptionCreate(
            allocator: kCFAllocatorDefault,
            codecType: kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange,
            width: Int32(cameraWidth),
            height: Int32(cameraHeight),
            extensions: nil,
            formatDescriptionOut: &description
        )
        videoFormatDescription = description!
        let duration = CMTime(value: 1, timescale: cameraFPS)
        format = CMIOExtensionStreamFormat(
            formatDescription: videoFormatDescription,
            maxFrameDuration: duration,
            minFrameDuration: duration,
            validFrameDurations: nil
        )

        let pixelAttributes: [CFString: Any] = [
            kCVPixelBufferWidthKey: cameraWidth,
            kCVPixelBufferHeightKey: cameraHeight,
            kCVPixelBufferPixelFormatTypeKey: kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange,
            kCVPixelBufferIOSurfacePropertiesKey: [:],
            kCVPixelBufferMetalCompatibilityKey: true,
        ]
        var pool: CVPixelBufferPool?
        CVPixelBufferPoolCreate(
            kCFAllocatorDefault,
            nil,
            pixelAttributes as CFDictionary,
            &pool
        )
        pixelBufferPool = pool!

        var fallback = [UInt8](repeating: 16, count: frameSize)
        fallback.withUnsafeMutableBytes { raw in
            guard let base = raw.baseAddress else { return }
            memset(base.advanced(by: cameraWidth * cameraHeight), 128, cameraWidth * cameraHeight / 2)
        }
        fallbackFrame = fallback
        super.init()
        stream = CMIOExtensionStream(
            localizedName: cameraName,
            streamID: UUID(uuidString: "C00F9D8C-D77E-476D-94AA-6C493E428C36")!,
            direction: .source,
            clockType: .hostTime,
            source: self
        )
    }

    func attach(to device: CMIOExtensionDevice) {
        try? device.addStream(stream)
    }

    var formats: [CMIOExtensionStreamFormat] { [format] }

    var availableProperties: Set<CMIOExtensionProperty> {
        [.streamActiveFormatIndex, .streamFrameDuration]
    }

    func streamProperties(forProperties properties: Set<CMIOExtensionProperty>) throws -> CMIOExtensionStreamProperties {
        let result = CMIOExtensionStreamProperties(dictionary: [:])
        if properties.contains(.streamActiveFormatIndex) {
            result.activeFormatIndex = 0
        }
        if properties.contains(.streamFrameDuration) {
            result.frameDuration = CMTime(value: 1, timescale: cameraFPS)
        }
        return result
    }

    func setStreamProperties(_ streamProperties: CMIOExtensionStreamProperties) throws {
        if let requestedIndex = streamProperties.activeFormatIndex, requestedIndex != 0 {
            throw NSError(
                domain: "com.djilivebridge.camera",
                code: 1,
                userInfo: [NSLocalizedDescriptionKey: "Only the portrait format is supported"]
            )
        }
    }

    func authorizedToStartStream(for client: CMIOExtensionClient) -> Bool { true }

    func startStream() throws {
        guard !running else { return }
        running = true
        frameIndex = 0
        queue.async { [weak self] in self?.readFrames() }
    }

    func stopStream() throws {
        running = false
    }

    private func readFrames() {
        var descriptor: Int32 = -1
        var bytes = [UInt8](repeating: 0, count: frameSize)
        var offset = 0

        while running {
            if descriptor < 0 {
                descriptor = connectToFrameServer()
                if descriptor < 0 {
                    sendFallbackFrame()
                    usleep(100_000)
                    continue
                }
            }

            let count = bytes.withUnsafeMutableBytes { rawBuffer in
                guard let baseAddress = rawBuffer.baseAddress else { return -1 }
                return read(
                    descriptor,
                    baseAddress.advanced(by: offset),
                    frameSize - offset
                )
            }
            if count > 0 {
                offset += count
                if offset == frameSize {
                    if let sampleBuffer = makeSampleBuffer(bytes) {
                        send(sampleBuffer)
                    }
                    offset = 0
                }
            } else if count == 0 {
                close(descriptor)
                descriptor = -1
                offset = 0
                usleep(20_000)
            } else if errno == EAGAIN || errno == EWOULDBLOCK {
                usleep(2_000)
            } else {
                close(descriptor)
                descriptor = -1
                offset = 0
                usleep(20_000)
            }
        }
        if descriptor >= 0 { close(descriptor) }
    }

    private func connectToFrameServer() -> Int32 {
        let descriptor = socket(AF_INET, SOCK_STREAM, 0)
        guard descriptor >= 0 else { return -1 }

        var address = sockaddr_in()
        address.sin_len = UInt8(MemoryLayout<sockaddr_in>.size)
        address.sin_family = sa_family_t(AF_INET)
        address.sin_port = frameServerPort.bigEndian
        address.sin_addr = in_addr(s_addr: inet_addr("127.0.0.1"))

        let result = withUnsafePointer(to: &address) { pointer in
            pointer.withMemoryRebound(to: sockaddr.self, capacity: 1) { socketAddress in
                connect(descriptor, socketAddress, socklen_t(MemoryLayout<sockaddr_in>.size))
            }
        }
        guard result == 0 else {
            close(descriptor)
            return -1
        }

        let flags = fcntl(descriptor, F_GETFL, 0)
        if flags >= 0 {
            _ = fcntl(descriptor, F_SETFL, flags | O_NONBLOCK)
        }
        return descriptor
    }

    private func sendFallbackFrame() {
        if let sampleBuffer = makeSampleBuffer(fallbackFrame) {
            send(sampleBuffer)
        }
    }

    private func makeSampleBuffer(_ frame: [UInt8]) -> CMSampleBuffer? {
        var pixelBuffer: CVPixelBuffer?
        guard CVPixelBufferPoolCreatePixelBuffer(
            kCFAllocatorDefault,
            pixelBufferPool,
            &pixelBuffer
        ) == kCVReturnSuccess, let pixelBuffer else { return nil }

        CVPixelBufferLockBaseAddress(pixelBuffer, [])
        defer { CVPixelBufferUnlockBaseAddress(pixelBuffer, []) }
        frame.withUnsafeBytes { raw in
            guard let source = raw.baseAddress else { return }
            copyPlane(
                source: source,
                sourceStride: cameraWidth,
                destination: CVPixelBufferGetBaseAddressOfPlane(pixelBuffer, 0)!,
                destinationStride: CVPixelBufferGetBytesPerRowOfPlane(pixelBuffer, 0),
                rows: cameraHeight,
                bytesPerRow: cameraWidth
            )
            copyPlane(
                source: source.advanced(by: cameraWidth * cameraHeight),
                sourceStride: cameraWidth,
                destination: CVPixelBufferGetBaseAddressOfPlane(pixelBuffer, 1)!,
                destinationStride: CVPixelBufferGetBytesPerRowOfPlane(pixelBuffer, 1),
                rows: cameraHeight / 2,
                bytesPerRow: cameraWidth
            )
        }

        let presentationTime = CMTime(value: frameIndex, timescale: cameraFPS)
        var timing = CMSampleTimingInfo(
            duration: CMTime(value: 1, timescale: cameraFPS),
            presentationTimeStamp: presentationTime,
            decodeTimeStamp: .invalid
        )
        var sampleBuffer: CMSampleBuffer?
        guard CMSampleBufferCreateReadyWithImageBuffer(
            allocator: kCFAllocatorDefault,
            imageBuffer: pixelBuffer,
            formatDescription: videoFormatDescription,
            sampleTiming: &timing,
            sampleBufferOut: &sampleBuffer
        ) == noErr else { return nil }
        frameIndex += 1
        return sampleBuffer
    }

    private func send(_ sampleBuffer: CMSampleBuffer) {
        let hostTime = CMClockGetTime(CMClockGetHostTimeClock())
        stream.send(
            sampleBuffer,
            discontinuity: [],
            hostTimeInNanoseconds: CMClockConvertHostTimeToSystemUnits(hostTime)
        )
    }

    private func copyPlane(
        source: UnsafeRawPointer,
        sourceStride: Int,
        destination: UnsafeMutableRawPointer,
        destinationStride: Int,
        rows: Int,
        bytesPerRow: Int
    ) {
        for row in 0..<rows {
            memcpy(
                destination.advanced(by: row * destinationStride),
                source.advanced(by: row * sourceStride),
                bytesPerRow
            )
        }
    }
}
