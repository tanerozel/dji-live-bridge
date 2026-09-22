use jni::objects::{JClass, JString};
use jni::sys::jstring;
use jni::JNIEnv;
use librtmp2::server::Server;
use librtmp2::types::{Frame, FrameType, ServerConfig};
use serde::Serialize;
use std::ptr;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, RwLock};
use std::thread::{self, JoinHandle};
use std::time::Instant;

const DEFAULT_BIND_ADDRESS: &str = "0.0.0.0:1935";
const EXPECTED_APP: &str = "live";

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
        }
    }
}

struct RuntimeControl {
    stop: Arc<AtomicBool>,
    thread: JoinHandle<()>,
}

static CONTROL: OnceLock<Mutex<Option<RuntimeControl>>> = OnceLock::new();
static SNAPSHOT: OnceLock<RwLock<RelaySnapshot>> = OnceLock::new();
static EXPECTED_STREAM_KEY: OnceLock<RwLock<String>> = OnceLock::new();
static VIDEO_FRAMES: AtomicU64 = AtomicU64::new(0);
static AUDIO_FRAMES: AtomicU64 = AtomicU64::new(0);
static REJECTED_PUBLISH_ATTEMPTS: AtomicU64 = AtomicU64::new(0);

fn control() -> &'static Mutex<Option<RuntimeControl>> {
    CONTROL.get_or_init(|| Mutex::new(None))
}

fn snapshot() -> &'static RwLock<RelaySnapshot> {
    SNAPSHOT.get_or_init(|| RwLock::new(RelaySnapshot::stopped()))
}

fn expected_stream_key() -> &'static RwLock<String> {
    EXPECTED_STREAM_KEY.get_or_init(|| RwLock::new(String::new()))
}

fn set_snapshot(value: RelaySnapshot) {
    if let Ok(mut current) = snapshot().write() {
        *current = value;
    }
}

fn allow_publish(_conn_id: u64, app: &str, stream_name: &str) -> bool {
    let allowed = app == EXPECTED_APP
        && expected_stream_key()
            .read()
            .map(|expected| constant_time_eq(expected.as_bytes(), stream_name.as_bytes()))
            .unwrap_or(false);

    if !allowed {
        REJECTED_PUBLISH_ATTEMPTS.fetch_add(1, Ordering::Relaxed);
    }
    allowed
}

fn count_frame(frame: &Frame) {
    match frame.frame_type {
        FrameType::Video => {
            VIDEO_FRAMES.fetch_add(1, Ordering::Relaxed);
        }
        FrameType::Audio => {
            AUDIO_FRAMES.fetch_add(1, Ordering::Relaxed);
        }
        _ => {}
    }
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }

    let mut difference = 0u8;
    for (left_byte, right_byte) in left.iter().zip(right.iter()) {
        difference |= left_byte ^ right_byte;
    }
    difference == 0
}

fn valid_stream_key(stream_key: &str) -> bool {
    (16..=128).contains(&stream_key.len())
        && stream_key
            .bytes()
            .all(|value| value.is_ascii_alphanumeric() || value == b'-' || value == b'_')
}

pub fn start_server(stream_key: &str) -> Result<(), String> {
    start_server_on(DEFAULT_BIND_ADDRESS, stream_key)
}

pub fn start_server_on(bind_address: &str, stream_key: &str) -> Result<(), String> {
    if !valid_stream_key(stream_key) {
        return Err(
            "Yayın anahtarı en az 16 karakter olmalı ve yalnızca harf, rakam, - veya _ içermeli"
                .to_owned(),
        );
    }

    stop_server();
    VIDEO_FRAMES.store(0, Ordering::Relaxed);
    AUDIO_FRAMES.store(0, Ordering::Relaxed);
    REJECTED_PUBLISH_ATTEMPTS.store(0, Ordering::Relaxed);
    if let Ok(mut expected) = expected_stream_key().write() {
        *expected = stream_key.to_owned();
    }

    let stop = Arc::new(AtomicBool::new(false));
    let thread_stop = Arc::clone(&stop);
    let bind_address = bind_address.to_owned();
    let thread = thread::Builder::new()
        .name("dji-rtmp-ingest".to_owned())
        .spawn(move || run_server(&bind_address, thread_stop))
        .map_err(|error| format!("RTMP iş parçacığı başlatılamadı: {error}"))?;

    let mut current = control()
        .lock()
        .map_err(|_| "RTMP çalışma durumu kilitlenemedi".to_owned())?;
    *current = Some(RuntimeControl { stop, thread });
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
            set_error(format!("RTMP sunucusu oluşturulamadı: {error}"));
            return;
        }
    };
    server.on_publish_cb = Some(allow_publish);
    server.on_frame_cb = Some(count_frame);

    if let Err(error) = server.listen(bind_address) {
        set_error(format!("{bind_address} dinlenemedi: {error}"));
        return;
    }

    set_snapshot(RelaySnapshot {
        status: "listening".to_owned(),
        detail: "RC 2 yayını bekleniyor".to_owned(),
        ..RelaySnapshot::stopped()
    });

    let mut previous_bytes = 0u64;
    let mut previous_sample = Instant::now();
    let mut bitrate_kbps = 0.0;

    while !stop.load(Ordering::Acquire) {
        if let Err(error) = server.poll(25) {
            set_error(format!("RTMP alıcısı durdu: {error}"));
            server.stop();
            return;
        }

        let publisher = server.connections.iter().find(|connection| {
            connection
                .current_stream
                .as_ref()
                .is_some_and(|stream| stream.is_publishing)
        });
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

        set_snapshot(RelaySnapshot {
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
        });
    }

    server.stop();
    let mut final_snapshot = current_snapshot();
    final_snapshot.status = "stopped".to_owned();
    final_snapshot.detail = "RTMP alıcısı kapalı".to_owned();
    final_snapshot.bitrate_kbps = 0.0;
    final_snapshot.remote_address = None;
    set_snapshot(final_snapshot);
}

fn set_error(detail: String) {
    let mut value = current_snapshot();
    value.status = "error".to_owned();
    value.detail = detail;
    value.bitrate_kbps = 0.0;
    value.remote_address = None;
    set_snapshot(value);
}

pub fn stop_server() {
    let current = control().lock().ok().and_then(|mut value| value.take());
    if let Some(runtime) = current {
        runtime.stop.store(true, Ordering::Release);
        let _ = runtime.thread.join();
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
    stream_key: JString<'_>,
) -> jstring {
    let stream_key: String = match env.get_string(&stream_key) {
        Ok(value) => value.into(),
        Err(error) => return java_string(env, &format!("Yayın anahtarı okunamadı: {error}")),
    };
    let result = start_server(&stream_key).err().unwrap_or_default();
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
    fn stream_key_validation_rejects_short_or_unsafe_values() {
        assert!(!valid_stream_key("short"));
        assert!(!valid_stream_key("123456789012345/unsafe"));
        assert!(valid_stream_key("AbCdEf1234567890-_"));
    }

    #[test]
    fn constant_time_comparison_checks_content_and_length() {
        assert!(constant_time_eq(b"same-key", b"same-key"));
        assert!(!constant_time_eq(b"same-key", b"diff-key"));
        assert!(!constant_time_eq(b"short", b"longer"));
    }

    #[test]
    fn snapshot_is_valid_json() {
        let parsed: serde_json::Value = serde_json::from_str(&snapshot_json()).unwrap();
        assert!(parsed.get("status").is_some());
        assert!(parsed.get("receivedBytes").is_some());
    }
}
