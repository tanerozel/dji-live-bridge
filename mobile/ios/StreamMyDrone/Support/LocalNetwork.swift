import Darwin
import Foundation

enum LanKind: Int, Comparable {
    case wifi, hotspot, wired, other

    static func < (lhs: LanKind, rhs: LanKind) -> Bool { lhs.rawValue < rhs.rawValue }
}

struct LanAddress: Equatable {
    let address: String
    let kind: LanKind

    /// The address DJI Fly publishes to; the path matches the desktop app.
    var publishUrl: String { "rtmp://\(address):1935/drone" }
}

/// The IPv4 address the RC 2 can reach, preferring Wi-Fi, then the phone's own hotspot.
func findLocalLanAddress() -> LanAddress? {
    var first: UnsafeMutablePointer<ifaddrs>?
    guard getifaddrs(&first) == 0, let first else { return nil }
    defer { freeifaddrs(first) }
    var candidates: [(String, String)] = []
    for entry in sequence(first: first, next: { $0.pointee.ifa_next }) {
        let flags = Int32(entry.pointee.ifa_flags)
        guard let address = entry.pointee.ifa_addr,
              address.pointee.sa_family == UInt8(AF_INET),
              flags & IFF_UP != 0, flags & IFF_LOOPBACK == 0 else { continue }
        var host = [CChar](repeating: 0, count: Int(NI_MAXHOST))
        guard getnameinfo(address, socklen_t(address.pointee.sa_len), &host, socklen_t(host.count), nil, 0, NI_NUMERICHOST) == 0
        else { continue }
        let ip = String(cString: host)
        // Link-local addresses (no DHCP answer yet) are never reachable from the remote.
        if ip.hasPrefix("169.254.") { continue }
        candidates.append((String(cString: entry.pointee.ifa_name), ip))
    }
    return preferredLanAddress(candidates)
}

/**
 * Picks the best `(interface name, IPv4 address)` candidate. Mobile data, VPN and the phone's
 * private Apple links are skipped: the RC 2 can never reach them, so showing one would only
 * mislead.
 */
func preferredLanAddress(_ candidates: [(String, String)]) -> LanAddress? {
    candidates
        .compactMap { name, address in lanKind(name).map { LanAddress(address: address, kind: $0) } }
        .enumerated()
        .min { ($0.element.kind, $0.offset) < ($1.element.kind, $1.offset) }?
        .element
}

/// iOS calls Wi-Fi `en0`, the Personal Hotspot bridge `bridge100` and USB or Ethernet adapters
/// `en1` and up.
func lanKind(_ interfaceName: String) -> LanKind? {
    let name = interfaceName.lowercased()
    if unreachablePrefixes.contains(where: name.hasPrefix) { return nil }
    if name.hasPrefix("bridge") || name.hasPrefix("ap") { return .hotspot }
    if name == "en0" { return .wifi }
    if name.hasPrefix("en") { return .wired }
    return .other
}

private let unreachablePrefixes = [
    "lo", "pdp_ip", "utun", "ipsec", "ppp", "awdl", "llw", "anpi", "gif", "stf", "xhc", "rmnet", "tun",
]
