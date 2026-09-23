//! Core type definitions for librtmp2
//!
//! This module contains all public types, enums, structs, and constants
//! that mirror the C header `include/librtmp2/types.h` and related headers.

#![allow(dead_code)]

use std::ffi::c_int;

/* ── Opaque forward-declared types ── */
// These are opaque in the C API; in Rust they are pub(crate) structs.

/// Opaque server handle
#[repr(C)]
pub struct Server {
    _private: [u8; 0],
}

/// Opaque client handle
#[repr(C)]
pub struct Client {
    _private: [u8; 0],
}

/// Opaque connection handle
#[repr(C)]
pub struct Conn {
    _private: [u8; 0],
}

/// Opaque stream handle
#[repr(C)]
pub struct Stream {
    _private: [u8; 0],
}

/* ── Error codes ── */

/// Error codes returned by librtmp2 functions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[repr(C)]
pub enum ErrorCode {
    /// Operation succeeded
    Ok = 0,
    /// I/O error
    Io = -1,
    /// Timeout
    Timeout = -2,
    /// Protocol error
    Protocol = -3,
    /// Handshake error
    Handshake = -4,
    /// Chunk error
    Chunk = -5,
    /// AMF error
    Amf = -6,
    /// Unsupported feature
    Unsupported = -7,
    /// Authentication error
    Auth = -8,
    /// Internal error
    Internal = -9,
}

impl ErrorCode {
    /// Convert an error code to a human-readable string.
    pub fn as_str(self) -> &'static str {
        match self {
            ErrorCode::Ok => "OK",
            ErrorCode::Io => "I/O error",
            ErrorCode::Timeout => "Timeout",
            ErrorCode::Protocol => "Protocol error",
            ErrorCode::Handshake => "Handshake error",
            ErrorCode::Chunk => "Chunk error",
            ErrorCode::Amf => "AMF error",
            ErrorCode::Unsupported => "Unsupported",
            ErrorCode::Auth => "Authentication error",
            ErrorCode::Internal => "Internal error",
        }
    }
}

impl std::fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Result type used throughout the library.
pub type Result<T> = std::result::Result<T, ErrorCode>;

/* ── Connection state machine ── */

/// Connection state machine states.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(C)]
pub enum ConnState {
    TcpAccepted = 0,
    Handshake,
    Connected,
    CapsNegotiated,
    AppConnected,
    StreamCreated,
    Publishing,
    Playing,
    Closing,
    Closed,
}

/* ── Frame types ── */

/// Frame type classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub enum FrameType {
    Audio = 0,
    Video = 1,
    Script = 2,
    Metadata = 3,
}

/* ── Audio codec IDs ── */

/// Audio codec identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub enum AudioCodec {
    Pcm = 0,
    Adpcm = 1,
    Mp3 = 2,
    PcmLe = 3,
    Nelly16k = 4,
    Nelly8k = 5,
    Nelly = 6,
    G711A = 7,
    G711U = 8,
    Aac = 10,
    Speex = 11,
    Opus = 14,
}

impl Default for AudioCodec {
    fn default() -> Self {
        AudioCodec::Pcm
    }
}

/* ── Video codec IDs (legacy) ── */

/// Video codec identifiers (legacy).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub enum VideoCodec {
    Jpeg = 1,
    Sorenson = 2,
    Screen = 3,
    Vp6 = 4,
    Vp6a = 5,
    Screen2 = 6,
    H264 = 7,
    H265 = 12,
    Av1 = 13,
    Vp9 = 14,
}

impl Default for VideoCodec {
    fn default() -> Self {
        VideoCodec::Jpeg
    }
}

/* ── FourCC for E-RTMP ── */

/// FourCC code for Enhanced RTMP.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(C)]
pub struct FourCc {
    /// null-terminated, e.g. "hvc1"
    pub cc: [u8; 5],
}

/* ── Parsed frame ── */

/// A parsed frame delivered via the `on_frame` callback.
#[derive(Debug, Clone)]
#[repr(C)]
pub struct Frame {
    pub frame_type: FrameType,
    pub timestamp: u32,
    pub composition_time: u32,
    pub size: u32,
    pub data: *const u8,
    /// Audio-specific
    pub audio_codec: AudioCodec,
    pub audio_sample_rate: u32,
    pub audio_channels: u8,
    pub audio_bit_depth: u8,
    /// Enhanced: FourCC from ExAudioTagHeader
    pub audio_fourcc: FourCc,
    /// Video-specific
    pub video_codec: VideoCodec,
    pub video_fourcc: FourCc,
    /// 1=keyframe, 2=inter, etc.
    pub video_frame_type: u8,
    /// Script/metadata flag
    pub is_metadata: u8,
    /// E-RTMP multitrack id (0 = default track; 255 = not set / single-track).
    pub track_id: u8,
}

impl Default for Frame {
    fn default() -> Self {
        Self {
            frame_type: FrameType::Audio,
            timestamp: 0,
            composition_time: 0,
            size: 0,
            data: std::ptr::null(),
            audio_codec: AudioCodec::default(),
            audio_sample_rate: 0,
            audio_channels: 0,
            audio_bit_depth: 0,
            audio_fourcc: FourCc::default(),
            video_codec: VideoCodec::default(),
            video_fourcc: FourCc::default(),
            video_frame_type: 0,
            is_metadata: 0,
            track_id: u8::MAX,
        }
    }
}

/* ── Error info ── */

/// Error information struct.
#[derive(Debug, Clone)]
#[repr(C)]
pub struct ErrorInfo {
    pub code: ErrorCode,
    pub message: [u8; 256],
}

impl Default for ErrorInfo {
    fn default() -> Self {
        Self {
            code: ErrorCode::Ok,
            message: [0; 256],
        }
    }
}

/* ── Server config ── */

/// Per-connection and server-wide memory limits (Rust-only; not part of the C ABI).
#[derive(Debug, Clone, Copy)]
pub struct ResourceLimits {
    /// Total bytes retained across all stream-cache entries.
    pub max_stream_cache_bytes: usize,
    /// Max reassembly buffer bytes per RTMP connection.
    pub max_reassembly_bytes: usize,
    /// Max queued relay payload bytes per publisher connection.
    pub max_pending_relay_bytes: usize,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_stream_cache_bytes: 64 * 1024 * 1024,
            max_reassembly_bytes: 32 * 1024 * 1024,
            max_pending_relay_bytes: 8 * 1024 * 1024,
        }
    }
}

/// Server configuration struct.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct ServerConfig {
    pub max_connections: c_int,
    pub chunk_size: c_int,
    /// TLS / RTMPS fields
    pub tls_enabled: c_int,
    /// Server: PEM certificate chain file
    pub tls_cert_file: *const u8,
    /// Server: PEM private key file
    pub tls_key_file: *const u8,
    /// Client: CA bundle for verification (or NULL)
    pub tls_ca_file: *const u8,
    /// Client: skip certificate verification
    pub tls_insecure: c_int,
    /// Max incomplete TLS handshakes retained per remote peer IP before the
    /// oldest is evicted to admit a new one. `0` (or negative) uses the
    /// built-in default (see `server::DEFAULT_MAX_PENDING_TLS_PER_ADDR`).
    /// Appended field: see `docs/abi-policy.md` (config struct fields may be
    /// appended between minor releases).
    pub max_pending_tls_per_addr: c_int,
    /// Max active (already-accepted) connections retained per remote peer
    /// IP. `0` (or negative) uses the built-in default (see
    /// `server::DEFAULT_MAX_CONNECTIONS_PER_ADDR`). Kept independent of
    /// `max_pending_tls_per_addr` so tuning one does not change the other:
    /// they defend different resources (the connection table vs. the
    /// pending-handshake queue). Appended field: see `docs/abi-policy.md`.
    pub max_connections_per_addr: c_int,
}

/* ── Callback types ── */

/// on_connect callback type
pub type OnConnectCb = Option<unsafe extern "C" fn(conn: *mut Conn, userdata: *mut u8) -> c_int>;
/// on_publish callback type
pub type OnPublishCb = Option<
    unsafe extern "C" fn(
        conn: *mut Conn,
        app: *const u8,
        stream_key: *const u8,
        userdata: *mut u8,
    ) -> c_int,
>;
/// on_play callback type
pub type OnPlayCb = Option<
    unsafe extern "C" fn(
        conn: *mut Conn,
        app: *const u8,
        stream_key: *const u8,
        userdata: *mut u8,
    ) -> c_int,
>;
/// on_frame callback type
pub type OnFrameCb =
    Option<unsafe extern "C" fn(conn: *mut Conn, frame: *const Frame, userdata: *mut u8) -> c_int>;
/// on_close callback type
pub type OnCloseCb = Option<unsafe extern "C" fn(conn: *mut Conn, userdata: *mut u8)>;
/// on_send_data callback type
pub type OnSendDataCb = Option<
    unsafe extern "C" fn(conn: *mut Conn, data: *const u8, len: usize, userdata: *mut u8) -> c_int,
>;

/* ── Version constants ── */

/// Parses a plain-digit version component (as provided by Cargo's
/// `CARGO_PKG_VERSION_*` env vars) into a `u32` at compile time.
const fn parse_version_component(s: &str) -> u32 {
    let bytes = s.as_bytes();
    let mut value: u32 = 0;
    let mut i = 0;
    while i < bytes.len() {
        value = value * 10 + (bytes[i] - b'0') as u32;
        i += 1;
    }
    value
}

pub const VERSION_MAJOR: u32 = parse_version_component(env!("CARGO_PKG_VERSION_MAJOR"));
pub const VERSION_MINOR: u32 = parse_version_component(env!("CARGO_PKG_VERSION_MINOR"));
pub const VERSION_PATCH: u32 = parse_version_component(env!("CARGO_PKG_VERSION_PATCH"));
pub const VERSION_STRING: &str = env!("CARGO_PKG_VERSION");

/* ── E-RTMP constants ── */

/// Enhanced RTMP v1 VideoPacketType
pub const ERTMP_PACKET_TYPE_SEQUENCE_START: u8 = 0;
pub const ERTMP_PACKET_TYPE_CODED_FRAMES: u8 = 1;
pub const ERTMP_PACKET_TYPE_SEQUENCE_END: u8 = 2;
pub const ERTMP_PACKET_TYPE_CODED_FRAMES_X: u8 = 3;
pub const ERTMP_PACKET_TYPE_METADATA: u8 = 4;
pub const ERTMP_PACKET_TYPE_MPEG2TS_SEQUENCE_START: u8 = 5;

/// Enhanced RTMP v1/v2 AudioPacketType
pub const ERTMP_AUDIO_PACKET_TYPE_SEQUENCE_START: u8 = 0;
pub const ERTMP_AUDIO_PACKET_TYPE_CODED_FRAMES: u8 = 1;
pub const ERTMP_AUDIO_PACKET_TYPE_SEQUENCE_END: u8 = 2;
pub const ERTMP_AUDIO_PACKET_TYPE_METADATA: u8 = 3;
pub const ERTMP_AUDIO_PACKET_TYPE_MULTICHANNEL: u8 = 4;
pub const ERTMP_AUDIO_PACKET_TYPE_MULTITRACK: u8 = 5;
pub const ERTMP_VIDEO_PACKET_TYPE_MULTITRACK: u8 = 6;

/// E-RTMP v2 `CapsExMask` bits carried in the connect `capsEx` AMF number.
pub const CAPS_EX_MASK_RECONNECT: u32 = 0x01;
pub const CAPS_EX_MASK_MULTITRACK: u32 = 0x02;
pub const CAPS_EX_MASK_MODEX: u32 = 0x04;
pub const CAPS_EX_MASK_TIMESTAMP_NANO: u32 = 0x08;
pub const CAPS_EX_MASK_SERVER_DEFAULT: u32 =
    CAPS_EX_MASK_MULTITRACK | CAPS_EX_MASK_MODEX | CAPS_EX_MASK_TIMESTAMP_NANO;

/// Enhanced RTMP v1 AudioSampleRate (legacy)
pub const ERTMP_AUDIO_RATE_5500: u8 = 0;
pub const ERTMP_AUDIO_RATE_11025: u8 = 1;
pub const ERTMP_AUDIO_RATE_22050: u8 = 2;
pub const ERTMP_AUDIO_RATE_44100: u8 = 3;

/// Enhanced RTMP v1 AudioSampleSize (legacy)
pub const ERTMP_AUDIO_SAMPLE_8BIT: u8 = 0;
pub const ERTMP_AUDIO_SAMPLE_16BIT: u8 = 1;

/* ── Parsed video tag header ── */

/// Parsed video tag header.
#[derive(Debug, Clone, Default)]
#[repr(C)]
pub struct VideoHeader {
    pub is_ex_header: u8,
    pub packet_type: u8,
    pub fourcc: [u8; 5],
    pub frame_type: u8,
    pub composition_time: u32,
    pub header_size: usize,
}

/* ── Parsed audio tag header ── */

/// Parsed audio tag header.
#[derive(Debug, Clone, Default)]
#[repr(C)]
pub struct AudioHeader {
    pub is_ex_header: u8,
    pub packet_type: u8,
    pub fourcc: [u8; 5],
    pub audio_codec: AudioCodec,
    pub sample_rate: u8,
    pub sample_size: u8,
    pub channels: u8,
    pub aac_packet_type: u8,
    pub header_size: usize,
}

/* ── HDR color info ── */

/// HDR color information.
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct HdrInfo {
    pub color_primaries: u16,
    pub transfer_chars: u16,
    pub matrix_coeffs: u16,
}

/* ── fourCcList ── */

/// Maximum number of FourCC entries in a list.
pub const MAX_FOURCCS: usize = 16;

/// A list of FourCC codes.
#[derive(Debug, Clone, Default)]
#[repr(C)]
pub struct FourCcList {
    pub entries: [FourCc; MAX_FOURCCS],
    pub count: usize,
}

/* ── E-RTMP v2 capability negotiation ── */

/// E-RTMP v2 per-codec capability flags.
pub const FOUR_CC_INFO_CAN_DECODE: u32 = 0x01;
pub const FOUR_CC_INFO_CAN_ENCODE: u32 = 0x02;
pub const FOUR_CC_INFO_CAN_FORWARD: u32 = 0x04;
pub const FOUR_CC_INFO_ALL: u32 =
    FOUR_CC_INFO_CAN_DECODE | FOUR_CC_INFO_CAN_ENCODE | FOUR_CC_INFO_CAN_FORWARD;

/// Video FourCC info map. Each entry's mask uses `FOUR_CC_INFO_*`.
#[derive(Debug, Clone, Default)]
#[repr(C)]
pub struct VideoFourCcInfoMap {
    pub entries: [FourCc; MAX_FOURCCS],
    pub masks: [u32; MAX_FOURCCS],
    pub count: usize,
}

/// Capability negotiation struct.
#[derive(Debug, Clone, Default)]
#[repr(C)]
pub struct CapsExit {
    pub version: u32,
    pub video_codec_32: i32,
    pub audio_codec_32: i32,
}

/* ── E-RTMP v2 reconnect ── */

/// Reconnect request struct.
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct Reconnect {
    pub replay: u32,
    pub limit: u32,
}

/* ── E-RTMP v2 multitrack ── */

/// Multitrack type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub enum MultitrackType {
    Audio = 0,
    Video = 1,
    Metadata = 2,
}

impl Default for MultitrackType {
    fn default() -> Self {
        MultitrackType::Audio
    }
}

/// Multitrack descriptor.
#[derive(Debug, Clone)]
#[repr(C)]
pub struct Multitrack {
    pub track_type: MultitrackType,
    pub track_name: [u8; 64],
}

impl Default for Multitrack {
    fn default() -> Self {
        Self {
            track_type: MultitrackType::Audio,
            track_name: [0; 64],
        }
    }
}

/* ── E-RTMP v2 ModEx ── */

/// ModEx type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub enum ModexType {
    Nop = 0,
    Timestamp = 1,
}

impl Default for ModexType {
    fn default() -> Self {
        ModexType::Nop
    }
}

/// ModEx extension.
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct Modex {
    pub modex_type: ModexType,
    pub offset: u64,
}

/* ── FLV tag types ── */

/// FLV audio tag.
#[derive(Debug, Clone)]
#[repr(C)]
pub struct AudioTag {
    pub codec: AudioCodec,
    pub sample_rate: u8,
    pub bit_depth: u8,
    pub channels: u8,
    pub aac_packet_type: u8,
    pub data: *const u8,
    pub size: usize,
}

impl Default for AudioTag {
    fn default() -> Self {
        Self {
            codec: AudioCodec::default(),
            sample_rate: 0,
            bit_depth: 0,
            channels: 0,
            aac_packet_type: 0,
            data: std::ptr::null(),
            size: 0,
        }
    }
}

/// FLV video tag.
#[derive(Debug, Clone)]
#[repr(C)]
pub struct VideoTag {
    pub frame_type: u8,
    pub codec: VideoCodec,
    pub avc_packet_type: u8,
    pub composition_time: u32,
    pub data: *const u8,
    pub size: usize,
}

impl Default for VideoTag {
    fn default() -> Self {
        Self {
            frame_type: 0,
            codec: VideoCodec::default(),
            avc_packet_type: 0,
            composition_time: 0,
            data: std::ptr::null(),
            size: 0,
        }
    }
}

/// FLV script tag.
#[derive(Debug, Clone)]
#[repr(C)]
pub struct ScriptTag {
    pub name: [u8; 64],
    pub data: *const u8,
    pub size: usize,
}

impl Default for ScriptTag {
    fn default() -> Self {
        Self {
            name: [0; 64],
            data: std::ptr::null(),
            size: 0,
        }
    }
}

/* ── Connect info ── */

/// Negotiated E-RTMP capability set returned in connect `_result`.
#[derive(Debug, Clone, Default)]
pub struct NegotiatedCaps {
    pub four_cc_list: FourCcList,
    pub caps_ex: CapsExit,
    pub video_four_cc_info_map: VideoFourCcInfoMap,
    pub reconnect: Reconnect,
    pub caps_ex_mask: u32,
    pub has_four_cc_list: bool,
    pub has_caps_ex: bool,
    pub has_video_four_cc_info_map: bool,
    pub has_reconnect: bool,
    pub multitrack_enabled: bool,
}

/// Connect command information.
#[derive(Debug, Clone)]
#[repr(C)]
pub struct ConnectInfo {
    pub name: [u8; 64],
    pub transaction_id: f64,
    pub app: [u8; 256],
    pub tc_url: [u8; 512],
    pub page_url: [u8; 512],
    pub swf_url: [u8; 512],
    pub flash_ver: [u8; 64],
    pub audio_codecs: i32,
    pub video_codecs: i32,
    pub four_cc_list: FourCcList,
    pub caps_ex: CapsExit,
    pub video_four_cc_info_map: VideoFourCcInfoMap,
    pub reconnect: Reconnect,
    pub caps_ex_mask: u32,
    pub has_four_cc_list: bool,
    pub has_caps_ex: bool,
    pub has_video_four_cc_info_map: bool,
    pub has_reconnect: bool,
}

impl Default for ConnectInfo {
    fn default() -> Self {
        Self {
            name: [0; 64],
            transaction_id: 0.0,
            app: [0; 256],
            tc_url: [0; 512],
            page_url: [0; 512],
            swf_url: [0; 512],
            flash_ver: [0; 64],
            audio_codecs: 0,
            video_codecs: 0,
            four_cc_list: FourCcList::default(),
            caps_ex: CapsExit::default(),
            video_four_cc_info_map: VideoFourCcInfoMap::default(),
            reconnect: Reconnect::default(),
            caps_ex_mask: 0,
            has_four_cc_list: false,
            has_caps_ex: false,
            has_video_four_cc_info_map: false,
            has_reconnect: false,
        }
    }
}
