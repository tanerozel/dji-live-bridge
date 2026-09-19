use std::{
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::process::Command;

use crate::{
    config::{FitMode, OutputLayout},
    error::{BridgeError, BridgeResult},
    process::{ProcessSpec, ProcessSupervisor, RestartPolicy},
    state::StreamMetadata,
};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FfmpegCapabilities {
    pub ffmpeg_path: Option<String>,
    pub ffprobe_path: Option<String>,
    pub version: Option<String>,
    pub h264_videotoolbox: bool,
    /// VideoToolbox honours `-constant_bit_rate` (macOS 13+, not every GPU).
    pub h264_videotoolbox_cbr: bool,
    pub libx264: bool,
    pub libopus: bool,
    pub avfoundation: bool,
    pub aac: bool,
    pub afftdn: bool,
    pub compressor: bool,
    pub limiter: bool,
    pub amix: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeProductionSettings {
    pub layout: OutputLayout,
    pub fit_mode: FitMode,
    pub microphone: Option<String>,
    pub microphone_muted: bool,
    pub microphone_volume_db: f64,
    pub microphone_sync_ms: u32,
    pub noise_suppression: bool,
    pub compressor: bool,
    pub limiter: bool,
}

pub async fn capabilities() -> FfmpegCapabilities {
    let ffmpeg = locate("ffmpeg");
    let ffprobe = locate("ffprobe");
    let mut result = FfmpegCapabilities {
        ffmpeg_path: ffmpeg.as_ref().map(|path| path.display().to_string()),
        ffprobe_path: ffprobe.as_ref().map(|path| path.display().to_string()),
        ..FfmpegCapabilities::default()
    };
    let Some(ffmpeg) = ffmpeg else {
        return result;
    };
    if let Ok(output) = Command::new(&ffmpeg).arg("-version").output().await {
        result.version = String::from_utf8_lossy(&output.stdout)
            .lines()
            .next()
            .map(str::to_string);
    }
    if let Ok(output) = Command::new(&ffmpeg)
        .args(["-hide_banner", "-encoders"])
        .output()
        .await
    {
        let encoders = String::from_utf8_lossy(&output.stdout);
        result.h264_videotoolbox = encoders.contains("h264_videotoolbox");
        result.libx264 = encoders.contains("libx264");
        result.libopus = encoders.contains("libopus");
        result.aac = encoders.contains(" AAC ") || encoders.contains(" aac ");
    }
    if let Ok(output) = Command::new(&ffmpeg)
        .args(["-hide_banner", "-devices"])
        .output()
        .await
    {
        let devices = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        result.avfoundation = devices.contains("avfoundation");
    }
    if let Ok(output) = Command::new(&ffmpeg)
        .args(["-hide_banner", "-filters"])
        .output()
        .await
    {
        let filters = String::from_utf8_lossy(&output.stdout);
        result.afftdn = filters.contains(" afftdn ");
        result.compressor = filters.contains(" acompressor ");
        result.limiter = filters.contains(" alimiter ");
        result.amix = filters.contains(" amix ");
    }
    if result.h264_videotoolbox {
        result.h264_videotoolbox_cbr = Command::new(&ffmpeg)
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-f",
                "lavfi",
                "-i",
                "color=size=320x240:rate=30",
                "-frames:v",
                "3",
                "-c:v",
                "h264_videotoolbox",
                "-constant_bit_rate",
                "1",
                "-b:v",
                "1M",
                "-f",
                "null",
                "-",
            ])
            .output()
            .await
            .is_ok_and(|output| output.status.success());
    }
    result
}

/// Output frame rate. Instagram and TikTok ingest cap at 30 fps; DJI and phone
/// sources are often 60 fps, which only doubles encoder load.
const OUTPUT_FPS: u32 = 30;
/// Video bitrate for the single production encode (Mbps-level upload budget).
const VIDEO_BITRATE: &str = "6000k";
const VIDEO_BUFSIZE: &str = "12000k";
/// 2-second keyframe interval, required by Instagram/Facebook live ingest.
const KEYFRAME_INTERVAL: &str = "60";

/// Scale the drone picture onto the output canvas.
///
/// `Fit` keeps the whole picture and fills the empty area with a blurred,
/// darkened copy of the same video instead of black bars, so a 16:9 drone
/// shot in a 9:16 story still fills the screen. `Fill` crops to the canvas.
fn production_video_filter(layout: OutputLayout, fit_mode: FitMode) -> String {
    let (width, height) = match layout {
        OutputLayout::Landscape => (1920_u32, 1080_u32),
        OutputLayout::Portrait => (1080_u32, 1920_u32),
    };
    match fit_mode {
        FitMode::Fit => {
            // Blur a quarter-size copy; it is invisible under the blur and 16x cheaper.
            let (small_width, small_height) = (width / 4, height / 4);
            format!(
                "fps={OUTPUT_FPS},split=2[bgsrc][fgsrc];\
                 [bgsrc]scale={small_width}:{small_height}:force_original_aspect_ratio=increase,crop={small_width}:{small_height},boxblur=10:2,eq=brightness=-0.08:saturation=0.9,scale={width}:{height}:flags=bicubic[bg];\
                 [fgsrc]scale={width}:{height}:force_original_aspect_ratio=decrease:force_divisible_by=2:flags=lanczos[fg];\
                 [bg][fg]overlay=(W-w)/2:(H-h)/2:shortest=1,format=yuv420p"
            )
        }
        FitMode::Fill => format!(
            "fps={OUTPUT_FPS},scale={width}:{height}:force_original_aspect_ratio=increase:flags=lanczos,crop={width}:{height},format=yuv420p"
        ),
    }
}

fn production_encoder_args(encoder: &str, videotoolbox_cbr: bool) -> Vec<&'static str> {
    let mut args = vec!["-c:v"];
    if encoder == "h264_videotoolbox" {
        // Measured on 720p drone footage at 1080x1920 (VMAF): realtime/main
        // scored 81.4 and undershot to ~4 Mbps; quality-priority CBR/high
        // scored 82.5 at the full rate. Hardware encoding keeps CPU free, so
        // the RTSP reader never falls behind and MediaMTX never drops frames.
        args.extend([
            "h264_videotoolbox",
            "-prio_speed",
            "0",
            "-profile:v",
            "high",
        ]);
        if videotoolbox_cbr {
            args.extend(["-constant_bit_rate", "1"]);
        }
        args.extend(["-bf", "0"]);
    } else {
        args.extend([
            "libx264",
            "-preset",
            "veryfast",
            "-profile:v",
            "high",
            "-bf",
            "2",
            "-sc_threshold",
            "0",
            "-x264-params",
            "nal-hrd=cbr",
        ]);
    }
    args.extend([
        "-pix_fmt",
        "yuv420p",
        "-b:v",
        VIDEO_BITRATE,
        "-maxrate",
        VIDEO_BITRATE,
        "-bufsize",
        VIDEO_BUFSIZE,
        "-g",
        KEYFRAME_INTERVAL,
        "-keyint_min",
        KEYFRAME_INTERVAL,
    ]);
    args
}

pub async fn start_production(
    supervisor: &ProcessSupervisor,
    settings: &NativeProductionSettings,
    has_drone_audio: bool,
) -> BridgeResult<String> {
    if !(-60.0..=12.0).contains(&settings.microphone_volume_db) {
        return Err(BridgeError::Validation(
            "Microphone volume must be between -60 dB and +12 dB".into(),
        ));
    }
    let capabilities = capabilities().await;
    let ffmpeg = capabilities
        .ffmpeg_path
        .as_deref()
        .map(PathBuf::from)
        .ok_or_else(|| BridgeError::Ffmpeg("FFmpeg is unavailable".into()))?;
    if !capabilities.aac {
        return Err(BridgeError::Ffmpeg(
            "Installed FFmpeg does not provide the AAC encoder".into(),
        ));
    }
    if settings.microphone.is_some() && !capabilities.avfoundation {
        return Err(BridgeError::Ffmpeg(
            "Installed FFmpeg does not provide the AVFoundation input device".into(),
        ));
    }
    if settings.microphone.is_some()
        && (!capabilities.amix
            || (settings.noise_suppression && !capabilities.afftdn)
            || (settings.compressor && !capabilities.compressor)
            || (settings.limiter && !capabilities.limiter))
    {
        return Err(BridgeError::Ffmpeg(
            "Installed FFmpeg is missing a selected commentary audio filter".into(),
        ));
    }
    let encoder = if capabilities.h264_videotoolbox {
        "h264_videotoolbox"
    } else if capabilities.libx264 {
        "libx264"
    } else {
        return Err(BridgeError::Ffmpeg(
            "No production H.264 encoder is available".into(),
        ));
    };

    let video_filter = production_video_filter(settings.layout, settings.fit_mode);

    let mut args: Vec<OsString> = [
        "-hide_banner",
        "-loglevel",
        "warning",
        // Headroom so a short CPU stall does not back up the RTSP reader.
        "-thread_queue_size",
        "4096",
        "-rtsp_transport",
        "tcp",
        "-i",
        "rtsp://127.0.0.1:8554/drone",
    ]
    .into_iter()
    .map(OsString::from)
    .collect();

    if let Some(microphone) = settings.microphone.as_deref() {
        args.extend(
            ["-thread_queue_size", "1024", "-f", "avfoundation", "-i"]
                .into_iter()
                .map(OsString::from),
        );
        args.push(OsString::from(format!(":{microphone}")));
    } else if !has_drone_audio {
        args.extend(
            [
                "-f",
                "lavfi",
                "-i",
                "anullsrc=channel_layout=stereo:sample_rate=48000",
            ]
            .into_iter()
            .map(OsString::from),
        );
    }

    args.extend(["-map", "0:v:0", "-vf"].into_iter().map(OsString::from));
    args.push(OsString::from(video_filter));

    if settings.microphone.is_some() {
        let mut mic_chain = "aresample=48000".to_string();
        if settings.noise_suppression {
            mic_chain.push_str(",afftdn=nr=12:nf=-50");
        }
        if settings.compressor {
            mic_chain.push_str(",acompressor=threshold=0.125:ratio=3:attack=20:release=250");
        }
        if settings.microphone_sync_ms > 0 {
            mic_chain.push_str(&format!(",adelay={0}|{0}", settings.microphone_sync_ms));
        }
        let volume = if settings.microphone_muted {
            -90.0
        } else {
            settings.microphone_volume_db
        };
        mic_chain.push_str(&format!(",volume={volume}dB"));
        if settings.limiter {
            mic_chain.push_str(",alimiter=limit=0.95");
        }
        let graph = if has_drone_audio {
            format!(
                "[0:a:0]aresample=48000[drone];[1:a:0]{mic_chain}[mic];[drone][mic]amix=inputs=2:duration=longest:dropout_transition=2[aout]"
            )
        } else {
            format!("[1:a:0]{mic_chain}[aout]")
        };
        args.extend(["-filter_complex"].into_iter().map(OsString::from));
        args.push(OsString::from(graph));
        args.extend(["-map", "[aout]"].into_iter().map(OsString::from));
    } else if has_drone_audio {
        args.extend(["-map", "0:a:0"].into_iter().map(OsString::from));
    } else {
        args.extend(["-map", "1:a:0"].into_iter().map(OsString::from));
    }

    args.extend(
        production_encoder_args(encoder, capabilities.h264_videotoolbox_cbr)
            .into_iter()
            .map(OsString::from),
    );
    args.extend(
        [
            "-c:a",
            "aac",
            "-b:a",
            "160k",
            "-ar",
            "48000",
            "-ac",
            "2",
            "-f",
            "rtsp",
            "-rtsp_transport",
            "tcp",
            "rtsp://127.0.0.1:8554/production",
        ]
        .into_iter()
        .map(OsString::from),
    );
    supervisor
        .start(ProcessSpec {
            name: "native-production".into(),
            executable: ffmpeg,
            args,
            restart_policy: RestartPolicy::Never,
        })
        .await?;
    Ok(encoder.to_string())
}

pub async fn start_test_drone(supervisor: &ProcessSupervisor, input: &Path) -> BridgeResult<()> {
    let ffmpeg = locate("ffmpeg").ok_or_else(|| {
        BridgeError::Ffmpeg("FFmpeg is not installed or not available on PATH".into())
    })?;
    let input = fs::canonicalize(input)
        .map_err(|error| BridgeError::Validation(format!("test video: {error}")))?;
    if !input.is_file() {
        return Err(BridgeError::Validation(
            "Test Drone input must be a regular video file".into(),
        ));
    }
    // Keep the file's own soundtrack; only synthesize silence for video-only files.
    let audio_args: &[&str] = if file_has_audio(&input).await {
        &["-map", "0:v:0", "-map", "0:a:0"]
    } else {
        &[
            "-f",
            "lavfi",
            "-i",
            "anullsrc=channel_layout=stereo:sample_rate=48000",
            "-map",
            "0:v:0",
            "-map",
            "1:a:0",
        ]
    };
    let args = [
        "-hide_banner",
        "-loglevel",
        "warning",
        "-re",
        "-stream_loop",
        "-1",
        "-i",
    ]
    .into_iter()
    .map(OsString::from)
    .chain([input.into_os_string()])
    .chain(audio_args.iter().map(OsString::from))
    .chain(
        [
            // Behave like the RC 2 (long side 1280, 30 fps) but keep the
            // clip's own orientation: padding a portrait clip into 1280x720
            // left a 405x720 strip that production then shrank even further.
            "-vf",
            "fps=30,scale='if(gte(iw,ih),min(1280,iw),-2)':'if(gte(iw,ih),-2,min(1280,ih))':flags=lanczos",
            "-c:v",
            "libx264",
            "-preset",
            "veryfast",
            "-tune",
            "zerolatency",
            "-pix_fmt",
            "yuv420p",
            "-profile:v",
            "high",
            "-b:v",
            "8000k",
            "-maxrate",
            "8000k",
            "-bufsize",
            "16000k",
            "-g",
            "30",
            "-bf",
            "0",
            "-c:a",
            "aac",
            "-b:a",
            "128k",
            "-ar",
            "48000",
            "-ac",
            "2",
            "-f",
            "flv",
            "rtmp://127.0.0.1:1935/drone",
        ]
        .into_iter()
        .map(OsString::from),
    )
    .collect();
    supervisor
        .start(ProcessSpec {
            name: "test-drone".into(),
            executable: ffmpeg,
            args,
            restart_policy: RestartPolicy::Never,
        })
        .await
}

pub async fn start_preview_fallback(supervisor: &ProcessSupervisor) -> BridgeResult<String> {
    let capabilities = capabilities().await;
    let ffmpeg = capabilities
        .ffmpeg_path
        .as_deref()
        .map(PathBuf::from)
        .ok_or_else(|| BridgeError::Ffmpeg("FFmpeg is unavailable".into()))?;
    if !capabilities.libopus {
        return Err(BridgeError::Ffmpeg(
            "Installed FFmpeg does not provide the libopus encoder".into(),
        ));
    }
    let encoder = if capabilities.h264_videotoolbox {
        "h264_videotoolbox"
    } else if capabilities.libx264 {
        "libx264"
    } else {
        return Err(BridgeError::Ffmpeg(
            "No browser-compatible H.264 encoder is available".into(),
        ));
    };

    let mut raw_args = vec![
        "-hide_banner",
        "-loglevel",
        "warning",
        "-rtsp_transport",
        "tcp",
        "-i",
        "rtsp://127.0.0.1:8554/drone",
        "-map",
        "0:v:0",
        "-map",
        "0:a:0?",
        "-c:v",
        encoder,
    ];
    if encoder == "h264_videotoolbox" {
        raw_args.extend(["-realtime", "1"]);
    } else {
        raw_args.extend(["-preset", "veryfast", "-tune", "zerolatency"]);
    }
    raw_args.extend([
        "-profile:v",
        "baseline",
        "-pix_fmt",
        "yuv420p",
        "-b:v",
        "3000k",
        "-maxrate",
        "3000k",
        "-bufsize",
        "1000k",
        "-g",
        "30",
        "-bf",
        "0",
        "-c:a",
        "libopus",
        "-b:a",
        "96k",
        "-ar",
        "48000",
        "-ac",
        "2",
        "-f",
        "rtsp",
        "-rtsp_transport",
        "tcp",
        "rtsp://127.0.0.1:8554/drone-preview",
    ]);
    supervisor
        .start(ProcessSpec {
            name: "preview-transcode".into(),
            executable: ffmpeg,
            args: raw_args.into_iter().map(OsString::from).collect(),
            restart_policy: RestartPolicy::OnFailure,
        })
        .await?;
    Ok(encoder.to_string())
}

pub async fn inspect_stream() -> BridgeResult<StreamMetadata> {
    let ffprobe =
        locate("ffprobe").ok_or_else(|| BridgeError::Ffmpeg("ffprobe is unavailable".into()))?;
    let child = Command::new(ffprobe)
        .args([
            "-v",
            "error",
            "-rtsp_transport",
            "tcp",
            "-show_entries",
            "stream=codec_type,codec_name,width,height,avg_frame_rate,bit_rate",
            "-of",
            "json",
            "rtsp://127.0.0.1:8554/drone",
        ])
        .stdin(Stdio::null())
        .output();
    let output = tokio::time::timeout(Duration::from_secs(5), child)
        .await
        .map_err(|_| BridgeError::Ffmpeg("ffprobe timed out".into()))??;
    if !output.status.success() {
        return Err(BridgeError::Ffmpeg(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }
    let root: Value = serde_json::from_slice(&output.stdout)?;
    let streams = root
        .get("streams")
        .and_then(Value::as_array)
        .ok_or_else(|| BridgeError::Ffmpeg("ffprobe returned no streams".into()))?;
    let video = streams
        .iter()
        .find(|stream| stream.get("codec_type").and_then(Value::as_str) == Some("video"));
    let audio = streams
        .iter()
        .find(|stream| stream.get("codec_type").and_then(Value::as_str) == Some("audio"));
    let resolution = video.and_then(|stream| {
        Some(format!(
            "{}x{}",
            stream.get("width")?.as_u64()?,
            stream.get("height")?.as_u64()?
        ))
    });
    Ok(StreamMetadata {
        resolution,
        fps: video
            .and_then(|stream| stream.get("avg_frame_rate"))
            .and_then(Value::as_str)
            .and_then(parse_fraction),
        video_codec: video
            .and_then(|stream| stream.get("codec_name"))
            .and_then(Value::as_str)
            .map(str::to_uppercase),
        audio_codec: audio
            .and_then(|stream| stream.get("codec_name"))
            .and_then(Value::as_str)
            .map(str::to_uppercase),
        video_bitrate_bps: video.and_then(parse_bitrate),
        audio_bitrate_bps: audio.and_then(parse_bitrate),
        ..StreamMetadata::default()
    })
}

async fn file_has_audio(input: &Path) -> bool {
    let Some(ffprobe) = locate("ffprobe") else {
        return false;
    };
    let child = Command::new(ffprobe)
        .args([
            "-v",
            "error",
            "-select_streams",
            "a",
            "-show_entries",
            "stream=index",
            "-of",
            "csv=p=0",
        ])
        .arg(input)
        .stdin(Stdio::null())
        .output();
    match tokio::time::timeout(Duration::from_secs(5), child).await {
        Ok(Ok(output)) => output.status.success() && !output.stdout.trim_ascii().is_empty(),
        _ => false,
    }
}

pub(crate) fn locate(name: &str) -> Option<PathBuf> {
    [
        PathBuf::from("/opt/homebrew/bin").join(name),
        PathBuf::from("/usr/local/bin").join(name),
        PathBuf::from("/usr/bin").join(name),
    ]
    .into_iter()
    .find(|candidate| candidate.is_file())
}

fn parse_fraction(value: &str) -> Option<f64> {
    let (numerator, denominator) = value.split_once('/')?;
    let denominator = denominator.parse::<f64>().ok()?;
    if denominator == 0.0 {
        return None;
    }
    Some(numerator.parse::<f64>().ok()? / denominator)
}

fn parse_bitrate(stream: &Value) -> Option<u64> {
    stream
        .get("bit_rate")
        .and_then(Value::as_str)
        .and_then(|value| value.parse().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn portrait_fit_uses_blurred_background_not_black_bars() {
        let filter = production_video_filter(OutputLayout::Portrait, FitMode::Fit);
        assert!(filter.starts_with("fps=30,"));
        assert!(filter.contains("boxblur"));
        assert!(filter.contains("scale=1080:1920:force_original_aspect_ratio=decrease"));
        assert!(!filter.contains("pad="));
    }

    #[test]
    fn fill_crops_to_the_canvas() {
        let filter = production_video_filter(OutputLayout::Landscape, FitMode::Fill);
        assert!(filter.contains("crop=1920:1080"));
        assert!(filter.contains("flags=lanczos"));
    }

    #[test]
    fn encoders_use_constant_rate_and_two_second_keyframes() {
        for (encoder, cbr) in [("h264_videotoolbox", true), ("libx264", false)] {
            let args = production_encoder_args(encoder, cbr);
            let value = |flag: &str| args[args.iter().position(|arg| *arg == flag).unwrap() + 1];
            assert_eq!(value("-b:v"), value("-maxrate"));
            assert_eq!(value("-g"), "60");
            assert_eq!(value("-profile:v"), "high");
        }
        assert!(production_encoder_args("h264_videotoolbox", true).contains(&"-constant_bit_rate"));
        assert!(
            !production_encoder_args("h264_videotoolbox", false).contains(&"-constant_bit_rate")
        );
    }
}
