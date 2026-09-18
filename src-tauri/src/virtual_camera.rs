use std::{
    ffi::{CStr, OsString},
    path::{Path, PathBuf},
    process::Command,
    sync::OnceLock,
};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

use crate::{
    error::{BridgeError, BridgeResult},
    ffmpeg,
    process::{ProcessSpec, ProcessSupervisor, RestartPolicy},
    state::ServiceStatus,
};

static APP_HANDLE: OnceLock<AppHandle> = OnceLock::new();

#[cfg(target_os = "macos")]
extern "C" fn extension_event_callback(
    event: *const std::os::raw::c_char,
    message: *const std::os::raw::c_char,
) {
    let event = unsafe { CStr::from_ptr(event) }
        .to_string_lossy()
        .into_owned();
    let message = unsafe { CStr::from_ptr(message) }
        .to_string_lossy()
        .into_owned();
    if let Some(app) = APP_HANDLE.get() {
        let _ = app.emit(
            "virtual-camera-extension-event",
            serde_json::json!({ "event": event, "message": message }),
        );
    }
}

pub const DEVICE_NAME: &str = "DJI Live Bridge Camera";
pub const EXTENSION_IDENTIFIER: &str = "com.djilivebridge.desktop.camera";
const EXTENSION_BUNDLE_NAME: &str = "com.djilivebridge.desktop.camera.systemextension";
pub const FEED_PROCESS_NAME: &str = "virtual-camera-feed";
const FEED_WIDTH: u32 = 1080;
const FEED_HEIGHT: u32 = 1920;
const FEED_FPS: u32 = 30;
const FEED_PORT: u16 = 49213;

#[derive(Debug, Clone, Serialize, Deserialize)]
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
            detail: Some("The signed camera extension is not installed".into()),
        }
    }
}

pub fn inspect(feed_active: bool) -> VirtualCameraState {
    let bundle_path = bundled_extension_path();
    let bundled = bundle_path.as_ref().is_some_and(|path| path.is_dir());
    let bundled_build_version = bundle_path
        .as_deref()
        .and_then(extension_bundle_build_version);
    let app_installed = current_app_path()
        .as_ref()
        .is_some_and(|path| path.starts_with("/Applications/"));
    let system_status = Command::new("/usr/bin/systemextensionsctl")
        .arg("list")
        .output()
        .ok()
        .map(|output| {
            format!(
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            )
        })
        .unwrap_or_default();
    let matching_lines: Vec<&str> = system_status
        .lines()
        .filter(|line| line.contains(EXTENSION_IDENTIFIER))
        .collect();
    let active_line = matching_lines.iter().find(|line| {
        line.contains("activated enabled")
            || (line.contains("[activated") && line.contains("enabled]"))
    });
    let active = active_line.is_some();
    let active_build_version = active_line.and_then(|line| system_extension_build_version(line));
    let update_available = active
        && bundled_build_version
            .as_deref()
            .is_some_and(|bundled_version| active_build_version != Some(bundled_version));
    let awaiting_approval = matching_lines
        .iter()
        .any(|line| line.contains("activated waiting for user"));

    let (status, detail) = if update_available {
        (
            ServiceStatus::Unavailable,
            Some(
                "A camera extension update is ready. Enable it to replace the active version."
                    .into(),
            ),
        )
    } else if active {
        (
            ServiceStatus::Ready,
            Some(format!("{DEVICE_NAME} is enabled by macOS")),
        )
    } else if awaiting_approval {
        (
            ServiceStatus::Starting,
            Some("Camera extension activation is waiting for macOS approval".into()),
        )
    } else if !bundled {
        (
            ServiceStatus::Unavailable,
            Some("This app bundle does not contain the camera extension".into()),
        )
    } else if !app_installed {
        (
            ServiceStatus::Unavailable,
            Some("Move DJI Live Bridge.app to /Applications before enabling the camera".into()),
        )
    } else {
        (
            ServiceStatus::Unavailable,
            Some("Camera extension is bundled but not enabled".into()),
        )
    };

    VirtualCameraState {
        device_name: DEVICE_NAME.into(),
        bundled,
        app_installed,
        status,
        feed_active,
        width: FEED_WIDTH,
        height: FEED_HEIGHT,
        fps: FEED_FPS,
        detail,
    }
}

pub fn request_activation() -> BridgeResult<()> {
    let state = inspect(false);
    if !state.bundled {
        return Err(BridgeError::VirtualCamera(
            "Camera extension is missing from the application bundle".into(),
        ));
    }
    if !state.app_installed {
        return Err(BridgeError::VirtualCamera(
            "DJI Live Bridge.app must run from /Applications before the camera extension can be enabled"
                .into(),
        ));
    }
    #[cfg(target_os = "macos")]
    unsafe {
        dji_request_camera_extension_activation();
    }
    Ok(())
}

pub async fn start_feed(supervisor: &ProcessSupervisor) -> BridgeResult<()> {
    let state = inspect(false);
    if state.status != ServiceStatus::Ready {
        return Err(BridgeError::VirtualCamera(
            state
                .detail
                .unwrap_or_else(|| "Camera extension is not enabled".into()),
        ));
    }
    let ffmpeg = ffmpeg::locate("ffmpeg")
        .ok_or_else(|| BridgeError::Ffmpeg("FFmpeg is unavailable".into()))?;
    let filter = format!(
        "fps={FEED_FPS},scale=w={FEED_WIDTH}:h={FEED_HEIGHT}:force_original_aspect_ratio=decrease,pad={FEED_WIDTH}:{FEED_HEIGHT}:(ow-iw)/2:(oh-ih)/2,format=nv12"
    );
    let output = format!("tcp://127.0.0.1:{FEED_PORT}?listen=1");
    let args = [
        "-hide_banner",
        "-loglevel",
        "warning",
        "-y",
        "-rtsp_transport",
        "tcp",
        "-i",
        "rtsp://127.0.0.1:8554/drone",
        "-map",
        "0:v:0",
        "-an",
        "-vf",
        &filter,
        "-pix_fmt",
        "nv12",
        "-f",
        "rawvideo",
    ]
    .into_iter()
    .map(OsString::from)
    .chain([OsString::from(output)])
    .collect();
    supervisor
        .start(ProcessSpec {
            name: FEED_PROCESS_NAME.into(),
            executable: ffmpeg,
            args,
            restart_policy: RestartPolicy::OnFailure,
        })
        .await
}

pub async fn stop_feed(supervisor: &ProcessSupervisor) -> BridgeResult<()> {
    supervisor.stop(FEED_PROCESS_NAME).await
}

fn current_app_path() -> Option<PathBuf> {
    let executable = std::env::current_exe().ok()?;
    executable
        .parent()?
        .parent()?
        .parent()
        .map(Path::to_path_buf)
}

fn bundled_extension_path() -> Option<PathBuf> {
    let app = current_app_path()?;
    Some(
        app.join("Contents/Library/SystemExtensions")
            .join(EXTENSION_BUNDLE_NAME),
    )
}

fn extension_bundle_build_version(bundle_path: &Path) -> Option<String> {
    let info_plist = bundle_path.join("Contents/Info.plist");
    let output = Command::new("/usr/bin/plutil")
        .args(["-extract", "CFBundleVersion", "raw", "-o", "-"])
        .arg(info_plist)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn system_extension_build_version(line: &str) -> Option<&str> {
    let (_, version_and_rest) = line.split_once('(')?;
    let (version, _) = version_and_rest.split_once(')')?;
    version.rsplit_once('/').map(|(_, build)| build)
}

#[cfg(target_os = "macos")]
unsafe extern "C" {
    fn dji_request_camera_extension_activation();
    fn dji_set_camera_extension_callback(
        callback: extern "C" fn(*const std::os::raw::c_char, *const std::os::raw::c_char),
    );
}

pub fn register_app_handle(app: AppHandle) {
    let _ = APP_HANDLE.set(app.clone());
    #[cfg(target_os = "macos")]
    unsafe {
        dji_set_camera_extension_callback(extension_event_callback);
    }
}
