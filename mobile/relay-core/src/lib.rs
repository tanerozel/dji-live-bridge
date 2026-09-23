use jni::objects::{JClass, JString};
use jni::sys::jstring;
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

const DEFAULT_BIND_ADDRESS: &str = "0.0.0.0:1935";
const EXPECTED_APP: &str = "drone";
const OUTBOUND_QUEUE_CAPACITY: usize = 256;
const MAX_QUEUED_FRAME_BYTES: usize = 16 * 1024 * 1024;
const OUTPUT_CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
const OUTPUT_RETRY_MAX_DELAY: Duration = Duration::from_secs(15);

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

enum OutboundMessage {
    Frame {
        frame_type: FrameType,
        timestamp: u32,
        payload: Vec<u8>,
    },
    SourceEnded,
}

struct RuntimeControl {
    stop: Arc<AtomicBool>,
    server_thread: JoinHandle<()>,
    outbound_thread: Option<JoinHandle<()>>,
}

static CONTROL: OnceLock<Mutex<Option<RuntimeControl>>> = OnceLock::new();
static SNAPSHOT: OnceLock<RwLock<RelaySnapshot>> = OnceLock::new();
static OUTBOUND_SNAPSHOT: OnceLock<RwLock<OutboundSnapshot>> = OnceLock::new();
static OUTBOUND_SENDER: OnceLock<RwLock<Option<SyncSender<OutboundMessage>>>> = OnceLock::new();
static VIDEO_FRAMES: AtomicU64 = AtomicU64::new(0);
static AUDIO_FRAMES: AtomicU64 = AtomicU64::new(0);
static REJECTED_PUBLISH_ATTEMPTS: AtomicU64 = AtomicU64::new(0);
static OUTBOUND_BYTES: AtomicU64 = AtomicU64::new(0);
static DROPPED_OUTPUT_FRAMES: AtomicU64 = AtomicU64::new(0);
static OUTPUT_RECONNECT_ATTEMPTS: AtomicU64 = AtomicU64::new(0);
static OUTBOUND_QUEUE_OVERFLOWED: AtomicBool = AtomicBool::new(false);
static OUTPUT_SECURE: AtomicBool = AtomicBool::new(false);

fn control() -> &'static Mutex<Option<RuntimeControl>> {
    CONTROL.get_or_init(|| Mutex::new(None))
}

fn snapshot() -> &'static RwLock<RelaySnapshot> {
    SNAPSHOT.get_or_init(|| RwLock::new(RelaySnapshot::stopped()))
}

fn outbound_snapshot() -> &'static RwLock<OutboundSnapshot> {
    OUTBOUND_SNAPSHOT.get_or_init(|| RwLock::new(OutboundSnapshot::disabled()))
}

fn outbound_sender() -> &'static RwLock<Option<SyncSender<OutboundMessage>>> {
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

    let sender = outbound_sender()
        .read()
        .ok()
        .and_then(|value| value.clone());
    let Some(sender) = sender else {
        return;
    };
    let payload = if frame.size == 0 {
        Vec::new()
    } else {
        // librtmp2 guarantees Frame.data remains valid for the duration of this callback.
        unsafe { std::slice::from_raw_parts(frame.data, frame.size as usize) }.to_vec()
    };
    let message = OutboundMessage::Frame {
        frame_type: frame.frame_type,
        timestamp: frame.timestamp,
        payload,
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
    let sender = outbound_sender()
        .read()
        .ok()
        .and_then(|value| value.clone());
    if let Some(sender) = sender {
        let _ = sender.try_send(OutboundMessage::SourceEnded);
    }
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

fn start_runtime(bind_address: &str, destination: Option<Destination>) -> Result<(), String> {
    stop_server();
    let mut control_slot = control()
        .lock()
        .map_err(|_| "RTMP çalışma durumu kilitlenemedi".to_owned())?;
    VIDEO_FRAMES.store(0, Ordering::Relaxed);
    AUDIO_FRAMES.store(0, Ordering::Relaxed);
    REJECTED_PUBLISH_ATTEMPTS.store(0, Ordering::Relaxed);
    OUTBOUND_BYTES.store(0, Ordering::Relaxed);
    DROPPED_OUTPUT_FRAMES.store(0, Ordering::Relaxed);
    OUTPUT_RECONNECT_ATTEMPTS.store(0, Ordering::Relaxed);
    OUTBOUND_QUEUE_OVERFLOWED.store(false, Ordering::Relaxed);
    OUTPUT_SECURE.store(
        destination.as_ref().is_some_and(|value| value.secure),
        Ordering::Relaxed,
    );
    let stop = Arc::new(AtomicBool::new(false));
    let outbound_thread = if let Some(destination) = destination {
        let (sender, receiver) = mpsc::sync_channel(OUTBOUND_QUEUE_CAPACITY);
        if let Ok(mut current) = outbound_sender().write() {
            *current = Some(sender);
        }
        set_output_status("armed", "Kaynak yayın gelince hedefe bağlanacak");
        let outbound_stop = Arc::clone(&stop);
        match thread::Builder::new()
            .name("dji-rtmp-output".to_owned())
            .spawn(move || run_outbound(destination, receiver, outbound_stop))
        {
            Ok(thread) => Some(thread),
            Err(error) => {
                if let Ok(mut current) = outbound_sender().write() {
                    *current = None;
                }
                set_output_status("disabled", "Harici hedef başlatılamadı");
                return Err(format!("Hedef RTMP iş parçacığı başlatılamadı: {error}"));
            }
        }
    } else {
        if let Ok(mut current) = outbound_sender().write() {
            *current = None;
        }
        set_output_status("disabled", "Harici hedef yapılandırılmadı");
        None
    };

    let server_stop = Arc::clone(&stop);
    let bind_address = bind_address.to_owned();
    let server_thread = match thread::Builder::new()
        .name("dji-rtmp-ingest".to_owned())
        .spawn(move || run_server(&bind_address, server_stop))
    {
        Ok(thread) => thread,
        Err(error) => {
            stop.store(true, Ordering::Release);
            if let Ok(mut current) = outbound_sender().write() {
                *current = None;
            }
            if let Some(thread) = outbound_thread {
                let _ = thread.join();
            }
            return Err(format!("RTMP iş parçacığı başlatılamadı: {error}"));
        }
    };

    *control_slot = Some(RuntimeControl {
        stop,
        server_thread,
        outbound_thread,
    });
    Ok(())
}

fn run_server(bind_address: &str, stop: Arc<AtomicBool>) {
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

    while !stop.load(Ordering::Acquire) {
        if let Err(error) = server.poll(25) {
            set_ingest_error(format!("RTMP alıcısı durdu: {error}"));
            server.stop();
            return;
        }

        let publisher = server.connections.iter().find(|connection| {
            connection
                .current_stream
                .as_ref()
                .is_some_and(|stream| stream.is_publishing)
        });
        let source_is_publishing = publisher.is_some();
        if source_was_publishing && !source_is_publishing {
            notify_source_ended();
        }
        source_was_publishing = source_is_publishing;

        let received_bytes = publisher
            .map(|connection| connection.media_bytes_received)
            .unwrap_or(0);
        let now = Instant::now();
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
    server.stop();
    let mut final_snapshot = current_snapshot();
    final_snapshot.status = "stopped".to_owned();
    final_snapshot.detail = "RTMP alıcısı kapalı".to_owned();
    final_snapshot.bitrate_kbps = 0.0;
    final_snapshot.remote_address = None;
    apply_output_state(&mut final_snapshot);
    set_snapshot(final_snapshot);
}

#[derive(Default)]
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
        let mut sent_bytes = 0u64;
        for (frame_type, payload) in frames {
            if let Some(payload) = payload {
                send_output_frame(client, frame_type, 0, payload)?;
                sent_bytes = sent_bytes.saturating_add(payload.len() as u64);
            }
        }
        Ok(sent_bytes)
    }
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

fn run_outbound(
    destination: Destination,
    receiver: Receiver<OutboundMessage>,
    stop: Arc<AtomicBool>,
) {
    let mut client: Option<Client> = None;
    let mut bootstrap = MediaBootstrap::default();
    let mut consecutive_failures = 0u32;
    let mut next_retry_at = Instant::now();
    let mut timestamp_base: Option<u32> = None;
    let mut waiting_for_keyframe = false;
    let transport_name = if destination.secure { "RTMPS" } else { "RTMP" };

    while !stop.load(Ordering::Acquire) {
        if OUTBOUND_QUEUE_OVERFLOWED.swap(false, Ordering::AcqRel) {
            client = None;
            consecutive_failures = consecutive_failures.saturating_add(1);
            OUTPUT_RECONNECT_ATTEMPTS.fetch_add(1, Ordering::Relaxed);
            let delay = output_retry_delay(consecutive_failures);
            next_retry_at = Instant::now() + delay;
            timestamp_base = None;
            waiting_for_keyframe = bootstrap.video_seen;
            set_output_status(
                "reconnecting",
                &reconnect_detail("çıkış tamponu doldu", delay),
            );
        }

        match receiver.recv_timeout(Duration::from_millis(50)) {
            Ok(OutboundMessage::Frame {
                frame_type,
                timestamp,
                payload,
            }) => {
                bootstrap.observe(frame_type, &payload);

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
                        Ok(mut next_client) => {
                            match bootstrap.send_to(&mut next_client) {
                                Ok(bytes) => {
                                    OUTBOUND_BYTES.fetch_add(bytes, Ordering::Relaxed);
                                }
                                Err(error) => {
                                    consecutive_failures = consecutive_failures.saturating_add(1);
                                    OUTPUT_RECONNECT_ATTEMPTS.fetch_add(1, Ordering::Relaxed);
                                    let delay = output_retry_delay(consecutive_failures);
                                    next_retry_at = Instant::now() + delay;
                                    set_output_status(
                                        "reconnecting",
                                        &reconnect_detail(&error, delay),
                                    );
                                    DROPPED_OUTPUT_FRAMES.fetch_add(1, Ordering::Relaxed);
                                    continue;
                                }
                            }
                            client = Some(next_client);
                            consecutive_failures = 0;
                            timestamp_base = None;
                            waiting_for_keyframe = bootstrap.video_seen;
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
                    }
                }

                if waiting_for_keyframe {
                    if !is_video_keyframe(&payload) {
                        DROPPED_OUTPUT_FRAMES.fetch_add(1, Ordering::Relaxed);
                        continue;
                    }
                    waiting_for_keyframe = false;
                    timestamp_base = Some(timestamp);
                }
                let output_timestamp =
                    timestamp.wrapping_sub(*timestamp_base.get_or_insert(timestamp));
                let send_result = client.as_mut().map(|active_client| {
                    send_output_frame(active_client, frame_type, output_timestamp, &payload)
                });
                if let Some(Err(error)) = send_result {
                    client = None;
                    consecutive_failures = consecutive_failures.saturating_add(1);
                    OUTPUT_RECONNECT_ATTEMPTS.fetch_add(1, Ordering::Relaxed);
                    let delay = output_retry_delay(consecutive_failures);
                    next_retry_at = Instant::now() + delay;
                    timestamp_base = None;
                    waiting_for_keyframe = bootstrap.video_seen;
                    set_output_status("reconnecting", &reconnect_detail(&error, delay));
                    DROPPED_OUTPUT_FRAMES.fetch_add(1, Ordering::Relaxed);
                    continue;
                }
                OUTBOUND_BYTES.fetch_add(payload.len() as u64, Ordering::Relaxed);
                let detail = if destination.secure {
                    "Yayın doğrulanmış TLS üzerinden harici hedefe aktarılıyor"
                } else {
                    "Yayın harici RTMP hedefine aktarılıyor"
                };
                set_output_status("forwarding", detail);
            }
            Ok(OutboundMessage::SourceEnded) => {
                if let Some(mut active_client) = client.take() {
                    flush_outbound_client(&mut active_client);
                }
                bootstrap = MediaBootstrap::default();
                consecutive_failures = 0;
                next_retry_at = Instant::now();
                timestamp_base = None;
                waiting_for_keyframe = false;
                OUTBOUND_QUEUE_OVERFLOWED.store(false, Ordering::Relaxed);
                set_output_status("armed", "Kaynak yayın gelince hedefe bağlanacak");
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if let Some(active_client) = client.as_mut() {
                    if let Err(error) = active_client.poll(0) {
                        client = None;
                        consecutive_failures = consecutive_failures.saturating_add(1);
                        OUTPUT_RECONNECT_ATTEMPTS.fetch_add(1, Ordering::Relaxed);
                        let delay = output_retry_delay(consecutive_failures);
                        next_retry_at = Instant::now() + delay;
                        timestamp_base = None;
                        waiting_for_keyframe = bootstrap.video_seen;
                        set_output_status(
                            "reconnecting",
                            &reconnect_detail(&error.to_string(), delay),
                        );
                    }
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
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
    let current = control().lock().ok().and_then(|mut value| value.take());
    if let Some(runtime) = current {
        runtime.stop.store(true, Ordering::Release);
        if let Ok(mut sender) = outbound_sender().write() {
            *sender = None;
        }
        let _ = runtime.server_thread.join();
        if let Some(thread) = runtime.outbound_thread {
            let _ = thread.join();
        }
    } else if let Ok(mut sender) = outbound_sender().write() {
        *sender = None;
    }
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

#[no_mangle]
pub extern "system" fn Java_com_djilivebridge_android_NativeRelay_nativeStart(
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
    let result = start_bridge_with_tls_ca(&target_server_url, &target_stream_key, &tls_ca_file)
        .err()
        .unwrap_or_default();
    java_string(env, &result)
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
    fn snapshot_is_valid_json() {
        let parsed: serde_json::Value = serde_json::from_str(&snapshot_json()).unwrap();
        assert!(parsed.get("status").is_some());
        assert!(parsed.get("receivedBytes").is_some());
        assert!(parsed.get("outputStatus").is_some());
        assert!(parsed.get("outboundBytes").is_some());
    }
}
