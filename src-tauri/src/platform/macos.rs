use std::{path::Path, process::Command};

use crate::error::{BridgeError, BridgeResult};

const OBS_APP: &str = "/Applications/OBS.app";
const TIKTOK_LIVE_STUDIO_APP: &str = "/Applications/TikTok LIVE Studio.app";
const TIKTOK_LIVE_STUDIO_DOWNLOAD_URL: &str = "https://www.tiktok.com/studio/download";

pub fn obs_installed() -> bool {
    Path::new(OBS_APP).is_dir()
}

pub fn obs_running() -> bool {
    Command::new("/usr/bin/pgrep")
        .args(["-x", "OBS"])
        .status()
        .is_ok_and(|status| status.success())
}

pub fn open_obs() -> BridgeResult<()> {
    if !obs_installed() {
        return Err(BridgeError::Obs(
            "OBS is not installed in /Applications/OBS.app".into(),
        ));
    }
    let status = Command::new("/usr/bin/open").args(["-a", "OBS"]).status()?;
    if !status.success() {
        return Err(BridgeError::Obs("macOS could not open OBS".into()));
    }
    Ok(())
}

pub fn tiktok_live_studio_installed() -> bool {
    Path::new(TIKTOK_LIVE_STUDIO_APP).is_dir()
}

pub fn open_tiktok_live_studio_or_download() -> BridgeResult<()> {
    if !tiktok_live_studio_installed() {
        return open_url(TIKTOK_LIVE_STUDIO_DOWNLOAD_URL);
    }
    let status = Command::new("/usr/bin/open")
        .arg(TIKTOK_LIVE_STUDIO_APP)
        .status()?;
    if !status.success() {
        return Err(BridgeError::Process(
            "macOS could not open TikTok LIVE Studio".into(),
        ));
    }
    Ok(())
}

pub fn open_url(url: &str) -> BridgeResult<()> {
    let parsed =
        url::Url::parse(url).map_err(|_| BridgeError::Validation("invalid external URL".into()))?;
    if parsed.scheme() != "https" {
        return Err(BridgeError::Validation(
            "only HTTPS external URLs are allowed".into(),
        ));
    }
    let status = Command::new("/usr/bin/open").arg(url).status()?;
    if !status.success() {
        return Err(BridgeError::Process("macOS could not open the URL".into()));
    }
    Ok(())
}
