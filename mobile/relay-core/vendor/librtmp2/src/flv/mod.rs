//! FLV audio/video/script tag parsing
//!
//! Mirrors `src/flv/` directory.
//!
//! These are standalone, spec-precise tag decoders exposed for embedders that
//! want structured FLV tag fields (codec, `AVCPacketType`, composition time,
//! `onMetaData` keys, ...) from raw payload bytes — e.g. a muxer/remuxer built
//! on top of this crate's Rust API. They are intentionally **not** used by
//! `session::conn::Conn`'s ingest/relay path: the relay only needs a coarse
//! codec fourcc for the `on_media_cb` authorization hook (see
//! `session::conn::detect_video_codec`/`detect_audio_codec`), and forwards
//! payload bytes to players unparsed. That lighter check intentionally covers
//! both legacy FLV codec nibbles and Enhanced RTMP (E-RTMP) headers in one
//! pass, which `video_tag::parse`/`audio_tag::parse` do not (they reject
//! E-RTMP's `IsExHeader` bit by design — see `ertmp::exvideo`/`exaudio` for
//! that path instead).

pub mod audio_tag;
pub mod script_tag;
pub mod video_tag;
