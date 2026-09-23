# librtmp2

A modern, open-source **Rust library** for Legacy RTMP and Enhanced RTMP v1/v2.  

[![crates.io](https://img.shields.io/crates/v/librtmp2.svg)](https://crates.io/crates/librtmp2)
![Crates.io Downloads (recent)](https://img.shields.io/crates/dr/librtmp2)
[![docs.rs](https://img.shields.io/docsrs/librtmp2/latest.svg)](https://docs.rs/librtmp2)
[![License](https://img.shields.io/github/license/OpenRTMP/librtmp2)](LICENSE)
![GitHub Release](https://img.shields.io/github/v/release/OpenRTMP/librtmp2)
![Language](https://img.shields.io/badge/language-Rust-orange)

---

## Overview

`librtmp2` is a Rust RTMP protocol library: handshake, chunking, AMF commands, live publish/play relay, and an `extern "C"` FFI layer. It is meant to be embedded in custom servers, clients, and relay tools.

**What it is good at today:**
- Live ingest and relay for OBS/FFmpeg-style workflows (H.264/AAC and Enhanced-RTMP passthrough for HEVC/AV1)
- Minimal RTMP server and client (`connect` → `createStream` → `publish` / `play`)
- RTMPS (RTMP over TLS) via the optional `tls` Cargo feature (OpenSSL), enabled by default
- Parser/serializer modules for E-RTMP v1/v2 structures (usable from embedders; not all are wired into the session layer yet)

**What it is not:**
- Not a complete Adobe RTMP 1.0 implementation (no VOD commands, encrypted handshake, etc.; AMF3 shared object *envelopes* are parsed/delivered, but multi-client attribute sync/persistence is left to the host)
- Not a full E-RTMP v2 session stack (`capsEx` negotiation, multitrack, ModEx, and the reconnect-request handshake are wired into the session; the actual reconnect-redirect *transport* logic is left to the host application)
- Not an HTTP server, media policy layer, or FFmpeg wrapper

See [Implementation status](#implementation-status) for the code-accurate breakdown.

---

## Architecture

```text
OBS / FFmpeg / App
        │
        ▼
      librtmp2          ← this library
      ├── Handshake / Chunking / Control
      ├── AMF0 commands (connect, publish, play, …)
      ├── Session relay (publisher → players)
      ├── ertmp/ + flv/ parsers (library modules)
      └── RTMPS transport (optional)
```

The live path is implemented in `session/conn.rs` and `server/mod.rs`. Parser modules under `ertmp/` and `flv/` can be used directly by embedders even when they are not yet called from the default session flow.

---

## State Machine

```text
TCP_ACCEPTED
  → HANDSHAKE
  → CONNECTED
  → [CAPS_NEGOTIATED]      ← entered when connect advertises E-RTMP v2 caps
  → APP_CONNECTED
  → STREAM_CREATED
  → PUBLISHING | PLAYING
  → CLOSING
  → CLOSED
```

`CAPS_NEGOTIATED` is entered when the client's `connect` command advertises
`fourCcList` / `capsEx` / `videoFourCcInfoMap` / `reconnect`; `negotiate_caps()`
computes the response and the negotiated caps are echoed back in the `connect`
`_result` (`session/conn.rs`). Clients that don't advertise v2 caps skip
straight to `APP_CONNECTED` as before.

---

## Build

```bash
# Debug build
cargo build

# Release build
cargo build --release

# Run tests
cargo test

# Build without TLS (no OpenSSL dependency)
cargo build --no-default-features
```

The crate uses `crate-type = ["cdylib", "staticlib", "lib"]` and produces:
- `librtmp2.so` / `librtmp2.dll` — cdylib for FFI consumers
- `librtmp2.a` / `librtmp2.lib` — staticlib for FFI consumers
- Rust `lib` — for direct Cargo dependency

### TLS / RTMPS

RTMPS (RTMP over TLS) is supported via OpenSSL and is **enabled by default** via the `tls` Cargo feature. To produce a plaintext-only build without the optional TLS/OpenSSL dependency:

```bash
cargo build --no-default-features
```

Call `lrtmp2_tls_supported()` at runtime to check whether the library was built with TLS.

---

## Using as a Rust Crate

Add to your `Cargo.toml`:

```toml
[dependencies]
librtmp2 = { path = "../librtmp2" }
```

Without TLS:

```toml
[dependencies]
librtmp2 = { path = "../librtmp2", default-features = false }
```

### External media export / inject (Rust)

Integrators can observe or feed the same local relay path used by publishers
and players — without sockets or cluster concepts:

```rust
use librtmp2::server::{
    is_external_publisher_id, Server, StreamInitSnapshot, EXTERNAL_RELAY_PUBLISHER_ID,
};
use librtmp2::{FrameType, RelayFrame, ServerConfig};

// Bounded export of publisher frames (disabled by default = no clones).
server.enable_relay_export(/*max_frames*/ 256, /*max_bytes*/ 4 * 1024 * 1024);
let frames: Vec<RelayFrame> = server.drain_exported_relay_frames();

// Socket-less inject into cache + player fan-out for (app, stream).
// Claims the route until `release_injected_route` (call when the feed ends).
// Required for continually changing routes — otherwise new claims stop at
// `MAX_EXTERNAL_PUBLISH_ROUTES` (soft cap; no mid-feed eviction).
server.inject_relay_frame("live", "cam1", FrameType::Video, ts, &payload)?;
server.release_injected_route("live", "cam1");
// Or on shutdown / remesh: server.release_all_injected_routes();

// Late-joiner / remote-subscriber init snapshot from StreamCache.
if let Some(snap) = server.stream_init_snapshot("live", "cam1") {
    let _: &StreamInitSnapshot = &snap;
    let _ = snap.avc_header;
    let _ = EXTERNAL_RELAY_PUBLISHER_ID;
    let _ = is_external_publisher_id(EXTERNAL_RELAY_PUBLISHER_ID);
}
```

On an existing local publisher `Conn`, `Conn::inject_relay_frame` queues into
`pending_relay` the same way as socket-received media.

### HA / cluster mirroring notes

- `enable_relay_export` + `drain_exported_relay_frames` mirror **live** relay
  frames only. Init state (AVC/AAC sequence headers, metadata, last keyframe) is
  **not** included in the export buffer.
- Integrators mirroring to remote nodes must also call `stream_init_snapshot`
  (or equivalent init-cache handoff) so late joiners can decode the stream.
- External inject routes (`Server::inject_relay_frame`) must call
  `release_injected_route` when a feed ends. Stale claims are reaped automatically
  after ~120s without frames, but explicit release is still recommended.

`Conn::inject_relay_frame` skips media auth callbacks.

---

## Using via the `extern "C"` FFI

The crate also builds as a `cdylib`/`staticlib` and exposes a stable `extern "C"` API for use from C, Go, Python, PHP, and others. See `src/lib.rs` for the full FFI surface.

### Server

```c
lrtmp2_server_t *lrtmp2_server_create(const lrtmp2_server_config_t *config);
void             lrtmp2_server_destroy(lrtmp2_server_t *server);
int              lrtmp2_server_listen(lrtmp2_server_t *server, const char *bind_addr);
int              lrtmp2_server_poll(lrtmp2_server_t *server, int timeout_ms);
void             lrtmp2_server_stop(lrtmp2_server_t *server);
```

### Client

```c
lrtmp2_client_t *lrtmp2_client_create(const lrtmp2_server_config_t *config);
void             lrtmp2_client_destroy(lrtmp2_client_t *client);
int              lrtmp2_client_connect(lrtmp2_client_t *client, const char *url);
int              lrtmp2_client_publish(lrtmp2_client_t *client);
int              lrtmp2_client_play(lrtmp2_client_t *client);
int              lrtmp2_client_send_frame(lrtmp2_client_t *client, const lrtmp2_frame_t *frame);
int              lrtmp2_client_poll(lrtmp2_client_t *client, int timeout_ms);
```

### Utilities

```c
int          lrtmp2_tls_supported(void);
const char  *lrtmp2_version_string(void);
int          lrtmp2_version_major(void);
int          lrtmp2_version_minor(void);
int          lrtmp2_version_patch(void);
const char  *lrtmp2_error_string(int code);
```

---

## Repository Structure

```text
librtmp2/
├── src/
│   ├── lib.rs              Rust API + extern "C" FFI layer
│   ├── alloc.rs            Custom allocator hook
│   ├── amf/                AMF0 + AMF3 encoding/decoding
│   ├── buffer.rs           Growable byte buffers
│   ├── bytes.rs            Big-endian byte helpers
│   ├── chunk/              Chunk reader/writer/state (per-csid)
│   ├── client/             Outbound client: connect → publish/play
│   ├── ertmp/              E-RTMP v1/v2 parsers (see Implementation status)
│   ├── flv/                FLV tag parsers (library; not used in live relay path)
│   ├── handshake.rs        C0/C1/C2 ↔ S0/S1/S2
│   ├── media/              Init-frame cache classification + ModEx helpers shared by session/server
│   ├── message/            Message reassembly, control, commands
│   ├── net.rs              Host:port parsing shared by server bind / client connect
│   ├── server/             Listening socket, accept loop, relay
│   ├── session/            Connection state, publish/play handling
│   ├── transport.rs        TLS/plaintext transport
│   └── types.rs            Shared types
├── tests/
│   ├── interop/
│   └── server_client_loopback.rs
├── build.rs
├── Cargo.toml
└── docs/
```

---

## Implementation status

Status reflects what is **wired into the live session path** (`conn.rs`, `server/mod.rs`, `client/mod.rs`), not merely what exists as parser code elsewhere in the repo.

### Legacy RTMP — live path

| Area | Status |
|------|--------|
| Standard handshake (C0–C2) | Done |
| Encrypted / Adobe-digest handshake | Not implemented |
| Chunking, control messages, ping | Done |
| Commands `connect`, `createStream`, `publish`, `play` | Done |
| Commands `pause`, `seek`, `receiveAudio`, `receiveVideo`, `closeStream` | Done — `pause`/`receiveAudio`/`receiveVideo` gate per-frame relay; `closeStream` mirrors `deleteStream` cleanup |
| `FCPublish` / `releaseStream` | `FCPublish` ignored (no-op); `releaseStream` force-releases a stale publish-route claim so a reconnecting encoder can immediately republish |
| `FCUnpublish` / `deleteStream` | Done — clears publish route, play state (`is_playing`/`paused`), and sends `StreamEOF` |
| Audio / video ingest and relay | Done |
| Aggregate messages | Done (unpack → relay) |
| Publisher `onMetaData` parsing (stats) | Done — relayed live to players and cached for late joiners |
| AMF3 shared objects | Done — envelope (name/version/flags + event list) parsed and delivered via `on_shared_object_cb`; `Conn::send_shared_object` writes them. Event payloads are opaque bytes — interpreting `Change`/`SendMessage` contents as AMF is left to the host, per this crate's "deliver the event, not the policy" design |
| User Control `StreamBegin` / `StreamEOF` / `SetBufferLength` | Done — `StreamBegin`/`SetBufferLength` sent on `play`, `StreamEOF` sent on `deleteStream`/`closeStream`, inbound `SetBufferLength` read and stored |
| One stream per connection (`current_stream`) | By design today |
| Init-frame cache for late joiners | Legacy H.264 (`0x17`) + AAC, plus enhanced (ex-header) and multitrack sequence starts per track |

### E-RTMP v1

| Area | Status |
|------|--------|
| Enhanced A/V passthrough (HEVC/AV1/Opus from FFmpeg/OBS) | Done (opaque byte relay) |
| `exvideo_parse` / `exaudio_parse` in session hot path | Done — called for codec detection and init-cache classification |
| `fourCcList` in `connect` | Done — parsed on read; echoed back in the `_result` when v2 caps negotiation triggers |
| HDR / `colorInfo` (`metadata.rs`) | Done — an enhanced video Metadata packet (`ERTMP_PACKET_TYPE_METADATA`) is parsed and exposed via `Conn::detected_hdr_info` (Rust-only; kept off `Frame` to preserve its ABI-stable `#[repr(C)]` layout, see `docs/abi-policy.md`) |
| Enhanced sequence-start cache for players | Done (see init-frame cache above) |
| `exvideo_write` / `exaudio_write` helpers | Done — mirror `exvideo_parse` / `exaudio_parse` in reverse (`src/ertmp/exvideo.rs`, `src/ertmp/exaudio.rs`); `exvideo_write` covers enhanced (ex-header) headers only — `VideoHeader` carries no legacy codec ID, so legacy output is rejected (returns 0) rather than emitting an invalid tag byte |

### E-RTMP v2

| Area | Status |
|------|--------|
| `capsEx`, `videoFourCcInfoMap`, `reconnect`, `multitrack`, `modex` parse/write | Library code + unit tests, also wired into the session (see below) |
| v2 capability negotiation in session | Done — `negotiate_caps()` runs on `connect` when the client advertises v2 caps; state transitions to `CAPS_NEGOTIATED` and the response echoes negotiated caps |
| Multitrack / ModEx in session | Done — ModEx normalized on every ingested frame; multitrack containers demuxed per track for codec authorization and per-track init-cache headers |
| Reconnect in session | Done — `Conn::send_reconnect_request` / `Server::request_reconnect` send `NetConnection.Connect.ReconnectRequest`; the client detects it mid-session and fires `on_reconnect_request_cb` with the (optional) `tcUrl`. The client also advertises the `capsEx` reconnect bit + a `reconnect` value on `connect` whenever `on_reconnect_request_cb` is set, so spec-compliant servers know to expect it. The library only delivers the event — establishing the new connection is left to the host application |

### Client, TLS, tests

| Area | Status |
|------|--------|
| Minimal publish client | Done |
| Minimal play client (A/V receive callback) | Done — also delivers metadata (AMF0/AMF3 data) and handles Aggregate messages |
| RTMPS | Done |
| Loopback + FFmpeg interop tests | Present (`tests/`, `tests/interop/`) |

Parser-level details and spec mappings: [`docs/protocol-mapping-legacy.md`](docs/protocol-mapping-legacy.md), [`docs/protocol-mapping-ertmp-v1.md`](docs/protocol-mapping-ertmp-v1.md), [`docs/protocol-mapping-ertmp-v2.md`](docs/protocol-mapping-ertmp-v2.md).

---

## License

MIT — see [LICENSE](LICENSE)
