use jni::objects::{JByteArray, JClass, JString};
use jni::sys::{jbyteArray, jint, jlong, jstring};
use jni::JNIEnv;
use librtmp2::client::Client;
use librtmp2::server::Server;
use librtmp2::types::{Frame, FrameType, ServerConfig};
use serde::Serialize;
use std::collections::VecDeque;
use std::ptr;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Mutex, OnceLock, RwLock};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

#[cfg(target_vendor = "apple")]
mod apple_trust;
pub mod ffi;
pub mod phone_audio;
mod preview;

const DEFAULT_BIND_ADDRESS: &str = "0.0.0.0:1935";
const EXPECTED_APP: &str = "drone";
const OUTBOUND_QUEUE_CAPACITY: usize = 256;
/// The output id of the single-destination `start_bridge*` entry points.
const BRIDGE_OUTPUT_ID: &str = "bridge";
const MAX_QUEUED_FRAME_BYTES: usize = 16 * 1024 * 1024;
const OUTPUT_CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
const OUTPUT_PUBLISH_TIMEOUT: Duration = Duration::from_secs(10);
const OUTPUT_RETRY_MAX_DELAY: Duration = Duration::from_secs(15);
/// How long the platform connection stays open while the drone reconnects. DJI Fly usually
/// returns within a few seconds after a Wi-Fi drop; closing at once can end the broadcast.
const SOURCE_HOLD: Duration = Duration::from_secs(20);
/// Unsent output (ours plus the kernel's) beyond this much stream time means the uplink cannot
/// keep up: video is skipped to the next keyframe so the delay cannot grow without bound.
const CONGESTION_SECONDS: f64 = 1.5;
/// Never below this, so one large keyframe on a slow stream does not count as congestion.
const MIN_CONGESTION_BYTES: usize = 512 * 1024;
/// A platform connection whose unsent output has not shrunk for this long is treated as dead.
const OUTPUT_STALL_TIMEOUT: Duration = Duration::from_secs(10);
/// Every platform gets the drone's frames this long after their place on the drone's timeline,
/// at the pace they were filmed. A pause in what DJI Fly sends (Wi-Fi roaming, a weak link) up
/// to this long is then absorbed here and never reaches the platform as a gap.
const OUTPUT_DELAY: Duration = Duration::from_secs(3);
/// Arrival jitter the pacer tolerates before it moves its reference point.
const PACER_TOLERANCE: Duration = Duration::from_millis(500);
/// When every frame in this window came late, the reference moves so the full delay is back.
const PACER_WINDOW: Duration = Duration::from_secs(10);
/// A frame this far from the reference belongs to a timeline that jumped: start over at it.
const PACER_JUMP: Duration = Duration::from_secs(20);
/// Beyond this much held payload frames leave at once, whatever their time.
const PACER_MAX_BYTES: usize = 32 * 1024 * 1024;
/// On "End broadcast" the held frames are still sent for at most this long.
const PACER_DRAIN_LIMIT: Duration = Duration::from_secs(4);
/// How often the ingest rebuilds the status snapshot the app polls.
const SNAPSHOT_INTERVAL: Duration = Duration::from_millis(250);
/// Nice values for the media threads (Android's THREAD_PRIORITY_URGENT_DISPLAY is -8).
const INGEST_THREAD_NICE: i32 = -8;
const OUTPUT_THREAD_NICE: i32 = -8;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RelaySnapshot {
    pub status: String,
    /// Why the receiver stopped, as a code the app turns into its own language.
    pub error_code: Option<String>,
    /// The technical cause behind [error_code], in English (e.g. the operating system's error).
    pub error_detail: Option<String>,
    pub remote_address: Option<String>,
    pub received_bytes: u64,
    pub bitrate_kbps: f64,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    pub video_frames: u64,
    pub audio_frames: u64,
    pub rejected_publish_attempts: u64,
    /// Gaps of at least [STALL_THRESHOLD] in the drone's video: the picture froze that long.
    pub stalls: u64,
    pub longest_stall_ms: u64,
    /// Times DJI Fly reconnected since the receiver opened.
    pub source_reconnects: u64,
    /// Stalls and reconnects in the last minute, for the app's weak-link tip.
    pub recent_interruptions: u64,
    /// The phone's microphone replaces DJI Fly's audio.
    pub phone_audio: bool,
    /// Microphone frames sent on the drone's timeline since the app started.
    pub phone_audio_frames: u64,
    /// One entry per platform the stream is going to.
    pub outputs: Vec<OutputSnapshot>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutputSnapshot {
    pub id: String,
    pub status: String,
    /// While reconnecting: why, as a code, with the technical cause and the wait before retrying.
    pub reason: Option<String>,
    pub reason_detail: Option<String>,
    pub retry_in_seconds: Option<u64>,
    pub outbound_bytes: u64,
    pub dropped_frames: u64,
    pub reconnect_attempts: u64,
    pub secure: bool,
}

impl RelaySnapshot {
    fn stopped() -> Self {
        Self {
            status: "stopped".to_owned(),
            error_code: None,
            error_detail: None,
            remote_address: None,
            received_bytes: 0,
            bitrate_kbps: 0.0,
            video_codec: None,
            audio_codec: None,
            video_frames: 0,
            audio_frames: 0,
            rejected_publish_attempts: 0,
            stalls: 0,
            longest_stall_ms: 0,
            source_reconnects: 0,
            recent_interruptions: 0,
            phone_audio: false,
            phone_audio_frames: 0,
            outputs: Vec::new(),
        }
    }
}

/// A gap this long between two video frames of one source is a freeze the viewer notices.
const STALL_THRESHOLD: Duration = Duration::from_millis(700);
/// Stalls and reconnects this recent count toward the app's weak-link tip.
const RECENT_INTERRUPTION_WINDOW: Duration = Duration::from_secs(60);

/// How smoothly the drone's stream arrives: gaps in its video, and DJI Fly reconnecting. Both
/// come from the Wi-Fi between the remote controller and the phone (or the drone's own link),
/// so they tell a weak link apart from a problem on the phone.
#[derive(Default)]
struct IngestHealth {
    last_video_at: Option<Instant>,
    stalls: u64,
    longest_stall: Duration,
    sources: u64,
    recent: VecDeque<Instant>,
}

impl IngestHealth {
    fn video_arrived(&mut self, now: Instant) {
        if let Some(last) = self.last_video_at {
            let gap = now.saturating_duration_since(last);
            if gap >= STALL_THRESHOLD {
                self.stalls += 1;
                self.longest_stall = self.longest_stall.max(gap);
                self.recent.push_back(now);
            }
        }
        self.last_video_at = Some(now);
    }

    /// A publisher started sending: the receiver's first, or DJI Fly reconnecting.
    fn source_started(&mut self, now: Instant) {
        self.sources += 1;
        if self.sources > 1 {
            self.recent.push_back(now);
        }
        // The gap across a reconnect is the reconnect, not a stall.
        self.last_video_at = None;
    }

    fn source_ended(&mut self) {
        self.last_video_at = None;
    }

    fn source_reconnects(&self) -> u64 {
        self.sources.saturating_sub(1)
    }

    fn recent_interruptions(&mut self, now: Instant) -> u64 {
        while self
            .recent
            .front()
            .is_some_and(|at| now.saturating_duration_since(*at) > RECENT_INTERRUPTION_WINDOW)
        {
            self.recent.pop_front();
        }
        self.recent.len() as u64
    }
}

#[derive(Clone)]
struct Destination {
    url: String,
    secure: bool,
    tls_ca_file: Option<String>,
}

/// Why something failed: a code the app puts into words, and the technical cause in English.
#[derive(Clone, Debug)]
struct Failure {
    reason: &'static str,
    detail: String,
}

impl Failure {
    fn new(reason: &'static str, detail: impl ToString) -> Self {
        Self {
            reason,
            detail: detail.to_string(),
        }
    }
}

#[derive(Clone, Default)]
struct OutputState {
    status: String,
    failure: Option<Failure>,
    retry_in_seconds: Option<u64>,
}

/// One platform output: its own connection, retries, congestion state and counters.
struct Output {
    id: String,
    secure: bool,
    state: RwLock<OutputState>,
    outbound_bytes: AtomicU64,
    dropped_frames: AtomicU64,
    reconnect_attempts: AtomicU64,
    /// Frames for this output were lost before it could read them.
    overflowed: AtomicBool,
}

impl Output {
    fn new(id: &str, secure: bool) -> Self {
        Self {
            id: id.to_owned(),
            secure,
            state: RwLock::new(OutputState {
                status: "armed".to_owned(),
                ..OutputState::default()
            }),
            outbound_bytes: AtomicU64::new(0),
            dropped_frames: AtomicU64::new(0),
            reconnect_attempts: AtomicU64::new(0),
            overflowed: AtomicBool::new(false),
        }
    }

    fn set_status(&self, status: &str) {
        if let Ok(mut current) = self.state.write() {
            // A retry in progress still shows why the previous attempt failed.
            let failure = if status == "reconnecting" {
                current.failure.take()
            } else {
                None
            };
            *current = OutputState {
                status: status.to_owned(),
                failure,
                retry_in_seconds: None,
            };
        }
    }

    fn set_reconnecting(&self, failure: Failure, delay: Duration) {
        if let Ok(mut current) = self.state.write() {
            *current = OutputState {
                status: "reconnecting".to_owned(),
                failure: Some(failure),
                retry_in_seconds: Some(delay.as_secs()),
            };
        }
    }

    fn snapshot(&self) -> OutputSnapshot {
        let state = self
            .state
            .read()
            .map(|value| value.clone())
            .unwrap_or_default();
        OutputSnapshot {
            id: self.id.clone(),
            status: state.status,
            reason: state
                .failure
                .as_ref()
                .map(|failure| failure.reason.to_owned()),
            reason_detail: state
                .failure
                .map(|failure| failure.detail)
                .filter(|detail| !detail.is_empty()),
            retry_in_seconds: state.retry_in_seconds,
            outbound_bytes: self.outbound_bytes.load(Ordering::Relaxed),
            dropped_frames: self.dropped_frames.load(Ordering::Relaxed),
            reconnect_attempts: self.reconnect_attempts.load(Ordering::Relaxed),
            secure: self.secure,
        }
    }
}

/// A frame for a platform output, tagged with the source it belongs to, so a source change is
/// noticed in order even when the queue overflowed. Every output shares the same payload.
struct OutboundFrame {
    source: u64,
    frame_type: FrameType,
    timestamp: u32,
    payload: Arc<[u8]>,
}

struct RuntimeControl {
    stop: Arc<AtomicBool>,
    server_thread: JoinHandle<()>,
}

/// A running platform output. Outputs come and go while the ingest keeps the drone connected.
struct OutputHandle {
    output: Arc<Output>,
    sender: SyncSender<OutboundFrame>,
    stop: Arc<AtomicBool>,
    thread: JoinHandle<()>,
}

static CONTROL: OnceLock<Mutex<Option<RuntimeControl>>> = OnceLock::new();
static OUTPUTS: OnceLock<RwLock<Vec<OutputHandle>>> = OnceLock::new();
/// Codec headers and metadata of the current source, so an output attached mid-stream can
/// start with them instead of waiting for a source restart.
static SOURCE_BOOTSTRAP: OnceLock<Mutex<MediaBootstrap>> = OnceLock::new();
static SNAPSHOT: OnceLock<RwLock<RelaySnapshot>> = OnceLock::new();
static INGEST_HEALTH: OnceLock<Mutex<IngestHealth>> = OnceLock::new();
static VIDEO_FRAMES: AtomicU64 = AtomicU64::new(0);
static AUDIO_FRAMES: AtomicU64 = AtomicU64::new(0);
static REJECTED_PUBLISH_ATTEMPTS: AtomicU64 = AtomicU64::new(0);
/// Grows whenever the source ends or another publisher takes over.
static SOURCE_GENERATION: AtomicU64 = AtomicU64::new(0);
/// A publisher is sending right now.
static SOURCE_ACTIVE: AtomicBool = AtomicBool::new(false);
/// The connection whose frames are relayed; frames of a connection it replaced are dropped.
static CURRENT_PUBLISHER: AtomicU64 = AtomicU64::new(0);

fn control() -> &'static Mutex<Option<RuntimeControl>> {
    CONTROL.get_or_init(|| Mutex::new(None))
}

fn ingest_health() -> &'static Mutex<IngestHealth> {
    INGEST_HEALTH.get_or_init(|| Mutex::new(IngestHealth::default()))
}

fn outputs() -> &'static RwLock<Vec<OutputHandle>> {
    OUTPUTS.get_or_init(|| RwLock::new(Vec::new()))
}

fn source_bootstrap() -> &'static Mutex<MediaBootstrap> {
    SOURCE_BOOTSTRAP.get_or_init(|| Mutex::new(MediaBootstrap::default()))
}

fn reset_source_bootstrap() {
    if let Ok(mut bootstrap) = source_bootstrap().lock() {
        *bootstrap = MediaBootstrap::default();
    }
}

fn snapshot() -> &'static RwLock<RelaySnapshot> {
    SNAPSHOT.get_or_init(|| RwLock::new(RelaySnapshot::stopped()))
}

fn set_snapshot(value: RelaySnapshot) {
    if let Ok(mut current) = snapshot().write() {
        *current = value;
    }
}

fn apply_output_state(value: &mut RelaySnapshot) {
    value.outputs = outputs()
        .read()
        .map(|handles| {
            handles
                .iter()
                .map(|handle| handle.output.snapshot())
                .collect()
        })
        .unwrap_or_default();
}

fn allow_publish(_conn_id: u64, app: &str, stream_name: &str) -> bool {
    let allowed = app == EXPECTED_APP && stream_name.is_empty();

    if !allowed {
        REJECTED_PUBLISH_ATTEMPTS.fetch_add(1, Ordering::Relaxed);
    }
    allowed
}

fn handle_frame(frame: &Frame) {
    // A connection that was taken over (see run_server) may still deliver a late frame; the
    // first frame of a new publisher starts a new source for the preview and the output.
    let publisher = frame.publisher_conn_id;
    let current = CURRENT_PUBLISHER.load(Ordering::Relaxed);
    if publisher < current {
        return;
    }
    if publisher != current {
        if current != 0 {
            reset_source();
        }
        CURRENT_PUBLISHER.store(publisher, Ordering::Relaxed);
        if let Ok(mut health) = ingest_health().lock() {
            health.source_started(Instant::now());
        }
    }

    match frame.frame_type {
        FrameType::Video => {
            VIDEO_FRAMES.fetch_add(1, Ordering::Relaxed);
            if let Ok(mut health) = ingest_health().lock() {
                health.video_arrived(Instant::now());
            }
        }
        FrameType::Audio => {
            AUDIO_FRAMES.fetch_add(1, Ordering::Relaxed);
        }
        _ => {}
    }

    if frame.size as usize > MAX_QUEUED_FRAME_BYTES || (frame.size > 0 && frame.data.is_null()) {
        if let Ok(handles) = outputs().read() {
            for handle in handles.iter() {
                handle.output.dropped_frames.fetch_add(1, Ordering::Relaxed);
                handle.output.overflowed.store(true, Ordering::Release);
            }
        }
        return;
    }

    let payload: &[u8] = if frame.size == 0 {
        &[]
    } else {
        // librtmp2 guarantees Frame.data remains valid for the duration of this callback.
        unsafe { std::slice::from_raw_parts(frame.data, frame.size as usize) }
    };
    let source = SOURCE_GENERATION.load(Ordering::Relaxed);
    match frame.frame_type {
        FrameType::Video => {
            preview::offer(frame.timestamp, payload);
            phone_audio::video_arrived(source, frame.timestamp);
        }
        // The phone's microphone may stand in for DJI Fly's audio.
        FrameType::Audio if !phone_audio::pass_drone_audio(source, frame.timestamp, payload) => {
            return;
        }
        _ => {}
    }
    // Observed before the outputs are read: see attach_destination.
    if let Ok(mut bootstrap) = source_bootstrap().lock() {
        bootstrap.observe(frame.frame_type, payload);
    }

    let Ok(handles) = outputs().read() else {
        return;
    };
    send_to_outputs(&handles, source, frame.frame_type, frame.timestamp, payload);
}

/// Relays frames the app made itself (the phone's microphone) as part of source `generation`;
/// false when that source has ended meanwhile and the frames were dropped.
pub(crate) fn inject_frames(generation: u64, frames: &[(FrameType, u32, &[u8])]) -> bool {
    // Under the bootstrap lock, as reset_source changes the generation under it: no output can
    // see these frames after the first frame of a newer source.
    let Ok(mut bootstrap) = source_bootstrap().lock() else {
        return false;
    };
    if SOURCE_GENERATION.load(Ordering::Relaxed) != generation {
        return false;
    }
    for &(frame_type, _, payload) in frames {
        bootstrap.observe(frame_type, payload);
    }
    if let Ok(handles) = outputs().read() {
        for &(frame_type, timestamp, payload) in frames {
            send_to_outputs(&handles, generation, frame_type, timestamp, payload);
        }
    }
    true
}

fn send_to_outputs(
    handles: &[OutputHandle],
    source: u64,
    frame_type: FrameType,
    timestamp: u32,
    payload: &[u8],
) {
    if handles.is_empty() {
        return;
    }
    let shared: Arc<[u8]> = Arc::from(payload);
    for handle in handles {
        let message = OutboundFrame {
            source,
            frame_type,
            timestamp,
            payload: Arc::clone(&shared),
        };
        match handle.sender.try_send(message) {
            Ok(()) => {}
            // A slow platform only loses its own frames; the others are unaffected.
            Err(TrySendError::Full(_)) => {
                handle.output.dropped_frames.fetch_add(1, Ordering::Relaxed);
                handle.output.overflowed.store(true, Ordering::Release);
            }
            Err(TrySendError::Disconnected(_)) => {}
        }
    }
}

fn notify_source_ended() {
    CURRENT_PUBLISHER.store(0, Ordering::Relaxed);
    reset_source();
    if let Ok(mut health) = ingest_health().lock() {
        health.source_ended();
    }
}

/// Forgets everything learned from the previous source before a new one can start.
fn reset_source() {
    // Under the bootstrap lock, so attach_destination sees headers and generation together.
    if let Ok(mut bootstrap) = source_bootstrap().lock() {
        SOURCE_GENERATION.fetch_add(1, Ordering::Relaxed);
        *bootstrap = MediaBootstrap::default();
    }
    preview::source_ended();
}

/// Raises the calling thread's scheduling priority; best effort, the default works too.
fn raise_thread_priority(nice: i32) {
    #[cfg(any(target_os = "android", target_os = "linux"))]
    // SAFETY: setpriority only reads its arguments; on Linux, PRIO_PROCESS with 0 means the
    // calling thread.
    unsafe {
        libc::setpriority(libc::PRIO_PROCESS, 0, nice);
    }
    // Apple systems schedule by quality of service; user-interactive is what the display uses.
    #[cfg(target_vendor = "apple")]
    // SAFETY: only changes the calling thread's own QoS class.
    unsafe {
        libc::pthread_set_qos_class_self_np(libc::qos_class_t::QOS_CLASS_USER_INTERACTIVE, 0);
    }
    let _ = nice;
}

/// Whether RTMPS can be verified without a CA file: Apple systems check the chain themselves
/// (see `apple_trust`); elsewhere the app passes its system CAs as a file.
fn platform_trust() -> bool {
    cfg!(target_vendor = "apple")
}

fn build_destination(
    server_url: &str,
    stream_key: &str,
    tls_ca_file: &str,
) -> Result<Destination, String> {
    let server_url = server_url.trim().trim_end_matches('/');
    let (authority_and_app, secure) = if let Some(value) = server_url.strip_prefix("rtmps://") {
        (value, true)
    } else if let Some(value) = server_url.strip_prefix("rtmp://") {
        (value, false)
    } else {
        return Err("invalid_destination_scheme".to_owned());
    };
    let Some((authority, app)) = authority_and_app.split_once('/') else {
        return Err("invalid_destination_path".to_owned());
    };
    if authority.is_empty()
        || app.is_empty()
        || authority.contains('@')
        || server_url.contains(['?', '#'])
        || server_url.chars().any(char::is_whitespace)
    {
        return Err("invalid_destination_server".to_owned());
    }
    if !(4..=512).contains(&stream_key.len())
        || !stream_key.is_ascii()
        || stream_key.bytes().any(|value| {
            value.is_ascii_whitespace() || value.is_ascii_control() || matches!(value, b'/' | b'#')
        })
    {
        return Err("invalid_stream_key".to_owned());
    }
    let tls_ca_file = tls_ca_file.trim();
    if secure && tls_ca_file.is_empty() && !platform_trust() {
        return Err("missing_ca_bundle".to_owned());
    }

    Ok(Destination {
        url: format!("{server_url}/{stream_key}"),
        secure,
        tls_ca_file: (secure && !tls_ca_file.is_empty()).then(|| tls_ca_file.to_owned()),
    })
}

pub fn start_server() -> Result<(), String> {
    start_server_on(DEFAULT_BIND_ADDRESS)
}

pub fn start_server_on(bind_address: &str) -> Result<(), String> {
    start_runtime(bind_address, None)
}

pub fn start_bridge(target_server_url: &str, target_stream_key: &str) -> Result<(), String> {
    start_bridge_on_with_tls_ca(
        DEFAULT_BIND_ADDRESS,
        target_server_url,
        target_stream_key,
        "",
    )
}

pub fn start_bridge_with_tls_ca(
    target_server_url: &str,
    target_stream_key: &str,
    tls_ca_file: &str,
) -> Result<(), String> {
    start_bridge_on_with_tls_ca(
        DEFAULT_BIND_ADDRESS,
        target_server_url,
        target_stream_key,
        tls_ca_file,
    )
}

pub fn start_bridge_on(
    bind_address: &str,
    target_server_url: &str,
    target_stream_key: &str,
) -> Result<(), String> {
    start_bridge_on_with_tls_ca(bind_address, target_server_url, target_stream_key, "")
}

pub fn start_bridge_on_with_tls_ca(
    bind_address: &str,
    target_server_url: &str,
    target_stream_key: &str,
    tls_ca_file: &str,
) -> Result<(), String> {
    let destination = build_destination(target_server_url, target_stream_key, tls_ca_file)?;
    start_runtime(bind_address, Some(destination))
}

/// Starts sending the running ingest's stream to one more platform, known by `output_id` (an
/// output with the same id is replaced). The drone stays connected, other platforms are left
/// alone, and a stream that is already arriving goes out from its next keyframe.
pub fn set_destination_with_tls_ca(
    output_id: &str,
    target_server_url: &str,
    target_stream_key: &str,
    tls_ca_file: &str,
) -> Result<(), String> {
    let destination = build_destination(target_server_url, target_stream_key, tls_ca_file)?;
    attach_destination(output_id, destination)
}

/// Stops sending to one platform; the drone and the other platforms carry on.
pub fn clear_destination(output_id: &str) {
    detach_destination(output_id);
}

/// Stops sending to every platform; the ingest keeps receiving the drone.
pub fn clear_destinations() {
    detach_all_destinations();
}

fn start_runtime(bind_address: &str, destination: Option<Destination>) -> Result<(), String> {
    stop_server();
    #[cfg(target_vendor = "apple")]
    apple_trust::install();
    {
        let mut control_slot = control()
            .lock()
            .map_err(|_| "state_lock_failed".to_owned())?;
        VIDEO_FRAMES.store(0, Ordering::Relaxed);
        AUDIO_FRAMES.store(0, Ordering::Relaxed);
        REJECTED_PUBLISH_ATTEMPTS.store(0, Ordering::Relaxed);
        if let Ok(mut health) = ingest_health().lock() {
            *health = IngestHealth::default();
        }
        reset_source_bootstrap();

        let stop = Arc::new(AtomicBool::new(false));
        let server_stop = Arc::clone(&stop);
        let bind_address = bind_address.to_owned();
        let server_thread = thread::Builder::new()
            .name("dji-rtmp-ingest".to_owned())
            .spawn(move || run_server(&bind_address, server_stop))
            .map_err(|error| format!("thread_failed: {error}"))?;
        *control_slot = Some(RuntimeControl {
            stop,
            server_thread,
        });
    }
    if let Some(destination) = destination {
        if let Err(error) = attach_destination(BRIDGE_OUTPUT_ID, destination) {
            stop_server();
            return Err(error);
        }
    }
    Ok(())
}

fn attach_destination(output_id: &str, destination: Destination) -> Result<(), String> {
    let receiving = control().lock().is_ok_and(|slot| slot.is_some());
    if !receiving {
        return Err("receiver_not_running".to_owned());
    }
    detach_destination(output_id);

    let output = Arc::new(Output::new(output_id, destination.secure));
    let (sender, receiver) = mpsc::sync_channel(OUTBOUND_QUEUE_CAPACITY);
    let stop = Arc::new(AtomicBool::new(false));
    // handle_frame records a frame in the bootstrap before it reads the outputs, so taking the
    // snapshot and adding the output under the bootstrap lock puts every frame in the
    // snapshot, in the channel, or in both.
    let bootstrap = source_bootstrap()
        .lock()
        .map_err(|_| "state_lock_failed".to_owned())?;
    let snapshot = bootstrap.clone();
    let source = SOURCE_GENERATION.load(Ordering::Relaxed);
    let thread = {
        let output = Arc::clone(&output);
        let stop = Arc::clone(&stop);
        thread::Builder::new()
            .name("dji-rtmp-output".to_owned())
            .spawn(move || run_outbound(output, destination, receiver, stop, snapshot, source))
            .map_err(|error| format!("thread_failed: {error}"))?
    };
    outputs()
        .write()
        .map_err(|_| "state_lock_failed".to_owned())?
        .push(OutputHandle {
            output,
            sender,
            stop,
            thread,
        });
    drop(bootstrap);
    Ok(())
}

fn detach_destination(output_id: &str) {
    let removed = outputs().write().ok().and_then(|mut handles| {
        let index = handles
            .iter()
            .position(|handle| handle.output.id == output_id)?;
        Some(handles.remove(index))
    });
    if let Some(handle) = removed {
        stop_output(handle);
    }
}

fn detach_all_destinations() {
    let removed = outputs()
        .write()
        .map(|mut handles| std::mem::take(&mut *handles))
        .unwrap_or_default();
    // Stopped together, so ending several platforms takes one flush, not one each.
    for handle in &removed {
        handle.stop.store(true, Ordering::Release);
    }
    for handle in removed {
        stop_output(handle);
    }
}

/// Dropping the sender and raising the stop flag ends the thread, which flushes and closes.
fn stop_output(handle: OutputHandle) {
    let OutputHandle {
        sender,
        stop,
        thread,
        ..
    } = handle;
    drop(sender);
    stop.store(true, Ordering::Release);
    let _ = thread.join();
}

fn run_server(bind_address: &str, stop: Arc<AtomicBool>) {
    raise_thread_priority(INGEST_THREAD_NICE);
    let config = ServerConfig {
        max_connections: 4,
        chunk_size: 4096,
        tls_enabled: 0,
        tls_cert_file: ptr::null(),
        tls_key_file: ptr::null(),
        tls_ca_file: ptr::null(),
        tls_insecure: 0,
        max_pending_tls_per_addr: 0,
        max_connections_per_addr: 2,
    };

    let mut server = match Server::new(config) {
        Ok(server) => server,
        Err(error) => {
            set_ingest_error("server_create_failed", error.to_string());
            return;
        }
    };
    server.on_publish_cb = Some(allow_publish);
    // DJI Fly sends releaseStream before publish; allowing it on the drone route lets a
    // reconnect take over from its own stale connection instead of waiting for it to time out.
    server.on_release_stream_cb = Some(allow_publish);
    server.on_frame_cb = Some(handle_frame);

    if let Err(error) = server.listen(bind_address) {
        set_ingest_error("listen_failed", format!("{bind_address}: {error}"));
        return;
    }

    let mut initial = RelaySnapshot {
        status: "listening".to_owned(),
        ..RelaySnapshot::stopped()
    };
    apply_output_state(&mut initial);
    set_snapshot(initial);

    let mut previous_bytes = 0u64;
    let mut previous_sample = Instant::now();
    let mut bitrate_kbps = 0.0;
    let mut source_was_publishing = false;
    let mut next_snapshot_at = Instant::now();

    while !stop.load(Ordering::Acquire) {
        if let Err(error) = server.poll(25) {
            set_ingest_error("receiver_failed", error.to_string());
            server.stop();
            return;
        }

        // The newest publisher wins; one it replaced is closed so two never interleave.
        let newest = server
            .connections
            .iter()
            .filter(|connection| is_publishing(connection))
            .map(|connection| connection.conn_id)
            .max();
        if let Some(newest) = newest {
            for connection in server.connections.iter_mut() {
                if connection.conn_id != newest && is_publishing(connection) {
                    connection.disconnect_transport();
                }
            }
        }
        let publisher = newest.and_then(|id| {
            server
                .connections
                .iter()
                .find(|connection| connection.conn_id == id)
        });
        let source_is_publishing = publisher.is_some();
        if source_was_publishing && !source_is_publishing {
            notify_source_ended();
        }
        source_was_publishing = source_is_publishing;
        SOURCE_ACTIVE.store(source_is_publishing, Ordering::Relaxed);

        let now = Instant::now();
        if now < next_snapshot_at {
            continue;
        }
        next_snapshot_at = now + SNAPSHOT_INTERVAL;

        let received_bytes = publisher
            .map(|connection| connection.media_bytes_received)
            .unwrap_or(0);
        let elapsed = now.duration_since(previous_sample).as_secs_f64();
        if elapsed >= 0.5 {
            bitrate_kbps = if received_bytes >= previous_bytes {
                (received_bytes - previous_bytes) as f64 * 8.0 / elapsed / 1000.0
            } else {
                0.0
            };
            previous_bytes = received_bytes;
            previous_sample = now;
        }

        let (status, remote_address, video_codec, audio_codec) = if let Some(connection) = publisher
        {
            (
                "publishing",
                Some(connection.remote_addr.clone()),
                connection.detected_video_codec.clone(),
                connection.detected_audio_codec.clone(),
            )
        } else if let Some(connection) = server.connections.first() {
            (
                "connected",
                Some(connection.remote_addr.clone()),
                None,
                None,
            )
        } else {
            bitrate_kbps = 0.0;
            previous_bytes = 0;
            ("listening", None, None, None)
        };

        let mut next = RelaySnapshot {
            status: status.to_owned(),
            remote_address,
            received_bytes,
            bitrate_kbps,
            video_codec,
            audio_codec,
            video_frames: VIDEO_FRAMES.load(Ordering::Relaxed),
            audio_frames: AUDIO_FRAMES.load(Ordering::Relaxed),
            rejected_publish_attempts: REJECTED_PUBLISH_ATTEMPTS.load(Ordering::Relaxed),
            phone_audio: phone_audio::is_on(),
            phone_audio_frames: phone_audio::frames_sent(),
            ..RelaySnapshot::stopped()
        };
        if let Ok(mut health) = ingest_health().lock() {
            next.stalls = health.stalls;
            next.longest_stall_ms =
                u64::try_from(health.longest_stall.as_millis()).unwrap_or(u64::MAX);
            next.source_reconnects = health.source_reconnects();
            next.recent_interruptions = health.recent_interruptions(now);
        }
        apply_output_state(&mut next);
        set_snapshot(next);
    }

    if source_was_publishing {
        notify_source_ended();
    }
    SOURCE_ACTIVE.store(false, Ordering::Relaxed);
    server.stop();
    let mut final_snapshot = current_snapshot();
    final_snapshot.status = "stopped".to_owned();
    final_snapshot.bitrate_kbps = 0.0;
    final_snapshot.remote_address = None;
    apply_output_state(&mut final_snapshot);
    set_snapshot(final_snapshot);
}

#[derive(Clone, Default)]
struct MediaBootstrap {
    script: Option<Vec<u8>>,
    metadata: Option<Vec<u8>>,
    video_sequence: Option<Vec<u8>>,
    audio_sequence: Option<Vec<u8>>,
    video_seen: bool,
}

impl MediaBootstrap {
    fn observe(&mut self, frame_type: FrameType, payload: &[u8]) {
        match frame_type {
            FrameType::Script => self.script = Some(payload.to_vec()),
            FrameType::Metadata => self.metadata = Some(payload.to_vec()),
            FrameType::Video => {
                self.video_seen = true;
                if is_video_sequence_header(payload) {
                    self.video_sequence = Some(payload.to_vec());
                }
            }
            FrameType::Audio if is_audio_sequence_header(payload) => {
                self.audio_sequence = Some(payload.to_vec());
            }
            FrameType::Audio => {}
        }
    }

    fn send_to(&self, client: &mut Client) -> Result<u64, Failure> {
        let frames = [
            (FrameType::Script, self.script.as_deref()),
            (FrameType::Metadata, self.metadata.as_deref()),
            (FrameType::Video, self.video_sequence.as_deref()),
            (FrameType::Audio, self.audio_sequence.as_deref()),
        ];
        Self::send_frames(client, &frames, 0)
    }

    /// A new source on a running connection: its metadata and codec headers at `timestamp`.
    fn send_codec_headers(&self, client: &mut Client, timestamp: u32) -> Result<u64, Failure> {
        let frames = [
            (FrameType::Metadata, self.metadata.as_deref()),
            (FrameType::Video, self.video_sequence.as_deref()),
            (FrameType::Audio, self.audio_sequence.as_deref()),
        ];
        Self::send_frames(client, &frames, timestamp)
    }

    fn send_frames(
        client: &mut Client,
        frames: &[(FrameType, Option<&[u8]>)],
        timestamp: u32,
    ) -> Result<u64, Failure> {
        let mut sent_bytes = 0u64;
        for &(frame_type, payload) in frames {
            if let Some(payload) = payload {
                send_output_frame(client, frame_type, timestamp, payload)?;
                sent_bytes = sent_bytes.saturating_add(payload.len() as u64);
            }
        }
        Ok(sent_bytes)
    }
}

fn is_publishing(connection: &librtmp2::session::conn::Conn) -> bool {
    connection
        .current_stream
        .as_ref()
        .is_some_and(|stream| stream.is_publishing)
}

fn is_video_sequence_header(payload: &[u8]) -> bool {
    payload.len() >= 2 && payload[1] == 0
}

fn is_video_keyframe(payload: &[u8]) -> bool {
    payload.first().is_some_and(|value| value >> 4 == 1) && !is_video_sequence_header(payload)
}

fn is_audio_sequence_header(payload: &[u8]) -> bool {
    payload.len() >= 2 && payload[0] >> 4 == 10 && payload[1] == 0
}

fn output_retry_delay(consecutive_failures: u32) -> Duration {
    let exponent = consecutive_failures.saturating_sub(1).min(4);
    Duration::from_secs(1u64 << exponent).min(OUTPUT_RETRY_MAX_DELAY)
}

fn connect_output(destination: &Destination) -> Result<Client, Failure> {
    let mut client = Client::new();
    client.set_connect_timeout(OUTPUT_CONNECT_TIMEOUT);
    if destination.secure {
        client.set_tls_client_config(destination.tls_ca_file.clone(), false);
    }
    client.connect(&destination.url).map_err(|error| {
        let reason = if destination.secure {
            "tls_or_network"
        } else {
            "connect_failed"
        };
        Failure::new(reason, error)
    })?;
    // The platform checks the key before it answers publish, which can take longer than connecting.
    client.set_connect_timeout(OUTPUT_PUBLISH_TIMEOUT);
    client
        .publish()
        .map_err(|error| Failure::new("publish_rejected", error))?;
    Ok(client)
}

fn send_output_frame(
    client: &mut Client,
    frame_type: FrameType,
    timestamp: u32,
    payload: &[u8],
) -> Result<(), Failure> {
    client
        .send_frame_payload(frame_type, timestamp, payload)
        .and_then(|()| client.poll(0))
        .map_err(|error| Failure::new("send_failed", error))
}

/// Estimates the stream's byte rate from what the source sends, to size the congestion limit.
struct RateMeter {
    window_start: Instant,
    window_bytes: usize,
    bytes_per_second: f64,
}

impl RateMeter {
    fn new() -> Self {
        Self {
            window_start: Instant::now(),
            window_bytes: 0,
            bytes_per_second: 0.0,
        }
    }

    fn add(&mut self, bytes: usize) {
        self.window_bytes += bytes;
        let elapsed = self.window_start.elapsed().as_secs_f64();
        if elapsed >= 1.0 {
            let rate = self.window_bytes as f64 / elapsed;
            self.bytes_per_second = if self.bytes_per_second == 0.0 {
                rate
            } else {
                0.7 * self.bytes_per_second + 0.3 * rate
            };
            self.window_start = Instant::now();
            self.window_bytes = 0;
        }
    }

    fn congestion_limit(&self) -> usize {
        ((self.bytes_per_second * CONGESTION_SECONDS) as usize).max(MIN_CONGESTION_BYTES)
    }
}

/// Bytes the output has not delivered yet: its own buffer plus the TCP send queue.
fn unsent_output_bytes(client: &Client) -> usize {
    #[allow(unused_mut)]
    let mut unsent = client.send_buffer.available();
    #[cfg(any(target_os = "android", target_os = "linux"))]
    if client.client_fd >= 0 {
        let mut queued: libc::c_int = 0;
        // SAFETY: TIOCOUTQ (SIOCOUTQ) writes one int: the bytes of the socket's send queue
        // that the peer has not acknowledged yet.
        let result = unsafe { libc::ioctl(client.client_fd, libc::TIOCOUTQ, &mut queued) };
        if result == 0 && queued > 0 {
            unsent += queued as usize;
        }
    }
    #[cfg(target_vendor = "apple")]
    if client.client_fd >= 0 {
        let mut queued: libc::c_int = 0;
        let mut length = std::mem::size_of::<libc::c_int>() as libc::socklen_t;
        // SAFETY: SO_NWRITE writes one int: the bytes in the socket's send buffer, sent or not,
        // that the peer has not acknowledged yet.
        let result = unsafe {
            libc::getsockopt(
                client.client_fd,
                libc::SOL_SOCKET,
                libc::SO_NWRITE,
                (&mut queued as *mut libc::c_int).cast(),
                &mut length,
            )
        };
        if result == 0 && queued > 0 {
            unsent += queued as usize;
        }
    }
    unsent
}

/// Notices a platform that stopped taking data: a slow uplink still drains now and then, a
/// dead connection never does.
#[derive(Default)]
struct StallWatch {
    last_unsent: usize,
    since: Option<Instant>,
}

impl StallWatch {
    fn stalled(&mut self, unsent: usize, now: Instant) -> bool {
        if unsent == 0 || unsent < self.last_unsent {
            self.since = None;
        } else if self.since.is_none() {
            self.since = Some(now);
        }
        self.last_unsent = unsent;
        self.since
            .is_some_and(|since| now.duration_since(since) >= OUTPUT_STALL_TIMEOUT)
    }
}

/// Output timestamps continue across source changes, so the platform sees one timeline.
struct OutputClock {
    /// Source timestamp that maps to `origin`.
    base: Option<u32>,
    origin: u32,
    last: u32,
}

impl OutputClock {
    fn new() -> Self {
        Self {
            base: None,
            origin: 0,
            last: 0,
        }
    }

    /// Starts a fresh platform session at zero.
    fn restart(&mut self) {
        *self = Self::new();
    }

    /// Continues after `gap` without a source, starting at a new source timestamp.
    fn resume(&mut self, source_timestamp: u32, gap: Duration) {
        let gap_ms = u32::try_from(gap.as_millis()).unwrap_or(u32::MAX).max(1);
        self.origin = self.last.wrapping_add(gap_ms);
        self.base = Some(source_timestamp);
    }

    /// The platform timestamp of a source timestamp, or None for a frame older than the start.
    fn map(&mut self, source_timestamp: u32) -> Option<u32> {
        let base = *self.base.get_or_insert(source_timestamp);
        let delta = source_timestamp.wrapping_sub(base) as i32;
        if delta < 0 {
            return None;
        }
        let output = self.origin.wrapping_add(delta as u32);
        if output.wrapping_sub(self.last) as i32 > 0 || self.last == 0 {
            self.last = output;
        }
        Some(output)
    }
}

/// Holds a platform's frames for [OUTPUT_DELAY] after the time the drone's timeline gives them,
/// and lets them go in the order they came. Frames that arrive late because the Wi-Fi paused
/// still leave on time, so the platform sees a steady stream instead of a gap and a burst.
///
/// The reference point maps a source timestamp to the moment it was expected to arrive. It
/// follows the earliest arrivals: a frame that comes clearly early moves it back at once, and
/// a window in which every frame came late moves it forward, so the full delay returns after
/// the drone's clock and the phone's drift apart. A new source or a jump in the timeline starts
/// it over.
struct Pacer {
    delay: Duration,
    frames: VecDeque<(Instant, OutboundFrame)>,
    bytes: usize,
    /// (source, wall time, source timestamp) of the reference point.
    anchor: Option<(u64, Instant, u32)>,
    window_start: Option<Instant>,
    /// The smallest lateness seen in the current window.
    window_lateness: Duration,
}

impl Pacer {
    fn new(delay: Duration) -> Self {
        Self {
            delay,
            frames: VecDeque::new(),
            bytes: 0,
            anchor: None,
            window_start: None,
            window_lateness: Duration::MAX,
        }
    }

    fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    /// When the first held frame is due.
    fn next_due(&self) -> Option<Instant> {
        self.frames.front().map(|(due, _)| *due)
    }

    fn push(&mut self, frame: OutboundFrame, now: Instant) {
        let due = self.expected(&frame, now) + self.delay;
        self.bytes += frame.payload.len();
        self.frames.push_back((due, frame));
    }

    /// The first held frame if its time has come (or too much is held).
    fn pop_due(&mut self, now: Instant) -> Option<OutboundFrame> {
        let (due, _) = self.frames.front()?;
        if *due > now && self.bytes <= PACER_MAX_BYTES {
            return None;
        }
        let (_, frame) = self.frames.pop_front()?;
        self.bytes -= frame.payload.len();
        Some(frame)
    }

    /// Every held frame, oldest first, without waiting.
    fn take_all(&mut self) -> impl Iterator<Item = OutboundFrame> + '_ {
        self.bytes = 0;
        self.frames.drain(..).map(|(_, frame)| frame)
    }

    /// When `frame` should have arrived if nothing delayed it; moves the reference as needed.
    fn expected(&mut self, frame: &OutboundFrame, now: Instant) -> Instant {
        let Some((source, at, timestamp)) = self.anchor else {
            return self.restart(frame, now);
        };
        if source != frame.source {
            return self.restart(frame, now);
        }
        let offset_ms = i64::from(frame.timestamp.wrapping_sub(timestamp) as i32);
        let offset = Duration::from_millis(offset_ms.unsigned_abs());
        let expected = if offset_ms >= 0 {
            at.checked_add(offset)
        } else {
            at.checked_sub(offset)
        };
        let Some(expected) = expected else {
            return self.restart(frame, now);
        };
        if expected > now + PACER_TOLERANCE {
            // Earlier than the reference allows: the reference itself came late.
            if expected > now + PACER_JUMP {
                return self.restart(frame, now);
            }
            let early = expected - now;
            self.anchor = Some((source, at.checked_sub(early).unwrap_or(now), timestamp));
            self.window_start = Some(now);
            self.window_lateness = Duration::MAX;
            return now;
        }
        let lateness = now.saturating_duration_since(expected);
        if lateness > PACER_JUMP {
            return self.restart(frame, now);
        }
        self.window_lateness = self.window_lateness.min(lateness);
        let window_start = *self.window_start.get_or_insert(now);
        if now.duration_since(window_start) >= PACER_WINDOW {
            let shift = self.window_lateness;
            self.window_start = Some(now);
            self.window_lateness = Duration::MAX;
            if shift > PACER_TOLERANCE {
                self.anchor = Some((source, at + shift, timestamp));
                return expected + shift;
            }
        }
        expected
    }

    fn restart(&mut self, frame: &OutboundFrame, now: Instant) -> Instant {
        self.anchor = Some((frame.source, now, frame.timestamp));
        self.window_start = Some(now);
        self.window_lateness = Duration::MAX;
        now
    }
}

/// Waits up to `wait` for frames from the ingest, holds them in `pacer`, and returns the next
/// one whose time has come. `Timeout` means none is due yet; `Disconnected` that nothing more
/// will come and nothing is held.
fn next_paced_frame(
    receiver: &Receiver<OutboundFrame>,
    pacer: &mut Pacer,
    wait: Duration,
) -> Result<OutboundFrame, mpsc::RecvTimeoutError> {
    if let Some(frame) = pacer.pop_due(Instant::now()) {
        return Ok(frame);
    }
    let now = Instant::now();
    let wait = pacer
        .next_due()
        .map_or(wait, |due| due.saturating_duration_since(now).min(wait));
    match receiver.recv_timeout(wait) {
        Ok(frame) => {
            pacer.push(frame, Instant::now());
            while let Ok(frame) = receiver.try_recv() {
                pacer.push(frame, Instant::now());
            }
        }
        Err(mpsc::RecvTimeoutError::Timeout) => {}
        Err(mpsc::RecvTimeoutError::Disconnected) if pacer.is_empty() => {
            return Err(mpsc::RecvTimeoutError::Disconnected);
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            // Nothing more will come: wait for the next held frame without spinning.
            if let Some(due) = pacer.next_due() {
                thread::sleep(due.saturating_duration_since(Instant::now()).min(wait));
            }
        }
    }
    pacer
        .pop_due(Instant::now())
        .ok_or(mpsc::RecvTimeoutError::Timeout)
}

fn run_outbound(
    output: Arc<Output>,
    destination: Destination,
    receiver: Receiver<OutboundFrame>,
    stop: Arc<AtomicBool>,
    mut bootstrap: MediaBootstrap,
    mut source: u64,
) {
    raise_thread_priority(OUTPUT_THREAD_NICE);
    let mut client: Option<Client> = None;
    let mut consecutive_failures = 0u32;
    let mut next_retry_at = Instant::now();
    let mut clock = OutputClock::new();
    let mut waiting_for_keyframe = false;
    // The source changed under a live platform connection: send the new source's codec
    // headers at its first keyframe and continue the timeline.
    let mut resume_pending = false;
    // Set while the platform connection is kept open without a source.
    let mut holding_since: Option<Instant> = None;
    // Congestion: video waits for a keyframe that finds the uplink caught up; audio goes on.
    let mut skipping_video = false;
    let mut rate = RateMeter::new();
    let mut stall = StallWatch::default();
    let mut pacer = Pacer::new(OUTPUT_DELAY);

    while !stop.load(Ordering::Acquire) {
        if output.overflowed.swap(false, Ordering::AcqRel) && client.is_some() {
            // Frames were lost on the way here; resume at a keyframe on the same connection.
            skipping_video = true;
        }

        let frame = match next_paced_frame(&receiver, &mut pacer, Duration::from_millis(50)) {
            Ok(frame) => frame,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if client.is_some() {
                    // Held frames still go out before the platform waits for the drone.
                    if holding_since.is_none()
                        && pacer.is_empty()
                        && !SOURCE_ACTIVE.load(Ordering::Relaxed)
                    {
                        holding_since = Some(Instant::now());
                        output.set_status("holding");
                    }
                    if holding_since.is_some_and(|since| since.elapsed() >= SOURCE_HOLD) {
                        if let Some(mut active_client) = client.take() {
                            flush_outbound_client(&mut active_client);
                        }
                        holding_since = None;
                        resume_pending = false;
                        clock.restart();
                        output.set_status("armed");
                    }
                }
                if let Some(active_client) = client.as_mut() {
                    let result = active_client
                        .poll(0)
                        .map_err(|error| Failure::new("network", error))
                        .and_then(|()| {
                            if stall.stalled(unsent_output_bytes(active_client), Instant::now()) {
                                Err(Failure::new("stalled", ""))
                            } else {
                                Ok(())
                            }
                        });
                    if let Err(error) = result {
                        client = None;
                        holding_since = None;
                        resume_pending = false;
                        consecutive_failures = consecutive_failures.saturating_add(1);
                        output.reconnect_attempts.fetch_add(1, Ordering::Relaxed);
                        let delay = output_retry_delay(consecutive_failures);
                        next_retry_at = Instant::now() + delay;
                        output.set_reconnecting(error, delay);
                    }
                }
                continue;
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        };
        let OutboundFrame {
            source: frame_source,
            frame_type,
            timestamp,
            payload,
        } = frame;

        if frame_source != source {
            source = frame_source;
            bootstrap = MediaBootstrap::default();
            skipping_video = false;
            if client.is_some() {
                holding_since.get_or_insert_with(Instant::now);
                resume_pending = true;
                waiting_for_keyframe = true;
            }
        }
        bootstrap.observe(frame_type, &payload);
        rate.add(payload.len());

        if client.is_none() {
            if Instant::now() < next_retry_at {
                output.dropped_frames.fetch_add(1, Ordering::Relaxed);
                continue;
            }
            let status = if consecutive_failures == 0 {
                "connecting"
            } else {
                "reconnecting"
            };
            output.set_status(status);
            match connect_output(&destination) {
                Ok(mut next_client) => match bootstrap.send_to(&mut next_client) {
                    Ok(bytes) => {
                        output.outbound_bytes.fetch_add(bytes, Ordering::Relaxed);
                        client = Some(next_client);
                        consecutive_failures = 0;
                        stall = StallWatch::default();
                        clock.restart();
                        waiting_for_keyframe = bootstrap.video_seen;
                        resume_pending = false;
                        holding_since = None;
                        skipping_video = false;
                        output.set_status("ready");
                    }
                    Err(error) => {
                        consecutive_failures = consecutive_failures.saturating_add(1);
                        output.reconnect_attempts.fetch_add(1, Ordering::Relaxed);
                        let delay = output_retry_delay(consecutive_failures);
                        next_retry_at = Instant::now() + delay;
                        output.set_reconnecting(error, delay);
                        output.dropped_frames.fetch_add(1, Ordering::Relaxed);
                        continue;
                    }
                },
                Err(error) => {
                    consecutive_failures = consecutive_failures.saturating_add(1);
                    output.reconnect_attempts.fetch_add(1, Ordering::Relaxed);
                    let delay = output_retry_delay(consecutive_failures);
                    next_retry_at = Instant::now() + delay;
                    output.set_reconnecting(error, delay);
                    output.dropped_frames.fetch_add(1, Ordering::Relaxed);
                    continue;
                }
            }
        }
        let Some(active_client) = client.as_mut() else {
            continue;
        };

        let mut headers_sent = Ok(0u64);
        if waiting_for_keyframe {
            if !is_video_keyframe(&payload) {
                output.dropped_frames.fetch_add(1, Ordering::Relaxed);
                continue;
            }
            waiting_for_keyframe = false;
            if resume_pending {
                let gap = holding_since
                    .take()
                    .map(|since| since.elapsed())
                    .unwrap_or_default();
                clock.resume(timestamp, gap);
                resume_pending = false;
                headers_sent = bootstrap.send_codec_headers(active_client, clock.origin);
            }
        }

        let unsent = unsent_output_bytes(active_client);
        let limit = rate.congestion_limit();
        let drop_frame = match frame_type {
            FrameType::Video if is_video_sequence_header(&payload) => false,
            FrameType::Video if skipping_video => {
                if is_video_keyframe(&payload) && unsent < limit {
                    skipping_video = false;
                    false
                } else {
                    true
                }
            }
            FrameType::Video if unsent > limit => {
                skipping_video = true;
                true
            }
            // Audio goes on while video waits, unless the uplink is far behind.
            FrameType::Audio => unsent > limit.saturating_mul(2),
            _ => false,
        };
        if drop_frame {
            output.dropped_frames.fetch_add(1, Ordering::Relaxed);
            output.set_status("congested");
            continue;
        }
        let Some(output_timestamp) = clock.map(timestamp) else {
            output.dropped_frames.fetch_add(1, Ordering::Relaxed);
            continue;
        };

        let send_result = headers_sent
            .and_then(|header_bytes| {
                send_output_frame(active_client, frame_type, output_timestamp, &payload)
                    .map(|()| header_bytes)
            })
            .and_then(|header_bytes| {
                if stall.stalled(unsent_output_bytes(active_client), Instant::now()) {
                    Err(Failure::new("stalled", ""))
                } else {
                    Ok(header_bytes)
                }
            });
        match send_result {
            Ok(header_bytes) => {
                output.outbound_bytes.fetch_add(
                    header_bytes.saturating_add(payload.len() as u64),
                    Ordering::Relaxed,
                );
                if skipping_video {
                    output.set_status("congested");
                } else {
                    output.set_status("forwarding");
                }
            }
            Err(error) => {
                client = None;
                holding_since = None;
                resume_pending = false;
                consecutive_failures = consecutive_failures.saturating_add(1);
                output.reconnect_attempts.fetch_add(1, Ordering::Relaxed);
                let delay = output_retry_delay(consecutive_failures);
                next_retry_at = Instant::now() + delay;
                output.set_reconnecting(error, delay);
                output.dropped_frames.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
    if let Some(mut active_client) = client.take() {
        // The last seconds (the pilot's goodbye on the phone microphone) are still held here.
        if !waiting_for_keyframe && !resume_pending {
            while let Ok(frame) = receiver.try_recv() {
                pacer.push(frame, Instant::now());
            }
            let deadline = Instant::now() + PACER_DRAIN_LIMIT;
            for frame in pacer.take_all() {
                if Instant::now() >= deadline || frame.source != source {
                    break;
                }
                if skipping_video && frame.frame_type == FrameType::Video {
                    continue;
                }
                let Some(timestamp) = clock.map(frame.timestamp) else {
                    continue;
                };
                if send_output_frame(
                    &mut active_client,
                    frame.frame_type,
                    timestamp,
                    &frame.payload,
                )
                .is_err()
                {
                    break;
                }
                output
                    .outbound_bytes
                    .fetch_add(frame.payload.len() as u64, Ordering::Relaxed);
            }
        }
        flush_outbound_client(&mut active_client);
    }
    output.set_status("stopped");
}

fn flush_outbound_client(client: &mut Client) {
    let deadline = Instant::now() + Duration::from_secs(2);
    while client.send_buffer.available() > 0 && Instant::now() < deadline {
        if client.poll(10).is_err() {
            break;
        }
    }
}

fn set_ingest_error(code: &str, detail: String) {
    let mut value = current_snapshot();
    value.status = "error".to_owned();
    value.error_code = Some(code.to_owned());
    value.error_detail = Some(detail);
    value.bitrate_kbps = 0.0;
    value.remote_address = None;
    apply_output_state(&mut value);
    set_snapshot(value);
}

pub fn stop_server() {
    detach_all_destinations();
    preview::source_ended();
    let current = control().lock().ok().and_then(|mut value| value.take());
    if let Some(runtime) = current {
        runtime.stop.store(true, Ordering::Release);
        let _ = runtime.server_thread.join();
    }
    reset_source_bootstrap();
}

pub fn current_snapshot() -> RelaySnapshot {
    let mut value = snapshot()
        .read()
        .map(|value| value.clone())
        .unwrap_or_else(|_| RelaySnapshot::stopped());
    // Read live, so an output that was just added or ended shows at once.
    apply_output_state(&mut value);
    value
}

pub fn snapshot_json() -> String {
    serde_json::to_string(&current_snapshot())
        .unwrap_or_else(|_| "{\"status\":\"error\",\"errorCode\":\"snapshot_failed\"}".to_owned())
}

fn java_string(env: JNIEnv<'_>, value: &str) -> jstring {
    env.new_string(value)
        .map(|result| result.into_raw())
        .unwrap_or(ptr::null_mut())
}

/// Starts receiving DJI Fly without a platform; returns an error message or an empty string.
#[no_mangle]
pub extern "system" fn Java_com_streammydrone_app_NativeRelay_nativeStartReceiver(
    env: JNIEnv<'_>,
    _class: JClass<'_>,
) -> jstring {
    let result = start_server().err().unwrap_or_default();
    java_string(env, &result)
}

/// Starts sending the received stream to one more platform, known by `output_id`; returns an
/// error message or an empty string.
#[no_mangle]
pub extern "system" fn Java_com_streammydrone_app_NativeRelay_nativeGoLive(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    output_id: JString<'_>,
    target_server_url: JString<'_>,
    target_stream_key: JString<'_>,
    tls_ca_file: JString<'_>,
) -> jstring {
    let output_id: String = match env.get_string(&output_id) {
        Ok(value) => value.into(),
        Err(_) => return java_string(env, "jni_argument"),
    };
    let target_server_url: String = match env.get_string(&target_server_url) {
        Ok(value) => value.into(),
        Err(error) => return java_string(env, &format!("jni_argument: {error}")),
    };
    let target_stream_key: String = match env.get_string(&target_stream_key) {
        Ok(value) => value.into(),
        Err(_) => return java_string(env, "jni_argument"),
    };
    let tls_ca_file: String = match env.get_string(&tls_ca_file) {
        Ok(value) => value.into(),
        Err(_) => return java_string(env, "jni_argument"),
    };
    let result = set_destination_with_tls_ca(
        &output_id,
        &target_server_url,
        &target_stream_key,
        &tls_ca_file,
    )
    .err()
    .unwrap_or_default();
    java_string(env, &result)
}

/// Stops sending to the platform `output_id`, or to every platform when it is empty; the drone
/// stays connected either way.
#[no_mangle]
pub extern "system" fn Java_com_streammydrone_app_NativeRelay_nativeEndLive(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    output_id: JString<'_>,
) {
    let output_id: String = env
        .get_string(&output_id)
        .map(Into::into)
        .unwrap_or_default();
    if output_id.is_empty() {
        clear_destinations();
    } else {
        clear_destination(&output_id);
    }
}

#[no_mangle]
pub extern "system" fn Java_com_streammydrone_app_NativeRelay_nativeSnapshot(
    env: JNIEnv<'_>,
    _class: JClass<'_>,
) -> jstring {
    java_string(env, &snapshot_json())
}

#[no_mangle]
pub extern "system" fn Java_com_streammydrone_app_NativeRelay_nativeStop(
    _env: JNIEnv<'_>,
    _class: JClass<'_>,
) {
    stop_server();
}

/// Sends the phone's microphone in place of DJI Fly's audio; `config` is the AAC encoder's
/// AudioSpecificConfig. Returns an error message or an empty string.
#[no_mangle]
pub extern "system" fn Java_com_streammydrone_app_NativeRelay_nativePhoneAudioStart(
    env: JNIEnv<'_>,
    _class: JClass<'_>,
    config: JByteArray<'_>,
) -> jstring {
    let result = match env.convert_byte_array(&config) {
        Ok(config) => phone_audio::start(&config).err().unwrap_or_default(),
        Err(_) => "jni_argument".to_owned(),
    };
    java_string(env, &result)
}

/// One encoded AAC frame of the microphone: the first `length` bytes of `data`, captured at
/// `pts_us`.
#[no_mangle]
pub extern "system" fn Java_com_streammydrone_app_NativeRelay_nativePhoneAudioFrame(
    env: JNIEnv<'_>,
    _class: JClass<'_>,
    pts_us: jlong,
    data: JByteArray<'_>,
    length: jint,
) {
    let Ok(frame) = env.convert_byte_array(&data) else {
        return;
    };
    let length = usize::try_from(length).unwrap_or(0).min(frame.len());
    phone_audio::frame(pts_us, &frame[..length]);
}

/// Goes back to DJI Fly's audio.
#[no_mangle]
pub extern "system" fn Java_com_streammydrone_app_NativeRelay_nativePhoneAudioStop(
    _env: JNIEnv<'_>,
    _class: JClass<'_>,
) {
    phone_audio::stop();
}

/// Starts an in-app preview session and returns its number for the calls below.
#[no_mangle]
pub extern "system" fn Java_com_streammydrone_app_NativeRelay_nativePreviewStart(
    _env: JNIEnv<'_>,
    _class: JClass<'_>,
) -> jlong {
    preview::start() as jlong
}

/// Waits up to `timeout_ms` for the next video tag of `session`: a 4-byte big-endian RTMP
/// timestamp followed by the FLV video tag body, or null when nothing arrived.
#[no_mangle]
pub extern "system" fn Java_com_streammydrone_app_NativeRelay_nativePreviewNext(
    env: JNIEnv<'_>,
    _class: JClass<'_>,
    session: jlong,
    timeout_ms: jint,
) -> jbyteArray {
    let timeout = Duration::from_millis(u64::try_from(timeout_ms).unwrap_or(0));
    let Some((timestamp, payload)) = preview::next(session as u64, timeout) else {
        return ptr::null_mut();
    };
    let mut packet = Vec::with_capacity(4 + payload.len());
    packet.extend_from_slice(&timestamp.to_be_bytes());
    packet.extend_from_slice(&payload);
    env.byte_array_from_slice(&packet)
        .map(|array| array.into_raw())
        .unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub extern "system" fn Java_com_streammydrone_app_NativeRelay_nativePreviewStop(
    _env: JNIEnv<'_>,
    _class: JClass<'_>,
    session: jlong,
) {
    preview::stop(session as u64);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn publish_route_requires_drone_app_with_empty_stream_name() {
        assert!(allow_publish(1, "drone", ""));
        assert!(!allow_publish(1, "live", "drone"));
        assert!(!allow_publish(1, "drone", "extra"));
    }

    #[test]
    fn destination_requires_rtmp_server_app_and_safe_key() {
        assert!(build_destination("rtmp://example.com/live", "target-key-1234", "").is_ok());
        assert!(build_destination("rtmp://example.com", "target-key-1234", "").is_err());
        assert!(build_destination("rtmp://example.com/live", "bad/key", "").is_err());
        assert!(build_destination("https://example.com/live", "target-key-1234", "").is_err());
    }

    #[test]
    fn secure_destination_requires_android_ca_bundle() {
        assert!(build_destination(
            "rtmps://example.com/live",
            "target-key-1234",
            "/tmp/system-cas.pem"
        )
        .is_ok_and(|destination| destination.secure));
        // Apple systems verify with their own trust store instead of a file.
        let without_file = build_destination("rtmps://example.com/live", "target-key-1234", "");
        if platform_trust() {
            assert!(without_file.is_ok_and(|destination| destination.tls_ca_file.is_none()));
        } else {
            assert_eq!(without_file.err().as_deref(), Some("missing_ca_bundle"));
        }
    }

    #[test]
    fn destination_url_is_not_exposed_in_snapshot() {
        let destination =
            build_destination("rtmp://example.com/live", "secret-key-1234", "").unwrap();
        assert_eq!(destination.url, "rtmp://example.com/live/secret-key-1234");
        assert!(!snapshot_json().contains("secret-key-1234"));
    }

    #[test]
    fn retry_backoff_is_exponential_and_capped() {
        assert_eq!(output_retry_delay(1), Duration::from_secs(1));
        assert_eq!(output_retry_delay(2), Duration::from_secs(2));
        assert_eq!(output_retry_delay(3), Duration::from_secs(4));
        assert_eq!(output_retry_delay(4), Duration::from_secs(8));
        assert_eq!(output_retry_delay(5), Duration::from_secs(15));
        assert_eq!(output_retry_delay(50), Duration::from_secs(15));
    }

    #[test]
    fn media_bootstrap_recognizes_avc_aac_headers_and_keyframes() {
        assert!(is_video_sequence_header(&[0x17, 0x00, 0x00]));
        assert!(!is_video_keyframe(&[0x17, 0x00, 0x00]));
        assert!(is_video_keyframe(&[0x17, 0x01, 0x00]));
        assert!(!is_video_keyframe(&[0x27, 0x01, 0x00]));
        assert!(is_audio_sequence_header(&[0xAF, 0x00, 0x12]));
        assert!(!is_audio_sequence_header(&[0xAF, 0x01, 0x12]));
    }

    #[test]
    fn a_destination_needs_a_running_receiver() {
        assert_eq!(
            set_destination_with_tls_ca(
                "youtube",
                "rtmp://127.0.0.1:9/live",
                "target-key-1234",
                ""
            ),
            Err("receiver_not_running".to_owned())
        );
    }

    #[test]
    fn each_output_reports_its_own_state() {
        let youtube = Output::new("youtube", true);
        let twitch = Output::new("twitch", false);
        youtube.set_status("forwarding");
        youtube.outbound_bytes.fetch_add(1_000, Ordering::Relaxed);
        twitch.reconnect_attempts.fetch_add(2, Ordering::Relaxed);

        let youtube = youtube.snapshot();
        let twitch = twitch.snapshot();
        assert_eq!(
            (youtube.id.as_str(), youtube.status.as_str()),
            ("youtube", "forwarding")
        );
        assert_eq!((youtube.outbound_bytes, youtube.secure), (1_000, true));
        assert_eq!(
            (twitch.status.as_str(), twitch.reconnect_attempts),
            ("armed", 2)
        );
        assert_eq!(twitch.outbound_bytes, 0);
    }

    #[test]
    fn gaps_in_the_drone_video_count_as_stalls() {
        let start = Instant::now();
        let mut health = IngestHealth::default();
        health.source_started(start);
        health.video_arrived(start);
        health.video_arrived(start + Duration::from_millis(33));
        // Ordinary Wi-Fi jitter is not a stall.
        health.video_arrived(start + Duration::from_millis(400));
        assert_eq!(health.stalls, 0);
        health.video_arrived(start + Duration::from_millis(1_600));
        health.video_arrived(start + Duration::from_millis(1_633));
        health.video_arrived(start + Duration::from_millis(2_500));
        assert_eq!(health.stalls, 2);
        assert_eq!(health.longest_stall, Duration::from_millis(1_200));
        assert_eq!(
            health.recent_interruptions(start + Duration::from_secs(3)),
            2
        );
        // A minute later they no longer count as recent.
        assert_eq!(
            health.recent_interruptions(start + Duration::from_secs(70)),
            0
        );
        assert_eq!(health.stalls, 2);
    }

    #[test]
    fn a_reconnect_is_counted_once_and_not_as_a_stall() {
        let start = Instant::now();
        let mut health = IngestHealth::default();
        health.source_started(start);
        health.video_arrived(start);
        health.source_ended();
        let back = start + Duration::from_secs(3);
        health.source_started(back);
        health.video_arrived(back);
        assert_eq!((health.source_reconnects(), health.stalls), (1, 0));
        assert_eq!(health.recent_interruptions(back), 1);
    }

    #[test]
    fn a_retry_keeps_the_reason_until_the_output_recovers() {
        let output = Output::new("kick", false);
        output.set_reconnecting(
            Failure::new("connect_failed", "connection refused"),
            Duration::from_secs(4),
        );
        let waiting = output.snapshot();
        assert_eq!(waiting.reason.as_deref(), Some("connect_failed"));
        assert_eq!(waiting.reason_detail.as_deref(), Some("connection refused"));
        assert_eq!(waiting.retry_in_seconds, Some(4));

        output.set_status("reconnecting");
        let retrying = output.snapshot();
        assert_eq!(retrying.reason.as_deref(), Some("connect_failed"));
        assert_eq!(retrying.retry_in_seconds, None);

        output.set_status("forwarding");
        let recovered = output.snapshot();
        assert_eq!((recovered.reason, recovered.reason_detail), (None, None));
    }

    #[test]
    fn a_mid_stream_output_starts_from_the_cached_headers() {
        let mut bootstrap = MediaBootstrap::default();
        bootstrap.observe(FrameType::Video, &[0x17, 0x00, 0x00, 0x00, 0x00, 0x01]);
        bootstrap.observe(FrameType::Audio, &[0xAF, 0x00, 0x12, 0x10]);
        bootstrap.observe(FrameType::Video, &[0x27, 0x01, 0x00, 0x00, 0x00]);
        let snapshot = bootstrap.clone();
        assert!(snapshot.video_seen);
        assert_eq!(
            snapshot.video_sequence.as_deref(),
            Some(&[0x17, 0x00, 0x00, 0x00, 0x00, 0x01][..])
        );
        assert_eq!(
            snapshot.audio_sequence.as_deref(),
            Some(&[0xAF, 0x00, 0x12, 0x10][..])
        );
    }

    #[test]
    fn the_platform_timeline_continues_across_a_drone_reconnect() {
        let mut clock = OutputClock::new();
        assert_eq!(clock.map(5_000), Some(0));
        assert_eq!(clock.map(5_033), Some(33));
        // An audio frame from before the first keyframe cannot go out before the start.
        assert_eq!(clock.map(4_990), None);
        // The drone is back after three seconds and its clock started over.
        clock.resume(120, Duration::from_secs(3));
        assert_eq!(clock.map(120), Some(3_033));
        assert_eq!(clock.map(153), Some(3_066));
        clock.restart();
        assert_eq!(clock.map(40), Some(0));
    }

    #[test]
    fn a_slow_uplink_is_not_a_dead_one() {
        let start = Instant::now();
        let mut watch = StallWatch::default();
        // Unsent output grows, but drains now and then: never stalled.
        for (second, unsent) in [(0, 100), (4, 900), (8, 600), (12, 1_200), (16, 800)] {
            assert!(!watch.stalled(unsent, start + Duration::from_secs(second)));
        }
        // It stops draining.
        assert!(!watch.stalled(900, start + Duration::from_secs(17)));
        assert!(!watch.stalled(950, start + Duration::from_secs(26)));
        assert!(watch.stalled(950, start + Duration::from_secs(27)));
        // Everything delivered clears it.
        assert!(!watch.stalled(0, start + Duration::from_secs(28)));
    }

    #[test]
    fn congestion_allows_about_a_second_and_a_half_of_stream() {
        let mut rate = RateMeter::new();
        assert_eq!(rate.congestion_limit(), MIN_CONGESTION_BYTES);
        rate.bytes_per_second = 1_000_000.0;
        assert_eq!(rate.congestion_limit(), 1_500_000);
    }

    fn paced(source: u64, timestamp: u32) -> OutboundFrame {
        OutboundFrame {
            source,
            frame_type: FrameType::Video,
            timestamp,
            payload: Arc::from(&[0x27, 0x01, 0x00][..]),
        }
    }

    fn ms(value: u64) -> Duration {
        Duration::from_millis(value)
    }

    /// Feeds `arrivals` (arrival ms, source timestamp) and returns when each frame left, in ms.
    fn pace(arrivals: &[(u64, u32)]) -> Vec<(u32, u64)> {
        let start = Instant::now();
        let mut pacer = Pacer::new(OUTPUT_DELAY);
        let mut left = Vec::new();
        let mut next = 0;
        let end = arrivals.last().map_or(0, |(at, _)| *at) + 10_000;
        for tick in 0..=end {
            let now = start + ms(tick);
            while next < arrivals.len() && arrivals[next].0 == tick {
                pacer.push(paced(1, arrivals[next].1), now);
                next += 1;
            }
            while let Some(frame) = pacer.pop_due(now) {
                left.push((frame.timestamp, tick));
            }
        }
        left
    }

    #[test]
    fn steady_frames_leave_after_the_delay_at_their_own_pace() {
        let arrivals: Vec<_> = (0..10).map(|i| (i * 33, 1_000 + i as u32 * 33)).collect();
        let left = pace(&arrivals);
        assert_eq!(left.len(), 10);
        for (i, (timestamp, at)) in left.iter().enumerate() {
            assert_eq!(*timestamp, 1_000 + i as u32 * 33);
            assert_eq!(*at, 3_000 + i as u64 * 33);
        }
    }

    #[test]
    fn a_wifi_pause_shorter_than_the_delay_never_reaches_the_platform() {
        // 30 fps; frames filmed during a 2.5 s pause arrive together when it ends.
        let mut arrivals = Vec::new();
        for i in 0..300u64 {
            let filmed = i * 33;
            let arrived = if (2_000..4_500).contains(&filmed) {
                4_500
            } else {
                filmed
            };
            arrivals.push((arrived, filmed as u32));
        }
        let left = pace(&arrivals);
        assert_eq!(left.len(), 300);
        // Every frame leaves exactly the delay after it was filmed: no gap, no burst.
        for (timestamp, at) in &left {
            assert_eq!(*at, u64::from(*timestamp) + 3_000);
        }
    }

    #[test]
    fn a_longer_pause_only_delays_what_it_held_back() {
        let mut arrivals = Vec::new();
        for i in 0..400u64 {
            let filmed = i * 33;
            // A 5 s pause, longer than the delay.
            let arrived = if (2_000..7_000).contains(&filmed) {
                7_000
            } else {
                filmed
            };
            arrivals.push((arrived, filmed as u32));
        }
        let left = pace(&arrivals);
        assert_eq!(left.len(), 400);
        // Held-back frames leave as soon as they arrive; afterwards the full delay is back.
        assert!(left
            .iter()
            .filter(|(timestamp, _)| (2_000..4_000).contains(timestamp))
            .all(|(_, at)| *at == 7_000));
        assert!(left
            .iter()
            .filter(|(timestamp, _)| *timestamp >= 7_000)
            .all(|(timestamp, at)| *at == u64::from(*timestamp) + 3_000));
    }

    #[test]
    fn frames_leave_in_the_order_they_came() {
        // Audio slightly behind video on the timeline: no reordering.
        let left = pace(&[(0, 1_000), (0, 990), (33, 1_033), (33, 1_023)]);
        let order: Vec<_> = left.iter().map(|(timestamp, _)| *timestamp).collect();
        assert_eq!(order, vec![1_000, 990, 1_033, 1_023]);
    }

    #[test]
    fn a_late_reference_frame_does_not_add_to_the_delay() {
        // The first frame (filmed at 0) came 1 s late; the ones after it were on time.
        let mut arrivals = vec![(1_000, 0)];
        arrivals.extend((31..90).map(|i| (i * 33, i as u32 * 33)));
        let left = pace(&arrivals);
        // Measured from the frames that came on time: 3 s, not 4 s.
        let (timestamp, at) = *left.last().unwrap();
        assert_eq!(at, u64::from(timestamp) + 3_000);
    }

    #[test]
    fn a_clock_that_falls_behind_gets_its_full_delay_back() {
        // From 1 s on, every frame arrives 1.5 s later than its timestamp says.
        let mut arrivals: Vec<_> = (0..30).map(|i| (i * 33, i as u32 * 33)).collect();
        arrivals.extend((30..800).map(|i| (i * 33 + 1_500, i as u32 * 33)));
        let left = pace(&arrivals);
        assert_eq!(left.len(), 800);
        let (timestamp, at) = *left.last().unwrap();
        let arrived = arrivals.last().unwrap().0;
        assert_eq!(timestamp, 799 * 33);
        assert_eq!(at - arrived, 3_000);
    }

    #[test]
    fn a_new_source_or_a_jump_starts_over() {
        let start = Instant::now();
        let mut pacer = Pacer::new(OUTPUT_DELAY);
        pacer.push(paced(1, 50_000), start);
        // The drone reconnected and its clock started over.
        pacer.push(paced(2, 10), start + ms(100));
        // Within one source, a timestamp a minute ahead.
        pacer.push(paced(2, 60_010), start + ms(200));
        assert_eq!(
            pacer.pop_due(start + ms(3_000)).map(|f| f.timestamp),
            Some(50_000)
        );
        assert_eq!(
            pacer.pop_due(start + ms(3_100)).map(|f| f.timestamp),
            Some(10)
        );
        assert!(pacer.pop_due(start + ms(3_199)).is_none());
        assert_eq!(
            pacer.pop_due(start + ms(3_200)).map(|f| f.timestamp),
            Some(60_010)
        );
        assert!(pacer.is_empty());
    }

    #[test]
    fn too_much_held_payload_leaves_at_once() {
        let start = Instant::now();
        let mut pacer = Pacer::new(OUTPUT_DELAY);
        let big: Arc<[u8]> = vec![0u8; PACER_MAX_BYTES].into();
        let frame = |timestamp| OutboundFrame {
            source: 1,
            frame_type: FrameType::Video,
            timestamp,
            payload: Arc::clone(&big),
        };
        pacer.push(frame(0), start);
        assert!(pacer.pop_due(start).is_none());
        pacer.push(frame(33), start);
        assert!(pacer.pop_due(start).is_some());
        assert!(pacer.pop_due(start).is_none());
    }

    #[test]
    fn the_receiver_hands_out_frames_only_when_due() {
        let (sender, receiver) = mpsc::sync_channel(8);
        let mut pacer = Pacer::new(ms(100));
        sender.send(paced(1, 0)).unwrap();
        assert_eq!(
            next_paced_frame(&receiver, &mut pacer, ms(10)).err(),
            Some(mpsc::RecvTimeoutError::Timeout)
        );
        let started = Instant::now();
        let frame = loop {
            match next_paced_frame(&receiver, &mut pacer, ms(50)) {
                Ok(frame) => break frame,
                Err(error) => assert_eq!(error, mpsc::RecvTimeoutError::Timeout),
            }
        };
        assert_eq!(frame.timestamp, 0);
        assert!(started.elapsed() >= ms(60));
        // Held frames still come out after the ingest has gone; then it says so.
        sender.send(paced(1, 10)).unwrap();
        drop(sender);
        let frame = loop {
            if let Ok(frame) = next_paced_frame(&receiver, &mut pacer, ms(50)) {
                break frame;
            }
        };
        assert_eq!(frame.timestamp, 10);
        assert_eq!(
            next_paced_frame(&receiver, &mut pacer, ms(50)).err(),
            Some(mpsc::RecvTimeoutError::Disconnected)
        );
    }

    #[test]
    fn snapshot_is_valid_json() {
        let parsed: serde_json::Value = serde_json::from_str(&snapshot_json()).unwrap();
        assert!(parsed.get("status").is_some());
        assert!(parsed.get("receivedBytes").is_some());
        assert!(parsed
            .get("outputs")
            .is_some_and(serde_json::Value::is_array));
    }
}
