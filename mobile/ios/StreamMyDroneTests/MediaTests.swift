import XCTest
@testable import StreamMyDrone

final class FlvMediaTests: XCTestCase {
    private let sps = Data([0x67, 0x64, 0x00, 0x1F, 0xAC, 0xD9])
    private let pps = Data([0x68, 0xEB, 0xE3, 0xCB])

    private var record: Data {
        Data([1, 0x64, 0x00, 0x1F, 0xFF, 0xE1, 0x00, UInt8(sps.count)]) + sps + Data([1, 0x00, UInt8(pps.count)]) + pps
    }

    func testTheConfigurationRecordIsReadBack() {
        let config = parseAvcConfigurationRecord(record)
        XCTAssertEqual(config, AvcDecoderConfig(sps: [sps], pps: [pps], nalLengthSize: 4))
        XCTAssertNil(parseAvcConfigurationRecord(Data([1, 2, 3])))
        XCTAssertNil(parseAvcConfigurationRecord(record.prefix(10)))
    }

    func testTagsRoundTrip() {
        XCTAssertEqual(parseFlvVideoTag(flvAvcSequenceHeader(record)), .config(record))
        let frame = Data([0, 0, 0, 2, 0x65, 0x88])
        XCTAssertEqual(parseFlvVideoTag(flvAvcFrame(frame, keyframe: true, compositionTimeMs: 66)),
                       .picture(keyframe: true, compositionTimeMs: 66, data: frame))
        // The 24-bit composition time is signed.
        XCTAssertEqual(parseFlvVideoTag(flvAvcFrame(frame, keyframe: false, compositionTimeMs: -33)),
                       .picture(keyframe: false, compositionTimeMs: -33, data: frame))
    }

    func testEnhancedAndNonAvcVideoIsReportedAsUnsupported() {
        XCTAssertEqual(parseFlvVideoTag(Data([0x90, 0x68, 0x76, 0x63, 0x31])), .unsupported) // enhanced RTMP (hvc1)
        XCTAssertEqual(parseFlvVideoTag(Data([0x12, 0x00])), .unsupported) // Sorenson H.263
        XCTAssertNil(parseFlvVideoTag(Data([0x17])))
    }

    func testAacTagsCarryTheirHeader() {
        XCTAssertEqual(flvAacSequenceHeader(Data([0x12, 0x10])), Data([0xAF, 0x00, 0x12, 0x10]))
        XCTAssertEqual(flvAacFrame(Data([0x21, 0x00])), Data([0xAF, 0x01, 0x21, 0x00]))
    }

    func testTheAudioSpecificConfigComesOutOfAnEsDescriptor() {
        // ES_Descriptor → DecoderConfigDescriptor → DecoderSpecificInfo (AAC-LC, 48 kHz, stereo).
        let esds = Data([
            0x03, 0x19, 0x00, 0x00, 0x00,
            0x04, 0x11, 0x40, 0x15, 0x00, 0x00, 0x00, 0x00, 0x01, 0xF4, 0x00, 0x00, 0x01, 0xF4, 0x00,
            0x05, 0x02, 0x11, 0x90,
            0x06, 0x01, 0x02,
        ])
        XCTAssertEqual(audioSpecificConfig(fromMagicCookie: esds), Data([0x11, 0x90]))
        XCTAssertEqual(audioSpecificConfig(fromMagicCookie: Data([0, 0, 0, 0]) + esds), Data([0x11, 0x90]))
        XCTAssertEqual(audioSpecificConfig(fromMagicCookie: Data([0x12, 0x10])), Data([0x12, 0x10]))
        XCTAssertEqual(audioSpecificConfig(objectType: 2, sampleRate: 48_000, channels: 1), Data([0x11, 0x88]))
        XCTAssertEqual(audioSpecificConfig(objectType: 2, sampleRate: 44_100, channels: 2), Data([0x12, 0x10]))
        XCTAssertNil(audioSpecificConfig(objectType: 2, sampleRate: 12_345, channels: 2))
    }
}

final class RtmpTests: XCTestCase {
    func testAMessageIsSplitIntoChunksWithAFullFirstHeader() {
        let payload = Data((0..<300).map { UInt8($0 & 0xFF) })
        let chunks = encodeRtmpChunks(chunkStreamId: 6, messageStreamId: 1, typeId: 9, timestamp: 1_000, payload: payload, chunkSize: 128)
        XCTAssertEqual([UInt8](chunks.prefix(12)), [0x06, 0x00, 0x03, 0xE8, 0x00, 0x01, 0x2C, 0x09, 0x01, 0x00, 0x00, 0x00])
        // 300 bytes in 128-byte chunks: two type 3 continuation headers.
        XCTAssertEqual(chunks.count, 12 + 300 + 2)
        XCTAssertEqual(chunks[12 + 128], 0xC6)
    }

    func testALargeTimestampUsesTheExtendedField() {
        let chunks = encodeRtmpChunks(chunkStreamId: 4, messageStreamId: 1, typeId: 8, timestamp: 0x0100_0000,
                                      payload: Data(repeating: 0, count: 200), chunkSize: 128)
        XCTAssertEqual([UInt8](chunks[1...3]), [0xFF, 0xFF, 0xFF])
        XCTAssertEqual([UInt8](chunks[12...15]), [0x01, 0x00, 0x00, 0x00])
        // Continuation chunks repeat it, as librtmp2's reader expects.
        XCTAssertEqual([UInt8](chunks[16 + 128...16 + 128 + 4]), [0xC4, 0x01, 0x00, 0x00, 0x00])
    }

    func testTheChunkReaderReassemblesWhatTheEncoderSplit() throws {
        let payload = Data((0..<1_000).map { UInt8($0 % 251) })
        let bytes = encodeRtmpChunks(chunkStreamId: 3, messageStreamId: 0, typeId: 20, timestamp: 5, payload: payload, chunkSize: 128)
        let reader = RtmpChunkReader(input: DataInput(bytes))
        let message = try reader.readMessage()
        XCTAssertEqual(message.typeId, 20)
        XCTAssertEqual(message.payload, payload)
    }

    func testAmfRoundTripsCommandsAndMetadata() {
        let values: [AmfValue] = [
            .string("connect"), .number(1), .object([("app", .string("drone")), ("tcUrl", .string("rtmp://127.0.0.1:1935/drone"))]),
            .null, .boolean(true), .ecmaArray([("width", .number(1280)), ("stereo", .boolean(false))]),
            .array([.number(1), .string("a")]),
        ]
        XCTAssertEqual(Amf0.decode(Amf0.encode(values)), values)
        XCTAssertEqual(Amf0.encode([.number(1)]), Data([0x00, 0x3F, 0xF0, 0, 0, 0, 0, 0, 0]))
    }

    func testDecodingStopsAtAnUnknownMarkerButKeepsWhatCameBefore() {
        let bytes = Amf0.encode([.string("_result"), .number(2)]) + Data([0x42, 0x00])
        XCTAssertEqual(Amf0.decode(bytes), [.string("_result"), .number(2)])
    }
}

/// Bytes read from memory, standing in for a socket.
private final class DataInput: ByteInput {
    private let bytes: [UInt8]
    private var position = 0

    init(_ data: Data) { bytes = [UInt8](data) }

    func readByte() throws -> UInt8 {
        guard position < bytes.count else { throw RtmpError(message: "end") }
        defer { position += 1 }
        return bytes[position]
    }

    func read(_ count: Int) throws -> Data {
        guard position + count <= bytes.count else { throw RtmpError(message: "end") }
        defer { position += count }
        return Data(bytes[position..<position + count])
    }
}
