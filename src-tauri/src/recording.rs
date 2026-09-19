use std::{ffi::OsString, fs, path::PathBuf};

use crate::{
    config::APP_DIR_NAME,
    error::{BridgeError, BridgeResult},
    ffmpeg,
    process::{ProcessSpec, ProcessSupervisor, RestartPolicy},
};

pub fn default_recording_dir() -> BridgeResult<PathBuf> {
    // Movies on macOS, Videos on Windows.
    let directory = dirs::video_dir()
        .ok_or_else(|| BridgeError::Config("Video directory not found".into()))?
        .join(APP_DIR_NAME);
    fs::create_dir_all(&directory)?;
    Ok(directory)
}

pub async fn start_native_recording(
    supervisor: &ProcessSupervisor,
    production_ready: bool,
) -> BridgeResult<PathBuf> {
    let ffmpeg = ffmpeg::locate("ffmpeg")
        .ok_or_else(|| BridgeError::Ffmpeg("FFmpeg is unavailable".into()))?;
    let directory = default_recording_dir()?;
    let timestamp = time::OffsetDateTime::now_utc().unix_timestamp();
    let output = directory.join(format!("DJI-Live-{timestamp}.mkv"));
    let input = if production_ready {
        "rtsp://127.0.0.1:8554/production"
    } else {
        "rtsp://127.0.0.1:8554/drone"
    };
    let mut args = [
        "-hide_banner",
        "-loglevel",
        "warning",
        "-rtsp_transport",
        "tcp",
        "-i",
        input,
        "-map",
        "0",
        "-c",
        "copy",
        "-f",
        "matroska",
    ]
    .into_iter()
    .map(OsString::from)
    .collect::<Vec<_>>();
    args.push(output.as_os_str().to_os_string());
    supervisor
        .start(ProcessSpec {
            name: "native-recording".into(),
            executable: ffmpeg,
            args,
            restart_policy: RestartPolicy::Never,
        })
        .await?;
    Ok(output)
}

pub async fn stop_native_recording(supervisor: &ProcessSupervisor) -> BridgeResult<()> {
    supervisor.stop("native-recording").await
}
