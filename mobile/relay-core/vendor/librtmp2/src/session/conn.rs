use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::buffer::Buffer;
use crate::chunk::reader::{ChunkMessage, chunk_read_owned};
use crate::chunk::state::{
    ChunkRegistry, DEFAULT_CHUNK_SIZE, DEFAULT_MAX_MSG_LENGTH, RTMP_WIRE_MAX_MSG_LENGTH,
};
use crate::chunk::writer::chunk_write;
use crate::ertmp::connect_amf::{negotiate_caps, write_negotiated_caps};
use crate::ertmp::multitrack_media::{first_track_fourcc, foreach_track, is_multitrack_container};
use crate::handshake::{self, Handshake, HandshakeState};
use crate::media::{
    ERTMP_PACKET_TYPE_MODEX, is_on_metadata_payload, normalize_modex_payload,
    parse_video_metadata_hdr, populate_av_frame, populate_multitrack_frame,
};
use crate::message::command;
use crate::message::control::{
    self, UCTRL_PING_REQUEST, UCTRL_PING_RESPONSE, UCTRL_SET_BUFFER_LENGTH, UCTRL_STREAM_BEGIN,
    UCTRL_STREAM_EOF,
};
use crate::message::message as msg_dispatch;
use crate::message::shared_object::{self, SharedObjectMessage};
use crate::session::publish_route::PublishRouteRegistry;
use crate::session::state_machine;
use crate::session::stream::Stream;
use crate::transport::Transport;
use crate::types::*;

pub const MAX_STREAMS_PER_CONN: u32 = 16;
pub const MAX_PENDING_RELAY_FRAMES: usize = 1024;
pub const MAX_PENDING_RELAY_BYTES: usize = 8 * 1024 * 1024;
/// Cap complete messages handled per `read_messages` call so one TCP recv batch
/// cannot drain thousands of tiny control messages in a single state-machine step.
const MAX_MESSAGES_PER_READ: usize = 256;
/// Hard cap on complete messages handled across all `read_messages` passes in a
/// single `recv` call. Without this, the outer `recv` loop (256 iterations)
/// multiplies the per-pass cap into 65,536 message parses per recv batch.
const MAX_MESSAGES_PER_RECV: usize = 256;
/// Maximum bytes retained in the per-connection inbound staging buffer. When
/// the per-call message budget stops draining faster than the peer sends,
/// complete wire data can otherwise accumulate here up to `BUFFER_MAX_SIZE`
/// (64 MiB) per connection. Sized to comfortably hold one in-flight
/// `DEFAULT_MAX_MSG_LENGTH` message even when reassembled from many small
/// chunks (e.g. a peer that never raises chunk size above the 128-byte
/// default) plus normal per-poll staging, while staying well below the
/// original unbounded-growth risk this cap replaces.
const MAX_RECV_BUFFER_BYTES: usize = 2 * DEFAULT_MAX_MSG_LENGTH as usize;
/// Cap deferred init-cache eviction markers per connection. Without a bound,
/// a peer that repeatedly renames its publish route on a `Conn` driven
/// outside `Server::poll` can grow this vector without bound.
const MAX_PENDING_CACHE_EVICTIONS: usize = 64;

const SERVER_WINDOW_ACK_SIZE: u32 = 2_500_000;
const SERVER_PEER_BANDWIDTH: u32 = 2_500_000;
const PEER_BANDWIDTH_DYNAMIC: u8 = 2;
const PING_INTERVAL: Duration = Duration::from_secs(5);
const PING_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_PENDING_PINGS: usize = 4;

#[derive(Debug, Clone)]
struct QueuedPing {
    token: u32,
    queued_at: Instant,
    /// Bytes that must drain from the front of `send_buffer` before this ping
    /// is considered fully on the wire (bytes queued ahead of the ping at
    /// queue time plus the ping itself, but not data appended afterward).
    bytes_until_flushed: usize,
}
/// Cap inbound Ping-Request reflections per connection to prevent trivial
/// outbound bandwidth/CPU amplification from unauthenticated peers.
const MAX_INBOUND_PING_RESPONSES: usize = 8;
/// Minimum wall-clock gap between init-frame replay requests caused by
/// switching play routes. This prevents a client from alternating stream
/// names to force cached headers and keyframes to be resent every poll batch.
const INIT_REPLAY_COOLDOWN: Duration = Duration::from_secs(1);
/// Close publishers that claimed a route but never sent media. Shorter than
/// [`RTMP_SESSION_SETUP_TIMEOUT`] so squatters cannot block legitimate
/// publishers for the full post-connect grace window.
const PUBLISH_MEDIA_REQUIRED_TIMEOUT: Duration = Duration::from_secs(2);
const INBOUND_PING_WINDOW: Duration = Duration::from_secs(1);
/// Close inbound sessions that never complete the AMF `connect` exchange or
/// never start publishing/playing. Matches the RTMPS accept deadline so a
/// peer cannot hold a connection slot indefinitely with a partial legacy
/// handshake, an idle post-handshake TCP session, or ping responses alone
/// after `connect`.
pub(crate) const RTMP_SESSION_SETUP_TIMEOUT: Duration = Duration::from_secs(10);
/// Cap sub-tags unpacked from a single Aggregate message (mirrors
/// `message::message::MAX_AGGREGATE_SUBTAGS`).
const MAX_AGGREGATE_SUBTAGS: usize = 4096;

/// One media/script frame queued for local relay, stream-cache update, and
/// optional export to integrators.
///
/// Constructed by publishers (via the session media path), by
/// [`Conn::inject_relay_frame`], or by [`crate::server::Server::inject_relay_frame`].
/// Integrators that drain export buffers receive clones of these frames.
#[derive(Clone)]
pub struct RelayFrame {
    /// Audio, video, script (`onMetaData`), or metadata classification.
    pub frame_type: FrameType,
    /// RTMP message timestamp (milliseconds).
    pub timestamp: u32,
    /// Wire payload as received or injected (relayed to players unchanged).
    pub payload: Vec<u8>,
    /// ModEx-normalized bytes used only for codec parsing and cache classification.
    /// `None` means the normalized bytes are identical to `payload`.
    pub cache_payload: Option<Vec<u8>>,
    /// Application name used as the first half of the relay route key.
    pub app: String,
    /// Stream / relay-route key used to match publishers to players.
    pub stream_name: String,
    /// Owning publisher connection id. Socket-less injects use a high-bit
    /// external id ([`crate::server::is_external_publisher_id`]); the sentinel
    /// [`crate::server::EXTERNAL_RELAY_PUBLISHER_ID`] remains `u64::MAX`.
    pub publisher_conn_id: u64,
}

impl RelayFrame {
    /// Bytes used for codec parsing and cache classification.
    pub fn cache_payload(&self) -> &[u8] {
        self.cache_payload.as_deref().unwrap_or(&self.payload)
    }

    pub(crate) fn retained_bytes(&self) -> usize {
        self.payload
            .len()
            .saturating_add(
                self.cache_payload
                    .as_ref()
                    .map(|payload| payload.len())
                    .unwrap_or(0),
            )
            .saturating_add(self.app.len())
            .saturating_add(self.stream_name.len())
    }
}
pub struct Conn {
    pub state: ConnState,
    pub handshake: Handshake,
    pub recv_buffer: Buffer,
    pub send_buffer: Buffer,
    pub chunk_reg: ChunkRegistry,
    /// Target chunk size announced to the peer (from server config).
    pub chunk_size: u32,
    /// Active outbound chunk size; stays at the RTMP default until SetChunkSize
    /// is negotiated on the wire.
    active_chunk_size: u32,
    pub window_ack_size: u32,
    pub bytes_received: u64,
    pub bytes_at_last_ack: u64,
    /// Audio/video payload bytes received (excludes handshake/control overhead).
    pub media_bytes_received: u64,
    /// Bytes accepted via [`Conn::inject_relay_frame`] (AV + script/metadata;
    /// socket telemetry stays in `media_bytes_received`). Counts trusted
    /// injects toward publisher liveness and deferred-relay drain so route
    /// squatters still time out and metadata is not stuck while relay is deferred.
    pub injected_media_bytes: u64,
    /// Audio/video payload bytes sent to this peer.
    pub media_bytes_sent: u64,
    /// Snapshot of `media_bytes_sent` consumed by the last pause-grace reset.
    /// A later pause may refresh setup grace only after additional relay bytes
    /// have been sent, so historical activity cannot keep a slot alive forever.
    pause_grace_media_bytes_sent: u64,
    pub client_fd: i32,
    /// Stable per-connection id (monotonic, never reused while the server runs).
    pub conn_id: u64,
    /// Peer socket address for logging (not persisted).
    pub remote_addr: String,
    pub transport: Option<Transport>,
    pub app: String,
    /// Canonical relay route key. When set, publisher/player media is matched
    /// on this value instead of the RTMP stream name (e.g. separate publish/play keys).
    pub relay_key: String,
    pub next_stream_id: u32,
    pub current_stream: Option<Box<Stream>>,
    pub connect_cb_fired: bool,
    pub send_mutex: Mutex<()>,
    pub pending_relay: Vec<RelayFrame>,
    pub needs_init_frames: bool,
    /// Last play-route change that requested cached init frames.
    last_init_replay_request: Option<Instant>,
    pub detected_video_codec: Option<String>,
    pub detected_audio_codec: Option<String>,
    pub detected_video_width: Option<u32>,
    pub detected_video_height: Option<u32>,
    pub detected_video_framerate: Option<f64>,
    pub detected_audio_sample_rate: Option<u32>,
    pub detected_audio_channels: Option<u32>,
    /// HDR `colorInfo` parsed from the publisher's most recent enhanced video
    /// Metadata packet (`ERTMP_PACKET_TYPE_METADATA`), if any. Rust-only --
    /// not part of `Frame`'s ABI-stable `#[repr(C)]` layout (see
    /// `docs/abi-policy.md`).
    pub detected_hdr_info: Option<HdrInfo>,
    pub relay_enabled: bool,
    /// When true, media relay stays off until the integrator sets `relay_enabled`
    /// after its own post-auth bookkeeping (used by librtmp2-server).
    pub defer_media_relay: bool,
    /// Cap on queued relay payload bytes for this connection.
    pub max_pending_relay_bytes: usize,
    pub on_frame_cb: Option<fn(&Frame)>,
    /// When set, must return true before audio/video is queued for relay.
    /// For a multitrack (`ManyTracks`/`ManyTracksManyCodecs`) container, this
    /// is called once per track with that track's codec, not once per frame —
    /// any single denial rejects the whole frame.
    pub on_media_cb: Option<fn(u64, FrameType, Option<&str>) -> bool>,
    pub on_connect_cb: Option<fn()>,
    pub on_publish_cb: Option<fn(conn_id: u64, app: &str, stream_name: &str) -> bool>,
    pub on_play_cb: Option<fn(conn_id: u64, app: &str, stream_name: &str) -> bool>,
    /// Must return true before a parsed AMF3 Shared Object message is delivered
    /// to [`Self::on_shared_object_cb`]. When unset while `on_publish_cb` or
    /// `on_play_cb` is configured, inbound shared objects are dropped so a
    /// peer authorized only for publish/play cannot inject shared-object events.
    pub on_shared_object_auth_cb: Option<fn(conn_id: u64, so: &SharedObjectMessage) -> bool>,
    /// Fired for every parsed AMF3 Shared Object message (RTMP message type
    /// `0x10`) received on this connection. The library only delivers the
    /// parsed envelope -- multi-client attribute sync/persistence is left to
    /// the host application, consistent with this crate's "deliver the
    /// event, not the policy" design.
    pub on_shared_object_cb: Option<fn(conn_id: u64, so: &SharedObjectMessage)>,
    /// Must return true to allow `releaseStream` to force-evict *another*
    /// connection's publish-route claim for `stream_name` (e.g. reclaiming a
    /// route after a reconnecting encoder's old TCP session went stale but
    /// hasn't timed out yet). Unset (the default) makes `releaseStream` a
    /// no-op: unlike `on_media_cb`/`on_publish_cb`, this has no permissive
    /// default, since evicting a still-live publisher would otherwise let
    /// any authenticated peer hijack another connection's stream by name.
    pub on_release_stream_cb: Option<fn(conn_id: u64, app: &str, stream_name: &str) -> bool>,
    /// When set by the built-in server, enforces single-publisher-per-route.
    pub(crate) publish_routes: Option<PublishRouteRegistry>,
    /// Exact stream-name key currently held in `publish_routes` for this conn.
    /// Must be released on teardown even when `relay_key` later diverges from
    /// the RTMP publish name (librtmp2-server pins `relay_key` to the DB id).
    claimed_publish_route: Option<String>,
    /// Cache keys to evict after the publisher renames its stream.
    pub pending_cache_evictions: Vec<(String, String)>,
    /// Last measured client↔server RTT in milliseconds (RTMP UserControl ping).
    pub rtt_ms: f64,
    pending_pings: HashMap<u32, Instant>,
    /// Ping queued in `send_buffer` but not yet fully flushed.
    queued_ping: Option<QueuedPing>,
    last_ping_sent: Option<Instant>,
    next_ping_token: u32,
    inbound_ping_responses: usize,
    inbound_ping_window_start: Option<Instant>,
    /// Retains the last frame payload delivered through `on_frame_cb` so
    /// `Frame.data` stays valid until the next callback on this connection.
    frame_cb_scratch: Vec<u8>,
    /// Set when `read_messages` stops because `MAX_MESSAGES_PER_READ` or
    /// `MAX_MESSAGES_PER_RECV` was hit while `recv_buffer` still holds
    /// complete, unprocessed messages -- as opposed to stopping because the
    /// buffer ran out of data. Callers must keep draining (e.g. via another
    /// `recv(&[])`) until this clears, or a batch that exceeds the budget can
    /// sit unprocessed until the peer happens to send more bytes.
    budget_exhausted: bool,
    /// Start of the current "must become active" grace window: set when
    /// this inbound TCP session was accepted, and reset every time the
    /// session goes idle after publishing/playing (`FCUnpublish`,
    /// `deleteStream`, `closeStream`) so a legitimate client that stops and
    /// later republishes on the same connection gets a fresh window instead
    /// of being reaped for the unpublished gap. Used by
    /// [`Conn::session_setup_timed_out`].
    session_setup_started: Instant,
    /// E-RTMP caps agreed during connect (empty when legacy connect).
    pub negotiated_caps: NegotiatedCaps,
    /// Player-side buffer length from SetBufferLength user control (ms).
    pub buffer_length_ms: u32,
}

impl Conn {
    pub fn new() -> Self {
        let mut chunk_reg = ChunkRegistry::new();
        chunk_reg.init();
        Self {
            state: ConnState::TcpAccepted,
            handshake: Handshake::default(),
            recv_buffer: Buffer::new(),
            send_buffer: Buffer::new(),
            chunk_reg,
            chunk_size: DEFAULT_CHUNK_SIZE,
            active_chunk_size: DEFAULT_CHUNK_SIZE,
            window_ack_size: 0,
            bytes_received: 0,
            bytes_at_last_ack: 0,
            media_bytes_received: 0,
            injected_media_bytes: 0,
            media_bytes_sent: 0,
            pause_grace_media_bytes_sent: 0,
            client_fd: -1,
            conn_id: 0,
            remote_addr: String::new(),
            transport: None,
            app: String::new(),
            relay_key: String::new(),
            next_stream_id: 0,
            current_stream: None,
            connect_cb_fired: false,
            send_mutex: Mutex::new(()),
            pending_relay: Vec::new(),
            needs_init_frames: false,
            last_init_replay_request: None,
            detected_video_codec: None,
            detected_audio_codec: None,
            detected_video_width: None,
            detected_video_height: None,
            detected_video_framerate: None,
            detected_audio_sample_rate: None,
            detected_audio_channels: None,
            detected_hdr_info: None,
            relay_enabled: false,
            defer_media_relay: false,
            max_pending_relay_bytes: MAX_PENDING_RELAY_BYTES,
            on_frame_cb: None,
            on_media_cb: None,
            on_connect_cb: None,
            on_publish_cb: None,
            on_play_cb: None,
            on_shared_object_auth_cb: None,
            on_shared_object_cb: None,
            on_release_stream_cb: None,
            publish_routes: None,
            claimed_publish_route: None,
            pending_cache_evictions: Vec::new(),
            rtt_ms: 0.0,
            pending_pings: HashMap::new(),
            queued_ping: None,
            last_ping_sent: None,
            next_ping_token: 1,
            inbound_ping_responses: 0,
            inbound_ping_window_start: None,
            frame_cb_scratch: Vec::new(),
            budget_exhausted: false,
            session_setup_started: Instant::now(),
            negotiated_caps: NegotiatedCaps::default(),
            buffer_length_ms: 3000,
        }
    }

    /// True when an inbound peer has held a connection slot without being an
    /// active publisher or player for longer than
    /// [`RTMP_SESSION_SETUP_TIMEOUT`]. Covers incomplete handshakes, post-
    /// `connect` idle sessions that only answer server pings, and sessions
    /// that stopped publishing/playing and haven't resumed within a fresh
    /// grace window (`session_setup_started` is reset on every such
    /// transition -- see `FCUnpublish`/`deleteStream`/`closeStream` in
    /// `handle_command`).
    pub fn session_setup_timed_out(&self) -> bool {
        // `ConnState` only moves forward, so a peer that briefly published
        // (or played) and then unpublished before the deadline would stay
        // "exempt" forever if this checked `self.state`. Check the current
        // stream's active flags instead -- they're cleared on unpublish.
        let Some(stream) = self.current_stream.as_ref() else {
            // Inject-held claims can exist without `current_stream` (socket-less
            // inject path). Sustain while media has flowed; otherwise apply the
            // publish-media squat timer instead of the longer setup timeout.
            if self.claimed_publish_route.is_some() {
                if self.injected_media_bytes > 0 || self.media_bytes_received > 0 {
                    return false;
                }
                return self.session_setup_started.elapsed() >= PUBLISH_MEDIA_REQUIRED_TIMEOUT;
            }
            return self.session_setup_started.elapsed() >= RTMP_SESSION_SETUP_TIMEOUT;
        };
        // Publish role *or* an inject-held route claim must obey the media
        // squat timer. Otherwise an idle/inject Conn (or a player that somehow
        // held a claim) blocks socket publishers until TCP disconnect.
        let holding_inject_claim = self.claimed_publish_route.is_some();
        if stream.is_publishing || holding_inject_claim {
            // A peer that claims a publish route but never sends media can
            // otherwise squat the route indefinitely by keeping TCP alive.
            // Trusted injects count toward liveness without polluting socket
            // receive telemetry (`media_bytes_received`).
            if self.media_bytes_received == 0 && self.injected_media_bytes == 0 {
                return self.session_setup_started.elapsed() >= PUBLISH_MEDIA_REQUIRED_TIMEOUT;
            }
            return false;
        }
        // Inject-only squatters (no publish role) must not hold a route
        // longer than a real publisher would without sending media.
        if self.claimed_publish_route.is_some()
            && self.media_bytes_received == 0
            && self.injected_media_bytes == 0
        {
            return self.session_setup_started.elapsed() >= PUBLISH_MEDIA_REQUIRED_TIMEOUT;
        }
        if self.session_setup_started.elapsed() < RTMP_SESSION_SETUP_TIMEOUT {
            return false;
        }
        if stream.is_playing {
            // Paused players opt out of relay delivery, so they never hit the
            // slow-reader disconnect path. Unpaused viewers who have received
            // outbound relay are protected; squatters who issued play against a
            // dead route with zero relay must not hold slots forever.
            if !stream.paused && self.media_bytes_sent > 0 {
                return false;
            }
            return true;
        }
        true
    }

    #[cfg(test)]
    pub(crate) fn set_session_setup_started_for_test(&mut self, started: Instant) {
        self.session_setup_started = started;
    }

    #[cfg(test)]
    fn handle_message_for_test(&mut self, msg: &ChunkMessage, payload: &[u8]) -> Result<()> {
        let mut messages_budget = usize::MAX;
        self.handle_message(msg, payload, &mut messages_budget)
    }

    #[cfg(test)]
    fn handle_aggregate_for_test(
        &mut self,
        msg_stream_id: u32,
        base_timestamp: u32,
        payload: &[u8],
    ) -> Result<()> {
        let mut messages_budget = usize::MAX;
        self.handle_aggregate(msg_stream_id, base_timestamp, payload, &mut messages_budget)
    }

    /// True if the last `recv`/`process` pass stopped early because the
    /// per-call message budget was exhausted while complete messages were
    /// still buffered. Callers (e.g. the server poll loop) should keep
    /// calling `recv(&[])` until this returns false so a batch larger than
    /// the budget doesn't stall waiting on the peer to send more bytes.
    pub fn has_buffered_messages(&self) -> bool {
        self.budget_exhausted
    }

    fn pending_relay_bytes(&self) -> usize {
        self.pending_relay
            .iter()
            .map(RelayFrame::retained_bytes)
            .sum()
    }

    /// Key used to route relayed media between publishers and players.
    pub fn relay_route_key(&self) -> String {
        if !self.relay_key.is_empty() {
            return self.relay_key.clone();
        }
        self.current_stream
            .as_ref()
            .map(|s| s.name.clone())
            .unwrap_or_default()
    }

    pub fn accepts_multitrack(&self) -> bool {
        !self.negotiated_caps.has_caps_ex || self.negotiated_caps.multitrack_enabled
    }

    /// Queue eviction of the current publish route's cache key when
    /// `current_stream` is about to be repurposed or replaced while still
    /// marked as actively publishing (a fresh `createStream`, or switching
    /// to `play`, both abandon whatever was being published). Unlike a
    /// `publish` rename there is no new route to keep serving, so this
    /// always evicts when the connection was publishing. No-op otherwise.
    fn evict_active_publish_route(&mut self) {
        let was_publishing = self
            .current_stream
            .as_ref()
            .map(|s| s.is_publishing)
            .unwrap_or(false);
        if !was_publishing {
            // Idle/prior-epoch inject must not survive createStream / play /
            // FCUnpublish into a later empty publish's media deadline. A
            // connection-level inject can claim a route without ever
            // transitioning to Publishing, so release that claim here too or
            // the route stays unavailable to socket publishers until close.
            // Also evict any cache this idle inject wrote — a later publisher
            // on the same route must not inherit stale codec headers.
            let claimed_route = self.claimed_publish_route.clone();
            self.release_claimed_publish_route();
            if let Some(route_key) = claimed_route
                && !route_key.is_empty()
            {
                self.try_push_pending_cache_eviction(self.app.clone(), route_key);
            }
            self.injected_media_bytes = 0;
            return;
        }
        let claimed_route = self.claimed_publish_route.clone();
        self.release_claimed_publish_route();
        // Cached frames are keyed by whatever relay_route_key() was live at
        // the moment each frame was queued (see RelayFrame::stream_name), so
        // if an integrator pins relay_key after the route was claimed, cache
        // entries can exist under both the originally claimed key and the
        // current one. Evict both to avoid leaving either behind.
        let live_route_key = self.relay_route_key();
        let mut evicted = std::collections::HashSet::new();
        for route_key in [claimed_route, Some(live_route_key)].into_iter().flatten() {
            if !route_key.is_empty() && evicted.insert(route_key.clone()) {
                self.try_push_pending_cache_eviction(self.app.clone(), route_key);
            }
        }
        self.clear_detected_stream_metadata();
        self.injected_media_bytes = 0;
    }

    fn release_claimed_publish_route(&mut self) {
        if let Some(claimed) = self.claimed_publish_route.take() {
            if let Some(routes) = self.publish_routes.as_ref() {
                routes.release(self.conn_id, &self.app, &claimed);
            }
        }
    }

    fn try_push_pending_cache_eviction(&mut self, app: String, route_key: String) {
        if self.pending_cache_evictions.len() < MAX_PENDING_CACHE_EVICTIONS {
            self.pending_cache_evictions.push((app, route_key));
        }
    }

    fn push_pending_cache_eviction(&mut self, app: String, route_key: String) -> Result<()> {
        if self.pending_cache_evictions.len() >= MAX_PENDING_CACHE_EVICTIONS {
            return Err(ErrorCode::Protocol);
        }
        self.pending_cache_evictions.push((app, route_key));
        Ok(())
    }

    fn claim_publish_route(&mut self, stream: &str) -> bool {
        let claimed = match self.publish_routes.as_ref() {
            Some(routes) => routes.claim(self.conn_id, &self.app, stream),
            None => {
                self.claimed_publish_route = Some(stream.to_string());
                return true;
            }
        };
        if !claimed {
            return false;
        }
        match self.claimed_publish_route.take() {
            Some(prev) if prev == stream => self.claimed_publish_route = Some(prev),
            Some(prev) => {
                if let Some(routes) = self.publish_routes.as_ref() {
                    routes.release(self.conn_id, &self.app, &prev);
                }
                // Mirror the publish-rename path: free the old route's init
                // cache so a later publisher on `prev` cannot inherit stale
                // codec headers from this connection.
                if !prev.is_empty()
                    && self
                        .push_pending_cache_eviction(self.app.clone(), prev.clone())
                        .is_err()
                {
                    if let Some(routes) = self.publish_routes.as_ref() {
                        routes.release(self.conn_id, &self.app, stream);
                        let _ = routes.claim(self.conn_id, &self.app, &prev);
                    }
                    self.claimed_publish_route = Some(prev);
                    return false;
                }
                self.claimed_publish_route = Some(stream.to_string());
            }
            None => self.claimed_publish_route = Some(stream.to_string()),
        }
        true
    }

    fn clear_detected_stream_metadata(&mut self) {
        self.detected_video_codec = None;
        self.detected_audio_codec = None;
        self.detected_video_width = None;
        self.detected_video_height = None;
        self.detected_video_framerate = None;
        self.detected_audio_sample_rate = None;
        self.detected_audio_channels = None;
        self.detected_hdr_info = None;
    }

    fn publishing_metadata_allowed(&self, msg_stream_id: u32) -> bool {
        let expected_stream_id = self
            .current_stream
            .as_ref()
            .filter(|s| s.is_publishing)
            .map(|s| s.stream_id)
            .unwrap_or(0);
        msg_stream_id == expected_stream_id && expected_stream_id != 0
    }

    fn handle_publisher_data_message(
        &mut self,
        msg_stream_id: u32,
        timestamp: u32,
        payload: &[u8],
    ) -> Result<()> {
        if !self.publishing_metadata_allowed(msg_stream_id) {
            return Ok(());
        }
        let relay_metadata = is_on_metadata_payload(payload)
            && self.relay_enabled
            && self
                .current_stream
                .as_ref()
                .map(|s| s.is_publishing)
                .unwrap_or(false);
        if relay_metadata && !self.media_allowed(FrameType::Script, None) {
            return Err(ErrorCode::Auth);
        }
        self.handle_data_message(payload)?;
        if relay_metadata {
            if let Some(cb) = self.on_frame_cb {
                self.frame_cb_scratch.clear();
                self.frame_cb_scratch.extend_from_slice(payload);
                let frame = Frame {
                    frame_type: FrameType::Script,
                    timestamp,
                    size: self.frame_cb_scratch.len() as u32,
                    data: self.frame_cb_scratch.as_ptr(),
                    is_metadata: 1,
                    ..Default::default()
                };
                cb(&frame);
            }
            self.queue_relay_frame(FrameType::Script, timestamp, payload, payload)?;
        }
        Ok(())
    }

    /// Inject media into this connection's pending relay queue as if it were
    /// published locally.
    ///
    /// Uses the same ModEx-normalize + `queue_relay_frame` path as inbound
    /// publisher media, but skips `on_media_cb` / `on_frame_cb` (integrator-
    /// trusted). Does not require `relay_enabled`. Rejects playing-only
    /// connections so inject cannot squat publish routes from a player session.
    pub fn inject_relay_frame(
        &mut self,
        frame_type: FrameType,
        timestamp: u32,
        payload: &[u8],
    ) -> Result<()> {
        let max_len = self.chunk_reg.max_msg_length.min(RTMP_WIRE_MAX_MSG_LENGTH) as usize;
        if payload.len() > max_len {
            return Err(ErrorCode::Internal);
        }
        if let Some(stream) = self.current_stream.as_ref() {
            if stream.is_playing && !stream.is_publishing {
                return Err(ErrorCode::Internal);
            }
        }
        let stream_name = self.relay_route_key();
        if self.app.is_empty() || stream_name.is_empty() {
            return Err(ErrorCode::Internal);
        }
        if self.app.len() > 1024 || stream_name.len() > 1024 {
            return Err(ErrorCode::Internal);
        }
        let previous_claim = self.claimed_publish_route.clone();
        let eviction_len_before = self.pending_cache_evictions.len();
        let already_owned_route =
            self.claimed_publish_route.as_deref() == Some(stream_name.as_str());
        if !self.claim_publish_route(&stream_name) {
            // `relay_route_key()` prefers `relay_key`. If an integrator pointed
            // it at a contended route, restore it to the still-held claim so
            // later socket media cannot snapshot an unclaimed key.
            if let Some(ref claimed) = self.claimed_publish_route {
                self.relay_key = claimed.clone();
            }
            return Err(ErrorCode::Internal);
        }
        let normalized = normalize_modex_payload(payload, self.negotiated_caps.caps_ex_mask);
        if let Err(e) = self.queue_relay_frame(frame_type, timestamp, payload, normalized.as_ref())
        {
            // Don't let a rejected frame (e.g. over the pending-byte budget)
            // leave a route claimed that this connection never actually
            // delivered anything on — that would block a real publisher.
            // Only roll back a claim/switch we just took; a claim already held
            // from an earlier successful inject on the same route stays intact.
            // On a failed route switch, restore the previous claim and undo the
            // cache-eviction marker so the active feed is not abandoned.
            if !already_owned_route {
                self.release_claimed_publish_route();
                self.pending_cache_evictions.truncate(eviction_len_before);
                if let Some(prev) = previous_claim {
                    if let Some(routes) = self.publish_routes.as_ref() {
                        let _ = routes.claim(self.conn_id, &self.app, &prev);
                    }
                    self.claimed_publish_route = Some(prev.clone());
                    // Keep relay_route_key() aligned with the restored claim so
                    // socket media cannot queue under the rejected key B.
                    self.relay_key = prev;
                }
            }
            return Err(e);
        }
        // Count all successful injects (AV + script/metadata) as publisher
        // activity for squat timeout and deferred-relay drain/export. Script
        // and metadata never touch socket receive telemetry, but they must
        // still wake `Server::process_connections` when `defer_media_relay`
        // holds `relay_enabled` off.
        self.injected_media_bytes = self
            .injected_media_bytes
            .saturating_add((payload.len() as u64).max(1));
        Ok(())
    }

    fn queue_relay_frame(
        &mut self,
        frame_type: FrameType,
        timestamp: u32,
        payload: &[u8],
        cache_payload: &[u8],
    ) -> Result<()> {
        let max_len = self.chunk_reg.max_msg_length.min(RTMP_WIRE_MAX_MSG_LENGTH) as usize;
        if payload.len() > max_len {
            return Err(ErrorCode::Internal);
        }
        let cache_payload = if cache_payload.len() == payload.len()
            && std::ptr::eq(cache_payload.as_ptr(), payload.as_ptr())
        {
            None
        } else {
            Some(cache_payload.to_vec())
        };
        let app = self.app.clone();
        let stream_name = self.relay_route_key();
        let retained_bytes = payload
            .len()
            .saturating_add(
                cache_payload
                    .as_ref()
                    .map(|payload| payload.len())
                    .unwrap_or(0),
            )
            .saturating_add(app.len())
            .saturating_add(stream_name.len());
        if self.pending_relay.len() >= MAX_PENDING_RELAY_FRAMES
            || self.pending_relay_bytes().saturating_add(retained_bytes)
                > self.max_pending_relay_bytes
        {
            return Err(ErrorCode::Internal);
        }
        self.pending_relay.push(RelayFrame {
            frame_type,
            timestamp,
            payload: payload.to_vec(),
            cache_payload,
            app,
            stream_name,
            publisher_conn_id: self.conn_id,
        });
        Ok(())
    }

    fn media_allowed(&self, frame_type: FrameType, codec: Option<&str>) -> bool {
        let Some(cb) = self.on_media_cb else {
            return true;
        };
        // Unknown codec identity must not bypass deny-list policies by appearing
        // as `None` when the frame is otherwise valid publisher media.
        if matches!(frame_type, FrameType::Video | FrameType::Audio) && codec.is_none() {
            return false;
        }
        cb(self.conn_id, frame_type, codec)
    }

    /// Request a one-shot init-frame replay for a playing client,
    /// rate-limited so rapid play-route changes cannot amplify cached data.
    fn request_init_replay(&mut self) {
        let now = Instant::now();
        if self
            .last_init_replay_request
            .is_some_and(|last| now.duration_since(last) < INIT_REPLAY_COOLDOWN)
        {
            return;
        }
        self.last_init_replay_request = Some(now);
        self.needs_init_frames = true;
    }

    fn is_active_publisher_stream(&self, msg_stream_id: u32) -> bool {
        self.relay_enabled
            && self
                .current_stream
                .as_ref()
                .is_some_and(|stream| stream.is_publishing && stream.stream_id == msg_stream_id)
    }

    /// True when an active->idle transition carried meaningful media flow and
    /// should grant a fresh setup-timeout window (mirrors pause grace).
    fn teardown_refreshes_setup_timer(&self) -> bool {
        self.media_bytes_sent > 0 || self.media_bytes_received > 0 || self.injected_media_bytes > 0
    }

    fn authorize_media_frame(
        &self,
        frame_type: FrameType,
        parse_payload: &[u8],
        current_codec: Option<&str>,
        is_multitrack: bool,
        mut messages_budget: Option<&mut usize>,
    ) -> Result<()> {
        // Without an authorization callback there is no per-track auth work
        // to budget here. The frame-callback path applies its own sub-track
        // budget independently below.
        if self.on_media_cb.is_none() {
            return Ok(());
        }

        if !is_multitrack {
            return if self.media_allowed(frame_type, current_codec) {
                Ok(())
            } else {
                Err(ErrorCode::Auth)
            };
        }

        // A `ManyTracksManyCodecs` container can carry a different codec per
        // track; checking only the first codec would allow a later disallowed
        // codec to bypass authorization.
        let mut auth_denied = false;
        let mut auth_budget_exhausted = false;
        let mut track_index = 0usize;
        let tracks_valid = foreach_track(frame_type, parse_payload, |track| {
            if auth_denied || auth_budget_exhausted {
                return;
            }
            if track_index > 0 {
                if let Some(budget) = messages_budget.as_deref_mut() {
                    if *budget == 0 {
                        auth_budget_exhausted = true;
                        return;
                    }
                    *budget = budget.saturating_sub(1);
                }
            }
            track_index += 1;
            let track_codec = media_fourcc_auth_label(&track.fourcc);
            if !self.media_allowed(frame_type, track_codec.as_deref()) {
                auth_denied = true;
            }
        });
        if auth_budget_exhausted {
            return Err(ErrorCode::Protocol);
        }
        if !tracks_valid {
            return Err(ErrorCode::Protocol);
        }
        if auth_denied {
            return Err(ErrorCode::Auth);
        }
        Ok(())
    }

    fn update_detected_media_metadata(
        &mut self,
        frame_type: FrameType,
        current_codec: Option<&str>,
        is_multitrack: bool,
        parse_payload: &[u8],
    ) {
        // Only pin the detected codec once the frame has cleared
        // authorization, so a rejected frame's codec never lingers for
        // telemetry readers.
        match frame_type {
            FrameType::Video if self.detected_video_codec.is_none() => {
                self.detected_video_codec = current_codec.map(str::to_owned);
            }
            FrameType::Audio if self.detected_audio_codec.is_none() => {
                self.detected_audio_codec = current_codec.map(str::to_owned);
            }
            _ => {}
        }

        if frame_type == FrameType::Video && !is_multitrack {
            if let Some(color_info) = parse_video_metadata_hdr(parse_payload) {
                self.detected_hdr_info = Some(color_info);
            }
        }
    }

    fn dispatch_media_frame_callbacks(
        &mut self,
        frame_type: FrameType,
        timestamp: u32,
        parse_payload: &[u8],
        is_multitrack: bool,
        mut messages_budget: Option<&mut usize>,
    ) -> Result<()> {
        let cb = self.on_frame_cb;
        let mut track_index = 0usize;
        let mut track_budget_exhausted = false;
        let parsed_multitrack = foreach_track(frame_type, parse_payload, |track| {
            if track_budget_exhausted {
                return;
            }
            if track_index > 0 {
                if let Some(budget) = messages_budget.as_deref_mut() {
                    if *budget == 0 {
                        track_budget_exhausted = true;
                        return;
                    }
                    *budget = budget.saturating_sub(1);
                }
            }
            track_index += 1;
            if let Some(cb) = cb {
                self.invoke_multitrack_on_frame_cb(
                    cb,
                    frame_type,
                    timestamp,
                    track.track_id,
                    track.fourcc,
                    track.packet_type,
                    track.video_frame_type,
                    track.payload,
                );
            }
        });

        if is_multitrack {
            if !parsed_multitrack {
                return Err(ErrorCode::Protocol);
            }
            return Ok(());
        }

        if let Some(cb) = cb {
            self.invoke_on_frame_cb(cb, frame_type, timestamp, u8::MAX, parse_payload);
        }
        Ok(())
    }

    fn handle_media_frame(
        &mut self,
        msg_stream_id: u32,
        frame_type: FrameType,
        timestamp: u32,
        payload: &[u8],
        mut messages_budget: Option<&mut usize>,
    ) -> Result<()> {
        if !self.is_active_publisher_stream(msg_stream_id) {
            return Ok(());
        }

        // Always peel ModEx wrappers for codec authorization and callbacks.
        // Gating on negotiated `caps_ex_mask` let publishers craft ModEx
        // frames whose extension bytes spell an allowed FourCC while relaying
        // a different inner codec to players.
        let normalized_payload = normalize_modex_payload(payload, CAPS_EX_MASK_MODEX);
        let parse_payload = normalized_payload.as_ref();
        let current_codec = match frame_type {
            FrameType::Video => detect_video_codec(parse_payload),
            FrameType::Audio => detect_audio_codec(parse_payload),
            _ => None,
        };
        let is_multitrack = is_multitrack_container(frame_type, parse_payload);

        self.authorize_media_frame(
            frame_type,
            parse_payload,
            current_codec.as_deref(),
            is_multitrack,
            messages_budget.as_deref_mut(),
        )?;
        self.update_detected_media_metadata(
            frame_type,
            current_codec.as_deref(),
            is_multitrack,
            parse_payload,
        );

        self.media_bytes_received = self
            .media_bytes_received
            .saturating_add(payload.len() as u64);
        self.dispatch_media_frame_callbacks(
            frame_type,
            timestamp,
            parse_payload,
            is_multitrack,
            messages_budget,
        )?;

        if self
            .queue_relay_frame(frame_type, timestamp, payload, parse_payload)
            .is_err()
        {
            return Err(ErrorCode::Internal);
        }
        Ok(())
    }

    /// Unpack an RTMP Aggregate message (type 0x16) into its constituent FLV
    /// sub-tags and route each audio/video sub-tag through the same relay path
    /// as a standalone `RTMP_MSG_AUDIO`/`RTMP_MSG_VIDEO` message.
    ///
    /// Each sub-tag is `tag_type(1) + data_size(3) + timestamp(3) + timestamp_ext(1)
    /// + stream_id(3) + data(data_size) + prev_tag_size(4)`. Sub-tag timestamps are
    /// relative to the first sub-tag's timestamp; that delta is added to the
    /// aggregate message's own chunk timestamp per the FLV/RTMP aggregate spec.
    fn handle_aggregate(
        &mut self,
        msg_stream_id: u32,
        base_timestamp: u32,
        payload: &[u8],
        messages_budget: &mut usize,
    ) -> Result<()> {
        let mut pos = 0;
        let mut have_base = false;
        let mut sub_base_ts: u32 = 0;
        let mut subtags = 0usize;

        while pos + 11 <= payload.len() {
            if subtags >= MAX_AGGREGATE_SUBTAGS {
                return Err(ErrorCode::Protocol);
            }
            subtags += 1;

            let tag_type = payload[pos];
            let data_size = ((payload[pos + 1] as u32) << 16)
                | ((payload[pos + 2] as u32) << 8)
                | (payload[pos + 3] as u32);
            let ts = ((payload[pos + 4] as u32) << 16)
                | ((payload[pos + 5] as u32) << 8)
                | (payload[pos + 6] as u32)
                | ((payload[pos + 7] as u32) << 24);

            let body = pos + 11;
            let data_size = data_size as usize;
            if body + data_size > payload.len() {
                return Err(ErrorCode::Protocol);
            }
            // Mirror multitrack: zero-length sub-tags pack the maximum sub-tag
            // count into the smallest wire footprint and bypass per-sub-tag
            // processing budgets when their type is not audio/video/script.
            if data_size == 0 {
                return Err(ErrorCode::Protocol);
            }
            if *messages_budget == 0 {
                return Ok(());
            }
            *messages_budget = messages_budget.saturating_sub(1);

            if !have_base {
                sub_base_ts = ts;
                have_base = true;
            }
            // Use wrapping arithmetic: a sub-tag ts less than the first sub-tag's
            // ts (malformed but possible) must not panic in debug or silently
            // wrap in an unexpected direction in release.
            let out_ts = base_timestamp.wrapping_add(ts.wrapping_sub(sub_base_ts));
            let tag_payload = &payload[body..body + data_size];

            match tag_type {
                msg_dispatch::RTMP_MSG_AUDIO
                | msg_dispatch::RTMP_MSG_VIDEO
                | msg_dispatch::RTMP_MSG_AMF0_DATA => {}
                _ => {
                    pos = body + data_size + 4;
                    continue;
                }
            }

            match tag_type {
                msg_dispatch::RTMP_MSG_AUDIO => {
                    self.handle_media_frame(
                        msg_stream_id,
                        FrameType::Audio,
                        out_ts,
                        tag_payload,
                        Some(messages_budget),
                    )?;
                }
                msg_dispatch::RTMP_MSG_VIDEO => {
                    self.handle_media_frame(
                        msg_stream_id,
                        FrameType::Video,
                        out_ts,
                        tag_payload,
                        Some(messages_budget),
                    )?;
                }
                msg_dispatch::RTMP_MSG_AMF0_DATA => {
                    self.handle_publisher_data_message(msg_stream_id, out_ts, tag_payload)?;
                }
                _ => {}
            }

            pos = body + data_size + 4;
        }

        Ok(())
    }

    pub fn get_fd(&self) -> i32 {
        self.client_fd
    }

    pub fn recv(&mut self, data: &[u8]) -> Result<()> {
        if !data.is_empty()
            && self.recv_buffer.available().saturating_add(data.len()) > MAX_RECV_BUFFER_BYTES
        {
            return Err(ErrorCode::Protocol);
        }
        self.recv_buffer
            .write(data)
            .map_err(|_| ErrorCode::Internal)?;
        self.bytes_received = self.bytes_received.saturating_add(data.len() as u64);
        self.budget_exhausted = false;
        let mut max_iter = 256;
        let mut no_progress = 0;
        let mut messages_budget = MAX_MESSAGES_PER_RECV;
        while max_iter > 0 {
            if messages_budget == 0 {
                self.budget_exhausted = true;
                break;
            }
            max_iter -= 1;
            let avail = self.recv_buffer.available();
            if avail == 0 && self.state != ConnState::Handshake {
                break;
            }
            let before = avail;
            let rc = self.process(&mut messages_budget);
            if rc < 0 {
                return Err(match rc {
                    -1 => ErrorCode::Io,
                    -2 => ErrorCode::Timeout,
                    -3 => ErrorCode::Protocol,
                    -4 => ErrorCode::Handshake,
                    -5 => ErrorCode::Chunk,
                    -6 => ErrorCode::Amf,
                    -7 => ErrorCode::Unsupported,
                    -8 => ErrorCode::Auth,
                    -9 => ErrorCode::Internal,
                    _ => ErrorCode::Internal,
                });
            }
            if rc == 0 {
                let after = self.recv_buffer.available();
                if after == before {
                    no_progress += 1;
                    if no_progress > 3 {
                        break;
                    }
                } else {
                    no_progress = 0;
                }
                if after == 0 && self.state < ConnState::Closing {
                    break;
                }
            } else {
                no_progress = 0;
            }
        }
        if self.window_ack_size > 0
            && self.bytes_received.saturating_sub(self.bytes_at_last_ack)
                >= self.window_ack_size as u64
        {
            self.send_acknowledgement(self.bytes_received as u32)?;
            self.bytes_at_last_ack = self.bytes_received;
        }
        Ok(())
    }

    pub fn process(&mut self, messages_budget: &mut usize) -> i32 {
        match self.state {
            ConnState::TcpAccepted | ConnState::Handshake => self.do_handshake(),
            ConnState::Connected
            | ConnState::AppConnected
            | ConnState::StreamCreated
            | ConnState::Publishing
            | ConnState::Playing
            | ConnState::CapsNegotiated => self.read_messages(messages_budget),
            ConnState::Closing | ConnState::Closed => 0,
        }
    }

    pub fn do_handshake(&mut self) -> i32 {
        match self.handshake.state {
            HandshakeState::ServerWaitC0 => {
                handshake::server_init(&mut self.handshake);
                match handshake::server_read_c0(&mut self.handshake, &mut self.recv_buffer) {
                    Ok(()) => {
                        self.state = ConnState::Handshake;
                        self.do_handshake_recurse()
                    }
                    Err(ErrorCode::Io) => 0,
                    Err(e) => e as i32,
                }
            }
            HandshakeState::ServerWaitC1 => self.do_handshake_recurse(),
            HandshakeState::ServerWaitC2 => {
                match handshake::server_read_c2(&mut self.handshake, &mut self.recv_buffer) {
                    Ok(()) => {
                        self.state = ConnState::Connected;
                        1
                    }
                    Err(ErrorCode::Io) => 0,
                    Err(e) => e as i32,
                }
            }
            HandshakeState::Done => {
                self.state = ConnState::Connected;
                1
            }
            _ => -1,
        }
    }

    fn do_handshake_recurse(&mut self) -> i32 {
        match handshake::server_read_c1(&mut self.handshake, &mut self.recv_buffer) {
            Ok(()) => {
                if self.client_fd >= 0 {
                    let s0 = [0x03u8];
                    if self.send_buffer.write(&s0).is_err() {
                        return ErrorCode::Internal as i32;
                    }
                    let out_data = self.handshake.out.peek();
                    if self.send_buffer.write(out_data).is_err() {
                        return ErrorCode::Internal as i32;
                    }
                }
                self.handshake.out.reset();
                1
            }
            Err(ErrorCode::Io) => 0,
            Err(e) => e as i32,
        }
    }

    pub fn read_messages(&mut self, messages_budget: &mut usize) -> i32 {
        let mut processed = 0usize;
        loop {
            if processed >= MAX_MESSAGES_PER_READ || *messages_budget == 0 {
                // Stopped because the budget ran out, not because the buffer
                // is empty -- there may still be complete messages sitting in
                // `recv_buffer` that the caller needs to keep draining.
                self.budget_exhausted = true;
                break;
            }
            let before = self.recv_buffer.available();
            let mut msg = ChunkMessage::default();
            match chunk_read_owned(&mut self.recv_buffer, &mut self.chunk_reg, &mut msg) {
                Ok((0, _)) => {
                    if self.recv_buffer.available() >= before {
                        break;
                    }
                }
                Ok((1, payload_owned)) => {
                    if msg.is_complete {
                        processed += 1;
                        if let Err(e) = self.handle_message(&msg, &payload_owned, messages_budget) {
                            return match e {
                                ErrorCode::Auth => -8,
                                _ => -3,
                            };
                        }
                        let _ = self.flush();
                    }
                }
                Ok(_) => {
                    if self.recv_buffer.available() >= before {
                        break;
                    }
                }
                Err(ErrorCode::Chunk) => return -5,
                Err(_) => return -1,
            }
        }
        1
    }

    fn handle_message(
        &mut self,
        msg: &ChunkMessage,
        payload: &[u8],
        messages_budget: &mut usize,
    ) -> Result<()> {
        match msg.msg_type_id {
            msg_dispatch::RTMP_MSG_AGGREGATE => {
                return self.handle_aggregate(
                    msg.msg_stream_id,
                    msg.timestamp,
                    payload,
                    messages_budget,
                );
            }
            _ => {
                if *messages_budget == 0 {
                    return Ok(());
                }
                *messages_budget = messages_budget.saturating_sub(1);
            }
        }

        match msg.msg_type_id {
            msg_dispatch::RTMP_MSG_SET_CHUNK_SIZE
            | msg_dispatch::RTMP_MSG_ABORT_MESSAGE
            | msg_dispatch::RTMP_MSG_ACKNOWLEDGEMENT
            | msg_dispatch::RTMP_MSG_WINDOW_ACK_SIZE
            | msg_dispatch::RTMP_MSG_SET_PEER_BANDWIDTH => {
                self.handle_control(msg.msg_type_id, payload)
            }
            msg_dispatch::RTMP_MSG_USER_CONTROL => self.handle_user_control(payload),
            msg_dispatch::RTMP_MSG_AMF0_COMMAND => self.handle_command(payload),
            msg_dispatch::RTMP_MSG_AMF3_COMMAND => {
                if !payload.is_empty() && payload[0] == 0x00 {
                    self.handle_command(&payload[1..])
                } else {
                    self.handle_command(payload)
                }
            }
            msg_dispatch::RTMP_MSG_AUDIO => self.handle_media_frame(
                msg.msg_stream_id,
                FrameType::Audio,
                msg.timestamp,
                payload,
                Some(messages_budget),
            ),
            msg_dispatch::RTMP_MSG_VIDEO => self.handle_media_frame(
                msg.msg_stream_id,
                FrameType::Video,
                msg.timestamp,
                payload,
                Some(messages_budget),
            ),
            msg_dispatch::RTMP_MSG_AMF0_DATA => {
                self.handle_publisher_data_message(msg.msg_stream_id, msg.timestamp, payload)
            }
            msg_dispatch::RTMP_MSG_AMF3_DATA => {
                if !payload.is_empty() && payload[0] == 0x00 {
                    self.handle_publisher_data_message(
                        msg.msg_stream_id,
                        msg.timestamp,
                        &payload[1..],
                    )
                } else {
                    self.handle_publisher_data_message(msg.msg_stream_id, msg.timestamp, payload)
                }
            }
            msg_dispatch::RTMP_MSG_AMF3_SHARED_OBJECT => {
                let data = if !payload.is_empty() && payload[0] == 0x00 {
                    &payload[1..]
                } else {
                    payload
                };
                self.handle_amf3_shared_object(data)
            }
            msg_dispatch::RTMP_MSG_AGGREGATE => Ok(()),
            _ => Ok(()),
        }
    }

    fn handle_amf3_shared_object(&mut self, payload: &[u8]) -> Result<()> {
        // Shared objects are an application-scoped feature: reject them
        // before `connect` completes, when `self.app` is still empty and no
        // connect-time authorization has run, so the host callback is never
        // handed a message it cannot attribute to an application.
        if self.state < ConnState::AppConnected {
            return Ok(());
        }
        // Malformed shared-object messages are dropped, not fatal to the
        // connection -- mirrors how other opportunistic-parse paths (e.g.
        // onMetaData) degrade on bad input rather than closing the session.
        let Ok(so) = shared_object::parse(payload) else {
            return Ok(());
        };
        if self.on_shared_object_cb.is_none() {
            return Ok(());
        }
        match self.on_shared_object_auth_cb {
            Some(auth_cb) if !auth_cb(self.conn_id, &so) => return Ok(()),
            None if self.on_publish_cb.is_some()
                || self.on_play_cb.is_some()
                || self.on_media_cb.is_some()
                || self.on_frame_cb.is_some()
                || self.on_release_stream_cb.is_some() =>
            {
                return Ok(());
            }
            _ => {}
        }
        if let Some(cb) = self.on_shared_object_cb {
            cb(self.conn_id, &so);
        }
        Ok(())
    }

    /// Send an AMF3 Shared Object message (RTMP message type `0x10`) on this
    /// connection.
    pub fn send_shared_object(&mut self, so: &SharedObjectMessage) -> Result<()> {
        let mut amf_buf = Buffer::with_capacity(256);
        shared_object::write(so, &mut amf_buf)?;
        // AMF3 command/data/shared-object messages carry a leading 0x00
        // marker byte ahead of the AMF3 payload (mirrors how this module's
        // AMF3 command/data paths are unwrapped on read).
        let mut wire = Vec::with_capacity(amf_buf.available() + 1);
        wire.push(0x00);
        wire.extend_from_slice(amf_buf.as_slice());

        let mut cmsg = ChunkMessage::default();
        cmsg.msg_length = wire.len() as u32;
        cmsg.msg_stream_id = 0;
        cmsg.fmt = 0;
        cmsg.csid = 3;
        cmsg.msg_type_id = msg_dispatch::RTMP_MSG_AMF3_SHARED_OBJECT;
        chunk_write(
            &mut self.send_buffer,
            &cmsg,
            &wire,
            wire.len(),
            self.active_chunk_size as usize,
        )
    }

    fn handle_data_message(&mut self, payload: &[u8]) -> Result<()> {
        let mut buf = Buffer::from_slice(payload);
        let first_byte = match buf.peek().first().copied() {
            Some(b) => b,
            None => return Ok(()),
        };
        if first_byte != crate::amf::amf0::Amf0Type::String as u8
            && first_byte != crate::amf::amf0::Amf0Type::LongString as u8
        {
            return Ok(());
        }

        let mut name = [0u8; 64];
        let Some(name_len) = read_data_event_name(
            &mut buf,
            first_byte == crate::amf::amf0::Amf0Type::String as u8,
            &mut name,
        ) else {
            return Ok(());
        };
        let name_str = std::str::from_utf8(&name[..name_len]).unwrap_or("");

        if name_str == "@setDataFrame" {
            let next_byte = match buf.peek().first().copied() {
                Some(b) => b,
                None => return Ok(()),
            };
            if next_byte != crate::amf::amf0::Amf0Type::String as u8
                && next_byte != crate::amf::amf0::Amf0Type::LongString as u8
            {
                return Ok(());
            }
            let mut inner = [0u8; 64];
            let Some(inner_len) = read_data_event_name(
                &mut buf,
                next_byte == crate::amf::amf0::Amf0Type::String as u8,
                &mut inner,
            ) else {
                return Ok(());
            };
            let inner_str = std::str::from_utf8(&inner[..inner_len]).unwrap_or("");
            if inner_str != "onMetaData" {
                return Ok(());
            }
        } else if name_str != "onMetaData" {
            return Ok(());
        }

        self.clear_detected_stream_metadata();
        self.parse_on_metadata_object(&mut buf)
    }

    fn parse_on_metadata_object(&mut self, buf: &mut Buffer) -> Result<()> {
        let ty = match crate::amf::amf0::read_type(buf) {
            Ok(t) => t,
            Err(_) => return Ok(()),
        };
        if ty != crate::amf::amf0::Amf0Type::Object && ty != crate::amf::amf0::Amf0Type::EcmaArray {
            return Ok(());
        }
        if ty == crate::amf::amf0::Amf0Type::EcmaArray {
            let mut count_bytes = [0u8; 4];
            if buf.read(&mut count_bytes).is_err() {
                return Ok(());
            }
        }

        let mut keys = 0usize;
        while !crate::amf::amf0::is_object_end(buf) {
            keys += 1;
            if keys > crate::amf::amf0::MAX_OBJECT_KEYS {
                return Ok(());
            }
            let mut key = [0u8; 256];
            if crate::amf::amf0::read_object_key(buf, &mut key).is_err() {
                return Ok(());
            }
            let key_len = key.iter().position(|&b| b == 0).unwrap_or(key.len());
            let key_str = std::str::from_utf8(&key[..key_len]).unwrap_or("");
            if !self.apply_metadata_key(key_str, buf) {
                return Ok(());
            }
        }
        let mut end = [0u8; 3];
        if buf.read(&mut end).is_err() {
            return Ok(());
        }
        Ok(())
    }

    fn apply_metadata_key(&mut self, key: &str, buf: &mut Buffer) -> bool {
        let ty = match crate::amf::amf0::read_type(buf) {
            Ok(t) => t,
            Err(_) => return false,
        };
        match key {
            "width" => {
                if ty == crate::amf::amf0::Amf0Type::Number {
                    let v = match crate::amf::amf0::read_number(buf) {
                        Ok(v) => v,
                        Err(_) => return false,
                    };
                    if let Some(w) = positive_f64_to_u32(v) {
                        self.detected_video_width = Some(w);
                    }
                } else {
                    return crate::amf::amf0::skip_value_after_type(buf, ty).is_ok();
                }
            }
            "height" => {
                if ty == crate::amf::amf0::Amf0Type::Number {
                    let v = match crate::amf::amf0::read_number(buf) {
                        Ok(v) => v,
                        Err(_) => return false,
                    };
                    if let Some(h) = positive_f64_to_u32(v) {
                        self.detected_video_height = Some(h);
                    }
                } else {
                    return crate::amf::amf0::skip_value_after_type(buf, ty).is_ok();
                }
            }
            "framerate" | "videoframerate" => {
                if ty == crate::amf::amf0::Amf0Type::Number {
                    let v = match crate::amf::amf0::read_number(buf) {
                        Ok(v) => v,
                        Err(_) => return false,
                    };
                    if sane_framerate(v) {
                        self.detected_video_framerate = Some(v);
                    }
                } else {
                    return crate::amf::amf0::skip_value_after_type(buf, ty).is_ok();
                }
            }
            "audiosamplerate" => {
                if ty == crate::amf::amf0::Amf0Type::Number {
                    let v = match crate::amf::amf0::read_number(buf) {
                        Ok(v) => v,
                        Err(_) => return false,
                    };
                    if let Some(sr) = positive_f64_to_u32(v) {
                        self.detected_audio_sample_rate = Some(sr);
                    }
                } else {
                    return crate::amf::amf0::skip_value_after_type(buf, ty).is_ok();
                }
            }
            "audiochannels" => {
                if ty == crate::amf::amf0::Amf0Type::Number {
                    let v = match crate::amf::amf0::read_number(buf) {
                        Ok(v) => v,
                        Err(_) => return false,
                    };
                    if let Some(ch) = positive_f64_to_u32(v) {
                        if ch > 0 && ch <= 32 {
                            self.detected_audio_channels = Some(ch);
                        }
                    }
                } else {
                    return crate::amf::amf0::skip_value_after_type(buf, ty).is_ok();
                }
            }
            "stereo" => {
                if ty == crate::amf::amf0::Amf0Type::Boolean {
                    let stereo = match crate::amf::amf0::read_boolean(buf) {
                        Ok(v) => v,
                        Err(_) => return false,
                    };
                    self.detected_audio_channels = Some(if stereo { 2 } else { 1 });
                } else {
                    return crate::amf::amf0::skip_value_after_type(buf, ty).is_ok();
                }
            }
            _ => return crate::amf::amf0::skip_value_after_type(buf, ty).is_ok(),
        }
        true
    }

    fn handle_control(&mut self, msg_type_id: u8, payload: &[u8]) -> Result<()> {
        match msg_type_id {
            msg_dispatch::RTMP_MSG_SET_CHUNK_SIZE => {
                if payload.len() >= 4 {
                    if let Ok(cs) = control::read_set_chunk_size(payload) {
                        self.chunk_reg.set_all_chunk_size(cs);
                    }
                }
            }
            msg_dispatch::RTMP_MSG_ABORT_MESSAGE => {
                if payload.len() >= 4 {
                    if let Ok(csid) = control::read_abort_message(payload) {
                        self.chunk_reg.reset_stream(csid);
                    }
                }
            }
            msg_dispatch::RTMP_MSG_WINDOW_ACK_SIZE => {
                if payload.len() >= 4 {
                    if let Ok(win) = control::read_window_ack_size(payload) {
                        self.window_ack_size = win;
                    }
                }
            }
            msg_dispatch::RTMP_MSG_ACKNOWLEDGEMENT => {
                if payload.len() >= 4 {
                    let _ = control::read_acknowledgement_size(payload);
                }
            }
            msg_dispatch::RTMP_MSG_SET_PEER_BANDWIDTH => {
                if payload.len() >= 5 {
                    let _ = control::read_set_peer_bandwidth(payload);
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Apply a chunk size to outbound writes and inbound reassembly.
    pub fn apply_chunk_size(&mut self, chunk_size: u32) {
        self.chunk_size = chunk_size;
        self.active_chunk_size = chunk_size;
        self.chunk_reg.set_all_chunk_size(chunk_size);
    }

    fn activate_announced_chunk_size(&mut self) {
        // Only our own chunks change size. The peer keeps sending 128-byte chunks until it
        // announces a size of its own (handle_control); librtmp-based publishers such as DJI
        // Fly never do, so reading them at our size splits their first large frame wrongly.
        self.active_chunk_size = self.chunk_size;
    }

    fn handle_user_control(&mut self, payload: &[u8]) -> Result<()> {
        if payload.len() < 6 {
            return Ok(());
        }
        let event_type = ((payload[0] as u16) << 8) | (payload[1] as u16);
        let (event_type, param1, param2) = if event_type == UCTRL_SET_BUFFER_LENGTH {
            control::read_user_control(payload, true)?
        } else {
            let (ty, p1, _) = control::read_user_control(payload, false)?;
            (ty, p1, None)
        };
        match event_type {
            UCTRL_PING_RESPONSE => {
                if let Some(sent_at) = self.pending_pings.remove(&param1) {
                    self.rtt_ms = sent_at.elapsed().as_secs_f64() * 1000.0;
                }
            }
            UCTRL_PING_REQUEST => {
                let now = Instant::now();
                if let Some(start) = self.inbound_ping_window_start {
                    if now.duration_since(start) >= INBOUND_PING_WINDOW {
                        self.inbound_ping_window_start = Some(now);
                        self.inbound_ping_responses = 0;
                    }
                } else {
                    self.inbound_ping_window_start = Some(now);
                }
                if self.inbound_ping_responses >= MAX_INBOUND_PING_RESPONSES {
                    return Err(ErrorCode::Protocol);
                }
                self.inbound_ping_responses += 1;
                self.send_user_control_ping_response(param1)?;
            }
            UCTRL_STREAM_BEGIN => {
                let _ = param1;
            }
            UCTRL_STREAM_EOF => {
                let _ = param1;
            }
            UCTRL_SET_BUFFER_LENGTH => {
                if let Some(ms) = param2 {
                    self.buffer_length_ms = ms;
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Send an RTMP ping when due and measure RTT from the client's response.
    pub fn maybe_send_ping(&mut self) -> Result<()> {
        if self.state < ConnState::AppConnected {
            return Ok(());
        }
        let now = Instant::now();
        if self
            .last_ping_sent
            .is_some_and(|t| now.duration_since(t) < PING_INTERVAL)
        {
            return Ok(());
        }
        if let Some(queued) = &self.queued_ping {
            if now.duration_since(queued.queued_at) >= PING_TIMEOUT {
                return Err(ErrorCode::Protocol);
            }
            return Ok(());
        }

        let had_stale_ping = self
            .pending_pings
            .values()
            .any(|sent| now.duration_since(*sent) >= PING_TIMEOUT);
        self.pending_pings
            .retain(|_, sent| now.duration_since(*sent) < PING_TIMEOUT);
        if had_stale_ping {
            return Err(ErrorCode::Protocol);
        }
        if self.pending_pings.len() + usize::from(self.queued_ping.is_some()) >= MAX_PENDING_PINGS {
            return Err(ErrorCode::Protocol);
        }

        let token = self.next_ping_token;
        self.next_ping_token = self.next_ping_token.wrapping_add(1);
        self.send_user_control_ping_request(token)?;
        self.queued_ping = Some(QueuedPing {
            token,
            queued_at: now,
            bytes_until_flushed: self.send_buffer.available(),
        });
        Ok(())
    }

    /// True when any configured callback implies publish/play must be explicitly
    /// authorized via `on_publish_cb` / `on_play_cb` (mirrors the cross-gate
    /// used for `on_media_cb` / `on_frame_cb`).
    fn requires_explicit_publish_auth(&self) -> bool {
        self.on_play_cb.is_some()
            || self.on_media_cb.is_some()
            || self.on_frame_cb.is_some()
            || self.on_shared_object_auth_cb.is_some()
            || self.on_release_stream_cb.is_some()
    }

    fn requires_explicit_play_auth(&self) -> bool {
        self.on_publish_cb.is_some()
            || self.on_media_cb.is_some()
            || self.on_frame_cb.is_some()
            || self.on_shared_object_auth_cb.is_some()
            || self.on_release_stream_cb.is_some()
    }

    pub fn handle_command(&mut self, payload: &[u8]) -> Result<()> {
        let mut buf = Buffer::from_slice(payload);
        let mut name_buf = [0u8; 64];
        if command::peek_name(&mut buf, &mut name_buf).is_err() {
            return Ok(());
        }
        let name = std::str::from_utf8(&name_buf)
            .unwrap_or("")
            .trim_end_matches('\0');
        match name {
            "connect" => {
                // A connection may only negotiate its app namespace once. Without
                // this guard a peer that's already publishing/playing under an
                // authorized app could send a second `connect` with a different
                // app, silently repointing `self.app` (and therefore relay/cache
                // routing) to a namespace `on_publish_cb`/`on_play_cb` never
                // authorized -- cache poisoning / cross-app playback.
                if self.state >= ConnState::AppConnected {
                    return Ok(());
                }
                let mut info = ConnectInfo::default();
                if command::read_connect(&mut buf, &mut info).is_err() {
                    self.send_command_error(
                        info.transaction_id,
                        "NetConnection.Connect.Rejected",
                        "Invalid connect command or capability negotiation.",
                    )?;
                    return Ok(());
                }
                self.app = match command::decode_route_amf_string(&info.app) {
                    Ok(app) => app,
                    Err(_) => {
                        self.send_command_error(
                            info.transaction_id,
                            "NetConnection.Connect.Rejected",
                            "Invalid connect app name.",
                        )?;
                        return Ok(());
                    }
                };
                if self.app.is_empty() {
                    self.send_command_error(
                        info.transaction_id,
                        "NetConnection.Connect.Rejected",
                        "Empty connect app name.",
                    )?;
                    return Ok(());
                }
                let needs_caps = info.has_four_cc_list
                    || info.has_caps_ex
                    || info.has_video_four_cc_info_map
                    || info.has_reconnect;
                if needs_caps {
                    let _ =
                        state_machine::conn_transition(&mut self.state, ConnState::CapsNegotiated);
                }
                let negotiated = if needs_caps {
                    let caps = negotiate_caps(&info);
                    self.negotiated_caps = caps.clone();
                    Some(caps)
                } else {
                    None
                };
                let _ = state_machine::conn_transition(&mut self.state, ConnState::AppConnected);
                self.send_connect_response(info.transaction_id, negotiated.as_ref())?;
                if !self.connect_cb_fired {
                    self.connect_cb_fired = true;
                    if let Some(cb) = self.on_connect_cb {
                        cb();
                    }
                }
            }
            "createStream" => {
                if self.state < ConnState::AppConnected {
                    return self.send_onstatus(
                        0,
                        "error",
                        "NetStream.Failed",
                        "connect required before createStream",
                    );
                }
                let txn = command::read_create_stream(&mut buf)?;
                if self.next_stream_id >= MAX_STREAMS_PER_CONN {
                    self.send_onstatus(0, "error", "NetStream.Failed", "Too many streams")?;
                } else {
                    // A fresh createStream replaces current_stream outright;
                    // if the stream it's replacing was actively publishing,
                    // that route is being abandoned and must be evicted now
                    // rather than left as a dangling cache-key tracking
                    // entry until this connection eventually disconnects.
                    let was_active = self
                        .current_stream
                        .as_ref()
                        .is_some_and(|s| s.is_publishing || s.is_playing);
                    self.evict_active_publish_route();
                    // Mirror FCUnpublish/deleteStream/closeStream: a fresh
                    // createStream abandons the current publish/play role but
                    // must not leave a stale `relay_enabled` from a prior
                    // integrator approval, or `defer_media_relay` servers can
                    // be bypassed by createStream -> publish without
                    // re-authorization.
                    self.relay_enabled = false;
                    self.next_stream_id += 1;
                    let stream_id = self.next_stream_id;
                    self.current_stream = Some(Box::new(Stream::new(stream_id)));
                    if was_active && self.teardown_refreshes_setup_timer() {
                        self.session_setup_started = Instant::now();
                    }
                    let _ =
                        state_machine::conn_transition(&mut self.state, ConnState::StreamCreated);
                    self.send_create_stream_response(txn, stream_id)?;
                }
            }
            "publish" => {
                let mut stream_name = [0u8; 256];
                let mut publish_type = [0u8; 64];
                command::read_publish(&mut buf, &mut stream_name, &mut publish_type)?;
                let name_str = match command::decode_route_amf_string(&stream_name) {
                    Ok(name) => name,
                    Err(_) => {
                        return self.send_onstatus(
                            0,
                            "error",
                            "NetStream.Publish.BadName",
                            "Invalid stream name",
                        );
                    }
                };
                // A single-segment RTMP URL such as rtmp://host/drone uses the
                // application name as the complete route and publishes an empty
                // stream name. Let an installed authorization callback decide
                // whether that explicit application route is allowed.
                if name_str.is_empty() && self.on_publish_cb.is_none() {
                    return self.send_onstatus(
                        0,
                        "error",
                        "NetStream.Publish.BadName",
                        "Empty stream name",
                    );
                }
                if self.current_stream.is_none() {
                    return self.send_onstatus(
                        0,
                        "error",
                        "NetStream.Publish.BadConnection",
                        "No stream created",
                    );
                }
                if self.on_publish_cb.is_none() && self.requires_explicit_publish_auth() {
                    return self.send_onstatus(
                        0,
                        "error",
                        "NetStream.Publish.BadName",
                        "Publish not authorized",
                    );
                }
                if let Some(cb) = self.on_publish_cb {
                    if !cb(self.conn_id, &self.app, &name_str) {
                        return self.send_onstatus(
                            0,
                            "error",
                            "NetStream.Publish.BadName",
                            "Publish not authorized",
                        );
                    }
                }
                // Use current_stream.is_publishing rather than self.state
                // here: conn_transition() only allows forward moves through
                // ConnState, so a play-then-publish sequence leaves
                // self.state stuck at Playing (Publishing < Playing) even
                // though this stream is genuinely publishing again. `play`
                // explicitly clears is_publishing when it takes over a
                // stream, so the flag accurately reflects whether this
                // specific current_stream is the one actively publishing.
                let was_publishing = self
                    .current_stream
                    .as_ref()
                    .map(|s| s.is_publishing)
                    .unwrap_or(false);
                let prev_route_key = self.relay_route_key();
                let next_route_key = if !self.relay_key.is_empty() {
                    self.relay_key.clone()
                } else {
                    name_str.clone()
                };
                let renaming_route = was_publishing
                    && !prev_route_key.is_empty()
                    && prev_route_key != next_route_key;
                if !self.claim_publish_route(&next_route_key) {
                    return self.send_onstatus(
                        0,
                        "error",
                        "NetStream.Publish.BadName",
                        "Route already publishing",
                    );
                }
                // Start a publish-media deadline only for a genuinely new
                // publish session or route. Repeating `publish` for the route
                // already owned by this connection must not refresh the timer.
                if !was_publishing || renaming_route {
                    self.session_setup_started = Instant::now();
                    // Injected activity from a prior publish/idle epoch must
                    // not exempt a new empty publish from the media deadline.
                    self.injected_media_bytes = 0;
                }
                if renaming_route {
                    if self
                        .push_pending_cache_eviction(self.app.clone(), prev_route_key.clone())
                        .is_err()
                    {
                        if let Some(routes) = self.publish_routes.as_ref() {
                            routes.release(self.conn_id, &self.app, &next_route_key);
                            let _ = routes.claim(self.conn_id, &self.app, &prev_route_key);
                        }
                        self.claimed_publish_route = Some(prev_route_key);
                        return self.send_onstatus(
                            0,
                            "error",
                            "NetStream.Publish.BadName",
                            "Publish not authorized",
                        );
                    }
                }
                if self.defer_media_relay {
                    // Reset only when leaving a prior play role or switching
                    // the RTMP publish name. Duplicate `publish` for the same
                    // name must keep an already-authorized deferred relay and
                    // its pending frames (integrator re-auth is unchanged).
                    // Detect switches via stream.name — with a pinned
                    // relay_key, next_route_key stays on the old DB id so
                    // renaming_route alone cannot see A→B.
                    let was_playing = self.current_stream.as_ref().is_some_and(|s| s.is_playing);
                    let publish_name_changed = self
                        .current_stream
                        .as_ref()
                        .is_none_or(|s| s.name != name_str);
                    if was_playing || (was_publishing && publish_name_changed) {
                        self.relay_enabled = false;
                        self.pending_relay.clear();
                    }
                } else {
                    self.relay_enabled = true;
                }
                {
                    if let Some(ref mut stream) = self.current_stream {
                        stream.is_publishing = true;
                        stream.is_playing = false;
                        stream.name = name_str;
                    }
                    self.clear_detected_stream_metadata();
                    let _ = state_machine::conn_transition(&mut self.state, ConnState::Publishing);
                    let sid = self
                        .current_stream
                        .as_ref()
                        .map(|s| s.stream_id)
                        .unwrap_or(0);
                    self.send_onstatus(sid, "status", "NetStream.Publish.Start", "Publishing")?;
                }
            }
            "play" => {
                let mut stream_name = [0u8; 256];
                command::read_play(&mut buf, &mut stream_name)?;
                let name_str = match command::decode_route_amf_string(&stream_name) {
                    Ok(name) => name,
                    Err(_) => {
                        return self.send_onstatus(
                            0,
                            "error",
                            "NetStream.Play.Failed",
                            "Invalid stream name",
                        );
                    }
                };
                if name_str.is_empty() {
                    return self.send_onstatus(
                        0,
                        "error",
                        "NetStream.Play.Failed",
                        "Empty stream name",
                    );
                }
                if self.current_stream.is_none() {
                    return self.send_onstatus(
                        0,
                        "error",
                        "NetStream.Play.BadConnection",
                        "No stream created",
                    );
                }
                if self.on_play_cb.is_none() && self.requires_explicit_play_auth() {
                    return self.send_onstatus(
                        0,
                        "error",
                        "NetStream.Play.Failed",
                        "Play not authorized",
                    );
                }
                if let Some(cb) = self.on_play_cb {
                    if !cb(self.conn_id, &self.app, &name_str) {
                        return self.send_onstatus(
                            0,
                            "error",
                            "NetStream.Play.Failed",
                            "Play not authorized",
                        );
                    }
                }
                if !self.defer_media_relay {
                    self.relay_enabled = true;
                }
                {
                    // `play` supersedes any publish role this current_stream
                    // held: evict the abandoned publish route's cache key
                    // now (there's no "next" publish route to preserve it
                    // for), and clear is_publishing so it can't be
                    // mistaken for an active publish by a later command.
                    self.evict_active_publish_route();
                    let already_playing_same = self
                        .current_stream
                        .as_ref()
                        .map(|s| s.is_playing && s.name == name_str)
                        .unwrap_or(false);
                    if let Some(ref mut stream) = self.current_stream {
                        stream.is_playing = true;
                        stream.is_publishing = false;
                        stream.name = name_str;
                    }
                    if !already_playing_same {
                        // Relay bytes from a previous play route are historical
                        // for pause-grace purposes. A new route must deliver
                        // fresh bytes before it can earn another grace reset.
                        self.pause_grace_media_bytes_sent = self.media_bytes_sent;
                        self.request_init_replay();
                    }
                    let _ = state_machine::conn_transition(&mut self.state, ConnState::Playing);
                    let sid = self
                        .current_stream
                        .as_ref()
                        .map(|s| s.stream_id)
                        .unwrap_or(0);
                    self.send_onstatus(sid, "status", "NetStream.Play.Start", "Playing")?;
                    self.send_stream_lifecycle_begin(sid)?;
                }
            }
            "FCUnpublish" | "deleteStream" => {
                // The client is explicitly tearing down its publish or play
                // role -- deleteStream is sent by both publishers and
                // players, not just publishers, so this must clear
                // is_playing too or a player using deleteStream would never
                // be considered idle (see `session_setup_timed_out`).
                // Release the claimed publish route now rather than holding
                // it until the TCP connection eventually closes, so a
                // different connection can immediately republish the same
                // (app, stream) route.
                let was_active = self
                    .current_stream
                    .as_ref()
                    .is_some_and(|s| s.is_publishing || s.is_playing);
                self.evict_active_publish_route();
                if let Some(ref mut stream) = self.current_stream {
                    stream.is_publishing = false;
                    stream.is_playing = false;
                    // Matches closeStream: play() never resets `paused`, so
                    // a stale true here would leave a later play on this
                    // connection silently receiving no relayed frames
                    // (relay delivery is gated on !paused).
                    stream.paused = false;
                }
                // Give this connection a fresh setup-timeout window now that
                // it's gone idle, rather than treating the moment it stops
                // publishing/playing as an immediate setup failure --
                // otherwise a client that has been legitimately active for a
                // long time gets reaped the instant it tears down if it
                // doesn't restart within whatever's left of the original 10s
                // window (see `session_setup_timed_out`). Only reset when
                // this was a genuine active->idle transition: a peer that
                // was never active gets no reset here, so it can't use
                // repeated FCUnpublish/deleteStream calls to keep dodging
                // the reaper forever.
                if was_active && self.teardown_refreshes_setup_timer() {
                    self.session_setup_started = Instant::now();
                }
                // Clear relay_enabled along with the publish role: with
                // `defer_media_relay`, the publish handler leaves it
                // untouched on the next `publish` (the integrator must
                // explicitly re-authorize it via on_publish_cb), so a stale
                // `true` here would let a new publish on this connection
                // relay before it's actually re-authorized.
                self.relay_enabled = false;
                if let Some(sid) = self.current_stream.as_ref().map(|s| s.stream_id) {
                    let _ = self.send_stream_lifecycle_eof(sid);
                }
            }
            "FCPublish" => {}
            "releaseStream" => {
                let mut stream_name = [0u8; 256];
                if command::read_release_stream(&mut buf, &mut stream_name).is_ok() {
                    if let Ok(name_str) = command::decode_route_amf_string(&stream_name) {
                        // Force-releasing a route can evict a *different*, still-live
                        // connection's active publish claim, so this stays a safe no-op
                        // unless the host explicitly authorizes it -- unlike
                        // on_media_cb/on_publish_cb, there is no permissive default here.
                        // An empty name is the application-only route of a single-segment
                        // URL such as rtmp://host/drone; as with publish, the callback
                        // decides whether that route may be taken over.
                        let authorized = self
                            .on_release_stream_cb
                            .is_some_and(|cb| cb(self.conn_id, &self.app, &name_str));
                        if authorized {
                            if let Some(ref routes) = self.publish_routes {
                                routes.force_release(&self.app, &name_str);
                            }
                        }
                    }
                }
            }
            "pause" => {
                if let Ok(pause_flag) = command::read_pause(&mut buf) {
                    if let Some(ref mut stream) = self.current_stream {
                        if pause_flag && stream.is_playing && !stream.paused {
                            // Give paused players the same setup-timeout grace
                            // window as FCUnpublish/deleteStream so a brief pause
                            // during playback is not reaped immediately. Require
                            // relay activity since the previous grace reset; the
                            // lifetime `media_bytes_sent` counter alone would let
                            // historical bytes power unlimited pause cycling.
                            if self.media_bytes_sent > self.pause_grace_media_bytes_sent {
                                self.session_setup_started = Instant::now();
                                self.pause_grace_media_bytes_sent = self.media_bytes_sent;
                            }
                        }
                        stream.paused = pause_flag;
                    }
                    let sid = self
                        .current_stream
                        .as_ref()
                        .map(|s| s.stream_id)
                        .unwrap_or(0);
                    let (code, desc) = if pause_flag {
                        ("NetStream.Pause.Notify", "Paused")
                    } else {
                        ("NetStream.Unpause.Notify", "Unpaused")
                    };
                    self.send_onstatus(sid, "status", code, desc)?;
                }
            }
            "seek" => {
                let _millis = command::read_seek(&mut buf).unwrap_or(0.0);
                let sid = self
                    .current_stream
                    .as_ref()
                    .map(|s| s.stream_id)
                    .unwrap_or(0);
                self.send_onstatus(sid, "status", "NetStream.Seek.Notify", "Seeking")?;
            }
            "receiveAudio" => {
                if let Ok(flag) = command::read_bool_command(&mut buf) {
                    if let Some(ref mut stream) = self.current_stream {
                        stream.receive_audio = flag;
                    }
                }
            }
            "receiveVideo" => {
                if let Ok(flag) = command::read_bool_command(&mut buf) {
                    if let Some(ref mut stream) = self.current_stream {
                        stream.receive_video = flag;
                    }
                }
            }
            "closeStream" => {
                let target_id = command::read_close_stream(&mut buf)
                    .ok()
                    .flatten()
                    .or_else(|| self.current_stream.as_ref().map(|s| s.stream_id))
                    .unwrap_or(0);
                if self.current_stream.as_ref().map(|s| s.stream_id) == Some(target_id) {
                    let was_active = self
                        .current_stream
                        .as_ref()
                        .is_some_and(|s| s.is_publishing || s.is_playing);
                    self.evict_active_publish_route();
                    if let Some(ref mut stream) = self.current_stream {
                        stream.is_playing = false;
                        stream.is_publishing = false;
                        stream.paused = false;
                    }
                    // See the matching comment in the FCUnpublish/deleteStream
                    // arm: give this connection a fresh setup-timeout window
                    // now that it's gone idle, but only on a genuine
                    // active->idle transition -- a peer that was never
                    // publishing/playing gets no reset, so it can't use
                    // repeated closeStream calls to dodge the reaper forever.
                    if was_active && self.teardown_refreshes_setup_timer() {
                        self.session_setup_started = Instant::now();
                    }
                    self.relay_enabled = false;
                    let _ = self.send_stream_lifecycle_eof(target_id);
                }
            }
            _ => {}
        }
        Ok(())
    }

    pub fn send_connect_response(
        &mut self,
        transaction_id: f64,
        caps: Option<&NegotiatedCaps>,
    ) -> Result<()> {
        let win = SERVER_WINDOW_ACK_SIZE.to_be_bytes();
        self.send_control(0x05, &win)?;
        let mut bw = [0u8; 5];
        let bw_val = SERVER_PEER_BANDWIDTH.to_be_bytes();
        bw[..4].copy_from_slice(&bw_val);
        bw[4] = PEER_BANDWIDTH_DYNAMIC;
        self.send_control(0x06, &bw)?;
        let cs = self.chunk_size.to_be_bytes();
        self.send_control(0x01, &cs)?;
        // Negotiation complete: subsequent server chunks use the announced size
        // (connect AMF above was still at 128).
        self.activate_announced_chunk_size();
        let mut amf_buf = Buffer::with_capacity(512);
        crate::amf::amf0::write_string(&mut amf_buf, "_result")?;
        crate::amf::amf0::write_number(&mut amf_buf, transaction_id)?;
        crate::amf::amf0::write_null(&mut amf_buf)?;
        crate::amf::amf0::write_object_begin(&mut amf_buf)?;
        crate::amf::amf0::write_object_key(&mut amf_buf, "level")?;
        crate::amf::amf0::write_string(&mut amf_buf, "status")?;
        crate::amf::amf0::write_object_key(&mut amf_buf, "code")?;
        crate::amf::amf0::write_string(&mut amf_buf, "NetConnection.Connect.Success")?;
        crate::amf::amf0::write_object_key(&mut amf_buf, "description")?;
        crate::amf::amf0::write_string(&mut amf_buf, "Connection succeeded.")?;
        if let Some(caps) = caps {
            write_negotiated_caps(&mut amf_buf, caps)?;
        }
        crate::amf::amf0::write_object_end(&mut amf_buf)?;
        self.send_command(0, amf_buf.as_slice())
    }

    pub fn send_command_error(
        &mut self,
        transaction_id: f64,
        code: &str,
        description: &str,
    ) -> Result<()> {
        let mut amf_buf = Buffer::with_capacity(256);
        command::build_error(&mut amf_buf, transaction_id, code, description)?;
        self.send_command(0, amf_buf.as_slice())
    }

    pub fn send_create_stream_response(
        &mut self,
        transaction_id: f64,
        stream_id: u32,
    ) -> Result<()> {
        let mut amf_buf = Buffer::with_capacity(256);
        command::build_create_stream_result(&mut amf_buf, transaction_id, stream_id as f64)?;
        self.send_command(0, amf_buf.as_slice())
    }

    pub fn send_onstatus(
        &mut self,
        stream_id: u32,
        level: &str,
        code: &str,
        description: &str,
    ) -> Result<()> {
        let mut amf_buf = Buffer::with_capacity(512);
        command::build_onstatus(&mut amf_buf, level, code, description)?;
        self.send_command(stream_id, amf_buf.as_slice())
    }

    /// Ask the client to reconnect, per the E-RTMP v2 reconnect mechanism:
    /// sends `NetConnection.Connect.ReconnectRequest` on the connection
    /// (stream id 0). `tc_url` redirects the client to a different server;
    /// `None` tells it to reuse its current connection URL. The client is
    /// expected to keep streaming through the next media boundary, then
    /// establish a new connection and disconnect from this one -- this call
    /// only sends the request, it does not close the connection itself.
    pub fn send_reconnect_request(
        &mut self,
        tc_url: Option<&str>,
        description: Option<&str>,
    ) -> Result<()> {
        let mut amf_buf = Buffer::with_capacity(512);
        command::build_reconnect_request(&mut amf_buf, tc_url, description)?;
        self.send_command(0, amf_buf.as_slice())
    }

    pub fn flush(&mut self) -> Result<()> {
        if self.client_fd < 0 || self.send_buffer.available() == 0 {
            self.commit_flushed_ping();
            return Ok(());
        }
        let Some(ref mut transport) = self.transport else {
            self.commit_flushed_ping();
            return Ok(());
        };
        while self.send_buffer.available() > 0 {
            let pending = self.send_buffer.peek();
            let n = transport.try_send(pending, &mut 0i32)?;
            if n == 0 {
                break;
            }
            self.send_buffer.drain(n);
            if let Some(ref mut queued) = self.queued_ping {
                queued.bytes_until_flushed = queued.bytes_until_flushed.saturating_sub(n);
            }
        }
        self.commit_flushed_ping();
        Ok(())
    }

    /// Drop the transport and clear the embedder-visible fd copy.
    pub fn disconnect_transport(&mut self) {
        self.transport = None;
        self.client_fd = -1;
    }

    fn commit_flushed_ping(&mut self) {
        let Some(queued) = self.queued_ping.as_ref() else {
            return;
        };
        if queued.bytes_until_flushed > 0 {
            return;
        }
        let now = Instant::now();
        self.pending_pings.insert(queued.token, now);
        self.last_ping_sent = Some(now);
        self.queued_ping = None;
    }

    pub fn send_frame(
        &mut self,
        frame_type: FrameType,
        timestamp: u32,
        payload: &[u8],
    ) -> Result<()> {
        let stream_id = self
            .current_stream
            .as_ref()
            .map(|s| s.stream_id)
            .unwrap_or(1);
        let mut cmsg = ChunkMessage::default();
        cmsg.timestamp = timestamp;
        cmsg.msg_length = payload.len() as u32;
        cmsg.msg_stream_id = stream_id;
        cmsg.fmt = 0;
        match frame_type {
            FrameType::Audio => {
                cmsg.csid = 4;
                cmsg.msg_type_id = 0x08;
            }
            FrameType::Video => {
                cmsg.csid = 6;
                cmsg.msg_type_id = 0x09;
            }
            FrameType::Script | FrameType::Metadata => {
                cmsg.csid = 5;
                cmsg.msg_type_id = 0x12;
            }
        }
        chunk_write(
            &mut self.send_buffer,
            &cmsg,
            payload,
            payload.len(),
            self.active_chunk_size as usize,
        )?;
        self.media_bytes_sent = self.media_bytes_sent.saturating_add(payload.len() as u64);
        Ok(())
    }

    /// Send an AMF0 data message (e.g. onMetaData) on the current stream.
    pub fn send_data_message(&mut self, timestamp: u32, payload: &[u8]) -> Result<()> {
        let stream_id = self
            .current_stream
            .as_ref()
            .map(|s| s.stream_id)
            .unwrap_or(1);
        let mut cmsg = ChunkMessage::default();
        cmsg.timestamp = timestamp;
        cmsg.msg_length = payload.len() as u32;
        cmsg.msg_stream_id = stream_id;
        cmsg.fmt = 0;
        cmsg.csid = 5;
        cmsg.msg_type_id = msg_dispatch::RTMP_MSG_AMF0_DATA;
        chunk_write(
            &mut self.send_buffer,
            &cmsg,
            payload,
            payload.len(),
            self.active_chunk_size as usize,
        )?;
        self.media_bytes_sent = self.media_bytes_sent.saturating_add(payload.len() as u64);
        Ok(())
    }

    fn send_control(&mut self, ty: u8, data: &[u8]) -> Result<()> {
        let mut msg = ChunkMessage::default();
        msg.csid = 2;
        msg.fmt = 0;
        msg.msg_length = data.len() as u32;
        msg.msg_type_id = ty;
        msg.msg_stream_id = 0;
        chunk_write(
            &mut self.send_buffer,
            &msg,
            data,
            data.len(),
            self.active_chunk_size as usize,
        )
    }

    fn send_command(&mut self, msg_stream_id: u32, amf_data: &[u8]) -> Result<()> {
        let mut cmd_msg = ChunkMessage::default();
        cmd_msg.csid = 3;
        cmd_msg.fmt = 0;
        cmd_msg.timestamp = 0;
        cmd_msg.msg_length = amf_data.len() as u32;
        cmd_msg.msg_type_id = 0x14;
        cmd_msg.msg_stream_id = msg_stream_id;
        chunk_write(
            &mut self.send_buffer,
            &cmd_msg,
            amf_data,
            amf_data.len(),
            self.active_chunk_size as usize,
        )
    }

    fn send_acknowledgement(&mut self, seq: u32) -> Result<()> {
        self.send_control(0x03, &seq.to_be_bytes())
    }

    fn send_user_control_ping_request(&mut self, timestamp: u32) -> Result<()> {
        let mut buf = Buffer::with_capacity(6);
        control::write_user_control_ping_request(&mut buf, timestamp)?;
        self.send_control(msg_dispatch::RTMP_MSG_USER_CONTROL, buf.as_slice())
    }

    fn send_user_control_ping_response(&mut self, timestamp: u32) -> Result<()> {
        let mut buf = Buffer::with_capacity(6);
        control::write_user_control_ping_response(&mut buf, timestamp)?;
        self.send_control(msg_dispatch::RTMP_MSG_USER_CONTROL, buf.as_slice())
    }

    fn send_stream_lifecycle_begin(&mut self, stream_id: u32) -> Result<()> {
        let mut buf = Buffer::with_capacity(14);
        control::write_user_control_stream_begin(&mut buf, stream_id)?;
        self.send_control(msg_dispatch::RTMP_MSG_USER_CONTROL, buf.as_slice())?;
        buf.reset();
        control::write_user_control_set_buffer_length(&mut buf, stream_id, self.buffer_length_ms)?;
        self.send_control(msg_dispatch::RTMP_MSG_USER_CONTROL, buf.as_slice())
    }

    fn send_stream_lifecycle_eof(&mut self, stream_id: u32) -> Result<()> {
        let mut buf = Buffer::with_capacity(6);
        control::write_user_control_stream_eof(&mut buf, stream_id)?;
        self.send_control(msg_dispatch::RTMP_MSG_USER_CONTROL, buf.as_slice())
    }

    fn invoke_multitrack_on_frame_cb(
        &mut self,
        cb: fn(&Frame),
        frame_type: FrameType,
        timestamp: u32,
        track_id: u8,
        fourcc: [u8; 4],
        packet_type: u8,
        video_frame_type: u8,
        payload: &[u8],
    ) {
        self.frame_cb_scratch.clear();
        self.frame_cb_scratch.extend_from_slice(payload);
        let mut frame = Frame {
            frame_type,
            timestamp,
            size: self.frame_cb_scratch.len() as u32,
            data: self.frame_cb_scratch.as_ptr(),
            track_id,
            publisher_conn_id: self.conn_id,
            ..Default::default()
        };
        populate_multitrack_frame(&mut frame, fourcc, packet_type, video_frame_type);
        cb(&frame);
    }
    fn invoke_on_frame_cb(
        &mut self,
        cb: fn(&Frame),
        frame_type: FrameType,
        timestamp: u32,
        track_id: u8,
        payload: &[u8],
    ) {
        self.frame_cb_scratch.clear();
        self.frame_cb_scratch.extend_from_slice(payload);
        let mut frame = Frame {
            frame_type,
            timestamp,
            size: self.frame_cb_scratch.len() as u32,
            data: self.frame_cb_scratch.as_ptr(),
            track_id,
            publisher_conn_id: self.conn_id,
            ..Default::default()
        };
        populate_av_frame(&mut frame, &self.frame_cb_scratch);
        cb(&frame);
    }
}

impl Default for Conn {
    fn default() -> Self {
        Self::new()
    }
}

fn positive_f64_to_u32(v: f64) -> Option<u32> {
    if !v.is_finite() || v < 0.0 || v > u32::MAX as f64 {
        return None;
    }
    Some(v as u32)
}

fn sane_framerate(v: f64) -> bool {
    v.is_finite() && v > 0.0 && v <= 1000.0
}

fn read_data_event_name(buf: &mut Buffer, is_string: bool, out: &mut [u8; 64]) -> Option<usize> {
    match if is_string {
        crate::amf::amf0::read_string(buf, out)
    } else {
        crate::amf::amf0::read_long_string(buf, out)
    } {
        Ok(n) => Some(n),
        Err(_) => None,
    }
}

/// Wildcard FourCC (`*`) is valid in E-RTMP capability objects but must not
/// appear on publisher media tags — it carries no concrete codec identity and
/// would let codec-specific `on_media_cb` deny lists be bypassed.
fn is_wildcard_media_fourcc(fourcc: &[u8; 4]) -> bool {
    fourcc[0] == b'*'
}

/// Codec label for `on_media_cb`, or `None` when the wire FourCC cannot be
/// authorized (wildcard / unknown-enhanced identity).
fn media_fourcc_auth_label(fourcc: &[u8; 4]) -> Option<String> {
    if is_wildcard_media_fourcc(fourcc) {
        return None;
    }
    Some(fourcc_auth_label(fourcc))
}

/// Stable codec label for `on_media_cb` authorization. Valid UTF-8 FourCCs are
/// passed through; non-UTF-8 bytes are hex-encoded so deny-list policies can
/// still observe the identity instead of receiving `None`.
fn fourcc_auth_label(fourcc: &[u8; 4]) -> String {
    match std::str::from_utf8(fourcc) {
        Ok(label) if !label.is_empty() => label.to_string(),
        _ => format!(
            "fourcc:{:02x}{:02x}{:02x}{:02x}",
            fourcc[0], fourcc[1], fourcc[2], fourcc[3]
        ),
    }
}

fn detect_video_codec(payload: &[u8]) -> Option<String> {
    if let Some(cc) = first_track_fourcc(FrameType::Video, payload) {
        return media_fourcc_auth_label(&cc);
    }
    let mut hdr = VideoHeader::default();
    if crate::ertmp::exvideo::exvideo_parse(payload, &mut hdr).is_err() {
        return None;
    }
    if hdr.is_ex_header != 0 {
        // Unpeeled ModEx wrappers expose extension metadata at bytes 1..5,
        // not the inner codec FourCC — treat as unknown so deny lists apply.
        if hdr.packet_type == ERTMP_PACKET_TYPE_MODEX {
            return None;
        }
        let mut fourcc = [0u8; 4];
        fourcc.copy_from_slice(&hdr.fourcc[..4]);
        return media_fourcc_auth_label(&fourcc);
    } else {
        match payload[0] & 0x0F {
            7 => Some("avc1".to_string()),
            12 => Some("hvc1".to_string()),
            13 => Some("av01".to_string()),
            nibble => Some(format!("legacy:{nibble:x}")),
        }
    }
}

fn detect_audio_codec(payload: &[u8]) -> Option<String> {
    if let Some(cc) = first_track_fourcc(FrameType::Audio, payload) {
        return media_fourcc_auth_label(&cc);
    }
    let mut hdr = AudioHeader::default();
    if crate::ertmp::exaudio::exaudio_parse(payload, &mut hdr).is_err() {
        return None;
    }
    if hdr.is_ex_header != 0 {
        if hdr.packet_type == ERTMP_PACKET_TYPE_MODEX {
            return None;
        }
        let mut fourcc = [0u8; 4];
        fourcc.copy_from_slice(&hdr.fourcc[..4]);
        return media_fourcc_auth_label(&fourcc);
    } else {
        match hdr.audio_codec {
            AudioCodec::Aac => Some("mp4a".to_string()),
            AudioCodec::Mp3 => Some("mp3".to_string()),
            AudioCodec::Opus => Some("Opus".to_string()),
            other => Some(format!("legacy:{other:?}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::amf::amf0;
    use crate::session::stream::Stream;

    #[test]
    fn relay_budget_counts_actual_retained_bytes() {
        let mut conn = Conn::new();
        conn.max_pending_relay_bytes = 6;

        assert!(
            conn.queue_relay_frame(FrameType::Video, 0, b"data", b"data")
                .is_ok()
        );
        assert_eq!(conn.pending_relay_bytes(), 4);
        assert!(conn.pending_relay[0].cache_payload.is_none());
        assert_eq!(conn.pending_relay[0].cache_payload(), b"data");

        assert!(
            conn.queue_relay_frame(FrameType::Video, 0, b"x", b"y")
                .is_ok()
        );
        assert_eq!(conn.pending_relay_bytes(), 6);
        assert!(
            conn.queue_relay_frame(FrameType::Video, 0, b"z", b"z")
                .is_err()
        );
    }
    #[test]
    fn relay_route_key_prefers_relay_key_over_rtmp_name() {
        let mut conn = Conn::new();
        conn.relay_key = "stream-db-id".to_string();
        conn.current_stream = Some(Box::new(Stream::new(1)));
        if let Some(ref mut stream) = conn.current_stream {
            stream.name = "pub_or_play_key".to_string();
        }
        assert_eq!(conn.relay_route_key(), "stream-db-id");
    }

    #[test]
    fn relay_route_key_falls_back_to_rtmp_stream_name() {
        let mut conn = Conn::new();
        conn.current_stream = Some(Box::new(Stream::new(1)));
        if let Some(ref mut stream) = conn.current_stream {
            stream.name = "legacy_name".to_string();
        }
        assert_eq!(conn.relay_route_key(), "legacy_name");
    }

    #[test]
    fn connect_with_caps_sets_negotiated_state_and_caps() {
        let mut conn = Conn::new();
        conn.state = ConnState::Connected;

        let mut buf = Buffer::new();
        amf0::write_string(&mut buf, "connect").unwrap();
        amf0::write_number(&mut buf, 1.0).unwrap();
        amf0::write_object_begin(&mut buf).unwrap();
        amf0::write_object_key(&mut buf, "app").unwrap();
        amf0::write_string(&mut buf, "live").unwrap();
        amf0::write_object_key(&mut buf, "fourCcList").unwrap();
        buf.write(&[0x0A, 0x00, 0x00, 0x00, 0x02]).unwrap();
        amf0::write_string(&mut buf, "av01").unwrap();
        amf0::write_string(&mut buf, "hvc1").unwrap();
        amf0::write_object_end(&mut buf).unwrap();

        conn.handle_command(buf.as_slice()).unwrap();
        assert_eq!(conn.state, ConnState::AppConnected);
        assert!(conn.negotiated_caps.has_four_cc_list);
        assert_eq!(conn.negotiated_caps.four_cc_list.count, 2);
    }

    #[test]
    fn connect_with_caps_ex_enables_multitrack() {
        let mut conn = Conn::new();
        conn.state = ConnState::Connected;

        let mut buf = Buffer::new();
        amf0::write_string(&mut buf, "connect").unwrap();
        amf0::write_number(&mut buf, 1.0).unwrap();
        amf0::write_object_begin(&mut buf).unwrap();
        amf0::write_object_key(&mut buf, "app").unwrap();
        amf0::write_string(&mut buf, "live").unwrap();
        amf0::write_object_key(&mut buf, "capsEx").unwrap();
        amf0::write_number(&mut buf, CAPS_EX_MASK_MULTITRACK as f64).unwrap();
        amf0::write_object_end(&mut buf).unwrap();

        conn.handle_command(buf.as_slice()).unwrap();
        assert_eq!(conn.state, ConnState::AppConnected);
        assert!(conn.negotiated_caps.has_caps_ex);
        assert!(conn.negotiated_caps.multitrack_enabled);
    }

    #[test]
    fn connect_parse_failure_sends_error_response() {
        let mut conn = Conn::new();
        let mut buf = Buffer::with_capacity(512);
        let long_app = "a".repeat(256);
        command::build_connect(
            &mut buf,
            &long_app,
            "rtmp://host/app",
            "",
            "",
            "FMLE/3.0",
            0,
            0,
            None,
        )
        .unwrap();
        assert_eq!(conn.handle_command(buf.as_slice()), Ok(()));
        assert!(conn.app.is_empty());
        assert_ne!(conn.state, ConnState::AppConnected);
        assert!(
            conn.send_buffer
                .peek()
                .windows(b"_error".len())
                .any(|window| window == b"_error")
        );
    }

    #[test]
    fn connect_after_app_connected_does_not_repoint_app_namespace() {
        let mut conn = Conn::new();

        let mut buf = Buffer::with_capacity(256);
        command::build_connect(
            &mut buf,
            "public",
            "rtmp://host/public",
            "",
            "",
            "FMLE/3.0",
            0,
            0,
            None,
        )
        .unwrap();
        conn.handle_command(buf.as_slice()).unwrap();
        assert_eq!(conn.app, "public");
        assert_eq!(conn.state, ConnState::AppConnected);

        // A second `connect` after the app namespace is already authorized
        // must be ignored -- it must not silently repoint `self.app` (and
        // therefore relay/cache routing) to a namespace that was never
        // passed through on_publish_cb/on_play_cb.
        let mut buf2 = Buffer::with_capacity(256);
        command::build_connect(
            &mut buf2,
            "private",
            "rtmp://host/private",
            "",
            "",
            "FMLE/3.0",
            0,
            0,
            None,
        )
        .unwrap();
        conn.handle_command(buf2.as_slice()).unwrap();
        assert_eq!(conn.app, "public");
        assert_eq!(conn.state, ConnState::AppConnected);
    }

    fn build_publish_with_raw_stream_name(stream_bytes: &[u8]) -> Buffer {
        let mut buf = Buffer::with_capacity(128);
        amf0::write_string(&mut buf, "publish").unwrap();
        amf0::write_number(&mut buf, 1.0).unwrap();
        amf0::write_null(&mut buf).unwrap();
        let len = stream_bytes.len();
        assert!(len <= u16::MAX as usize);
        buf.write(&[amf0::Amf0Type::String as u8, (len >> 8) as u8, len as u8])
            .unwrap();
        buf.write(stream_bytes).unwrap();
        amf0::write_string(&mut buf, "live").unwrap();
        buf
    }

    fn build_connect_with_raw_app(app_bytes: &[u8]) -> Buffer {
        let mut buf = Buffer::with_capacity(256);
        amf0::write_string(&mut buf, "connect").unwrap();
        amf0::write_number(&mut buf, 1.0).unwrap();
        amf0::write_object_begin(&mut buf).unwrap();
        amf0::write_object_key(&mut buf, "app").unwrap();
        let len = app_bytes.len();
        assert!(len <= u16::MAX as usize);
        buf.write(&[amf0::Amf0Type::String as u8, (len >> 8) as u8, len as u8])
            .unwrap();
        buf.write(app_bytes).unwrap();
        amf0::write_object_key(&mut buf, "tcUrl").unwrap();
        amf0::write_string(&mut buf, "rtmp://host/live").unwrap();
        amf0::write_object_end(&mut buf).unwrap();
        buf
    }

    #[test]
    fn connect_rejects_invalid_utf8_app_name() {
        let mut conn = Conn::new();
        let buf = build_connect_with_raw_app(&[0xFF, 0xFE]);
        conn.handle_command(buf.as_slice()).unwrap();
        assert_eq!(conn.app, "");
        assert_eq!(conn.state, ConnState::TcpAccepted);
    }

    #[test]
    fn publish_rejects_invalid_utf8_stream_name() {
        let mut conn = Conn::new();
        conn.app = "live".to_string();
        conn.current_stream = Some(Box::new(Stream::new(1)));
        let buf = build_publish_with_raw_stream_name(&[0x80, 0x81]);
        conn.handle_command(buf.as_slice()).unwrap();
        assert!(!conn.current_stream.as_ref().unwrap().is_publishing);
    }

    #[test]
    fn publish_rejects_play_only_connections_when_publish_cb_missing() {
        let mut conn = Conn::new();
        conn.app = "live".to_string();
        conn.current_stream = Some(Box::new(Stream::new(1)));
        conn.on_play_cb = Some(|_, _, _| true);

        let mut play = Buffer::with_capacity(128);
        command::build_play(&mut play, "viewer").unwrap();
        conn.handle_command(play.as_slice()).unwrap();
        assert!(conn.current_stream.as_ref().unwrap().is_playing);

        let mut publish = Buffer::with_capacity(128);
        command::build_publish(&mut publish, "inject", "live").unwrap();
        conn.handle_command(publish.as_slice()).unwrap();
        assert!(
            !conn.current_stream.as_ref().unwrap().is_publishing,
            "play-authorized peers must not publish when on_publish_cb is unset"
        );
    }

    #[test]
    fn play_rejects_publish_only_connections_when_play_cb_missing() {
        let mut conn = Conn::new();
        conn.app = "live".to_string();
        conn.current_stream = Some(Box::new(Stream::new(1)));
        conn.on_publish_cb = Some(|_, _, _| true);

        let mut play = Buffer::with_capacity(128);
        command::build_play(&mut play, "viewer").unwrap();
        conn.handle_command(play.as_slice()).unwrap();
        assert!(
            !conn.current_stream.as_ref().unwrap().is_playing,
            "publish-authorized peers must not play when on_play_cb is unset"
        );
        assert!(
            !conn.relay_enabled,
            "relay must stay disabled when play is rejected on a publish-only server"
        );
    }

    #[test]
    fn publish_rejects_media_only_connections_when_publish_cb_missing() {
        let mut conn = Conn::new();
        conn.app = "live".to_string();
        conn.current_stream = Some(Box::new(Stream::new(1)));
        conn.on_media_cb = Some(|_, _, _| true);

        let mut publish = Buffer::with_capacity(128);
        command::build_publish(&mut publish, "inject", "live").unwrap();
        conn.handle_command(publish.as_slice()).unwrap();
        assert!(
            !conn.current_stream.as_ref().unwrap().is_publishing,
            "media-filtered servers must not accept publish without on_publish_cb"
        );
        assert!(
            !conn.relay_enabled,
            "relay must stay disabled when publish is rejected on a media-only server"
        );
    }

    #[test]
    fn play_rejects_media_only_connections_when_play_cb_missing() {
        let mut conn = Conn::new();
        conn.app = "live".to_string();
        conn.current_stream = Some(Box::new(Stream::new(1)));
        conn.on_media_cb = Some(|_, _, _| true);

        let mut play = Buffer::with_capacity(128);
        command::build_play(&mut play, "viewer").unwrap();
        conn.handle_command(play.as_slice()).unwrap();
        assert!(
            !conn.current_stream.as_ref().unwrap().is_playing,
            "media-filtered servers must not accept play without on_play_cb"
        );
        assert!(
            !conn.relay_enabled,
            "relay must stay disabled when play is rejected on a media-only server"
        );
    }

    #[test]
    fn publish_rejects_frame_only_connections_when_publish_cb_missing() {
        let mut conn = Conn::new();
        conn.app = "live".to_string();
        conn.current_stream = Some(Box::new(Stream::new(1)));
        conn.on_frame_cb = Some(|_| {});

        let mut publish = Buffer::with_capacity(128);
        command::build_publish(&mut publish, "inject", "live").unwrap();
        conn.handle_command(publish.as_slice()).unwrap();
        assert!(
            !conn.current_stream.as_ref().unwrap().is_publishing,
            "frame-observing servers must not accept publish without on_publish_cb"
        );
        assert!(
            !conn.relay_enabled,
            "relay must stay disabled when publish is rejected on a frame-only server"
        );
    }

    #[test]
    fn play_rejects_frame_only_connections_when_play_cb_missing() {
        let mut conn = Conn::new();
        conn.app = "live".to_string();
        conn.current_stream = Some(Box::new(Stream::new(1)));
        conn.on_frame_cb = Some(|_| {});

        let mut play = Buffer::with_capacity(128);
        command::build_play(&mut play, "viewer").unwrap();
        conn.handle_command(play.as_slice()).unwrap();
        assert!(
            !conn.current_stream.as_ref().unwrap().is_playing,
            "frame-observing servers must not accept play without on_play_cb"
        );
        assert!(
            !conn.relay_enabled,
            "relay must stay disabled when play is rejected on a frame-only server"
        );
    }

    #[test]
    fn publish_rejects_shared_object_auth_only_connections_when_publish_cb_missing() {
        use crate::message::shared_object::{SharedObjectEvent, SharedObjectEventType};

        fn allow_so(_conn_id: u64, _so: &SharedObjectMessage) -> bool {
            true
        }

        let mut conn = Conn::new();
        conn.app = "live".to_string();
        conn.current_stream = Some(Box::new(Stream::new(1)));
        conn.on_shared_object_auth_cb = Some(allow_so);

        let mut publish = Buffer::with_capacity(128);
        command::build_publish(&mut publish, "inject", "live").unwrap();
        conn.handle_command(publish.as_slice()).unwrap();
        assert!(
            !conn.current_stream.as_ref().unwrap().is_publishing,
            "shared-object-auth servers must not accept publish without on_publish_cb"
        );
        assert!(
            !conn.relay_enabled,
            "relay must stay disabled when publish is rejected on a shared-object-auth server"
        );

        // Sanity: shared-object auth still applies to AMF3 shared objects.
        conn.on_shared_object_cb = Some(|_, _| {});
        let so = SharedObjectMessage {
            name: "chat".to_string(),
            version: 1,
            flags: 0,
            events: vec![SharedObjectEvent {
                event_type: SharedObjectEventType::Use,
                data: Vec::new(),
            }],
        };
        let mut amf_buf = Buffer::with_capacity(64);
        shared_object::write(&so, &mut amf_buf).unwrap();
        let mut payload = vec![0x00];
        payload.extend_from_slice(amf_buf.as_slice());
        let msg = ChunkMessage {
            csid: 3,
            fmt: 0,
            timestamp: 0,
            msg_length: payload.len() as u32,
            msg_type_id: msg_dispatch::RTMP_MSG_AMF3_SHARED_OBJECT,
            msg_stream_id: 0,
            is_complete: true,
        };
        conn.handle_message_for_test(&msg, &payload).unwrap();
    }

    #[test]
    fn play_rejects_shared_object_auth_only_connections_when_play_cb_missing() {
        fn allow_so(_conn_id: u64, _so: &SharedObjectMessage) -> bool {
            true
        }

        let mut conn = Conn::new();
        conn.app = "live".to_string();
        conn.current_stream = Some(Box::new(Stream::new(1)));
        conn.on_shared_object_auth_cb = Some(allow_so);

        let mut play = Buffer::with_capacity(128);
        command::build_play(&mut play, "viewer").unwrap();
        conn.handle_command(play.as_slice()).unwrap();
        assert!(
            !conn.current_stream.as_ref().unwrap().is_playing,
            "shared-object-auth servers must not accept play without on_play_cb"
        );
        assert!(
            !conn.relay_enabled,
            "relay must stay disabled when play is rejected on a shared-object-auth server"
        );
    }

    #[test]
    fn publish_rejects_release_stream_only_connections_when_publish_cb_missing() {
        fn allow_release(_conn_id: u64, _app: &str, _stream: &str) -> bool {
            true
        }

        let mut conn = Conn::new();
        conn.app = "live".to_string();
        conn.current_stream = Some(Box::new(Stream::new(1)));
        conn.on_release_stream_cb = Some(allow_release);

        let mut publish = Buffer::with_capacity(128);
        command::build_publish(&mut publish, "inject", "live").unwrap();
        conn.handle_command(publish.as_slice()).unwrap();
        assert!(
            !conn.current_stream.as_ref().unwrap().is_publishing,
            "release-stream servers must not accept publish without on_publish_cb"
        );
        assert!(
            !conn.relay_enabled,
            "relay must stay disabled when publish is rejected on a release-stream server"
        );
    }

    #[test]
    fn play_rejects_release_stream_only_connections_when_play_cb_missing() {
        fn allow_release(_conn_id: u64, _app: &str, _stream: &str) -> bool {
            true
        }

        let mut conn = Conn::new();
        conn.app = "live".to_string();
        conn.current_stream = Some(Box::new(Stream::new(1)));
        conn.on_release_stream_cb = Some(allow_release);

        let mut play = Buffer::with_capacity(128);
        command::build_play(&mut play, "viewer").unwrap();
        conn.handle_command(play.as_slice()).unwrap();
        assert!(
            !conn.current_stream.as_ref().unwrap().is_playing,
            "release-stream servers must not accept play without on_play_cb"
        );
        assert!(
            !conn.relay_enabled,
            "relay must stay disabled when play is rejected on a release-stream server"
        );
    }

    #[test]
    fn defer_media_relay_keeps_relay_disabled_without_publish_cb() {
        let mut conn = Conn::new();
        conn.app = "live".to_string();
        conn.current_stream = Some(Box::new(Stream::new(1)));
        conn.defer_media_relay = true;

        let mut publish = Buffer::with_capacity(128);
        command::build_publish(&mut publish, "stream", "live").unwrap();
        conn.handle_command(publish.as_slice()).unwrap();

        assert!(conn.current_stream.as_ref().unwrap().is_publishing);
        assert!(
            !conn.relay_enabled,
            "defer_media_relay must hold relay off until the integrator enables it"
        );
    }

    #[test]
    fn create_stream_clears_relay_enabled_under_defer_media_relay() {
        let mut conn = Conn::new();
        conn.app = "live".to_string();
        conn.state = ConnState::AppConnected;
        conn.defer_media_relay = true;
        conn.current_stream = Some(Box::new(Stream::new(1)));
        if let Some(stream) = conn.current_stream.as_mut() {
            stream.is_publishing = true;
        }
        conn.relay_enabled = true;

        let mut create = Buffer::with_capacity(128);
        command::build_create_stream(&mut create, 2.0).unwrap();
        conn.handle_command(create.as_slice()).unwrap();

        assert!(
            !conn.relay_enabled,
            "createStream must clear relay_enabled so defer_media_relay requires re-authorization"
        );

        let mut publish = Buffer::with_capacity(128);
        command::build_publish(&mut publish, "stream2", "live").unwrap();
        conn.handle_command(publish.as_slice()).unwrap();

        assert!(
            !conn.relay_enabled,
            "publish under defer_media_relay must not re-enable relay without integrator approval"
        );
    }

    #[test]
    fn invalid_utf8_stream_names_do_not_collide_on_empty_relay_namespace() {
        let mut first = Conn::new();
        first.app = "live".to_string();
        first.current_stream = Some(Box::new(Stream::new(1)));
        let buf = build_publish_with_raw_stream_name(&[0x80]);
        first.handle_command(buf.as_slice()).unwrap();
        assert!(!first.current_stream.as_ref().unwrap().is_publishing);

        let mut second = Conn::new();
        second.app = "live".to_string();
        second.current_stream = Some(Box::new(Stream::new(1)));
        let buf = build_publish_with_raw_stream_name(&[0x81]);
        second.handle_command(buf.as_slice()).unwrap();
        assert!(!second.current_stream.as_ref().unwrap().is_publishing);
    }

    #[test]
    fn apply_chunk_size_updates_outbound_and_inbound() {
        let mut conn = Conn::new();
        conn.apply_chunk_size(4096);
        assert_eq!(conn.chunk_size, 4096);
        assert_eq!(conn.active_chunk_size, 4096);
        assert_eq!(conn.chunk_reg.default_chunk_size, 4096);
    }

    #[test]
    fn new_connection_starts_at_rtmp_default_chunk_size() {
        let conn = Conn::new();
        assert_eq!(conn.chunk_size, DEFAULT_CHUNK_SIZE);
        assert_eq!(conn.active_chunk_size, DEFAULT_CHUNK_SIZE);
        assert_eq!(conn.chunk_reg.default_chunk_size, DEFAULT_CHUNK_SIZE);
    }

    #[test]
    fn enhanced_av1_media_frames_accepted_while_publishing() {
        let mut conn = Conn::new();
        conn.relay_enabled = true;
        conn.current_stream = Some(Box::new(Stream::new(1)));
        if let Some(s) = conn.current_stream.as_mut() {
            s.is_publishing = true;
        }

        let av1_seq = vec![0x90, b'a', b'v', b'0', b'1', 0x01, 0x02, 0x03];
        assert!(
            conn.handle_media_frame(1, FrameType::Video, 0, &av1_seq, None)
                .is_ok()
        );

        let aac_seq = vec![0xAF, 0x00, 0x12, 0x10];
        assert!(
            conn.handle_media_frame(1, FrameType::Audio, 0, &aac_seq, None)
                .is_ok()
        );

        let av1_frame = vec![0x91, b'a', b'v', b'0', b'1', 0xDE, 0xAD, 0xBE, 0xEF];
        assert!(
            conn.handle_media_frame(1, FrameType::Video, 40, &av1_frame, None)
                .is_ok()
        );
    }

    #[test]
    fn publish_rename_with_relay_key_does_not_evict_stale_rtmp_name() {
        let mut conn = Conn::new();
        conn.app = "live".to_string();
        conn.relay_key = "route-1".to_string();
        conn.current_stream = Some(Box::new(Stream::new(1)));

        let mut buf = Buffer::with_capacity(128);
        command::build_publish(&mut buf, "A", "live").unwrap();
        conn.handle_command(buf.as_slice()).unwrap();
        assert!(conn.pending_cache_evictions.is_empty());
        assert_eq!(conn.current_stream.as_ref().unwrap().name, "A");

        let mut buf = Buffer::with_capacity(128);
        command::build_publish(&mut buf, "B", "live").unwrap();
        conn.handle_command(buf.as_slice()).unwrap();

        // relay_route_key() is pinned to relay_key, so republishing under a new
        // RTMP stream name must NOT queue an eviction for the stale RTMP name
        // ("A") -- the real cache key ("route-1") never changed.
        assert!(conn.pending_cache_evictions.is_empty());
        assert_eq!(conn.current_stream.as_ref().unwrap().name, "B");
    }

    #[test]
    fn publish_rename_without_relay_key_evicts_old_route_key() {
        let mut conn = Conn::new();
        conn.app = "live".to_string();
        conn.current_stream = Some(Box::new(Stream::new(1)));

        let mut buf = Buffer::with_capacity(128);
        command::build_publish(&mut buf, "A", "live").unwrap();
        conn.handle_command(buf.as_slice()).unwrap();
        assert!(conn.pending_cache_evictions.is_empty());

        let mut buf = Buffer::with_capacity(128);
        command::build_publish(&mut buf, "B", "live").unwrap();
        conn.handle_command(buf.as_slice()).unwrap();

        assert_eq!(
            conn.pending_cache_evictions,
            vec![("live".to_string(), "A".to_string())]
        );
    }

    #[test]
    fn play_then_publish_does_not_evict_foreign_cache_key() {
        let mut conn = Conn::new();
        conn.app = "live".to_string();
        conn.current_stream = Some(Box::new(Stream::new(1)));

        let mut buf = Buffer::with_capacity(128);
        command::build_play(&mut buf, "victim").unwrap();
        conn.handle_command(buf.as_slice()).unwrap();
        assert_eq!(conn.current_stream.as_ref().unwrap().name, "victim");
        assert!(!conn.current_stream.as_ref().unwrap().is_publishing);

        let mut buf = Buffer::with_capacity(128);
        command::build_publish(&mut buf, "other", "live").unwrap();
        conn.handle_command(buf.as_slice()).unwrap();

        // This connection only ever played "victim" -- it never published it
        // -- so publishing "other" afterwards must not queue an eviction for
        // a cache key this connection never created.
        assert!(conn.pending_cache_evictions.is_empty());
        assert_eq!(conn.current_stream.as_ref().unwrap().name, "other");
        assert!(!conn.current_stream.as_ref().unwrap().is_playing);
    }

    #[test]
    fn evict_active_publish_route_evicts_both_claimed_and_pinned_relay_key() {
        let mut conn = Conn::new();
        conn.app = "live".to_string();
        conn.current_stream = Some(Box::new(Stream::new(1)));

        // Publish claims the route under the RTMP name (no relay_key set
        // yet), so frames get cached under "A".
        let mut buf = Buffer::with_capacity(128);
        command::build_publish(&mut buf, "A", "live").unwrap();
        conn.handle_command(buf.as_slice()).unwrap();
        assert!(conn.pending_cache_evictions.is_empty());

        // Integrator pins relay_key mid-publish, after the route was
        // already claimed under "A". New frames from this point on are
        // cached under "route-pinned" (relay_route_key() is live at
        // frame-queue time), diverging from the claimed route key.
        conn.relay_key = "route-pinned".to_string();

        // `play` supersedes the active publish, evicting whatever was
        // being served. Both the originally claimed key ("A") and the
        // now-current relay route key ("route-pinned") must be evicted,
        // since cached frames may exist under either.
        let mut buf = Buffer::with_capacity(128);
        command::build_play(&mut buf, "victim").unwrap();
        conn.handle_command(buf.as_slice()).unwrap();

        assert_eq!(
            conn.pending_cache_evictions,
            vec![
                ("live".to_string(), "A".to_string()),
                ("live".to_string(), "route-pinned".to_string()),
            ]
        );
    }

    #[test]
    fn publish_then_play_then_publish_does_not_evict_played_stream_key() {
        let mut conn = Conn::new();
        conn.app = "live".to_string();
        conn.current_stream = Some(Box::new(Stream::new(1)));

        let mut buf = Buffer::with_capacity(128);
        command::build_publish(&mut buf, "A", "live").unwrap();
        conn.handle_command(buf.as_slice()).unwrap();
        assert!(conn.pending_cache_evictions.is_empty());

        // `play` supersedes the active publish of "A" on this stream, so it
        // must evict "A" itself right away (there's no future publish
        // rename to hang that eviction off of, and is_publishing is cleared
        // so this stream can no longer be mistaken for an active publisher
        // of "A").
        let mut buf = Buffer::with_capacity(128);
        command::build_play(&mut buf, "victim").unwrap();
        conn.handle_command(buf.as_slice()).unwrap();
        assert_eq!(conn.current_stream.as_ref().unwrap().name, "victim");
        assert!(!conn.current_stream.as_ref().unwrap().is_publishing);
        assert_eq!(
            conn.pending_cache_evictions,
            vec![("live".to_string(), "A".to_string())]
        );
        conn.pending_cache_evictions.clear();

        let mut buf = Buffer::with_capacity(128);
        command::build_publish(&mut buf, "other", "live").unwrap();
        conn.handle_command(buf.as_slice()).unwrap();

        // This connection was never actively publishing "victim" -- it was
        // only playing it -- so publishing "other" must not queue a second
        // eviction for a cache key it never owned.
        assert!(conn.pending_cache_evictions.is_empty());
        assert_eq!(conn.current_stream.as_ref().unwrap().name, "other");
    }

    #[test]
    fn create_stream_evicts_active_publish_route() {
        let mut conn = Conn::new();
        conn.app = "live".to_string();
        conn.state = ConnState::AppConnected;
        conn.current_stream = Some(Box::new(Stream::new(1)));

        let mut buf = Buffer::with_capacity(128);
        command::build_publish(&mut buf, "A", "live").unwrap();
        conn.handle_command(buf.as_slice()).unwrap();
        assert!(conn.pending_cache_evictions.is_empty());
        assert!(conn.current_stream.as_ref().unwrap().is_publishing);

        // A fresh createStream replaces current_stream outright, abandoning
        // the active publish of "A" with no future rename to hang an
        // eviction off of -- it must be evicted immediately here, not left
        // as a dangling publisher_cache_keys tracking entry until this
        // connection eventually disconnects.
        let mut buf = Buffer::with_capacity(128);
        command::build_create_stream(&mut buf, 4.0).unwrap();
        conn.handle_command(buf.as_slice()).unwrap();

        assert_eq!(
            conn.pending_cache_evictions,
            vec![("live".to_string(), "A".to_string())]
        );
        assert!(!conn.current_stream.as_ref().unwrap().is_publishing);
        assert_eq!(conn.current_stream.as_ref().unwrap().name, "");
    }

    #[test]
    fn handle_control_peer_set_chunk_size_updates_inbound_only() {
        let mut conn = Conn::new();
        conn.chunk_size = 4096;
        conn.handle_control(
            msg_dispatch::RTMP_MSG_SET_CHUNK_SIZE,
            &8192u32.to_be_bytes(),
        )
        .unwrap();
        assert_eq!(conn.chunk_size, 4096);
        assert_eq!(conn.active_chunk_size, DEFAULT_CHUNK_SIZE);
        assert_eq!(conn.chunk_reg.default_chunk_size, 8192);
    }

    #[test]
    fn inbound_ping_requests_are_rate_limited() {
        let mut conn = Conn::new();
        conn.client_fd = 0;
        conn.transport = None;
        let mut ping = |token: u32| {
            let mut buf = Buffer::with_capacity(6);
            control::write_user_control_ping_request(&mut buf, token).unwrap();
            conn.handle_user_control(buf.as_slice())
        };
        for token in 0..8 {
            assert!(ping(token).is_ok(), "token {token} should be accepted");
        }
        assert!(
            matches!(ping(99), Err(ErrorCode::Protocol)),
            "9th ping in one second must be rejected"
        );
    }

    #[test]
    fn post_connect_idle_without_publish_or_play_times_out() {
        use std::time::{Duration, Instant};

        let mut conn = Conn::new();
        conn.state = ConnState::AppConnected;
        conn.set_session_setup_started_for_test(Instant::now() - Duration::from_secs(11));
        assert!(
            conn.session_setup_timed_out(),
            "AppConnected peers that never publish/play must be reaped"
        );

        let mut publishing = Conn::new();
        publishing.state = ConnState::Publishing;
        publishing.current_stream = Some(Box::new(Stream {
            is_publishing: true,
            ..Stream::new(1)
        }));
        publishing.media_bytes_received = 1;
        publishing.set_session_setup_started_for_test(Instant::now() - Duration::from_secs(11));
        assert!(
            !publishing.session_setup_timed_out(),
            "publishers that have sent media must not be reaped by the setup timer"
        );

        let mut injected_only = Conn::new();
        injected_only.state = ConnState::Publishing;
        injected_only.app = "live".to_string();
        injected_only.current_stream = Some(Box::new(Stream {
            is_publishing: true,
            name: "cam".to_string(),
            ..Stream::new(1)
        }));
        injected_only
            .inject_relay_frame(FrameType::Video, 0, &[0x17, 0x01, 0xAA])
            .unwrap();
        injected_only.set_session_setup_started_for_test(Instant::now() - Duration::from_secs(11));
        assert_eq!(injected_only.media_bytes_received, 0);
        assert!(injected_only.injected_media_bytes > 0);
        assert!(
            !injected_only.session_setup_timed_out(),
            "trusted inject must count toward publisher liveness"
        );

        let mut script_only = Conn::new();
        script_only.state = ConnState::Publishing;
        script_only.app = "live".to_string();
        script_only.defer_media_relay = true;
        script_only.relay_enabled = false;
        script_only.current_stream = Some(Box::new(Stream {
            is_publishing: true,
            name: "meta".to_string(),
            ..Stream::new(1)
        }));
        let script = b"@setDataFrame";
        script_only
            .inject_relay_frame(FrameType::Script, 0, script)
            .unwrap();
        assert_eq!(
            script_only.injected_media_bytes,
            script.len() as u64,
            "script inject must count toward deferred-relay drain liveness"
        );
        assert_eq!(script_only.pending_relay.len(), 1);

        // Injected bytes from a prior epoch must not exempt a later empty publish.
        injected_only.evict_active_publish_route();
        injected_only.current_stream = Some(Box::new(Stream {
            is_publishing: true,
            ..Stream::new(2)
        }));
        injected_only.set_session_setup_started_for_test(Instant::now() - Duration::from_secs(3));
        assert_eq!(injected_only.injected_media_bytes, 0);
        assert!(
            injected_only.session_setup_timed_out(),
            "new publish epoch without media must still time out"
        );

        // Idle inject (no publish role) must also clear on createStream/unpublish
        // paths that call evict while !was_publishing.
        let mut idle_inject = Conn::new();
        idle_inject.state = ConnState::StreamCreated;
        idle_inject.app = "live".to_string();
        idle_inject.current_stream = Some(Box::new(Stream {
            name: "cam".to_string(),
            ..Stream::new(1)
        }));
        idle_inject
            .inject_relay_frame(FrameType::Video, 0, &[0x17, 0x01, 0xAA])
            .unwrap();
        assert!(idle_inject.injected_media_bytes > 0);
        idle_inject.evict_active_publish_route();
        assert!(
            idle_inject.injected_media_bytes == 0,
            "idle inject must not carry into a later publish epoch"
        );

        // Active players must not claim a publish route via Conn inject.
        let mut playing = Conn::new();
        playing.app = "live".to_string();
        playing.current_stream = Some(Box::new(Stream {
            name: "cam".to_string(),
            is_playing: true,
            ..Stream::new(1)
        }));
        assert!(
            playing
                .inject_relay_frame(FrameType::Video, 0, &[0x17, 0x01, 0xAA])
                .is_err(),
            "playing connections must not squat publish routes via inject"
        );

        let mut squatting = Conn::new();
        squatting.state = ConnState::Publishing;
        squatting.current_stream = Some(Box::new(Stream {
            is_publishing: true,
            ..Stream::new(1)
        }));
        squatting.set_session_setup_started_for_test(Instant::now() - Duration::from_secs(3));
        assert!(
            squatting.session_setup_timed_out(),
            "publishers that never send media must be reaped so they cannot squat routes"
        );

        // A peer that published and then unpublished before the deadline
        // must not be exempted forever just because `ConnState` only moves
        // forward -- the exemption must track the *current* is_publishing
        // flag, not the monotonic state.
        let mut unpublished = Conn::new();
        unpublished.state = ConnState::Publishing;
        unpublished.current_stream = Some(Box::new(Stream::new(1)));
        unpublished.set_session_setup_started_for_test(Instant::now() - Duration::from_secs(11));
        assert!(
            unpublished.session_setup_timed_out(),
            "peers that unpublished before the deadline must be reaped"
        );

        let mut paused_player = Conn::new();
        paused_player.state = ConnState::Playing;
        paused_player.current_stream = Some(Box::new(Stream {
            is_playing: true,
            paused: true,
            ..Stream::new(1)
        }));
        paused_player.set_session_setup_started_for_test(Instant::now() - Duration::from_secs(11));
        assert!(
            paused_player.session_setup_timed_out(),
            "paused players must be reaped after the setup grace window"
        );

        let mut active_player = Conn::new();
        active_player.state = ConnState::Playing;
        active_player.current_stream = Some(Box::new(Stream {
            is_playing: true,
            paused: false,
            ..Stream::new(1)
        }));
        active_player.set_session_setup_started_for_test(Instant::now() - Duration::from_secs(11));
        assert!(
            active_player.session_setup_timed_out(),
            "unpaused players with no outbound relay must be reaped after the setup grace window"
        );

        let mut receiving_player = Conn::new();
        receiving_player.state = ConnState::Playing;
        receiving_player.current_stream = Some(Box::new(Stream {
            is_playing: true,
            paused: false,
            ..Stream::new(1)
        }));
        receiving_player.media_bytes_sent = 1;
        receiving_player
            .set_session_setup_started_for_test(Instant::now() - Duration::from_secs(11));
        assert!(
            !receiving_player.session_setup_timed_out(),
            "unpaused players receiving relay must not be reaped by the setup timer"
        );
    }

    #[test]
    fn pause_unpause_cycling_cannot_reset_setup_timer_without_relay() {
        use std::time::{Duration, Instant};

        let mut conn = Conn::new();
        conn.app = "live".to_string();
        conn.state = ConnState::Playing;
        conn.current_stream = Some(Box::new(Stream {
            is_playing: true,
            paused: false,
            ..Stream::new(1)
        }));
        conn.set_session_setup_started_for_test(Instant::now() - Duration::from_secs(11));

        let mut pause = Buffer::with_capacity(64);
        command::build_pause(&mut pause, true).unwrap();
        conn.handle_command(pause.as_slice()).unwrap();
        assert!(
            conn.session_setup_timed_out(),
            "pausing a viewer that never received relay must not refresh the setup timer"
        );

        let mut receiving = Conn::new();
        receiving.app = "live".to_string();
        receiving.state = ConnState::Playing;
        receiving.media_bytes_sent = 1;
        receiving.current_stream = Some(Box::new(Stream {
            is_playing: true,
            paused: false,
            ..Stream::new(1)
        }));
        receiving.set_session_setup_started_for_test(Instant::now() - Duration::from_secs(11));

        let mut pause = Buffer::with_capacity(64);
        command::build_pause(&mut pause, true).unwrap();
        receiving.handle_command(pause.as_slice()).unwrap();
        assert!(
            !receiving.session_setup_timed_out(),
            "pausing a viewer that is receiving relay must still refresh the setup timer"
        );

        let mut unpause = Buffer::with_capacity(64);
        command::build_pause(&mut unpause, false).unwrap();
        receiving.handle_command(unpause.as_slice()).unwrap();
        receiving.set_session_setup_started_for_test(Instant::now() - Duration::from_secs(11));

        let mut stale_pause = Buffer::with_capacity(64);
        command::build_pause(&mut stale_pause, true).unwrap();
        receiving.handle_command(stale_pause.as_slice()).unwrap();
        assert!(
            receiving.session_setup_timed_out(),
            "historical relay bytes must not refresh pause grace more than once"
        );

        let mut unpause = Buffer::with_capacity(64);
        command::build_pause(&mut unpause, false).unwrap();
        receiving.handle_command(unpause.as_slice()).unwrap();
        receiving.media_bytes_sent = 2;
        receiving.set_session_setup_started_for_test(Instant::now() - Duration::from_secs(11));

        let mut fresh_pause = Buffer::with_capacity(64);
        command::build_pause(&mut fresh_pause, true).unwrap();
        receiving.handle_command(fresh_pause.as_slice()).unwrap();
        assert!(
            !receiving.session_setup_timed_out(),
            "fresh relay activity must allow another pause grace window"
        );
    }

    #[test]
    fn play_route_change_does_not_reuse_historical_relay_for_pause_grace() {
        use std::time::{Duration, Instant};

        let mut conn = Conn::new();
        conn.app = "live".to_string();
        conn.state = ConnState::Playing;
        conn.media_bytes_sent = 1;
        conn.current_stream = Some(Box::new(Stream {
            is_playing: true,
            paused: false,
            name: "live-route".to_string(),
            ..Stream::new(1)
        }));

        let mut play = Buffer::with_capacity(128);
        command::build_play(&mut play, "dead-route").unwrap();
        conn.handle_command(play.as_slice()).unwrap();
        conn.set_session_setup_started_for_test(Instant::now() - Duration::from_secs(11));

        let mut pause = Buffer::with_capacity(64);
        command::build_pause(&mut pause, true).unwrap();
        conn.handle_command(pause.as_slice()).unwrap();
        assert!(
            conn.session_setup_timed_out(),
            "relay bytes from a previous play route must not qualify a dead route for pause grace"
        );
    }

    #[test]
    fn repeated_publish_same_route_does_not_refresh_media_deadline() {
        use std::time::{Duration, Instant};

        let mut conn = Conn::new();
        conn.app = "live".to_string();
        conn.current_stream = Some(Box::new(Stream::new(1)));

        let mut buf = Buffer::with_capacity(128);
        command::build_publish(&mut buf, "room", "live").unwrap();
        conn.handle_command(buf.as_slice()).unwrap();
        assert!(conn.current_stream.as_ref().unwrap().is_publishing);

        conn.set_session_setup_started_for_test(Instant::now() - Duration::from_secs(3));

        let mut buf = Buffer::with_capacity(128);
        command::build_publish(&mut buf, "room", "live").unwrap();
        conn.handle_command(buf.as_slice()).unwrap();

        assert!(
            conn.session_setup_timed_out(),
            "repeating publish for the already-owned route must not refresh the media deadline"
        );
    }

    #[test]
    fn unpublish_grants_a_fresh_setup_timeout_window() {
        use std::time::{Duration, Instant};

        // Regression test: a client that has been legitimately publishing
        // for a long time (well past RTMP_SESSION_SETUP_TIMEOUT) must not
        // be reaped the instant it unpublishes -- it should get a fresh
        // grace window to republish on the same connection, not be judged
        // against however little time is left of its original setup
        // window (which may already be long gone).
        let mut conn = Conn::new();
        conn.app = "live".to_string();
        conn.current_stream = Some(Box::new(Stream::new(1)));

        let mut buf = Buffer::with_capacity(128);
        command::build_publish(&mut buf, "A", "live").unwrap();
        conn.handle_command(buf.as_slice()).unwrap();
        assert!(conn.current_stream.as_ref().unwrap().is_publishing);
        conn.media_bytes_received = 1;

        // Simulate having been connected/publishing for a long time already.
        conn.set_session_setup_started_for_test(Instant::now() - Duration::from_secs(3600));
        assert!(
            !conn.session_setup_timed_out(),
            "an active publisher must not be reaped regardless of connection age"
        );

        let mut buf = Buffer::with_capacity(128);
        command::build_fcunpublish(&mut buf, "A").unwrap();
        conn.handle_command(buf.as_slice()).unwrap();
        assert!(!conn.current_stream.as_ref().unwrap().is_publishing);

        assert!(
            !conn.session_setup_timed_out(),
            "unpublishing must grant a fresh setup-timeout window, not reap \
             immediately using up whatever remained of the original window"
        );
    }

    #[test]
    fn play_teardown_cycling_cannot_reset_setup_timer_without_relay() {
        use std::time::{Duration, Instant};

        let mut conn = Conn::new();
        conn.app = "live".to_string();
        conn.current_stream = Some(Box::new(Stream::new(1)));
        conn.set_session_setup_started_for_test(Instant::now() - Duration::from_secs(11));

        let mut play = Buffer::with_capacity(128);
        command::build_play(&mut play, "room").unwrap();
        conn.handle_command(play.as_slice()).unwrap();
        assert!(conn.current_stream.as_ref().unwrap().is_playing);

        let mut teardown = Buffer::with_capacity(128);
        command::build_deletestream(&mut teardown, 2.0, 1).unwrap();
        conn.handle_command(teardown.as_slice()).unwrap();
        assert!(
            conn.session_setup_timed_out(),
            "teardown of a viewer that never received relay must not refresh the setup timer"
        );

        conn.set_session_setup_started_for_test(Instant::now() - Duration::from_secs(9));
        conn.current_stream.as_mut().unwrap().is_playing = true;

        let mut teardown = Buffer::with_capacity(128);
        command::build_deletestream(&mut teardown, 2.0, 1).unwrap();
        conn.handle_command(teardown.as_slice()).unwrap();

        let mut play = Buffer::with_capacity(128);
        command::build_play(&mut play, "room").unwrap();
        conn.handle_command(play.as_slice()).unwrap();

        conn.set_session_setup_started_for_test(Instant::now() - Duration::from_secs(11));
        assert!(
            conn.session_setup_timed_out(),
            "play/teardown cycling without relay must not hold slots indefinitely"
        );
    }

    #[test]
    fn teardown_commands_do_not_refresh_timer_for_a_never_active_peer() {
        use std::time::{Duration, Instant};

        // Regression test: a peer that never published/played must not be
        // able to keep dodging the setup-timeout reaper forever by
        // repeatedly sending FCUnpublish/deleteStream/closeStream --
        // resetting session_setup_started must be conditioned on a genuine
        // active->idle transition, not fire on every teardown command.
        let mut conn = Conn::new();
        conn.app = "live".to_string();
        conn.current_stream = Some(Box::new(Stream::new(1)));
        conn.set_session_setup_started_for_test(Instant::now() - Duration::from_secs(11));

        let mut buf = Buffer::with_capacity(128);
        command::build_fcunpublish(&mut buf, "A").unwrap();
        conn.handle_command(buf.as_slice()).unwrap();
        assert!(
            conn.session_setup_timed_out(),
            "FCUnpublish on a stream that was never publishing must not reset the timer"
        );

        conn.set_session_setup_started_for_test(Instant::now() - Duration::from_secs(11));
        let mut buf = Buffer::with_capacity(128);
        command::build_deletestream(&mut buf, 2.0, 1).unwrap();
        conn.handle_command(buf.as_slice()).unwrap();
        assert!(
            conn.session_setup_timed_out(),
            "deleteStream on a stream that was never active must not reset the timer"
        );

        conn.set_session_setup_started_for_test(Instant::now() - Duration::from_secs(11));
        let mut buf = Buffer::with_capacity(128);
        crate::amf::amf0::write_string(&mut buf, "closeStream").unwrap();
        crate::amf::amf0::write_number(&mut buf, 2.0).unwrap();
        crate::amf::amf0::write_null(&mut buf).unwrap();
        conn.handle_command(buf.as_slice()).unwrap();
        assert!(
            conn.session_setup_timed_out(),
            "closeStream on a stream that was never active must not reset the timer"
        );
    }

    #[test]
    fn delete_stream_clears_is_playing_and_grants_a_fresh_window() {
        use std::time::{Duration, Instant};

        // Regression test: deleteStream is sent by players, not just
        // publishers. The FCUnpublish/deleteStream arm must clear
        // is_playing too (not just is_publishing), otherwise a player that
        // tears down via deleteStream would never be considered idle and
        // could hold its slot forever.
        let mut conn = Conn::new();
        conn.app = "live".to_string();
        // Model a viewer that has actually received relay; squatters with
        // zero outbound relay are reaped separately (see
        // `session_setup_timed_out`).
        conn.media_bytes_sent = 1;
        conn.current_stream = Some(Box::new(Stream {
            is_playing: true,
            ..Stream::new(1)
        }));
        conn.set_session_setup_started_for_test(Instant::now() - Duration::from_secs(3600));
        assert!(
            !conn.session_setup_timed_out(),
            "an active player must not be reaped regardless of connection age"
        );

        let mut buf = Buffer::with_capacity(128);
        command::build_deletestream(&mut buf, 2.0, 1).unwrap();
        conn.handle_command(buf.as_slice()).unwrap();

        assert!(
            !conn.current_stream.as_ref().unwrap().is_playing,
            "deleteStream must clear is_playing"
        );
        assert!(
            !conn.session_setup_timed_out(),
            "tearing down an active player via deleteStream must grant a fresh setup-timeout \
             window, not reap immediately"
        );
    }

    #[test]
    fn delete_stream_clears_paused_flag() {
        // Regression test: deleteStream must reset `paused` like
        // closeStream does. play() never resets `paused` on its own, and
        // relay delivery is gated on !paused, so a pause -> deleteStream ->
        // play sequence on the same connection would otherwise leave
        // playback silently stuck with no relayed frames.
        let mut conn = Conn::new();
        conn.app = "live".to_string();
        conn.current_stream = Some(Box::new(Stream {
            is_playing: true,
            paused: true,
            ..Stream::new(1)
        }));

        let mut buf = Buffer::with_capacity(128);
        command::build_deletestream(&mut buf, 2.0, 1).unwrap();
        conn.handle_command(buf.as_slice()).unwrap();

        assert!(
            !conn.current_stream.as_ref().unwrap().paused,
            "deleteStream must clear paused"
        );
    }

    #[test]
    fn unanswered_ping_timeouts_close_connection() {
        let mut conn = Conn::new();
        conn.client_fd = 0;
        conn.transport = None;
        conn.state = ConnState::AppConnected;
        conn.last_ping_sent = Some(Instant::now() - PING_TIMEOUT - Duration::from_secs(1));
        conn.pending_pings
            .insert(42, Instant::now() - PING_TIMEOUT - Duration::from_secs(1));

        assert!(
            matches!(conn.maybe_send_ping(), Err(ErrorCode::Protocol)),
            "stale unanswered pings must fail the connection"
        );
    }

    #[test]
    fn excessive_pending_pings_close_connection() {
        let mut conn = Conn::new();
        conn.client_fd = 0;
        conn.transport = None;
        conn.state = ConnState::AppConnected;
        conn.last_ping_sent = Some(Instant::now() - PING_INTERVAL - Duration::from_millis(1));
        for token in 1..=MAX_PENDING_PINGS as u32 {
            conn.pending_pings
                .insert(token, Instant::now() - Duration::from_millis(100));
        }

        assert!(
            matches!(conn.maybe_send_ping(), Err(ErrorCode::Protocol)),
            "too many unanswered pings must fail the connection"
        );
    }

    #[test]
    fn ping_timeout_starts_after_flush_not_queue() {
        use std::os::unix::io::IntoRawFd;
        use std::os::unix::net::UnixStream;

        let (client_end, _peer) = UnixStream::pair().unwrap();
        client_end.set_nonblocking(true).unwrap();

        let mut conn = Conn::new();
        conn.client_fd = 0;
        conn.state = ConnState::AppConnected;
        conn.transport = Some(Transport::new_plain(client_end.into_raw_fd()));
        conn.send_buffer.write(b"backlog").unwrap();

        conn.maybe_send_ping().unwrap();
        assert!(
            conn.pending_pings.is_empty(),
            "unflushed ping must not start the RTT timeout"
        );
        assert!(conn.queued_ping.is_some());
        assert!(
            conn.last_ping_sent.is_none(),
            "ping interval must not advance until the ping is flushed"
        );

        conn.flush().unwrap();
        assert_eq!(conn.pending_pings.len(), 1);
        assert!(conn.queued_ping.is_none());
        assert!(conn.last_ping_sent.is_some());
    }

    #[test]
    fn commit_flushed_ping_waits_for_ping_bytes_not_later_media() {
        let mut conn = Conn::new();
        conn.client_fd = 0;
        conn.state = ConnState::AppConnected;
        conn.transport = None;
        conn.send_buffer.write(b"still queued").unwrap();
        conn.queued_ping = Some(QueuedPing {
            token: 1,
            queued_at: Instant::now(),
            bytes_until_flushed: conn.send_buffer.available(),
        });

        conn.flush().unwrap();
        assert!(
            conn.pending_pings.is_empty(),
            "ping must not be timed until its own queued bytes are flushed"
        );
        assert!(conn.queued_ping.is_some());

        if let Some(ref mut queued) = conn.queued_ping {
            queued.bytes_until_flushed = 0;
        }
        conn.flush().unwrap();
        assert_eq!(conn.pending_pings.len(), 1);
        assert!(conn.queued_ping.is_none());
    }

    #[test]
    fn commit_flushed_ping_does_not_wait_for_post_ping_media() {
        let mut conn = Conn::new();
        conn.client_fd = 0;
        conn.state = ConnState::AppConnected;
        conn.transport = None;
        conn.send_buffer.write(b"backlog").unwrap();

        conn.maybe_send_ping().unwrap();
        let bytes_through_ping = conn.queued_ping.as_ref().unwrap().bytes_until_flushed;
        conn.send_buffer.write(b"after-ping").unwrap();
        assert_eq!(
            bytes_through_ping,
            conn.send_buffer.available() - b"after-ping".len(),
            "flush target must exclude post-ping media appended afterward"
        );

        conn.send_buffer.drain(bytes_through_ping);
        if let Some(ref mut queued) = conn.queued_ping {
            queued.bytes_until_flushed = 0;
        }
        conn.flush().unwrap();

        assert_eq!(
            conn.pending_pings.len(),
            1,
            "ping RTT must start once the ping leaves the buffer, not after post-ping media"
        );
        assert!(conn.queued_ping.is_none());
        assert_eq!(conn.send_buffer.available(), b"after-ping".len());
        assert_eq!(conn.send_buffer.peek(), b"after-ping");
    }

    #[test]
    fn stale_queued_ping_closes_connection() {
        let mut conn = Conn::new();
        conn.client_fd = 0;
        conn.transport = None;
        conn.state = ConnState::AppConnected;
        conn.queued_ping = Some(QueuedPing {
            token: 7,
            queued_at: Instant::now() - PING_TIMEOUT - Duration::from_secs(1),
            bytes_until_flushed: 1,
        });

        assert!(
            matches!(conn.maybe_send_ping(), Err(ErrorCode::Protocol)),
            "unflushed ping stuck longer than PING_TIMEOUT must close the connection"
        );
    }

    #[test]
    fn media_frames_on_wrong_msg_stream_id_are_ignored() {
        let mut conn = Conn::new();
        conn.relay_enabled = true;
        conn.current_stream = Some(Box::new(Stream::new(1)));
        if let Some(s) = conn.current_stream.as_mut() {
            s.is_publishing = true;
        }

        let payload = vec![0x17, 0x01, 0x00, 0x00, 0x00];
        conn.handle_media_frame(99, FrameType::Video, 0, &payload, None)
            .unwrap();
        assert!(conn.pending_relay.is_empty());
    }

    /// Captured from DJI Fly on an RC 2 right after its `connect`: releaseStream, FCPublish,
    /// createStream (fmt 3), a ping response that opens CSID 2 with a fmt 1 header, then
    /// publish("", "live") on CSID 4.
    const DJI_FLY_AFTER_CONNECT: [u8; 152] = [
        0x43, 0x00, 0x00, 0x00, 0x00, 0x00, 0x1D, 0x14, 0x02, 0x00, 0x0D, 0x72, 0x65, 0x6C, 0x65,
        0x61, 0x73, 0x65, 0x53, 0x74, 0x72, 0x65, 0x61, 0x6D, 0x00, 0x40, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x05, 0x02, 0x00, 0x00, 0x43, 0x00, 0x00, 0x00, 0x00, 0x00, 0x19, 0x14,
        0x02, 0x00, 0x09, 0x46, 0x43, 0x50, 0x75, 0x62, 0x6C, 0x69, 0x73, 0x68, 0x00, 0x40, 0x08,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x05, 0x02, 0x00, 0x00, 0xC3, 0x02, 0x00, 0x0C, 0x63,
        0x72, 0x65, 0x61, 0x74, 0x65, 0x53, 0x74, 0x72, 0x65, 0x61, 0x6D, 0x00, 0x40, 0x10, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x05, 0x42, 0x00, 0x00, 0x00, 0x00, 0x00, 0x06, 0x04, 0x00,
        0x07, 0x00, 0x00, 0x00, 0x01, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x1E, 0x14, 0x01, 0x00,
        0x00, 0x00, 0x02, 0x00, 0x07, 0x70, 0x75, 0x62, 0x6C, 0x69, 0x73, 0x68, 0x00, 0x40, 0x14,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x05, 0x02, 0x00, 0x00, 0x02, 0x00, 0x04, 0x6C, 0x69,
        0x76, 0x65,
    ];

    /// A connection that has answered DJI Fly's `connect` the way the server does.
    fn dji_fly_connection() -> Conn {
        let mut conn = Conn::new();
        conn.chunk_size = 4096;
        conn.state = ConnState::AppConnected;
        conn.app = "drone".to_string();
        conn.on_publish_cb = Some(|_, app, stream| app == "drone" && stream.is_empty());
        conn.send_connect_response(1.0, None).unwrap();
        conn
    }

    #[test]
    fn dji_fly_publish_sequence_reaches_publishing() {
        let mut conn = dji_fly_connection();
        conn.recv_buffer.write(&DJI_FLY_AFTER_CONNECT).unwrap();

        let mut budget = MAX_MESSAGES_PER_RECV;
        assert_eq!(conn.read_messages(&mut budget), 1);
        assert!(
            conn.current_stream
                .as_ref()
                .is_some_and(|stream| stream.is_publishing)
        );
    }

    #[test]
    fn a_reconnecting_publisher_takes_its_route_over_from_a_stale_connection() {
        // After a Wi-Fi drop, DJI Fly reconnects while its old connection still holds the
        // application-only route; its releaseStream("") frees the route when the host allows.
        let routes = std::sync::Arc::new(Mutex::new(HashMap::new()));
        let registry = PublishRouteRegistry::new(std::sync::Arc::clone(&routes));
        assert!(registry.claim(1, "drone", ""));
        let route = ("drone".to_string(), String::new());

        let mut refused = dji_fly_connection();
        refused.conn_id = 2;
        refused.publish_routes = Some(registry.clone());
        refused.recv_buffer.write(&DJI_FLY_AFTER_CONNECT).unwrap();
        let mut budget = MAX_MESSAGES_PER_RECV;
        refused.read_messages(&mut budget);
        assert!(!refused.current_stream.as_ref().unwrap().is_publishing);
        assert_eq!(routes.lock().unwrap().get(&route), Some(&1));

        let mut replacing = dji_fly_connection();
        replacing.conn_id = 3;
        replacing.publish_routes = Some(registry);
        replacing.on_release_stream_cb = Some(|_, app, stream| app == "drone" && stream.is_empty());
        replacing.recv_buffer.write(&DJI_FLY_AFTER_CONNECT).unwrap();
        let mut budget = MAX_MESSAGES_PER_RECV;
        assert_eq!(replacing.read_messages(&mut budget), 1);
        assert!(replacing.current_stream.as_ref().unwrap().is_publishing);
        assert_eq!(routes.lock().unwrap().get(&route), Some(&3));
    }

    #[test]
    fn a_publisher_that_never_announces_a_chunk_size_keeps_128_byte_chunks() {
        // Our connect response announced 4096 for our own chunks, but DJI Fly never sends Set
        // Chunk Size: its first keyframe still arrives as 128-byte chunks (fmt 1, then 0xC4
        // continuations on CSID 4), exactly as captured from the RC 2.
        let mut conn = dji_fly_connection();
        assert_eq!(conn.active_chunk_size, 4096);
        assert_eq!(conn.chunk_reg.default_chunk_size, DEFAULT_CHUNK_SIZE);
        conn.recv_buffer.write(&DJI_FLY_AFTER_CONNECT).unwrap();

        let mut keyframe = vec![0u8; 1000];
        keyframe[..2].copy_from_slice(&[0x17, 0x01]);
        let mut wire = vec![0x44, 0x00, 0x00, 0x00, 0x00, 0x03, 0xE8, 0x09];
        for (index, chunk) in keyframe.chunks(128).enumerate() {
            if index > 0 {
                wire.push(0xC4);
            }
            wire.extend_from_slice(chunk);
        }
        conn.recv_buffer.write(&wire).unwrap();

        let mut budget = MAX_MESSAGES_PER_RECV;
        assert_eq!(conn.read_messages(&mut budget), 1);
        assert_eq!(conn.media_bytes_received, 1000);
    }

    #[test]
    fn read_messages_respects_recv_level_message_budget() {
        use crate::chunk::writer::chunk_write;

        let mut conn = Conn::new();
        conn.state = ConnState::Connected;

        let mut wire = Buffer::new();
        for i in 0..300usize {
            let payload = [i as u8];
            let msg = ChunkMessage {
                csid: 2,
                fmt: 0,
                timestamp: 0,
                msg_length: 1,
                msg_type_id: 0x03,
                msg_stream_id: 0,
                is_complete: false,
            };
            chunk_write(&mut wire, &msg, &payload, 1, 128).unwrap();
        }
        conn.recv_buffer = wire;

        let mut budget = MAX_MESSAGES_PER_RECV;
        let _ = conn.read_messages(&mut budget);
        assert_eq!(budget, 0);
        assert!(conn.recv_buffer.available() > 0);
    }

    #[test]
    fn recv_rejects_recv_buffer_growth_past_cap() {
        let mut conn = Conn::new();
        conn.recv_buffer
            .write(&vec![0u8; MAX_RECV_BUFFER_BYTES])
            .unwrap();
        assert!(matches!(conn.recv(&[1]), Err(ErrorCode::Protocol)));
    }

    #[test]
    fn multitrack_invokes_on_frame_cb_per_track() {
        use std::sync::{LazyLock, Mutex};

        static SEEN_TRACK_IDS: LazyLock<Mutex<Vec<u8>>> = LazyLock::new(|| Mutex::new(Vec::new()));

        fn record_track_id(frame: &Frame) {
            SEEN_TRACK_IDS.lock().unwrap().push(frame.track_id);
        }

        SEEN_TRACK_IDS.lock().unwrap().clear();
        let mut conn = Conn::new();
        conn.relay_enabled = true;
        conn.current_stream = Some(Box::new(Stream::new(1)));
        if let Some(s) = conn.current_stream.as_mut() {
            s.is_publishing = true;
        }
        conn.on_frame_cb = Some(record_track_id);

        let payload = vec![
            0x86, 0x10, b'a', b'v', b'c', b'1', 0x00, 0x00, 0x00, 0x03, 0xAA, 0xBB, 0xCC, 0x01,
            0x00, 0x00, 0x02, 0xDD, 0xEE,
        ];
        conn.handle_media_frame(1, FrameType::Video, 0, &payload, None)
            .unwrap();

        assert_eq!(*SEEN_TRACK_IDS.lock().unwrap(), vec![0, 1]);
        assert_eq!(conn.pending_relay.len(), 1);
        assert_eq!(conn.pending_relay[0].payload, payload);
    }

    #[test]
    fn multitrack_codec_is_detected_before_authorization() {
        use std::sync::{LazyLock, Mutex};

        static SEEN_CODEC: LazyLock<Mutex<Option<String>>> = LazyLock::new(|| Mutex::new(None));
        fn allow_media(_: u64, _: FrameType, codec: Option<&str>) -> bool {
            *SEEN_CODEC.lock().unwrap() = codec.map(str::to_owned);
            true
        }

        let mut conn = Conn::new();
        conn.relay_enabled = true;
        conn.current_stream = Some(Box::new(Stream::new(1)));
        conn.current_stream.as_mut().unwrap().is_publishing = true;
        conn.on_media_cb = Some(allow_media);
        let payload = vec![0x86, 0x10, b'a', b'v', b'c', b'1', 0, 0, 0, 1, 0xAA];
        conn.handle_media_frame(1, FrameType::Video, 0, &payload, None)
            .unwrap();
        assert_eq!(SEEN_CODEC.lock().unwrap().as_deref(), Some("avc1"));
    }

    #[test]
    fn on_media_cb_authorizes_each_frame_codec_not_first_frame_pin() {
        fn allow_avc1_only(_: u64, frame_type: FrameType, codec: Option<&str>) -> bool {
            if frame_type == FrameType::Video {
                return codec == Some("avc1");
            }
            true
        }

        let mut conn = Conn::new();
        conn.relay_enabled = true;
        conn.current_stream = Some(Box::new(Stream::new(1)));
        conn.current_stream.as_mut().unwrap().is_publishing = true;
        conn.on_media_cb = Some(allow_avc1_only);

        let avc1_multitrack = vec![0x86, 0x10, b'a', b'v', b'c', b'1', 0, 0, 0, 1, 0xAA];
        conn.handle_media_frame(1, FrameType::Video, 0, &avc1_multitrack, None)
            .unwrap();
        assert_eq!(conn.pending_relay.len(), 1);

        let vp09 = vec![0x90, b'v', b'p', b'0', b'9'];
        assert_eq!(
            conn.handle_media_frame(1, FrameType::Video, 1, &vp09, None),
            Err(ErrorCode::Auth)
        );
        assert_eq!(conn.pending_relay.len(), 1);
    }

    #[test]
    fn on_media_cb_authorizes_every_track_in_many_codecs_container() {
        fn allow_avc1_only(_: u64, frame_type: FrameType, codec: Option<&str>) -> bool {
            if frame_type == FrameType::Video {
                return codec == Some("avc1");
            }
            true
        }

        let mut conn = Conn::new();
        conn.relay_enabled = true;
        conn.current_stream = Some(Box::new(Stream::new(1)));
        conn.current_stream.as_mut().unwrap().is_publishing = true;
        conn.on_media_cb = Some(allow_avc1_only);

        // ManyTracksManyCodecs container: first track is the allowed "avc1",
        // second track is the disallowed "vp09". Authorization must inspect
        // every track, not just the first, or a publisher could smuggle the
        // disallowed codec past `on_media_cb` by pairing it with an allowed
        // first track.
        let mixed_codec_frame = vec![
            0x86, 0x20, // multitrack video, type=ManyTracksManyCodecs
            b'a', b'v', b'c', b'1', 0x00, 0x00, 0x00, 0x01, 0xAA, // track 0: avc1
            b'v', b'p', b'0', b'9', 0x01, 0x00, 0x00, 0x01, 0xBB, // track 1: vp09
        ];
        assert_eq!(
            conn.handle_media_frame(1, FrameType::Video, 0, &mixed_codec_frame, None),
            Err(ErrorCode::Auth)
        );
        assert!(conn.pending_relay.is_empty());
    }

    #[test]
    fn on_media_cb_authorizes_shared_codec_in_many_tracks_container() {
        fn allow_avc1_only(_: u64, frame_type: FrameType, codec: Option<&str>) -> bool {
            if frame_type == FrameType::Video {
                return codec == Some("avc1");
            }
            true
        }

        let mut conn = Conn::new();
        conn.relay_enabled = true;
        conn.current_stream = Some(Box::new(Stream::new(1)));
        conn.current_stream.as_mut().unwrap().is_publishing = true;
        conn.on_media_cb = Some(allow_avc1_only);

        // ManyTracks container: both tracks share the disallowed "vp09"
        // codec. Even though every track carries the same codec here, this
        // container type still runs `foreach_track`/`media_allowed` rather
        // than the single-codec fast path, and must reject the frame.
        let shared_disallowed_codec_frame = vec![
            0x86, 0x10, // multitrack video, type=ManyTracks
            b'v', b'p', b'0', b'9', // shared fourcc: vp09
            0x00, 0x00, 0x00, 0x01, 0xAA, // track 0
            0x01, 0x00, 0x00, 0x01, 0xBB, // track 1
        ];
        assert_eq!(
            conn.handle_media_frame(1, FrameType::Video, 0, &shared_disallowed_codec_frame, None),
            Err(ErrorCode::Auth)
        );
        assert!(conn.pending_relay.is_empty());
    }

    #[test]
    fn on_media_cb_receives_hex_label_for_non_utf8_fourcc() {
        use std::sync::{LazyLock, Mutex};

        static SEEN_CODEC: LazyLock<Mutex<Option<String>>> = LazyLock::new(|| Mutex::new(None));
        fn record_codec(_: u64, _: FrameType, codec: Option<&str>) -> bool {
            *SEEN_CODEC.lock().unwrap() = codec.map(str::to_owned);
            true
        }

        let mut conn = Conn::new();
        conn.relay_enabled = true;
        conn.current_stream = Some(Box::new(Stream::new(1)));
        conn.current_stream.as_mut().unwrap().is_publishing = true;
        conn.on_media_cb = Some(record_codec);

        let non_utf8_fourcc = vec![0x90, 0x80, 0x80, 0x80, 0x80, 0x00, 0x00, 0x00, 0x01, 0xAA];
        conn.handle_media_frame(1, FrameType::Video, 0, &non_utf8_fourcc, None)
            .unwrap();
        assert_eq!(
            SEEN_CODEC.lock().unwrap().as_deref(),
            Some("fourcc:80808080")
        );
    }

    #[test]
    fn on_media_cb_deny_list_can_block_hex_fourcc_labels() {
        fn deny_hex_fourcc(_: u64, frame_type: FrameType, codec: Option<&str>) -> bool {
            if frame_type == FrameType::Video {
                return !codec.is_some_and(|c| c.starts_with("fourcc:"));
            }
            true
        }

        let mut conn = Conn::new();
        conn.relay_enabled = true;
        conn.current_stream = Some(Box::new(Stream::new(1)));
        conn.current_stream.as_mut().unwrap().is_publishing = true;
        conn.on_media_cb = Some(deny_hex_fourcc);

        let non_utf8_fourcc = vec![0x90, 0x80, 0x80, 0x80, 0x80, 0x00, 0x00, 0x00, 0x01, 0xAA];
        assert_eq!(
            conn.handle_media_frame(1, FrameType::Video, 0, &non_utf8_fourcc, None),
            Err(ErrorCode::Auth)
        );
        assert!(conn.pending_relay.is_empty());
    }

    #[test]
    fn on_media_cb_deny_list_observes_unknown_legacy_codec_nibble() {
        fn deny_legacy_vp6(_: u64, frame_type: FrameType, codec: Option<&str>) -> bool {
            if frame_type == FrameType::Video {
                return codec != Some("legacy:4");
            }
            true
        }

        let mut conn = Conn::new();
        conn.relay_enabled = true;
        conn.current_stream = Some(Box::new(Stream::new(1)));
        conn.current_stream.as_mut().unwrap().is_publishing = true;
        conn.on_media_cb = Some(deny_legacy_vp6);

        // Legacy FLV video with codec nibble 4 (VP6) must surface a stable
        // identity to `on_media_cb`, not `None`.
        let legacy_vp6 = vec![0x14, 0x01, 0xAA];
        assert_eq!(
            conn.handle_media_frame(1, FrameType::Video, 0, &legacy_vp6, None),
            Err(ErrorCode::Auth)
        );
        assert!(conn.pending_relay.is_empty());
    }

    #[test]
    fn modex_wrapper_cannot_bypass_codec_deny_list_without_caps_ex() {
        fn allow_avc1_only(_: u64, frame_type: FrameType, codec: Option<&str>) -> bool {
            frame_type == FrameType::Video && codec == Some("avc1")
        }

        let mut conn = Conn::new();
        conn.relay_enabled = true;
        // Legacy connect: negotiated_caps.caps_ex_mask stays 0.
        conn.current_stream = Some(Box::new(Stream::new(1)));
        conn.current_stream.as_mut().unwrap().is_publishing = true;
        conn.on_media_cb = Some(allow_avc1_only);

        // Naive exvideo_parse reads bytes 1..5 as the FourCC on ModEx packets.
        // Craft those bytes as "avc1" while the peeled inner packet is vp09.
        let payload = vec![
            0x97, b'a', b'v', b'c', b'1', 0x02, 0, 1, 2, 0x01, 0x90, b'v', b'p', b'0', b'9', 0, 0,
            0, 0xBB,
        ];
        assert_eq!(
            conn.handle_media_frame(1, FrameType::Video, 0, &payload, None),
            Err(ErrorCode::Auth),
            "ModEx wrapper must not let publishers smuggle a disallowed inner codec \
             past on_media_cb when capsEx was not negotiated"
        );
        assert!(conn.pending_relay.is_empty());
    }

    #[test]
    fn excessive_modex_chain_cannot_bypass_codec_deny_list() {
        fn allow_avc1_only(_: u64, frame_type: FrameType, codec: Option<&str>) -> bool {
            frame_type == FrameType::Video && codec == Some("avc1")
        }

        let mut conn = Conn::new();
        conn.relay_enabled = true;
        conn.negotiated_caps.has_caps_ex = true;
        conn.negotiated_caps.caps_ex_mask = CAPS_EX_MASK_MODEX;
        conn.current_stream = Some(Box::new(Stream::new(1)));
        conn.current_stream.as_mut().unwrap().is_publishing = true;
        conn.on_media_cb = Some(allow_avc1_only);

        let mut payload = vec![0x97, b'a', b'v', b'c', b'1'];
        for _ in 0..40 {
            payload.extend_from_slice(&[0x00, 0x00, 0x07]);
        }
        payload.extend_from_slice(&[0x90, b'v', b'p', b'0', b'9', 0, 0, 0, 0xBB]);
        assert_eq!(
            conn.handle_media_frame(1, FrameType::Video, 0, &payload, None),
            Err(ErrorCode::Auth),
            "unpeelable ModEx chains must not bypass codec-specific deny lists"
        );
        assert!(conn.pending_relay.is_empty());
    }

    #[test]
    fn on_media_cb_deny_list_blocks_wildcard_enhanced_fourcc() {
        fn deny_hevc_and_av1(_: u64, frame_type: FrameType, codec: Option<&str>) -> bool {
            if frame_type == FrameType::Video {
                return !matches!(codec, Some("hvc1") | Some("av01"));
            }
            true
        }

        let mut conn = Conn::new();
        conn.relay_enabled = true;
        conn.current_stream = Some(Box::new(Stream::new(1)));
        conn.current_stream.as_mut().unwrap().is_publishing = true;
        conn.on_media_cb = Some(deny_hevc_and_av1);

        let hvc1 = vec![0x90, b'h', b'v', b'c', b'1', 0, 0, 0, 1, 0xAA];
        assert_eq!(
            conn.handle_media_frame(1, FrameType::Video, 0, &hvc1, None),
            Err(ErrorCode::Auth)
        );

        let wildcard = vec![0x90, b'*', b' ', b' ', b' ', 0, 0, 0, 1, 0xAA];
        assert_eq!(
            conn.handle_media_frame(1, FrameType::Video, 0, &wildcard, None),
            Err(ErrorCode::Auth),
            "wildcard media FourCC must not bypass codec-specific deny lists"
        );
        assert!(conn.pending_relay.is_empty());
    }

    #[test]
    fn evict_active_publish_route_clears_detected_codecs() {
        let mut conn = Conn::new();
        conn.relay_enabled = true;
        conn.current_stream = Some(Box::new(Stream::new(1)));
        conn.current_stream.as_mut().unwrap().is_publishing = true;
        let payload = vec![0x86, 0x10, b'a', b'v', b'c', b'1', 0, 0, 0, 1, 0xAA];
        conn.handle_media_frame(1, FrameType::Video, 0, &payload, None)
            .unwrap();
        assert_eq!(conn.detected_video_codec.as_deref(), Some("avc1"));

        conn.evict_active_publish_route();
        assert!(conn.detected_video_codec.is_none());
        assert!(conn.detected_audio_codec.is_none());
    }

    #[test]
    fn enhanced_video_metadata_frame_populates_detected_hdr_info() {
        let mut conn = Conn::new();
        conn.relay_enabled = true;
        conn.current_stream = Some(Box::new(Stream::new(1)));
        conn.current_stream.as_mut().unwrap().is_publishing = true;

        assert!(conn.detected_hdr_info.is_none());

        let mut amf = Buffer::new();
        crate::amf::amf0::write_object_begin(&mut amf).unwrap();
        crate::amf::amf0::write_object_key(&mut amf, "colorPrimaries").unwrap();
        crate::amf::amf0::write_number(&mut amf, 1.0).unwrap();
        crate::amf::amf0::write_object_key(&mut amf, "transferCharacteristics").unwrap();
        crate::amf::amf0::write_number(&mut amf, 2.0).unwrap();
        crate::amf::amf0::write_object_key(&mut amf, "matrixCoefficients").unwrap();
        crate::amf::amf0::write_number(&mut amf, 3.0).unwrap();
        crate::amf::amf0::write_object_end(&mut amf).unwrap();

        let mut payload = vec![0x94, b'h', b'v', b'c', b'1']; // ex-header, frame_type=1, packet_type=4 (Metadata)
        payload.extend_from_slice(amf.as_slice());
        conn.handle_media_frame(1, FrameType::Video, 0, &payload, None)
            .unwrap();

        let hdr = conn.detected_hdr_info.expect("colorInfo must be captured");
        assert_eq!(hdr.color_primaries, 1);
        assert_eq!(hdr.transfer_chars, 2);
        assert_eq!(hdr.matrix_coeffs, 3);

        conn.evict_active_publish_route();
        assert!(
            conn.detected_hdr_info.is_none(),
            "detected_hdr_info must be cleared alongside the other detected-* fields"
        );
    }

    #[test]
    fn inject_relay_frame_rejects_playing_only_connection() {
        let mut conn = Conn::new();
        conn.app = "live".to_string();
        conn.current_stream = Some(Box::new(Stream {
            stream_id: 1,
            name: "cam".to_string(),
            is_publishing: false,
            is_playing: true,
            paused: false,
            receive_audio: true,
            receive_video: true,
        }));
        assert!(
            conn.inject_relay_frame(FrameType::Video, 0, &[0x17, 0x01, 0xAA])
                .is_err(),
            "playing-only connections must not inject publisher media"
        );
    }

    #[test]
    fn evict_active_publish_route_releases_idle_inject_claim() {
        use std::collections::HashMap;
        use std::sync::{Arc, Mutex};

        let routes = Arc::new(Mutex::new(HashMap::new()));
        let registry = PublishRouteRegistry::new(Arc::clone(&routes));
        let key = ("live".to_string(), "cam".to_string());

        let mut conn = Conn::new();
        conn.conn_id = 7;
        conn.app = "live".to_string();
        conn.current_stream = Some(Box::new(Stream {
            name: "cam".to_string(),
            ..Stream::new(1)
        }));
        conn.publish_routes = Some(registry);

        conn.inject_relay_frame(FrameType::Video, 0, &[0x17, 0x01, 0xAA])
            .unwrap();
        assert_eq!(routes.lock().unwrap().get(&key), Some(&7));

        // No publish/play command ever ran (current_stream.is_publishing is
        // still false), but a later createStream/play/teardown still calls
        // evict_active_publish_route(); the claim from the idle inject must
        // not survive it, or the route stays unavailable to a real publisher
        // until this connection closes.
        conn.evict_active_publish_route();
        assert!(
            routes.lock().unwrap().get(&key).is_none(),
            "route claimed by an idle inject must be released on stream replacement"
        );
    }

    #[test]
    fn inject_relay_frame_releases_fresh_claim_when_queueing_fails() {
        use std::collections::HashMap;
        use std::sync::{Arc, Mutex};

        let routes = Arc::new(Mutex::new(HashMap::new()));
        let registry = PublishRouteRegistry::new(Arc::clone(&routes));
        let key = ("live".to_string(), "cam".to_string());

        let mut conn = Conn::new();
        conn.conn_id = 7;
        conn.app = "live".to_string();
        conn.current_stream = Some(Box::new(Stream {
            name: "cam".to_string(),
            ..Stream::new(1)
        }));
        conn.publish_routes = Some(registry);
        conn.max_pending_relay_bytes = 0;

        assert!(
            conn.inject_relay_frame(FrameType::Video, 0, &[0x17, 0x01, 0xAA])
                .is_err(),
            "queueing must fail once the pending-relay byte budget is exhausted"
        );
        assert!(
            routes.lock().unwrap().get(&key).is_none(),
            "a claim taken only to service a rejected frame must not be left behind"
        );

        // A different connection must now be able to claim the route.
        let mut other = Conn::new();
        other.conn_id = 9;
        other.app = "live".to_string();
        other.current_stream = Some(Box::new(Stream {
            name: "cam".to_string(),
            ..Stream::new(1)
        }));
        other.publish_routes = Some(PublishRouteRegistry::new(Arc::clone(&routes)));
        assert!(
            other
                .inject_relay_frame(FrameType::Video, 0, &[0x17, 0x01, 0xAA])
                .is_ok(),
            "a rolled-back claim must not block a subsequent legitimate claim"
        );
    }

    fn allow_release_stream(_conn_id: u64, _app: &str, _stream_name: &str) -> bool {
        true
    }

    #[test]
    fn release_stream_force_releases_a_route_owned_by_another_connection_when_authorized() {
        use std::collections::HashMap;
        use std::sync::{Arc, Mutex};

        let routes = Arc::new(Mutex::new(HashMap::new()));
        let key = ("live".to_string(), "cam1".to_string());
        PublishRouteRegistry::new(Arc::clone(&routes)).claim(7, "live", "cam1");
        assert_eq!(routes.lock().unwrap().get(&key), Some(&7));

        let mut conn = Conn::new();
        conn.conn_id = 9;
        conn.app = "live".to_string();
        conn.publish_routes = Some(PublishRouteRegistry::new(Arc::clone(&routes)));
        conn.on_release_stream_cb = Some(allow_release_stream);

        let mut buf = Buffer::new();
        command::build_release_stream(&mut buf, "cam1").unwrap();
        conn.handle_command(buf.as_slice()).unwrap();

        assert!(
            routes.lock().unwrap().get(&key).is_none(),
            "releaseStream must force-clear a stale claim once the host authorizes it, so a \
             reconnecting encoder can republish"
        );
    }

    #[test]
    fn release_stream_without_a_callback_does_not_evict_another_connections_live_claim() {
        use std::collections::HashMap;
        use std::sync::{Arc, Mutex};

        let routes = Arc::new(Mutex::new(HashMap::new()));
        let key = ("live".to_string(), "cam1".to_string());
        PublishRouteRegistry::new(Arc::clone(&routes)).claim(7, "live", "cam1");

        let mut conn = Conn::new();
        conn.conn_id = 9;
        conn.app = "live".to_string();
        conn.publish_routes = Some(PublishRouteRegistry::new(Arc::clone(&routes)));
        // No on_release_stream_cb set -- must be a safe no-op by default.

        let mut buf = Buffer::new();
        command::build_release_stream(&mut buf, "cam1").unwrap();
        conn.handle_command(buf.as_slice()).unwrap();

        assert_eq!(
            routes.lock().unwrap().get(&key),
            Some(&7),
            "releaseStream must not evict another connection's active claim unless the host \
             explicitly authorizes it -- otherwise any peer could hijack another stream by name"
        );
    }

    #[test]
    fn release_stream_callback_denial_does_not_evict() {
        use std::collections::HashMap;
        use std::sync::{Arc, Mutex};

        fn deny_release_stream(_conn_id: u64, _app: &str, _stream_name: &str) -> bool {
            false
        }

        let routes = Arc::new(Mutex::new(HashMap::new()));
        let key = ("live".to_string(), "cam1".to_string());
        PublishRouteRegistry::new(Arc::clone(&routes)).claim(7, "live", "cam1");

        let mut conn = Conn::new();
        conn.conn_id = 9;
        conn.app = "live".to_string();
        conn.publish_routes = Some(PublishRouteRegistry::new(Arc::clone(&routes)));
        conn.on_release_stream_cb = Some(deny_release_stream);

        let mut buf = Buffer::new();
        command::build_release_stream(&mut buf, "cam1").unwrap();
        conn.handle_command(buf.as_slice()).unwrap();

        assert_eq!(routes.lock().unwrap().get(&key), Some(&7));
    }

    #[test]
    fn release_stream_on_an_unclaimed_route_is_a_no_op() {
        use std::collections::HashMap;
        use std::sync::{Arc, Mutex};

        let routes = Arc::new(Mutex::new(HashMap::new()));

        let mut conn = Conn::new();
        conn.conn_id = 9;
        conn.app = "live".to_string();
        conn.publish_routes = Some(PublishRouteRegistry::new(Arc::clone(&routes)));

        let mut buf = Buffer::new();
        command::build_release_stream(&mut buf, "cam1").unwrap();
        assert!(conn.handle_command(buf.as_slice()).is_ok());
    }

    #[test]
    fn inject_relay_frame_keeps_prior_claim_when_a_later_frame_is_rejected() {
        use std::collections::HashMap;
        use std::sync::{Arc, Mutex};

        let routes = Arc::new(Mutex::new(HashMap::new()));
        let registry = PublishRouteRegistry::new(Arc::clone(&routes));
        let key = ("live".to_string(), "cam".to_string());

        let mut conn = Conn::new();
        conn.conn_id = 7;
        conn.app = "live".to_string();
        conn.current_stream = Some(Box::new(Stream {
            name: "cam".to_string(),
            ..Stream::new(1)
        }));
        conn.publish_routes = Some(registry);

        conn.inject_relay_frame(FrameType::Video, 0, &[0x17, 0x01, 0xAA])
            .unwrap();
        assert_eq!(routes.lock().unwrap().get(&key), Some(&7));

        // Starve the budget so the next frame is rejected.
        conn.max_pending_relay_bytes = 0;
        assert!(
            conn.inject_relay_frame(FrameType::Video, 0, &[0x17, 0x01, 0xAA])
                .is_err()
        );

        // The connection already legitimately owned this route from the
        // first successful inject; a later rejected frame must not evict it.
        assert_eq!(
            routes.lock().unwrap().get(&key),
            Some(&7),
            "an already-owned route must survive a subsequent rejected frame"
        );
    }

    #[test]
    fn inject_relay_frame_restores_prior_claim_when_route_switch_queueing_fails() {
        use std::collections::HashMap;
        use std::sync::{Arc, Mutex};

        let routes = Arc::new(Mutex::new(HashMap::new()));
        let registry = PublishRouteRegistry::new(Arc::clone(&routes));
        let key_a = ("live".to_string(), "A".to_string());
        let key_b = ("live".to_string(), "B".to_string());

        let mut conn = Conn::new();
        conn.conn_id = 7;
        conn.app = "live".to_string();
        conn.current_stream = Some(Box::new(Stream {
            name: "A".to_string(),
            ..Stream::new(1)
        }));
        conn.publish_routes = Some(registry);

        conn.inject_relay_frame(FrameType::Video, 0, &[0x17, 0x01, 0xAA])
            .unwrap();
        assert_eq!(routes.lock().unwrap().get(&key_a), Some(&7));
        assert!(conn.pending_cache_evictions.is_empty());

        // Switch the public relay key, then reject the inject by exhausting
        // the pending budget. Claim/eviction for B must roll back to A.
        conn.relay_key = "B".to_string();
        conn.max_pending_relay_bytes = 0;
        assert!(
            conn.inject_relay_frame(FrameType::Video, 0, &[0x17, 0x01, 0xBB])
                .is_err()
        );
        assert_eq!(
            routes.lock().unwrap().get(&key_a),
            Some(&7),
            "failed route-switch inject must restore the previous claim"
        );
        assert!(
            routes.lock().unwrap().get(&key_b).is_none(),
            "rejected switch must not leave the new route claimed"
        );
        assert!(
            conn.pending_cache_evictions.is_empty(),
            "failed switch must not queue eviction of the restored route"
        );
        assert_eq!(conn.claimed_publish_route.as_deref(), Some("A"));
    }

    #[test]
    fn multitrack_on_frame_cb_scratch_retains_last_track_payload() {
        let mut conn = Conn::new();
        conn.relay_enabled = true;
        conn.current_stream = Some(Box::new(Stream::new(1)));
        if let Some(s) = conn.current_stream.as_mut() {
            s.is_publishing = true;
        }
        conn.on_frame_cb = Some(|_| {});

        let payload = vec![
            0x86, 0x10, b'a', b'v', b'c', b'1', 0x00, 0x00, 0x00, 0x03, 0xAA, 0xBB, 0xCC, 0x01,
            0x00, 0x00, 0x02, 0xDD, 0xEE,
        ];
        conn.handle_media_frame(1, FrameType::Video, 0, &payload, None)
            .unwrap();

        assert_eq!(conn.frame_cb_scratch.as_slice(), &[0xDD, 0xEE]);
    }

    #[test]
    fn modex_is_normalized_for_callbacks_and_cache_but_relayed_opaque() {
        let mut conn = Conn::new();
        conn.relay_enabled = true;
        conn.negotiated_caps.has_caps_ex = true;
        conn.negotiated_caps.caps_ex_mask = CAPS_EX_MASK_MODEX;
        conn.current_stream = Some(Box::new(Stream::new(1)));
        conn.current_stream.as_mut().unwrap().is_publishing = true;
        conn.on_frame_cb = Some(|_| {});

        let payload = vec![
            0x97, 0x02, 0, 1, 2, 0x01, b'a', b'v', b'c', b'1', 0, 0, 0, 0xAA,
        ];
        conn.handle_media_frame(1, FrameType::Video, 0, &payload, None)
            .unwrap();

        assert_eq!(conn.pending_relay[0].payload, payload);
        assert_eq!(
            conn.pending_relay[0].cache_payload(),
            &[0x91, b'a', b'v', b'c', b'1', 0, 0, 0, 0xAA]
        );
        assert_eq!(
            conn.frame_cb_scratch,
            vec![0x91, b'a', b'v', b'c', b'1', 0, 0, 0, 0xAA]
        );
    }

    #[test]
    fn on_frame_cb_scratch_retains_payload_after_delivery() {
        let mut conn = Conn::new();
        conn.relay_enabled = true;
        conn.current_stream = Some(Box::new(Stream::new(1)));
        if let Some(s) = conn.current_stream.as_mut() {
            s.is_publishing = true;
        }
        conn.on_frame_cb = Some(|_| {});

        let payload = vec![0x17, 0x42];
        conn.handle_media_frame(1, FrameType::Video, 0, &payload, None)
            .unwrap();
        assert_eq!(conn.frame_cb_scratch.as_slice(), payload.as_slice());
    }

    fn flv_subtag(tag_type: u8, timestamp: u32, data: &[u8]) -> Vec<u8> {
        let mut v = Vec::new();
        v.push(tag_type);
        let len = data.len() as u32;
        v.extend_from_slice(&[(len >> 16) as u8, (len >> 8) as u8, len as u8]);
        v.extend_from_slice(&[
            (timestamp >> 16) as u8,
            (timestamp >> 8) as u8,
            timestamp as u8,
            (timestamp >> 24) as u8,
        ]);
        v.extend_from_slice(&[0, 0, 0]); // stream id (always 0 on the wire)
        v.extend_from_slice(data);
        let prev_tag_size = (11 + data.len()) as u32;
        v.extend_from_slice(&prev_tag_size.to_be_bytes());
        v
    }

    #[test]
    fn aggregate_message_unpacks_audio_and_video_subtags_into_relay() {
        let mut conn = Conn::new();
        conn.relay_enabled = true;
        conn.current_stream = Some(Box::new(Stream::new(1)));
        if let Some(s) = conn.current_stream.as_mut() {
            s.is_publishing = true;
        }

        let audio_payload = vec![0xAF, 0x01, 0x11, 0x22];
        let video_payload = vec![0x17, 0x01, 0x00, 0x00, 0x00, 0xAA];
        let mut aggregate = Vec::new();
        aggregate.extend(flv_subtag(0x08, 100, &audio_payload));
        aggregate.extend(flv_subtag(0x09, 140, &video_payload));

        conn.handle_aggregate_for_test(1, 1000, &aggregate).unwrap();

        assert_eq!(conn.pending_relay.len(), 2);
        assert_eq!(conn.pending_relay[0].frame_type, FrameType::Audio);
        assert_eq!(conn.pending_relay[0].timestamp, 1000);
        assert_eq!(conn.pending_relay[0].payload, audio_payload);
        assert_eq!(conn.pending_relay[1].frame_type, FrameType::Video);
        // Second sub-tag's ts (140) is 40 past the first sub-tag's ts (100),
        // so its relayed timestamp is the aggregate's own timestamp (1000) + 40.
        assert_eq!(conn.pending_relay[1].timestamp, 1040);
        assert_eq!(conn.pending_relay[1].payload, video_payload);
    }

    #[test]
    fn multitrack_subtracks_consume_message_budget() {
        use std::sync::{LazyLock, Mutex};

        static CALLBACKS: LazyLock<Mutex<usize>> = LazyLock::new(|| Mutex::new(0));

        let mut conn = Conn::new();
        conn.relay_enabled = true;
        conn.current_stream = Some(Box::new(Stream::new(1)));
        if let Some(s) = conn.current_stream.as_mut() {
            s.is_publishing = true;
        }
        conn.max_pending_relay_bytes = usize::MAX;
        *CALLBACKS.lock().unwrap() = 0;
        conn.on_frame_cb = Some(|_| {
            *CALLBACKS.lock().unwrap() += 1;
        });

        let mut payload = vec![0x86, 0x10, b'a', b'v', b'c', b'1'];
        for id in 0..8u8 {
            payload.push(id);
            payload.extend_from_slice(&[0x00, 0x00, 0x01]);
            payload.push(0xAA);
        }

        // `handle_message()` already charges one unit for the outer RTMP message.
        // This direct helper call therefore starts with the remaining budget.
        let mut messages_budget = 2;
        conn.handle_media_frame(1, FrameType::Video, 0, &payload, Some(&mut messages_budget))
            .unwrap();

        assert_eq!(messages_budget, 0);
        assert_eq!(*CALLBACKS.lock().unwrap(), 3);
    }

    #[test]
    fn multitrack_on_media_cb_authorization_consume_message_budget() {
        use std::sync::{LazyLock, Mutex};

        static CALLBACKS: LazyLock<Mutex<usize>> = LazyLock::new(|| Mutex::new(0));

        fn allow_all(_: u64, _: FrameType, _: Option<&str>) -> bool {
            *CALLBACKS.lock().unwrap() += 1;
            true
        }

        let mut conn = Conn::new();
        conn.relay_enabled = true;
        conn.current_stream = Some(Box::new(Stream::new(1)));
        if let Some(s) = conn.current_stream.as_mut() {
            s.is_publishing = true;
        }
        conn.on_media_cb = Some(allow_all);
        *CALLBACKS.lock().unwrap() = 0;

        let mut payload = vec![0x86, 0x10, b'a', b'v', b'c', b'1'];
        for id in 0..8u8 {
            payload.push(id);
            payload.extend_from_slice(&[0x00, 0x00, 0x01]);
            payload.push(0xAA);
        }

        let mut messages_budget = 2;
        assert_eq!(
            conn.handle_media_frame(1, FrameType::Video, 0, &payload, Some(&mut messages_budget)),
            Err(ErrorCode::Protocol)
        );

        assert_eq!(messages_budget, 0);
        assert_eq!(*CALLBACKS.lock().unwrap(), 3);
    }

    #[test]
    fn publish_rename_rejects_when_pending_cache_evictions_cap_reached() {
        let mut conn = Conn::new();
        conn.app = "live".to_string();
        conn.current_stream = Some(Box::new(Stream::new(1)));
        if let Some(s) = conn.current_stream.as_mut() {
            s.is_publishing = true;
            s.name = "route-a".to_string();
        }
        conn.claimed_publish_route = Some("route-a".to_string());
        conn.pending_cache_evictions = (0..MAX_PENDING_CACHE_EVICTIONS)
            .map(|i| ("live".to_string(), format!("stale-{i}")))
            .collect();

        let mut buf = Buffer::with_capacity(256);
        command::build_publish(&mut buf, "route-b", "live").unwrap();
        conn.handle_command(buf.as_slice()).unwrap();
        assert_eq!(
            conn.pending_cache_evictions.len(),
            MAX_PENDING_CACHE_EVICTIONS
        );
        assert_eq!(conn.claimed_publish_route.as_deref(), Some("route-a"));
    }

    #[test]
    fn aggregate_subtag_dispatches_consume_message_budget() {
        let mut conn = Conn::new();
        conn.relay_enabled = true;
        conn.current_stream = Some(Box::new(Stream::new(1)));
        if let Some(s) = conn.current_stream.as_mut() {
            s.is_publishing = true;
        }
        conn.max_pending_relay_bytes = usize::MAX;

        let audio_payload = vec![0xAF, 0x01];
        let mut aggregate = Vec::new();
        for i in 0..10 {
            aggregate.extend(flv_subtag(0x08, i, &audio_payload));
        }

        let mut messages_budget = 3;
        conn.handle_aggregate(1, 0, &aggregate, &mut messages_budget)
            .unwrap();

        assert_eq!(messages_budget, 0);
        assert_eq!(conn.pending_relay.len(), 3);
    }

    #[test]
    fn aggregate_unknown_subtags_consume_message_budget() {
        let mut conn = Conn::new();
        let filler = [0x00];
        let mut aggregate = Vec::new();
        for i in 0..10 {
            aggregate.extend(flv_subtag(0x01, i, &filler));
        }

        let mut messages_budget = 3;
        conn.handle_aggregate(1, 0, &aggregate, &mut messages_budget)
            .unwrap();

        assert_eq!(messages_budget, 0);
        assert!(conn.pending_relay.is_empty());
    }

    #[test]
    fn aggregate_rejects_zero_size_subtags() {
        let mut conn = Conn::new();
        let aggregate = flv_subtag(0x08, 0, &[]);
        let mut messages_budget = MAX_MESSAGES_PER_RECV;

        assert_eq!(
            conn.handle_aggregate(1, 0, &aggregate, &mut messages_budget),
            Err(ErrorCode::Protocol)
        );
    }

    #[test]
    fn aggregate_message_rejects_subtag_size_overrunning_payload() {
        let mut conn = Conn::new();
        conn.relay_enabled = true;
        conn.current_stream = Some(Box::new(Stream::new(1)));
        if let Some(s) = conn.current_stream.as_mut() {
            s.is_publishing = true;
        }

        // Claims a data_size far larger than the bytes actually present.
        let mut aggregate = vec![0x08, 0xFF, 0xFF, 0xFF];
        aggregate.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0]);
        aggregate.extend_from_slice(b"short");

        assert_eq!(
            conn.handle_aggregate_for_test(1, 0, &aggregate),
            Err(ErrorCode::Protocol)
        );
    }

    #[test]
    fn aggregate_dispatch_reaches_handle_aggregate_via_handle_message() {
        let mut conn = Conn::new();
        conn.relay_enabled = true;
        conn.current_stream = Some(Box::new(Stream::new(1)));
        if let Some(s) = conn.current_stream.as_mut() {
            s.is_publishing = true;
        }

        let video_payload = vec![0x17, 0x01, 0x00, 0x00, 0x00, 0xBB];
        let aggregate = flv_subtag(0x09, 0, &video_payload);
        let msg = ChunkMessage {
            csid: 6,
            fmt: 0,
            timestamp: 500,
            msg_length: aggregate.len() as u32,
            msg_type_id: msg_dispatch::RTMP_MSG_AGGREGATE,
            msg_stream_id: 1,
            is_complete: true,
        };

        conn.handle_message_for_test(&msg, &aggregate).unwrap();

        assert_eq!(conn.pending_relay.len(), 1);
        assert_eq!(conn.pending_relay[0].frame_type, FrameType::Video);
        assert_eq!(conn.pending_relay[0].timestamp, 500);
        assert_eq!(conn.pending_relay[0].payload, video_payload);
    }

    #[test]
    fn amf3_shared_object_dispatch_reaches_callback() {
        use crate::message::shared_object::{SharedObjectEvent, SharedObjectEventType};
        use std::sync::{LazyLock, Mutex};

        static SEEN: LazyLock<Mutex<Option<(u64, String, usize)>>> =
            LazyLock::new(|| Mutex::new(None));

        fn record_so(conn_id: u64, so: &SharedObjectMessage) {
            *SEEN.lock().unwrap() = Some((conn_id, so.name.clone(), so.events.len()));
        }

        *SEEN.lock().unwrap() = None;
        let mut conn = Conn::new();
        conn.conn_id = 42;
        conn.state = ConnState::AppConnected;
        conn.on_shared_object_cb = Some(record_so);

        let so = SharedObjectMessage {
            name: "chat".to_string(),
            version: 1,
            flags: 0,
            events: vec![SharedObjectEvent {
                event_type: SharedObjectEventType::Use,
                data: Vec::new(),
            }],
        };
        let mut amf_buf = Buffer::with_capacity(64);
        shared_object::write(&so, &mut amf_buf).unwrap();
        let mut payload = vec![0x00];
        payload.extend_from_slice(amf_buf.as_slice());

        let msg = ChunkMessage {
            csid: 3,
            fmt: 0,
            timestamp: 0,
            msg_length: payload.len() as u32,
            msg_type_id: msg_dispatch::RTMP_MSG_AMF3_SHARED_OBJECT,
            msg_stream_id: 0,
            is_complete: true,
        };
        conn.handle_message_for_test(&msg, &payload).unwrap();

        assert_eq!(
            *SEEN.lock().unwrap(),
            Some((42, "chat".to_string(), 1)),
            "on_shared_object_cb must fire with the parsed envelope"
        );
    }

    #[test]
    fn amf3_shared_object_dropped_when_publish_auth_configured_without_so_auth() {
        use crate::message::shared_object::{SharedObjectEvent, SharedObjectEventType};
        use std::sync::{LazyLock, Mutex};

        static SEEN: LazyLock<Mutex<bool>> = LazyLock::new(|| Mutex::new(false));

        fn record_so(_conn_id: u64, _so: &SharedObjectMessage) {
            *SEEN.lock().unwrap() = true;
        }

        *SEEN.lock().unwrap() = false;
        let mut conn = Conn::new();
        conn.conn_id = 42;
        conn.state = ConnState::AppConnected;
        conn.on_publish_cb = Some(|_, _, _| true);
        conn.on_shared_object_cb = Some(record_so);

        let so = SharedObjectMessage {
            name: "chat".to_string(),
            version: 1,
            flags: 0,
            events: vec![SharedObjectEvent {
                event_type: SharedObjectEventType::Change,
                data: b"evil".to_vec(),
            }],
        };
        let mut amf_buf = Buffer::with_capacity(64);
        shared_object::write(&so, &mut amf_buf).unwrap();
        let mut payload = vec![0x00];
        payload.extend_from_slice(amf_buf.as_slice());
        let msg = ChunkMessage {
            csid: 3,
            fmt: 0,
            timestamp: 0,
            msg_length: payload.len() as u32,
            msg_type_id: msg_dispatch::RTMP_MSG_AMF3_SHARED_OBJECT,
            msg_stream_id: 0,
            is_complete: true,
        };
        conn.handle_message_for_test(&msg, &payload).unwrap();
        assert!(
            !*SEEN.lock().unwrap(),
            "shared objects must not bypass publish/play auth hooks"
        );
    }

    #[test]
    fn amf3_shared_object_dropped_when_only_on_media_cb_configured() {
        use crate::message::shared_object::{SharedObjectEvent, SharedObjectEventType};
        use std::sync::{LazyLock, Mutex};

        static SEEN: LazyLock<Mutex<bool>> = LazyLock::new(|| Mutex::new(false));

        fn record_so(_conn_id: u64, _so: &SharedObjectMessage) {
            *SEEN.lock().unwrap() = true;
        }

        *SEEN.lock().unwrap() = false;
        let mut conn = Conn::new();
        conn.conn_id = 42;
        conn.state = ConnState::AppConnected;
        conn.on_media_cb = Some(|_, _, _| true);
        conn.on_shared_object_cb = Some(record_so);

        let so = SharedObjectMessage {
            name: "chat".to_string(),
            version: 1,
            flags: 0,
            events: vec![SharedObjectEvent {
                event_type: SharedObjectEventType::Change,
                data: b"evil".to_vec(),
            }],
        };
        let mut amf_buf = Buffer::with_capacity(64);
        shared_object::write(&so, &mut amf_buf).unwrap();
        let mut payload = vec![0x00];
        payload.extend_from_slice(amf_buf.as_slice());
        let msg = ChunkMessage {
            csid: 3,
            fmt: 0,
            timestamp: 0,
            msg_length: payload.len() as u32,
            msg_type_id: msg_dispatch::RTMP_MSG_AMF3_SHARED_OBJECT,
            msg_stream_id: 0,
            is_complete: true,
        };
        conn.handle_message_for_test(&msg, &payload).unwrap();
        assert!(
            !*SEEN.lock().unwrap(),
            "shared objects must not bypass media-only auth configuration"
        );
    }

    #[test]
    fn amf3_shared_object_dropped_when_only_on_frame_cb_configured() {
        use crate::message::shared_object::{SharedObjectEvent, SharedObjectEventType};
        use std::sync::{LazyLock, Mutex};

        static SEEN: LazyLock<Mutex<bool>> = LazyLock::new(|| Mutex::new(false));

        fn record_so(_conn_id: u64, _so: &SharedObjectMessage) {
            *SEEN.lock().unwrap() = true;
        }

        *SEEN.lock().unwrap() = false;
        let mut conn = Conn::new();
        conn.conn_id = 42;
        conn.state = ConnState::AppConnected;
        conn.on_frame_cb = Some(|_| {});
        conn.on_shared_object_cb = Some(record_so);

        let so = SharedObjectMessage {
            name: "chat".to_string(),
            version: 1,
            flags: 0,
            events: vec![SharedObjectEvent {
                event_type: SharedObjectEventType::Change,
                data: b"evil".to_vec(),
            }],
        };
        let mut amf_buf = Buffer::with_capacity(64);
        shared_object::write(&so, &mut amf_buf).unwrap();
        let mut payload = vec![0x00];
        payload.extend_from_slice(amf_buf.as_slice());
        let msg = ChunkMessage {
            csid: 3,
            fmt: 0,
            timestamp: 0,
            msg_length: payload.len() as u32,
            msg_type_id: msg_dispatch::RTMP_MSG_AMF3_SHARED_OBJECT,
            msg_stream_id: 0,
            is_complete: true,
        };
        conn.handle_message_for_test(&msg, &payload).unwrap();
        assert!(
            !*SEEN.lock().unwrap(),
            "shared objects must not bypass frame-only auth configuration"
        );
    }

    #[test]
    fn amf3_shared_object_delivered_when_only_on_connect_cb_configured() {
        use crate::message::shared_object::{SharedObjectEvent, SharedObjectEventType};
        use std::sync::{LazyLock, Mutex};

        static SEEN: LazyLock<Mutex<bool>> = LazyLock::new(|| Mutex::new(false));

        fn record_so(_conn_id: u64, _so: &SharedObjectMessage) {
            *SEEN.lock().unwrap() = true;
        }

        *SEEN.lock().unwrap() = false;
        let mut conn = Conn::new();
        conn.conn_id = 42;
        conn.state = ConnState::AppConnected;
        conn.on_connect_cb = Some(|| {});
        conn.on_shared_object_cb = Some(record_so);

        let so = SharedObjectMessage {
            name: "chat".to_string(),
            version: 1,
            flags: 0,
            events: vec![SharedObjectEvent {
                event_type: SharedObjectEventType::Change,
                data: b"evil".to_vec(),
            }],
        };
        let mut amf_buf = Buffer::with_capacity(64);
        shared_object::write(&so, &mut amf_buf).unwrap();
        let mut payload = vec![0x00];
        payload.extend_from_slice(amf_buf.as_slice());
        let msg = ChunkMessage {
            csid: 3,
            fmt: 0,
            timestamp: 0,
            msg_length: payload.len() as u32,
            msg_type_id: msg_dispatch::RTMP_MSG_AMF3_SHARED_OBJECT,
            msg_stream_id: 0,
            is_complete: true,
        };
        conn.handle_message_for_test(&msg, &payload).unwrap();
        assert!(
            *SEEN.lock().unwrap(),
            "connect callback is an observer and must not require shared-object auth"
        );
    }

    #[test]
    fn amf3_shared_object_dropped_when_only_on_release_stream_cb_configured() {
        use crate::message::shared_object::{SharedObjectEvent, SharedObjectEventType};
        use std::sync::{LazyLock, Mutex};

        static SEEN: LazyLock<Mutex<bool>> = LazyLock::new(|| Mutex::new(false));

        fn record_so(_conn_id: u64, _so: &SharedObjectMessage) {
            *SEEN.lock().unwrap() = true;
        }

        *SEEN.lock().unwrap() = false;
        let mut conn = Conn::new();
        conn.conn_id = 42;
        conn.state = ConnState::AppConnected;
        conn.on_release_stream_cb = Some(|_, _, _| true);
        conn.on_shared_object_cb = Some(record_so);

        let so = SharedObjectMessage {
            name: "chat".to_string(),
            version: 1,
            flags: 0,
            events: vec![SharedObjectEvent {
                event_type: SharedObjectEventType::Change,
                data: b"evil".to_vec(),
            }],
        };
        let mut amf_buf = Buffer::with_capacity(64);
        shared_object::write(&so, &mut amf_buf).unwrap();
        let mut payload = vec![0x00];
        payload.extend_from_slice(amf_buf.as_slice());
        let msg = ChunkMessage {
            csid: 3,
            fmt: 0,
            timestamp: 0,
            msg_length: payload.len() as u32,
            msg_type_id: msg_dispatch::RTMP_MSG_AMF3_SHARED_OBJECT,
            msg_stream_id: 0,
            is_complete: true,
        };
        conn.handle_message_for_test(&msg, &payload).unwrap();
        assert!(
            !*SEEN.lock().unwrap(),
            "shared objects must not bypass release-stream-only auth configuration"
        );
    }

    #[test]
    fn amf3_shared_object_honors_on_shared_object_auth_cb() {
        use crate::message::shared_object::{SharedObjectEvent, SharedObjectEventType};
        use std::sync::{LazyLock, Mutex};

        static SEEN: LazyLock<Mutex<bool>> = LazyLock::new(|| Mutex::new(false));

        fn record_so(_conn_id: u64, _so: &SharedObjectMessage) {
            *SEEN.lock().unwrap() = true;
        }

        fn allow_chat(_conn_id: u64, so: &SharedObjectMessage) -> bool {
            so.name == "chat"
        }

        *SEEN.lock().unwrap() = false;
        let mut conn = Conn::new();
        conn.conn_id = 42;
        conn.state = ConnState::AppConnected;
        conn.on_shared_object_auth_cb = Some(allow_chat);
        conn.on_shared_object_cb = Some(record_so);

        let denied = SharedObjectMessage {
            name: "admin".to_string(),
            version: 1,
            flags: 0,
            events: vec![SharedObjectEvent {
                event_type: SharedObjectEventType::Change,
                data: b"nope".to_vec(),
            }],
        };
        let mut denied_buf = Buffer::with_capacity(64);
        shared_object::write(&denied, &mut denied_buf).unwrap();
        let mut denied_payload = vec![0x00];
        denied_payload.extend_from_slice(denied_buf.as_slice());
        let denied_msg = ChunkMessage {
            csid: 3,
            fmt: 0,
            timestamp: 0,
            msg_length: denied_payload.len() as u32,
            msg_type_id: msg_dispatch::RTMP_MSG_AMF3_SHARED_OBJECT,
            msg_stream_id: 0,
            is_complete: true,
        };
        conn.handle_message_for_test(&denied_msg, &denied_payload)
            .unwrap();
        assert!(
            !*SEEN.lock().unwrap(),
            "denied shared object must not fire callback"
        );

        let allowed = SharedObjectMessage {
            name: "chat".to_string(),
            version: 1,
            flags: 0,
            events: vec![SharedObjectEvent {
                event_type: SharedObjectEventType::Use,
                data: Vec::new(),
            }],
        };
        let mut allowed_buf = Buffer::with_capacity(64);
        shared_object::write(&allowed, &mut allowed_buf).unwrap();
        let mut allowed_payload = vec![0x00];
        allowed_payload.extend_from_slice(allowed_buf.as_slice());
        let allowed_msg = ChunkMessage {
            csid: 3,
            fmt: 0,
            timestamp: 0,
            msg_length: allowed_payload.len() as u32,
            msg_type_id: msg_dispatch::RTMP_MSG_AMF3_SHARED_OBJECT,
            msg_stream_id: 0,
            is_complete: true,
        };
        conn.handle_message_for_test(&allowed_msg, &allowed_payload)
            .unwrap();
        assert!(
            *SEEN.lock().unwrap(),
            "authorized shared object must reach on_shared_object_cb"
        );
    }

    #[test]
    fn malformed_amf3_shared_object_is_dropped_without_error() {
        let mut conn = Conn::new();
        conn.state = ConnState::AppConnected;
        let payload = vec![0x00, 0xFF, 0xFF]; // truncated name length, no body
        let msg = ChunkMessage {
            csid: 3,
            fmt: 0,
            timestamp: 0,
            msg_length: payload.len() as u32,
            msg_type_id: msg_dispatch::RTMP_MSG_AMF3_SHARED_OBJECT,
            msg_stream_id: 0,
            is_complete: true,
        };
        assert!(conn.handle_message_for_test(&msg, &payload).is_ok());
    }

    #[test]
    fn amf3_shared_object_before_connect_does_not_reach_callback() {
        use crate::message::shared_object::{SharedObjectEvent, SharedObjectEventType};
        use std::sync::{LazyLock, Mutex};

        static SEEN: LazyLock<Mutex<bool>> = LazyLock::new(|| Mutex::new(false));

        fn record_so(_conn_id: u64, _so: &SharedObjectMessage) {
            *SEEN.lock().unwrap() = true;
        }

        *SEEN.lock().unwrap() = false;
        let mut conn = Conn::new();
        conn.conn_id = 42;
        conn.on_shared_object_cb = Some(record_so);
        // conn.state defaults to a pre-connect state (e.g. TcpAccepted or
        // Handshake) -- self.app is still empty here.

        let so = SharedObjectMessage {
            name: "chat".to_string(),
            version: 1,
            flags: 0,
            events: vec![SharedObjectEvent {
                event_type: SharedObjectEventType::Use,
                data: Vec::new(),
            }],
        };
        let mut amf_buf = Buffer::with_capacity(64);
        shared_object::write(&so, &mut amf_buf).unwrap();
        let mut payload = vec![0x00];
        payload.extend_from_slice(amf_buf.as_slice());

        let msg = ChunkMessage {
            csid: 3,
            fmt: 0,
            timestamp: 0,
            msg_length: payload.len() as u32,
            msg_type_id: msg_dispatch::RTMP_MSG_AMF3_SHARED_OBJECT,
            msg_stream_id: 0,
            is_complete: true,
        };
        conn.handle_message_for_test(&msg, &payload).unwrap();

        assert!(
            !*SEEN.lock().unwrap(),
            "shared objects must be rejected before connect completes"
        );
    }

    #[test]
    fn send_shared_object_round_trips_through_parse() {
        use crate::message::shared_object::{SharedObjectEvent, SharedObjectEventType};

        let mut conn = Conn::new();
        let so = SharedObjectMessage {
            name: "scoreboard".to_string(),
            version: 2,
            flags: 0x01,
            events: vec![SharedObjectEvent {
                event_type: SharedObjectEventType::Change,
                data: vec![1, 2, 3],
            }],
        };
        conn.send_shared_object(&so).unwrap();

        let mut reg = ChunkRegistry::new();
        let mut out_msg = ChunkMessage::default();
        let (status, payload) = chunk_read_owned(&mut conn.send_buffer, &mut reg, &mut out_msg)
            .expect("wire bytes from send_shared_object must decode as a valid chunk message");
        assert_eq!(status, 1);
        assert!(out_msg.is_complete);
        assert_eq!(
            out_msg.msg_type_id,
            msg_dispatch::RTMP_MSG_AMF3_SHARED_OBJECT
        );
        assert_eq!(payload[0], 0x00);
        let parsed = shared_object::parse(&payload[1..]).unwrap();
        assert_eq!(parsed.name, "scoreboard");
        assert_eq!(parsed.version, 2);
        assert!(parsed.is_persistent());
        assert_eq!(parsed.events.len(), 1);
    }

    fn amf0_string(s: &str) -> Vec<u8> {
        let mut buf = Buffer::with_capacity(64);
        crate::amf::amf0::write_string(&mut buf, s).unwrap();
        buf.as_slice().to_vec()
    }

    fn amf0_object_end() -> [u8; 3] {
        [0, 0, 0x09]
    }

    fn publishing_conn(stream_id: u32) -> Conn {
        let mut conn = Conn::new();
        conn.current_stream = Some(Box::new(Stream::new(stream_id)));
        if let Some(s) = conn.current_stream.as_mut() {
            s.is_publishing = true;
        }
        conn
    }

    fn build_on_metadata_payload(
        with_set_data_frame: bool,
        entries: &[(&str, f64)],
        extra: &[(&str, bool)],
    ) -> Vec<u8> {
        let mut payload = Vec::new();
        if with_set_data_frame {
            payload.extend(amf0_string("@setDataFrame"));
        }
        payload.extend(amf0_string("onMetaData"));
        payload.push(crate::amf::amf0::Amf0Type::Object as u8);
        for (key, value) in entries {
            let mut buf = Buffer::with_capacity(32);
            crate::amf::amf0::write_object_key(&mut buf, key).unwrap();
            crate::amf::amf0::write_number(&mut buf, *value).unwrap();
            payload.extend_from_slice(buf.as_slice());
        }
        for (key, value) in extra {
            let mut buf = Buffer::with_capacity(32);
            crate::amf::amf0::write_object_key(&mut buf, key).unwrap();
            crate::amf::amf0::write_boolean(&mut buf, *value).unwrap();
            payload.extend_from_slice(buf.as_slice());
        }
        payload.extend_from_slice(&amf0_object_end());
        payload
    }

    #[test]
    fn on_metadata_is_queued_for_relay_while_publishing() {
        let mut conn = Conn::new();
        conn.app = "live".to_string();
        conn.relay_enabled = true;
        conn.current_stream = Some(Box::new(Stream::new(1)));
        if let Some(ref mut stream) = conn.current_stream {
            stream.is_publishing = true;
            stream.name = "stream".to_string();
        }
        let payload = build_on_metadata_payload(false, &[("width", 1920.0)], &[]);
        conn.handle_publisher_data_message(1, 9000, &payload)
            .unwrap();
        assert_eq!(conn.pending_relay.len(), 1);
        assert_eq!(conn.pending_relay[0].frame_type, FrameType::Script);
        assert_eq!(conn.pending_relay[0].timestamp, 9000);
        assert_eq!(conn.pending_relay[0].payload, payload);
    }

    #[test]
    fn on_metadata_relay_honors_on_media_cb() {
        fn deny_all(_: u64, _: FrameType, _: Option<&str>) -> bool {
            false
        }

        let mut conn = Conn::new();
        conn.app = "live".to_string();
        conn.relay_enabled = true;
        conn.on_media_cb = Some(deny_all);
        conn.current_stream = Some(Box::new(Stream::new(1)));
        if let Some(ref mut stream) = conn.current_stream {
            stream.is_publishing = true;
            stream.name = "stream".to_string();
        }
        let payload = build_on_metadata_payload(false, &[("width", 1920.0)], &[]);
        assert_eq!(
            conn.handle_publisher_data_message(1, 9000, &payload),
            Err(ErrorCode::Auth)
        );
        assert!(conn.pending_relay.is_empty());
        assert_eq!(conn.detected_video_width, None);
    }

    #[test]
    fn connect_rejects_empty_app_name() {
        let mut conn = Conn::new();
        let mut buf = Buffer::with_capacity(256);
        command::build_connect(
            &mut buf,
            "",
            "rtmp://host/live",
            "",
            "",
            "FMLE/3.0",
            0,
            0,
            None,
        )
        .unwrap();
        conn.handle_command(buf.as_slice()).unwrap();
        assert!(conn.app.is_empty());
        assert_eq!(conn.state, ConnState::TcpAccepted);
        assert!(
            conn.send_buffer
                .peek()
                .windows(b"_error".len())
                .any(|window| window == b"_error")
        );
    }

    #[test]
    fn publish_rejects_empty_stream_name() {
        let mut conn = Conn::new();
        conn.app = "live".to_string();
        conn.current_stream = Some(Box::new(Stream::new(1)));
        let mut buf = Buffer::with_capacity(128);
        command::build_publish(&mut buf, "", "live").unwrap();
        conn.handle_command(buf.as_slice()).unwrap();
        assert!(!conn.current_stream.as_ref().unwrap().is_publishing);
        assert!(
            conn.send_buffer
                .peek()
                .windows(b"BadName".len())
                .any(|window| window == b"BadName")
        );
    }

    #[test]
    fn bytes_received_ack_uses_u64_after_u32_wrap() {
        let mut conn = Conn::new();
        conn.state = ConnState::Connected;
        conn.window_ack_size = 1024;
        conn.bytes_received = u32::MAX as u64;
        conn.bytes_at_last_ack = u32::MAX as u64;
        conn.recv(&[0u8; 2048]).unwrap();
        assert_eq!(conn.bytes_received, u32::MAX as u64 + 2048);
        assert_eq!(conn.bytes_at_last_ack, conn.bytes_received);
    }

    #[test]
    fn play_route_changes_are_init_replay_rate_limited() {
        use std::time::{Duration, Instant};

        let mut conn = Conn::new();
        conn.current_stream = Some(Box::new(Stream::new(1)));

        let play = |conn: &mut Conn, stream_name: &str| {
            let mut buf = Buffer::new();
            crate::amf::amf0::write_string(&mut buf, "play").unwrap();
            crate::amf::amf0::write_number(&mut buf, 1.0).unwrap();
            crate::amf::amf0::write_null(&mut buf).unwrap();
            crate::amf::amf0::write_string(&mut buf, stream_name).unwrap();
            conn.handle_command(buf.as_slice()).unwrap();
        };

        play(&mut conn, "room-a");
        assert!(conn.needs_init_frames);
        conn.needs_init_frames = false;

        play(&mut conn, "room-b");
        assert!(
            !conn.needs_init_frames,
            "rapid play-route changes must not request another cached replay"
        );

        conn.last_init_replay_request =
            Some(Instant::now() - INIT_REPLAY_COOLDOWN - Duration::from_millis(1));
        play(&mut conn, "room-c");
        assert!(
            conn.needs_init_frames,
            "a route change after the cooldown should request cached init frames"
        );
    }

    #[test]
    fn repeated_play_on_same_stream_does_not_request_init_replay() {
        let mut conn = Conn::new();
        conn.current_stream = Some(Box::new(Stream::new(1)));
        {
            let stream = conn.current_stream.as_mut().unwrap();
            stream.is_playing = true;
            stream.name = "room".to_string();
        }
        conn.needs_init_frames = false;

        let mut buf = Buffer::new();
        crate::amf::amf0::write_string(&mut buf, "play").unwrap();
        crate::amf::amf0::write_number(&mut buf, 1.0).unwrap();
        crate::amf::amf0::write_null(&mut buf).unwrap();
        crate::amf::amf0::write_string(&mut buf, "room").unwrap();
        conn.handle_command(buf.as_slice()).unwrap();

        assert!(!conn.needs_init_frames);
    }

    #[test]
    fn receive_video_reenable_does_not_request_cached_replay() {
        let mut conn = Conn::new();
        conn.current_stream = Some(Box::new(Stream::new(1)));
        {
            let stream = conn.current_stream.as_mut().unwrap();
            stream.is_playing = true;
            stream.receive_video = false;
        }
        conn.needs_init_frames = false;

        let mut buf = Buffer::new();
        crate::amf::amf0::write_string(&mut buf, "receiveVideo").unwrap();
        crate::amf::amf0::write_number(&mut buf, 1.0).unwrap();
        crate::amf::amf0::write_null(&mut buf).unwrap();
        crate::amf::amf0::write_boolean(&mut buf, true).unwrap();
        conn.handle_command(buf.as_slice()).unwrap();

        assert!(conn.current_stream.as_ref().unwrap().receive_video);
        assert!(
            !conn.needs_init_frames,
            "receiveAudio/receiveVideo toggles must not schedule init-cache replay"
        );
    }

    #[test]
    fn malformed_pause_command_leaves_stream_unpaused() {
        let mut conn = Conn::new();
        conn.current_stream = Some(Box::new(Stream::new(1)));
        if let Some(ref mut stream) = conn.current_stream {
            stream.is_playing = true;
            stream.paused = false;
        }

        let mut buf = Buffer::new();
        crate::amf::amf0::write_string(&mut buf, "pause").unwrap();
        crate::amf::amf0::write_number(&mut buf, 1.0).unwrap();
        // Missing null + boolean: parse must fail without forcing paused=true.
        assert!(conn.handle_command(buf.as_slice()).is_ok());
        assert!(!conn.current_stream.as_ref().unwrap().paused);
    }

    #[test]
    fn on_metadata_populates_conn_fields() {
        let mut conn = Conn::new();
        let payload = build_on_metadata_payload(
            false,
            &[
                ("width", 1920.0),
                ("height", 1080.0),
                ("framerate", 30.0),
                ("audiosamplerate", 48000.0),
                ("audiochannels", 2.0),
            ],
            &[],
        );
        conn.handle_data_message(&payload).unwrap();
        assert_eq!(conn.detected_video_width, Some(1920));
        assert_eq!(conn.detected_video_height, Some(1080));
        assert_eq!(conn.detected_video_framerate, Some(30.0));
        assert_eq!(conn.detected_audio_sample_rate, Some(48000));
        assert_eq!(conn.detected_audio_channels, Some(2));
    }

    #[test]
    fn on_metadata_with_set_data_frame_prefix() {
        let mut conn = Conn::new();
        let payload = build_on_metadata_payload(
            true,
            &[
                ("width", 1280.0),
                ("height", 720.0),
                ("videoframerate", 60.0),
            ],
            &[("stereo", true)],
        );
        conn.handle_data_message(&payload).unwrap();
        assert_eq!(conn.detected_video_width, Some(1280));
        assert_eq!(conn.detected_video_height, Some(720));
        assert_eq!(conn.detected_video_framerate, Some(60.0));
        assert_eq!(conn.detected_audio_channels, Some(2));
        assert_eq!(conn.detected_audio_sample_rate, None);
    }

    #[test]
    fn on_metadata_missing_keys_leave_fields_none() {
        let mut conn = Conn::new();
        let payload = build_on_metadata_payload(false, &[("width", 640.0)], &[]);
        conn.handle_data_message(&payload).unwrap();
        assert_eq!(conn.detected_video_width, Some(640));
        assert_eq!(conn.detected_video_height, None);
        assert_eq!(conn.detected_video_framerate, None);
        assert_eq!(conn.detected_audio_sample_rate, None);
        assert_eq!(conn.detected_audio_channels, None);
    }

    #[test]
    fn on_metadata_skips_unknown_extra_keys() {
        let mut conn = Conn::new();
        let mut payload = amf0_string("onMetaData");
        payload.push(crate::amf::amf0::Amf0Type::Object as u8);
        let mut width = Buffer::with_capacity(32);
        crate::amf::amf0::write_object_key(&mut width, "width").unwrap();
        crate::amf::amf0::write_number(&mut width, 800.0).unwrap();
        payload.extend_from_slice(width.as_slice());
        let mut unknown = Buffer::with_capacity(64);
        crate::amf::amf0::write_object_key(&mut unknown, "customTag").unwrap();
        crate::amf::amf0::write_string(&mut unknown, "ignored").unwrap();
        payload.extend_from_slice(unknown.as_slice());
        payload.extend_from_slice(&amf0_object_end());

        conn.handle_data_message(&payload).unwrap();
        assert_eq!(conn.detected_video_width, Some(800));
    }

    #[test]
    fn on_metadata_rejects_out_of_range_dimensions() {
        let mut conn = Conn::new();
        let payload = build_on_metadata_payload(
            false,
            &[("width", -1.0), ("height", (u32::MAX as f64) + 1.0)],
            &[],
        );
        conn.handle_data_message(&payload).unwrap();
        assert_eq!(conn.detected_video_width, None);
        assert_eq!(conn.detected_video_height, None);
    }

    #[test]
    fn amf3_data_message_strips_leading_zero_byte() {
        let mut conn = publishing_conn(1);
        let mut body =
            build_on_metadata_payload(false, &[("width", 1024.0), ("height", 576.0)], &[]);
        let mut payload = vec![0x00];
        payload.append(&mut body);
        let msg = ChunkMessage {
            csid: 4,
            fmt: 0,
            timestamp: 0,
            msg_length: payload.len() as u32,
            msg_type_id: msg_dispatch::RTMP_MSG_AMF3_DATA,
            msg_stream_id: 1,
            is_complete: true,
        };
        conn.handle_message_for_test(&msg, &payload).unwrap();
        assert_eq!(conn.detected_video_width, Some(1024));
        assert_eq!(conn.detected_video_height, Some(576));
    }

    #[test]
    fn on_metadata_ignores_oversized_leading_event_name() {
        let mut conn = Conn::new();
        let long_name = "x".repeat(65);
        let mut payload = amf0_string(&long_name);
        payload.push(crate::amf::amf0::Amf0Type::Object as u8);
        payload.extend_from_slice(&amf0_object_end());

        conn.handle_data_message(&payload).unwrap();
        assert_eq!(conn.detected_video_width, None);
    }

    #[test]
    fn on_metadata_ignores_oversized_set_data_frame_inner_name() {
        let mut conn = Conn::new();
        let long_name = "y".repeat(65);
        let mut payload = amf0_string("@setDataFrame");
        payload.extend(amf0_string(&long_name));
        payload.push(crate::amf::amf0::Amf0Type::Object as u8);
        payload.extend_from_slice(&amf0_object_end());

        conn.handle_data_message(&payload).unwrap();
        assert_eq!(conn.detected_video_width, None);
    }

    #[test]
    fn on_metadata_keeps_partial_fields_when_unknown_value_cannot_be_skipped() {
        let mut conn = Conn::new();
        let mut payload = amf0_string("onMetaData");
        payload.push(crate::amf::amf0::Amf0Type::Object as u8);
        let mut width = Buffer::with_capacity(32);
        crate::amf::amf0::write_object_key(&mut width, "width").unwrap();
        crate::amf::amf0::write_number(&mut width, 1280.0).unwrap();
        payload.extend_from_slice(width.as_slice());
        let mut unknown = Buffer::with_capacity(8);
        crate::amf::amf0::write_object_key(&mut unknown, "vendor").unwrap();
        unknown
            .write(&[crate::amf::amf0::Amf0Type::Recordset as u8])
            .unwrap();
        payload.extend_from_slice(unknown.as_slice());
        payload.extend_from_slice(&amf0_object_end());

        conn.handle_data_message(&payload).unwrap();
        assert_eq!(conn.detected_video_width, Some(1280));
    }

    #[test]
    fn on_metadata_ignored_while_not_publishing() {
        let mut conn = Conn::new();
        let payload = build_on_metadata_payload(false, &[("width", 1920.0)], &[]);
        let msg = ChunkMessage {
            csid: 4,
            fmt: 0,
            timestamp: 0,
            msg_length: payload.len() as u32,
            msg_type_id: msg_dispatch::RTMP_MSG_AMF0_DATA,
            msg_stream_id: 1,
            is_complete: true,
        };
        conn.handle_message_for_test(&msg, &payload).unwrap();
        assert_eq!(conn.detected_video_width, None);
    }

    #[test]
    fn on_metadata_ignored_on_wrong_stream_id() {
        let mut conn = publishing_conn(1);
        let payload = build_on_metadata_payload(false, &[("width", 1920.0)], &[]);
        let msg = ChunkMessage {
            csid: 4,
            fmt: 0,
            timestamp: 0,
            msg_length: payload.len() as u32,
            msg_type_id: msg_dispatch::RTMP_MSG_AMF0_DATA,
            msg_stream_id: 99,
            is_complete: true,
        };
        conn.handle_message_for_test(&msg, &payload).unwrap();
        assert_eq!(conn.detected_video_width, None);
    }

    #[test]
    fn later_on_metadata_replaces_stale_values() {
        let mut conn = publishing_conn(1);
        let first = build_on_metadata_payload(false, &[("width", 1920.0), ("height", 1080.0)], &[]);
        let second = build_on_metadata_payload(false, &[("width", 1280.0), ("height", 720.0)], &[]);
        conn.handle_data_message(&first).unwrap();
        conn.handle_data_message(&second).unwrap();
        assert_eq!(conn.detected_video_width, Some(1280));
        assert_eq!(conn.detected_video_height, Some(720));
    }

    #[test]
    fn aggregate_script_subtag_populates_metadata() {
        let mut conn = publishing_conn(1);
        let metadata =
            build_on_metadata_payload(false, &[("width", 720.0), ("height", 480.0)], &[]);
        let aggregate = flv_subtag(msg_dispatch::RTMP_MSG_AMF0_DATA, 0, &metadata);
        conn.handle_aggregate_for_test(1, 0, &aggregate).unwrap();
        assert_eq!(conn.detected_video_width, Some(720));
        assert_eq!(conn.detected_video_height, Some(480));
    }

    #[test]
    fn on_metadata_tolerates_excess_keys() {
        let mut conn = publishing_conn(1);
        let mut payload = amf0_string("onMetaData");
        payload.push(crate::amf::amf0::Amf0Type::Object as u8);
        let mut width = Buffer::with_capacity(32);
        crate::amf::amf0::write_object_key(&mut width, "width").unwrap();
        crate::amf::amf0::write_number(&mut width, 800.0).unwrap();
        payload.extend_from_slice(width.as_slice());
        for i in 0..crate::amf::amf0::MAX_OBJECT_KEYS {
            let mut entry = Buffer::with_capacity(32);
            crate::amf::amf0::write_object_key(&mut entry, &format!("k{i}")).unwrap();
            crate::amf::amf0::write_null(&mut entry).unwrap();
            payload.extend_from_slice(entry.as_slice());
        }
        payload.extend_from_slice(&amf0_object_end());

        conn.handle_data_message(&payload).unwrap();
        assert_eq!(conn.detected_video_width, Some(800));
    }
}
