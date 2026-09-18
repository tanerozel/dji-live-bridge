use serde::Serialize;

use crate::{
    ffmpeg::FfmpegCapabilities,
    platform::macos,
    state::{BridgeSnapshot, ServiceStatus},
};

#[derive(Debug, Clone, Copy, Serialize)]
pub enum DiagnosticLevel {
    Pass,
    Warning,
    Fail,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticItem {
    pub name: String,
    pub level: DiagnosticLevel,
    pub detail: String,
    pub action: Option<String>,
}

pub fn collect(snapshot: &BridgeSnapshot, ffmpeg: &FfmpegCapabilities) -> Vec<DiagnosticItem> {
    let mut items = Vec::new();
    items.push(item(
        "macOS / architecture",
        DiagnosticLevel::Pass,
        format!("macOS {}", std::env::consts::ARCH),
        None,
    ));
    items.push(
        if let (Some(interface), Some(ip)) = (
            snapshot.selected_interface.as_ref(),
            snapshot.lan_ipv4.as_ref(),
        ) {
            item(
                "LAN interface",
                DiagnosticLevel::Pass,
                format!("{interface} · {ip}"),
                None,
            )
        } else {
            item(
                "LAN interface",
                DiagnosticLevel::Fail,
                "No eligible IPv4 LAN interface detected".into(),
                Some("Connect the Mac and DJI RC 2 to the same LAN, then select the interface."),
            )
        },
    );
    items.push(match snapshot.media_mtx {
        ServiceStatus::Ready => item(
            "MediaMTX",
            DiagnosticLevel::Pass,
            "Process and loopback Control API are ready".into(),
            None,
        ),
        ServiceStatus::Starting => item(
            "MediaMTX",
            DiagnosticLevel::Warning,
            "Starting".into(),
            Some("Wait for readiness; inspect the process log if this persists."),
        ),
        ServiceStatus::Failed | ServiceStatus::Unavailable => item(
            "MediaMTX",
            DiagnosticLevel::Fail,
            "Sidecar is not ready".into(),
            Some("Check ports 1935, 8554, 8889, 9997 and reinstall the pinned sidecar."),
        ),
    });
    items.push(if snapshot.publisher_present {
        item(
            "/drone publisher",
            DiagnosticLevel::Pass,
            "A real MediaMTX publisher is present".into(),
            None,
        )
    } else {
        item(
            "/drone publisher",
            DiagnosticLevel::Warning,
            "Waiting for DJI Fly or Test Drone".into(),
            Some("Start RTMP streaming to the displayed URL."),
        )
    });
    items.push(
        if ffmpeg.ffmpeg_path.is_some() && ffmpeg.ffprobe_path.is_some() {
            item(
                "FFmpeg / ffprobe",
                DiagnosticLevel::Pass,
                ffmpeg.version.clone().unwrap_or_else(|| "Detected".into()),
                None,
            )
        } else {
            item(
                "FFmpeg / ffprobe",
                DiagnosticLevel::Fail,
                "External FFmpeg tools are unavailable".into(),
                Some("Install an FFmpeg build that includes H.264 and Opus encoders."),
            )
        },
    );
    items.push(
        if ffmpeg.aac && (ffmpeg.h264_videotoolbox || ffmpeg.libx264) {
            item(
                "Built-in production",
                DiagnosticLevel::Pass,
                format!(
                    "H.264 encoder: {} · AAC: available{}",
                    if ffmpeg.h264_videotoolbox {
                        "VideoToolbox"
                    } else {
                        "libx264"
                    },
                    if ffmpeg.avfoundation {
                        " · AVFoundation microphone input: available"
                    } else {
                        " · AVFoundation microphone input: unavailable"
                    }
                ),
                None,
            )
        } else {
            item(
                "Built-in production",
                DiagnosticLevel::Fail,
                "Required H.264/AAC encoders are unavailable".into(),
                Some("Install a supported FFmpeg build; OBS remains an optional fallback."),
            )
        },
    );
    items.push(if snapshot.production.active {
        item(
            "Production route",
            if snapshot.production.forward_error.is_some() {
                DiagnosticLevel::Fail
            } else {
                DiagnosticLevel::Pass
            },
            format!(
                "/production: {:?} · forward: {} · outbound: {} bytes{}",
                snapshot.production.path_status,
                snapshot
                    .production
                    .forward_state
                    .as_deref()
                    .unwrap_or("pending"),
                snapshot.production.outbound_bytes,
                snapshot
                    .production
                    .forward_error
                    .as_deref()
                    .map(|error| format!(" · {error}"))
                    .unwrap_or_default()
            ),
            snapshot
                .production
                .forward_error
                .as_ref()
                .map(|_| "Stop the live route, verify destination URL/key, then start again."),
        )
    } else {
        item(
            "Production route",
            DiagnosticLevel::Warning,
            "Built-in production is stopped".into(),
            Some("Prepare production and explicitly press START LIVE."),
        )
    });
    items.push(match snapshot.virtual_camera.status {
        ServiceStatus::Ready => item(
            "DJI Live Bridge Camera",
            DiagnosticLevel::Pass,
            format!(
                "Core Media I/O extension enabled · {}x{} @ {} fps · feed {}",
                snapshot.virtual_camera.width,
                snapshot.virtual_camera.height,
                snapshot.virtual_camera.fps,
                if snapshot.virtual_camera.feed_active {
                    "running"
                } else {
                    "stopped"
                }
            ),
            None,
        ),
        ServiceStatus::Starting => item(
            "DJI Live Bridge Camera",
            DiagnosticLevel::Warning,
            snapshot
                .virtual_camera
                .detail
                .clone()
                .unwrap_or_else(|| "Waiting for macOS approval".into()),
            Some("Allow the camera extension in System Settings > Privacy & Security."),
        ),
        ServiceStatus::Failed | ServiceStatus::Unavailable => item(
            "DJI Live Bridge Camera",
            DiagnosticLevel::Warning,
            snapshot
                .virtual_camera
                .detail
                .clone()
                .unwrap_or_else(|| "Camera extension is unavailable".into()),
            Some("Use a signed build from /Applications, then enable the virtual camera."),
        ),
    });
    items.push(if snapshot.obs.connected {
        item(
            "OBS WebSocket",
            DiagnosticLevel::Pass,
            format!(
                "OBS {} · WebSocket {}",
                snapshot.obs.obs_version.as_deref().unwrap_or("unknown"),
                snapshot
                    .obs
                    .websocket_version
                    .as_deref()
                    .unwrap_or("unknown")
            ),
            None,
        )
    } else {
        item(
            "OBS WebSocket",
            DiagnosticLevel::Warning,
            snapshot
                .obs
                .last_error
                .clone()
                .unwrap_or_else(|| "Not connected".into()),
            Some("Optional: install/start OBS and enable Tools > WebSocket Server Settings on port 4455."),
        )
    });
    items.push(if snapshot.audio_inputs.is_empty() {
        item(
            "Commentary microphone",
            DiagnosticLevel::Warning,
            "No CoreAudio input device is currently visible".into(),
            Some("Connect or enable a microphone. macOS will request Microphone permission only when built-in commentary capture is used."),
        )
    } else {
        item(
            "Commentary microphone",
            DiagnosticLevel::Pass,
            snapshot
                .audio_inputs
                .iter()
                .map(|device| device.name.as_str())
                .collect::<Vec<_>>()
                .join(", "),
            None,
        )
    });
    items.push(if macos::tiktok_live_studio_installed() {
        item(
            "TikTok LIVE Studio",
            DiagnosticLevel::Pass,
            "macOS application found in /Applications".into(),
            None,
        )
    } else {
        item(
            "TikTok LIVE Studio",
            DiagnosticLevel::Warning,
            "macOS application is not installed".into(),
            Some("Open the current official TikTok download page from the destination panel."),
        )
    });
    items
}

fn item(
    name: impl Into<String>,
    level: DiagnosticLevel,
    detail: String,
    action: Option<&str>,
) -> DiagnosticItem {
    DiagnosticItem {
        name: name.into(),
        level,
        detail,
        action: action.map(str::to_string),
    }
}
