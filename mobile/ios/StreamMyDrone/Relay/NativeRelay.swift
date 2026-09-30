import Foundation
import RelayCore

/// The Rust relay (mobile/relay-core, `ffi.rs`). Every call returns quickly except `stop` and
/// `endLive`, which flush the platform connections for up to two seconds.
enum NativeRelay {
    /// Starts receiving DJI Fly with no platform attached; an error code or nil.
    static func startReceiver() -> String? {
        take(djr_start_receiver())
    }

    /// Sends the received stream to one more platform, known by `outputId` (the destination
    /// profile id); an error code or nil.
    static func goLive(outputId: String, serverUrl: String, streamKey: String) -> String? {
        take(djr_go_live(outputId, serverUrl, streamKey))
    }

    /// Stops sending to the platform `outputId`, or to all of them when it is nil; the drone
    /// stays connected.
    static func endLive(outputId: String?) {
        djr_end_live(outputId ?? "")
    }

    static func snapshotJson() -> Data {
        let json = djr_snapshot_json()
        defer { djr_string_free(json) }
        return Data(bytes: json, count: strlen(json))
    }

    static func stop() {
        djr_stop()
    }

    /// Opens a preview session on the relay's video; returns its number.
    static func previewStart() -> UInt64 {
        djr_preview_start()
    }

    /// The next video tag of `session` within `timeoutMs`: its RTMP timestamp and the FLV video
    /// tag body, or nil when nothing arrived or the session ended.
    static func previewNext(session: UInt64, timeoutMs: UInt32) -> (timestamp: UInt32, body: Data)? {
        var frame = DjrPreviewFrame(timestamp: 0, data: nil, length: 0)
        guard djr_preview_next(session, timeoutMs, &frame), let data = frame.data else { return nil }
        let length = frame.length
        let body = Data(bytesNoCopy: data, count: length, deallocator: .custom { pointer, _ in
            djr_bytes_free(pointer.assumingMemoryBound(to: UInt8.self), length)
        })
        return (frame.timestamp, body)
    }

    static func previewStop(session: UInt64) {
        djr_preview_stop(session)
    }

    private static func take(_ value: UnsafeMutablePointer<CChar>?) -> String? {
        guard let value else { return nil }
        defer { djr_string_free(value) }
        return String(cString: value)
    }
}
