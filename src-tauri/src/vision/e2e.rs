//! End-to-end check of AI Vision on the real streaming paths (macOS):
//! a drone video is published to MediaMTX, then the production encode (with a
//! MediaMTX forward to a local RTMP server standing in for a platform) and the
//! virtual camera feed run first without and then with the overlay, while CPU,
//! memory and inference numbers are sampled.
//!
//! Needs files outside the repo and runs for minutes, so it is ignored:
//!
//! ```sh
//! DLB_E2E_VIDEO=/path/drone.webm DLB_E2E_MODELS=/dir/with/yolox_tiny.onnx \
//! DLB_E2E_OUT=/tmp/out DLB_E2E_SECONDS=120 \
//! cargo test --release -- --ignored --nocapture drone_to_rtmp_and_camera
//! ```
//!
//! Quit the app first: this uses the same ports (1935/8554/9997, 49213).

use std::{
    io::Read,
    path::{Path, PathBuf},
    process::Command,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use super::*;
use crate::{
    config::{FitMode, OutputLayout},
    ffmpeg::{self, NativeProductionSettings},
    process::{ProcessSpec, RestartPolicy},
    virtual_camera,
};

const SINK_URL: &str = "rtmp://127.0.0.1:19350/live";
const SINK_KEY: &str = "e2ekey";

fn env_path(name: &str) -> PathBuf {
    PathBuf::from(std::env::var(name).unwrap_or_else(|_| panic!("{name} is not set")))
}

fn binaries() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("binaries")
}

/// `ffmpeg::locate` looks next to the executable first; put the bundled LGPL
/// build there so the test runs what users run.
fn install_bundled_ffmpeg() {
    let directory = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    for tool in ["ffmpeg", "ffprobe"] {
        let source = binaries().join(format!("{tool}-{}", env!("BUILD_TARGET_TRIPLE")));
        std::fs::copy(&source, directory.join(tool)).unwrap();
    }
}

/// CPU seconds and resident memory of a process, from `ps`.
fn usage(pid: u32) -> Option<(f64, u64)> {
    let output = Command::new("ps")
        .args(["-o", "time=,rss=", "-p", &pid.to_string()])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    let mut fields = text.split_whitespace();
    let time = fields.next()?;
    let rss_kb: u64 = fields.next()?.parse().ok()?;
    // [[hh:]mm:]ss.ss
    let seconds = time.split(':').fold(0.0, |total, part| {
        total * 60.0 + part.parse::<f64>().unwrap_or(0.0)
    });
    Some((seconds, rss_kb * 1024))
}

struct Sampler {
    started: Instant,
    first: std::collections::HashMap<String, (f64, Instant)>,
}

impl Sampler {
    fn new() -> Self {
        Self {
            started: Instant::now(),
            first: Default::default(),
        }
    }

    /// Average CPU % since the first sample of each process, and RSS now.
    fn report(&mut self, label: &str, processes: &[(String, u32)]) -> Vec<(String, f64, u64)> {
        let mut rows = Vec::new();
        for (name, pid) in processes {
            let Some((cpu, rss)) = usage(*pid) else {
                continue;
            };
            let (cpu0, at0) = *self
                .first
                .entry(name.clone())
                .or_insert((cpu, Instant::now()));
            let wall = at0.elapsed().as_secs_f64();
            let percent = if wall > 0.5 {
                (cpu - cpu0) / wall * 100.0
            } else {
                0.0
            };
            rows.push((name.clone(), percent, rss));
        }
        let line = rows
            .iter()
            .map(|(name, cpu, rss)| format!("{name} {cpu:.1}% {:.0}MB", *rss as f64 / 1e6))
            .collect::<Vec<_>>()
            .join(" | ");
        println!(
            "[{label} t={:>5.1}s] {line}",
            self.started.elapsed().as_secs_f64()
        );
        rows
    }
}

async fn processes(supervisor: &ProcessSupervisor) -> Vec<(String, u32)> {
    let mut list: Vec<(String, u32)> = supervisor
        .tick()
        .await
        .into_iter()
        .filter(|process| process.name != "drone-publisher" && process.name != "rtmp-sink")
        .filter_map(|process| Some((process.name, process.pid?)))
        .collect();
    list.push(("app(this test)".into(), std::process::id()));
    list
}

/// Reads the virtual camera's NV12 frames like the camera extension does and
/// keeps the latest one.
fn read_camera(running: Arc<AtomicBool>, frames: Arc<AtomicU64>, latest: Arc<Mutex<Vec<u8>>>) {
    let size = 1080 * 1920 * 3 / 2;
    while running.load(Ordering::Relaxed) {
        let Ok(mut stream) = std::net::TcpStream::connect("127.0.0.1:49213") else {
            std::thread::sleep(Duration::from_millis(200));
            continue;
        };
        let mut frame = vec![0_u8; size];
        while running.load(Ordering::Relaxed) && stream.read_exact(&mut frame).is_ok() {
            frames.fetch_add(1, Ordering::Relaxed);
            latest.lock().unwrap().clone_from(&frame);
        }
    }
}

fn save_nv12(frame: &[u8], path: &Path) {
    let raw = path.with_extension("nv12");
    std::fs::write(&raw, frame).unwrap();
    let status = Command::new(binaries().join(format!("ffmpeg-{}", env!("BUILD_TARGET_TRIPLE"))))
        .args(["-hide_banner", "-loglevel", "error", "-y", "-f", "rawvideo"])
        .args(["-pix_fmt", "nv12", "-video_size", "1080x1920", "-i"])
        .arg(&raw)
        .args(["-frames:v", "1", "-vf", "scale=540:960"])
        .arg(path)
        .status()
        .unwrap();
    assert!(status.success());
    std::fs::remove_file(raw).ok();
}

fn grab(input: &str, path: &Path) -> bool {
    let mut command =
        Command::new(binaries().join(format!("ffmpeg-{}", env!("BUILD_TARGET_TRIPLE"))));
    command.args(["-hide_banner", "-loglevel", "error", "-y"]);
    if input.starts_with("rtsp://") {
        command.args(["-rtsp_transport", "tcp"]);
    }
    command
        .args(["-i", input])
        .args(["-frames:v", "1", "-vf", "scale=iw/2:-2"])
        .arg(path)
        .status()
        .is_ok_and(|status| status.success())
}

async fn start_outputs(
    supervisor: &ProcessSupervisor,
    overlay: Option<OverlayInput>,
    has_audio: bool,
) -> String {
    let settings = NativeProductionSettings {
        layout: OutputLayout::Portrait,
        fit_mode: FitMode::Fit,
        microphone: None,
        microphone_muted: false,
        microphone_volume_db: 0.0,
        microphone_sync_ms: 0,
        noise_suppression: false,
        compressor: false,
        limiter: false,
    };
    let encoder = ffmpeg::start_production(supervisor, &settings, has_audio, overlay.as_ref())
        .await
        .unwrap();
    virtual_camera::start_feed(supervisor, overlay)
        .await
        .unwrap();
    encoder
}

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn drone_to_rtmp_and_camera_with_overlay() {
    let video = env_path("DLB_E2E_VIDEO");
    let models = env_path("DLB_E2E_MODELS");
    let out = env_path("DLB_E2E_OUT");
    let seconds: u64 = std::env::var("DLB_E2E_SECONDS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(60);
    std::fs::create_dir_all(&out).unwrap();
    install_bundled_ffmpeg();
    let ffmpeg_path = ffmpeg::locate("ffmpeg").unwrap();
    let supervisor = ProcessSupervisor::default();
    let http = reqwest::Client::new();

    // MediaMTX with the app's own runtime configuration.
    supervisor
        .start(ProcessSpec {
            name: "mediamtx".into(),
            executable: binaries().join(format!("mediamtx-{}", env!("BUILD_TARGET_TRIPLE"))),
            args: vec![
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("mediamtx.runtime.yml")
                    .into_os_string(),
            ],
            restart_policy: RestartPolicy::Never,
        })
        .await
        .unwrap();
    for _ in 0..50 {
        if http
            .get("http://127.0.0.1:9997/v3/paths/list")
            .send()
            .await
            .is_ok()
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    // The "drone": the clip at its own resolution, in real time, looped.
    let publisher_args: Vec<std::ffi::OsString> = [
        "-hide_banner",
        "-loglevel",
        "error",
        "-re",
        "-stream_loop",
        "-1",
        "-i",
    ]
    .into_iter()
    .map(Into::into)
    .chain([video.into_os_string()])
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
            "fps=30",
            "-c:v",
            "h264_videotoolbox",
            "-b:v",
            "8M",
            "-g",
            "30",
            "-bf",
            "0",
            "-c:a",
            "aac",
            "-f",
            "flv",
            "rtmp://127.0.0.1:1935/drone",
        ]
        .into_iter()
        .map(Into::into),
    )
    .collect();
    supervisor
        .start(ProcessSpec {
            name: "drone-publisher".into(),
            executable: ffmpeg_path.clone(),
            args: publisher_args,
            restart_policy: RestartPolicy::Never,
        })
        .await
        .unwrap();

    // A second, RTMP-only MediaMTX standing in for Instagram/YouTube: the
    // production forward publishes to it exactly as to a platform. (FFmpeg's
    // own `-listen` RTMP server does not complete MediaMTX's handshake.)
    let sink_config = out.join("rtmp-destination.yml");
    std::fs::write(
        &sink_config,
        "logLevel: warn\napi: false\nmetrics: false\npprof: false\nplayback: false\n\
         rtsp: false\nrtmp: true\nrtmpAddress: 127.0.0.1:19350\nhls: false\nwebrtc: false\n\
         srt: false\nmoq: false\npaths:\n  all_others:\n",
    )
    .unwrap();
    supervisor
        .start(ProcessSpec {
            name: "rtmp-sink".into(),
            executable: binaries().join(format!("mediamtx-{}", env!("BUILD_TARGET_TRIPLE"))),
            args: vec![sink_config.into_os_string()],
            restart_policy: RestartPolicy::Never,
        })
        .await
        .unwrap();

    let mut metadata = None;
    for _ in 0..40 {
        if let Ok(probed) = ffmpeg::inspect_stream().await
            && probed.resolution.is_some()
        {
            metadata = Some(probed);
            break;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    let metadata = metadata.expect("the drone stream never became readable");
    let source = parse_resolution(metadata.resolution.as_deref());
    println!("source {:?} {:?} fps", metadata.resolution, metadata.fps);

    http.patch("http://127.0.0.1:9997/v3/config/paths/patch/production")
        .json(&serde_json::json!({ "forward": [{ "dest": format!("{SINK_URL}#{SINK_KEY}") }] }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();

    let camera_running = Arc::new(AtomicBool::new(true));
    let camera_frames = Arc::new(AtomicU64::new(0));
    let camera_latest = Arc::new(Mutex::new(Vec::new()));
    {
        let (running, frames, latest) = (
            camera_running.clone(),
            camera_frames.clone(),
            camera_latest.clone(),
        );
        std::thread::spawn(move || read_camera(running, frames, latest));
    }

    // Phase 1: today's pipeline, AI off.
    let encoder = start_outputs(&supervisor, None, true).await;
    println!("production encoder {encoder}");
    let phase = Duration::from_secs((seconds / 4).max(20));
    let mut sampler = Sampler::new();
    let started = Instant::now();
    let frames_before = camera_frames.load(Ordering::Relaxed);
    let mut baseline = Vec::new();
    while started.elapsed() < phase {
        tokio::time::sleep(Duration::from_secs(5)).await;
        baseline = sampler.report("AI off", &processes(&supervisor).await);
    }
    let camera_fps_off = (camera_frames.load(Ordering::Relaxed) - frames_before) as f64
        / started.elapsed().as_secs_f64();
    grab(
        "rtsp://127.0.0.1:8554/production",
        &out.join("production-ai-off.png"),
    );

    // Phase 2: AI Vision on, model taken from DLB_E2E_MODELS.
    let root = out.join("app-support");
    // DLB_E2E_MODEL_ID picks the model; its file is DLB_E2E_MODELS/<id>.onnx
    // with dashes as underscores (yolox_tiny.onnx), or DLB_E2E_MODEL.
    let id = std::env::var("DLB_E2E_MODEL_ID").unwrap_or_else(|_| "yolox-tiny".into());
    let spec = models::find(&id).expect("unknown DLB_E2E_MODEL_ID");
    let file = std::env::var("DLB_E2E_MODEL")
        .map(PathBuf::from)
        .unwrap_or_else(|_| models.join(format!("{}.onnx", id.replace('-', "_"))));
    let store = ModelStore::new(root.join("models"));
    let target = store.path(spec);
    std::fs::create_dir_all(target.parent().unwrap()).unwrap();
    std::fs::copy(file, &target).unwrap();
    std::fs::write(target.with_extension("onnx.sha256"), spec.sha256).unwrap();

    let settings = DetectionSettings {
        enabled: true,
        model_id: spec.id.into(),
        ..DetectionSettings::default()
    };
    let engine = VisionEngine::new(&root, supervisor.clone(), settings.clone());
    engine.apply(settings).await;
    engine.set_publisher(true).await;
    let overlay = engine.overlay_input(source).expect("overlay input");
    println!("overlay {overlay:?}, status {:?}", engine.state().status);
    start_outputs(&supervisor, Some(overlay), true).await;

    let mut sampler = Sampler::new();
    let started = Instant::now();
    let frames_before = camera_frames.load(Ordering::Relaxed);
    let mut with_ai = Vec::new();
    let mut max_rss = 0;
    let mut first_rss = None;
    let mut saved_camera = false;
    while started.elapsed() < Duration::from_secs(seconds) {
        tokio::time::sleep(Duration::from_secs(5)).await;
        let state = engine.state();
        with_ai = sampler.report("AI on ", &processes(&supervisor).await);
        let rss = with_ai
            .iter()
            .find(|(name, ..)| name.starts_with("app"))
            .map_or(0, |row| row.2);
        first_rss.get_or_insert(rss);
        max_rss = max_rss.max(rss);
        println!(
            "        vision {:?} {:?} {:?} ms {:?} fps dropped {} now {} unique {} {:?}",
            state.status,
            state.backend,
            state.inference_ms,
            state.inference_fps,
            state.dropped_frames,
            state.counts.current_total,
            state.counts.unique_total,
            state
                .counts
                .current_by_class
                .iter()
                .map(|c| format!("{} {}", c.class, c.count))
                .collect::<Vec<_>>()
        );
        if !saved_camera && state.counts.current_total > 0 {
            grab(
                "rtsp://127.0.0.1:8554/production",
                &out.join("production-ai-on.png"),
            );
            let frame = camera_latest.lock().unwrap().clone();
            if !frame.is_empty() {
                save_nv12(&frame, &out.join("virtual-camera-ai-on.png"));
                saved_camera = true;
            }
        }
    }
    let camera_fps_on = (camera_frames.load(Ordering::Relaxed) - frames_before) as f64
        / started.elapsed().as_secs_f64();
    let final_state = engine.state();
    // What the destination receives right now, overlay included.
    let destination_ok = grab(
        &format!("{SINK_URL}/{SINK_KEY}"),
        &out.join("rtmp-destination.png"),
    );
    let forward = http
        .get("http://127.0.0.1:9997/v3/paths/forward-dests/list?path=production")
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    println!("forward status: {forward}");

    // Turning AI off must release the model; the outputs keep running.
    let before_off = usage(std::process::id()).map_or(0, |u| u.1);
    engine
        .apply(DetectionSettings {
            enabled: false,
            ..engine.settings()
        })
        .await;
    tokio::time::sleep(Duration::from_secs(3)).await;
    let after_off = usage(std::process::id()).map_or(0, |u| u.1);
    let tap_gone = !supervisor
        .tick()
        .await
        .iter()
        .any(|process| process.name == tap::TAP_PROCESS_NAME && process.pid.is_some());

    camera_running.store(false, Ordering::Relaxed);
    supervisor.stop("native-production").await.ok();
    tokio::time::sleep(Duration::from_secs(2)).await;
    supervisor.shutdown_all().await;

    println!("\n==== summary ====");
    println!("camera fps: AI off {camera_fps_off:.1}, AI on {camera_fps_on:.1}");
    for (name, cpu, rss) in &baseline {
        println!(
            "AI off  {name:<22} {cpu:>6.1}% CPU {:>6.0} MB",
            *rss as f64 / 1e6
        );
    }
    for (name, cpu, rss) in &with_ai {
        println!(
            "AI on   {name:<22} {cpu:>6.1}% CPU {:>6.0} MB",
            *rss as f64 / 1e6
        );
    }
    println!(
        "inference {:?} ms on {:?}, {:?} fps, dropped {}",
        final_state.inference_ms,
        final_state.backend,
        final_state.inference_fps,
        final_state.dropped_frames
    );
    println!(
        "test process RSS: first {:.0} MB, max {:.0} MB; AI off: {:.0} -> {:.0} MB; tap stopped: {tap_gone}",
        first_rss.unwrap_or(0) as f64 / 1e6,
        max_rss as f64 / 1e6,
        before_off as f64 / 1e6,
        after_off as f64 / 1e6
    );
    println!("RTMP destination received video: {destination_ok}");
    assert!(
        final_state.counts.unique_total > 0,
        "no animal was detected"
    );
    assert!(
        camera_fps_on > 25.0,
        "virtual camera fell to {camera_fps_on:.1} fps"
    );
    assert!(destination_ok, "the RTMP destination received nothing");
    assert!(
        tap_gone,
        "the tap kept running after AI Vision was turned off"
    );
}

/// Runs the engine against a `/drone` that is already being published (by the
/// app, or anything else) without publishing anything itself: it only reads
/// `/drone`, so a running app is undisturbed. Checks the chosen model end to
/// end, saves a composite of the video and the overlay, and that a reconnect
/// reuses the loaded model.
///
/// ```sh
/// DLB_E2E_MODEL=/path/model.onnx DLB_E2E_MODEL_ID=dfine-x-obj2coco DLB_E2E_OUT=/tmp/out \
/// cargo test --release -- --ignored --nocapture engine_on_a_running_drone_path
/// ```
#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn engine_on_a_running_drone_path() {
    let model = env_path("DLB_E2E_MODEL");
    let id = std::env::var("DLB_E2E_MODEL_ID").expect("DLB_E2E_MODEL_ID");
    let out = env_path("DLB_E2E_OUT");
    std::fs::create_dir_all(&out).unwrap();
    install_bundled_ffmpeg();
    let spec = models::find(&id).expect("unknown model id");
    let root = out.join(format!("app-support-{id}"));
    let store = ModelStore::new(root.join("models"));
    let target = store.path(spec);
    std::fs::create_dir_all(target.parent().unwrap()).unwrap();
    if !target.exists() {
        std::fs::copy(&model, &target).unwrap();
    }
    std::fs::write(target.with_extension("onnx.sha256"), spec.sha256).unwrap();

    let source = parse_resolution(
        ffmpeg::inspect_stream()
            .await
            .expect("nothing is publishing /drone")
            .resolution
            .as_deref(),
    );
    let supervisor = ProcessSupervisor::default();
    let settings = DetectionSettings {
        enabled: true,
        model_id: spec.id.into(),
        confidence_threshold: 0.35,
        ..DetectionSettings::default()
    };
    let engine = VisionEngine::new(&root, supervisor.clone(), settings.clone());
    engine.apply(settings).await;
    let started = Instant::now();
    engine.set_publisher(true).await;
    println!(
        "{} started in {:.1} s: {:?} on {:?}",
        spec.name,
        started.elapsed().as_secs_f64(),
        engine.state().status,
        engine.state().backend
    );
    let overlay = engine.overlay_input(source).expect("overlay input");
    for _ in 0..4 {
        tokio::time::sleep(Duration::from_secs(5)).await;
        let state = engine.state();
        println!(
            "  {:?} ms {:?} fps, now {} unique {} {:?}, rss {:.0} MB",
            state.inference_ms,
            state.inference_fps,
            state.counts.current_total,
            state.counts.unique_total,
            state.counts.current_by_class,
            usage(std::process::id()).map_or(0.0, |u| u.1 as f64 / 1e6)
        );
    }

    // What an output would look like: the live video with the overlay on it.
    let composite = out.join(format!("composite-{id}.png"));
    let mut args: Vec<String> = [
        "-hide_banner",
        "-loglevel",
        "error",
        "-y",
        "-rtsp_transport",
        "tcp",
        "-i",
        "rtsp://127.0.0.1:8554/drone",
    ]
    .into_iter()
    .map(str::to_string)
    .collect();
    args.extend(overlay.input_args());
    args.extend(ffmpeg::video_filter_args("fps=30,scale=960:-2", Some(1)));
    args.extend(["-frames:v", "1"].map(str::to_string));
    args.push(composite.display().to_string());
    let status = Command::new(ffmpeg::locate("ffmpeg").unwrap())
        .args(&args)
        .status()
        .unwrap();
    println!("composite saved: {} ({status})", composite.display());

    // A drone reconnect must reuse the parked model instead of reloading it.
    engine.set_publisher(false).await;
    let started = Instant::now();
    engine.set_publisher(true).await;
    let reconnect = started.elapsed();
    println!("reconnect restarted in {:.2} s", reconnect.as_secs_f64());
    engine.shutdown().await;
    supervisor.shutdown_all().await;
    assert!(status.success());
    assert!(reconnect < Duration::from_secs(5), "the model was reloaded");
}

/// Replays pre-extracted pictures through a model, the tracker and the
/// counter at a fixed rate, to tune tracking without a live stream:
///
/// ```sh
/// ffmpeg -i drone.webm -vf fps=5,scale=640:640 -c:v ppm frames/%05d.ppm
/// DLB_REPLAY_FRAMES=frames DLB_REPLAY_FPS=5 DLB_E2E_MODEL=/path/model.onnx \
/// DLB_E2E_MODEL_ID=dfine-x-obj2coco cargo test --release -- --ignored --nocapture replay
/// ```
#[test]
#[ignore]
fn replay_through_detector_and_tracker() {
    let frames = env_path("DLB_REPLAY_FRAMES");
    let fps: f64 = std::env::var("DLB_REPLAY_FPS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(5.0);
    let threshold: f32 = std::env::var("DLB_REPLAY_THRESHOLD")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(0.6);
    let model = env_path("DLB_E2E_MODEL");
    let id = std::env::var("DLB_E2E_MODEL_ID").expect("DLB_E2E_MODEL_ID");
    let spec = models::find(&id).expect("unknown model id");
    let cache = std::env::temp_dir().join("dlb-replay-cache").join(spec.id);
    let mut detector = OnnxDetector::load(spec, &model, &cache).unwrap();
    let animals = settings::DetectionProfile::Animals.classes();
    let wanted: Vec<bool> = spec.labels.iter().map(|l| animals.contains(l)).collect();
    let mut tracker = ObjectTracker::new(TrackerConfig::with_threshold(threshold), 1);
    let mut counter = ObjectCounter::default();
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&frames)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "ppm"))
        .collect();
    paths.sort();
    let mut peak = 0;
    let mut summary = CountSummary::default();
    for (index, path) in paths.iter().enumerate() {
        let mut reader = std::io::BufReader::new(std::fs::File::open(path).unwrap());
        let frame = tap::read_ppm(&mut reader, spec.input_size)
            .unwrap()
            .unwrap();
        let detections = detector
            .detect(
                &frame,
                &DetectOptions {
                    min_confidence: (threshold * 0.5).clamp(0.05, 0.3),
                    wanted: &wanted,
                },
            )
            .unwrap();
        let now = index as f64 / fps;
        tracker.update(&detections, now);
        let visible = tracker.visible(now);
        peak = peak.max(visible.len());
        summary = counter.update(&visible, spec.labels);
        if index % (fps as usize * 4).max(1) == 0 {
            println!(
                "t={now:>5.1}s detections {:>3} visible {:>3} unique {:>4}",
                detections.len(),
                visible.len(),
                summary.unique_total
            );
        }
    }
    println!(
        "{} pictures at {fps} fps, threshold {threshold}: peak visible {peak}, unique {} {:?}",
        paths.len(),
        summary.unique_total,
        summary.unique_by_class
    );
}
