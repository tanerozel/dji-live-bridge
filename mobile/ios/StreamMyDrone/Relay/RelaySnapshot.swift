import Foundation

/// The relay core's state (`RelaySnapshot` in relay-core), polled twice a second.
struct RelaySnapshot: Equatable {
    var status = "stopped"
    /// Why the receiver stopped, in words for the screen; nil unless `status` is "error".
    var error: UiText?
    var remoteAddress: String?
    var receivedBytes: Int64 = 0
    var bitrateKbps: Double = 0
    var videoCodec: String?
    var audioCodec: String?
    var videoFrames: Int64 = 0
    var audioFrames: Int64 = 0
    var rejectedPublishAttempts: Int64 = 0
    /// Pauses of 0.7 s or more in the drone's video since the receiver started, and the longest.
    var stalls: Int64 = 0
    var longestStallMs: Int64 = 0
    /// Times DJI Fly started publishing again after its stream ended.
    var sourceReconnects: Int64 = 0
    /// Stalls and reconnects within the last minute.
    var recentInterruptions: Int64 = 0
    /// One entry per platform the stream is going to.
    var outputs: [OutputSnapshot] = []

    /// What the broadcast as a whole does: the best that any platform does.
    var outputStatus: String {
        outputStatusRank.first { status in outputs.contains { $0.status == status } } ?? "disabled"
    }

    var outboundBytes: Int64 { outputs.reduce(0) { $0 + $1.outboundBytes } }

    func output(_ id: String) -> OutputSnapshot? { outputs.first { $0.id == id } }

    static func fromJson(_ data: Data) throws -> RelaySnapshot {
        let raw = try JSONDecoder().decode(RawSnapshot.self, from: data)
        let error = raw.errorCode.flatMap { $0.isEmpty ? nil : $0 }.map { code in
            nativeError([code, raw.errorDetail].compactMap { $0 }.filter { !$0.isEmpty }.joined(separator: ": "))
        }
        return RelaySnapshot(
            status: raw.status ?? "error",
            error: error,
            remoteAddress: raw.remoteAddress.flatMap { $0.isEmpty ? nil : $0 },
            receivedBytes: raw.receivedBytes ?? 0,
            bitrateKbps: raw.bitrateKbps ?? 0,
            videoCodec: raw.videoCodec.flatMap { $0.isEmpty ? nil : $0 },
            audioCodec: raw.audioCodec.flatMap { $0.isEmpty ? nil : $0 },
            videoFrames: raw.videoFrames ?? 0,
            audioFrames: raw.audioFrames ?? 0,
            rejectedPublishAttempts: raw.rejectedPublishAttempts ?? 0,
            stalls: raw.stalls ?? 0,
            longestStallMs: raw.longestStallMs ?? 0,
            sourceReconnects: raw.sourceReconnects ?? 0,
            recentInterruptions: raw.recentInterruptions ?? 0,
            outputs: (raw.outputs ?? []).map { output in
                OutputSnapshot(
                    id: output.id ?? "",
                    status: output.status ?? "armed",
                    reason: output.reason,
                    reasonDetail: output.reasonDetail,
                    retryInSeconds: output.retryInSeconds,
                    outboundBytes: output.outboundBytes ?? 0,
                    droppedFrames: output.droppedFrames ?? 0,
                    reconnectAttempts: output.reconnectAttempts ?? 0,
                    secure: output.secure ?? false
                )
            }
        )
    }
}

/// One platform's output.
struct OutputSnapshot: Equatable {
    var id: String
    var status = "armed"
    /// While reconnecting: why, as a code, the technical cause, and the wait before retrying.
    var reason: String?
    var reasonDetail: String?
    var retryInSeconds: Int64?
    var outboundBytes: Int64 = 0
    var droppedFrames: Int64 = 0
    var reconnectAttempts: Int64 = 0
    var secure = false
}

/// Best first: a broadcast is live while any platform receives it.
private let outputStatusRank = [
    "forwarding", "congested", "ready", "connecting", "holding", "reconnecting", "armed", "error", "stopped",
]

/// The output reaches the platform; "congested" still does, with video frames skipped.
let liveOutputStatuses: Set<String> = ["forwarding", "congested"]

private struct RawSnapshot: Decodable {
    var status: String?
    var errorCode: String?
    var errorDetail: String?
    var remoteAddress: String?
    var receivedBytes: Int64?
    var bitrateKbps: Double?
    var videoCodec: String?
    var audioCodec: String?
    var videoFrames: Int64?
    var audioFrames: Int64?
    var rejectedPublishAttempts: Int64?
    var stalls: Int64?
    var longestStallMs: Int64?
    var sourceReconnects: Int64?
    var recentInterruptions: Int64?
    var outputs: [RawOutput]?
}

private struct RawOutput: Decodable {
    var id: String?
    var status: String?
    var reason: String?
    var reasonDetail: String?
    var retryInSeconds: Int64?
    var outboundBytes: Int64?
    var droppedFrames: Int64?
    var reconnectAttempts: Int64?
    var secure: Bool?
}
