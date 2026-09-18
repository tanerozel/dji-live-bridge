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
    result
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

    let (width, height) = match settings.layout {
        OutputLayout::Landscape => (1920_u32, 1080_u32),
        OutputLayout::Portrait => (1080_u32, 1920_u32),
    };
    let video_filter = match settings.fit_mode {
        FitMode::Fit => format!(
            "scale=w={width}:h={height}:force_original_aspect_ratio=decrease,pad={width}:{height}:(ow-iw)/2:(oh-ih)/2"
        ),
        FitMode::Fill => format!(
            "scale=w={width}:h={height}:force_original_aspect_ratio=increase,crop={width}:{height}"
        ),
    };

    let mut args: Vec<OsString> = [
        "-hide_banner",
        "-loglevel",
        "warning",
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

    args.extend(["-c:v", encoder].into_iter().map(OsString::from));
    if encoder == "h264_videotoolbox" {
        args.extend(
            ["-realtime", "1", "-allow_sw", "1"]
                .into_iter()
                .map(OsString::from),
        );
    } else {
        args.extend(
            ["-preset", "veryfast", "-tune", "zerolatency"]
                .into_iter()
                .map(OsString::from),
        );
    }
    args.extend(
        [
            "-pix_fmt",
            "yuv420p",
            "-profile:v",
            "main",
            "-b:v",
            "5500k",
            "-maxrate",
            "6000k",
            "-bufsize",
            "3000k",
            "-g",
            "60",
            "-bf",
            "0",
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
    .chain(
        [
            "-f",
            "lavfi",
            "-i",
            "anullsrc=channel_layout=stereo:sample_rate=48000",
            "-map",
            "0:v:0",
            "-map",
            "1:a:0",
            "-vf",
            "scale=w=1280:h=720:force_original_aspect_ratio=decrease,pad=1280:720:(ow-iw)/2:(oh-ih)/2",
            "-c:v",
            "libx264",
            "-preset",
            "veryfast",
            "-tune",
            "zerolatency",
            "-pix_fmt",
            "yuv420p",
            "-profile:v",
            "main",
            "-level:v",
            "3.1",
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
