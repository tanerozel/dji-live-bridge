//! RTMP server listener
//!
//! Mirrors `src/server/server.h` and `src/server/server.c`.

use std::collections::{HashMap, HashSet, VecDeque};
use std::net::TcpListener;
use std::os::unix::io::{AsRawFd, IntoRawFd};
use std::sync::{Arc, Mutex};
#[cfg(feature = "tls")]
use std::time::{Duration, Instant};

use crate::chunk::state::{DEFAULT_CHUNK_SIZE, DEFAULT_MAX_MSG_LENGTH, RTMP_WIRE_MAX_MSG_LENGTH};
use crate::ertmp::multitrack_media::{foreach_track, is_multitrack_container};
use crate::media::{
    CacheFrameKind, classify_cache_frame, is_on_metadata_payload, normalize_modex_payload,
};
use crate::message::shared_object::SharedObjectMessage;
use crate::net;
use crate::session::conn::{Conn, MAX_PENDING_RELAY_FRAMES, RelayFrame};
use crate::session::publish_route::PublishRouteRegistry;
#[cfg(feature = "tls")]
use crate::transport::{PendingTlsAccept, TlsAcceptOutcome};
use crate::transport::{TlsCtx, Transport};
use crate::types::*;

/// High bit set on `RelayFrame::publisher_conn_id` marks socket-less injects.
/// Socket connection ids stay below this range.
const EXTERNAL_PUBLISHER_ID_BIT: u64 = 0x8000_0000_0000_0000;

/// Canonical sentinel for socket-less / external media (`u64::MAX`).
///
/// [`Server::inject_relay_frame`] assigns a **stable per-route** id in the
/// high-bit range (see [`is_external_publisher_id`]) so stream-cache limits
/// apply per external stream rather than across all injects.
pub const EXTERNAL_RELAY_PUBLISHER_ID: u64 = u64::MAX;

/// True when `id` belongs to the socket-less inject publisher-id range.
#[inline]
pub fn is_external_publisher_id(id: u64) -> bool {
    id & EXTERNAL_PUBLISHER_ID_BIT != 0
}

fn external_publisher_id_for_route(_app: &str, _stream_name: &str) -> u64 {
    // Legacy helper retained for tests that pre-seed cache rows without inject.
    EXTERNAL_PUBLISHER_ID_BIT | 1
}

/// Stable per-route external publisher ids without hash collisions.
fn allocate_external_publisher_id(
    routes: &mut HashMap<(String, String), u64>,
    seq: &mut u64,
    app: &str,
    stream_name: &str,
) -> u64 {
    let key = (app.to_string(), stream_name.to_string());
    if let Some(&id) = routes.get(&key) {
        return id;
    }
    let mut next = *seq;
    if next == 0 {
        next = 1;
    }
    let id = EXTERNAL_PUBLISHER_ID_BIT | (next & !EXTERNAL_PUBLISHER_ID_BIT);
    *seq = next.wrapping_add(1);
    if *seq == 0 {
        *seq = 1;
    }
    routes.insert(key, id);
    id
}

/// Maximum distinct (app, stream_name) cache entries retained server-wide.
const MAX_STREAM_CACHE_ENTRIES: usize = 1024;
/// Maximum distinct (app, stream) cache keys a single publisher may hold.
const MAX_STREAM_CACHE_KEYS_PER_PUBLISHER: usize = 64;

/// Maximum total bytes retained across all stream_cache entries server-wide.
const MAX_STREAM_CACHE_BYTES: usize = 64 * 1024 * 1024;

/// Maximum payload size cached as a codec header. Real AVC/AAC sequence
/// headers are small; larger payloads are relayed live but not stored for
/// init-frame replay.
const MAX_CACHED_INIT_FRAME_BYTES: usize = 64 * 1024;

/// Maximum payload size cached as a keyframe. Unlike codec headers, a
/// legitimate IDR frame (e.g. 1080p/4K H.264) can be several hundred KB to
/// a few MB, so keyframes get a separate, larger cap than codec headers.
/// Bounded by `DEFAULT_MAX_MSG_LENGTH` (the chunk layer's own hard ceiling
/// on a single message) rather than left unbounded.
const MAX_CACHED_KEYFRAME_BYTES: usize = 2 * 1024 * 1024;

/// Cap on `app` / `stream_name` length accepted by socket-less inject.
/// Prevents multi-megabyte route strings from bloating `stream_cache` keys
/// and `publisher_cache_keys` when an integrator feeds untrusted names.
const MAX_INJECT_ROUTE_COMPONENT_BYTES: usize = 1024;

/// Soft cap on concurrent socket-less inject claims in `active_publish_routes`.
///
/// Claims persist until [`Server::release_injected_route`] (or
/// [`Server::release_all_injected_routes`]). New unique routes beyond this
/// limit are rejected so buggy integrators that cycle routes without release
/// cannot grow the map without bound — without stealing mid-feed claims.
pub const MAX_EXTERNAL_PUBLISH_ROUTES: usize = 1024;

/// Reap external inject route claims with no frames for this long.
const INJECT_ROUTE_STALE_SECS: u64 = 120;

/// Maximum number of incomplete TLS handshakes retained when `max_connections`
/// is unlimited. When `max_connections` is set, active connections and pending
/// handshakes share that configured cap instead.
#[cfg(feature = "tls")]
const MAX_PENDING_TLS_HANDSHAKES: usize = 128;

/// Default max simultaneous connections accepted per remote address when
/// `ServerConfig::max_connections_per_addr` is `0` (unset). Prevents one peer
/// from monopolizing the global connection table on plaintext listeners.
/// Deployments where many clients share one source IP (NAT/load balancer/proxy)
/// can raise this via `ServerConfig::max_connections_per_addr`.
pub const DEFAULT_MAX_CONNECTIONS_PER_ADDR: usize = 4;

/// Default max incomplete TLS handshakes retained per remote address, used
/// when `ServerConfig::max_pending_tls_per_addr` is `0` (unset). Prevents one
/// peer from monopolizing the global pending queue. Deployments where many
/// clients share one source IP (NAT/load balancer/proxy) can raise this via
/// `ServerConfig::max_pending_tls_per_addr`.
#[cfg(feature = "tls")]
pub const DEFAULT_MAX_PENDING_TLS_PER_ADDR: usize = DEFAULT_MAX_CONNECTIONS_PER_ADDR;

/// Drop TLS handshakes that do not complete within this overall budget.
#[cfg(feature = "tls")]
const TLS_HANDSHAKE_TIMEOUT_SECS: u64 = 10;

/// Maximum inbound bytes drained from one connection per `process_connections`
/// pass. Without this cap a peer that keeps the kernel recv buffer full can
/// monopolize the single-threaded poll loop and starve every other session
/// until its socket buffer is empty.
const MAX_RECV_BYTES_PER_CONN_PER_POLL: usize = 256 * 1024;

/// Maximum number of `accept()` calls serviced per `accept_new_connections()`
/// pass. Without this cap, a source IP that keeps the listen backlog full
/// (e.g. by opening connections faster than the per-IP cap admits them)
/// would make `accept_new_connections` loop accepting-and-immediately-
/// closing sockets until the backlog empties, starving `process_connections`
/// of time within the same `poll()` call. Any backlog left over after the
/// budget is exhausted is serviced on the next `poll()` pass instead.
const MAX_ACCEPTS_PER_POLL: usize = 256;

/// Maximum number of extra budget-only `recv(&[])` passes used to drain
/// already-buffered messages left over from `Conn`'s per-recv message cap.
/// Each pass affords another full message budget, so this bounds one
/// connection to this many extra budgets per poll tick -- any remainder
/// waits for the next `process_connections` pass instead of starving other
/// connections in this one.
const MAX_BUDGET_DRAIN_PASSES_PER_CONN_PER_POLL: usize = 3;

/// Default cap on relay sends issued while fanning publisher media out to
/// players in one `process_connections` pass. Integrators can tune the active
/// value through [`Server::max_relay_sends_per_poll`].
pub const DEFAULT_MAX_RELAY_SENDS_PER_POLL: usize = 4096;

/// Cached codec headers and last keyframe for a (app, stream_name) pair.
/// Replayed to players that join after the publisher has already sent headers.
struct StreamCache {
    avc_header: Option<Vec<u8>>,
    /// Per-track video sequence headers when publishers send multitrack inits separately.
    video_track_headers: HashMap<u8, Vec<u8>>,
    aac_header: Option<Vec<u8>>,
    audio_track_headers: HashMap<u8, Vec<u8>>,
    /// Last onMetaData AMF0 payload for late-joining players.
    metadata: Option<Vec<u8>>,
    /// (timestamp, payload) of the most recent IDR keyframe.
    last_keyframe: Option<(u32, Vec<u8>)>,
}

/// Public snapshot of a stream's init-cache contents for late joiners or
/// remote subscribers.
#[derive(Clone, Debug, Default)]
pub struct StreamInitSnapshot {
    /// Last cached `onMetaData` / script payload, if any.
    pub metadata: Option<Vec<u8>>,
    /// Legacy / single-track AVC (or enhanced) video sequence header.
    pub avc_header: Option<Vec<u8>>,
    /// Legacy / single-track AAC (or enhanced) audio sequence header.
    pub aac_header: Option<Vec<u8>>,
    /// Per-track video sequence headers `(track_id, payload)`, sorted by track id.
    pub video_track_headers: Vec<(u8, Vec<u8>)>,
    /// Per-track audio sequence headers `(track_id, payload)`, sorted by track id.
    pub audio_track_headers: Vec<(u8, Vec<u8>)>,
    /// Most recent cached video keyframe as `(timestamp, payload)`.
    pub last_keyframe: Option<(u32, Vec<u8>)>,
}

/// Bounded ring of cloned publisher [`RelayFrame`]s for integrator export.
/// On overflow the oldest frames are dropped so the buffer stays within
/// `max_frames` / `max_bytes`.
struct RelayExportBuffer {
    frames: VecDeque<RelayFrame>,
    max_frames: usize,
    max_bytes: usize,
    bytes: usize,
}

impl RelayExportBuffer {
    fn new(max_frames: usize, max_bytes: usize) -> Self {
        Self {
            frames: VecDeque::new(),
            max_frames,
            max_bytes,
            bytes: 0,
        }
    }

    fn push_clone(&mut self, frame: &RelayFrame) {
        if self.max_frames == 0 {
            return;
        }
        let frame_bytes = frame.retained_bytes();
        if frame_bytes > self.max_bytes {
            // Single frame exceeds the byte budget: clear stale buffered
            // frames so a later drain does not return only pre-overflow data,
            // then skip this frame so the buffer never grows unbounded.
            self.frames.clear();
            self.bytes = 0;
            return;
        }
        while !self.frames.is_empty()
            && (self.frames.len() >= self.max_frames
                || self.bytes.saturating_add(frame_bytes) > self.max_bytes)
        {
            if let Some(old) = self.frames.pop_front() {
                self.bytes = self.bytes.saturating_sub(old.retained_bytes());
            }
        }
        if self.frames.len() >= self.max_frames
            || self.bytes.saturating_add(frame_bytes) > self.max_bytes
        {
            return;
        }
        self.bytes = self.bytes.saturating_add(frame_bytes);
        self.frames.push_back(frame.clone());
    }

    fn drain(&mut self) -> Vec<RelayFrame> {
        self.bytes = 0;
        self.frames.drain(..).collect()
    }
}

/// One bound listener socket plus the TLS context (if any) new connections
/// accepted on it should use. A `Server` holds one of these per call to
/// [`Server::listen`] / [`Server::listen_tls`].
struct ListenerEntry {
    tcp: TcpListener,
    tls_ctx: Option<TlsCtx>,
}

#[cfg(feature = "tls")]
struct PendingTlsConnection {
    handshake: PendingTlsAccept,
    remote_addr: String,
    /// Peer IP only (no port), used to key the per-address pending cap so a
    /// single host can't bypass it by opening connections from distinct
    /// ephemeral source ports.
    remote_ip: String,
    deadline: Instant,
}

/// Server object.
///
/// A single `Server` can bind more than one listener (see [`Server::listen`]
/// and [`Server::listen_tls`]) — e.g. plaintext RTMP on one port and RTMPS on
/// another. All listeners share the same `connections`, media relay, and
/// stream cache, so a publisher on one listener is relayed to players on any
/// other listener exactly as if they shared a port. Running two *separate*
/// `Server` instances instead would silently split the relay: each instance
/// only relays among its own `connections`.
pub struct Server {
    pub config: ServerConfig,
    pub resource_limits: ResourceLimits,
    /// Maximum relay send operations allowed in one `process_connections` pass.
    ///
    /// The first eligible frame in a pass is always relayed even when its fan-out
    /// exceeds this value, which guarantees forward progress instead of
    /// perpetually re-queueing an oversized frame. Later frames are deferred once
    /// the accumulated send count would exceed the configured budget.
    pub max_relay_sends_per_poll: usize,
    pub running: bool,
    /// Identifies *one* bound listener (whichever was bound first) for
    /// diagnostics/backward compatibility. When more than one listener is
    /// bound, use [`Server::listener_fds`] to register every listener with an
    /// external readiness loop. [`Server::poll`] itself checks every bound
    /// listener internally.
    pub server_fd: i32,
    pub connections: Vec<Conn>,
    /// Fired for every audio/video frame on every connection.
    pub on_frame_cb: Option<fn(&Frame)>,
    /// Fired when a client completes the AMF `connect` exchange.
    pub on_connect_cb: Option<fn()>,
    /// When set, must return true to allow `publish`; false rejects the command.
    pub on_publish_cb: Option<fn(conn_id: u64, app: &str, stream_name: &str) -> bool>,
    /// When set, must return true to allow `play`; false rejects the command.
    pub on_play_cb: Option<fn(conn_id: u64, app: &str, stream_name: &str) -> bool>,
    /// When set, must return true before publisher media is queued for relay.
    pub on_media_cb: Option<fn(u64, FrameType, Option<&str>) -> bool>,
    /// Must return true before a parsed AMF3 Shared Object message is delivered
    /// to [`Self::on_shared_object_cb`]. When unset while `on_publish_cb` or
    /// `on_play_cb` is configured, inbound shared objects are dropped.
    pub on_shared_object_auth_cb: Option<fn(u64, &SharedObjectMessage) -> bool>,
    /// Fired for every parsed AMF3 Shared Object message received on any
    /// connection.
    pub on_shared_object_cb: Option<fn(u64, &SharedObjectMessage)>,
    /// Must return true to allow `releaseStream` to force-evict another
    /// connection's publish-route claim. Unset (the default) makes
    /// `releaseStream` a safe no-op -- see `Conn::on_release_stream_cb`.
    pub on_release_stream_cb: Option<fn(conn_id: u64, app: &str, stream_name: &str) -> bool>,
    /// TLS context built from `config` at construction time; used by
    /// [`Server::listen`] calls. This field stays public for Rust API
    /// compatibility so integrators that used to replace `server.tls_ctx`
    /// directly can continue to do so before calling `listen()`.
    pub tls_ctx: Option<TlsCtx>,
    listeners: Vec<ListenerEntry>,
    /// Listener index to try first on the next accept pass.
    next_listener_accept: usize,
    #[cfg(feature = "tls")]
    pending_tls: Vec<PendingTlsConnection>,
    stream_cache: HashMap<(String, String), StreamCache>,
    /// Cache keys created by each publisher connection (for teardown).
    publisher_cache_keys: HashMap<u64, Vec<(String, String)>>,
    next_conn_id: u64,
    /// Set once a connection ID has been handed out. This prevents resetting
    /// the counter later and reusing IDs after earlier connections were closed.
    conn_ids_issued: bool,
    /// Hold media relay until the integrator enables it per connection.
    pub defer_media_relay: bool,
    /// Active publish routes: (app, stream_name) -> owning conn_id.
    pub(crate) active_publish_routes: Arc<Mutex<HashMap<(String, String), u64>>>,
    /// Optional bounded export of publisher relay frames. `None` = disabled
    /// (default): no extra clones.
    relay_export: Option<RelayExportBuffer>,
    /// Socket-less frames waiting to enter the same cache + fan-out path as
    /// publisher `pending_relay` drains.
    pending_injected_relay: Vec<RelayFrame>,
    /// Local publisher frames whose connection closed before fan-out finished.
    /// Kept server-side so players still receive already-accepted media.
    orphaned_relay: Vec<RelayFrame>,
    /// Alternates which source leads fair inject↔local interleave each poll,
    /// so a budget of 1 cannot permanently starve local (or inject) frames.
    relay_interleave_inject_first: bool,
    /// Rotates which non-empty local publisher queue leads round-robin merge
    /// so a tight send budget cannot starve later connections forever.
    relay_local_rr_offset: usize,
    /// Stable external publisher id per `(app, stream_name)` inject route.
    external_route_ids: HashMap<(String, String), u64>,
    external_route_seq: u64,
    /// Last inject activity per external route (for stale-claim reaping).
    inject_route_last_seen: HashMap<(String, String), std::time::Instant>,
}

impl Server {
    /// Create a new server.
    pub fn new(config: ServerConfig) -> Result<Self> {
        let tls_ctx = if config.tls_enabled != 0 {
            if config.tls_cert_file.is_null() || config.tls_key_file.is_null() {
                return Err(ErrorCode::Internal);
            }
            let cert = unsafe {
                std::ffi::CStr::from_ptr(config.tls_cert_file as *const std::ffi::c_char)
            };
            let key =
                unsafe { std::ffi::CStr::from_ptr(config.tls_key_file as *const std::ffi::c_char) };
            let cert_str = cert.to_str().map_err(|_| ErrorCode::Internal)?;
            let key_str = key.to_str().map_err(|_| ErrorCode::Internal)?;
            if cert_str.is_empty() || key_str.is_empty() {
                return Err(ErrorCode::Internal);
            }
            Some(TlsCtx::new_server(cert_str, key_str)?)
        } else {
            None
        };

        Ok(Self {
            config,
            resource_limits: ResourceLimits::default(),
            max_relay_sends_per_poll: DEFAULT_MAX_RELAY_SENDS_PER_POLL,
            running: false,
            server_fd: -1,
            connections: Vec::new(),
            on_frame_cb: None,
            on_connect_cb: None,
            on_publish_cb: None,
            on_play_cb: None,
            on_media_cb: None,
            on_shared_object_auth_cb: None,
            on_shared_object_cb: None,
            on_release_stream_cb: None,
            tls_ctx,
            listeners: Vec::new(),
            next_listener_accept: 0,
            #[cfg(feature = "tls")]
            pending_tls: Vec::new(),
            stream_cache: HashMap::new(),
            publisher_cache_keys: HashMap::new(),
            next_conn_id: 1,
            conn_ids_issued: false,
            defer_media_relay: false,
            active_publish_routes: Arc::new(Mutex::new(HashMap::new())),
            relay_export: None,
            pending_injected_relay: Vec::new(),
            orphaned_relay: Vec::new(),
            relay_interleave_inject_first: true,
            relay_local_rr_offset: 0,
            external_route_ids: HashMap::new(),
            external_route_seq: 1,
            inject_route_last_seen: HashMap::new(),
        })
    }

    fn release_all_publish_routes(&self, conn_id: u64) {
        if let Ok(mut routes) = self.active_publish_routes.lock() {
            routes.retain(|_, owner| *owner != conn_id);
        }
    }

    /// Set the starting value used for auto-generated connection IDs.
    ///
    /// Only needed when integrating with something *outside* this crate that
    /// numbers connections independently and must not collide with this
    /// `Server`'s IDs. Prefer [`Server::listen_tls`] over running a second
    /// `Server` instance for a second listener — one `Server` with multiple
    /// listeners numbers all of its connections from one counter already, so
    /// this is unnecessary in that case. Call right after [`Server::new`].
    ///
    /// Panics if `base` is zero, has the high bit set (reserved for
    /// [`is_external_publisher_id`]), or if any connection ID has already been
    /// issued.
    pub fn set_conn_id_base(&mut self, base: u64) {
        assert!(base != 0, "conn_id base must be non-zero");
        assert!(
            base < EXTERNAL_PUBLISHER_ID_BIT,
            "conn_id base must stay below the external publisher id range (high bit reserved)"
        );
        assert!(
            !self.conn_ids_issued && self.connections.is_empty(),
            "set_conn_id_base must be called before accepting any connections"
        );
        #[cfg(feature = "tls")]
        assert!(
            self.pending_tls.is_empty(),
            "set_conn_id_base must be called before accepting any connections"
        );
        self.next_conn_id = base;
    }

    /// Enable bounded export of publisher relay frames.
    ///
    /// Disabled by default (`None`): zero extra copies. When enabled, each
    /// publisher frame drained in [`Server::process_connections`] is cloned
    /// into an internal buffer capped by `max_frames` and `max_bytes`. On
    /// overflow the oldest frames are dropped so the buffer stays bounded.
    /// Injected (socket-less) frames are not exported, to avoid echo loops.
    pub fn enable_relay_export(&mut self, max_frames: usize, max_bytes: usize) {
        self.relay_export = Some(RelayExportBuffer::new(max_frames, max_bytes));
    }

    /// Disable relay export and discard any buffered frames.
    pub fn disable_relay_export(&mut self) {
        self.relay_export = None;
    }

    /// Drain and return all currently buffered export frames (empty when
    /// export is disabled).
    pub fn drain_exported_relay_frames(&mut self) -> Vec<RelayFrame> {
        match self.relay_export.as_mut() {
            Some(buf) => buf.drain(),
            None => Vec::new(),
        }
    }

    fn external_publisher_id_for_route(&mut self, app: &str, stream_name: &str) -> u64 {
        allocate_external_publisher_id(
            &mut self.external_route_ids,
            &mut self.external_route_seq,
            app,
            stream_name,
        )
    }

    /// Inject media into the local relay / init-cache / player fan-out path
    /// for `(app, stream_name)` without creating a socket connection.
    ///
    /// Uses the same `cache_relay_frame` + player delivery path as a local
    /// publisher. Frames are tagged with a stable per-route external publisher
    /// id ([`is_external_publisher_id`]). Resource limits mirror per-connection
    /// pending-relay caps (`MAX_PENDING_RELAY_FRAMES` and
    /// `resource_limits.max_pending_relay_bytes`), counting payload plus route
    /// string storage. Payloads larger than [`DEFAULT_MAX_MSG_LENGTH`] (capped
    /// by [`RTMP_WIRE_MAX_MSG_LENGTH`]) and route
    /// components longer than `MAX_INJECT_ROUTE_COMPONENT_BYTES` are rejected.
    /// Empty `app` / `stream_name` are rejected. Conflicts with a socket publisher
    /// (or another external id) are rejected via `active_publish_routes`.
    /// The claim persists until [`Self::release_injected_route`] — including
    /// across empty polls between non-cacheable frames. Callers that cycle
    /// many unique routes **must** release finished feeds; new claims beyond
    /// [`MAX_EXTERNAL_PUBLISH_ROUTES`] are rejected (soft cap, no mid-feed
    /// eviction).
    pub fn inject_relay_frame(
        &mut self,
        app: &str,
        stream_name: &str,
        frame_type: FrameType,
        timestamp: u32,
        payload: &[u8],
    ) -> Result<()> {
        // Cap at the chunk-layer default (not the absolute wire max) so injected
        // frames stay within what typical Conn readers accept on fan-out.
        let max_payload = (DEFAULT_MAX_MSG_LENGTH as usize).min(RTMP_WIRE_MAX_MSG_LENGTH as usize);
        if payload.len() > max_payload {
            return Err(ErrorCode::Internal);
        }
        if app.is_empty() || stream_name.is_empty() {
            return Err(ErrorCode::Internal);
        }
        if app.len() > MAX_INJECT_ROUTE_COMPONENT_BYTES
            || stream_name.len() > MAX_INJECT_ROUTE_COMPONENT_BYTES
        {
            return Err(ErrorCode::Internal);
        }
        let normalized = normalize_modex_payload(payload, CAPS_EX_MASK_MODEX);
        let cache_payload = if normalized.as_ref().len() == payload.len()
            && std::ptr::eq(normalized.as_ref().as_ptr(), payload.as_ptr())
        {
            None
        } else {
            Some(normalized.into_owned())
        };
        let retained_bytes = payload
            .len()
            .saturating_add(cache_payload.as_ref().map(|p| p.len()).unwrap_or(0))
            .saturating_add(app.len())
            .saturating_add(stream_name.len());
        let pending_bytes: usize = self
            .pending_injected_relay
            .iter()
            .map(RelayFrame::retained_bytes)
            .sum();
        // Validate pending-queue budget before allocating a route id so a full
        // backlog cannot leak entries into `external_route_ids`.
        if self.pending_injected_relay.len() >= MAX_PENDING_RELAY_FRAMES
            || pending_bytes.saturating_add(retained_bytes)
                > self.resource_limits.max_pending_relay_bytes
        {
            return Err(ErrorCode::Internal);
        }
        let route_key = (app.to_string(), stream_name.to_string());
        let publisher_conn_id = {
            enum ClaimPlan {
                Existing(u64),
                New,
            }
            let claim_plan = {
                let Ok(routes) = self.active_publish_routes.lock() else {
                    return Err(ErrorCode::Internal);
                };
                match routes.get(&route_key).copied() {
                    Some(owner) => {
                        // Already claimed — reuse only when this external feed owns it.
                        match self.external_route_ids.get(&route_key).copied() {
                            Some(id) if id == owner => ClaimPlan::Existing(id),
                            _ => return Err(ErrorCode::Internal),
                        }
                    }
                    None => {
                        let external_claims = routes
                            .values()
                            .filter(|id| is_external_publisher_id(**id))
                            .count();
                        if external_claims >= MAX_EXTERNAL_PUBLISH_ROUTES {
                            return Err(ErrorCode::Internal);
                        }
                        ClaimPlan::New
                    }
                }
            };
            match claim_plan {
                ClaimPlan::Existing(id) => id,
                ClaimPlan::New => {
                    // Allocate the stable external id only after the claim is
                    // admitted — failed injects must not leak `external_route_ids`.
                    let id = self.external_publisher_id_for_route(app, stream_name);
                    let Ok(mut routes) = self.active_publish_routes.lock() else {
                        return Err(ErrorCode::Internal);
                    };
                    match routes.get(&route_key).copied() {
                        Some(owner) if owner == id => id,
                        Some(_) => return Err(ErrorCode::Internal),
                        None => {
                            routes.insert(route_key.clone(), id);
                            id
                        }
                    }
                }
            }
        };
        self.pending_injected_relay.push(RelayFrame {
            frame_type,
            timestamp,
            payload: payload.to_vec(),
            cache_payload,
            app: app.to_string(),
            stream_name: stream_name.to_string(),
            publisher_conn_id,
        });
        self.inject_route_last_seen
            .insert(route_key, std::time::Instant::now());
        Ok(())
    }

    fn reap_stale_inject_routes(&mut self) {
        let stale = std::time::Duration::from_secs(INJECT_ROUTE_STALE_SECS);
        let now = std::time::Instant::now();
        // Index once — a linear scan per route would be O(routes × frames)
        // on every tight-budget poll while the backlog remains.
        let pending_routes: HashSet<(String, String)> = self
            .pending_injected_relay
            .iter()
            .map(|f| (f.app.clone(), f.stream_name.clone()))
            .collect();
        let expired: Vec<(String, String)> = self
            .inject_route_last_seen
            .iter()
            .filter_map(|(key, seen)| {
                if now.duration_since(*seen) <= stale {
                    return None;
                }
                // Frames still waiting in the inject backlog must not be
                // discarded by the stale reaper — last-seen can age while the
                // poll loop is backlogged.
                if pending_routes.contains(key) {
                    return None;
                }
                Some(key.clone())
            })
            .collect();
        for key in expired {
            self.release_injected_route(&key.0, &key.1);
        }
    }

    /// Release a socket-less inject claim on `(app, stream_name)`.
    ///
    /// External inject claims stay in `active_publish_routes` until this is
    /// called — including across empty polls between non-cacheable frames —
    /// so socket publishers cannot steal the route mid-feed. Call when an
    /// external feed ends (required for continually changing routes; see
    /// [`MAX_EXTERNAL_PUBLISH_ROUTES`]). Also drops any init-cache entry owned
    /// solely by that external id and pending inject frames for the route.
    pub fn release_injected_route(&mut self, app: &str, stream_name: &str) {
        let key = (app.to_string(), stream_name.to_string());
        self.inject_route_last_seen.remove(&key);
        let external_id = self.external_route_ids.remove(&key);
        self.pending_injected_relay
            .retain(|f| !(f.app == key.0 && f.stream_name == key.1));
        let Some(external_id) = external_id else {
            return;
        };
        if let Some(keys) = self.publisher_cache_keys.remove(&external_id) {
            for k in keys {
                let still_owned = self.publisher_cache_keys.values().any(|v| v.contains(&k));
                if !still_owned {
                    self.stream_cache.remove(&k);
                }
            }
        }
        if let Ok(mut routes) = self.active_publish_routes.lock() {
            if routes.get(&key) == Some(&external_id) {
                routes.remove(&key);
            }
        }
    }

    /// Release every socket-less inject claim currently held by this server.
    ///
    /// Convenience for shutdown / remesh; prefer
    /// [`Self::release_injected_route`] per finished feed in normal operation.
    pub fn release_all_injected_routes(&mut self) {
        let external_keys: Vec<(String, String)> = {
            let Ok(routes) = self.active_publish_routes.lock() else {
                return;
            };
            routes
                .iter()
                .filter(|(_, id)| is_external_publisher_id(**id))
                .map(|(k, _)| k.clone())
                .collect()
        };
        for (app, stream_name) in external_keys {
            self.release_injected_route(&app, &stream_name);
        }
        self.external_route_ids.clear();
    }

    /// Copy the current init-cache snapshot for `(app, stream_name)`, if any
    /// entry exists (even when all fields are empty after partial eviction).
    pub fn stream_init_snapshot(&self, app: &str, stream_name: &str) -> Option<StreamInitSnapshot> {
        let cache = self
            .stream_cache
            .get(&(app.to_string(), stream_name.to_string()))?;
        let mut video_track_headers: Vec<(u8, Vec<u8>)> = cache
            .video_track_headers
            .iter()
            .map(|(id, payload)| (*id, payload.clone()))
            .collect();
        video_track_headers.sort_by_key(|(id, _)| *id);
        let mut audio_track_headers: Vec<(u8, Vec<u8>)> = cache
            .audio_track_headers
            .iter()
            .map(|(id, payload)| (*id, payload.clone()))
            .collect();
        audio_track_headers.sort_by_key(|(id, _)| *id);
        Some(StreamInitSnapshot {
            metadata: cache.metadata.clone(),
            avc_header: cache.avc_header.clone(),
            aac_header: cache.aac_header.clone(),
            video_track_headers,
            audio_track_headers,
            last_keyframe: cache.last_keyframe.clone(),
        })
    }

    /// Ask a connected client to reconnect (E-RTMP v2 reconnect mechanism):
    /// sends `NetConnection.Connect.ReconnectRequest` to the connection
    /// identified by `conn_id`. `tc_url` redirects the client to a different
    /// server; `None` tells it to reuse its current connection URL. The
    /// client is expected to keep streaming through the next media boundary
    /// before reconnecting -- this call only sends the request and does not
    /// close `conn_id` itself. Returns `Err(ErrorCode::Internal)` if no
    /// connection with `conn_id` is currently held by this server.
    pub fn request_reconnect(
        &mut self,
        conn_id: u64,
        tc_url: Option<&str>,
        description: Option<&str>,
    ) -> Result<()> {
        let Some(conn) = self.connections.iter_mut().find(|c| c.conn_id == conn_id) else {
            return Err(ErrorCode::Internal);
        };
        conn.send_reconnect_request(tc_url, description)?;
        conn.flush()
    }

    /// Resolve a "host:port" (default port 1935) string into a bindable address.
    fn resolve_bind_addr(bind_addr: &str) -> Result<String> {
        let mut host = String::new();
        let mut port = String::new();
        net::split_host_port(bind_addr, &mut host, &mut port, "1935")?;
        Ok(if host.is_empty() {
            format!("0.0.0.0:{port}")
        } else if host.contains(':') {
            format!("[{host}]:{port}")
        } else {
            format!("{host}:{port}")
        })
    }

    fn bind_listener(&mut self, addr: &str) -> Result<TcpListener> {
        let listener = TcpListener::bind(addr).map_err(|_| ErrorCode::Io)?;
        listener.set_nonblocking(true).map_err(|_| ErrorCode::Io)?;
        if self.server_fd < 0 {
            self.server_fd = listener.as_raw_fd();
        }
        self.running = true;
        Ok(listener)
    }

    /// Return the file descriptor for every currently bound listener.
    ///
    /// Use this instead of the legacy [`Server::server_fd`] field when an
    /// external readiness loop needs to watch a multi-listener `Server`.
    pub fn listener_fds(&self) -> Vec<i32> {
        self.listeners
            .iter()
            .map(|listener| listener.tcp.as_raw_fd())
            .collect()
    }

    /// Start listening on the given address ("host:port", default port 1935).
    ///
    /// Uses the TLS/plaintext mode selected at construction time via
    /// [`ServerConfig::tls_enabled`]. Can be called more than once (e.g. to
    /// also bind an IPv6 address); every bound listener shares this `Server`'s
    /// connections, media relay, and stream cache. To add a listener with an
    /// *independent* TLS certificate — e.g. plaintext RTMP plus RTMPS on a
    /// second port — use [`Server::listen_tls`] instead.
    pub fn listen(&mut self, bind_addr: &str) -> Result<()> {
        let addr = Self::resolve_bind_addr(bind_addr)?;
        let tcp = self.bind_listener(&addr)?;
        self.listeners.push(ListenerEntry {
            tcp,
            tls_ctx: self.tls_ctx.clone(),
        });
        Ok(())
    }

    /// Start an additional RTMPS listener with its own certificate/key,
    /// independent of the TLS/plaintext mode passed to [`Server::new`].
    ///
    /// Connections accepted here land in the same `connections` list as every
    /// other listener on this `Server`, so a publisher here is relayed to
    /// players on any other listener (plaintext or TLS) and vice versa.
    pub fn listen_tls(&mut self, bind_addr: &str, cert_file: &str, key_file: &str) -> Result<()> {
        let ctx = TlsCtx::new_server(cert_file, key_file)?;
        let addr = Self::resolve_bind_addr(bind_addr)?;
        let tcp = self.bind_listener(&addr)?;
        self.listeners.push(ListenerEntry {
            tcp,
            tls_ctx: Some(ctx),
        });
        Ok(())
    }

    #[cfg(all(test, feature = "tls"))]
    fn pending_tls_count_for_addr(&self, remote_addr: &str) -> usize {
        let remote_ip = Self::peer_ip(remote_addr);
        self.pending_tls
            .iter()
            .filter(|pending| pending.remote_ip == remote_ip)
            .count()
    }

    #[cfg(all(test, feature = "tls"))]
    fn pending_tls_count(&self) -> usize {
        self.pending_tls.len()
    }

    /// Poll for events (non-blocking).
    pub fn poll(&mut self, timeout_ms: i32) -> Result<()> {
        if !self.running {
            return Err(ErrorCode::Internal);
        }
        // Reap stale/timed-out connections before enforcing per-IP/global
        // admission caps below -- otherwise a peer reconnecting from the
        // same source IP right as its old sockets die can be rejected by
        // `max_connections_per_addr` for connections that are about to be
        // removed in this very tick anyway.
        self.process_connections()?;
        self.accept_new_connections();
        // Give sockets accepted just above their first processing pass in
        // this same poll() call, rather than leaving them untouched through
        // the sleep below until the next call -- otherwise every new
        // connection's handshake is delayed by a full poll cycle.
        self.process_connections()?;
        if timeout_ms > 0 {
            std::thread::sleep(std::time::Duration::from_millis(timeout_ms as u64));
        }
        Ok(())
    }

    /// Stop the server.
    pub fn stop(&mut self) {
        self.running = false;
        self.listeners.clear();
        self.next_listener_accept = 0;
        #[cfg(feature = "tls")]
        self.pending_tls.clear();
        // bind_listener() only assigns server_fd when it's negative, so a
        // later listen() call after stop() must see it reset here or it
        // would keep exposing the now-closed fd from before this stop().
        self.server_fd = -1;
    }

    fn total_connection_slots_in_use(&self) -> usize {
        let mut slots = self.connections.len();
        #[cfg(feature = "tls")]
        {
            slots += self.pending_tls.len();
        }
        slots
    }

    fn max_connections_reached(&self) -> bool {
        self.config.max_connections > 0
            && self.total_connection_slots_in_use() >= self.config.max_connections as usize
    }

    #[cfg(feature = "tls")]
    fn tls_handshake_deadline() -> Instant {
        Instant::now() + Duration::from_secs(TLS_HANDSHAKE_TIMEOUT_SECS)
    }

    #[cfg(feature = "tls")]
    fn pending_tls_limit_reached(&self) -> bool {
        if self.config.max_connections > 0 {
            self.max_connections_reached()
        } else {
            self.pending_tls.len() >= MAX_PENDING_TLS_HANDSHAKES
        }
    }

    /// Canonical peer host for per-IP caps: strips the port from a
    /// `SocketAddr::to_string()` value and maps IPv4-mapped IPv6 addresses
    /// (e.g. `[::ffff:203.0.113.5]:5678`) to their IPv4 form so dual-stack
    /// listeners cannot be bypassed by choosing a different address family.
    fn peer_ip(remote_addr: &str) -> String {
        if let Ok(sock) = remote_addr.parse::<std::net::SocketAddr>() {
            return Self::canonical_ip(sock.ip());
        }
        remote_addr
            .rsplit_once(':')
            .map(|(ip, _port)| ip)
            .unwrap_or(remote_addr)
            .to_string()
    }

    fn canonical_ip(ip: std::net::IpAddr) -> String {
        match ip {
            std::net::IpAddr::V4(v4) => v4.to_string(),
            std::net::IpAddr::V6(v6) => v6
                .to_ipv4_mapped()
                .map(|mapped| mapped.to_string())
                .unwrap_or_else(|| v6.to_string()),
        }
    }

    fn max_connections_per_addr(&self) -> usize {
        if self.config.max_connections_per_addr > 0 {
            self.config.max_connections_per_addr as usize
        } else {
            DEFAULT_MAX_CONNECTIONS_PER_ADDR
        }
    }

    fn active_connections_for_ip(&self, remote_ip: &str) -> usize {
        self.connections
            .iter()
            .filter(|conn| Self::peer_ip(&conn.remote_addr) == remote_ip)
            .count()
    }

    #[cfg(feature = "tls")]
    fn max_pending_tls_per_addr(&self) -> usize {
        if self.config.max_pending_tls_per_addr > 0 {
            self.config.max_pending_tls_per_addr as usize
        } else {
            DEFAULT_MAX_PENDING_TLS_PER_ADDR
        }
    }

    #[cfg(feature = "tls")]
    fn queue_pending_tls(&mut self, conn: PendingTlsConnection) {
        let same_addr = self
            .pending_tls
            .iter()
            .filter(|pending| pending.remote_ip == conn.remote_ip)
            .count();
        if same_addr >= self.max_pending_tls_per_addr() {
            // Drop the new stalled handshake instead of evicting an existing
            // pending entry from the same IP -- otherwise a co-located attacker
            // can repeatedly open stalled RTMPS connects and deny TLS completion
            // for legitimate peers behind the same NAT/egress address.
            return;
        }
        if self.pending_tls_limit_reached() {
            // Drop the new stalled handshake instead of evicting an unrelated
            // peer's pending entry from the front of the queue.
            return;
        }
        self.pending_tls.push(conn);
    }

    fn allocate_conn_id(&mut self) -> Option<u64> {
        let conn_id = self.next_conn_id;
        // Reserve the high bit for socket-less inject publisher ids so a
        // socket conn_id can never collide with `is_external_publisher_id`.
        if conn_id == 0 || conn_id >= EXTERNAL_PUBLISHER_ID_BIT {
            return None;
        }
        self.next_conn_id = conn_id + 1;
        self.conn_ids_issued = true;
        Some(conn_id)
    }

    fn add_connection(&mut self, transport: Transport, remote_addr: String) -> bool {
        if self.max_connections_reached() {
            return false;
        }
        let remote_ip = Self::peer_ip(&remote_addr);
        if self.active_connections_for_ip(&remote_ip) >= self.max_connections_per_addr() {
            return false;
        }
        let Some(conn_id) = self.allocate_conn_id() else {
            return false;
        };

        let conn_fd = transport.fd();
        let mut conn = Conn::new();
        conn.chunk_reg.max_reassembly_bytes = self.resource_limits.max_reassembly_bytes;
        conn.max_pending_relay_bytes = self.resource_limits.max_pending_relay_bytes;
        // Outbound chunk size only: peers start sending at the RTMP
        // default (128) until SetChunkSize is negotiated.
        conn.chunk_size = if self.config.chunk_size > 0 {
            self.config.chunk_size as u32
        } else {
            DEFAULT_CHUNK_SIZE
        };
        conn.client_fd = conn_fd;
        conn.conn_id = conn_id;
        conn.remote_addr = remote_addr;
        conn.defer_media_relay = self.defer_media_relay;
        conn.transport = Some(transport);
        conn.on_frame_cb = self.on_frame_cb;
        conn.on_media_cb = self.on_media_cb;
        conn.on_connect_cb = self.on_connect_cb;
        conn.on_publish_cb = self.on_publish_cb;
        conn.on_play_cb = self.on_play_cb;
        conn.on_shared_object_auth_cb = self.on_shared_object_auth_cb;
        conn.on_shared_object_cb = self.on_shared_object_cb;
        conn.on_release_stream_cb = self.on_release_stream_cb;
        conn.publish_routes = Some(PublishRouteRegistry::new(Arc::clone(
            &self.active_publish_routes,
        )));
        self.connections.push(conn);
        true
    }

    #[cfg(feature = "tls")]
    fn progress_pending_tls(&mut self) {
        let pending = std::mem::take(&mut self.pending_tls);
        let now = Instant::now();
        for pending_conn in pending {
            if now >= pending_conn.deadline {
                continue;
            }
            if self.max_connections_reached() {
                self.pending_tls.push(pending_conn);
                continue;
            }

            match pending_conn.handshake.progress() {
                Ok(TlsAcceptOutcome::Complete(transport)) => {
                    self.add_connection(transport, pending_conn.remote_addr);
                }
                Ok(TlsAcceptOutcome::WouldBlock(handshake)) => {
                    if Instant::now() < pending_conn.deadline {
                        self.pending_tls.push(PendingTlsConnection {
                            handshake,
                            remote_addr: pending_conn.remote_addr,
                            remote_ip: pending_conn.remote_ip,
                            deadline: pending_conn.deadline,
                        });
                    }
                }
                Err(_) => {}
            }
        }
    }

    #[cfg(not(feature = "tls"))]
    fn progress_pending_tls(&mut self) {}

    /// Accept any pending inbound connections on every bound listener.
    ///
    /// Both TCP accept and TLS handshakes are driven non-blockingly. TLS
    /// handshakes that need more bytes are retried on later `poll()` calls so a
    /// stalled RTMPS peer cannot freeze other listeners or active sessions.
    fn accept_new_connections(&mut self) {
        self.progress_pending_tls();

        let listener_count = self.listeners.len();
        if listener_count == 0 {
            self.next_listener_accept = 0;
            return;
        }
        self.next_listener_accept %= listener_count;

        let mut accepts_serviced = 0usize;
        loop {
            let mut accepted_any = false;

            for offset in 0..listener_count {
                if self.max_connections_reached() {
                    return;
                }
                if accepts_serviced >= MAX_ACCEPTS_PER_POLL {
                    return;
                }
                let i = (self.next_listener_accept + offset) % listener_count;
                match self.listeners[i].tcp.accept() {
                    Ok((stream, addr)) => {
                        accepted_any = true;
                        accepts_serviced += 1;
                        self.next_listener_accept = (i + 1) % listener_count;
                        let remote_addr = addr.to_string();
                        let tls_ctx = self.listeners[i].tls_ctx.clone();
                        if let Some(ctx) = tls_ctx.as_ref() {
                            #[cfg(feature = "tls")]
                            {
                                match ctx.accept_nonblocking(stream.into_raw_fd()) {
                                    Ok(TlsAcceptOutcome::Complete(transport)) => {
                                        self.add_connection(transport, remote_addr);
                                    }
                                    Ok(TlsAcceptOutcome::WouldBlock(handshake)) => {
                                        let remote_ip = Self::peer_ip(&remote_addr);
                                        self.queue_pending_tls(PendingTlsConnection {
                                            handshake,
                                            remote_addr,
                                            remote_ip,
                                            deadline: Self::tls_handshake_deadline(),
                                        });
                                    }
                                    Err(_) => {}
                                }
                            }
                            #[cfg(not(feature = "tls"))]
                            {
                                let _ = ctx;
                                drop(stream);
                            }
                        } else {
                            let _ = stream.set_nonblocking(true);
                            let transport = Transport::new_plain(stream.into_raw_fd());
                            self.add_connection(transport, remote_addr);
                        }
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                    Err(_) => {}
                }
            }

            if !accepted_any {
                break;
            }
        }
    }

    /// Process all active connections: drain readable bytes, drive the
    /// protocol state machine, relay frames from publishers to players,
    /// flush pending writes, and reap closed peers.
    pub fn process_connections(&mut self) -> Result<()> {
        let mut buf = [0u8; 65536];
        let mut closed = Vec::new();

        // Drive recv/processing for every connection.
        self.drain_pending_cache_evictions();
        // Reap expired external inject claims *before* receiving publish
        // commands — otherwise a socket publisher on a stale inject route
        // gets BadName in this poll and never retries after the reaper runs.
        self.reap_stale_inject_routes();
        // Cloned so a closed connection's publish route can be released
        // immediately (below) without conflicting with the mutable borrow
        // of `self.connections` this loop holds via `iter_mut()`.
        let active_publish_routes = Arc::clone(&self.active_publish_routes);
        for (i, conn) in self.connections.iter_mut().enumerate() {
            let mut conn_closed_this_iteration = false;
            if conn.session_setup_timed_out() {
                conn.disconnect_transport();
                closed.push(i);
                conn_closed_this_iteration = true;
            }
            let mut bytes_drained = 0usize;
            while !conn_closed_this_iteration {
                if bytes_drained >= MAX_RECV_BYTES_PER_CONN_PER_POLL {
                    break;
                }
                let Some(transport) = conn.transport.as_mut() else {
                    closed.push(i);
                    conn_closed_this_iteration = true;
                    break;
                };
                let mut again = 0i32;
                let n = transport.recv(&mut buf, &mut again);
                if n > 0 {
                    let chunk_len = n as usize;
                    if conn.recv(&buf[..chunk_len]).is_err() {
                        closed.push(i);
                        conn_closed_this_iteration = true;
                        break;
                    }
                    bytes_drained += chunk_len;
                } else if n == 0 {
                    closed.push(i);
                    conn_closed_this_iteration = true;
                    break;
                } else if again != 0 {
                    break;
                } else {
                    closed.push(i);
                    conn_closed_this_iteration = true;
                    break;
                }
            }
            // A batch larger than the per-`recv` message budget leaves
            // complete messages buffered but unprocessed; keep draining them
            // with no new bytes instead of waiting on the peer to send more,
            // which may never happen if it's waiting on our response. Capped
            // so one connection with a huge batch can't monopolize this poll
            // tick -- any remainder is picked up on the next poll() call.
            for _ in 0..MAX_BUDGET_DRAIN_PASSES_PER_CONN_PER_POLL {
                if conn_closed_this_iteration || !conn.has_buffered_messages() {
                    break;
                }
                if conn.recv(&[]).is_err() {
                    closed.push(i);
                    conn_closed_this_iteration = true;
                    break;
                }
            }
            // Release this connection's claimed publish route(s) right away
            // rather than deferring to the end-of-batch cleanup below: a
            // later connection processed in this same loop may try to
            // publish the same (app, stream) route, and PublishRouteRegistry
            // must not see this now-closed connection as the stale owner.
            if conn_closed_this_iteration {
                if let Ok(mut routes) = active_publish_routes.lock() {
                    routes.retain(|_, owner| *owner != conn.conn_id);
                }
            }
        }

        // Ordering within this function is deliberate: evictions from
        // renames processed by the recv loop just above are applied here,
        // *after* that recv loop but *before* both init-frame replay and
        // caching of this batch's freshly-relayed frames below. This means
        // init-frame replay never sees a cache entry under a route key its
        // publisher abandoned earlier in this same batch, and a same-batch
        // rename can't leave a stale entry alive until the next poll() call.
        //
        // The returned set also guards the caching step below: a frame
        // queued (via pending_relay) under the old route *before* its
        // publisher's rename was processed in this same recv loop must not
        // resurrect the entry we just evicted. It's keyed by (app, name,
        // conn_id) -- not just (app, name) -- so this only suppresses
        // caching for the connection that actually abandoned the route;
        // a *different* publisher's legitimate same-batch frame for an
        // identical (app, name) is unaffected.
        let abandoned_this_batch = self.drain_pending_cache_evictions();

        // Interleave socket-less injects with local publisher frames so neither
        // source starves the other under a tight relay-send budget. Relative
        // order within each source is preserved.
        // (Stale inject claims were already reaped at the start of this poll.)
        // When defer_media_relay clears relay_enabled (FCUnpublish/deleteStream)
        // after media was already queued, drop abandoned-route frames so they
        // cannot resurrect after a later reauth. Normal (non-deferred)
        // publishers must still fan out / export frames already accepted in
        // this batch — abandonment only skips init-cache writes for them.
        // Do not wipe the whole queue when injects arrived before publish:
        // publish resets `injected_media_bytes` for the media deadline while
        // those frames are still pending.
        for conn in &mut self.connections {
            if conn.defer_media_relay
                && !conn.relay_enabled
                && !abandoned_this_batch.is_empty()
            {
                let conn_id = conn.conn_id;
                conn.pending_relay.retain(|f| {
                    !abandoned_this_batch.contains(&(f.app.clone(), f.stream_name.clone(), conn_id))
                });
            }
            if conn.defer_media_relay
                && !conn.relay_enabled
                && conn.injected_media_bytes == 0
                && conn.pending_relay.is_empty()
            {
                continue;
            }
        }
        // Round-robin across local publishers (not flat_map by connection
        // order) so the first publisher cannot monopolize the send budget.
        let local_queues: Vec<Vec<_>> = self
            .connections
            .iter_mut()
            .filter(|c| {
                !c.defer_media_relay
                    || c.relay_enabled
                    || c.injected_media_bytes > 0
                    || !c.pending_relay.is_empty()
            })
            .map(|c| c.pending_relay.drain(..).collect::<Vec<_>>())
            .filter(|q| !q.is_empty())
            .collect();
        let local_rr_start = if local_queues.is_empty() {
            0
        } else {
            let start = self.relay_local_rr_offset % local_queues.len();
            self.relay_local_rr_offset = self.relay_local_rr_offset.wrapping_add(1);
            start
        };
        let mut local_frames = round_robin_relay_queues(local_queues, local_rr_start);
        if !self.orphaned_relay.is_empty() {
            let orphan = std::mem::take(&mut self.orphaned_relay);
            // Orphans are already-accepted local media — prepend so they are
            // not starved behind a fresh publisher's same-poll frames.
            let mut merged = orphan;
            merged.append(&mut local_frames);
            local_frames = merged;
        }
        let injected_frames = std::mem::take(&mut self.pending_injected_relay);
        let inject_first = self.relay_interleave_inject_first;
        self.relay_interleave_inject_first = !inject_first;
        let mut relay_frames =
            merge_local_and_injected_by_route(local_frames, injected_frames, inject_first);

        // Replay cached codec headers and last keyframe to newly-joined players
        // using the pre-batch cache state, so init frames always precede live
        // frames from the current batch.
        for (i, conn) in self.connections.iter_mut().enumerate() {
            if conn.transport.is_none() || !conn.needs_init_frames {
                continue;
            }
            let Some(ref stream) = conn.current_stream else {
                continue;
            };
            if !stream.is_playing || !conn.relay_enabled {
                continue;
            }
            conn.needs_init_frames = false;
            let key = (conn.app.clone(), conn.relay_route_key());
            let receive_audio = conn
                .current_stream
                .as_ref()
                .map(|s| s.receive_audio)
                .unwrap_or(true);
            let receive_video = conn
                .current_stream
                .as_ref()
                .map(|s| s.receive_video)
                .unwrap_or(true);
            if let Some(cache) = self.stream_cache.get(&key) {
                let mut send_failed = false;
                if (receive_audio || receive_video)
                    && let Some(ref md) = cache.metadata.clone()
                {
                    send_failed |= conn.send_data_message(0, md).is_err();
                }
                if receive_video {
                    if let Some(ref hdr) = cache.avc_header.clone() {
                        if !Self::cached_payload_is_multitrack(FrameType::Video, hdr)
                            || conn.accepts_multitrack()
                        {
                            send_failed |= conn.send_frame(FrameType::Video, 0, hdr).is_err();
                        }
                    }
                    for hdr in cache.video_track_headers.values() {
                        if conn.accepts_multitrack() {
                            send_failed |= conn.send_frame(FrameType::Video, 0, hdr).is_err();
                        }
                    }
                }
                if receive_audio && !send_failed {
                    if let Some(ref hdr) = cache.aac_header.clone() {
                        if !Self::cached_payload_is_multitrack(FrameType::Audio, hdr)
                            || conn.accepts_multitrack()
                        {
                            send_failed |= conn.send_frame(FrameType::Audio, 0, hdr).is_err();
                        }
                    }
                    for hdr in cache.audio_track_headers.values() {
                        if conn.accepts_multitrack() {
                            send_failed |= conn.send_frame(FrameType::Audio, 0, hdr).is_err();
                        }
                    }
                }
                if receive_video && !send_failed {
                    if let Some((ts, ref kf)) = cache.last_keyframe.clone() {
                        if !Self::cached_payload_is_multitrack(FrameType::Video, kf)
                            || conn.accepts_multitrack()
                        {
                            send_failed |= conn.send_frame(FrameType::Video, ts, kf).is_err();
                        }
                    }
                }
                if send_failed {
                    // Cached init-frame replay filled the send buffer for a
                    // slow player. Close immediately just like live relay sends.
                    conn.relay_enabled = false;
                    conn.needs_init_frames = false;
                    conn.disconnect_transport();
                    closed.push(i);
                }
            }
        }

        // Update per-stream cache and relay each frame in order so players
        // receive frames in the same sequence the publisher sent them.
        let mut relay_sends = 0usize;
        let mut relay_processed = 0usize;
        for frame in &relay_frames {
            let player_count = self.count_relay_players(frame);
            if player_count > 0
                && relay_sends > 0
                && relay_sends.saturating_add(player_count) > self.max_relay_sends_per_poll
            {
                break;
            }

            let abandon_key = (
                frame.app.clone(),
                frame.stream_name.clone(),
                frame.publisher_conn_id,
            );
            if !abandoned_this_batch.contains(&abandon_key) {
                // Orphaned local frames must not recreate stream-cache ownership
                // for a publisher already removed from `connections`. External
                // inject ids have no socket row and still cache.
                let local_publisher_gone = !is_external_publisher_id(frame.publisher_conn_id)
                    && !self
                        .connections
                        .iter()
                        .any(|c| c.conn_id == frame.publisher_conn_id);
                if !local_publisher_gone {
                    self.cache_relay_frame(frame);
                }
            }
            for (i, conn) in self.connections.iter_mut().enumerate() {
                if !Self::conn_will_receive_relay_frame(conn, frame) {
                    continue;
                }
                let send_result = match frame.frame_type {
                    FrameType::Script | FrameType::Metadata => {
                        conn.send_data_message(frame.timestamp, &frame.payload)
                    }
                    _ => conn.send_frame(frame.frame_type, frame.timestamp, &frame.payload),
                };
                if send_result.is_err() {
                    // Player stopped reading; outbound send_buffer is full.
                    // Drop the connection immediately so later relay frames in
                    // this poll batch skip it and no more socket work is done.
                    conn.relay_enabled = false;
                    conn.needs_init_frames = false;
                    conn.disconnect_transport();
                    closed.push(i);
                }
            }
            relay_sends += player_count;
            relay_processed += 1;
        }
        // Export only frames that completed this poll (not requeued). Injected
        // frames stay off the export path to avoid remote→local echo loops.
        if let Some(export) = self.relay_export.as_mut() {
            for frame in &relay_frames[..relay_processed] {
                if !is_external_publisher_id(frame.publisher_conn_id) {
                    export.push_clone(frame);
                }
            }
        }
        for frame in relay_frames.drain(relay_processed..) {
            self.requeue_relay_frame(frame);
        }

        // Flush all connections.
        for (i, conn) in self.connections.iter_mut().enumerate() {
            if conn.transport.is_none() {
                closed.push(i);
                continue;
            }
            if conn.maybe_send_ping().is_err() {
                closed.push(i);
                continue;
            }
            if conn.flush().is_err() {
                closed.push(i);
            }
        }

        // A connection that errors on both recv and flush gets pushed twice.
        // Sort then dedup so each index is removed exactly once.
        closed.sort_unstable();
        closed.dedup();
        for i in closed.into_iter().rev() {
            // Budget-deferred local frames stay on the connection until teardown;
            // push to orphaned_relay only (no export here). The completed-frame
            // export path above exports once when they fan out next poll.
            let pending: Vec<_> = self.connections[i].pending_relay.drain(..).collect();
            for frame in pending {
                if !is_external_publisher_id(frame.publisher_conn_id) {
                    self.push_orphaned_relay(frame);
                }
            }
            let conn = &self.connections[i];
            self.release_all_publish_routes(conn.conn_id);
            // Tracking must be cleared unconditionally: a publisher can issue
            // another createStream after publishing, replacing current_stream
            // and leaving is_publishing false even though this conn_id still
            // owns cache entries.
            if let Some(keys) = self.publisher_cache_keys.remove(&conn.conn_id) {
                for key in keys {
                    // Two connections can end up tracking the same (app,
                    // stream_name) key (e.g. a stale entry left behind by a
                    // publisher that renamed via createStream, then a
                    // different connection republished under that same
                    // name). Only actually drop the cache entry once no
                    // other tracked publisher still claims it, or we'd wipe
                    // out a still-active publisher's cached headers.
                    let still_owned = self.publisher_cache_keys.values().any(|v| v.contains(&key));
                    if !still_owned {
                        self.stream_cache.remove(&key);
                    }
                }
            }
            self.connections.remove(i);
        }
        Ok(())
    }

    fn conn_will_receive_relay_frame(conn: &Conn, frame: &RelayFrame) -> bool {
        let Some(stream) = conn.current_stream.as_ref() else {
            return false;
        };
        if !conn.relay_enabled
            || conn.transport.is_none()
            || conn.app != frame.app
            || !stream.is_playing
            || conn.relay_route_key() != frame.stream_name
            || stream.paused
        {
            return false;
        }
        if frame.frame_type == FrameType::Audio && !stream.receive_audio {
            return false;
        }
        if frame.frame_type == FrameType::Video && !stream.receive_video {
            return false;
        }
        if matches!(frame.frame_type, FrameType::Script | FrameType::Metadata)
            && !stream.receive_audio
            && !stream.receive_video
        {
            return false;
        }
        if matches!(frame.frame_type, FrameType::Audio | FrameType::Video)
            && is_multitrack_container(frame.frame_type, frame.cache_payload())
            && !conn.accepts_multitrack()
        {
            return false;
        }
        true
    }

    fn count_relay_players(&self, frame: &RelayFrame) -> usize {
        self.connections
            .iter()
            .filter(|conn| Self::conn_will_receive_relay_frame(conn, frame))
            .count()
    }

    fn requeue_relay_frame(&mut self, frame: RelayFrame) {
        if is_external_publisher_id(frame.publisher_conn_id) {
            self.pending_injected_relay.push(frame);
            return;
        }
        if let Some(conn) = self
            .connections
            .iter_mut()
            .find(|conn| conn.conn_id == frame.publisher_conn_id)
        {
            conn.pending_relay.push(frame);
            return;
        }
        // Publisher already gone this poll — keep for player fan-out next tick.
        self.push_orphaned_relay(frame);
    }

    /// Cap `orphaned_relay` by frame count and byte budget; drop-oldest until
    /// `frame` fits. If `frame` alone exceeds capacity, drop it. Dropped local
    /// frames are still exported once so HA consumers do not permanently miss
    /// media that was accepted before the backlog bound kicked in.
    fn push_orphaned_relay(&mut self, frame: RelayFrame) {
        let frame_bytes = frame.retained_bytes();
        let max_bytes = self.resource_limits.max_pending_relay_bytes;
        if frame_bytes > max_bytes || MAX_PENDING_RELAY_FRAMES == 0 {
            self.export_orphaned_relay_frame(&frame);
            return;
        }
        let mut pending_bytes: usize = self
            .orphaned_relay
            .iter()
            .map(RelayFrame::retained_bytes)
            .sum();
        while !self.orphaned_relay.is_empty()
            && (self.orphaned_relay.len() >= MAX_PENDING_RELAY_FRAMES
                || pending_bytes.saturating_add(frame_bytes) > max_bytes)
        {
            let dropped = self.orphaned_relay.remove(0);
            pending_bytes = pending_bytes.saturating_sub(dropped.retained_bytes());
            self.export_orphaned_relay_frame(&dropped);
        }
        if self.orphaned_relay.len() >= MAX_PENDING_RELAY_FRAMES
            || pending_bytes.saturating_add(frame_bytes) > max_bytes
        {
            self.export_orphaned_relay_frame(&frame);
            return;
        }
        self.orphaned_relay.push(frame);
    }

    fn export_orphaned_relay_frame(&mut self, frame: &RelayFrame) {
        if is_external_publisher_id(frame.publisher_conn_id) {
            return;
        }
        if let Some(export) = self.relay_export.as_mut() {
            export.push_clone(frame);
        }
    }

    /// Bytes retained by a single stream_cache entry, including HashMap key
    /// string storage for `(app, stream_name)`.
    fn stream_cache_entry_bytes(key: &(String, String), cache: &StreamCache) -> usize {
        key.0
            .len()
            .saturating_add(key.1.len())
            .saturating_add(cache.avc_header.as_ref().map(|v| v.len()).unwrap_or(0))
            .saturating_add(
                cache
                    .video_track_headers
                    .values()
                    .map(|v| v.len())
                    .sum::<usize>(),
            )
            .saturating_add(cache.aac_header.as_ref().map(|v| v.len()).unwrap_or(0))
            .saturating_add(
                cache
                    .audio_track_headers
                    .values()
                    .map(|v| v.len())
                    .sum::<usize>(),
            )
            .saturating_add(cache.metadata.as_ref().map(|v| v.len()).unwrap_or(0))
            .saturating_add(
                cache
                    .last_keyframe
                    .as_ref()
                    .map(|(_, v)| v.len())
                    .unwrap_or(0),
            )
    }

    /// Total bytes currently retained across all stream_cache entries.
    fn stream_cache_bytes(&self) -> usize {
        self.stream_cache
            .iter()
            .map(|(key, cache)| Self::stream_cache_entry_bytes(key, cache))
            .sum()
    }

    fn evict_stream_cache_key(&mut self, key: &(String, String)) {
        self.stream_cache.remove(key);
        for keys in self.publisher_cache_keys.values_mut() {
            keys.retain(|k| k != key);
        }
        // External inject has no connection teardown; drop empty owner rows so
        // `publisher_cache_keys` cannot grow without bound across routes.
        // Publish-route claims stay until `release_injected_route` so a
        // continuous feed is not stolen between non-cacheable frames.
        self.publisher_cache_keys.retain(|_, keys| !keys.is_empty());
    }

    /// Drop another cache entry owned by `publisher_conn_id`, if any.
    fn evict_stream_cache_for_publisher(
        &mut self,
        publisher_conn_id: u64,
        except_key: &(String, String),
    ) -> bool {
        let Some(keys) = self.publisher_cache_keys.get(&publisher_conn_id).cloned() else {
            return false;
        };
        for key in keys {
            if &key != except_key {
                self.evict_stream_cache_key(&key);
                return true;
            }
        }
        false
    }

    /// Evict any external-owned cache route other than `except_key`.
    ///
    /// Per-route external publisher IDs mean same-owner eviction cannot free
    /// older inject routes when the global entry/byte cap is hit.
    fn evict_any_external_stream_cache(&mut self, except_key: &(String, String)) -> bool {
        let mut victim: Option<(String, String)> = None;
        for (pub_id, keys) in &self.publisher_cache_keys {
            if !is_external_publisher_id(*pub_id) {
                continue;
            }
            if let Some(key) = keys.iter().find(|k| *k != except_key) {
                victim = Some(key.clone());
                break;
            }
        }
        if let Some(key) = victim {
            self.evict_stream_cache_key(&key);
            return true;
        }
        false
    }

    /// Prefer same-owner eviction; fall back to other external routes.
    fn evict_for_stream_cache_pressure(
        &mut self,
        publisher_conn_id: u64,
        except_key: &(String, String),
    ) -> bool {
        self.evict_stream_cache_for_publisher(publisher_conn_id, except_key)
            || self.evict_any_external_stream_cache(except_key)
    }

    fn publisher_cache_key_count(&self, publisher_conn_id: u64) -> usize {
        self.publisher_cache_keys
            .get(&publisher_conn_id)
            .map(|keys| keys.len())
            .unwrap_or(0)
    }

    fn stream_cache_is_empty(cache: &StreamCache) -> bool {
        cache.avc_header.is_none()
            && cache.video_track_headers.is_empty()
            && cache.aac_header.is_none()
            && cache.audio_track_headers.is_empty()
            && cache.metadata.is_none()
            && cache.last_keyframe.is_none()
    }

    fn cached_payload_is_multitrack(frame_type: FrameType, payload: &[u8]) -> bool {
        let normalized = normalize_modex_payload(payload, CAPS_EX_MASK_MODEX);
        is_multitrack_container(frame_type, normalized.as_ref())
    }

    fn multitrack_sequence_track_ids(frame_type: FrameType, payload: &[u8]) -> Vec<u8> {
        let mut ids = Vec::new();
        if is_multitrack_container(frame_type, payload) {
            foreach_track(frame_type, payload, |track| {
                if track.packet_type == 0 {
                    ids.push(track.track_id);
                }
            });
        }
        ids
    }

    fn reserve_stream_cache_storage(
        &mut self,
        key: &(String, String),
        incoming_len: usize,
        existing_field_len: usize,
        publisher_conn_id: u64,
    ) -> bool {
        let is_new_key = !self.stream_cache.contains_key(key);
        let route_key_bytes = if is_new_key {
            key.0.len().saturating_add(key.1.len())
        } else {
            0
        };
        let max_cache_bytes = self.resource_limits.max_stream_cache_bytes;
        // This reservation's own contribution cannot shrink by evicting peers.
        // Include other fields already retained on the same route so we reject
        // before wiping unrelated streams when the updated entry still cannot fit.
        let irreducible = if let Some(cache) = self.stream_cache.get(key) {
            Self::stream_cache_entry_bytes(key, cache)
                .saturating_sub(existing_field_len)
                .saturating_add(incoming_len)
        } else {
            incoming_len.saturating_add(route_key_bytes)
        };
        if irreducible > max_cache_bytes {
            return false;
        }

        let mut projected_total = self
            .stream_cache_bytes()
            .saturating_add(incoming_len)
            .saturating_add(route_key_bytes)
            .saturating_sub(existing_field_len);
        // Refuse before wiping peer caches when even clearing every route
        // eviction can actually free (same-publisher peers + other
        // external routes) still cannot fit this reservation. Run this before
        // per-publisher / entry-cap eviction so a valid victim is not deleted
        // when the new entry still cannot fit.
        if projected_total > max_cache_bytes {
            let freeable = self.stream_cache_freeable_bytes(key, publisher_conn_id);
            if projected_total.saturating_sub(freeable) > max_cache_bytes {
                return false;
            }
        }

        if is_new_key
            && self.publisher_cache_key_count(publisher_conn_id)
                >= MAX_STREAM_CACHE_KEYS_PER_PUBLISHER
        {
            if !self.evict_for_stream_cache_pressure(publisher_conn_id, key) {
                return false;
            }
            let add_key_bytes = if !self.stream_cache.contains_key(key) {
                key.0.len().saturating_add(key.1.len())
            } else {
                0
            };
            projected_total = self
                .stream_cache_bytes()
                .saturating_add(incoming_len)
                .saturating_add(add_key_bytes)
                .saturating_sub(existing_field_len);
        }

        if self.stream_cache.len() >= MAX_STREAM_CACHE_ENTRIES && is_new_key {
            if !self.evict_for_stream_cache_pressure(publisher_conn_id, key) {
                return false;
            }
            let add_key_bytes = if !self.stream_cache.contains_key(key) {
                key.0.len().saturating_add(key.1.len())
            } else {
                0
            };
            projected_total = self
                .stream_cache_bytes()
                .saturating_add(incoming_len)
                .saturating_add(add_key_bytes)
                .saturating_sub(existing_field_len);
        }

        while projected_total > max_cache_bytes
            && self.evict_for_stream_cache_pressure(publisher_conn_id, key)
        {
            // After evicting *other* routes, recompute from the live total.
            // Subtract only the field being replaced — not the whole entry
            // for `key`, whose other headers/keyframe remain in the map.
            let add_key_bytes = if is_new_key && !self.stream_cache.contains_key(key) {
                key.0.len().saturating_add(key.1.len())
            } else {
                0
            };
            projected_total = self
                .stream_cache_bytes()
                .saturating_add(incoming_len)
                .saturating_add(add_key_bytes)
                .saturating_sub(existing_field_len);
        }
        projected_total <= max_cache_bytes
    }

    /// Bytes freeable by evicting same-publisher peers or other external routes.
    fn stream_cache_freeable_bytes(
        &self,
        key: &(String, String),
        publisher_conn_id: u64,
    ) -> usize {
        self.stream_cache
            .iter()
            .filter(|(k, _)| *k != key)
            .filter(|(k, _)| {
                self.publisher_cache_keys.iter().any(|(pub_id, keys)| {
                    keys.contains(*k)
                        && (*pub_id == publisher_conn_id || is_external_publisher_id(*pub_id))
                })
            })
            .map(|(k, cache)| Self::stream_cache_entry_bytes(k, cache))
            .sum()
    }

    fn cache_relay_frame(&mut self, frame: &RelayFrame) {
        if frame.frame_type == FrameType::Script || frame.frame_type == FrameType::Metadata {
            // Injected Script events (e.g. onCuePoint) must still fan out live,
            // but only onMetaData belongs in the late-joiner metadata cache.
            if frame.frame_type == FrameType::Script && !is_on_metadata_payload(&frame.payload) {
                return;
            }
            if frame.payload.len() > MAX_CACHED_INIT_FRAME_BYTES {
                return;
            }
            let key = (frame.app.clone(), frame.stream_name.clone());
            let existing_field_len = self
                .stream_cache
                .get(&key)
                .and_then(|cache| cache.metadata.as_ref())
                .map(|v| v.len())
                .unwrap_or(0);
            if !self.reserve_stream_cache_storage(
                &key,
                frame.payload.len(),
                existing_field_len,
                frame.publisher_conn_id,
            ) {
                return;
            }
            Self::track_publisher_cache_key(
                &mut self.publisher_cache_keys,
                frame.publisher_conn_id,
                &key,
            );
            let cache = self
                .stream_cache
                .entry(key)
                .or_insert_with(empty_stream_cache);
            cache.metadata = Some(frame.payload.clone());
            return;
        }

        let cache_kind = classify_cache_frame(frame.frame_type, frame.cache_payload());
        let is_avc_header = cache_kind == CacheFrameKind::VideoSequenceHeader;
        let is_keyframe = cache_kind == CacheFrameKind::VideoKeyframe;
        let is_aac_header = cache_kind == CacheFrameKind::AudioSequenceHeader;

        // Frames that are neither a codec header nor a keyframe are relayed
        // live but never cached; don't create an empty entry for them.
        if !is_avc_header && !is_keyframe && !is_aac_header {
            return;
        }

        let key = (frame.app.clone(), frame.stream_name.clone());

        let cap = if is_keyframe {
            MAX_CACHED_KEYFRAME_BYTES
        } else {
            MAX_CACHED_INIT_FRAME_BYTES
        };
        if frame.payload.len() > cap {
            // An oversized replacement still means the previously cached
            // copy for this field is stale (the publisher has moved past
            // it) -- drop it rather than keep replaying it to late joiners.
            if let Some(cache) = self.stream_cache.get_mut(&key) {
                if is_avc_header {
                    let seq_tracks = Self::multitrack_sequence_track_ids(
                        FrameType::Video,
                        frame.cache_payload(),
                    );
                    if seq_tracks.len() > 1 {
                        cache.avc_header = None;
                        cache.video_track_headers.clear();
                    } else if let Some(track_id) = seq_tracks.first().copied() {
                        cache.avc_header = None;
                        cache.video_track_headers.remove(&track_id);
                    } else {
                        cache.avc_header = None;
                        cache.video_track_headers.clear();
                    }
                } else if is_keyframe {
                    cache.last_keyframe = None;
                } else {
                    let seq_tracks = Self::multitrack_sequence_track_ids(
                        FrameType::Audio,
                        frame.cache_payload(),
                    );
                    if seq_tracks.len() > 1 {
                        cache.aac_header = None;
                        cache.audio_track_headers.clear();
                    } else if let Some(track_id) = seq_tracks.first().copied() {
                        cache.aac_header = None;
                        cache.audio_track_headers.remove(&track_id);
                    } else {
                        cache.aac_header = None;
                        cache.audio_track_headers.clear();
                    }
                }
            }
            if self
                .stream_cache
                .get(&key)
                .is_some_and(Self::stream_cache_is_empty)
            {
                self.evict_stream_cache_key(&key);
            }
            return;
        }

        let existing_field_len = self
            .stream_cache
            .get(&key)
            .map(|cache| {
                if is_avc_header {
                    let seq_tracks = Self::multitrack_sequence_track_ids(
                        FrameType::Video,
                        frame.cache_payload(),
                    );
                    let combined = cache.avc_header.as_ref().map(|v| v.len()).unwrap_or(0);
                    let tracks: usize = cache.video_track_headers.values().map(|v| v.len()).sum();
                    if seq_tracks.len() > 1 {
                        // Clears all track headers; replaces combined header.
                        combined.saturating_add(tracks)
                    } else if let Some(track_id) = seq_tracks.first() {
                        // Clears combined header; replaces one track slot.
                        combined.saturating_add(
                            cache
                                .video_track_headers
                                .get(track_id)
                                .map(|v| v.len())
                                .unwrap_or(0),
                        )
                    } else {
                        combined.saturating_add(tracks)
                    }
                } else if is_keyframe {
                    cache
                        .last_keyframe
                        .as_ref()
                        .map(|(_, v)| v.len())
                        .unwrap_or(0)
                } else {
                    let seq_tracks = Self::multitrack_sequence_track_ids(
                        FrameType::Audio,
                        frame.cache_payload(),
                    );
                    let combined = cache.aac_header.as_ref().map(|v| v.len()).unwrap_or(0);
                    let tracks: usize = cache.audio_track_headers.values().map(|v| v.len()).sum();
                    if seq_tracks.len() > 1 {
                        combined.saturating_add(tracks)
                    } else if let Some(track_id) = seq_tracks.first() {
                        combined.saturating_add(
                            cache
                                .audio_track_headers
                                .get(track_id)
                                .map(|v| v.len())
                                .unwrap_or(0),
                        )
                    } else {
                        combined.saturating_add(tracks)
                    }
                }
            })
            .unwrap_or(0);
        if !self.reserve_stream_cache_storage(
            &key,
            frame.payload.len(),
            existing_field_len,
            frame.publisher_conn_id,
        ) {
            return;
        }
        Self::track_publisher_cache_key(
            &mut self.publisher_cache_keys,
            frame.publisher_conn_id,
            &key,
        );

        let cache = self
            .stream_cache
            .entry(key)
            .or_insert_with(empty_stream_cache);
        if is_avc_header {
            let seq_tracks =
                Self::multitrack_sequence_track_ids(FrameType::Video, frame.cache_payload());
            if seq_tracks.len() > 1 {
                cache.video_track_headers.clear();
                cache.avc_header = Some(frame.payload.clone());
            } else if let Some(track_id) = seq_tracks.first().copied() {
                cache.avc_header = None;
                cache
                    .video_track_headers
                    .insert(track_id, frame.payload.clone());
            } else {
                cache.video_track_headers.clear();
                cache.avc_header = Some(frame.payload.clone());
            }
        } else if is_keyframe {
            cache.last_keyframe = Some((frame.timestamp, frame.payload.clone()));
        } else if is_aac_header {
            let seq_tracks =
                Self::multitrack_sequence_track_ids(FrameType::Audio, frame.cache_payload());
            if seq_tracks.len() > 1 {
                cache.audio_track_headers.clear();
                cache.aac_header = Some(frame.payload.clone());
            } else if let Some(track_id) = seq_tracks.first().copied() {
                cache.aac_header = None;
                cache
                    .audio_track_headers
                    .insert(track_id, frame.payload.clone());
            } else {
                cache.audio_track_headers.clear();
                cache.aac_header = Some(frame.payload.clone());
            }
        }
    }

    /// Drain every connection's queued rename evictions, returning the set of
    /// keys actually removed from `stream_cache`.
    ///
    /// A `pending_cache_evictions` entry only reflects what the connection
    /// *believes* it was routing under before the rename -- it has no
    /// visibility into `publisher_cache_keys`. Two independent connections
    /// can publish under the same (app, stream_name) (accidentally, or via a
    /// hostile client racing a legitimate publisher's route name), so an
    /// eviction is only honored when this conn_id is a confirmed owner of
    /// the key in `publisher_cache_keys`; otherwise it would delete a cache
    /// entry that belongs to a different, still-active publisher.
    fn drain_pending_cache_evictions(
        &mut self,
    ) -> std::collections::HashSet<(String, String, u64)> {
        let mut abandoned = std::collections::HashSet::new();
        for conn in &mut self.connections {
            for key in conn.pending_cache_evictions.drain(..) {
                // Record the raw request regardless of ownership outcome
                // below: a connection's own frame, queued under this key
                // earlier in the same batch (before this rename/abandon was
                // processed), must never resurrect the entry -- whether or
                // not publisher_cache_keys already reflects ownership yet.
                // Scoped by conn_id so this doesn't suppress a *different*
                // publisher's frame for the same (app, name).
                abandoned.insert((key.0.clone(), key.1.clone(), conn.conn_id));

                let owns_key = self
                    .publisher_cache_keys
                    .get(&conn.conn_id)
                    .map(|keys| keys.contains(&key))
                    .unwrap_or(false);
                if !owns_key {
                    continue;
                }
                if let Some(keys) = self.publisher_cache_keys.get_mut(&conn.conn_id) {
                    keys.retain(|k| k != &key);
                    if keys.is_empty() {
                        self.publisher_cache_keys.remove(&conn.conn_id);
                    }
                }
                // A different conn_id can still be tracking this exact
                // (app, stream_name) (two publishers sharing a route name).
                // Only actually drop the cache entry once no other tracked
                // publisher claims it, or renaming away would wipe out a
                // still-active publisher's cached headers/keyframe.
                let still_owned = self
                    .publisher_cache_keys
                    .values()
                    .any(|keys| keys.contains(&key));
                if !still_owned {
                    self.stream_cache.remove(&key);
                }
            }
        }
        abandoned
    }
    fn track_publisher_cache_key(
        publisher_cache_keys: &mut HashMap<u64, Vec<(String, String)>>,
        publisher_conn_id: u64,
        key: &(String, String),
    ) {
        let publisher_keys = publisher_cache_keys.entry(publisher_conn_id).or_default();
        if !publisher_keys.iter().any(|k| k == key) {
            publisher_keys.push(key.clone());
        }
    }
}

/// Merge local and injected relay frames. Same-route frames keep handoff order
/// (local deferred media before a new inject on that route). Independent routes
/// are then round-robin merged; `inject_first` selects which route's head leads
/// when both sources contribute distinct routes in this poll.
fn merge_local_and_injected_by_route(
    local: Vec<RelayFrame>,
    injected: Vec<RelayFrame>,
    inject_first: bool,
) -> Vec<RelayFrame> {
    let mut route_order: Vec<(String, String)> = Vec::new();
    let mut by_route: HashMap<(String, String), Vec<RelayFrame>> = HashMap::new();
    let mut push = |frame: RelayFrame| {
        let key = (frame.app.clone(), frame.stream_name.clone());
        if !by_route.contains_key(&key) {
            route_order.push(key.clone());
        }
        by_route.entry(key).or_default().push(frame);
    };
    // Local first so an abandoned publisher's deferred frames precede a new
    // external claim on the same route in this merge.
    for frame in local {
        push(frame);
    }
    for frame in injected {
        push(frame);
    }
    let route_queues: Vec<Vec<RelayFrame>> = route_order
        .into_iter()
        .filter_map(|k| by_route.remove(&k))
        .collect();
    let start = if inject_first && !route_queues.is_empty() {
        // Prefer a route whose head came from inject when possible.
        route_queues
            .iter()
            .position(|q| {
                q.first()
                    .map(|f| is_external_publisher_id(f.publisher_conn_id))
                    .unwrap_or(false)
            })
            .unwrap_or(0)
    } else {
        0
    };
    round_robin_relay_queues(route_queues, start)
}

/// Round-robin merge of per-publisher local queues, preserving relative order
/// within each queue. Queues that share the same `(app, stream_name)` route are
/// concatenated in encounter order first so a same-batch A→B handoff cannot
/// emit B's media before A's already-accepted final frames. Round-robin then
/// applies across independent routes. `start` selects which route queue leads
/// the first round; callers should rotate it across polls under a tiny send
/// budget.
fn round_robin_relay_queues(queues: Vec<Vec<RelayFrame>>, start: usize) -> Vec<RelayFrame> {
    if queues.is_empty() {
        return Vec::new();
    }
    let mut route_order: Vec<(String, String)> = Vec::new();
    let mut by_route: HashMap<(String, String), Vec<RelayFrame>> = HashMap::new();
    // Group by each frame's route — a single publisher queue can mix routes
    // after an in-batch rename; first-frame grouping would mis-order handoffs.
    for q in queues {
        for frame in q {
            let key = (frame.app.clone(), frame.stream_name.clone());
            if !by_route.contains_key(&key) {
                route_order.push(key.clone());
            }
            by_route.entry(key).or_default().push(frame);
        }
    }
    let route_queues: Vec<Vec<RelayFrame>> = route_order
        .into_iter()
        .filter_map(|k| by_route.remove(&k))
        .collect();
    if route_queues.is_empty() {
        return Vec::new();
    }
    let n = route_queues.len();
    let start = start % n;
    let total: usize = route_queues.iter().map(Vec::len).sum();
    let mut iters: Vec<_> = route_queues.into_iter().map(|q| q.into_iter()).collect();
    let mut out = Vec::with_capacity(total);
    loop {
        let mut progressed = false;
        for i in 0..n {
            if let Some(frame) = iters[(start + i) % n].next() {
                out.push(frame);
                progressed = true;
            }
        }
        if !progressed {
            break;
        }
    }
    out
}

fn empty_stream_cache() -> StreamCache {
    StreamCache {
        avc_header: None,
        video_track_headers: HashMap::new(),
        aac_header: None,
        audio_track_headers: HashMap::new(),
        metadata: None,
        last_keyframe: None,
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.running = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recv_budget_is_at_least_one_socket_read() {
        assert!(MAX_RECV_BYTES_PER_CONN_PER_POLL >= 65536);
    }

    #[test]
    fn recv_budget_is_small_enough_for_fairness_across_connections() {
        assert!(MAX_RECV_BYTES_PER_CONN_PER_POLL <= 1024 * 1024);
    }

    #[test]
    fn relay_send_budget_limits_worst_case_player_fan_out() {
        // One publisher can queue 1024 frames; with 256 connections that
        // could be 261_120 sends per poll without a relay budget.
        let worst_case = 1024 * 256;
        let server = test_server();
        assert_eq!(
            server.max_relay_sends_per_poll,
            DEFAULT_MAX_RELAY_SENDS_PER_POLL
        );
        assert!(
            server.max_relay_sends_per_poll < worst_case / 10,
            "relay budget should be well below unbounded fan-out"
        );
    }

    #[test]
    fn oversized_relay_fan_out_still_makes_progress_each_poll() {
        use crate::session::stream::Stream;
        use crate::transport::Transport;
        use std::os::unix::io::IntoRawFd;
        use std::os::unix::net::UnixStream;

        fn attached_conn(conn_id: u64, publishing: bool) -> (Conn, UnixStream) {
            let (server_end, peer_end) = UnixStream::pair().unwrap();
            server_end.set_nonblocking(true).unwrap();
            peer_end.set_nonblocking(true).unwrap();

            let mut conn = Conn::new();
            conn.conn_id = conn_id;
            conn.app = "live".to_string();
            conn.relay_enabled = true;
            conn.transport = Some(Transport::new_plain(server_end.into_raw_fd()));
            conn.current_stream = Some(Box::new(Stream {
                stream_id: 1,
                name: "stream".to_string(),
                is_publishing: publishing,
                is_playing: !publishing,
                paused: false,
                receive_audio: true,
                receive_video: true,
            }));
            (conn, peer_end)
        }

        let mut server = test_server();
        server.max_relay_sends_per_poll = 1;

        let (mut publisher, _publisher_peer) = attached_conn(1, true);
        publisher
            .pending_relay
            .push(relay_frame(FrameType::Video, vec![0x17, 0x01, 0xAA]));
        publisher
            .pending_relay
            .push(relay_frame(FrameType::Video, vec![0x27, 0x01, 0xBB]));
        let (player_a, _player_a_peer) = attached_conn(2, false);
        let (player_b, _player_b_peer) = attached_conn(3, false);
        server.connections = vec![publisher, player_a, player_b];

        server.process_connections().unwrap();
        assert_eq!(
            server.connections[0].pending_relay.len(),
            1,
            "the first oversized fan-out frame must be relayed instead of re-queuing the whole batch"
        );

        server.process_connections().unwrap();
        assert!(
            server.connections[0].pending_relay.is_empty(),
            "the deferred frame must make progress on the next poll"
        );
    }

    #[test]
    fn merge_local_and_injected_keeps_same_route_local_first() {
        let injected = vec![relay_frame_for_publisher(
            EXTERNAL_RELAY_PUBLISHER_ID,
            "stream",
            FrameType::Video,
            vec![0xA1],
        )];
        let local = vec![relay_frame_for_publisher(
            1,
            "stream",
            FrameType::Video,
            vec![0xB1],
        )];
        // Even with inject_first, same-route handoff keeps deferred local media
        // ahead of the new external claim.
        let merged = merge_local_and_injected_by_route(local, injected, true);
        assert_eq!(merged[0].payload, vec![0xB1]);
        assert_eq!(merged[1].payload, vec![0xA1]);
    }

    #[test]
    fn merge_local_and_injected_can_lead_with_inject_on_other_route() {
        let injected = vec![relay_frame_for_publisher(
            EXTERNAL_RELAY_PUBLISHER_ID,
            "inj",
            FrameType::Video,
            vec![0xA1],
        )];
        let local = vec![relay_frame_for_publisher(
            1,
            "loc",
            FrameType::Video,
            vec![0xB1],
        )];
        let merged = merge_local_and_injected_by_route(local, injected, true);
        assert_eq!(merged[0].payload, vec![0xA1]);
        assert_eq!(merged[1].payload, vec![0xB1]);
    }

    #[test]
    fn round_robin_relay_queues_rotates_lead_and_interleaves() {
        let q0 = vec![
            relay_frame_for_publisher(1, "a", FrameType::Video, vec![0xA1]),
            relay_frame_for_publisher(1, "a", FrameType::Video, vec![0xA2]),
        ];
        let q1 = vec![
            relay_frame_for_publisher(2, "b", FrameType::Video, vec![0xB1]),
            relay_frame_for_publisher(2, "b", FrameType::Video, vec![0xB2]),
        ];
        let from_0 = round_robin_relay_queues(vec![q0.clone(), q1.clone()], 0);
        assert_eq!(
            from_0
                .iter()
                .map(|f| f.payload[0])
                .collect::<Vec<_>>(),
            vec![0xA1, 0xB1, 0xA2, 0xB2]
        );
        let from_1 = round_robin_relay_queues(vec![q0, q1], 1);
        assert_eq!(
            from_1
                .iter()
                .map(|f| f.payload[0])
                .collect::<Vec<_>>(),
            vec![0xB1, 0xA1, 0xB2, 0xA2]
        );
    }

    #[test]
    fn round_robin_relay_queues_preserves_same_route_handoff_order() {
        let old_pub = vec![
            relay_frame_for_publisher(1, "live", FrameType::Video, vec![0xA1]),
            relay_frame_for_publisher(1, "live", FrameType::Video, vec![0xA2]),
        ];
        let new_pub = vec![relay_frame_for_publisher(
            2,
            "live",
            FrameType::Video,
            vec![0xB1],
        )];
        // Rotating start must not put B ahead of A's already-accepted finals
        // on the same route.
        let merged = round_robin_relay_queues(vec![old_pub, new_pub], 1);
        assert_eq!(
            merged.iter().map(|f| f.payload[0]).collect::<Vec<_>>(),
            vec![0xA1, 0xA2, 0xB1]
        );
    }

    #[test]
    fn process_connections_rotates_interleave_lead() {
        let mut server = test_server();
        assert!(server.relay_interleave_inject_first);
        server.process_connections().unwrap();
        assert!(!server.relay_interleave_inject_first);
        server.process_connections().unwrap();
        assert!(server.relay_interleave_inject_first);
    }

    fn test_server() -> Server {
        Server::new(ServerConfig {
            max_connections: 4,
            chunk_size: 128,
            tls_enabled: 0,
            tls_cert_file: std::ptr::null(),
            tls_key_file: std::ptr::null(),
            tls_ca_file: std::ptr::null(),
            tls_insecure: 0,
            max_pending_tls_per_addr: 0,
            max_connections_per_addr: 0,
        })
        .unwrap()
    }

    /// Loopback clients can return from `connect()` before the listener's
    /// non-blocking `accept()` sees the socket (notably on macOS). Retry
    /// until the expected connections are admitted or the deadline expires.
    fn accept_pending_connections(server: &mut Server, expected: usize) {
        use std::time::{Duration, Instant};

        let deadline = Instant::now() + Duration::from_secs(2);
        while server.connections.len() < expected {
            server.accept_new_connections();
            if server.connections.len() >= expected {
                return;
            }
            if Instant::now() >= deadline {
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(
            server.connections.len(),
            expected,
            "expected {expected} accepted connection(s)"
        );
    }

    fn relay_frame(frame_type: FrameType, payload: Vec<u8>) -> crate::session::conn::RelayFrame {
        relay_frame_for_publisher(1, "stream", frame_type, payload)
    }

    #[test]
    fn script_metadata_relay_respects_receive_audio_and_video_toggles() {
        use crate::session::stream::Stream;
        use crate::transport::Transport;
        use std::os::unix::io::IntoRawFd;
        use std::os::unix::net::UnixStream;

        let (server_end, _peer_end) = UnixStream::pair().unwrap();
        server_end.set_nonblocking(true).unwrap();

        let mut player = Conn::new();
        player.app = "live".to_string();
        player.relay_enabled = true;
        player.transport = Some(Transport::new_plain(server_end.into_raw_fd()));
        player.current_stream = Some(Box::new(Stream {
            stream_id: 1,
            name: "stream".to_string(),
            is_publishing: false,
            is_playing: true,
            paused: false,
            receive_audio: false,
            receive_video: false,
        }));

        let script = relay_frame(
            FrameType::Script,
            vec![0x12, 0x00, 0x0A, b'o', b'n', b'M', b'e', b't', b'a'],
        );
        let metadata = relay_frame(
            FrameType::Metadata,
            vec![0x12, 0x00, 0x0A, b'o', b'n', b'M', b'e', b't', b'a'],
        );
        assert!(
            !Server::conn_will_receive_relay_frame(&player, &script),
            "script must not relay when both receiveAudio and receiveVideo are false"
        );
        assert!(
            !Server::conn_will_receive_relay_frame(&player, &metadata),
            "metadata must not relay when both receiveAudio and receiveVideo are false"
        );

        player.current_stream.as_mut().unwrap().receive_video = true;
        assert!(
            Server::conn_will_receive_relay_frame(&player, &script),
            "script should relay when at least one receive toggle is true"
        );
        assert!(
            Server::conn_will_receive_relay_frame(&player, &metadata),
            "metadata should relay when at least one receive toggle is true"
        );
    }

    #[test]
    fn cached_metadata_replay_respects_receive_audio_and_video_toggles() {
        use crate::session::stream::Stream;
        use crate::transport::Transport;
        use std::io::Read;
        use std::os::unix::io::IntoRawFd;
        use std::os::unix::net::UnixStream;

        let mut server = test_server();
        let mut payload = vec![0x02, 0x00, 0x0A];
        payload.extend_from_slice(b"onMetaData");
        payload.push(crate::amf::amf0::Amf0Type::Object as u8);
        payload.extend_from_slice(&[0x00, 0x00, 0x09]);
        server.cache_relay_frame(&relay_frame(FrameType::Script, payload));

        let (server_end, mut peer_end) = UnixStream::pair().unwrap();
        server_end.set_nonblocking(true).unwrap();
        peer_end.set_nonblocking(true).unwrap();

        let mut player = Conn::new();
        player.app = "live".to_string();
        player.relay_enabled = true;
        player.needs_init_frames = true;
        player.client_fd = 0;
        player.transport = Some(Transport::new_plain(server_end.into_raw_fd()));
        player.current_stream = Some(Box::new(Stream {
            stream_id: 1,
            name: "stream".to_string(),
            is_publishing: false,
            is_playing: true,
            paused: false,
            receive_audio: false,
            receive_video: false,
        }));
        server.connections = vec![player];

        server.process_connections().unwrap();
        let mut buf = [0u8; 64];
        let read = peer_end.read(&mut buf);
        assert!(
            matches!(&read, Err(e) if e.kind() == std::io::ErrorKind::WouldBlock),
            "cached metadata must not be replayed when both receiveAudio and receiveVideo are false, got {read:?}"
        );

        server.connections[0].needs_init_frames = true;
        server.connections[0]
            .current_stream
            .as_mut()
            .unwrap()
            .receive_video = true;
        server.process_connections().unwrap();
        let read = peer_end.read(&mut buf);
        assert!(
            matches!(&read, Ok(n) if *n > 0),
            "cached metadata should be replayed once at least one receive toggle is true, got {read:?}"
        );
    }

    fn relay_frame_for_publisher(
        publisher_conn_id: u64,
        stream_name: &str,
        frame_type: FrameType,
        payload: Vec<u8>,
    ) -> crate::session::conn::RelayFrame {
        crate::session::conn::RelayFrame {
            app: "live".to_string(),
            stream_name: stream_name.to_string(),
            publisher_conn_id,
            frame_type,
            timestamp: 0,
            cache_payload: None,
            payload,
        }
    }

    #[test]
    fn modex_wrapped_multitrack_is_detected_for_player_gating() {
        let payload = [
            0x87, 0x02, 0x00, 0x01, 0x02, 0x06, 0x10, b'a', b'v', b'c', b'1', 0x00, 0x00, 0x00,
            0x01, 0xAA,
        ];
        assert!(Server::cached_payload_is_multitrack(
            FrameType::Video,
            &payload
        ));
    }

    #[test]
    fn multitrack_video_sequence_header_is_cached() {
        let mut server = test_server();
        let payload = vec![
            0x86, 0x10, b'a', b'v', b'c', b'1', 0x00, 0x00, 0x00, 0x01, 0xAA, 0x01, 0x00, 0x00,
            0x01, 0xBB,
        ];
        server.cache_relay_frame(&relay_frame(FrameType::Video, payload));

        let key = ("live".to_string(), "stream".to_string());
        assert!(server.stream_cache.get(&key).unwrap().avc_header.is_some());
    }

    #[test]
    fn multitrack_per_track_video_inits_are_retained() {
        let mut server = test_server();
        let track0 = vec![
            0x86, 0x10, b'a', b'v', b'c', b'1', 0x00, 0x00, 0x00, 0x01, 0xAA,
        ];
        let track1 = vec![
            0x86, 0x10, b'a', b'v', b'c', b'1', 0x01, 0x00, 0x00, 0x01, 0xBB,
        ];
        server.cache_relay_frame(&relay_frame(FrameType::Video, track0));
        server.cache_relay_frame(&relay_frame(FrameType::Video, track1));

        let key = ("live".to_string(), "stream".to_string());
        let cache = server.stream_cache.get(&key).unwrap();
        assert!(cache.avc_header.is_none());
        assert_eq!(cache.video_track_headers.len(), 2);
        assert!(cache.video_track_headers.contains_key(&0));
        assert!(cache.video_track_headers.contains_key(&1));
    }

    #[test]
    fn enhanced_hevc_sequence_header_is_cached() {
        let mut server = test_server();
        let payload = vec![0x90, b'h', b'v', b'c', b'1', 0x01, 0x02];
        server.cache_relay_frame(&relay_frame(FrameType::Video, payload));

        let key = ("live".to_string(), "stream".to_string());
        assert!(server.stream_cache.get(&key).unwrap().avc_header.is_some());
    }

    #[test]
    fn on_metadata_script_is_cached() {
        let mut server = test_server();
        let mut payload = vec![0x02, 0x00, 0x0A];
        payload.extend_from_slice(b"onMetaData");
        payload.push(crate::amf::amf0::Amf0Type::Object as u8);
        payload.extend_from_slice(&[0x00, 0x00, 0x09]);

        server.cache_relay_frame(&relay_frame(FrameType::Script, payload));
        let key = ("live".to_string(), "stream".to_string());
        assert!(server.stream_cache.get(&key).unwrap().metadata.is_some());
    }

    #[test]
    fn stream_cache_eviction_is_scoped_to_publisher() {
        let mut server = test_server();
        let legit_payload = vec![0x17, 0x00, 0xAA];
        server.cache_relay_frame(&relay_frame_for_publisher(
            1,
            "legit",
            FrameType::Video,
            legit_payload,
        ));
        let legit_key = ("live".to_string(), "legit".to_string());
        assert!(server.stream_cache.contains_key(&legit_key));

        for i in 0..=MAX_STREAM_CACHE_KEYS_PER_PUBLISHER {
            let name = format!("spam-{i}");
            server.cache_relay_frame(&relay_frame_for_publisher(
                2,
                &name,
                FrameType::Video,
                vec![0x17, 0x00, 0xBB],
            ));
        }

        assert!(
            server.stream_cache.contains_key(&legit_key),
            "another publisher's cache entry must not be evicted"
        );
    }

    #[test]
    fn oversized_codec_header_is_not_cached() {
        let mut server = test_server();

        let mut payload = vec![0x17, 0x00];
        payload.resize(MAX_CACHED_INIT_FRAME_BYTES + 1, 0xAA);
        server.cache_relay_frame(&relay_frame(FrameType::Video, payload));
        assert!(server.stream_cache.is_empty());
    }

    #[test]
    fn oversized_keyframe_is_not_cached() {
        let mut server = test_server();

        let mut payload = vec![0x17, 0x01];
        payload.resize(MAX_CACHED_KEYFRAME_BYTES + 1, 0xAA);
        server.cache_relay_frame(&relay_frame(FrameType::Video, payload));
        assert!(server.stream_cache.is_empty());
    }

    #[test]
    fn keyframe_within_larger_cap_is_still_cached() {
        // A real IDR frame can comfortably exceed the codec-header cap while
        // staying under the dedicated (larger) keyframe cap.
        let mut server = test_server();

        let mut payload = vec![0x17, 0x01];
        payload.resize(MAX_CACHED_INIT_FRAME_BYTES + 1, 0xAA);
        assert!(payload.len() <= MAX_CACHED_KEYFRAME_BYTES);
        server.cache_relay_frame(&relay_frame(FrameType::Video, payload));

        let key = ("live".to_string(), "stream".to_string());
        assert!(
            server
                .stream_cache
                .get(&key)
                .unwrap()
                .last_keyframe
                .is_some()
        );
    }

    #[test]
    fn oversized_replacement_clears_stale_cached_header() {
        let mut server = test_server();
        let key = ("live".to_string(), "stream".to_string());

        // Cache a normal-sized AVC header first.
        server.cache_relay_frame(&relay_frame(FrameType::Video, vec![0x17, 0x00, 0xAA]));
        assert!(server.stream_cache.get(&key).unwrap().avc_header.is_some());

        // A later oversized "header" replacement must drop the stale cached
        // copy instead of leaving late joiners with outdated codec data.
        let mut oversized = vec![0x17, 0x00];
        oversized.resize(MAX_CACHED_INIT_FRAME_BYTES + 1, 0xBB);
        server.cache_relay_frame(&relay_frame(FrameType::Video, oversized));

        // The whole entry is gone since it had no other cached fields.
        assert!(server.stream_cache.get(&key).is_none());
    }

    #[test]
    fn max_connections_limit_is_enforced_when_configured() {
        let config = ServerConfig {
            max_connections: 2,
            chunk_size: 128,
            tls_enabled: 0,
            tls_cert_file: std::ptr::null(),
            tls_key_file: std::ptr::null(),
            tls_ca_file: std::ptr::null(),
            tls_insecure: 0,
            max_pending_tls_per_addr: 0,
            max_connections_per_addr: 0,
        };
        let mut server = Server::new(config).unwrap();
        server.listen("127.0.0.1:0").unwrap();

        let port = {
            let mut addr: libc::sockaddr_in = unsafe { std::mem::zeroed() };
            let mut len = std::mem::size_of::<libc::sockaddr_in>() as libc::socklen_t;
            let rc = unsafe {
                libc::getsockname(
                    server.server_fd,
                    &mut addr as *mut _ as *mut libc::sockaddr,
                    &mut len,
                )
            };
            assert_eq!(rc, 0);
            u16::from_be(addr.sin_port)
        };
        let addr = format!("127.0.0.1:{port}");

        let mut streams = Vec::new();
        for _ in 0..2 {
            streams.push(std::net::TcpStream::connect(&addr).unwrap());
        }
        accept_pending_connections(&mut server, 2);

        let _third = std::net::TcpStream::connect(&addr).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while std::time::Instant::now() < deadline {
            server.accept_new_connections();
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert_eq!(server.connections.len(), 2);
    }

    #[cfg(feature = "tls")]
    #[test]
    fn pending_tls_handshakes_share_max_connections_budget_with_active_sessions() {
        let (cert_path, key_path) = self_signed_cert_files("pending-tls-max-conn-budget.test");
        let config = ServerConfig {
            max_connections: 4,
            chunk_size: 128,
            tls_enabled: 0,
            tls_cert_file: std::ptr::null(),
            tls_key_file: std::ptr::null(),
            tls_ca_file: std::ptr::null(),
            tls_insecure: 0,
            max_pending_tls_per_addr: 0,
            max_connections_per_addr: 0,
        };
        let mut server = Server::new(config).unwrap();
        server.listen("127.0.0.1:0").unwrap();
        server
            .listen_tls(
                "127.0.0.1:0",
                cert_path.to_str().unwrap(),
                key_path.to_str().unwrap(),
            )
            .unwrap();

        let plaintext_port = {
            let fd = server.listener_fds()[0];
            let mut addr: libc::sockaddr_in = unsafe { std::mem::zeroed() };
            let mut len = std::mem::size_of::<libc::sockaddr_in>() as libc::socklen_t;
            let rc = unsafe {
                libc::getsockname(fd, &mut addr as *mut _ as *mut libc::sockaddr, &mut len)
            };
            assert_eq!(rc, 0);
            u16::from_be(addr.sin_port)
        };
        let tls_port = {
            let fd = server.listener_fds()[1];
            let mut addr: libc::sockaddr_in = unsafe { std::mem::zeroed() };
            let mut len = std::mem::size_of::<libc::sockaddr_in>() as libc::socklen_t;
            let rc = unsafe {
                libc::getsockname(fd, &mut addr as *mut _ as *mut libc::sockaddr, &mut len)
            };
            assert_eq!(rc, 0);
            u16::from_be(addr.sin_port)
        };
        let plaintext_addr = format!("127.0.0.1:{plaintext_port}");
        let tls_addr = format!("127.0.0.1:{tls_port}");

        fn accept_until_slots_in_use(server: &mut Server, expected: usize) {
            use std::time::{Duration, Instant};

            let deadline = Instant::now() + Duration::from_secs(2);
            while server.total_connection_slots_in_use() < expected && Instant::now() < deadline {
                server.accept_new_connections();
                if server.total_connection_slots_in_use() < expected {
                    std::thread::sleep(Duration::from_millis(1));
                }
            }
            assert_eq!(
                server.total_connection_slots_in_use(),
                expected,
                "expected {expected} total connection slot(s) in use"
            );
        }

        let mut stalled_tls = Vec::new();
        for expected in 1..=3 {
            stalled_tls.push(std::net::TcpStream::connect(&tls_addr).unwrap());
            accept_until_slots_in_use(&mut server, expected);
        }
        assert_eq!(server.pending_tls_count(), 3);

        let mut plaintext = Vec::new();
        for _ in 0..4 {
            plaintext.push(std::net::TcpStream::connect(&plaintext_addr).unwrap());
        }
        accept_until_slots_in_use(&mut server, 4);

        assert_eq!(
            server.connections.len(),
            1,
            "only one active slot should remain once three pending TLS handshakes occupy the cap"
        );
        assert_eq!(server.pending_tls_count(), 3);
        assert!(
            server.connections.len() + server.pending_tls_count() <= 4,
            "pending TLS and active connections must share max_connections"
        );

        let _ = std::fs::remove_file(cert_path);
        let _ = std::fs::remove_file(key_path);
    }

    #[test]
    fn per_ip_connection_cap_limits_plaintext_accepts() {
        let config = ServerConfig {
            max_connections: 16,
            chunk_size: 128,
            tls_enabled: 0,
            tls_cert_file: std::ptr::null(),
            tls_key_file: std::ptr::null(),
            tls_ca_file: std::ptr::null(),
            tls_insecure: 0,
            max_pending_tls_per_addr: 0,
            max_connections_per_addr: 2,
        };
        let mut server = Server::new(config).unwrap();
        server.listen("127.0.0.1:0").unwrap();

        let port = {
            let mut addr: libc::sockaddr_in = unsafe { std::mem::zeroed() };
            let mut len = std::mem::size_of::<libc::sockaddr_in>() as libc::socklen_t;
            let rc = unsafe {
                libc::getsockname(
                    server.server_fd,
                    &mut addr as *mut _ as *mut libc::sockaddr,
                    &mut len,
                )
            };
            assert_eq!(rc, 0);
            u16::from_be(addr.sin_port)
        };
        let addr = format!("127.0.0.1:{port}");

        let mut streams = Vec::new();
        for _ in 0..2 {
            streams.push(std::net::TcpStream::connect(&addr).unwrap());
        }
        accept_pending_connections(&mut server, 2);

        let _third = std::net::TcpStream::connect(&addr).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while std::time::Instant::now() < deadline {
            server.accept_new_connections();
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert_eq!(
            server.connections.len(),
            2,
            "third connection from the same IP must be rejected"
        );
    }

    #[test]
    fn poll_reaps_stale_connections_before_enforcing_per_ip_cap() {
        use std::time::{Duration, Instant};

        // Regression test: `poll()` must reap timed-out connections before
        // admitting new ones, so a same-IP reconnect landing in the same
        // tick as its predecessor's reap isn't spuriously rejected by
        // max_connections_per_addr.
        let config = ServerConfig {
            max_connections: 16,
            chunk_size: 128,
            tls_enabled: 0,
            tls_cert_file: std::ptr::null(),
            tls_key_file: std::ptr::null(),
            tls_ca_file: std::ptr::null(),
            tls_insecure: 0,
            max_pending_tls_per_addr: 0,
            max_connections_per_addr: 1,
        };
        let mut server = Server::new(config).unwrap();
        server.listen("127.0.0.1:0").unwrap();

        let port = {
            let mut addr: libc::sockaddr_in = unsafe { std::mem::zeroed() };
            let mut len = std::mem::size_of::<libc::sockaddr_in>() as libc::socklen_t;
            let rc = unsafe {
                libc::getsockname(
                    server.server_fd,
                    &mut addr as *mut _ as *mut libc::sockaddr,
                    &mut len,
                )
            };
            assert_eq!(rc, 0);
            u16::from_be(addr.sin_port)
        };
        let addr = format!("127.0.0.1:{port}");

        let _first = std::net::TcpStream::connect(&addr).unwrap();
        accept_pending_connections(&mut server, 1);
        let first_conn_id = server.connections[0].conn_id;
        server.connections[0].state = ConnState::AppConnected;
        server.connections[0]
            .set_session_setup_started_for_test(Instant::now() - Duration::from_secs(11));

        let _second = std::net::TcpStream::connect(&addr).unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            server.poll(0).unwrap();
            if server.connections.len() == 1 {
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(
            server.connections.len(),
            1,
            "the same-IP reconnect must be admitted once the stale predecessor is reaped, \
             not rejected by the per-IP cap for a connection dying in this same tick"
        );
        assert_ne!(
            server.connections[0].conn_id, first_conn_id,
            "the surviving connection must be the new reconnect, not the stale \
             predecessor left in place while the reconnect was rejected by the cap"
        );
    }

    #[test]
    fn max_pending_tls_per_addr_does_not_affect_active_connection_cap() {
        // Regression test: the active per-IP connection cap and the TLS
        // pending-handshake cap must be independently configurable. Raising
        // max_pending_tls_per_addr alone must not also raise the number of
        // plaintext connections one IP can hold.
        let config = ServerConfig {
            max_connections: 16,
            chunk_size: 128,
            tls_enabled: 0,
            tls_cert_file: std::ptr::null(),
            tls_key_file: std::ptr::null(),
            tls_ca_file: std::ptr::null(),
            tls_insecure: 0,
            max_pending_tls_per_addr: 64,
            max_connections_per_addr: 0,
        };
        let mut server = Server::new(config).unwrap();
        server.listen("127.0.0.1:0").unwrap();

        let port = {
            let mut addr: libc::sockaddr_in = unsafe { std::mem::zeroed() };
            let mut len = std::mem::size_of::<libc::sockaddr_in>() as libc::socklen_t;
            let rc = unsafe {
                libc::getsockname(
                    server.server_fd,
                    &mut addr as *mut _ as *mut libc::sockaddr,
                    &mut len,
                )
            };
            assert_eq!(rc, 0);
            u16::from_be(addr.sin_port)
        };
        let addr = format!("127.0.0.1:{port}");

        let mut streams = Vec::new();
        for _ in 0..(DEFAULT_MAX_CONNECTIONS_PER_ADDR + 2) {
            streams.push(std::net::TcpStream::connect(&addr).unwrap());
        }
        server.accept_new_connections();
        assert_eq!(
            server.connections.len(),
            DEFAULT_MAX_CONNECTIONS_PER_ADDR,
            "a large max_pending_tls_per_addr must not raise the active per-IP connection cap"
        );
    }

    #[test]
    fn stale_pre_connect_sessions_are_closed_during_poll() {
        use std::time::{Duration, Instant};

        let config = ServerConfig {
            max_connections: 4,
            chunk_size: 128,
            tls_enabled: 0,
            tls_cert_file: std::ptr::null(),
            tls_key_file: std::ptr::null(),
            tls_ca_file: std::ptr::null(),
            tls_insecure: 0,
            max_pending_tls_per_addr: 0,
            max_connections_per_addr: 0,
        };
        let mut server = Server::new(config).unwrap();
        server.listen("127.0.0.1:0").unwrap();

        let port = {
            let mut addr: libc::sockaddr_in = unsafe { std::mem::zeroed() };
            let mut len = std::mem::size_of::<libc::sockaddr_in>() as libc::socklen_t;
            let rc = unsafe {
                libc::getsockname(
                    server.server_fd,
                    &mut addr as *mut _ as *mut libc::sockaddr,
                    &mut len,
                )
            };
            assert_eq!(rc, 0);
            u16::from_be(addr.sin_port)
        };
        let addr = format!("127.0.0.1:{port}");

        let _stream = std::net::TcpStream::connect(&addr).unwrap();
        accept_pending_connections(&mut server, 1);

        server.connections[0].state = ConnState::Handshake;
        server.connections[0]
            .set_session_setup_started_for_test(Instant::now() - Duration::from_secs(11));

        server.process_connections().unwrap();
        assert_eq!(server.connections.len(), 0);
    }

    #[test]
    fn post_connect_idle_sessions_are_closed_during_poll() {
        use std::time::{Duration, Instant};

        let config = ServerConfig {
            max_connections: 4,
            chunk_size: 128,
            tls_enabled: 0,
            tls_cert_file: std::ptr::null(),
            tls_key_file: std::ptr::null(),
            tls_ca_file: std::ptr::null(),
            tls_insecure: 0,
            max_pending_tls_per_addr: 0,
            max_connections_per_addr: 0,
        };
        let mut server = Server::new(config).unwrap();
        server.listen("127.0.0.1:0").unwrap();

        let port = {
            let mut addr: libc::sockaddr_in = unsafe { std::mem::zeroed() };
            let mut len = std::mem::size_of::<libc::sockaddr_in>() as libc::socklen_t;
            let rc = unsafe {
                libc::getsockname(
                    server.server_fd,
                    &mut addr as *mut _ as *mut libc::sockaddr,
                    &mut len,
                )
            };
            assert_eq!(rc, 0);
            u16::from_be(addr.sin_port)
        };
        let addr = format!("127.0.0.1:{port}");

        let _stream = std::net::TcpStream::connect(&addr).unwrap();
        accept_pending_connections(&mut server, 1);

        server.connections[0].state = ConnState::AppConnected;
        server.connections[0]
            .set_session_setup_started_for_test(Instant::now() - Duration::from_secs(11));

        server.process_connections().unwrap();
        assert_eq!(server.connections.len(), 0);
    }

    #[test]
    fn paused_players_are_closed_during_poll() {
        use std::time::{Duration, Instant};

        let config = ServerConfig {
            max_connections: 4,
            chunk_size: 128,
            tls_enabled: 0,
            tls_cert_file: std::ptr::null(),
            tls_key_file: std::ptr::null(),
            tls_ca_file: std::ptr::null(),
            tls_insecure: 0,
            max_pending_tls_per_addr: 0,
            max_connections_per_addr: 0,
        };
        let mut server = Server::new(config).unwrap();
        server.listen("127.0.0.1:0").unwrap();

        let port = {
            let mut addr: libc::sockaddr_in = unsafe { std::mem::zeroed() };
            let mut len = std::mem::size_of::<libc::sockaddr_in>() as libc::socklen_t;
            let rc = unsafe {
                libc::getsockname(
                    server.server_fd,
                    &mut addr as *mut _ as *mut libc::sockaddr,
                    &mut len,
                )
            };
            assert_eq!(rc, 0);
            u16::from_be(addr.sin_port)
        };
        let addr = format!("127.0.0.1:{port}");

        let _stream = std::net::TcpStream::connect(&addr).unwrap();
        accept_pending_connections(&mut server, 1);

        server.connections[0].state = ConnState::Playing;
        server.connections[0].current_stream = Some(Box::new(crate::session::stream::Stream {
            stream_id: 1,
            is_playing: true,
            paused: true,
            ..crate::session::stream::Stream::new(1)
        }));
        server.connections[0]
            .set_session_setup_started_for_test(Instant::now() - Duration::from_secs(11));

        server.process_connections().unwrap();
        assert_eq!(server.connections.len(), 0);
    }

    #[test]
    fn second_publisher_on_same_route_is_rejected() {
        let server = test_server();

        let mut first = Conn::new();
        first.conn_id = 1;
        first.app = "live".to_string();
        first.current_stream = Some(Box::new(crate::session::stream::Stream {
            stream_id: 1,
            name: String::new(),
            is_publishing: false,
            is_playing: false,
            paused: false,
            receive_audio: true,
            receive_video: true,
        }));
        first.publish_routes = Some(PublishRouteRegistry::new(Arc::clone(
            &server.active_publish_routes,
        )));

        let mut second = Conn::new();
        second.conn_id = 2;
        second.app = "live".to_string();
        second.current_stream = Some(Box::new(crate::session::stream::Stream {
            stream_id: 1,
            name: String::new(),
            is_publishing: false,
            is_playing: false,
            paused: false,
            receive_audio: true,
            receive_video: true,
        }));
        second.publish_routes = Some(PublishRouteRegistry::new(Arc::clone(
            &server.active_publish_routes,
        )));

        let mut buf = crate::buffer::Buffer::with_capacity(128);
        crate::message::command::build_create_stream(&mut buf, 1.0).unwrap();
        first.handle_command(buf.as_slice()).unwrap();
        second.handle_command(buf.as_slice()).unwrap();

        let mut publish_a = crate::buffer::Buffer::with_capacity(128);
        crate::message::command::build_publish(&mut publish_a, "victim", "live").unwrap();
        first.handle_command(publish_a.as_slice()).unwrap();
        assert!(first.relay_enabled);

        let mut publish_b = crate::buffer::Buffer::with_capacity(128);
        crate::message::command::build_publish(&mut publish_b, "victim", "live").unwrap();
        second.handle_command(publish_b.as_slice()).unwrap();
        assert!(
            !second.relay_enabled,
            "second publisher on the same route must be rejected"
        );
    }

    #[test]
    fn publish_rename_onto_occupied_route_keeps_old_route_claimed() {
        let server = test_server();

        let mut first = Conn::new();
        first.conn_id = 1;
        first.app = "live".to_string();
        first.current_stream = Some(Box::new(crate::session::stream::Stream {
            stream_id: 1,
            name: String::new(),
            is_publishing: false,
            is_playing: false,
            paused: false,
            receive_audio: true,
            receive_video: true,
        }));
        first.publish_routes = Some(PublishRouteRegistry::new(Arc::clone(
            &server.active_publish_routes,
        )));

        let mut second = Conn::new();
        second.conn_id = 2;
        second.app = "live".to_string();
        second.current_stream = Some(Box::new(crate::session::stream::Stream {
            stream_id: 1,
            name: String::new(),
            is_publishing: false,
            is_playing: false,
            paused: false,
            receive_audio: true,
            receive_video: true,
        }));
        second.publish_routes = Some(PublishRouteRegistry::new(Arc::clone(
            &server.active_publish_routes,
        )));

        let mut buf = crate::buffer::Buffer::with_capacity(128);
        crate::message::command::build_create_stream(&mut buf, 1.0).unwrap();
        first.handle_command(buf.as_slice()).unwrap();
        second.handle_command(buf.as_slice()).unwrap();

        // `first` claims "a", `second` claims "b".
        let mut publish_a = crate::buffer::Buffer::with_capacity(128);
        crate::message::command::build_publish(&mut publish_a, "a", "live").unwrap();
        first.handle_command(publish_a.as_slice()).unwrap();
        assert!(first.relay_enabled);

        let mut publish_b = crate::buffer::Buffer::with_capacity(128);
        crate::message::command::build_publish(&mut publish_b, "b", "live").unwrap();
        second.handle_command(publish_b.as_slice()).unwrap();
        assert!(second.relay_enabled);

        // `first` tries to re-publish onto "b", which `second` already owns.
        // The rename must be rejected, and "a" must remain claimed by `first`
        // so a third connection cannot hijack it.
        let mut publish_rename = crate::buffer::Buffer::with_capacity(128);
        crate::message::command::build_publish(&mut publish_rename, "b", "live").unwrap();
        first.handle_command(publish_rename.as_slice()).unwrap();

        let mut third = Conn::new();
        third.conn_id = 3;
        third.app = "live".to_string();
        third.current_stream = Some(Box::new(crate::session::stream::Stream {
            stream_id: 1,
            name: String::new(),
            is_publishing: false,
            is_playing: false,
            paused: false,
            receive_audio: true,
            receive_video: true,
        }));
        third.publish_routes = Some(PublishRouteRegistry::new(Arc::clone(
            &server.active_publish_routes,
        )));
        let mut create_third = crate::buffer::Buffer::with_capacity(128);
        crate::message::command::build_create_stream(&mut create_third, 1.0).unwrap();
        third.handle_command(create_third.as_slice()).unwrap();

        let mut publish_hijack = crate::buffer::Buffer::with_capacity(128);
        crate::message::command::build_publish(&mut publish_hijack, "a", "live").unwrap();
        third.handle_command(publish_hijack.as_slice()).unwrap();
        assert!(
            !third.relay_enabled,
            "route \"a\" must stay claimed by the original publisher after a failed rename"
        );
    }

    #[test]
    fn delete_stream_releases_publish_route_for_next_publisher() {
        let server = test_server();

        let mut first = Conn::new();
        first.conn_id = 1;
        first.app = "live".to_string();
        first.current_stream = Some(Box::new(crate::session::stream::Stream {
            stream_id: 1,
            name: String::new(),
            is_publishing: false,
            is_playing: false,
            paused: false,
            receive_audio: true,
            receive_video: true,
        }));
        first.publish_routes = Some(PublishRouteRegistry::new(Arc::clone(
            &server.active_publish_routes,
        )));

        let mut create_first = crate::buffer::Buffer::with_capacity(128);
        crate::message::command::build_create_stream(&mut create_first, 1.0).unwrap();
        first.handle_command(create_first.as_slice()).unwrap();

        let mut publish_a = crate::buffer::Buffer::with_capacity(128);
        crate::message::command::build_publish(&mut publish_a, "victim", "live").unwrap();
        first.handle_command(publish_a.as_slice()).unwrap();
        assert!(first.relay_enabled);

        // Client unpublishes but keeps the TCP connection open.
        let mut delete_stream = crate::buffer::Buffer::with_capacity(128);
        crate::message::command::build_deletestream(&mut delete_stream, 1.0, 1).unwrap();
        first.handle_command(delete_stream.as_slice()).unwrap();
        assert!(
            !first.relay_enabled,
            "relay_enabled must be cleared along with the publish role, so a later \
             publish under defer_media_relay can't relay before being re-authorized"
        );

        let mut second = Conn::new();
        second.conn_id = 2;
        second.app = "live".to_string();
        second.current_stream = Some(Box::new(crate::session::stream::Stream {
            stream_id: 1,
            name: String::new(),
            is_publishing: false,
            is_playing: false,
            paused: false,
            receive_audio: true,
            receive_video: true,
        }));
        second.publish_routes = Some(PublishRouteRegistry::new(Arc::clone(
            &server.active_publish_routes,
        )));
        let mut create_second = crate::buffer::Buffer::with_capacity(128);
        crate::message::command::build_create_stream(&mut create_second, 1.0).unwrap();
        second.handle_command(create_second.as_slice()).unwrap();

        let mut publish_b = crate::buffer::Buffer::with_capacity(128);
        crate::message::command::build_publish(&mut publish_b, "victim", "live").unwrap();
        second.handle_command(publish_b.as_slice()).unwrap();
        assert!(
            second.relay_enabled,
            "route must be free for a new publisher after deleteStream"
        );
    }

    #[test]
    fn closed_connections_release_routes_before_later_publish_in_same_batch() {
        use crate::chunk::reader::ChunkMessage;
        use crate::chunk::writer::chunk_write;
        use crate::message::message::RTMP_MSG_AMF0_COMMAND;
        use crate::session::stream::Stream;
        use crate::transport::Transport;
        use crate::types::ConnState;
        use std::os::unix::io::IntoRawFd;
        use std::os::unix::net::UnixStream;

        let mut server = test_server();

        // Connection A already owns the "victim" route and is about to be
        // detected as closed (peer hung up) during this same
        // process_connections() pass.
        let mut conn_a = Conn::new();
        conn_a.conn_id = 1;
        conn_a.app = "live".to_string();
        conn_a.current_stream = Some(Box::new(Stream {
            stream_id: 1,
            name: "victim".to_string(),
            is_publishing: true,
            is_playing: false,
            paused: false,
            receive_audio: true,
            receive_video: true,
        }));
        conn_a.publish_routes = Some(PublishRouteRegistry::new(Arc::clone(
            &server.active_publish_routes,
        )));
        server
            .active_publish_routes
            .lock()
            .unwrap()
            .insert(("live".to_string(), "victim".to_string()), conn_a.conn_id);
        let (a_end, a_peer) = UnixStream::pair().unwrap();
        a_end.set_nonblocking(true).unwrap();
        conn_a.transport = Some(Transport::new_plain(a_end.into_raw_fd()));
        // Drop the peer side so conn_a's transport.recv() observes EOF (n == 0)
        // when process_connections() reads it, simulating the publisher
        // disconnecting.
        drop(a_peer);

        // Connection B is a second, already-connected client publishing the
        // same route later in the same connections vector.
        let mut conn_b = Conn::new();
        conn_b.conn_id = 2;
        conn_b.app = "live".to_string();
        conn_b.state = ConnState::StreamCreated;
        conn_b.current_stream = Some(Box::new(Stream {
            stream_id: 1,
            name: String::new(),
            is_publishing: false,
            is_playing: false,
            paused: false,
            receive_audio: true,
            receive_video: true,
        }));
        conn_b.publish_routes = Some(PublishRouteRegistry::new(Arc::clone(
            &server.active_publish_routes,
        )));
        let (b_end, mut b_peer) = UnixStream::pair().unwrap();
        b_end.set_nonblocking(true).unwrap();
        conn_b.transport = Some(Transport::new_plain(b_end.into_raw_fd()));

        let mut publish_cmd = crate::buffer::Buffer::with_capacity(128);
        crate::message::command::build_publish(&mut publish_cmd, "victim", "live").unwrap();
        let payload_len = publish_cmd.available();
        let mut wire = crate::buffer::Buffer::new();
        let mut cmsg = ChunkMessage::default();
        cmsg.csid = 3;
        cmsg.fmt = 0;
        cmsg.msg_length = payload_len as u32;
        cmsg.msg_type_id = RTMP_MSG_AMF0_COMMAND;
        cmsg.msg_stream_id = 1;
        chunk_write(&mut wire, &cmsg, publish_cmd.as_slice(), payload_len, 128).unwrap();
        use std::io::Write;
        b_peer.write_all(wire.peek()).unwrap();

        server.connections = vec![conn_a, conn_b];
        server.process_connections().unwrap();

        assert_eq!(
            server.connections.len(),
            1,
            "conn_a should have been removed"
        );
        assert!(
            server.connections[0].relay_enabled,
            "conn_b's publish must succeed in the same batch conn_a's route was freed in"
        );
    }

    #[cfg(feature = "tls")]
    fn self_signed_cert_files(cn: &str) -> (std::path::PathBuf, std::path::PathBuf) {
        use openssl::asn1::Asn1Time;
        use openssl::hash::MessageDigest;
        use openssl::nid::Nid;
        use openssl::pkey::PKey;
        use openssl::rsa::Rsa;
        use openssl::x509::extension::{BasicConstraints, SubjectAlternativeName};
        use openssl::x509::{X509, X509NameBuilder};
        use std::sync::atomic::{AtomicU32, Ordering};

        let rsa = Rsa::generate(2048).unwrap();
        let pkey = PKey::from_rsa(rsa).unwrap();
        let mut name = X509NameBuilder::new().unwrap();
        name.append_entry_by_nid(Nid::COMMONNAME, cn).unwrap();
        let name = name.build();
        let mut builder = X509::builder().unwrap();
        builder.set_version(2).unwrap();
        builder.set_subject_name(&name).unwrap();
        builder.set_issuer_name(&name).unwrap();
        builder.set_pubkey(&pkey).unwrap();
        builder
            .set_not_before(&Asn1Time::days_from_now(0).unwrap())
            .unwrap();
        builder
            .set_not_after(&Asn1Time::days_from_now(1).unwrap())
            .unwrap();
        builder
            .append_extension(BasicConstraints::new().critical().ca().build().unwrap())
            .unwrap();
        let san = SubjectAlternativeName::new()
            .dns(cn)
            .build(&builder.x509v3_context(None, None))
            .unwrap();
        builder.append_extension(san).unwrap();
        builder.sign(&pkey, MessageDigest::sha256()).unwrap();
        let cert = builder.build();

        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let base = std::env::temp_dir().join(format!(
            "librtmp2-server-test-{}-{}-{}",
            std::process::id(),
            n,
            cn
        ));
        let cert_path = base.with_extension("cert.pem");
        let key_path = base.with_extension("key.pem");
        std::fs::write(&cert_path, cert.to_pem().unwrap()).unwrap();
        std::fs::write(&key_path, pkey.private_key_to_pem_pkcs8().unwrap()).unwrap();
        (cert_path, key_path)
    }

    #[cfg(feature = "tls")]
    #[test]
    fn pending_tls_queue_caps_incomplete_handshakes_per_remote_addr() {
        let (cert_path, key_path) = self_signed_cert_files("pending-tls.test");
        let mut server = test_server();
        server
            .listen_tls(
                "127.0.0.1:0",
                cert_path.to_str().unwrap(),
                key_path.to_str().unwrap(),
            )
            .unwrap();

        let port = {
            let mut addr: libc::sockaddr_in = unsafe { std::mem::zeroed() };
            let mut len = std::mem::size_of::<libc::sockaddr_in>() as libc::socklen_t;
            let rc = unsafe {
                libc::getsockname(
                    server.server_fd,
                    &mut addr as *mut _ as *mut libc::sockaddr,
                    &mut len,
                )
            };
            assert_eq!(rc, 0);
            u16::from_be(addr.sin_port)
        };
        let addr = format!("127.0.0.1:{port}");

        let mut clients = Vec::new();
        for _ in 0..(DEFAULT_MAX_PENDING_TLS_PER_ADDR + 2) {
            clients.push(std::net::TcpStream::connect(&addr).unwrap());
            server.accept_new_connections();
        }

        // Every client above connects from "127.0.0.1" but through a distinct
        // ephemeral source port, so the cap must be keyed on the peer IP —
        // not the full `ip:port` peer address — or a single host can bypass
        // it by opening from a fresh source port each time.
        assert_eq!(
            server.pending_tls_count_for_addr("127.0.0.1"),
            DEFAULT_MAX_PENDING_TLS_PER_ADDR,
            "one peer should retain exactly the per-IP pending TLS cap"
        );
        assert!(
            server.pending_tls_count() <= MAX_PENDING_TLS_HANDSHAKES,
            "global pending TLS cap must hold"
        );

        let _ = std::fs::remove_file(cert_path);
        let _ = std::fs::remove_file(key_path);
    }

    #[cfg(feature = "tls")]
    #[test]
    fn pending_tls_per_addr_cap_rejects_new_handshake_instead_of_evicting_oldest() {
        let (cert_path, key_path) = self_signed_cert_files("pending-tls-per-addr-no-evict.test");
        let mut server = test_server();
        server
            .listen_tls(
                "127.0.0.1:0",
                cert_path.to_str().unwrap(),
                key_path.to_str().unwrap(),
            )
            .unwrap();

        let port = {
            let mut addr: libc::sockaddr_in = unsafe { std::mem::zeroed() };
            let mut len = std::mem::size_of::<libc::sockaddr_in>() as libc::socklen_t;
            let rc = unsafe {
                libc::getsockname(
                    server.server_fd,
                    &mut addr as *mut _ as *mut libc::sockaddr,
                    &mut len,
                )
            };
            assert_eq!(rc, 0);
            u16::from_be(addr.sin_port)
        };
        let addr = format!("127.0.0.1:{port}");

        let mut clients = Vec::new();
        for _ in 0..DEFAULT_MAX_PENDING_TLS_PER_ADDR {
            clients.push(std::net::TcpStream::connect(&addr).unwrap());
            server.accept_new_connections();
        }
        assert_eq!(
            server.pending_tls_count_for_addr("127.0.0.1"),
            DEFAULT_MAX_PENDING_TLS_PER_ADDR
        );
        let oldest = server.pending_tls[0].remote_addr.clone();

        clients.push(std::net::TcpStream::connect(&addr).unwrap());
        server.accept_new_connections();

        assert_eq!(
            server.pending_tls_count_for_addr("127.0.0.1"),
            DEFAULT_MAX_PENDING_TLS_PER_ADDR
        );
        assert_eq!(
            server.pending_tls[0].remote_addr, oldest,
            "per-IP pending TLS cap must reject new handshakes instead of evicting same-IP peers"
        );

        let _ = std::fs::remove_file(cert_path);
        let _ = std::fs::remove_file(key_path);
    }

    #[cfg(feature = "tls")]
    #[test]
    fn pending_tls_per_addr_cap_is_configurable() {
        const CUSTOM_CAP: usize = 2;
        let (cert_path, key_path) = self_signed_cert_files("pending-tls-custom-cap.test");
        let mut server = Server::new(ServerConfig {
            max_connections: 8,
            chunk_size: 128,
            tls_enabled: 0,
            tls_cert_file: std::ptr::null(),
            tls_key_file: std::ptr::null(),
            tls_ca_file: std::ptr::null(),
            tls_insecure: 0,
            max_pending_tls_per_addr: CUSTOM_CAP as std::ffi::c_int,
            max_connections_per_addr: 0,
        })
        .unwrap();
        server
            .listen_tls(
                "127.0.0.1:0",
                cert_path.to_str().unwrap(),
                key_path.to_str().unwrap(),
            )
            .unwrap();

        let port = {
            let mut addr: libc::sockaddr_in = unsafe { std::mem::zeroed() };
            let mut len = std::mem::size_of::<libc::sockaddr_in>() as libc::socklen_t;
            let rc = unsafe {
                libc::getsockname(
                    server.server_fd,
                    &mut addr as *mut _ as *mut libc::sockaddr,
                    &mut len,
                )
            };
            assert_eq!(rc, 0);
            u16::from_be(addr.sin_port)
        };
        let addr = format!("127.0.0.1:{port}");

        let mut clients = Vec::new();
        for _ in 0..(CUSTOM_CAP + 2) {
            clients.push(std::net::TcpStream::connect(&addr).unwrap());
            server.accept_new_connections();
        }

        assert_eq!(
            server.pending_tls_count_for_addr("127.0.0.1"),
            CUSTOM_CAP,
            "a configured max_pending_tls_per_addr must override the built-in default"
        );

        let _ = std::fs::remove_file(cert_path);
        let _ = std::fs::remove_file(key_path);
    }

    #[cfg(feature = "tls")]
    #[test]
    fn peer_ip_strips_port_from_socket_addr_string() {
        assert_eq!(Server::peer_ip("127.0.0.1:54321"), "127.0.0.1");
        assert_eq!(Server::peer_ip("[::1]:54321"), "::1");
        assert_eq!(Server::peer_ip("[2001:db8::1]:443"), "2001:db8::1");
        // IPv4-mapped IPv6 must collapse to the IPv4 form for per-IP caps.
        assert_eq!(
            Server::peer_ip("[::ffff:203.0.113.5]:54321"),
            "203.0.113.5"
        );
        // No port present: falls back to the input unchanged.
        assert_eq!(Server::peer_ip("127.0.0.1"), "127.0.0.1");
    }

    #[test]
    fn peer_ip_canonicalizes_ipv4_mapped_ipv6_for_per_addr_caps() {
        let mut server = test_server();
        for i in 0..DEFAULT_MAX_CONNECTIONS_PER_ADDR {
            let mut conn = Conn::new();
            conn.remote_addr = format!("203.0.113.5:{i}");
            server.connections.push(conn);
        }
        let mapped = Server::peer_ip("[::ffff:203.0.113.5]:9999");
        assert_eq!(mapped, "203.0.113.5");
        assert_eq!(
            server.active_connections_for_ip(&mapped),
            DEFAULT_MAX_CONNECTIONS_PER_ADDR,
            "IPv4-mapped IPv6 must count against the same per-IP cap as IPv4"
        );
    }

    #[cfg(feature = "tls")]
    #[test]
    fn pending_tls_global_cap_rejects_new_handshake_instead_of_evicting_oldest() {
        let (cert_path, key_path) = self_signed_cert_files("pending-tls-no-evict.test");
        let mut server = test_server();
        // Isolate the global pending-TLS cap from the lower limits used by
        // test_server(), otherwise this test can never fill the global queue.
        server.config.max_connections = 0;
        server.config.max_pending_tls_per_addr =
            (MAX_PENDING_TLS_HANDSHAKES + 1) as std::ffi::c_int;
        server
            .listen_tls(
                "127.0.0.1:0",
                cert_path.to_str().unwrap(),
                key_path.to_str().unwrap(),
            )
            .unwrap();

        let port = {
            let mut addr: libc::sockaddr_in = unsafe { std::mem::zeroed() };
            let mut len = std::mem::size_of::<libc::sockaddr_in>() as libc::socklen_t;
            let rc = unsafe {
                libc::getsockname(
                    server.server_fd,
                    &mut addr as *mut _ as *mut libc::sockaddr,
                    &mut len,
                )
            };
            assert_eq!(rc, 0);
            u16::from_be(addr.sin_port)
        };
        let addr = format!("127.0.0.1:{port}");

        let mut clients = Vec::new();
        for _ in 0..MAX_PENDING_TLS_HANDSHAKES {
            clients.push(std::net::TcpStream::connect(&addr).unwrap());
            server.accept_new_connections();
        }
        assert_eq!(server.pending_tls_count(), MAX_PENDING_TLS_HANDSHAKES);
        let oldest = server.pending_tls[0].remote_addr.clone();

        clients.push(std::net::TcpStream::connect(&addr).unwrap());
        server.accept_new_connections();

        assert_eq!(server.pending_tls_count(), MAX_PENDING_TLS_HANDSHAKES);
        assert_eq!(
            server.pending_tls[0].remote_addr, oldest,
            "global pending TLS cap must reject new handshakes instead of evicting unrelated peers"
        );

        let _ = std::fs::remove_file(cert_path);
        let _ = std::fs::remove_file(key_path);
    }

    #[test]
    fn relay_export_disabled_drains_empty() {
        let mut server = test_server();
        assert!(server.drain_exported_relay_frames().is_empty());

        let mut publisher = Conn::new();
        publisher.conn_id = 1;
        publisher.app = "live".to_string();
        publisher
            .pending_relay
            .push(relay_frame(FrameType::Video, vec![0x17, 0x01, 0xAA]));
        server.connections = vec![publisher];
        server.process_connections().unwrap();
        assert!(
            server.drain_exported_relay_frames().is_empty(),
            "disabled export must not retain clones"
        );
    }

    #[test]
    fn relay_export_captures_publisher_media_kinds() {
        let mut server = test_server();
        server.enable_relay_export(64, 1024 * 1024);

        let mut metadata = vec![0x02, 0x00, 0x0A];
        metadata.extend_from_slice(b"onMetaData");
        metadata.push(crate::amf::amf0::Amf0Type::Object as u8);
        metadata.extend_from_slice(&[0x00, 0x00, 0x09]);

        let mut publisher = Conn::new();
        publisher.conn_id = 1;
        publisher.app = "live".to_string();
        publisher.pending_relay.extend([
            relay_frame(FrameType::Script, metadata.clone()),
            relay_frame(FrameType::Audio, vec![0xAF, 0x00, 0x11, 0x90]),
            relay_frame(FrameType::Video, vec![0x17, 0x00, 0x01, 0x02]),
            relay_frame(FrameType::Video, vec![0x17, 0x01, 0xDE, 0xAD]),
        ]);
        server.connections = vec![publisher];
        server.process_connections().unwrap();

        let exported = server.drain_exported_relay_frames();
        assert_eq!(exported.len(), 4);
        assert_eq!(exported[0].frame_type, FrameType::Script);
        assert_eq!(exported[0].payload, metadata);
        assert_eq!(exported[1].frame_type, FrameType::Audio);
        assert_eq!(exported[2].frame_type, FrameType::Video);
        assert_eq!(exported[2].payload, vec![0x17, 0x00, 0x01, 0x02]);
        assert_eq!(exported[3].frame_type, FrameType::Video);
        assert_eq!(exported[3].payload, vec![0x17, 0x01, 0xDE, 0xAD]);
        assert!(
            server.drain_exported_relay_frames().is_empty(),
            "second drain must be empty"
        );
    }

    #[test]
    fn relay_export_drops_oldest_on_overflow() {
        let mut server = test_server();
        server.enable_relay_export(2, 1024);

        let mut publisher = Conn::new();
        publisher.conn_id = 1;
        publisher.app = "live".to_string();
        publisher.pending_relay.extend([
            relay_frame(FrameType::Video, vec![0x01]),
            relay_frame(FrameType::Video, vec![0x02]),
            relay_frame(FrameType::Video, vec![0x03]),
        ]);
        server.connections = vec![publisher];
        server.process_connections().unwrap();

        let exported = server.drain_exported_relay_frames();
        assert_eq!(exported.len(), 2);
        assert_eq!(exported[0].payload, vec![0x02]);
        assert_eq!(exported[1].payload, vec![0x03]);
    }

    #[test]
    fn relay_export_respects_byte_budget() {
        let mut server = test_server();
        // Each frame retains 3 payload + 4 ("live") + 6 ("stream") = 13 bytes.
        // Budget 26 fits two frames; a third forces dropping the oldest.
        server.enable_relay_export(16, 26);

        let mut publisher = Conn::new();
        publisher.conn_id = 1;
        publisher.app = "live".to_string();
        publisher.pending_relay.extend([
            relay_frame(FrameType::Video, vec![0xAA, 0xAA, 0xAA]),
            relay_frame(FrameType::Video, vec![0xBB, 0xBB, 0xBB]),
            relay_frame(FrameType::Video, vec![0xCC, 0xCC, 0xCC]),
        ]);
        server.connections = vec![publisher];
        server.process_connections().unwrap();

        let exported = server.drain_exported_relay_frames();
        assert_eq!(exported.len(), 2);
        assert_eq!(exported[0].payload, vec![0xBB, 0xBB, 0xBB]);
        assert_eq!(exported[1].payload, vec![0xCC, 0xCC, 0xCC]);
    }

    #[test]
    fn relay_export_clears_buffer_when_frame_exceeds_budget() {
        let mut server = test_server();
        // Budget fits one small frame (1 + 4 + 6 = 11) but not a 32-byte payload
        // frame (32 + 10 = 42).
        server.enable_relay_export(16, 20);

        let mut publisher = Conn::new();
        publisher.conn_id = 1;
        publisher.app = "live".to_string();
        publisher.pending_relay.extend([
            relay_frame(FrameType::Video, vec![0x01]),
            relay_frame(FrameType::Video, vec![0xFF; 32]),
        ]);
        server.connections = vec![publisher];
        server.process_connections().unwrap();

        assert!(
            server.drain_exported_relay_frames().is_empty(),
            "oversized frame must clear stale buffered exports before skip"
        );
    }

    #[test]
    fn same_route_local_deferred_precedes_inject_under_budget() {
        use crate::session::stream::Stream;
        use crate::transport::Transport;
        use std::os::unix::io::IntoRawFd;
        use std::os::unix::net::UnixStream;

        fn attached(conn_id: u64, publishing: bool) -> (Conn, UnixStream) {
            let (server_end, peer_end) = UnixStream::pair().unwrap();
            server_end.set_nonblocking(true).unwrap();
            peer_end.set_nonblocking(true).unwrap();
            let mut conn = Conn::new();
            conn.conn_id = conn_id;
            conn.app = "live".to_string();
            conn.relay_enabled = true;
            conn.transport = Some(Transport::new_plain(server_end.into_raw_fd()));
            conn.current_stream = Some(Box::new(Stream {
                stream_id: 1,
                name: "stream".to_string(),
                is_publishing: publishing,
                is_playing: !publishing,
                paused: false,
                receive_audio: true,
                receive_video: true,
            }));
            (conn, peer_end)
        }

        let (mut publisher, _publisher_peer) = attached(1, true);
        publisher.pending_relay.extend([
            relay_frame(FrameType::Video, vec![0xB1]),
            relay_frame(FrameType::Video, vec![0xB2]),
        ]);
        let (player, _player_peer) = attached(2, false);

        let mut server = test_server();
        // One eligible player => each frame costs 1 send. Same-route merge
        // keeps local deferred frames ahead of injects (L1, L2, I1, I2);
        // budget 2 therefore completes both locals and leaves both injects.
        server.max_relay_sends_per_poll = 2;
        server.enable_relay_export(16, 1024);
        server.connections = vec![publisher, player];
        server
            .inject_relay_frame("live", "stream", FrameType::Video, 1, &[0xA1])
            .unwrap();
        server
            .inject_relay_frame("live", "stream", FrameType::Video, 2, &[0xA2])
            .unwrap();

        server.process_connections().unwrap();

        let exported = server.drain_exported_relay_frames();
        assert_eq!(
            exported.len(),
            2,
            "same-route handoff must drain local deferred frames before injects"
        );
        assert_eq!(exported[0].payload, vec![0xB1]);
        assert_eq!(exported[1].payload, vec![0xB2]);

        assert_eq!(server.pending_injected_relay.len(), 2);
        assert_eq!(server.pending_injected_relay[0].payload, vec![0xA1]);
        assert_eq!(server.pending_injected_relay[1].payload, vec![0xA2]);
        assert!(server.connections[0].pending_relay.is_empty());
    }

    #[test]
    #[should_panic(expected = "external publisher id range")]
    fn set_conn_id_base_rejects_high_bit() {
        let mut server = test_server();
        server.set_conn_id_base(EXTERNAL_PUBLISHER_ID_BIT);
    }

    #[test]
    fn inject_relay_frame_rejects_oversized_route_component() {
        let mut server = test_server();
        let long_name = "x".repeat(MAX_INJECT_ROUTE_COMPONENT_BYTES + 1);
        assert_eq!(
            server.inject_relay_frame(&long_name, "stream", FrameType::Video, 0, b"x"),
            Err(ErrorCode::Internal)
        );
        assert_eq!(
            server.inject_relay_frame("live", &long_name, FrameType::Video, 0, b"x"),
            Err(ErrorCode::Internal)
        );
    }

    #[test]
    fn inject_relay_frame_reaches_local_player() {
        use crate::session::stream::Stream;
        use crate::transport::Transport;
        use std::io::Read;
        use std::os::unix::io::IntoRawFd;
        use std::os::unix::net::UnixStream;

        let (server_end, mut peer_end) = UnixStream::pair().unwrap();
        server_end.set_nonblocking(true).unwrap();
        peer_end.set_nonblocking(true).unwrap();

        let mut player = Conn::new();
        player.conn_id = 2;
        player.app = "live".to_string();
        player.relay_enabled = true;
        player.client_fd = 0;
        player.transport = Some(Transport::new_plain(server_end.into_raw_fd()));
        player.current_stream = Some(Box::new(Stream {
            stream_id: 1,
            name: "stream".to_string(),
            is_publishing: false,
            is_playing: true,
            paused: false,
            receive_audio: true,
            receive_video: true,
        }));

        let mut server = test_server();
        server.connections = vec![player];
        server
            .inject_relay_frame("live", "stream", FrameType::Video, 40, &[0x27, 0x01, 0xBE])
            .unwrap();
        server.process_connections().unwrap();

        let mut buf = [0u8; 4096];
        let n = peer_end
            .read(&mut buf)
            .expect("player should receive injected frame");
        assert!(n > 0, "injected video must reach the playing client");
    }

    #[test]
    fn inject_headers_and_keyframe_seed_init_cache_for_late_player() {
        use crate::session::stream::Stream;
        use crate::transport::Transport;
        use std::io::Read;
        use std::os::unix::io::IntoRawFd;
        use std::os::unix::net::UnixStream;

        let mut server = test_server();
        server
            .inject_relay_frame("live", "stream", FrameType::Video, 0, &[0x17, 0x00, 0x01])
            .unwrap();
        server
            .inject_relay_frame(
                "live",
                "stream",
                FrameType::Audio,
                0,
                &[0xAF, 0x00, 0x11, 0x90],
            )
            .unwrap();
        server
            .inject_relay_frame("live", "stream", FrameType::Video, 100, &[0x17, 0x01, 0xDE])
            .unwrap();
        server.process_connections().unwrap();

        let snap = server
            .stream_init_snapshot("live", "stream")
            .expect("cache entry after inject");
        assert!(snap.avc_header.is_some());
        assert!(snap.aac_header.is_some());
        assert!(snap.last_keyframe.is_some());
        assert_eq!(snap.last_keyframe.as_ref().unwrap().0, 100);

        let (server_end, mut peer_end) = UnixStream::pair().unwrap();
        server_end.set_nonblocking(true).unwrap();
        peer_end.set_nonblocking(true).unwrap();

        let mut player = Conn::new();
        player.conn_id = 3;
        player.app = "live".to_string();
        player.relay_enabled = true;
        player.needs_init_frames = true;
        player.client_fd = 0;
        player.transport = Some(Transport::new_plain(server_end.into_raw_fd()));
        player.current_stream = Some(Box::new(Stream {
            stream_id: 1,
            name: "stream".to_string(),
            is_publishing: false,
            is_playing: true,
            paused: false,
            receive_audio: true,
            receive_video: true,
        }));
        server.connections = vec![player];
        server.process_connections().unwrap();

        let mut buf = [0u8; 8192];
        let n = peer_end
            .read(&mut buf)
            .expect("late joiner should get init replay");
        assert!(n > 0, "cached headers/keyframe must replay to late player");
    }

    #[test]
    fn inject_relay_frame_respects_pending_limits() {
        let mut server = test_server();
        // Route strings ("live" + "stream") count toward the byte budget.
        server.resource_limits.max_pending_relay_bytes = 14;
        assert!(
            server
                .inject_relay_frame("live", "stream", FrameType::Video, 0, b"abcd")
                .is_ok()
        );
        assert_eq!(
            server.inject_relay_frame("live", "stream", FrameType::Video, 0, b"x"),
            Err(ErrorCode::Internal)
        );
    }

    #[test]
    fn inject_relay_frame_rejects_oversized_payload() {
        let mut server = test_server();
        let oversized = vec![0u8; DEFAULT_MAX_MSG_LENGTH as usize + 1];
        assert_eq!(
            server.inject_relay_frame("live", "stream", FrameType::Video, 0, &oversized),
            Err(ErrorCode::Internal)
        );
    }

    #[test]
    fn inject_relay_frame_rejects_empty_route_components() {
        let mut server = test_server();
        assert_eq!(
            server.inject_relay_frame("", "stream", FrameType::Video, 0, b"x"),
            Err(ErrorCode::Internal)
        );
        assert_eq!(
            server.inject_relay_frame("live", "", FrameType::Video, 0, b"x"),
            Err(ErrorCode::Internal)
        );
    }

    #[test]
    fn relay_export_skips_requeued_frames() {
        use crate::session::stream::Stream;
        use crate::transport::Transport;
        use std::os::unix::io::IntoRawFd;
        use std::os::unix::net::UnixStream;

        fn attached(conn_id: u64, publishing: bool) -> (Conn, UnixStream) {
            let (server_end, peer_end) = UnixStream::pair().unwrap();
            server_end.set_nonblocking(true).unwrap();
            peer_end.set_nonblocking(true).unwrap();
            let mut conn = Conn::new();
            conn.conn_id = conn_id;
            conn.app = "live".to_string();
            conn.relay_enabled = true;
            conn.transport = Some(Transport::new_plain(server_end.into_raw_fd()));
            conn.current_stream = Some(Box::new(Stream {
                stream_id: 1,
                name: "stream".to_string(),
                is_publishing: publishing,
                is_playing: !publishing,
                paused: false,
                receive_audio: true,
                receive_video: true,
            }));
            (conn, peer_end)
        }

        let (mut publisher, _publisher_peer) = attached(1, true);
        publisher.pending_relay.extend([
            relay_frame(FrameType::Video, vec![0x11]),
            relay_frame(FrameType::Video, vec![0x22]),
        ]);
        let (player, _player_peer) = attached(2, false);

        let mut server = test_server();
        server.max_relay_sends_per_poll = 1;
        server.enable_relay_export(16, 1024);
        server.connections = vec![publisher, player];

        server.process_connections().unwrap();
        let first = server.drain_exported_relay_frames();
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].payload, vec![0x11]);

        server.process_connections().unwrap();
        let second = server.drain_exported_relay_frames();
        assert_eq!(second.len(), 1);
        assert_eq!(
            second[0].payload,
            vec![0x22],
            "deferred frame must export once on the poll that processes it"
        );
    }

    #[test]
    fn relay_export_orphaned_frames_on_next_poll_after_publisher_removed() {
        use crate::session::stream::Stream;
        use crate::transport::Transport;
        use std::os::unix::io::IntoRawFd;
        use std::os::unix::net::UnixStream;

        let (server_end, _peer_end) = UnixStream::pair().unwrap();
        server_end.set_nonblocking(true).unwrap();

        let mut publisher = Conn::new();
        publisher.conn_id = 1;
        publisher.app = "live".to_string();
        // No transport: removed later this poll after budget requeue.
        publisher.pending_relay.extend([
            relay_frame(FrameType::Video, vec![0x11]),
            relay_frame(FrameType::Video, vec![0x22]),
        ]);

        let mut player = Conn::new();
        player.conn_id = 2;
        player.app = "live".to_string();
        player.relay_enabled = true;
        player.transport = Some(Transport::new_plain(server_end.into_raw_fd()));
        player.current_stream = Some(Box::new(Stream {
            stream_id: 1,
            name: "stream".to_string(),
            is_publishing: false,
            is_playing: true,
            paused: false,
            receive_audio: true,
            receive_video: true,
        }));

        let mut server = test_server();
        server.max_relay_sends_per_poll = 1;
        server.enable_relay_export(16, 1024);
        server.connections = vec![publisher, player];
        server.process_connections().unwrap();

        let exported = server.drain_exported_relay_frames();
        assert_eq!(
            exported.len(),
            1,
            "first frame exports on the completed-frame path this poll"
        );
        assert_eq!(exported[0].payload, vec![0x11]);
        assert!(
            server.connections.iter().all(|c| c.conn_id != 1),
            "socket-less publisher must be removed"
        );

        // Budget-deferred remainder was orphaned without export; next poll
        // fans it out and exports once (no teardown double-export).
        server.process_connections().unwrap();
        let exported = server.drain_exported_relay_frames();
        assert_eq!(
            exported.len(),
            1,
            "orphaned deferred frame must export on the next poll fan-out"
        );
        assert_eq!(exported[0].payload, vec![0x22]);
    }

    #[test]
    fn injected_frames_respect_receive_audio_video() {
        use crate::session::stream::Stream;
        use crate::transport::Transport;
        use std::os::unix::io::IntoRawFd;
        use std::os::unix::net::UnixStream;

        let (server_end, _peer_end) = UnixStream::pair().unwrap();
        server_end.set_nonblocking(true).unwrap();

        let mut player = Conn::new();
        player.app = "live".to_string();
        player.relay_enabled = true;
        player.transport = Some(Transport::new_plain(server_end.into_raw_fd()));
        player.current_stream = Some(Box::new(Stream {
            stream_id: 1,
            name: "stream".to_string(),
            is_publishing: false,
            is_playing: true,
            paused: false,
            receive_audio: false,
            receive_video: true,
        }));

        let audio = RelayFrame {
            app: "live".to_string(),
            stream_name: "stream".to_string(),
            publisher_conn_id: EXTERNAL_RELAY_PUBLISHER_ID,
            frame_type: FrameType::Audio,
            timestamp: 0,
            cache_payload: None,
            payload: vec![0xAF, 0x01, 0x00],
        };
        let video = RelayFrame {
            app: "live".to_string(),
            stream_name: "stream".to_string(),
            publisher_conn_id: EXTERNAL_RELAY_PUBLISHER_ID,
            frame_type: FrameType::Video,
            timestamp: 0,
            cache_payload: None,
            payload: vec![0x27, 0x01, 0x00],
        };
        assert!(!Server::conn_will_receive_relay_frame(&player, &audio));
        assert!(Server::conn_will_receive_relay_frame(&player, &video));

        player.current_stream.as_mut().unwrap().receive_video = false;
        player.current_stream.as_mut().unwrap().receive_audio = true;
        assert!(Server::conn_will_receive_relay_frame(&player, &audio));
        assert!(!Server::conn_will_receive_relay_frame(&player, &video));
    }

    #[test]
    fn conn_inject_relay_frame_queues_without_auth_callback() {
        static mut MEDIA_CB_HIT: bool = false;
        fn deny_media(_conn_id: u64, _ft: FrameType, _codec: Option<&str>) -> bool {
            unsafe {
                MEDIA_CB_HIT = true;
            }
            false
        }

        let mut conn = Conn::new();
        conn.app = "live".to_string();
        conn.current_stream = Some(Box::new(crate::session::stream::Stream {
            stream_id: 1,
            name: "cam".to_string(),
            is_publishing: true,
            is_playing: false,
            paused: false,
            receive_audio: true,
            receive_video: true,
        }));
        conn.on_media_cb = Some(deny_media);
        conn.inject_relay_frame(FrameType::Video, 10, &[0x17, 0x01, 0xAA])
            .unwrap();
        assert_eq!(conn.pending_relay.len(), 1);
        assert_eq!(conn.pending_relay[0].timestamp, 10);
        assert_eq!(conn.pending_relay[0].payload, vec![0x17, 0x01, 0xAA]);
        assert!(!unsafe { MEDIA_CB_HIT }, "inject must skip on_media_cb");
    }

    #[test]
    fn conn_inject_relay_frame_rejects_playing_only_connection() {
        use crate::session::stream::Stream;

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
    fn conn_inject_relay_frame_rejects_foreign_route() {
        use std::collections::HashMap;
        use std::sync::{Arc, Mutex};

        let routes = Arc::new(Mutex::new(HashMap::new()));
        routes
            .lock()
            .unwrap()
            .insert(("live".to_string(), "cam".to_string()), 99);
        let registry = PublishRouteRegistry::new(Arc::clone(&routes));

        let mut conn = Conn::new();
        conn.conn_id = 7;
        conn.app = "live".to_string();
        conn.current_stream = Some(Box::new(crate::session::stream::Stream {
            stream_id: 1,
            name: "cam".to_string(),
            is_publishing: true,
            is_playing: true,
            paused: false,
            receive_audio: true,
            receive_video: true,
        }));
        conn.publish_routes = Some(registry);
        assert!(
            conn.inject_relay_frame(FrameType::Video, 0, &[0x17, 0x01, 0xAA])
                .is_err(),
            "connection-level inject must not bypass publish-route ownership"
        );
    }

    #[test]
    fn inject_rejects_route_owned_by_socket_publisher() {
        let mut server = test_server();
        let route = ("live".to_string(), "stream".to_string());
        server
            .active_publish_routes
            .lock()
            .unwrap()
            .insert(route, 7);
        assert!(
            server
                .inject_relay_frame("live", "stream", FrameType::Video, 0, &[0x17, 0x01])
                .is_err(),
            "inject must not share a route claimed by a socket publisher"
        );
        assert!(
            server.external_route_ids.is_empty(),
            "failed inject must not allocate an external route id"
        );
        assert!(
            server.inject_route_last_seen.is_empty(),
            "failed inject must not record last-seen for a never-queued route"
        );
    }

    #[test]
    fn failed_inject_does_not_leak_external_route_id_when_pending_full() {
        let mut server = test_server();
        server.resource_limits.max_pending_relay_bytes = 8;
        assert!(
            server
                .inject_relay_frame(
                    "live",
                    "overflow",
                    FrameType::Video,
                    0,
                    &[0x17, 0x01, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07],
                )
                .is_err(),
            "over-budget inject must be rejected"
        );
        assert!(
            server.external_route_ids.is_empty(),
            "pending-queue rejection must not leak external_route_ids"
        );
    }

    #[test]
    fn stale_reaper_skips_routes_with_pending_inject_frames() {
        let mut server = test_server();
        server
            .inject_relay_frame("live", "slow", FrameType::Video, 0, &[0x27, 0x01, 0x01])
            .unwrap();
        let key = ("live".to_string(), "slow".to_string());
        // Age last-seen past the stale threshold while the frame is still queued.
        server.inject_route_last_seen.insert(
            key.clone(),
            std::time::Instant::now()
                - std::time::Duration::from_secs(super::INJECT_ROUTE_STALE_SECS + 1),
        );
        server.reap_stale_inject_routes();
        assert_eq!(
            server.pending_injected_relay.len(),
            1,
            "reaper must not drop pending inject frames for a backlogged route"
        );
        assert!(
            server.external_route_ids.contains_key(&key),
            "route claim must survive while inject frames remain pending"
        );
    }

    #[test]
    fn cache_byte_precheck_counts_same_publisher_peers_as_freeable() {
        let mut server = test_server();
        let pub_id = 42u64;
        let old_key = ("live".to_string(), "old".to_string());
        let new_key = ("live".to_string(), "new".to_string());
        let mut old_cache = super::empty_stream_cache();
        old_cache.avc_header = Some(vec![0x17, 0x00, 0x01, 0x02]);
        let old_bytes = Server::stream_cache_entry_bytes(&old_key, &old_cache);
        server.stream_cache.insert(old_key.clone(), old_cache);
        server
            .publisher_cache_keys
            .insert(pub_id, vec![old_key.clone()]);
        // Cap just above the new reservation but below old+new, so fitting
        // requires same-publisher eviction that the precheck must count.
        server.resource_limits.max_stream_cache_bytes = old_bytes;
        let incoming = vec![0x17, 0x00, 0xAA];
        assert!(
            server.reserve_stream_cache_storage(&new_key, incoming.len(), 0, pub_id),
            "precheck must treat same-publisher peer cache as freeable"
        );
        assert!(
            !server.stream_cache.contains_key(&old_key),
            "same-publisher peer must be evicted to admit the new reservation"
        );
    }

    #[test]
    fn entry_cap_eviction_preserves_victims_when_bytes_cannot_fit() {
        let mut server = test_server();
        // Fill almost to the entry cap with socket-owned snapshots (not
        // freeable for a new external publisher), plus one tiny external
        // victim that entry-cap eviction would otherwise delete first.
        for i in 0..(super::MAX_STREAM_CACHE_ENTRIES - 1) {
            let name = format!("sock{i}");
            let key = ("live".to_string(), name);
            let mut cache = super::empty_stream_cache();
            cache.avc_header = Some(vec![0x17, 0x00, i as u8]);
            server.stream_cache.insert(key.clone(), cache);
            server.publisher_cache_keys.insert(i as u64 + 1, vec![key]);
        }
        let victim_key = ("live".to_string(), "ext0".to_string());
        let victim_pub = super::EXTERNAL_PUBLISHER_ID_BIT | 1;
        let mut victim = super::empty_stream_cache();
        victim.avc_header = Some(vec![0x17, 0x00, 0x01]);
        server
            .stream_cache
            .insert(victim_key.clone(), victim);
        server
            .publisher_cache_keys
            .insert(victim_pub, vec![victim_key.clone()]);
        assert_eq!(server.stream_cache.len(), super::MAX_STREAM_CACHE_ENTRIES);

        let total = server.stream_cache_bytes();
        // Cap equals current total: a modest new entry fits alone (irreducible)
        // but not alongside retained socket peers after freeing only the tiny
        // external victim.
        server.resource_limits.max_stream_cache_bytes = total;
        let new_key = ("live".to_string(), "new".to_string());
        let new_pub = super::EXTERNAL_PUBLISHER_ID_BIT | 2;
        let incoming_len = 32;
        assert!(
            !server.reserve_stream_cache_storage(&new_key, incoming_len, 0, new_pub),
            "reservation must fail when freeable bytes cannot make room"
        );
        assert!(
            server.stream_cache.contains_key(&victim_key),
            "entry-cap path must not delete the freeable victim when bytes still cannot fit"
        );
        assert_eq!(server.stream_cache.len(), super::MAX_STREAM_CACHE_ENTRIES);
    }

    #[test]
    fn inject_claims_route_blocking_socket_registry() {
        let mut server = test_server();
        server
            .inject_relay_frame("live", "stream", FrameType::Video, 0, &[0x17, 0x00, 0x01])
            .unwrap();
        let external_id = *server
            .external_route_ids
            .get(&("live".to_string(), "stream".to_string()))
            .unwrap();
        let owner = *server
            .active_publish_routes
            .lock()
            .unwrap()
            .get(&("live".to_string(), "stream".to_string()))
            .unwrap();
        assert_eq!(owner, external_id);
        let registry = PublishRouteRegistry::new(Arc::clone(&server.active_publish_routes));
        assert!(
            !registry.claim(42, "live", "stream"),
            "socket publisher must not claim an inject-owned route"
        );
    }

    #[test]
    fn stream_cache_evicts_stale_external_routes_under_entry_cap() {
        let mut server = test_server();
        // Manually fill to the entry cap with distinct external owners so
        // same-owner eviction cannot free space without the external fallback.
        for i in 0..super::MAX_STREAM_CACHE_ENTRIES {
            let name = format!("s{i}");
            let key = ("live".to_string(), name.clone());
            let pub_id = super::EXTERNAL_PUBLISHER_ID_BIT | (i as u64 + 1);
            server
                .stream_cache
                .insert(key.clone(), super::empty_stream_cache());
            server.publisher_cache_keys.insert(pub_id, vec![key]);
        }
        assert_eq!(server.stream_cache.len(), super::MAX_STREAM_CACHE_ENTRIES);
        server
            .inject_relay_frame("live", "overflow", FrameType::Video, 0, &[0x17, 0x00, 0x02])
            .unwrap();
        server.process_connections().unwrap();
        assert!(
            server
                .stream_cache
                .contains_key(&("live".to_string(), "overflow".to_string())),
            "new external route must evict an older external cache entry"
        );
        assert!(
            server.stream_cache.len() <= super::MAX_STREAM_CACHE_ENTRIES,
            "cache must stay within the entry cap"
        );
    }

    #[test]
    fn prune_empty_external_publisher_cache_keys_after_oversized_evict() {
        let mut server = test_server();
        server
            .inject_relay_frame("live", "cam", FrameType::Video, 0, &[0x17, 0x00, 0x01])
            .unwrap();
        server.process_connections().unwrap();
        let external_id = *server
            .external_route_ids
            .get(&("live".to_string(), "cam".to_string()))
            .unwrap();
        assert!(server.publisher_cache_keys.contains_key(&external_id));
        // Oversized AVC sequence header clears the cached field then drops the entry.
        let mut big = vec![0x17, 0x00];
        big.extend(std::iter::repeat(0xABu8).take(super::MAX_CACHED_INIT_FRAME_BYTES));
        server
            .inject_relay_frame("live", "cam", FrameType::Video, 1, &big)
            .unwrap();
        server.process_connections().unwrap();
        assert!(
            !server.publisher_cache_keys.contains_key(&external_id),
            "empty external owner row must be pruned"
        );
    }

    #[test]
    fn release_injected_route_frees_publish_claim() {
        let mut server = test_server();
        server
            .inject_relay_frame(
                "live",
                "ephemeral",
                FrameType::Video,
                0,
                &[0x27, 0x01, 0x01],
            )
            .unwrap();
        assert!(
            server
                .active_publish_routes
                .lock()
                .unwrap()
                .contains_key(&("live".to_string(), "ephemeral".to_string()))
        );
        // Explicit release while frames are still pending.
        server.release_injected_route("live", "ephemeral");
        assert!(
            !server
                .active_publish_routes
                .lock()
                .unwrap()
                .contains_key(&("live".to_string(), "ephemeral".to_string())),
            "release_injected_route must free the claim"
        );
        assert!(server.pending_injected_relay.is_empty());
        let registry = PublishRouteRegistry::new(Arc::clone(&server.active_publish_routes));
        assert!(registry.claim(99, "live", "ephemeral"));
    }

    #[test]
    fn external_inject_claim_persists_until_release() {
        let mut server = test_server();
        server
            .inject_relay_frame(
                "live",
                "ephemeral",
                FrameType::Video,
                0,
                &[0x27, 0x01, 0x01],
            )
            .unwrap();
        server.process_connections().unwrap();
        assert!(
            server
                .active_publish_routes
                .lock()
                .unwrap()
                .contains_key(&("live".to_string(), "ephemeral".to_string())),
            "non-cacheable inject must keep the route claim between polls"
        );
        let registry = PublishRouteRegistry::new(Arc::clone(&server.active_publish_routes));
        assert!(
            !registry.claim(99, "live", "ephemeral"),
            "socket publisher must not steal claim between inject frames"
        );
        server.release_injected_route("live", "ephemeral");
        assert!(
            !server
                .active_publish_routes
                .lock()
                .unwrap()
                .contains_key(&("live".to_string(), "ephemeral".to_string())),
            "only release_injected_route frees the claim"
        );
        assert!(registry.claim(99, "live", "ephemeral"));
    }

    #[test]
    fn inject_soft_caps_external_publish_routes() {
        let mut server = test_server();
        for i in 0..super::MAX_EXTERNAL_PUBLISH_ROUTES {
            let name = format!("cam{i}");
            server
                .inject_relay_frame("live", &name, FrameType::Video, 0, &[0x27, 0x01, 0x01])
                .unwrap();
            // Drain pending so the soft claim cap is what blocks, not the
            // pending-relay frame budget.
            server.process_connections().unwrap();
        }
        // Re-inject on an already-claimed route must still succeed at the cap.
        server
            .inject_relay_frame("live", "cam0", FrameType::Video, 1, &[0x27, 0x01, 0x02])
            .unwrap();
        assert!(
            server
                .inject_relay_frame("live", "overflow", FrameType::Video, 0, &[0x27, 0x01, 0x01])
                .is_err(),
            "new unique inject claim must fail at MAX_EXTERNAL_PUBLISH_ROUTES"
        );
        server.release_injected_route("live", "cam0");
        // Drop the successful re-inject pending frame so the freed claim slot
        // is the only gate for the overflow route.
        server.pending_injected_relay.clear();
        server
            .inject_relay_frame("live", "overflow", FrameType::Video, 0, &[0x27, 0x01, 0x01])
            .unwrap();
        server.release_all_injected_routes();
        assert_eq!(
            server
                .active_publish_routes
                .lock()
                .unwrap()
                .values()
                .filter(|id| super::is_external_publisher_id(**id))
                .count(),
            0,
            "release_all_injected_routes must clear every external claim"
        );
        assert!(server.pending_injected_relay.is_empty());
    }

    #[test]
    fn impossible_cache_reservation_does_not_evict_peers() {
        let mut server = test_server();
        server
            .inject_relay_frame("live", "peer", FrameType::Video, 0, &[0x17, 0x00, 0x01])
            .unwrap();
        server.process_connections().unwrap();
        assert!(
            server
                .stream_cache
                .contains_key(&("live".to_string(), "peer".to_string()))
        );

        // Cap below the irreducible size of the incoming field alone so peer
        // eviction cannot make this reservation fit.
        server.resource_limits.max_stream_cache_bytes = 4;
        let mut huge = vec![0x17, 0x00];
        huge.extend(std::iter::repeat(0xCDu8).take(64));
        server
            .inject_relay_frame("live", "too-big", FrameType::Video, 0, &huge)
            .unwrap();
        server.process_connections().unwrap();
        assert!(
            server
                .stream_cache
                .contains_key(&("live".to_string(), "peer".to_string())),
            "impossible reservation must not wipe peer cache routes"
        );
        assert!(
            !server
                .stream_cache
                .contains_key(&("live".to_string(), "too-big".to_string())),
            "impossible reservation must not create a cache entry"
        );
    }

    #[test]
    fn inject_is_not_echoed_through_relay_export() {
        let mut server = test_server();
        server.enable_relay_export(16, 1024);
        server
            .inject_relay_frame("live", "stream", FrameType::Video, 0, &[0x27, 0x01, 0x01])
            .unwrap();
        server.process_connections().unwrap();
        assert!(
            server.drain_exported_relay_frames().is_empty(),
            "injected frames must not be exported"
        );
    }
}
