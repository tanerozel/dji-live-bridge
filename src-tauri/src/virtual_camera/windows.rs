//! Windows virtual camera.
//!
//! The DirectShow filter in `native/windows/virtual_camera.cpp` is what apps
//! such as TikTok LIVE Studio see in their camera list. This module feeds it:
//! FFmpeg converts the drone stream to NV12 on stdout, and each frame is
//! published into shared memory that the filter reads.
//!
//! The filter is registered by the installer, so "activation" here is only a
//! check that the registration exists.

use std::{
    io::Read,
    process::{Child, Command, Stdio},
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
};

use crate::{
    error::{BridgeError, BridgeResult},
    ffmpeg,
    process::ProcessSupervisor,
    state::ServiceStatus,
};

use super::{DEVICE_NAME, FEED_FPS, FEED_HEIGHT, FEED_WIDTH, VirtualCameraState, frame_bridge};

/// Where the DirectShow filter registers itself; the CLSID matches
/// `CLSID_DjiLiveBridgeCamera` in the C++ source.
const FILTER_CLSID: &str = "{6F1D9A0C-6B2E-4F0B-9E2E-2C8B3F5A7D41}";

static FEED_RUNNING: AtomicBool = AtomicBool::new(false);

/// The running FFmpeg feed, so stopping actually stops it rather than waiting
/// for the pipe to close on its own.
fn feed_child() -> &'static Mutex<Option<Child>> {
    static CHILD: OnceLock<Mutex<Option<Child>>> = OnceLock::new();
    CHILD.get_or_init(|| Mutex::new(None))
}

/// The camera filter is registered when its CLSID is in the registry.
fn filter_registered() -> bool {
    crate::console::hide_std(&mut Command::new("reg"))
        .args(["query", &format!("HKCR\\CLSID\\{FILTER_CLSID}"), "/ve"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

pub fn inspect(feed_active: bool) -> VirtualCameraState {
    let registered = filter_registered();
    let (status, detail_key, detail) = if registered {
        (
            ServiceStatus::Ready,
            "camera.detail.windowsReady",
            Some(format!("{DEVICE_NAME} is registered with Windows")),
        )
    } else {
        (
            ServiceStatus::Unavailable,
            "camera.detail.windowsNotRegistered",
            Some(
                "The camera was not registered. Reinstall DJI Live Bridge with the installer, \
                 which registers it, or use OBS Virtual Camera instead."
                    .into(),
            ),
        )
    };

    VirtualCameraState {
        device_name: DEVICE_NAME.into(),
        bundled: registered,
        app_installed: true,
        status,
        feed_active,
        width: FEED_WIDTH,
        height: FEED_HEIGHT,
        fps: FEED_FPS,
        detail_key: detail_key.into(),
        detail,
    }
}

/// Windows registers the camera at install time, so there is nothing to ask
/// the user for — this only reports whether that registration is present.
pub fn request_activation() -> BridgeResult<()> {
    if filter_registered() {
        return Ok(());
    }
    Err(BridgeError::VirtualCamera(
        "The camera is registered by the installer. Run the DJI Live Bridge installer again, \
         then restart the app."
            .into(),
    ))
}

pub async fn start_feed(supervisor: &ProcessSupervisor) -> BridgeResult<()> {
    if !filter_registered() {
        return request_activation();
    }
    let capabilities = ffmpeg::capabilities().await;
    let ffmpeg_path = capabilities
        .ffmpeg_path
        .as_deref()
        .ok_or_else(|| BridgeError::Ffmpeg("FFmpeg is unavailable".into()))?;

    // Stop a previous feed before starting another writer.
    stop_feed(supervisor).await.ok();

    let mut child = crate::console::hide_std(&mut Command::new(ffmpeg_path))
        .args(ffmpeg::virtual_camera_feed_args())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| BridgeError::Ffmpeg(format!("camera feed: {error}")))?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| BridgeError::Ffmpeg("camera feed produced no output".into()))?;

    FEED_RUNNING.store(true, Ordering::Relaxed);
    *feed_child().lock().expect("feed lock") = Some(child);

    // A blocking reader: FFmpeg produces ~93 MB/s, so it gets its own thread.
    // It stops when stop_feed clears the flag and kills the child.
    std::thread::spawn(move || {
        if let Err(error) = frame_bridge::pump_frames(BufferedChild(stdout), &FEED_RUNNING) {
            tracing::warn!(%error, "virtual camera feed stopped");
        }
        FEED_RUNNING.store(false, Ordering::Relaxed);
        if let Some(mut child) = feed_child().lock().expect("feed lock").take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    });
    Ok(())
}

pub async fn stop_feed(_supervisor: &ProcessSupervisor) -> BridgeResult<()> {
    FEED_RUNNING.store(false, Ordering::Relaxed);
    // Killing our own child ends the pipe, which ends the reader thread. Only
    // this process's FFmpeg is touched, never another one on the machine.
    if let Some(mut child) = feed_child().lock().expect("feed lock").take() {
        let _ = child.kill();
        let _ = child.wait();
    }
    Ok(())
}

pub fn feed_running() -> bool {
    FEED_RUNNING.load(Ordering::Relaxed)
}

/// The macOS build keeps an app handle for system-extension callbacks; the
/// Windows camera needs no such channel.
pub fn register_app_handle(_app: tauri::AppHandle) {}

struct BufferedChild(std::process::ChildStdout);

impl Read for BufferedChild {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        self.0.read(buffer)
    }
}
