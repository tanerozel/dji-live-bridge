//! Stream management within a connection
//!
//! Mirrors `src/session/stream.h` and `src/session/stream.c`.

use std::sync::atomic::{AtomicU32, Ordering};

/// Per-stream state.
#[derive(Debug)]
pub struct Stream {
    pub stream_id: u32,
    pub is_publishing: bool,
    pub is_playing: bool,
    /// Stream name from the `publish` or `play` command.
    pub name: String,
    /// Set by the RTMP `pause` command.
    pub paused: bool,
    /// Honored by server relay when forwarding to this player.
    pub receive_audio: bool,
    pub receive_video: bool,
}

impl Stream {
    /// Create a new stream.
    pub fn new(stream_id: u32) -> Self {
        Self {
            stream_id,
            is_publishing: false,
            is_playing: false,
            name: String::new(),
            paused: false,
            receive_audio: true,
            receive_video: true,
        }
    }
}

/// Begin publishing on this stream.
pub fn publish_begin(stream: &mut Stream, _stream_key: &str) {
    stream.is_publishing = true;
}

/// Begin playing on this stream.
pub fn play_begin(stream: &mut Stream) {
    stream.is_playing = true;
}
