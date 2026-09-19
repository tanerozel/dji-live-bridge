//! Windows virtual camera.
//!
//! Not implemented yet: Windows has no equivalent of a Core Media I/O
//! extension, so this needs its own DirectShow filter or Media Foundation
//! virtual camera, plus registration at install time. Until then the app
//! reports the feature as unavailable and points at OBS's virtual camera,
//! which the optional OBS integration can already drive.

use crate::{
    error::{BridgeError, BridgeResult},
    process::ProcessSupervisor,
    state::ServiceStatus,
};

use super::{DEVICE_NAME, FEED_FPS, FEED_HEIGHT, FEED_WIDTH, VirtualCameraState};

const UNAVAILABLE: &str = "The built-in virtual camera is macOS only for now. On Windows, use the optional OBS \
     integration and start OBS Virtual Camera instead.";

pub fn inspect(feed_active: bool) -> VirtualCameraState {
    VirtualCameraState {
        device_name: DEVICE_NAME.into(),
        bundled: false,
        app_installed: true,
        status: ServiceStatus::Unavailable,
        feed_active,
        width: FEED_WIDTH,
        height: FEED_HEIGHT,
        fps: FEED_FPS,
        detail_key: "camera.detail.windowsUseObs".into(),
        detail: Some(UNAVAILABLE.into()),
    }
}

pub fn request_activation() -> BridgeResult<()> {
    Err(BridgeError::VirtualCamera(UNAVAILABLE.into()))
}

pub async fn start_feed(_supervisor: &ProcessSupervisor) -> BridgeResult<()> {
    Err(BridgeError::VirtualCamera(UNAVAILABLE.into()))
}

pub async fn stop_feed(supervisor: &ProcessSupervisor) -> BridgeResult<()> {
    supervisor.stop(super::FEED_PROCESS_NAME).await
}

/// The macOS build keeps an app handle for system-extension callbacks; there
/// is nothing to register here yet.
pub fn register_app_handle(_app: tauri::AppHandle) {}
