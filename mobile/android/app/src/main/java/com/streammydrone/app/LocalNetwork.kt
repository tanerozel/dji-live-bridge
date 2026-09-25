package com.streammydrone.app

import android.content.Context
import android.net.wifi.WifiManager
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

/** The phone's own connection to a Wi-Fi network, for the technical details. */
data class WifiLink(val rssiDbm: Int, val frequencyMhz: Int, val linkSpeedMbps: Int)

/**
 * The phone's Wi-Fi connection, or null without one (the phone may be the hotspot instead).
 * The deprecated call still reports signal, band and speed on every Android version the app
 * supports, and needs no location access for them.
 */
@Suppress("DEPRECATION")
fun currentWifiLink(context: Context): WifiLink? = runCatching {
    context.applicationContext.getSystemService(WifiManager::class.java)?.connectionInfo
        ?.takeIf { info -> info.rssi > NO_SIGNAL_DBM && info.frequency > 0 }
        ?.let { info -> WifiLink(info.rssi, info.frequency, info.linkSpeed) }
}.getOrNull()

/** What Android reports as the signal of a Wi-Fi that is not connected. */
private const val NO_SIGNAL_DBM = -127

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
