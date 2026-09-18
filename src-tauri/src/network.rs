use std::{collections::BTreeMap, net::Ipv4Addr, process::Command, str::FromStr};

use serde::{Deserialize, Serialize};

use crate::error::{BridgeError, BridgeResult};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkInterface {
    pub name: String,
    pub ipv4: String,
    pub is_default_route: bool,
    pub recommended: bool,
}

pub fn discover_interfaces() -> BridgeResult<Vec<NetworkInterface>> {
    let default_name = default_route_interface();
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

fn default_route_interface() -> Option<String> {
    let output = Command::new("/sbin/route")
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

fn is_excluded_interface(name: &str) -> bool {
    name == "lo0"
        || name.starts_with("utun")
        || name.starts_with("awdl")
        || name.starts_with("llw")
        || name.starts_with("gif")
        || name.starts_with("stf")
}

fn is_usable_lan_address(address: Ipv4Addr) -> bool {
    !address.is_loopback()
        && !address.is_link_local()
        && !address.is_unspecified()
        && !address.is_multicast()
}
