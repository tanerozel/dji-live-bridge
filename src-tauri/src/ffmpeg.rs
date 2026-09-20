use std::{
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::Emitter;
use tokio::io::AsyncBufReadExt;
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
    /// Windows hardware encoders, in the order they are preferred.
    pub h264_nvenc: bool,
    pub h264_qsv: bool,
    pub h264_amf: bool,
    /// VideoToolbox honours `-constant_bit_rate` (macOS 13+, not every GPU).
    pub h264_videotoolbox_cbr: bool,
    pub libx264: bool,
    pub libopus: bool,
    /// FFmpeg's own Opus encoder, used when libopus is absent.
    pub opus: bool,
    /// The OS audio capture input FFmpeg needs for the commentary microphone:
    /// avfoundation on macOS, dshow on Windows.
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
    if let Ok(output) = crate::console::hide(&mut Command::new(&ffmpeg))
        .arg("-version")
        .output()
        .await
    {
        result.version = String::from_utf8_lossy(&output.stdout)
            .lines()
            .next()
            .map(str::to_string);
    }
    if let Ok(output) = crate::console::hide(&mut Command::new(&ffmpeg))
        .args(["-hide_banner", "-encoders"])
        .output()
        .await
    {
        let encoders = String::from_utf8_lossy(&output.stdout);
        result.h264_videotoolbox = encoders.contains("h264_videotoolbox");
        result.h264_nvenc = encoders.contains("h264_nvenc");
        result.h264_qsv = encoders.contains("h264_qsv");
        result.h264_amf = encoders.contains("h264_amf");
        result.libx264 = encoders.contains("libx264");
        result.libopus = encoders.contains("libopus");
        result.opus = encoders.contains(" opus ");
        result.aac = encoders.contains(" AAC ") || encoders.contains(" aac ");
    }
    // Being listed only means the build contains the encoder, not that this
    // machine can open it: a PC with an NVIDIA driver too old for the runtime
    // lists h264_nvenc and then fails with "Cannot load cuMemAllocAsync", and
    // h264_amf is listed on machines with no AMD GPU at all. Selecting one of
    // those loses the stream, so each is tried for real before it counts.
    for (available, encoder) in [
        (&mut result.h264_nvenc, "h264_nvenc"),
        (&mut result.h264_qsv, "h264_qsv"),
        (&mut result.h264_amf, "h264_amf"),
    ] {
        if *available {
            *available = encodes_a_frame(&ffmpeg, encoder).await;
        }
    }
    if let Ok(output) = crate::console::hide(&mut Command::new(&ffmpeg))
        .args(["-hide_banner", "-devices"])
        .output()
        .await
    {
        let devices = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        result.avfoundation = devices.contains(AUDIO_INPUT_FORMAT);
    }
    if let Ok(output) = crate::console::hide(&mut Command::new(&ffmpeg))
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
        result.h264_videotoolbox_cbr = crate::console::hide(&mut Command::new(&ffmpeg))
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

/// Encodes three frames of a generated colour source. Cheap enough to run
/// while probing, and the only way to learn that a driver is really there.
async fn encodes_a_frame(ffmpeg: &Path, encoder: &str) -> bool {
    crate::console::hide(&mut Command::new(ffmpeg))
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
            encoder,
            "-f",
            "null",
            "-",
        ])
        .output()
        .await
        .is_ok_and(|output| output.status.success())
}

/// The filter that turns the drone stream into the virtual camera's frames.
/// Both platforms use the same geometry so the picture matches.
pub fn virtual_camera_filter(width: u32, height: u32, fps: u32) -> String {
    format!(
        "fps={fps},scale=w={width}:h={height}:force_original_aspect_ratio=decrease,\
         pad={width}:{height}:(ow-iw)/2:(oh-ih)/2,format=nv12"
    )
}

/// Windows feed: raw NV12 frames on stdout, which the app copies into the
/// shared buffer the DirectShow filter reads.
#[cfg(windows)]
pub fn virtual_camera_feed_args() -> Vec<String> {
    let filter = virtual_camera_filter(
        crate::virtual_camera::FEED_WIDTH,
        crate::virtual_camera::FEED_HEIGHT,
        crate::virtual_camera::FEED_FPS,
    );
    [
        "-hide_banner",
        "-loglevel",
        "warning",
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
        "-",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

/// Picks the H.264 encoder: hardware first, because it leaves the CPU free for
/// the RTSP reader. The bundled LGPL build has no libx264, so a system FFmpeg
/// is the only way that fallback ever applies.
pub fn select_h264_encoder(capabilities: &FfmpegCapabilities) -> Option<&'static str> {
    [
        ("h264_videotoolbox", capabilities.h264_videotoolbox),
        ("h264_nvenc", capabilities.h264_nvenc),
        ("h264_qsv", capabilities.h264_qsv),
        ("h264_amf", capabilities.h264_amf),
        ("libx264", capabilities.libx264),
    ]
    .into_iter()
    .find_map(|(name, available)| available.then_some(name))
}

/// FFmpeg's capture input for the commentary microphone.
#[cfg(target_os = "macos")]
const AUDIO_INPUT_FORMAT: &str = "avfoundation";
#[cfg(target_os = "windows")]
const AUDIO_INPUT_FORMAT: &str = "dshow";

/// avfoundation addresses devices as ":name"; dshow as "audio=name".
fn audio_input_argument(microphone: &str) -> String {
    if cfg!(target_os = "windows") {
        format!("audio={microphone}")
    } else {
        format!(":{microphone}")
    }
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
                 [bgsrc]scale={small_width}:{small_height}:force_original_aspect_ratio=increase,crop={small_width}:{small_height},gblur=sigma=14:steps=2,colorlevels=romax=0.73:gomax=0.73:bomax=0.73,scale={width}:{height}:flags=bicubic[bg];\
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
    } else if encoder == "h264_nvenc" {
        args.extend([
            "h264_nvenc",
            "-preset",
            "p4",
            "-tune",
            "ll",
            "-rc",
            "cbr",
            "-profile:v",
            "high",
            "-bf",
            "0",
        ]);
    } else if encoder == "h264_qsv" {
        args.extend([
            "h264_qsv",
            "-preset",
            "veryfast",
            "-profile:v",
            "high",
            "-bf",
            "0",
        ]);
    } else if encoder == "h264_amf" {
        args.extend([
            "h264_amf",
            "-quality",
            "speed",
            "-rc",
            "cbr",
            "-profile:v",
            "high",
            "-bf",
            "0",
        ]);
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
        return Err(BridgeError::Ffmpeg(format!(
            "This FFmpeg build has no {AUDIO_INPUT_FORMAT} input device for the microphone"
        )));
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
    let encoder = select_h264_encoder(&capabilities)
        .ok_or_else(|| BridgeError::Ffmpeg("No production H.264 encoder is available".into()))?;

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
            ["-thread_queue_size", "1024", "-f", AUDIO_INPUT_FORMAT, "-i"]
                .into_iter()
                .map(OsString::from),
        );
        args.push(OsString::from(audio_input_argument(microphone)));
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
    let capabilities = capabilities().await;
    let ffmpeg = capabilities
        .ffmpeg_path
        .as_deref()
        .map(PathBuf::from)
        .ok_or_else(|| BridgeError::Ffmpeg("FFmpeg is unavailable".into()))?;
    // The bundled LGPL build has no libx264; VideoToolbox is always there.
    let encoder = select_h264_encoder(&capabilities).ok_or_else(|| {
        BridgeError::Ffmpeg("No H.264 encoder is available for the test video".into())
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
            encoder,
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
    // WebRTC needs Opus. libopus is better, but the bundled LGPL build carries
    // FFmpeg's native (experimental) Opus encoder instead.
    let opus_encoder = if capabilities.libopus {
        vec!["-c:a", "libopus"]
    } else if capabilities.opus {
        vec!["-c:a", "opus", "-strict", "-2"]
    } else {
        return Err(BridgeError::Ffmpeg(
            "This FFmpeg build has no Opus encoder for the browser preview".into(),
        ));
    };
    let encoder = select_h264_encoder(&capabilities).ok_or_else(|| {
        BridgeError::Ffmpeg("No browser-compatible H.264 encoder is available".into())
    })?;

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
    ]);
    raw_args.extend(opus_encoder);
    raw_args.extend([
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
    let child = crate::console::hide(&mut Command::new(ffprobe))
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
    let child = crate::console::hide(&mut Command::new(ffprobe))
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

/// Homebrew's own binary, looked up by path: a GUI app's PATH does not
/// include /opt/homebrew/bin.
pub fn locate_homebrew() -> Option<PathBuf> {
    [
        PathBuf::from("/opt/homebrew/bin/brew"),
        PathBuf::from("/usr/local/bin/brew"),
    ]
    .into_iter()
    .find(|candidate| candidate.is_file())
}

/// Runs `brew install ffmpeg`, streaming each output line to the UI so the
/// user can watch a multi-minute install instead of staring at a frozen button.
pub async fn install_via_homebrew(app: &tauri::AppHandle) -> BridgeResult<()> {
    let brew = locate_homebrew().ok_or_else(|| {
        BridgeError::Ffmpeg(
            "Homebrew was not found. Install Homebrew from brew.sh, then try again.".into(),
        )
    })?;
    let mut child = Command::new(&brew)
        .args(["install", "ffmpeg"])
        .env("HOMEBREW_NO_AUTO_UPDATE", "1")
        .env("HOMEBREW_NO_ANALYTICS", "1")
        .env("HOMEBREW_NO_ENV_HINTS", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| BridgeError::Ffmpeg(format!("brew install ffmpeg: {error}")))?;

    let mut tasks = Vec::new();
    for stream in [
        child.stdout.take().map(Either::Out),
        child.stderr.take().map(Either::Err),
    ]
    .into_iter()
    .flatten()
    {
        let app = app.clone();
        tasks.push(tauri::async_runtime::spawn(async move {
            match stream {
                Either::Out(pipe) => emit_lines(app, pipe).await,
                Either::Err(pipe) => emit_lines(app, pipe).await,
            }
        }));
    }
    let status = child
        .wait()
        .await
        .map_err(|error| BridgeError::Ffmpeg(format!("brew install ffmpeg: {error}")))?;
    for task in tasks {
        let _ = task.await;
    }
    if !status.success() {
        return Err(BridgeError::Ffmpeg(format!(
            "Homebrew could not install FFmpeg ({status}). See the log above."
        )));
    }
    if locate("ffmpeg").is_none() {
        return Err(BridgeError::Ffmpeg(
            "Homebrew finished but FFmpeg is still not in /opt/homebrew/bin or /usr/local/bin."
                .into(),
        ));
    }
    Ok(())
}

enum Either {
    Out(tokio::process::ChildStdout),
    Err(tokio::process::ChildStderr),
}

async fn emit_lines<R: tokio::io::AsyncRead + Unpin>(app: tauri::AppHandle, pipe: R) {
    let mut lines = tokio::io::BufReader::new(pipe).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        let _ = app.emit("ffmpeg-install-log", line);
    }
}

/// The app ships its own LGPL FFmpeg next to the executable, so nothing has to
/// be installed. A system copy is still accepted as a fallback (development
/// runs, or a user who prefers their own build).
pub(crate) fn locate(name: &str) -> Option<PathBuf> {
    #[cfg(windows)]
    let name = &format!("{name}.exe");
    let bundled = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join(name)));
    bundled
        .into_iter()
        .chain([
            PathBuf::from("/opt/homebrew/bin").join(name),
            PathBuf::from("/usr/local/bin").join(name),
            PathBuf::from("/usr/bin").join(name),
        ])
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
        assert!(filter.contains("gblur"));
        assert!(filter.contains("scale=1080:1920:force_original_aspect_ratio=decrease"));
        assert!(!filter.contains("pad="));
    }

    /// The bundled FFmpeg is built without --enable-gpl, so a GPL-only filter
    /// would fail at runtime for every user.
    #[test]
    fn filters_stay_within_the_lgpl_build() {
        for layout in [OutputLayout::Portrait, OutputLayout::Landscape] {
            for fit in [FitMode::Fit, FitMode::Fill] {
                let filter = production_video_filter(layout, fit);
                for gpl_only in ["boxblur", "eq=", "geq", "hqdn3d", "smartblur", "delogo"] {
                    assert!(
                        !filter.contains(gpl_only),
                        "{gpl_only} is GPL-only but appears in {filter}"
                    );
                }
            }
        }
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
