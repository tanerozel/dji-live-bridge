package com.djilivebridge.android

import java.net.Inet4Address
import java.net.NetworkInterface

enum class LanKind { WIFI, HOTSPOT, WIRED, OTHER }

data class LanAddress(val address: String, val kind: LanKind) {
    /** The address DJI Fly publishes to; the path matches the desktop app. */
    val publishUrl: String
        get() = "rtmp://$address:1935/drone"
}

/** The IPv4 address the RC 2 can reach, preferring Wi-Fi, then the phone's own hotspot. */
fun findLocalLanAddress(): LanAddress? = runCatching {
    val candidates = NetworkInterface.getNetworkInterfaces()?.toList().orEmpty()
        .filter { network -> network.isUp && !network.isLoopback }
        .flatMap { network ->
            network.inetAddresses.toList()
                .filterIsInstance<Inet4Address>()
                .filter { address -> !address.isLoopbackAddress && !address.isLinkLocalAddress }
                .mapNotNull { address -> address.hostAddress?.let { network.name to it } }
        }
    preferredLanAddress(candidates)
}.getOrNull()

/**
 * Picks the best `(interface name, IPv4 address)` candidate. Mobile data, VPN and 464XLAT
 * interfaces are skipped: the RC 2 can never reach them, so showing one would only mislead.
 */
internal fun preferredLanAddress(candidates: List<Pair<String, String>>): LanAddress? =
    candidates
        .mapNotNull { (name, address) -> lanKind(name)?.let { kind -> LanAddress(address, kind) } }
        .minByOrNull { it.kind.ordinal }

internal fun lanKind(interfaceName: String): LanKind? {
    val name = interfaceName.lowercase()
    return when {
        UNREACHABLE_PREFIXES.any(name::startsWith) -> null
        HOTSPOT_PREFIXES.any(name::startsWith) -> LanKind.HOTSPOT
        name.startsWith("wlan") -> LanKind.WIFI
        WIRED_PREFIXES.any(name::startsWith) -> LanKind.WIRED
        else -> LanKind.OTHER
    }
}

private val UNREACHABLE_PREFIXES = listOf(
    "rmnet", "rev_rmnet", "ccmni", "seth", "wwan", "pdp", "clat", "v4-", "tun", "ppp", "ipsec", "dummy",
)
private val HOTSPOT_PREFIXES = listOf("swlan", "softap", "ap")
private val WIRED_PREFIXES = listOf("eth", "rndis", "usb", "ncm")
