import Foundation

// FLV tag bodies for H.264 + AAC, the stream DJI Fly itself sends over RTMP: built for the test
// video, read back for the live preview.

private let avcCodecId: UInt8 = 7

func flvAvcSequenceHeader(_ configurationRecord: Data) -> Data {
    Data([0x17, 0x00, 0x00, 0x00, 0x00]) + configurationRecord
}

/// A video tag body: frame type + codec, NALU packet, 24-bit composition time, AVCC data.
func flvAvcFrame(_ avcc: Data, keyframe: Bool, compositionTimeMs: Int32) -> Data {
    let composition = UInt32(bitPattern: compositionTimeMs)
    return Data([
        keyframe ? 0x17 : 0x27,
        0x01,
        UInt8(truncatingIfNeeded: composition >> 16),
        UInt8(truncatingIfNeeded: composition >> 8),
        UInt8(truncatingIfNeeded: composition),
    ]) + avcc
}

// 0xAF: AAC; the rate/size/channel bits are fixed for AAC and the decoder uses the ASC instead.
func flvAacSequenceHeader(_ audioSpecificConfig: Data) -> Data {
    Data([0xAF, 0x00]) + audioSpecificConfig
}

func flvAacFrame(_ data: Data) -> Data {
    Data([0xAF, 0x01]) + data
}

/// What the preview needs from one FLV video tag body.
enum FlvVideoTag: Equatable {
    case config(Data)
    case picture(keyframe: Bool, compositionTimeMs: Int32, data: Data)
    /// Enhanced-RTMP or non-AVC video; DJI Fly sends legacy AVC, which is all the preview shows.
    case unsupported
}

/// Reads a tag body; nil for tags the preview can skip.
func parseFlvVideoTag(_ bytes: Data) -> FlvVideoTag? {
    let bytes = Data(bytes) // Index from zero.
    guard bytes.count >= 2 else { return nil }
    let header = bytes[0]
    if header & 0x80 != 0 || header & 0x0F != avcCodecId { return .unsupported }
    guard bytes.count >= 5 else { return nil }
    let body = bytes.subdata(in: 5..<bytes.count)
    switch bytes[1] {
    case 0:
        return .config(body)
    case 1:
        let raw = UInt32(bytes[2]) << 16 | UInt32(bytes[3]) << 8 | UInt32(bytes[4])
        // Sign-extend the 24-bit composition time.
        let composition = Int32(bitPattern: raw << 8) >> 8
        return .picture(keyframe: header >> 4 == 1, compositionTimeMs: composition, data: body)
    default:
        return nil // end of sequence
    }
}

struct AvcDecoderConfig: Equatable {
    let sps: [Data]
    let pps: [Data]
    let nalLengthSize: Int
}

func parseAvcConfigurationRecord(_ record: Data) -> AvcDecoderConfig? {
    let record = Data(record)
    guard record.count >= 7, record[0] == 1 else { return nil }
    var position = 5
    func readParameterSets(_ count: Int) -> [Data]? {
        var sets: [Data] = []
        for _ in 0..<count {
            guard position + 2 <= record.count else { return nil }
            let length = Int(record[position]) << 8 | Int(record[position + 1])
            position += 2
            guard position + length <= record.count else { return nil }
            sets.append(record.subdata(in: position..<position + length))
            position += length
        }
        return sets
    }
    let spsCount = Int(record[position] & 0x1F)
    position += 1
    guard let sps = readParameterSets(spsCount), position < record.count else { return nil }
    let ppsCount = Int(record[position])
    position += 1
    guard let pps = readParameterSets(ppsCount), !sps.isEmpty, !pps.isEmpty else { return nil }
    return AvcDecoderConfig(sps: sps, pps: pps, nalLengthSize: Int(record[4] & 0x03) + 1)
}

/**
 * The AudioSpecificConfig inside an AAC magic cookie. MP4 files store an ES descriptor ("esds"
 * contents), whose DecoderSpecificInfo (tag 5) is the ASC; a cookie that is not a descriptor is
 * taken to be the ASC itself.
 */
func audioSpecificConfig(fromMagicCookie cookie: Data) -> Data? {
    let bytes = [UInt8](cookie)
    var index: Int
    if bytes.first == 0x03 {
        index = 0
    } else if bytes.count > 4, bytes[0..<4].allSatisfy({ $0 == 0 }), bytes[4] == 0x03 {
        index = 4 // The whole esds box body, with its version and flags.
    } else {
        return bytes.count >= 2 ? cookie : nil
    }
    func readLength() -> Int {
        var length = 0
        for _ in 0..<4 where index < bytes.count {
            let byte = bytes[index]
            index += 1
            length = length << 7 | Int(byte & 0x7F)
            if byte & 0x80 == 0 { break }
        }
        return length
    }
    // Walk the descriptors: ES (3) → DecoderConfig (4) → DecoderSpecificInfo (5).
    while index < bytes.count {
        let tag = bytes[index]
        index += 1
        let length = readLength()
        switch tag {
        case 0x03:
            index += 2 // ES_ID
            guard index < bytes.count else { return nil }
            let flags = bytes[index]
            index += 1
            if flags & 0x80 != 0 { index += 2 }
            if flags & 0x40 != 0, index < bytes.count { index += 1 + Int(bytes[index]) }
            if flags & 0x20 != 0 { index += 2 }
        case 0x04:
            index += 13 // objectType, streamType, bufferSize, maxBitrate, avgBitrate
        case 0x05:
            guard length >= 2, index + length <= bytes.count else { return nil }
            return Data(bytes[index..<index + length])
        default:
            index += length
        }
    }
    return nil
}

/// A two-byte AudioSpecificConfig for AAC with `objectType` (2 = AAC-LC) and the stream's
/// sample rate and channel count, for audio whose container carries none.
func audioSpecificConfig(objectType: Int, sampleRate: Double, channels: Int) -> Data? {
    let rates: [Double] = [96_000, 88_200, 64_000, 48_000, 44_100, 32_000, 24_000, 22_050, 16_000, 12_000, 11_025, 8_000, 7_350]
    guard let rateIndex = rates.firstIndex(of: sampleRate), (1...7).contains(channels), (1...31).contains(objectType) else {
        return nil
    }
    let value = UInt16(objectType) << 11 | UInt16(rateIndex) << 7 | UInt16(channels) << 3
    return Data([UInt8(value >> 8), UInt8(value & 0xFF)])
}
