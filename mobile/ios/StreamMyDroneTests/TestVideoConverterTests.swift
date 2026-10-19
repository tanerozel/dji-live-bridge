import AVFoundation
import XCTest
@testable import StreamMyDrone

/// Picking a test video again while the last one is still converting cancels that conversion.
final class TestVideoConverterTests: XCTestCase {
    private var cache: URL {
        FileManager.default.urls(for: .cachesDirectory, in: .userDomainMask)[0].appendingPathComponent("test-video", isDirectory: true)
    }

    /// Cancelling at any moment, even while the reader starts, ends that conversion only, and the
    /// same video converted again at once still comes out whole.
    func testACancelledConversionDoesNotBreakTheNextOne() async throws {
        let source = try await makeVideo()
        defer { try? FileManager.default.removeItem(at: source) }
        for round in 0..<12 {
            try? FileManager.default.removeItem(at: cache)
            let cancelled = TestVideoConverter()
            let first = Task { try await cancelled.convert(source: source) { _ in } }
            try await Task.sleep(for: .milliseconds(round * 7 % 60))
            cancelled.cancel()
            let next = try await within(seconds: 30) { try await TestVideoConverter().convert(source: source) { _ in } }
            // The cancelled one ends too, rather than waiting for ever.
            _ = try? await within(seconds: 30) { try await first.value }
            XCTAssertTrue(FileManager.default.fileExists(atPath: next.path), "round \(round)")
            let tracks = try await AVURLAsset(url: next).load(.tracks)
            XCTAssertTrue(tracks.contains { $0.mediaType == .video }, "round \(round)")
        }
        try? FileManager.default.removeItem(at: cache)
    }

    /// `work`, failing instead of hanging when it takes longer than `seconds`.
    private func within<T: Sendable>(seconds: Int, _ work: @escaping @Sendable () async throws -> T) async throws -> T {
        try await withThrowingTaskGroup(of: T?.self) { group in
            group.addTask { try await work() }
            group.addTask { try await Task.sleep(for: .seconds(seconds)); return nil }
            defer { group.cancelAll() }
            guard let result = try await group.next(), let value = result else {
                XCTFail("Still running after \(seconds) s")
                throw CancellationError()
            }
            return value
        }
    }

    /// A one-second 640×360 H.264 clip with moving gray levels.
    private func makeVideo() async throws -> URL {
        let url = FileManager.default.temporaryDirectory.appendingPathComponent("converter-\(UUID().uuidString).mp4")
        let writer = try AVAssetWriter(outputURL: url, fileType: .mp4)
        let input = AVAssetWriterInput(mediaType: .video, outputSettings: [
            AVVideoCodecKey: AVVideoCodecType.h264, AVVideoWidthKey: 640, AVVideoHeightKey: 360,
        ])
        input.expectsMediaDataInRealTime = false
        let adaptor = AVAssetWriterInputPixelBufferAdaptor(assetWriterInput: input, sourcePixelBufferAttributes: [
            kCVPixelBufferPixelFormatTypeKey as String: kCVPixelFormatType_32BGRA,
            kCVPixelBufferWidthKey as String: 640, kCVPixelBufferHeightKey as String: 360,
        ])
        writer.add(input)
        XCTAssertTrue(writer.startWriting())
        writer.startSession(atSourceTime: .zero)
        for frame in 0..<30 {
            while !input.isReadyForMoreMediaData { try await Task.sleep(for: .milliseconds(5)) }
            var buffer: CVPixelBuffer?
            CVPixelBufferPoolCreatePixelBuffer(nil, try XCTUnwrap(adaptor.pixelBufferPool), &buffer)
            let pixels = try XCTUnwrap(buffer)
            CVPixelBufferLockBaseAddress(pixels, [])
            memset(CVPixelBufferGetBaseAddress(pixels), Int32(frame * 8), CVPixelBufferGetDataSize(pixels))
            CVPixelBufferUnlockBaseAddress(pixels, [])
            XCTAssertTrue(adaptor.append(pixels, withPresentationTime: CMTime(value: CMTimeValue(frame), timescale: 30)))
        }
        input.markAsFinished()
        await writer.finishWriting()
        XCTAssertEqual(writer.status, .completed)
        return url
    }
}
