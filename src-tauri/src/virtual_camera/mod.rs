//! The virtual camera that publishes the drone picture to apps like
//! TikTok LIVE Studio. The mechanism is per-platform: a Core Media I/O system
//! extension on macOS, and (not yet) a DirectShow/Media Foundation camera on
//! Windows. Everything above this module only sees `VirtualCameraState`.

use serde::{Deserialize, Serialize};

use crate::state::ServiceStatus;

#[cfg(target_os = "windows")]
mod frame_bridge;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "macos")]
pub use macos::*;
#[cfg(target_os = "windows")]
pub use windows::*;

pub const DEVICE_NAME: &str = "DJI Live Bridge Camera";
pub const FEED_PROCESS_NAME: &str = "virtual-camera-feed";
pub(crate) const FEED_WIDTH: u32 = 1080;
pub(crate) const FEED_HEIGHT: u32 = 1920;
pub(crate) const FEED_FPS: u32 = 30;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VirtualCameraState {
    pub device_name: String,
    pub bundled: bool,
    pub app_installed: bool,
    pub status: ServiceStatus,
    pub feed_active: bool,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub detail_key: String,
    pub detail: Option<String>,
}

impl Default for VirtualCameraState {
    fn default() -> Self {
        Self {
            device_name: DEVICE_NAME.into(),
            bundled: false,
            app_installed: false,
            status: ServiceStatus::Unavailable,
            feed_active: false,
            width: FEED_WIDTH,
            height: FEED_HEIGHT,
            fps: FEED_FPS,
            detail_key: "camera.detail.notInstalled".into(),
            detail: Some("The signed camera extension is not installed".into()),
        }
    }
}

/// Whether the camera is currently being fed. macOS runs the feed under the
/// process supervisor; Windows owns the child directly.
#[cfg(target_os = "macos")]
pub fn feed_active(processes: &[crate::process::ProcessSnapshot]) -> bool {
    processes.iter().any(|process| {
        process.name == FEED_PROCESS_NAME
            && matches!(
                process.status,
                crate::process::ProcessStatus::Starting
                    | crate::process::ProcessStatus::Running
                    | crate::process::ProcessStatus::BackingOff
            )
    })
}

#[cfg(target_os = "windows")]
pub fn feed_active(_processes: &[crate::process::ProcessSnapshot]) -> bool {
    windows::feed_running()
}
