import Darwin
import Foundation

struct RtmpError: Error, LocalizedError {
    let message: String
    var errorDescription: String? { message }
}

/**
 * Just enough of an RTMP publisher to feed the bridge's own ingest over loopback the way DJI Fly
 * does: legacy handshake, connect → createStream → publish, then FLV-tag media messages. It
 * answers the server's pings (librtmp2 drops peers that stay silent for 10 seconds); everything
 * else the server sends is read and ignored.
 */
final class RtmpPublisher {
    private let host: String
    private let port: UInt16
    private let app: String
    private var socket: Int32 = -1
    private let writeLock = NSLock()
    private var input: SocketInput!
    private var reader: RtmpChunkReader!
    private var outChunkSize = defaultChunkSize
    private var streamId = 0
    private var readerThread: Thread?
    private let state = NSLock()
    private var closed = false
    private var failure: Error?

    init(host: String, port: UInt16, app: String) {
        self.host = host
        self.port = port
        self.app = app
    }

    deinit { close() }

    func connect() throws {
        socket = Darwin.socket(AF_INET, SOCK_STREAM, 0)
        guard socket >= 0 else { throw RtmpError(message: "socket failed (\(errno))") }
        var on: Int32 = 1
        setsockopt(socket, SOL_SOCKET, SO_NOSIGPIPE, &on, socklen_t(MemoryLayout<Int32>.size))
        setsockopt(socket, IPPROTO_TCP, TCP_NODELAY, &on, socklen_t(MemoryLayout<Int32>.size))
        setReceiveTimeout(seconds: 5)
        var address = sockaddr_in()
        address.sin_family = sa_family_t(AF_INET)
        address.sin_port = port.bigEndian
        inet_pton(AF_INET, host, &address.sin_addr)
        let connected = withUnsafePointer(to: &address) {
            $0.withMemoryRebound(to: sockaddr.self, capacity: 1) {
                Darwin.connect(socket, $0, socklen_t(MemoryLayout<sockaddr_in>.size))
            }
        }
        guard connected == 0 else { throw RtmpError(message: "the local receiver refused the connection (\(errno))") }
        input = SocketInput(socket: socket)
        reader = RtmpChunkReader(input: input)
        try handshake()

        try send(csid: csidControl, type: typeSetChunkSize, streamId: 0, timestamp: 0, payload: int32(publishChunkSize))
        outChunkSize = publishChunkSize
        let connect: [(String, AmfValue)] = [
            ("app", .string(app)),
            ("type", .string("nonprivate")),
            ("flashVer", .string("FMLE/3.0 (compatible; StreamMyDrone)")),
            ("tcUrl", .string("rtmp://\(host):\(port)/\(app)")),
        ]
        try sendCommand(streamId: 0, [.string("connect"), .number(1), .object(connect)])
        _ = try awaitResult(transaction: 1)
        try sendCommand(streamId: 0, [.string("createStream"), .number(2), .null])
        guard case let .number(id)? = try awaitResult(transaction: 2).dropFirst(3).first else {
            throw RtmpError(message: "the local receiver did not open a stream")
        }
        streamId = Int(id)
        try sendCommand(streamId: streamId, [.string("publish"), .number(3), .null, .string(""), .string("live")])
        try awaitPublishStart()

        setReceiveTimeout(seconds: 0)
        let thread = Thread { [weak self] in self?.drainServer() }
        thread.name = "rtmp-test-reader"
        thread.start()
        readerThread = thread
    }

    func sendMetadata(_ metadata: [(String, AmfValue)]) throws {
        try sendMedia(csid: csidData, type: typeDataAmf0, timestamp: 0,
                      payload: Amf0.encode([.string("@setDataFrame"), .string("onMetaData"), .ecmaArray(metadata)]))
    }

    func sendVideo(timestamp: Int64, _ payload: Data) throws {
        try sendMedia(csid: csidVideo, type: typeVideo, timestamp: timestamp, payload: payload)
    }

    func sendAudio(timestamp: Int64, _ payload: Data) throws {
        try sendMedia(csid: csidAudio, type: typeAudio, timestamp: timestamp, payload: payload)
    }

    func close() {
        state.lock()
        let wasClosed = closed
        closed = true
        state.unlock()
        guard !wasClosed, socket >= 0 else { return }
        if streamId != 0 {
            try? sendCommand(streamId: streamId, [.string("deleteStream"), .number(4), .null, .number(Double(streamId))])
        }
        shutdown(socket, SHUT_RDWR)
        Darwin.close(socket)
    }

    private func setReceiveTimeout(seconds: Int) {
        var timeout = timeval(tv_sec: seconds, tv_usec: 0)
        setsockopt(socket, SOL_SOCKET, SO_RCVTIMEO, &timeout, socklen_t(MemoryLayout<timeval>.size))
    }

    private func sendMedia(csid: Int, type: Int, timestamp: Int64, payload: Data) throws {
        state.lock()
        let failed = failure
        state.unlock()
        if let failed { throw failed }
        try send(csid: csid, type: type, streamId: streamId, timestamp: timestamp, payload: payload)
    }

    private func handshake() throws {
        var c1 = Data(count: handshakeSize)
        c1.withUnsafeMutableBytes { bytes in
            arc4random_buf(bytes.baseAddress! + 8, handshakeSize - 8)
        }
        try write(Data([UInt8(rtmpVersion)]) + c1)
        let version = try input.readByte()
        guard version == rtmpVersion else { throw RtmpError(message: "unexpected RTMP version \(version)") }
        let s1 = try input.read(handshakeSize)
        _ = try input.read(handshakeSize)
        try write(s1)
    }

    private func sendCommand(streamId: Int, _ values: [AmfValue]) throws {
        try send(csid: csidCommand, type: typeCommandAmf0, streamId: streamId, timestamp: 0, payload: Amf0.encode(values))
    }

    private func send(csid: Int, type: Int, streamId: Int, timestamp: Int64, payload: Data) throws {
        try write(encodeRtmpChunks(chunkStreamId: csid, messageStreamId: streamId, typeId: type,
                                   timestamp: timestamp, payload: payload, chunkSize: outChunkSize))
    }

    private func write(_ data: Data) throws {
        writeLock.lock()
        defer { writeLock.unlock() }
        try data.withUnsafeBytes { buffer in
            var offset = 0
            while offset < buffer.count {
                let written = Darwin.send(socket, buffer.baseAddress! + offset, buffer.count - offset, 0)
                if written < 0 {
                    if errno == EINTR { continue }
                    throw RtmpError(message: "the local receiver closed the connection (\(errno))")
                }
                offset += written
            }
        }
    }

    /// Reads until the reply to `transaction`; control messages met on the way are handled.
    private func awaitResult(transaction: Double) throws -> [AmfValue] {
        while true {
            guard let values = try nextCommand(), values.count > 1, values[1] == .number(transaction) else { continue }
            switch values.first {
            case .string("_result")?: return values
            case .string("_error")?: throw RtmpError(message: "the local receiver refused the request: \(statusDescription(values))")
            default: continue
            }
        }
    }

    private func awaitPublishStart() throws {
        while true {
            guard let values = try nextCommand(), values.first == .string("onStatus") else { continue }
            let info = values.count > 3 ? values[3].properties : [:]
            let code = info["code"]?.stringValue
            if code == "NetStream.Publish.Start" { return }
            if info["level"]?.stringValue == "error" || code?.contains("Publish") == true {
                throw RtmpError(message: "the local receiver did not accept the test stream: \(code ?? "unknown status")")
            }
        }
    }

    private func nextCommand() throws -> [AmfValue]? {
        let message = try reader.readMessage()
        if message.typeId == typeCommandAmf0 { return Amf0.decode(message.payload) }
        try handleControl(message)
        return nil
    }

    private func drainServer() {
        do {
            while true {
                state.lock()
                let isClosed = closed
                state.unlock()
                if isClosed { return }
                let message = try reader.readMessage()
                guard message.typeId == typeCommandAmf0 else {
                    try handleControl(message)
                    continue
                }
                let values = Amf0.decode(message.payload)
                let info = values.count > 3 ? values[3].properties : [:]
                if values.first == .string("onStatus"), info["level"]?.stringValue == "error" {
                    fail(RtmpError(message: "the local receiver stopped the test stream: \(info["code"]?.stringValue ?? "")"))
                }
            }
        } catch {
            state.lock()
            let isClosed = closed
            state.unlock()
            if !isClosed { fail(error) }
        }
    }

    private func fail(_ error: Error) {
        state.lock()
        failure = error
        state.unlock()
    }

    private func handleControl(_ message: RtmpMessage) throws {
        let payload = [UInt8](message.payload)
        switch message.typeId {
        case typeSetChunkSize where payload.count >= 4:
            reader.chunkSize = Int(UInt32(payload[0]) << 24 | UInt32(payload[1]) << 16 | UInt32(payload[2]) << 8 | UInt32(payload[3])) & 0x7FFF_FFFF
        case typeUserControl where payload.count >= 6 && (Int(payload[0]) << 8 | Int(payload[1])) == userControlPingRequest:
            var response = [UInt8](repeating: 0, count: 6)
            response[1] = UInt8(userControlPingResponse)
            response[2...5] = payload[2...5]
            try send(csid: csidControl, type: typeUserControl, streamId: 0, timestamp: 0, payload: Data(response))
        default:
            break
        }
    }

    private func statusDescription(_ values: [AmfValue]) -> String {
        let info = values.count > 3 ? values[3].properties : [:]
        return info["description"]?.stringValue ?? info["code"]?.stringValue ?? "unknown error"
    }
}

private let rtmpVersion = 3
private let handshakeSize = 1536
private let publishChunkSize = 4_096
private let csidControl = 2
private let csidCommand = 3
private let csidAudio = 4
private let csidData = 5
private let csidVideo = 6
private let userControlPingRequest = 6
private let userControlPingResponse = 7

let defaultChunkSize = 128
let typeSetChunkSize = 1
let typeUserControl = 4
let typeAudio = 8
let typeVideo = 9
let typeDataAmf0 = 18
let typeCommandAmf0 = 20
private let maxMessageBytes = 16 * 1024 * 1024
private let extendedTimestamp: UInt32 = 0xFFFFFF

struct RtmpMessage {
    let typeId: Int
    let payload: Data
}

/**
 * Splits one message into chunks. Every message starts with a full (type 0) header, which costs
 * a few bytes per frame but keeps the writer stateless. Continuation chunks repeat the extended
 * timestamp, as librtmp2's reader expects.
 */
func encodeRtmpChunks(
    chunkStreamId: Int,
    messageStreamId: Int,
    typeId: Int,
    timestamp: Int64,
    payload: Data,
    chunkSize: Int
) -> Data {
    precondition((2...63).contains(chunkStreamId), "Chunk stream id \(chunkStreamId) needs a longer basic header")
    let time = UInt32(truncatingIfNeeded: timestamp)
    let extended = time >= extendedTimestamp
    var out = Data(capacity: payload.count + 16 + payload.count / chunkSize * 5)
    out.append(UInt8(chunkStreamId))
    out.appendUInt24(extended ? extendedTimestamp : time)
    out.appendUInt24(UInt32(payload.count))
    out.append(UInt8(typeId))
    // The message stream id is the one little-endian field in the header.
    let streamId = UInt32(truncatingIfNeeded: messageStreamId)
    out.append(contentsOf: [UInt8(streamId & 0xFF), UInt8(streamId >> 8 & 0xFF), UInt8(streamId >> 16 & 0xFF), UInt8(streamId >> 24)])
    if extended { out.appendUInt32(time) }
    var offset = payload.startIndex
    while true {
        let end = min(offset + chunkSize, payload.endIndex)
        out.append(payload[offset..<end])
        offset = end
        if offset >= payload.endIndex { break }
        out.append(0xC0 | UInt8(chunkStreamId))
        if extended { out.appendUInt32(time) }
    }
    return out
}

/// Blocking reads from a socket.
final class SocketInput: ByteInput {
    private let socket: Int32
    private var buffer = [UInt8](repeating: 0, count: 64 * 1024)
    private var start = 0
    private var end = 0

    init(socket: Int32) {
        self.socket = socket
    }

    func readByte() throws -> UInt8 {
        try fill()
        defer { start += 1 }
        return buffer[start]
    }

    func read(_ count: Int) throws -> Data {
        var out = Data(capacity: count)
        while out.count < count {
            try fill()
            let take = min(count - out.count, end - start)
            out.append(contentsOf: buffer[start..<start + take])
            start += take
        }
        return out
    }

    private func fill() throws {
        if start < end { return }
        while true {
            let count = buffer.withUnsafeMutableBytes { Darwin.recv(socket, $0.baseAddress!, $0.count, 0) }
            if count > 0 {
                start = 0
                end = count
                return
            }
            if count < 0 && errno == EINTR { continue }
            throw RtmpError(message: count == 0 ? "the local receiver closed the connection" : "reading from the local receiver failed (\(errno))")
        }
    }
}

protocol ByteInput: AnyObject {
    func readByte() throws -> UInt8
    func read(_ count: Int) throws -> Data
}

/// Reassembles the server's chunked messages; only their type and payload matter here.
final class RtmpChunkReader {
    var chunkSize = defaultChunkSize
    private let input: ByteInput
    private var streams: [Int: ChunkStream] = [:]

    private final class ChunkStream {
        var length = 0
        var typeId = 0
        var extended = false
        var buffer: Data?
    }

    init(input: ByteInput) {
        self.input = input
    }

    func readMessage() throws -> RtmpMessage {
        while true {
            let first = try Int(input.readByte())
            let format = first >> 6
            var csid = first & 0x3F
            if csid == 0 {
                csid = try 64 + Int(input.readByte())
            } else if csid == 1 {
                csid = try 64 + Int(input.readByte()) + Int(input.readByte()) << 8
            }
            let stream = streams[csid] ?? ChunkStream()
            streams[csid] = stream
            if format <= 2 {
                let timestampField = try readUInt24()
                if format <= 1 {
                    stream.length = try Int(readUInt24())
                    stream.typeId = try Int(input.readByte())
                }
                if format == 0 { _ = try input.read(4) } // message stream id; unused here
                stream.extended = timestampField == extendedTimestamp
            }
            if stream.extended { _ = try input.read(4) }
            guard stream.length <= maxMessageBytes else { throw RtmpError(message: "RTMP message too large") }
            var buffer = stream.buffer ?? Data(capacity: stream.length)
            let count = min(chunkSize, stream.length - buffer.count)
            try buffer.append(input.read(count))
            if buffer.count >= stream.length {
                stream.buffer = nil
                return RtmpMessage(typeId: stream.typeId, payload: buffer)
            }
            stream.buffer = buffer
        }
    }

    private func readUInt24() throws -> UInt32 {
        let bytes = try input.read(3)
        return UInt32(bytes[bytes.startIndex]) << 16 | UInt32(bytes[bytes.startIndex + 1]) << 8 | UInt32(bytes[bytes.startIndex + 2])
    }
}

/// An AMF0 value; objects and ECMA arrays keep their properties in order.
indirect enum AmfValue: Equatable {
    case number(Double)
    case boolean(Bool)
    case string(String)
    case object([(String, AmfValue)])
    case ecmaArray([(String, AmfValue)])
    case array([AmfValue])
    case null

    var stringValue: String? {
        if case let .string(value) = self { value } else { nil }
    }

    var properties: [String: AmfValue] {
        switch self {
        case let .object(entries), let .ecmaArray(entries):
            Dictionary(entries, uniquingKeysWith: { first, _ in first })
        default:
            [:]
        }
    }

    static func == (lhs: AmfValue, rhs: AmfValue) -> Bool {
        switch (lhs, rhs) {
        case let (.number(a), .number(b)): a == b
        case let (.boolean(a), .boolean(b)): a == b
        case let (.string(a), .string(b)): a == b
        case let (.object(a), .object(b)), let (.ecmaArray(a), .ecmaArray(b)):
            a.count == b.count && zip(a, b).allSatisfy { $0.0 == $1.0 && $0.1 == $1.1 }
        case let (.array(a), .array(b)): a == b
        case (.null, .null): true
        default: false
        }
    }
}

/// The AMF0 subset RTMP commands and metadata use.
enum Amf0 {
    private static let number: UInt8 = 0x00
    private static let boolean: UInt8 = 0x01
    private static let string: UInt8 = 0x02
    private static let object: UInt8 = 0x03
    private static let null: UInt8 = 0x05
    private static let undefined: UInt8 = 0x06
    private static let ecmaArray: UInt8 = 0x08
    private static let objectEnd: UInt8 = 0x09
    private static let strictArray: UInt8 = 0x0A
    private static let date: UInt8 = 0x0B
    private static let longString: UInt8 = 0x0C

    static func encode(_ values: [AmfValue]) -> Data {
        var out = Data()
        values.forEach { write($0, to: &out) }
        return out
    }

    /// Decodes values in order, stopping at the first type it does not know.
    static func decode(_ data: Data) -> [AmfValue] {
        var reader = Reader(bytes: [UInt8](data))
        var values: [AmfValue] = []
        // An unknown marker or a truncated value ends decoding; what came before still counts.
        while reader.hasRemaining, let value = try? readValue(&reader) {
            values.append(value)
        }
        return values
    }

    private static func write(_ value: AmfValue, to out: inout Data) {
        switch value {
        case .null:
            out.append(null)
        case let .boolean(flag):
            out.append(boolean)
            out.append(flag ? 1 : 0)
        case let .number(number):
            out.append(Self.number)
            out.appendUInt32(UInt32(number.bitPattern >> 32))
            out.appendUInt32(UInt32(number.bitPattern & 0xFFFF_FFFF))
        case let .string(text):
            let bytes = Data(text.utf8)
            if bytes.count <= 0xFFFF {
                out.append(string)
                out.appendUInt16(UInt16(bytes.count))
            } else {
                out.append(longString)
                out.appendUInt32(UInt32(bytes.count))
            }
            out.append(bytes)
        case let .ecmaArray(entries):
            out.append(ecmaArray)
            out.appendUInt32(UInt32(entries.count))
            writeProperties(entries, to: &out)
        case let .object(entries):
            out.append(object)
            writeProperties(entries, to: &out)
        case let .array(items):
            out.append(strictArray)
            out.appendUInt32(UInt32(items.count))
            items.forEach { write($0, to: &out) }
        }
    }

    private static func writeProperties(_ entries: [(String, AmfValue)], to out: inout Data) {
        for (key, value) in entries {
            let name = Data(key.utf8)
            out.appendUInt16(UInt16(name.count))
            out.append(name)
            write(value, to: &out)
        }
        out.appendUInt16(0)
        out.append(objectEnd)
    }

    private struct Reader {
        let bytes: [UInt8]
        var position = 0

        var hasRemaining: Bool { position < bytes.count }

        mutating func take(_ count: Int) throws -> ArraySlice<UInt8> {
            guard count >= 0, position + count <= bytes.count else { throw RtmpError(message: "truncated AMF0") }
            defer { position += count }
            return bytes[position..<position + count]
        }

        mutating func uint(_ count: Int) throws -> UInt64 {
            try take(count).reduce(0) { $0 << 8 | UInt64($1) }
        }
    }

    private static func readValue(_ reader: inout Reader) throws -> AmfValue {
        let marker = try reader.take(1).first!
        switch marker {
        case number: return try .number(Double(bitPattern: reader.uint(8)))
        case boolean: return try .boolean(reader.take(1).first! != 0)
        case string: return try .string(readUtf8(&reader, Int(reader.uint(2))))
        case object: return try .object(readProperties(&reader))
        case null, undefined: return .null
        case ecmaArray:
            _ = try reader.uint(4)
            return try .ecmaArray(readProperties(&reader))
        case strictArray:
            let count = try Int(reader.uint(4))
            var items: [AmfValue] = []
            for _ in 0..<count { try items.append(readValue(&reader)) }
            return .array(items)
        case date:
            let time = try Double(bitPattern: reader.uint(8))
            _ = try reader.uint(2)
            return .number(time)
        case longString: return try .string(readUtf8(&reader, Int(reader.uint(4))))
        default: throw RtmpError(message: "unsupported AMF0 marker \(marker)")
        }
    }

    private static func readProperties(_ reader: inout Reader) throws -> [(String, AmfValue)] {
        var entries: [(String, AmfValue)] = []
        while true {
            let nameLength = try Int(reader.uint(2))
            if nameLength == 0, reader.position < reader.bytes.count, reader.bytes[reader.position] == objectEnd {
                reader.position += 1
                return entries
            }
            let name = try readUtf8(&reader, nameLength)
            try entries.append((name, readValue(&reader)))
        }
    }

    private static func readUtf8(_ reader: inout Reader, _ length: Int) throws -> String {
        try String(decoding: reader.take(length), as: UTF8.self)
    }
}

private func int32(_ value: Int) -> Data {
    var out = Data()
    out.appendUInt32(UInt32(value))
    return out
}

extension Data {
    mutating func appendUInt16(_ value: UInt16) {
        append(contentsOf: [UInt8(value >> 8), UInt8(value & 0xFF)])
    }

    mutating func appendUInt24(_ value: UInt32) {
        append(contentsOf: [UInt8(value >> 16 & 0xFF), UInt8(value >> 8 & 0xFF), UInt8(value & 0xFF)])
    }

    mutating func appendUInt32(_ value: UInt32) {
        append(contentsOf: [UInt8(value >> 24), UInt8(value >> 16 & 0xFF), UInt8(value >> 8 & 0xFF), UInt8(value & 0xFF)])
    }
}
