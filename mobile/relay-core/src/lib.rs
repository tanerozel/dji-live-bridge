use jni::objects::{JClass, JString};
use jni::sys::{jbyteArray, jint, jlong, jstring};
use jni::JNIEnv;
use librtmp2::client::Client;
use librtmp2::server::Server;
use librtmp2::types::{Frame, FrameType, ServerConfig};
use serde::Serialize;
use std::ptr;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Mutex, OnceLock, RwLock};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

mod preview;

const DEFAULT_BIND_ADDRESS: &str = "0.0.0.0:1935";
const EXPECTED_APP: &str = "drone";
const OUTBOUND_QUEUE_CAPACITY: usize = 256;
const MAX_QUEUED_FRAME_BYTES: usize = 16 * 1024 * 1024;
const OUTPUT_CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
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
/// How often the ingest rebuilds the status snapshot the app polls.
const SNAPSHOT_INTERVAL: Duration = Duration::from_millis(250);
/// Nice values for the media threads (Android's THREAD_PRIORITY_URGENT_DISPLAY is -8).
const INGEST_THREAD_NICE: i32 = -8;
const OUTPUT_THREAD_NICE: i32 = -8;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RelaySnapshot {
    pub status: String,
    pub detail: String,
    pub remote_address: Option<String>,
    pub received_bytes: u64,
    pub bitrate_kbps: f64,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    pub video_frames: u64,
    pub audio_frames: u64,
    pub rejected_publish_attempts: u64,
    pub output_status: String,
    pub output_detail: String,
    pub outbound_bytes: u64,
    pub dropped_output_frames: u64,
    pub output_reconnect_attempts: u64,
    pub output_secure: bool,
}

impl RelaySnapshot {
    fn stopped() -> Self {
        Self {
            status: "stopped".to_owned(),
            detail: "RTMP alıcısı kapalı".to_owned(),
            remote_address: None,
            received_bytes: 0,
            bitrate_kbps: 0.0,
            video_codec: None,
            audio_codec: None,
            video_frames: 0,
            audio_frames: 0,
            rejected_publish_attempts: 0,
            output_status: "disabled".to_owned(),
            output_detail: "Harici hedef yapılandırılmadı".to_owned(),
            outbound_bytes: 0,
            dropped_output_frames: 0,
            output_reconnect_attempts: 0,
            output_secure: false,
        }
    }
}

#[derive(Clone)]
struct Destination {
    url: String,
    secure: bool,
    tls_ca_file: Option<String>,
}

#[derive(Clone)]
struct OutboundSnapshot {
    status: String,
    detail: String,
}

impl OutboundSnapshot {
    fn disabled() -> Self {
        Self {
            status: "disabled".to_owned(),
            detail: "Harici hedef yapılandırılmadı".to_owned(),
        }
    }
}

/// A frame for the platform output, tagged with the source it belongs to, so a source change
/// is noticed in order even when the queue overflowed.
struct OutboundFrame {
    source: u64,
    frame_type: FrameType,
    timestamp: u32,
    payload: Vec<u8>,
}

struct RuntimeControl {
    stop: Arc<AtomicBool>,
    server_thread: JoinHandle<()>,
}

/// The platform output. It comes and goes while the ingest keeps the drone connected.
struct OutboundControl {
    stop: Arc<AtomicBool>,
    thread: JoinHandle<()>,
}

static CONTROL: OnceLock<Mutex<Option<RuntimeControl>>> = OnceLock::new();
static OUTBOUND_CONTROL: OnceLock<Mutex<Option<OutboundControl>>> = OnceLock::new();
/// Codec headers and metadata of the current source, so an output attached mid-stream can
/// start with them instead of waiting for a source restart.
static SOURCE_BOOTSTRAP: OnceLock<Mutex<MediaBootstrap>> = OnceLock::new();
static SNAPSHOT: OnceLock<RwLock<RelaySnapshot>> = OnceLock::new();
static OUTBOUND_SNAPSHOT: OnceLock<RwLock<OutboundSnapshot>> = OnceLock::new();
static OUTBOUND_SENDER: OnceLock<RwLock<Option<SyncSender<OutboundFrame>>>> = OnceLock::new();
static VIDEO_FRAMES: AtomicU64 = AtomicU64::new(0);
static AUDIO_FRAMES: AtomicU64 = AtomicU64::new(0);
static REJECTED_PUBLISH_ATTEMPTS: AtomicU64 = AtomicU64::new(0);
static OUTBOUND_BYTES: AtomicU64 = AtomicU64::new(0);
static DROPPED_OUTPUT_FRAMES: AtomicU64 = AtomicU64::new(0);
static OUTPUT_RECONNECT_ATTEMPTS: AtomicU64 = AtomicU64::new(0);
static OUTBOUND_QUEUE_OVERFLOWED: AtomicBool = AtomicBool::new(false);
static OUTPUT_SECURE: AtomicBool = AtomicBool::new(false);
/// Grows whenever the source ends or another publisher takes over.
static SOURCE_GENERATION: AtomicU64 = AtomicU64::new(0);
/// A publisher is sending right now.
static SOURCE_ACTIVE: AtomicBool = AtomicBool::new(false);
/// The connection whose frames are relayed; frames of a connection it replaced are dropped.
static CURRENT_PUBLISHER: AtomicU64 = AtomicU64::new(0);

fn control() -> &'static Mutex<Option<RuntimeControl>> {
    CONTROL.get_or_init(|| Mutex::new(None))
}

fn outbound_control() -> &'static Mutex<Option<OutboundControl>> {
    OUTBOUND_CONTROL.get_or_init(|| Mutex::new(None))
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

fn outbound_snapshot() -> &'static RwLock<OutboundSnapshot> {
    OUTBOUND_SNAPSHOT.get_or_init(|| RwLock::new(OutboundSnapshot::disabled()))
}

fn outbound_sender() -> &'static RwLock<Option<SyncSender<OutboundFrame>>> {
    OUTBOUND_SENDER.get_or_init(|| RwLock::new(None))
}

fn set_snapshot(value: RelaySnapshot) {
    if let Ok(mut current) = snapshot().write() {
        *current = value;
    }
}

fn set_output_status(status: &str, detail: &str) {
    if let Ok(mut current) = outbound_snapshot().write() {
        current.status = status.to_owned();
        current.detail = detail.to_owned();
    }
}

fn apply_output_state(value: &mut RelaySnapshot) {
    if let Ok(output) = outbound_snapshot().read() {
        value.output_status.clone_from(&output.status);
        value.output_detail.clone_from(&output.detail);
    }
    value.outbound_bytes = OUTBOUND_BYTES.load(Ordering::Relaxed);
    value.dropped_output_frames = DROPPED_OUTPUT_FRAMES.load(Ordering::Relaxed);
    value.output_reconnect_attempts = OUTPUT_RECONNECT_ATTEMPTS.load(Ordering::Relaxed);
    value.output_secure = OUTPUT_SECURE.load(Ordering::Relaxed);
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
    }

    match frame.frame_type {
        FrameType::Video => {
            VIDEO_FRAMES.fetch_add(1, Ordering::Relaxed);
        }
        FrameType::Audio => {
            AUDIO_FRAMES.fetch_add(1, Ordering::Relaxed);
        }
        _ => {}
    }

    if frame.size as usize > MAX_QUEUED_FRAME_BYTES || (frame.size > 0 && frame.data.is_null()) {
        DROPPED_OUTPUT_FRAMES.fetch_add(1, Ordering::Relaxed);
        OUTBOUND_QUEUE_OVERFLOWED.store(true, Ordering::Release);
        return;
    }

    let payload: &[u8] = if frame.size == 0 {
        &[]
    } else {
        // librtmp2 guarantees Frame.data remains valid for the duration of this callback.
        unsafe { std::slice::from_raw_parts(frame.data, frame.size as usize) }
    };
    if matches!(frame.frame_type, FrameType::Video) {
        preview::offer(frame.timestamp, payload);
    }
    // Observed before the sender is read: see attach_destination.
    if let Ok(mut bootstrap) = source_bootstrap().lock() {
        bootstrap.observe(frame.frame_type, payload);
    }

    let sender = outbound_sender()
        .read()
        .ok()
        .and_then(|value| value.clone());
    let Some(sender) = sender else {
        return;
    };
    let message = OutboundFrame {
        source: SOURCE_GENERATION.load(Ordering::Relaxed),
        frame_type: frame.frame_type,
        timestamp: frame.timestamp,
        payload: payload.to_vec(),
    };
    match sender.try_send(message) {
        Ok(()) => {}
        Err(TrySendError::Full(_)) => {
            DROPPED_OUTPUT_FRAMES.fetch_add(1, Ordering::Relaxed);
            OUTBOUND_QUEUE_OVERFLOWED.store(true, Ordering::Release);
        }
        Err(TrySendError::Disconnected(_)) => {}
    }
}

fn notify_source_ended() {
    CURRENT_PUBLISHER.store(0, Ordering::Relaxed);
    reset_source();
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
    #[cfg(not(any(target_os = "android", target_os = "linux")))]
    let _ = nice;
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
        return Err("Hedef adresi rtmp:// veya rtmps:// ile başlamalı".to_owned());
    };
    let Some((authority, app)) = authority_and_app.split_once('/') else {
        return Err("Hedef adresi sunucu ve uygulama yolunu içermeli".to_owned());
    };
    if authority.is_empty()
        || app.is_empty()
        || authority.contains('@')
        || server_url.contains(['?', '#'])
        || server_url.chars().any(char::is_whitespace)
    {
        return Err("Hedef RTMP sunucu adresi geçersiz".to_owned());
    }
    if !(4..=512).contains(&stream_key.len())
        || !stream_key.is_ascii()
        || stream_key.bytes().any(|value| {
            value.is_ascii_whitespace() || value.is_ascii_control() || matches!(value, b'/' | b'#')
        })
    {
        return Err("Hedef yayın anahtarı geçersiz".to_owned());
    }
    let tls_ca_file = tls_ca_file.trim();
    if secure && tls_ca_file.is_empty() {
        return Err("Android sistem sertifikaları RTMPS için hazırlanamadı".to_owned());
    }

    Ok(Destination {
        url: format!("{server_url}/{stream_key}"),
        secure,
        tls_ca_file: secure.then(|| tls_ca_file.to_owned()),
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

/// Starts sending the running ingest's stream to a platform. The drone stays connected, and a
/// stream that is already arriving goes out from its next keyframe.
pub fn set_destination_with_tls_ca(
    target_server_url: &str,
    target_stream_key: &str,
    tls_ca_file: &str,
) -> Result<(), String> {
    let destination = build_destination(target_server_url, target_stream_key, tls_ca_file)?;
    attach_destination(destination)
}

/// Stops sending to the platform; the ingest keeps receiving the drone.
pub fn clear_destination() {
    detach_destination();
}

fn start_runtime(bind_address: &str, destination: Option<Destination>) -> Result<(), String> {
    stop_server();
    {
        let mut control_slot = control()
            .lock()
            .map_err(|_| "RTMP çalışma durumu kilitlenemedi".to_owned())?;
        VIDEO_FRAMES.store(0, Ordering::Relaxed);
        AUDIO_FRAMES.store(0, Ordering::Relaxed);
        REJECTED_PUBLISH_ATTEMPTS.store(0, Ordering::Relaxed);
        reset_source_bootstrap();
        set_output_status("disabled", "Harici hedef yapılandırılmadı");

        let stop = Arc::new(AtomicBool::new(false));
        let server_stop = Arc::clone(&stop);
        let bind_address = bind_address.to_owned();
        let server_thread = thread::Builder::new()
            .name("dji-rtmp-ingest".to_owned())
            .spawn(move || run_server(&bind_address, server_stop))
            .map_err(|error| format!("RTMP iş parçacığı başlatılamadı: {error}"))?;
        *control_slot = Some(RuntimeControl {
            stop,
            server_thread,
        });
    }
    if let Some(destination) = destination {
        if let Err(error) = attach_destination(destination) {
            stop_server();
            return Err(error);
        }
    }
    Ok(())
}

fn attach_destination(destination: Destination) -> Result<(), String> {
    let receiving = control().lock().is_ok_and(|slot| slot.is_some());
    if !receiving {
        return Err("RTMP alıcısı çalışmıyor".to_owned());
    }
    detach_destination();
    OUTBOUND_BYTES.store(0, Ordering::Relaxed);
    DROPPED_OUTPUT_FRAMES.store(0, Ordering::Relaxed);
    OUTPUT_RECONNECT_ATTEMPTS.store(0, Ordering::Relaxed);
    OUTBOUND_QUEUE_OVERFLOWED.store(false, Ordering::Relaxed);
    OUTPUT_SECURE.store(destination.secure, Ordering::Relaxed);

    let (sender, receiver) = mpsc::sync_channel(OUTBOUND_QUEUE_CAPACITY);
    // handle_frame records a frame in the bootstrap before it reads the sender, so taking the
    // snapshot and installing the sender under the bootstrap lock puts every frame in the
    // snapshot, in the channel, or in both.
    let (bootstrap, source) = {
        let bootstrap = source_bootstrap()
            .lock()
            .map_err(|_| "RTMP çalışma durumu kilitlenemedi".to_owned())?;
        if let Ok(mut current) = outbound_sender().write() {
            *current = Some(sender);
        }
        (bootstrap.clone(), SOURCE_GENERATION.load(Ordering::Relaxed))
    };
    set_output_status("armed", "Kaynak yayın gelince hedefe bağlanacak");

    let stop = Arc::new(AtomicBool::new(false));
    let outbound_stop = Arc::clone(&stop);
    match thread::Builder::new()
        .name("dji-rtmp-output".to_owned())
        .spawn(move || run_outbound(destination, receiver, outbound_stop, bootstrap, source))
    {
        Ok(thread) => {
            if let Ok(mut slot) = outbound_control().lock() {
                *slot = Some(OutboundControl { stop, thread });
            }
            Ok(())
        }
        Err(error) => {
            if let Ok(mut current) = outbound_sender().write() {
                *current = None;
            }
            OUTPUT_SECURE.store(false, Ordering::Relaxed);
            set_output_status("disabled", "Harici hedef başlatılamadı");
            Err(format!("Hedef RTMP iş parçacığı başlatılamadı: {error}"))
        }
    }
}

fn detach_destination() {
    if let Ok(mut sender) = outbound_sender().write() {
        *sender = None;
    }
    let current = outbound_control()
        .lock()
        .ok()
        .and_then(|mut slot| slot.take());
    if let Some(output) = current {
        output.stop.store(true, Ordering::Release);
        let _ = output.thread.join();
    }
    OUTPUT_SECURE.store(false, Ordering::Relaxed);
    set_output_status("disabled", "Harici hedef yapılandırılmadı");
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
            set_ingest_error(format!("RTMP sunucusu oluşturulamadı: {error}"));
            return;
        }
    };
    server.on_publish_cb = Some(allow_publish);
    // DJI Fly sends releaseStream before publish; allowing it on the drone route lets a
    // reconnect take over from its own stale connection instead of waiting for it to time out.
    server.on_release_stream_cb = Some(allow_publish);
    server.on_frame_cb = Some(handle_frame);

    if let Err(error) = server.listen(bind_address) {
        set_ingest_error(format!("{bind_address} dinlenemedi: {error}"));
        return;
    }

    let mut initial = RelaySnapshot {
        status: "listening".to_owned(),
        detail: "RC 2 yayını bekleniyor".to_owned(),
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
            set_ingest_error(format!("RTMP alıcısı durdu: {error}"));
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

        let (status, detail, remote_address, video_codec, audio_codec) =
            if let Some(connection) = publisher {
                (
                    "publishing",
                    "RC 2 yayını alınıyor",
                    Some(connection.remote_addr.clone()),
                    connection.detected_video_codec.clone(),
                    connection.detected_audio_codec.clone(),
                )
            } else if let Some(connection) = server.connections.first() {
                (
                    "connected",
                    "RTMP istemcisi bağlandı; yayın komutu bekleniyor",
                    Some(connection.remote_addr.clone()),
                    None,
                    None,
                )
            } else {
                bitrate_kbps = 0.0;
                previous_bytes = 0;
                ("listening", "RC 2 yayını bekleniyor", None, None, None)
            };

        let mut next = RelaySnapshot {
            status: status.to_owned(),
            detail: detail.to_owned(),
            remote_address,
            received_bytes,
            bitrate_kbps,
            video_codec,
            audio_codec,
            video_frames: VIDEO_FRAMES.load(Ordering::Relaxed),
            audio_frames: AUDIO_FRAMES.load(Ordering::Relaxed),
            rejected_publish_attempts: REJECTED_PUBLISH_ATTEMPTS.load(Ordering::Relaxed),
            ..RelaySnapshot::stopped()
        };
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
    final_snapshot.detail = "RTMP alıcısı kapalı".to_owned();
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

    fn send_to(&self, client: &mut Client) -> Result<u64, String> {
        let frames = [
            (FrameType::Script, self.script.as_deref()),
            (FrameType::Metadata, self.metadata.as_deref()),
            (FrameType::Video, self.video_sequence.as_deref()),
            (FrameType::Audio, self.audio_sequence.as_deref()),
        ];
        Self::send_frames(client, &frames, 0)
    }

    /// A new source on a running connection: its metadata and codec headers at `timestamp`.
    fn send_codec_headers(&self, client: &mut Client, timestamp: u32) -> Result<u64, String> {
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
    ) -> Result<u64, String> {
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

fn connect_output(destination: &Destination) -> Result<Client, String> {
    let mut client = Client::new();
    client.set_connect_timeout(OUTPUT_CONNECT_TIMEOUT);
    if destination.secure {
        client.set_tls_client_config(destination.tls_ca_file.clone(), false);
    }
    client.connect(&destination.url).map_err(|error| {
        if destination.secure {
            format!("TLS sertifikası doğrulanamadı veya ağ bağlantısı kurulamadı: {error}")
        } else {
            format!("bağlantı kurulamadı: {error}")
        }
    })?;
    client
        .publish()
        .map_err(|error| format!("yayın kabul edilmedi: {error}"))?;
    Ok(client)
}

fn send_output_frame(
    client: &mut Client,
    frame_type: FrameType,
    timestamp: u32,
    payload: &[u8],
) -> Result<(), String> {
    client
        .send_frame_payload(frame_type, timestamp, payload)
        .and_then(|()| client.poll(0))
        .map_err(|error| error.to_string())
}

fn reconnect_detail(error: &str, delay: Duration) -> String {
    format!(
        "Harici hedef bağlantısı kesildi ({error}); {} saniye içinde yeniden denenecek",
        delay.as_secs()
    )
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

fn run_outbound(
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
    let transport_name = if destination.secure { "RTMPS" } else { "RTMP" };

    while !stop.load(Ordering::Acquire) {
        if OUTBOUND_QUEUE_OVERFLOWED.swap(false, Ordering::AcqRel) && client.is_some() {
            // Frames were lost on the way here; resume at a keyframe on the same connection.
            skipping_video = true;
        }

        let frame = match receiver.recv_timeout(Duration::from_millis(50)) {
            Ok(frame) => frame,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if client.is_some() {
                    if holding_since.is_none() && !SOURCE_ACTIVE.load(Ordering::Relaxed) {
                        holding_since = Some(Instant::now());
                        set_output_status(
                            "holding",
                            "Drone bağlantısı koptu; platform bağlantısı açık tutuluyor",
                        );
                    }
                    if holding_since.is_some_and(|since| since.elapsed() >= SOURCE_HOLD) {
                        if let Some(mut active_client) = client.take() {
                            flush_outbound_client(&mut active_client);
                        }
                        holding_since = None;
                        resume_pending = false;
                        clock.restart();
                        set_output_status("armed", "Kaynak yayın gelince hedefe bağlanacak");
                    }
                }
                if let Some(active_client) = client.as_mut() {
                    let result = active_client
                        .poll(0)
                        .map_err(|error| error.to_string())
                        .and_then(|()| {
                            if stall.stalled(unsent_output_bytes(active_client), Instant::now()) {
                                Err("hedef veri almayı bıraktı".to_owned())
                            } else {
                                Ok(())
                            }
                        });
                    if let Err(error) = result {
                        client = None;
                        holding_since = None;
                        resume_pending = false;
                        consecutive_failures = consecutive_failures.saturating_add(1);
                        OUTPUT_RECONNECT_ATTEMPTS.fetch_add(1, Ordering::Relaxed);
                        let delay = output_retry_delay(consecutive_failures);
                        next_retry_at = Instant::now() + delay;
                        set_output_status("reconnecting", &reconnect_detail(&error, delay));
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
                DROPPED_OUTPUT_FRAMES.fetch_add(1, Ordering::Relaxed);
                continue;
            }
            let status = if consecutive_failures == 0 {
                "connecting"
            } else {
                "reconnecting"
            };
            set_output_status(
                status,
                &format!("Harici {transport_name} hedefine bağlanıyor"),
            );
            match connect_output(&destination) {
                Ok(mut next_client) => match bootstrap.send_to(&mut next_client) {
                    Ok(bytes) => {
                        OUTBOUND_BYTES.fetch_add(bytes, Ordering::Relaxed);
                        client = Some(next_client);
                        consecutive_failures = 0;
                        stall = StallWatch::default();
                        clock.restart();
                        waiting_for_keyframe = bootstrap.video_seen;
                        resume_pending = false;
                        holding_since = None;
                        skipping_video = false;
                        let detail = if destination.secure {
                            "TLS sertifikası doğrulandı; harici hedef yayını kabul etti"
                        } else {
                            "Harici RTMP hedefi yayını kabul etti"
                        };
                        set_output_status("ready", detail);
                    }
                    Err(error) => {
                        consecutive_failures = consecutive_failures.saturating_add(1);
                        OUTPUT_RECONNECT_ATTEMPTS.fetch_add(1, Ordering::Relaxed);
                        let delay = output_retry_delay(consecutive_failures);
                        next_retry_at = Instant::now() + delay;
                        set_output_status("reconnecting", &reconnect_detail(&error, delay));
                        DROPPED_OUTPUT_FRAMES.fetch_add(1, Ordering::Relaxed);
                        continue;
                    }
                },
                Err(error) => {
                    consecutive_failures = consecutive_failures.saturating_add(1);
                    OUTPUT_RECONNECT_ATTEMPTS.fetch_add(1, Ordering::Relaxed);
                    let delay = output_retry_delay(consecutive_failures);
                    next_retry_at = Instant::now() + delay;
                    set_output_status("reconnecting", &reconnect_detail(&error, delay));
                    DROPPED_OUTPUT_FRAMES.fetch_add(1, Ordering::Relaxed);
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
                DROPPED_OUTPUT_FRAMES.fetch_add(1, Ordering::Relaxed);
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
            DROPPED_OUTPUT_FRAMES.fetch_add(1, Ordering::Relaxed);
            set_output_status(
                "congested",
                "Yükleme hızı yetmiyor; görüntü bir sonraki anahtar kareye atlanıyor",
            );
            continue;
        }
        let Some(output_timestamp) = clock.map(timestamp) else {
            DROPPED_OUTPUT_FRAMES.fetch_add(1, Ordering::Relaxed);
            continue;
        };

        let send_result = headers_sent
            .and_then(|header_bytes| {
                send_output_frame(active_client, frame_type, output_timestamp, &payload)
                    .map(|()| header_bytes)
            })
            .and_then(|header_bytes| {
                if stall.stalled(unsent_output_bytes(active_client), Instant::now()) {
                    Err("hedef veri almayı bıraktı".to_owned())
                } else {
                    Ok(header_bytes)
                }
            });
        match send_result {
            Ok(header_bytes) => {
                OUTBOUND_BYTES.fetch_add(
                    header_bytes.saturating_add(payload.len() as u64),
                    Ordering::Relaxed,
                );
                if skipping_video {
                    set_output_status(
                        "congested",
                        "Yükleme hızı yetmiyor; görüntü bir sonraki anahtar kareye atlanıyor",
                    );
                } else {
                    let detail = if destination.secure {
                        "Yayın doğrulanmış TLS üzerinden harici hedefe aktarılıyor"
                    } else {
                        "Yayın harici RTMP hedefine aktarılıyor"
                    };
                    set_output_status("forwarding", detail);
                }
            }
            Err(error) => {
                client = None;
                holding_since = None;
                resume_pending = false;
                consecutive_failures = consecutive_failures.saturating_add(1);
                OUTPUT_RECONNECT_ATTEMPTS.fetch_add(1, Ordering::Relaxed);
                let delay = output_retry_delay(consecutive_failures);
                next_retry_at = Instant::now() + delay;
                set_output_status("reconnecting", &reconnect_detail(&error, delay));
                DROPPED_OUTPUT_FRAMES.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
    if let Some(mut active_client) = client.take() {
        flush_outbound_client(&mut active_client);
    }
    set_output_status("stopped", "Harici RTMP aktarımı durduruldu");
}

fn flush_outbound_client(client: &mut Client) {
    let deadline = Instant::now() + Duration::from_secs(2);
    while client.send_buffer.available() > 0 && Instant::now() < deadline {
        if client.poll(10).is_err() {
            break;
        }
    }
}

fn set_ingest_error(detail: String) {
    let mut value = current_snapshot();
    value.status = "error".to_owned();
    value.detail = detail;
    value.bitrate_kbps = 0.0;
    value.remote_address = None;
    apply_output_state(&mut value);
    set_snapshot(value);
}

pub fn stop_server() {
    detach_destination();
    preview::source_ended();
    let current = control().lock().ok().and_then(|mut value| value.take());
    if let Some(runtime) = current {
        runtime.stop.store(true, Ordering::Release);
        let _ = runtime.server_thread.join();
    }
    reset_source_bootstrap();
}

pub fn current_snapshot() -> RelaySnapshot {
    snapshot()
        .read()
        .map(|value| value.clone())
        .unwrap_or_else(|_| RelaySnapshot::stopped())
}

pub fn snapshot_json() -> String {
    serde_json::to_string(&current_snapshot())
        .unwrap_or_else(|_| "{\"status\":\"error\",\"detail\":\"Durum okunamadı\"}".to_owned())
}

fn java_string(env: JNIEnv<'_>, value: &str) -> jstring {
    env.new_string(value)
        .map(|result| result.into_raw())
        .unwrap_or(ptr::null_mut())
}

/// Starts receiving DJI Fly without a platform; returns an error message or an empty string.
#[no_mangle]
pub extern "system" fn Java_com_djilivebridge_android_NativeRelay_nativeStartReceiver(
    env: JNIEnv<'_>,
    _class: JClass<'_>,
) -> jstring {
    let result = start_server().err().unwrap_or_default();
    java_string(env, &result)
}

/// Starts sending the received stream to a platform; returns an error message or an empty string.
#[no_mangle]
pub extern "system" fn Java_com_djilivebridge_android_NativeRelay_nativeGoLive(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    target_server_url: JString<'_>,
    target_stream_key: JString<'_>,
    tls_ca_file: JString<'_>,
) -> jstring {
    let target_server_url: String = match env.get_string(&target_server_url) {
        Ok(value) => value.into(),
        Err(error) => return java_string(env, &format!("Hedef adresi okunamadı: {error}")),
    };
    let target_stream_key: String = match env.get_string(&target_stream_key) {
        Ok(value) => value.into(),
        Err(_) => return java_string(env, "Hedef yayın anahtarı okunamadı"),
    };
    let tls_ca_file: String = match env.get_string(&tls_ca_file) {
        Ok(value) => value.into(),
        Err(_) => return java_string(env, "Android sertifika deposu okunamadı"),
    };
    let result = set_destination_with_tls_ca(&target_server_url, &target_stream_key, &tls_ca_file)
        .err()
        .unwrap_or_default();
    java_string(env, &result)
}

/// Stops sending to the platform while the drone stays connected.
#[no_mangle]
pub extern "system" fn Java_com_djilivebridge_android_NativeRelay_nativeEndLive(
    _env: JNIEnv<'_>,
    _class: JClass<'_>,
) {
    clear_destination();
}

#[no_mangle]
pub extern "system" fn Java_com_djilivebridge_android_NativeRelay_nativeSnapshot(
    env: JNIEnv<'_>,
    _class: JClass<'_>,
) -> jstring {
    java_string(env, &snapshot_json())
}

#[no_mangle]
pub extern "system" fn Java_com_djilivebridge_android_NativeRelay_nativeStop(
    _env: JNIEnv<'_>,
    _class: JClass<'_>,
) {
    stop_server();
}

/// Starts an in-app preview session and returns its number for the calls below.
#[no_mangle]
pub extern "system" fn Java_com_djilivebridge_android_NativeRelay_nativePreviewStart(
    _env: JNIEnv<'_>,
    _class: JClass<'_>,
) -> jlong {
    preview::start() as jlong
}

/// Waits up to `timeout_ms` for the next video tag of `session`: a 4-byte big-endian RTMP
/// timestamp followed by the FLV video tag body, or null when nothing arrived.
#[no_mangle]
pub extern "system" fn Java_com_djilivebridge_android_NativeRelay_nativePreviewNext(
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
pub extern "system" fn Java_com_djilivebridge_android_NativeRelay_nativePreviewStop(
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
        assert!(build_destination("rtmps://example.com/live", "target-key-1234", "").is_err());
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
            set_destination_with_tls_ca("rtmp://127.0.0.1:9/live", "target-key-1234", ""),
            Err("RTMP alıcısı çalışmıyor".to_owned())
        );
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

    #[test]
    fn snapshot_is_valid_json() {
        let parsed: serde_json::Value = serde_json::from_str(&snapshot_json()).unwrap();
        assert!(parsed.get("status").is_some());
        assert!(parsed.get("receivedBytes").is_some());
        assert!(parsed.get("outputStatus").is_some());
        assert!(parsed.get("outboundBytes").is_some());
    }
}
