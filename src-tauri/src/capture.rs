//! HDMI capture cards as the drone source.
//!
//! A controller with HDMI out (or a phone through an HDMI adapter) feeds a
//! USB capture card, which the OS lists as a camera. One FFmpeg reads it and
//! publishes `/drone` exactly like the test video does, so preview,
//! production, the virtual camera, recording and AI Vision need no changes.

use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    time::Duration,
};

use serde::{Deserialize, Serialize};
use tokio::process::Command;

use crate::{
    error::{BridgeError, BridgeResult},
    ffmpeg,
    process::{ProcessSpec, ProcessSupervisor, RestartPolicy},
};

pub const PROCESS_NAME: &str = "capture-card";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoInputDevice {
    pub name: String,
}

/// Lists the video capture devices FFmpeg can open, by the names it has to be
/// given later. Our own virtual camera is left out: reading it back would
/// loop the production into itself.
pub async fn discover_video_devices() -> BridgeResult<Vec<VideoInputDevice>> {
    let ffmpeg = ffmpeg::locate("ffmpeg")
        .ok_or_else(|| BridgeError::Process("the bundled FFmpeg is missing".into()))?;
    let input = if cfg!(target_os = "windows") {
        "dummy"
    } else {
        ""
    };
    // Listing devices is not a conversion, so FFmpeg reports the list on
    // stderr and then exits with a failure; only the output matters.
    let output = crate::console::hide(&mut Command::new(ffmpeg))
        .args([
            "-hide_banner",
            "-list_devices",
            "true",
            "-f",
            ffmpeg::AUDIO_INPUT_FORMAT,
            "-i",
            input,
        ])
        .output()
        .await?;
    let mut names = parse_video_devices(&String::from_utf8_lossy(&output.stderr));
    names.retain(|name| name != crate::virtual_camera::DEVICE_NAME);
    names.dedup();
    Ok(names
        .into_iter()
        .map(|name| VideoInputDevice { name })
        .collect())
}

/// avfoundation lists `[index] name` lines under "AVFoundation video
/// devices:", followed by the audio devices. Screen capture shows up as a
/// video device too and is skipped: it is not a source.
#[cfg(target_os = "macos")]
fn parse_video_devices(text: &str) -> Vec<String> {
    text.lines()
        .skip_while(|line| !line.contains("AVFoundation video devices:"))
        .skip(1)
        .take_while(|line| !line.contains("AVFoundation audio devices:"))
        .filter_map(|line| {
            let (_, entry) = line.split_once("] [")?;
            let (index, name) = entry.split_once("] ")?;
            index.parse::<u32>().ok()?;
            let name = name.trim();
            (!name.is_empty() && !name.starts_with("Capture screen")).then(|| name.to_string())
        })
        .collect()
}

/// dshow names one device per line as `"<name>" (video)`.
#[cfg(target_os = "windows")]
fn parse_video_devices(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| {
            let quoted = line.trim_end().strip_suffix("(video)")?.trim_end();
            let name = quoted[quoted.find('"')? + 1..].strip_suffix('"')?;
            (!name.is_empty()).then(|| name.to_string())
        })
        .collect()
}

/// One format a device offers, as avfoundation reports it.
#[derive(Debug, Clone, Copy, PartialEq)]
struct CaptureMode {
    width: u32,
    height: u32,
    max_fps: f64,
}

/// Reads avfoundation's `WxH@[min max]fps` lines.
fn parse_modes(text: &str) -> Vec<CaptureMode> {
    text.lines()
        .filter_map(|line| {
            let (size, rates) = line.trim_end().strip_suffix("]fps")?.rsplit_once("@[")?;
            let (width, height) = size.split_whitespace().last()?.split_once('x')?;
            Some(CaptureMode {
                width: width.parse().ok()?,
                height: height.parse().ok()?,
                max_fps: rates.split_whitespace().last()?.parse().ok()?,
            })
        })
        .collect()
}

/// avfoundation only opens a size and rate the device lists, and its default
/// (29.97 fps) is missing on many cards. Take the largest picture that fits
/// 1080p, landscape first, at the lowest rate that still reaches 30 fps:
/// production runs at 30, so a 60 fps capture would only cost CPU.
fn choose_mode(modes: &[CaptureMode]) -> Option<CaptureMode> {
    let fits = |mode: &CaptureMode| {
        mode.width.max(mode.height) <= 1920 && mode.width.min(mode.height) <= 1080
    };
    let reaches_30 = |mode: &CaptureMode| mode.max_fps >= 29.9;
    let best = modes
        .iter()
        .filter(|mode| fits(mode))
        .max_by(|left, right| {
            let area = |mode: &CaptureMode| u64::from(mode.width) * u64::from(mode.height);
            let rate = |mode: &CaptureMode| {
                if reaches_30(mode) {
                    -mode.max_fps
                } else {
                    mode.max_fps
                }
            };
            area(left)
                .cmp(&area(right))
                .then((left.width >= left.height).cmp(&(right.width >= right.height)))
                .then(reaches_30(left).cmp(&reaches_30(right)))
                .then(rate(left).total_cmp(&rate(right)))
        });
    // A 4K-only card: take its smallest picture and scale it down.
    best.or_else(|| {
        modes
            .iter()
            .min_by_key(|mode| u64::from(mode.width) * u64::from(mode.height))
    })
    .copied()
}

/// Asks the device for its formats by requesting a rate no device offers;
/// FFmpeg then lists the supported modes and gives up.
async fn probe_mode(ffmpeg: &Path, device: &str) -> BridgeResult<CaptureMode> {
    let probe = crate::console::hide(&mut Command::new(ffmpeg))
        .args([
            "-hide_banner",
            "-f",
            "avfoundation",
            "-framerate",
            "1",
            "-i",
            device,
            "-frames:v",
            "1",
            "-f",
            "null",
            "-",
        ])
        .kill_on_drop(true)
        .output();
    let output = tokio::time::timeout(Duration::from_secs(15), probe)
        .await
        .map_err(|_| BridgeError::Ffmpeg(format!("{device} did not answer")))??;
    let text = String::from_utf8_lossy(&output.stderr);
    choose_mode(&parse_modes(&text)).ok_or_else(|| {
        let last = text.lines().last().unwrap_or_default().trim();
        BridgeError::Ffmpeg(format!(
            "{device} could not be opened ({last}). Allow DJI Live Bridge in \
             System Settings → Privacy & Security → Camera."
        ))
    })
}

/// macOS passes the probed mode (avfoundation); Windows has none (dshow).
fn video_input_args(device: &str, mode: Option<CaptureMode>) -> Vec<OsString> {
    let mut args: Vec<OsString> = vec!["-thread_queue_size".into(), "1024".into()];
    if let Some(mode) = mode {
        args.extend([
            "-f".into(),
            "avfoundation".into(),
            "-framerate".into(),
            format!("{:.3}", mode.max_fps).into(),
            "-video_size".into(),
            format!("{}x{}", mode.width, mode.height).into(),
            "-i".into(),
            // A bare name selects video only.
            device.into(),
        ]);
    } else {
        // DirectShow drops frames once its small default buffer fills.
        args.extend([
            "-f".into(),
            "dshow".into(),
            "-rtbufsize".into(),
            "256M".into(),
            "-i".into(),
            format!("video={device}").into(),
        ]);
    }
    args
}

/// Capture cards bring their own audio device; without one the stream
/// carries silence so every platform still gets an audio track.
fn audio_input_args(audio: Option<&str>) -> Vec<OsString> {
    match audio {
        Some(audio) => vec![
            "-thread_queue_size".into(),
            "1024".into(),
            "-f".into(),
            ffmpeg::AUDIO_INPUT_FORMAT.into(),
            "-i".into(),
            ffmpeg::audio_input_argument(audio).into(),
        ],
        None => vec![
            "-f".into(),
            "lavfi".into(),
            "-i".into(),
            "anullsrc=channel_layout=stereo:sample_rate=48000".into(),
        ],
    }
}

pub async fn start(
    supervisor: &ProcessSupervisor,
    device: &str,
    audio: Option<&str>,
) -> BridgeResult<()> {
    if device.trim().is_empty() {
        return Err(BridgeError::Validation("Choose a capture device".into()));
    }
    let capabilities = ffmpeg::capabilities().await;
    let ffmpeg = capabilities
        .ffmpeg_path
        .as_deref()
        .map(PathBuf::from)
        .ok_or_else(|| BridgeError::Ffmpeg("FFmpeg is unavailable".into()))?;
    let encoder = ffmpeg::select_h264_encoder(&capabilities).ok_or_else(|| {
        BridgeError::Ffmpeg("No H.264 encoder is available for the capture card".into())
    })?;
    let mode = if cfg!(target_os = "macos") {
        Some(probe_mode(&ffmpeg, device).await?)
    } else {
        None
    };
    let args = ["-hide_banner", "-loglevel", "warning"]
        .into_iter()
        .map(OsString::from)
        .chain(video_input_args(device, mode))
        .chain(audio_input_args(audio))
        .chain(["-map", "0:v:0", "-map", "1:a:0"].map(OsString::from))
        .chain(ffmpeg::drone_publish_args(encoder, 1920))
        .collect();
    supervisor
        .start(ProcessSpec {
            name: PROCESS_NAME.into(),
            executable: ffmpeg,
            args,
            restart_policy: RestartPolicy::Never,
        })
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Real output from the bundled FFmpeg on a MacBook Pro.
    const MODES: &str =
        "[in#0 @ 0xb6d00c000] Selected framerate (1.000000) is not supported by the device.
[in#0 @ 0xb6d00c000] Supported modes:
[in#0 @ 0xb6d00c000]   1920x1080@[15.000000 30.000000]fps
[in#0 @ 0xb6d00c000]   1280x720@[15.000000 30.000000]fps
[in#0 @ 0xb6d00c000]   1080x1920@[15.000000 30.000000]fps
[in#0 @ 0xb6d00c000]   640x480@[15.000000 30.000000]fps
Error opening input file FaceTime HD Camera.
";

    fn mode(width: u32, height: u32, max_fps: f64) -> CaptureMode {
        CaptureMode {
            width,
            height,
            max_fps,
        }
    }

    #[test]
    fn reads_the_supported_modes() {
        let modes = parse_modes(MODES);
        assert_eq!(modes.len(), 4);
        assert_eq!(modes[0], mode(1920, 1080, 30.0));
        assert_eq!(modes[2], mode(1080, 1920, 30.0));
    }

    #[test]
    fn prefers_landscape_1080p() {
        assert_eq!(
            choose_mode(&parse_modes(MODES)),
            Some(mode(1920, 1080, 30.0))
        );
    }

    #[test]
    fn prefers_30_fps_over_60_and_over_25() {
        let modes = [
            mode(1920, 1080, 60.0),
            mode(1920, 1080, 30.0),
            mode(1920, 1080, 25.0),
        ];
        assert_eq!(choose_mode(&modes), Some(mode(1920, 1080, 30.0)));
        let modes = [mode(1920, 1080, 60.0), mode(1920, 1080, 25.0)];
        assert_eq!(choose_mode(&modes), Some(mode(1920, 1080, 60.0)));
        let modes = [mode(1920, 1080, 24.0), mode(1920, 1080, 25.0)];
        assert_eq!(choose_mode(&modes), Some(mode(1920, 1080, 25.0)));
    }

    #[test]
    fn a_4k_only_card_uses_its_smallest_mode() {
        let modes = [mode(4096, 2160, 30.0), mode(3840, 2160, 30.0)];
        assert_eq!(choose_mode(&modes), Some(mode(3840, 2160, 30.0)));
    }

    #[test]
    fn nothing_to_choose_without_modes() {
        assert_eq!(
            choose_mode(&parse_modes("Error opening input file x.\n")),
            None
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn lists_cameras_without_screens_or_audio() {
        let listing = "[AVFoundation indev @ 0x9ff008140] AVFoundation video devices:
[AVFoundation indev @ 0x9ff008140] [0] DJI Live Bridge Camera
[AVFoundation indev @ 0x9ff008140] [1] Cam Link 4K
[AVFoundation indev @ 0x9ff008140] [2] Capture screen 0
[AVFoundation indev @ 0x9ff008140] AVFoundation audio devices:
[AVFoundation indev @ 0x9ff008140] [0] MacBook Pro Microphone
Error opening input file .
";
        assert_eq!(
            parse_video_devices(listing),
            ["DJI Live Bridge Camera", "Cam Link 4K"]
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn lists_only_video_devices() {
        let listing = r#"[in#0 @ 0000024] "USB Video Aygıtı" (video)
[in#0 @ 0000024]   Alternative name "@device_pnp_\\?\usb#vid_174f"
[in#0 @ 0000024] "Mikrofon (Realtek(R) Audio)" (audio)
Error opening input file dummy.
"#;
        assert_eq!(parse_video_devices(listing), ["USB Video Aygıtı"]);
    }

    #[test]
    fn without_audio_the_stream_carries_silence() {
        let args = audio_input_args(None);
        assert!(
            args.iter()
                .any(|arg| arg.to_string_lossy().starts_with("anullsrc"))
        );
    }
}
