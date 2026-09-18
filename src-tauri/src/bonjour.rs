use std::{ffi::OsString, net::Ipv4Addr, path::PathBuf};

use crate::{
    error::{BridgeError, BridgeResult},
    process::{ProcessSpec, ProcessSupervisor, RestartPolicy},
};

pub const HOSTNAME: &str = "live.local";
pub const RTMP_URL: &str = "rtmp://live.local/drone";
const PROCESS_NAME: &str = "bonjour-live-local";

pub async fn advertise(supervisor: &ProcessSupervisor, ipv4: &str) -> BridgeResult<()> {
    let address = ipv4.parse::<Ipv4Addr>().map_err(|_| {
        BridgeError::Validation(format!(
            "Bonjour advertisement requires an IPv4 address: {ipv4}"
        ))
    })?;
    supervisor.start(process_spec(address)).await
}

pub async fn stop(supervisor: &ProcessSupervisor) -> BridgeResult<()> {
    supervisor.stop(PROCESS_NAME).await
}

fn process_spec(address: Ipv4Addr) -> ProcessSpec {
    ProcessSpec {
        name: PROCESS_NAME.into(),
        executable: PathBuf::from("/usr/bin/dns-sd"),
        args: [
            "-P",
            "DJI Live Bridge",
            "_rtmp._tcp",
            "local.",
            "1935",
            "live.local.",
            &address.to_string(),
            "path=/drone",
        ]
        .into_iter()
        .map(OsString::from)
        .collect(),
        restart_policy: RestartPolicy::OnFailure,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_proxy_registration_for_live_local() {
        let spec = process_spec(Ipv4Addr::new(172, 20, 10, 2));
        let args: Vec<_> = spec
            .args
            .iter()
            .map(|value| value.to_string_lossy().into_owned())
            .collect();
        assert_eq!(spec.name, PROCESS_NAME);
        assert_eq!(spec.executable, PathBuf::from("/usr/bin/dns-sd"));
        assert_eq!(
            args,
            [
                "-P",
                "DJI Live Bridge",
                "_rtmp._tcp",
                "local.",
                "1935",
                "live.local.",
                "172.20.10.2",
                "path=/drone",
            ]
        );
    }
}
