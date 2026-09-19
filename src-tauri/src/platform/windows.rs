//! Windows counterparts of the macOS integrations.
//!
//! Only the pieces the app actually needs: finding and opening OBS and TikTok
//! LIVE Studio, and opening an external link. Everything else in the app is
//! platform independent.

use std::{path::PathBuf, process::Command};

use crate::error::{BridgeError, BridgeResult};

const TIKTOK_LIVE_STUDIO_DOWNLOAD_URL: &str = "https://www.tiktok.com/studio/download";

/// `cmd /C start` is how a GUI app hands a URL or a document to the shell.
fn shell_open(target: &str) -> BridgeResult<()> {
    let status = Command::new("cmd")
        .args(["/C", "start", "", target])
        .status()?;
    if !status.success() {
        return Err(BridgeError::Process(format!(
            "Windows could not open {target}"
        )));
    }
    Ok(())
}

fn program_files_candidates(relative: &str) -> Vec<PathBuf> {
    ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"]
        .into_iter()
        .filter_map(std::env::var_os)
        .map(|root| PathBuf::from(root).join(relative))
        .collect()
}

fn obs_executable() -> Option<PathBuf> {
    program_files_candidates("obs-studio\\bin\\64bit\\obs64.exe")
        .into_iter()
        .find(|path| path.is_file())
}

pub fn obs_installed() -> bool {
    obs_executable().is_some()
}

pub fn obs_running() -> bool {
    Command::new("tasklist")
        .args(["/FI", "IMAGENAME eq obs64.exe", "/NH"])
        .output()
        .is_ok_and(|output| String::from_utf8_lossy(&output.stdout).contains("obs64.exe"))
}

pub fn open_obs() -> BridgeResult<()> {
    let executable =
        obs_executable().ok_or_else(|| BridgeError::Obs("OBS Studio is not installed".into()))?;
    // OBS insists on being started from its own bin directory.
    let working_directory = executable
        .parent()
        .ok_or_else(|| BridgeError::Obs("OBS install path is malformed".into()))?;
    Command::new(&executable)
        .current_dir(working_directory)
        .spawn()
        .map_err(|error| BridgeError::Obs(format!("Windows could not start OBS: {error}")))?;
    Ok(())
}

fn tiktok_live_studio_executable() -> Option<PathBuf> {
    program_files_candidates("TikTok LIVE Studio\\TikTok LIVE Studio.exe")
        .into_iter()
        .find(|path| path.is_file())
}

pub fn tiktok_live_studio_installed() -> bool {
    tiktok_live_studio_executable().is_some()
}

pub fn open_tiktok_live_studio_or_download() -> BridgeResult<()> {
    match tiktok_live_studio_executable() {
        Some(executable) => {
            Command::new(&executable).spawn().map_err(|error| {
                BridgeError::Process(format!(
                    "Windows could not start TikTok LIVE Studio: {error}"
                ))
            })?;
            Ok(())
        }
        None => open_url(TIKTOK_LIVE_STUDIO_DOWNLOAD_URL),
    }
}

/// Opens an https:// or mailto: link from the fixed list in lib.rs.
pub fn open_external(url: &str) -> BridgeResult<()> {
    if let Some(address) = url.strip_prefix("mailto:") {
        if address.is_empty() || address.contains(['\n', '\r', ' ']) {
            return Err(BridgeError::Validation("invalid e-mail address".into()));
        }
        return shell_open(url);
    }
    open_url(url)
}

pub fn open_url(url: &str) -> BridgeResult<()> {
    let parsed =
        url::Url::parse(url).map_err(|_| BridgeError::Validation("invalid external URL".into()))?;
    if parsed.scheme() != "https" {
        return Err(BridgeError::Validation(
            "only HTTPS external URLs are allowed".into(),
        ));
    }
    shell_open(url)
}
