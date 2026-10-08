//! AI Vision: on-device object detection on the drone video.
//!
//! ```text
//! MediaMTX /drone ─┬─► production FFmpeg ──► /production ──► every RTMP destination
//!                  │        ▲ overlay input
//!                  ├─► camera FFmpeg ──► virtual camera
//!                  │        ▲ overlay input
//!                  │        │
//!                  └─► tap FFmpeg ─► FrameSlot ─► worker ─► Scene ─► OverlayServer
//!                      (decode, adaptive fps,    (detect,
//!                       fit model input)          track, count)
//! ```
//!
//! The video never waits for the AI. The tap is just another MediaMTX reader;
//! the worker always takes the newest picture and drops the rest; overlay
//! frames come from the latest finished result. Turning AI Vision off stops
//! the tap and the worker, which drops the model and its memory.
//!
//! Everything runs on this machine. The only network access is the one-time
//! model download (see `models`); no picture, detection or metadata leaves
//! the computer.

mod counter;
mod detector;
#[cfg(all(test, target_os = "macos"))]
mod e2e;
mod models;
mod onnx;
mod overlay;
mod settings;
mod tap;
mod tracker;

use std::{
    path::Path,
    sync::{
        Arc, Mutex, OnceLock, RwLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread::JoinHandle,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use serde::Serialize;
use tauri::{AppHandle, Emitter};

pub use counter::CountSummary;
pub use models::ModelInfo;
pub use overlay::{OVERLAY_FILTER, OverlayInput};
pub use settings::DetectionSettings;

use self::{
    counter::ObjectCounter,
    detector::{BoundingBox, DetectOptions, ObjectDetector},
    models::{ModelSpec, ModelStore},
    onnx::OnnxDetector,
    overlay::{OverlayServer, Scene, SceneCell},
    tap::{FrameSlot, FrameTap, TapLifecycle},
    tracker::{ObjectTracker, TrackView, TrackerConfig},
};
use crate::{
    error::{BridgeError, BridgeResult},
    process::ProcessSupervisor,
};

/// Emitted after every inference while AI Vision runs; the in-app preview
/// draws its boxes from it. It never leaves the app.
pub const DETECTIONS_EVENT: &str = "vision://detections";

/// Tracker time: seconds since the first call, shared by the worker and the
/// overlay so drawn boxes can be extrapolated to the moment they are sent.
fn clock() -> f64 {
    static EPOCH: OnceLock<Instant> = OnceLock::new();
    EPOCH.get_or_init(Instant::now).elapsed().as_secs_f64()
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub enum VisionStatus {
    #[default]
    Off,
    Downloading,
    WaitingForVideo,
    Loading,
    Running,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadProgress {
    pub received_bytes: u64,
    pub total_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VisionState {
    pub settings: DetectionSettings,
    pub status: VisionStatus,
    pub detail: Option<String>,
    pub models: Vec<ModelInfo>,
    /// Classes of the profile the chosen model can report, for the species
    /// picker.
    pub classes: Vec<String>,
    pub download: Option<DownloadProgress>,
    /// "CoreML" or "CPU" once a model is loaded.
    pub backend: Option<String>,
    /// Average time of one inference, in milliseconds.
    pub inference_ms: Option<f64>,
    /// Inferences completed per second, over the last few seconds.
    pub inference_fps: Option<f64>,
    /// Pictures replaced before the detector got to them.
    pub dropped_frames: u64,
    pub counts: CountSummary,
}

impl Default for VisionState {
    fn default() -> Self {
        let settings = DetectionSettings::default();
        Self {
            classes: class_names(&settings),
            settings,
            status: VisionStatus::Off,
            detail: None,
            models: Vec::new(),
            download: None,
            backend: None,
            inference_ms: None,
            inference_fps: None,
            dropped_frames: 0,
            counts: CountSummary::default(),
        }
    }
}

/// One detection as reported to the UI, in the shape of
/// `{"class": "cow", "confidence": 0.94, "bbox": {...}, "trackId": 12}`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectionResult {
    #[serde(rename = "class")]
    pub class_name: String,
    pub confidence: f32,
    /// Source-video pixels, when the source size is known.
    pub bbox: Option<PixelBox>,
    /// Fractions of the source picture (0..1).
    pub normalized_bbox: BoundingBox,
    pub track_id: u64,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct PixelBox {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DetectionEvent {
    timestamp_ms: u64,
    frame_width: Option<u32>,
    frame_height: Option<u32>,
    detections: Vec<DetectionResult>,
}

#[derive(Default)]
struct Runtime {
    status: VisionStatus,
    detail: Option<String>,
    download: Option<DownloadProgress>,
    backend: Option<&'static str>,
    inference_ms: Option<f64>,
    inference_fps: Option<f64>,
    dropped_frames: u64,
    counts: CountSummary,
}

struct Shared {
    store: ModelStore,
    settings: RwLock<DetectionSettings>,
    scene: Arc<SceneCell>,
    scene_version: AtomicU64,
    overlay: OverlayServer,
    runtime: Mutex<Runtime>,
    counter: Mutex<ObjectCounter>,
    /// Track ids stay unique across pipeline restarts, so the unique counter
    /// never mistakes a new animal for an old one.
    next_track_id: AtomicU64,
    source_size: Mutex<Option<(u32, u32)>>,
    app: OnceLock<AppHandle>,
    /// Set on app exit: nothing may start an FFmpeg after the supervisor has
    /// stopped them all.
    shutting_down: AtomicBool,
    tap_lifecycle: Arc<TapLifecycle>,
}

impl Shared {
    fn settings(&self) -> DetectionSettings {
        self.settings.read().expect("vision settings lock").clone()
    }

    fn runtime(&self, update: impl FnOnce(&mut Runtime)) {
        update(&mut self.runtime.lock().expect("vision runtime lock"));
    }

    /// Publishes what the overlay should draw. `None` keeps the current
    /// tracks and only refreshes the on/off switches.
    fn publish(&self, content: Option<(Vec<TrackView>, CountSummary, &'static [&'static str])>) {
        let settings = self.settings();
        let previous = self.scene.load();
        let (tracks, counts, labels) = match content {
            Some(content) => content,
            None => (
                previous.tracks.clone(),
                previous.counts.clone(),
                previous.labels,
            ),
        };
        self.scene.store(Scene {
            version: self.scene_version.fetch_add(1, Ordering::Relaxed) + 1,
            show_boxes: settings.enabled && settings.show_boxes,
            show_counter: settings.enabled && settings.show_counter,
            heading: settings.profile.heading(),
            labels,
            tracks,
            counts,
        });
    }
}

struct Control {
    publisher_present: bool,
    downloading: bool,
    pipeline: Option<Pipeline>,
    /// The model of a pipeline stopped because the drone went away, kept
    /// while AI Vision stays on so a reconnect does not reload it (large
    /// models can take minutes to prepare). Dropped when AI Vision is turned
    /// off or another model is chosen.
    parked: Option<(&'static str, Box<dyn ObjectDetector>)>,
}

/// Detection on the live video. Cheap to clone; all clones share one engine.
#[derive(Clone)]
pub struct VisionEngine {
    shared: Arc<Shared>,
    control: Arc<tokio::sync::Mutex<Control>>,
    supervisor: ProcessSupervisor,
}

impl VisionEngine {
    pub fn new(
        app_support_dir: &Path,
        supervisor: ProcessSupervisor,
        settings: DetectionSettings,
    ) -> Self {
        let scene = Arc::new(SceneCell::default());
        Self {
            shared: Arc::new(Shared {
                store: ModelStore::new(app_support_dir.join("models")),
                settings: RwLock::new(settings),
                overlay: OverlayServer::new(scene.clone()),
                scene,
                scene_version: AtomicU64::new(0),
                runtime: Mutex::new(Runtime::default()),
                counter: Mutex::new(ObjectCounter::default()),
                next_track_id: AtomicU64::new(1),
                source_size: Mutex::new(None),
                app: OnceLock::new(),
                shutting_down: AtomicBool::new(false),
                tap_lifecycle: Arc::new(TapLifecycle::default()),
            }),
            control: Arc::new(tokio::sync::Mutex::new(Control {
                publisher_present: false,
                downloading: false,
                pipeline: None,
                parked: None,
            })),
            supervisor,
        }
    }

    pub fn attach(&self, app: AppHandle) {
        let _ = self.shared.app.set(app);
    }

    pub fn settings(&self) -> DetectionSettings {
        self.shared.settings()
    }

    /// Applies new settings: downloads, loads or releases the model as needed.
    pub async fn apply(&self, settings: DetectionSettings) {
        *self.shared.settings.write().expect("vision settings lock") = settings;
        self.shared.publish(None);
        self.reconcile().await;
    }

    pub async fn set_publisher(&self, present: bool) {
        self.control.lock().await.publisher_present = present;
        if !present {
            *self.shared.source_size.lock().expect("source size lock") = None;
        }
        self.reconcile().await;
    }

    /// The drone picture's size, once probed; overlays are drawn at it.
    pub fn set_source_size(&self, size: Option<(u32, u32)>) {
        if size.is_some() {
            *self.shared.source_size.lock().expect("source size lock") = size;
        }
    }

    /// The overlay input an FFmpeg reading `/drone` should add, or `None` when
    /// AI Vision is off. Without a known source size there is nothing to
    /// draw on, and the output simply goes out without an overlay.
    pub fn overlay_input(&self, source: Option<(u32, u32)>) -> Option<OverlayInput> {
        if !self.shared.settings().enabled {
            return None;
        }
        self.set_source_size(source);
        let (width, height) = (*self.shared.source_size.lock().expect("source size lock"))?;
        match self.shared.overlay.input_for(width, height) {
            Ok(input) => Some(input),
            Err(error) => {
                tracing::warn!(%error, "AI Vision overlay unavailable; output continues without it");
                None
            }
        }
    }

    pub fn state(&self) -> VisionState {
        let settings = self.shared.settings();
        let runtime = self.shared.runtime.lock().expect("vision runtime lock");
        VisionState {
            classes: class_names(&settings),
            settings,
            status: runtime.status,
            detail: runtime.detail.clone(),
            models: self.shared.store.info(),
            download: runtime.download.clone(),
            backend: runtime.backend.map(str::to_string),
            inference_ms: runtime
                .inference_ms
                .map(|value| (value * 10.0).round() / 10.0),
            inference_fps: runtime
                .inference_fps
                .map(|value| (value * 10.0).round() / 10.0),
            dropped_frames: runtime.dropped_frames,
            counts: runtime.counts.clone(),
        }
    }

    pub fn reset_counter(&self) {
        self.shared.counter.lock().expect("counter lock").reset();
        self.shared.runtime(|runtime| {
            runtime.counts.unique_total = runtime.counts.current_total;
            runtime.counts.unique_by_class = runtime.counts.current_by_class.clone();
        });
    }

    /// Stops inference on app exit. The tap FFmpeg is also stopped by the
    /// supervisor's shutdown; this releases the model first.
    ///
    /// Never waits for a model that is still loading (large ones can take
    /// minutes): app exit must stop every child promptly. A load in progress sees
    /// `shutting_down` and starts nothing.
    pub async fn shutdown(&self) {
        self.shared.shutting_down.store(true, Ordering::SeqCst);
        self.shared.tap_lifecycle.close().await;
        let Ok(mut control) = self.control.try_lock() else {
            return;
        };
        control.parked = None;
        if let Some(pipeline) = control.pipeline.take() {
            pipeline.stop(&self.supervisor).await;
        }
    }

    /// Brings the pipeline in line with the settings, the model cache and
    /// whether a drone is publishing.
    async fn reconcile(&self) {
        let mut control = self.control.lock().await;
        let settings = self.shared.settings();
        let spec = models::find(&settings.model_id)
            .or_else(|| models::find(models::default_model_id()))
            .expect("the default model is always available");
        let ready = self.shared.store.is_ready(spec);
        let wanted = settings.enabled && control.publisher_present && ready;

        if control
            .pipeline
            .as_ref()
            .is_some_and(|pipeline| !wanted || pipeline.model_id != spec.id)
            && let Some(pipeline) = control.pipeline.take()
        {
            let model_id = pipeline.model_id;
            let detector = pipeline.stop(&self.supervisor).await;
            if settings.enabled && model_id == spec.id {
                control.parked = detector.map(|detector| (model_id, detector));
            }
            self.shared
                .publish(Some((Vec::new(), CountSummary::default(), &[])));
            self.shared.runtime(|runtime| {
                runtime.backend = None;
                runtime.inference_ms = None;
                runtime.inference_fps = None;
            });
        }

        if control
            .parked
            .as_ref()
            .is_some_and(|(model_id, _)| !settings.enabled || *model_id != spec.id)
        {
            control.parked = None;
        }

        if !settings.enabled {
            self.shared.runtime(|runtime| {
                *runtime = Runtime {
                    counts: std::mem::take(&mut runtime.counts),
                    ..Runtime::default()
                };
            });
            return;
        }
        if !ready {
            if !control.downloading {
                control.downloading = true;
                self.download(spec);
            }
            return;
        }
        if !control.publisher_present {
            self.shared.runtime(|runtime| {
                runtime.status = VisionStatus::WaitingForVideo;
                runtime.detail = None;
            });
            return;
        }
        if control.pipeline.is_some() {
            return;
        }
        self.shared.runtime(|runtime| {
            runtime.status = VisionStatus::Loading;
            runtime.detail = None;
        });
        let parked = control.parked.take().map(|(_, detector)| detector);
        match Pipeline::start(self.shared.clone(), &self.supervisor, spec, parked).await {
            Ok(pipeline) => {
                control.pipeline = Some(pipeline);
                self.shared
                    .runtime(|runtime| runtime.status = VisionStatus::Running);
            }
            Err(error) => {
                tracing::warn!(%error, "AI Vision could not start");
                self.shared.runtime(|runtime| {
                    runtime.status = VisionStatus::Failed;
                    runtime.detail = Some(error.to_string());
                });
            }
        }
    }

    fn download(&self, spec: &'static ModelSpec) {
        let engine = self.clone();
        tauri::async_runtime::spawn(async move {
            engine.shared.runtime(|runtime| {
                runtime.status = VisionStatus::Downloading;
                runtime.detail = None;
                runtime.download = Some(DownloadProgress {
                    received_bytes: 0,
                    total_bytes: spec.size_bytes,
                });
            });
            let shared = engine.shared.clone();
            let result = engine
                .shared
                .store
                .download(spec, |received, total| {
                    shared.runtime(|runtime| {
                        runtime.download = Some(DownloadProgress {
                            received_bytes: received,
                            total_bytes: total,
                        });
                    });
                })
                .await;
            engine.control.lock().await.downloading = false;
            engine.shared.runtime(|runtime| runtime.download = None);
            match result {
                Ok(_) => engine.reconcile().await,
                Err(error) => {
                    tracing::warn!(%error, model = spec.id, "model download failed");
                    engine.shared.runtime(|runtime| {
                        runtime.status = VisionStatus::Failed;
                        runtime.detail = Some(error.to_string());
                    });
                }
            }
        });
    }
}

fn class_names(settings: &DetectionSettings) -> Vec<String> {
    let classes = match models::find(&settings.model_id) {
        Some(spec) => settings.profile.classes_of(spec.labels),
        None => settings.profile.classes().to_vec(),
    };
    classes.into_iter().map(str::to_string).collect()
}

/// A running tap and inference worker for one model.
struct Pipeline {
    model_id: &'static str,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<Box<dyn ObjectDetector>>>,
    tap: FrameTap,
}

impl Pipeline {
    async fn start(
        shared: Arc<Shared>,
        supervisor: &ProcessSupervisor,
        spec: &'static ModelSpec,
        parked: Option<Box<dyn ObjectDetector>>,
    ) -> BridgeResult<Self> {
        // Load before decoding anything: a model that cannot load must not
        // leave an FFmpeg running for nothing.
        let detector: Box<dyn ObjectDetector> = match parked {
            Some(detector) => detector,
            None => {
                let path = shared.store.path(spec);
                let cache = shared.store.compiled_cache_dir(spec);
                Box::new(
                    tokio::task::spawn_blocking(move || OnnxDetector::load(spec, &path, &cache))
                        .await
                        .map_err(|error| {
                            BridgeError::Vision(format!("model loading task failed: {error}"))
                        })??,
                )
            }
        };
        if shared.shutting_down.load(Ordering::SeqCst) {
            return Err(BridgeError::Vision("the app is quitting".into()));
        }
        shared.runtime(|runtime| runtime.backend = Some(detector.backend()));

        let slot = Arc::new(FrameSlot::default());
        slot.request_fps(
            shared
                .settings()
                .inference_rate
                .tap_fps(0.0, detector.backend() == "CoreML"),
        );
        let tap = FrameTap::start(
            supervisor,
            detector.input_size(),
            spec.resize,
            slot.clone(),
            shared.tap_lifecycle.clone(),
        )
        .await?;
        let stop = Arc::new(AtomicBool::new(false));
        let worker = {
            let stop = stop.clone();
            std::thread::Builder::new()
                .name("vision-worker".into())
                .spawn(move || run_worker(&shared, detector, spec, &slot, &stop))?
        };
        Ok(Self {
            model_id: spec.id,
            stop,
            worker: Some(worker),
            tap,
        })
    }

    /// Stops the tap and the worker and hands back the loaded model; the
    /// caller drops it (releasing its memory) or parks it for a reconnect.
    async fn stop(mut self, supervisor: &ProcessSupervisor) -> Option<Box<dyn ObjectDetector>> {
        self.stop.store(true, Ordering::Relaxed);
        self.tap.stop(supervisor).await;
        let worker = self.worker.take()?;
        tokio::task::spawn_blocking(move || worker.join())
            .await
            .ok()?
            .ok()
    }
}

fn run_worker(
    shared: &Shared,
    mut detector: Box<dyn ObjectDetector>,
    spec: &'static ModelSpec,
    slot: &FrameSlot,
    stop: &AtomicBool,
) -> Box<dyn ObjectDetector> {
    let mut tracker = ObjectTracker::new(
        TrackerConfig::new(shared.settings().confidence_threshold, 0.0),
        shared.next_track_id.load(Ordering::Relaxed),
    );
    let mut average_seconds: Option<f64> = None;
    let mut last_start: Option<Instant> = None;
    let mut last_picture = Instant::now();
    let mut completed: Vec<Instant> = Vec::new();
    let mut reported_empty = true;
    let accelerated = detector.backend() == "CoreML";

    while !stop.load(Ordering::Relaxed) {
        let settings = shared.settings();
        slot.request_fps(
            settings
                .inference_rate
                .tap_fps(average_seconds.unwrap_or(0.0), accelerated),
        );
        if let Some(started) = last_start {
            let interval = Duration::from_secs_f64(
                settings
                    .inference_rate
                    .interval_seconds(average_seconds.unwrap_or(0.0), accelerated),
            );
            let remaining = interval.saturating_sub(started.elapsed());
            if !remaining.is_zero() {
                std::thread::sleep(remaining.min(Duration::from_millis(100)));
                continue;
            }
        }
        let Some(frame) = slot.take(Duration::from_millis(250)) else {
            // No video (the drone paused or reconnects): nothing is visible.
            if last_picture.elapsed() > Duration::from_millis(1500) {
                tracker.clear();
                shared.runtime(|runtime| {
                    runtime.inference_fps = Some(0.0);
                    runtime.counts.current_total = 0;
                    runtime.counts.current_by_class.clear();
                });
                if !reported_empty {
                    let counts = shared
                        .runtime
                        .lock()
                        .expect("vision runtime lock")
                        .counts
                        .clone();
                    shared.publish(Some((Vec::new(), counts, spec.labels)));
                    emit(shared, &[], spec.labels);
                    reported_empty = true;
                }
            }
            continue;
        };
        last_picture = Instant::now();
        let started = Instant::now();
        let interval = last_start.map_or(0.0, |last| started.duration_since(last).as_secs_f64());
        last_start = Some(started);

        let active = settings.active_classes();
        let wanted: Vec<bool> = detector
            .labels()
            .iter()
            .map(|label| active.contains(label))
            .collect();
        let options = DetectOptions {
            // Weak candidates are kept for the tracker's second pass.
            min_confidence: (settings.confidence_threshold * 0.5).clamp(0.05, 0.3),
            wanted: &wanted,
        };
        let result = detector.detect(&frame, &options);
        slot.recycle(frame);
        let detections = match result {
            Ok(detections) => detections,
            Err(error) => {
                tracing::warn!(%error, "AI Vision inference failed");
                shared.runtime(|runtime| runtime.detail = Some(error.to_string()));
                continue;
            }
        };
        let elapsed = started.elapsed().as_secs_f64();
        average_seconds =
            Some(average_seconds.map_or(elapsed, |average| average * 0.9 + elapsed * 0.1));

        tracker.set_config(TrackerConfig::new(settings.confidence_threshold, interval));
        let now = clock();
        tracker.update(&detections, now);
        let visible = tracker.visible(now);
        let counts = shared
            .counter
            .lock()
            .expect("counter lock")
            .update(&visible, spec.labels);
        if !(visible.is_empty() && reported_empty) {
            emit(shared, &visible, spec.labels);
        }
        reported_empty = visible.is_empty();
        shared.publish(Some((visible, counts.clone(), spec.labels)));

        // Over three seconds, so a model doing one picture per second does
        // not read as alternating 0 and 1.
        completed.push(Instant::now());
        completed.retain(|at| at.elapsed() <= Duration::from_secs(3));
        let dropped = slot.dropped();
        shared.runtime(|runtime| {
            runtime.inference_ms = average_seconds.map(|seconds| seconds * 1000.0);
            runtime.inference_fps = Some(completed.len() as f64 / 3.0);
            runtime.dropped_frames = dropped;
            runtime.counts = counts;
            runtime.detail = None;
        });
    }
    shared
        .next_track_id
        .store(tracker.next_id(), Ordering::Relaxed);
    detector
}

fn emit(shared: &Shared, tracks: &[TrackView], labels: &[&str]) {
    let Some(app) = shared.app.get() else {
        return;
    };
    let size = *shared.source_size.lock().expect("source size lock");
    let detections = tracks
        .iter()
        .map(|track| DetectionResult {
            class_name: labels
                .get(track.class_id)
                .copied()
                .unwrap_or("object")
                .into(),
            confidence: (track.confidence * 100.0).round() / 100.0,
            bbox: size.map(|(width, height)| PixelBox {
                x: (track.bbox.x * width as f32).round() as u32,
                y: (track.bbox.y * height as f32).round() as u32,
                width: (track.bbox.width * width as f32).round() as u32,
                height: (track.bbox.height * height as f32).round() as u32,
            }),
            normalized_bbox: track.bbox,
            track_id: track.id,
        })
        .collect();
    let event = DetectionEvent {
        timestamp_ms: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64,
        frame_width: size.map(|(width, _)| width),
        frame_height: size.map(|(_, height)| height),
        detections,
    };
    if let Err(error) = app.emit(DETECTIONS_EVENT, event) {
        tracing::debug!(%error, "could not emit detections");
    }
}

/// Parses ffprobe's "1920x1080".
pub fn parse_resolution(value: Option<&str>) -> Option<(u32, u32)> {
    let (width, height) = value?.split_once('x')?;
    Some((width.parse().ok()?, height.parse().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolution_parsing() {
        assert_eq!(parse_resolution(Some("1920x1080")), Some((1920, 1080)));
        assert_eq!(parse_resolution(Some("bad")), None);
        assert_eq!(parse_resolution(None), None);
    }

    #[test]
    fn detection_result_serialises_in_the_documented_shape() {
        let result = DetectionResult {
            class_name: "cow".into(),
            confidence: 0.94,
            bbox: Some(PixelBox {
                x: 420,
                y: 180,
                width: 210,
                height: 160,
            }),
            normalized_bbox: BoundingBox {
                x: 0.2,
                y: 0.2,
                width: 0.1,
                height: 0.1,
            },
            track_id: 12,
        };
        let value = serde_json::to_value(result).unwrap();
        assert_eq!(value["class"], "cow");
        assert_eq!(value["trackId"], 12);
        assert_eq!(value["bbox"]["x"], 420);
        assert_eq!(value["bbox"]["height"], 160);
    }

    #[test]
    fn overlay_is_only_offered_while_enabled() {
        let engine = VisionEngine::new(
            &std::env::temp_dir(),
            ProcessSupervisor::default(),
            DetectionSettings::default(),
        );
        assert!(engine.overlay_input(Some((1920, 1080))).is_none());
        *engine.shared.settings.write().unwrap() = DetectionSettings {
            enabled: true,
            ..DetectionSettings::default()
        };
        let input = engine.overlay_input(Some((1919, 1080))).unwrap();
        assert_eq!((input.width, input.height), (1918, 1080));
        // The remembered size, and its listener, are reused.
        assert_eq!(engine.overlay_input(None).unwrap().port, input.port);
    }
}
