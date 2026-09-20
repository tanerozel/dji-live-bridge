use std::{collections::BTreeMap, net::Ipv4Addr};

use serde::{Deserialize, Serialize};

use crate::error::BridgeResult;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkInterface {
    pub name: String,
    pub ipv4: String,
    pub is_default_route: bool,
    pub recommended: bool,
}

/// What each OS has to report: the LAN address of every usable interface, and
/// the name of the one the default route leaves through.
struct Discovery {
    addresses: BTreeMap<String, Ipv4Addr>,
    default_name: Option<String>,
}

pub fn discover_interfaces() -> BridgeResult<Vec<NetworkInterface>> {
    let Discovery {
        addresses,
        default_name,
    } = discover()?;

    let mut result: Vec<_> = addresses
        .into_iter()
        .map(|(name, address)| {
            let is_default_route = default_name.as_deref() == Some(name.as_str());
            NetworkInterface {
                name,
                ipv4: address.to_string(),
                is_default_route,
                recommended: is_default_route,
            }
        })
        .collect();
    result.sort_by_key(|interface| (!interface.recommended, interface.name.clone()));
    Ok(result)
}

pub fn select_interface<'a>(
    interfaces: &'a [NetworkInterface],
    preferred: Option<&str>,
) -> Option<&'a NetworkInterface> {
    preferred
        .and_then(|name| interfaces.iter().find(|candidate| candidate.name == name))
        .or_else(|| interfaces.iter().find(|candidate| candidate.recommended))
        .or_else(|| interfaces.first())
}

fn is_usable_lan_address(address: Ipv4Addr) -> bool {
    !address.is_loopback()
        && !address.is_link_local()
        && !address.is_unspecified()
        && !address.is_multicast()
}

#[cfg(target_os = "macos")]
fn discover() -> BridgeResult<Discovery> {
    use std::{process::Command, str::FromStr};

    use crate::error::BridgeError;

    let output = Command::new("/sbin/ifconfig")
        .arg("-a")
        .output()
        .map_err(|error| BridgeError::Network(error.to_string()))?;
    if !output.status.success() {
        return Err(BridgeError::Network(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }

    let text = String::from_utf8_lossy(&output.stdout);
    let mut current = String::new();
    let mut addresses = BTreeMap::<String, Ipv4Addr>::new();
    for line in text.lines() {
        if !line.starts_with([' ', '\t']) {
            current = line.split(':').next().unwrap_or_default().to_string();
            continue;
        }
        let trimmed = line.trim();
        if let Some(raw) = trimmed
            .strip_prefix("inet ")
            .and_then(|rest| rest.split_whitespace().next())
            && let Ok(address) = Ipv4Addr::from_str(raw)
            && is_usable_lan_address(address)
            && !is_excluded_interface(&current)
        {
            addresses.insert(current.clone(), address);
        }
    }

    Ok(Discovery {
        addresses,
        default_name: default_route_interface(),
    })
}

#[cfg(target_os = "macos")]
fn default_route_interface() -> Option<String> {
    let output = std::process::Command::new("/sbin/route")
        .args(["-n", "get", "default"])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    text.lines().find_map(|line| {
        line.trim()
            .strip_prefix("interface:")
            .map(str::trim)
            .map(str::to_string)
    })
}

#[cfg(target_os = "macos")]
fn is_excluded_interface(name: &str) -> bool {
    name == "lo0"
        || name.starts_with("utun")
        || name.starts_with("awdl")
        || name.starts_with("llw")
        || name.starts_with("gif")
        || name.starts_with("stf")
}

/// Windows has no `ifconfig`, and `ipconfig` output is localised, so the
/// adapters come from the IP Helper API instead. The default route leaves
/// through the connected adapter that has a gateway and the lowest metric,
/// which is how Windows itself picks it and what stops a VMware or Hyper-V
/// adapter from being recommended over the real Wi-Fi card.
#[cfg(target_os = "windows")]
fn discover() -> BridgeResult<Discovery> {
    use windows::Win32::{
        Foundation::{ERROR_BUFFER_OVERFLOW, ERROR_SUCCESS},
        NetworkManagement::IpHelper::{
            GAA_FLAG_INCLUDE_GATEWAYS, GAA_FLAG_SKIP_ANYCAST, GAA_FLAG_SKIP_DNS_SERVER,
            GAA_FLAG_SKIP_MULTICAST, GetAdaptersAddresses, IP_ADAPTER_ADDRESSES_LH,
        },
        Networking::WinSock::AF_INET,
    };

    use crate::error::BridgeError;

    // Gateways are left out unless they are asked for, and they are how the
    // default route is recognised below.
    let flags = GAA_FLAG_INCLUDE_GATEWAYS
        | GAA_FLAG_SKIP_ANYCAST
        | GAA_FLAG_SKIP_MULTICAST
        | GAA_FLAG_SKIP_DNS_SERVER;
    // A u64 buffer so the adapter structs land on an 8-byte boundary. 15 KB is
    // the starting size the Win32 documentation recommends.
    let mut size: u32 = 15 * 1024;
    // The adapter list can grow between the sizing call and the real one.
    for _ in 0..4 {
        let mut buffer = vec![0u64; size.div_ceil(8) as usize];
        let head = buffer.as_mut_ptr().cast::<IP_ADAPTER_ADDRESSES_LH>();
        let code =
            unsafe { GetAdaptersAddresses(AF_INET.0 as u32, flags, None, Some(head), &mut size) };
        if code == ERROR_SUCCESS.0 {
            // SAFETY: the call succeeded, so `head` is the start of a linked
            // list that lives in `buffer`, which outlives the walk.
            return Ok(unsafe { collect_adapters(head) });
        }
        if code != ERROR_BUFFER_OVERFLOW.0 {
            return Err(BridgeError::Network(format!(
                "GetAdaptersAddresses failed with code {code}"
            )));
        }
    }
    Err(BridgeError::Network(
        "the list of network adapters kept growing while it was read".into(),
    ))
}

/// # Safety
/// `head` must be a linked list of adapters as returned by `GetAdaptersAddresses`.
#[cfg(target_os = "windows")]
unsafe fn collect_adapters(
    head: *const windows::Win32::NetworkManagement::IpHelper::IP_ADAPTER_ADDRESSES_LH,
) -> Discovery {
    use windows::Win32::NetworkManagement::Ndis::IfOperStatusUp;

    let mut addresses = BTreeMap::<String, Ipv4Addr>::new();
    let mut default_name = None;
    let mut best_metric = u32::MAX;

    let mut adapter = head;
    while !adapter.is_null() {
        let entry = unsafe { &*adapter };
        adapter = entry.Next;
        if entry.OperStatus != IfOperStatusUp {
            continue;
        }
        let Ok(name) = (unsafe { entry.FriendlyName.to_string() }) else {
            continue;
        };
        let Some(address) = (unsafe { first_lan_address(entry) }) else {
            continue;
        };
        if !entry.FirstGatewayAddress.is_null() && entry.Ipv4Metric < best_metric {
            best_metric = entry.Ipv4Metric;
            default_name = Some(name.clone());
        }
        addresses.insert(name, address);
    }

    Discovery {
        addresses,
        default_name,
    }
}

/// # Safety
/// `entry` must come from a live `GetAdaptersAddresses` buffer.
#[cfg(target_os = "windows")]
unsafe fn first_lan_address(
    entry: &windows::Win32::NetworkManagement::IpHelper::IP_ADAPTER_ADDRESSES_LH,
) -> Option<Ipv4Addr> {
    use windows::Win32::Networking::WinSock::{AF_INET, SOCKADDR_IN};

    let mut unicast = entry.FirstUnicastAddress;
    while !unicast.is_null() {
        let current = unsafe { &*unicast };
        unicast = current.Next;
        let socket_address = current.Address.lpSockaddr;
        if socket_address.is_null() || unsafe { (*socket_address).sa_family } != AF_INET {
            continue;
        }
        let inet = unsafe { &*socket_address.cast::<SOCKADDR_IN>() };
        let address = Ipv4Addr::from(u32::from_be(unsafe { inet.sin_addr.S_un.S_addr }));
        if is_usable_lan_address(address) {
            return Some(address);
        }
    }
    None
}
