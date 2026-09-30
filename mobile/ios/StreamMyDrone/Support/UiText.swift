import Foundation

/**
 * Text that is put into words only where it is shown, so messages made by the relay controller
 * or the relay core appear in the language the app runs in, even after it changes. Arguments may
 * themselves be [UiText].
 */
indirect enum UiText: Hashable {
    case res(String, [UiArg])
    case plural(String, count: Int, [UiArg])
    /// Already in words, such as a technical detail from the operating system.
    case raw(String)
    /// Parts shown side by side, such as "drone.mp4 · 00:20 · looping".
    case joined([UiText], separator: String)

    static func tr(_ key: String, _ arguments: UiArg...) -> UiText { .res(key, arguments) }

    func resolve(_ localizer: Localizer = .shared) -> String {
        switch self {
        case let .res(key, arguments):
            return arguments.isEmpty
                ? localizer.string(key)
                : localizer.string(key, arguments.map { $0.resolved(localizer) })
        case let .plural(key, count, arguments):
            return localizer.plural(key, count: count, arguments.map { $0.resolved(localizer) })
        case let .raw(text):
            return text
        case let .joined(parts, separator):
            return parts.map { $0.resolve(localizer) }.joined(separator: separator)
        }
    }
}

enum UiArg: Hashable, ExpressibleByStringLiteral, ExpressibleByIntegerLiteral {
    case string(String)
    case int(Int)
    case text(UiText)

    init(stringLiteral value: String) { self = .string(value) }
    init(integerLiteral value: Int) { self = .int(value) }

    fileprivate func resolved(_ localizer: Localizer) -> CVarArg {
        switch self {
        case let .string(value): value
        case let .int(value): value
        case let .text(value): value.resolve(localizer)
        }
    }
}

extension UiText {
    /// A message followed by the technical cause of `error`, in brackets, when it has one.
    static func withCause(_ key: String, _ error: Error) -> UiText {
        let detail = error.localizedDescription
        return detail.isEmpty ? .tr(key) : .tr("error_with_detail", .text(.tr(key)), .string(detail))
    }
}

/**
 * A relay core error, sent as "code" or "code: technical detail", in the app's words; the
 * technical detail (English, from the system) follows in brackets.
 */
func nativeError(_ raw: String) -> UiText {
    let parts = raw.split(separator: ":", maxSplits: 1).map { $0.trimmingCharacters(in: .whitespaces) }
    let code = parts.first ?? raw
    let detail = parts.count > 1 ? parts[1] : ""
    let key: String
    switch code {
    case "receiver_not_running": key = "error_receiver_not_running"
    case "listen_failed": key = "error_listen_failed"
    case "server_create_failed", "receiver_failed": key = "error_receiver_failed"
    // iOS verifies RTMPS with its own trust store, so a missing CA bundle is an internal error.
    case "thread_failed", "state_lock_failed", "snapshot_failed", "ffi_argument", "missing_ca_bundle":
        key = "error_internal"
    case "invalid_destination_scheme": key = "error_destination_scheme"
    case "invalid_destination_path": key = "error_destination_path"
    case "invalid_destination_server": key = "error_destination_server"
    case "invalid_stream_key": key = "error_stream_key"
    default: return .raw(raw)
    }
    return detail.isEmpty ? .tr(key) : .tr("error_with_detail", .text(.tr(key)), .string(detail))
}
