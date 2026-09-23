//! Chunk stream state/registry
//!
//! Mirrors `src/chunk/chunk_state.h` and `src/chunk/chunk_state.c`.

use std::collections::HashMap;

use crate::buffer::Buffer;
use crate::message::control::MAX_INBOUND_CHUNK_SIZE;
use crate::types::ErrorCode;
use crate::types::Result;

/// Default chunk size per RTMP spec
pub const DEFAULT_CHUNK_SIZE: u32 = 128;
/// Upper bound on distinct chunk streams
pub const MAX_CHUNK_STREAMS: usize = 4096;
/// Max reassembly bytes per connection
pub const MAX_REASSEMBLY_BYTES_PER_CONN: usize = 32 * 1024 * 1024;
/// Default cap on a single RTMP message body (24-bit length field allows 16 MiB).
pub const DEFAULT_MAX_MSG_LENGTH: u32 = 4 * 1024 * 1024;

/// Hard ceiling of the RTMP chunk message-length field (`hton24` / `msg_length`).
pub const RTMP_WIRE_MAX_MSG_LENGTH: u32 = 0x00FF_FFFF;
/// Default cap on concurrently active chunk-stream ids per connection.
pub const DEFAULT_MAX_ACTIVE_CSIDS: usize = 256;

/// Per-chunk-stream state.
#[derive(Debug)]
pub struct ChunkStream {
    pub csid: u32,
    /// peer's chunk size (SetChunkSize)
    pub chunk_size: u32,
    /// running timestamp for this chunk stream
    pub type0_timestamp: u32,
    pub type0_msg_length: u32,
    pub type0_msg_type_id: u8,
    pub type0_msg_stream_id: u32,
    /// current message uses extended timestamps
    pub type0_ext_ts: bool,
    /// The most recent fmt=1/2 timestamp delta applied on this CSID. A new
    /// message that starts with fmt=3 (reusing the prior header entirely)
    /// implicitly repeats this same delta per RTMP spec 5.3.1.3.
    pub last_delta: u32,
    /// bytes read so far for current message
    pub reassembly_bytes_read: u32,
    /// buffer for reassembling partial messages
    pub reassembly_buf: Buffer,
    /// Scratch space reused for each continuation chunk read on this CSID.
    pub(crate) chunk_read_scratch: Vec<u8>,
    /// Last completed payload on this CSID. Copied before reassembly_buf is
    /// reset/shrunk so callers can read via the returned pointer safely.
    pub last_payload: Vec<u8>,
    pub in_use: bool,
}

impl Default for ChunkStream {
    fn default() -> Self {
        Self {
            csid: 0,
            chunk_size: DEFAULT_CHUNK_SIZE,
            type0_timestamp: 0,
            type0_msg_length: 0,
            type0_msg_type_id: 0,
            type0_msg_stream_id: 0,
            type0_ext_ts: false,
            last_delta: 0,
            reassembly_bytes_read: 0,
            reassembly_buf: Buffer::new(),
            chunk_read_scratch: Vec::new(),
            last_payload: Vec::new(),
            in_use: false,
        }
    }
}

impl ChunkStream {
    /// Reset stream state and release retained payload/reassembly allocations.
    pub fn reset(&mut self, default_chunk_size: u32) {
        self.type0_timestamp = 0;
        self.type0_msg_length = 0;
        self.type0_msg_type_id = 0;
        self.type0_msg_stream_id = 0;
        self.type0_ext_ts = false;
        self.last_delta = 0;
        self.reassembly_bytes_read = 0;
        self.reassembly_buf.reset();
        self.chunk_read_scratch.clear();
        self.chunk_read_scratch.shrink_to_fit();
        self.last_payload.clear();
        self.last_payload.shrink_to_fit();
        self.chunk_size = default_chunk_size;
    }
}

/// Per-connection chunk-stream registry.
#[derive(Debug)]
pub struct ChunkRegistry {
    pub streams: Vec<ChunkStream>,
    /// O(1) lookup from CSID to `streams` index.
    csid_index: HashMap<u32, usize>,
    /// Running total of `reassembly_buf` bytes across active streams.
    reassembly_bytes_in_use: usize,
    /// Chunk size applied to new streams
    pub default_chunk_size: u32,
    /// Reject message bodies larger than this (bytes).
    pub max_msg_length: u32,
    /// Reject opening more than this many CSIDs at once on one connection.
    pub max_active_csids: usize,
    /// Max reassembly bytes for this connection's chunk streams.
    pub max_reassembly_bytes: usize,
    pub initialized: bool,
}

impl Default for ChunkRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ChunkRegistry {
    /// Create a new chunk registry.
    pub fn new() -> Self {
        Self {
            streams: Vec::new(),
            csid_index: HashMap::new(),
            reassembly_bytes_in_use: 0,
            default_chunk_size: DEFAULT_CHUNK_SIZE,
            max_msg_length: DEFAULT_MAX_MSG_LENGTH,
            max_active_csids: DEFAULT_MAX_ACTIVE_CSIDS,
            max_reassembly_bytes: MAX_REASSEMBLY_BYTES_PER_CONN,
            initialized: true,
        }
    }

    /// Initialize the registry.
    pub fn init(&mut self) {
        self.initialized = true;
        self.default_chunk_size = DEFAULT_CHUNK_SIZE;
    }

    /// Get or create a chunk stream for the given csid.
    pub fn get_or_create(&mut self, csid: u32) -> Result<&mut ChunkStream> {
        let idx = self.get_or_create_index(csid)?;
        Ok(&mut self.streams[idx])
    }

    /// Like [`Self::get_or_create`], but returns the slot index to avoid
    /// overlapping borrows when updating registry-wide reassembly accounting.
    pub(crate) fn get_or_create_index(&mut self, csid: u32) -> Result<usize> {
        if let Some(&idx) = self.csid_index.get(&csid) {
            let stream = &self.streams[idx];
            if stream.in_use && stream.csid == csid {
                return Ok(idx);
            }
        }

        let active = self.streams.iter().filter(|s| s.in_use).count();
        if active >= self.max_active_csids {
            return Err(ErrorCode::Chunk);
        }

        // Reuse a free slot before growing the vec; this prevents the stream
        // count from monotonically climbing to MAX_CHUNK_STREAMS on connections
        // that open and close many streams across their lifetime.
        if let Some(i) = self.streams.iter().position(|s| !s.in_use) {
            if self.streams[i].in_use {
                self.release_stream_reassembly(i);
            }
            self.streams[i].csid = csid;
            self.streams[i].in_use = true;
            self.streams[i].reset(self.default_chunk_size);
            self.csid_index.insert(csid, i);
            return Ok(i);
        }

        if self.streams.len() >= MAX_CHUNK_STREAMS {
            return Err(ErrorCode::Chunk);
        }

        let mut stream = ChunkStream::default();
        stream.csid = csid;
        stream.in_use = true;
        stream.chunk_size = self.default_chunk_size;
        let idx = self.streams.len();
        self.streams.push(stream);
        self.csid_index.insert(csid, idx);
        Ok(idx)
    }

    /// Get a chunk stream by csid (returns None if not found).
    pub fn get(&self, csid: u32) -> Option<&ChunkStream> {
        let idx = *self.csid_index.get(&csid)?;
        let stream = &self.streams[idx];
        if stream.in_use && stream.csid == csid {
            Some(stream)
        } else {
            None
        }
    }

    /// Get a mutable chunk stream by csid.
    pub fn get_mut(&mut self, csid: u32) -> Option<&mut ChunkStream> {
        let idx = *self.csid_index.get(&csid)?;
        let stream = &self.streams[idx];
        if stream.in_use && stream.csid == csid {
            Some(&mut self.streams[idx])
        } else {
            None
        }
    }

    /// Total reassembly bytes currently buffered across all active CSIDs.
    pub(crate) fn reassembly_bytes_in_use(&self) -> usize {
        self.reassembly_bytes_in_use
    }

    /// Release a stream's reassembly buffer from the running total.
    pub(crate) fn release_stream_reassembly_by_csid(&mut self, csid: u32) -> usize {
        let Some(idx) = self.csid_index.get(&csid).copied() else {
            return 0;
        };
        self.release_stream_reassembly(idx)
    }

    pub(crate) fn release_stream_reassembly(&mut self, idx: usize) -> usize {
        let released = self.streams[idx].reassembly_buf.available();
        self.reassembly_bytes_in_use = self.reassembly_bytes_in_use.saturating_sub(released);
        released
    }

    /// Account for newly written reassembly bytes on a stream.
    pub(crate) fn note_reassembly_growth(&mut self, nbytes: usize) {
        self.reassembly_bytes_in_use = self.reassembly_bytes_in_use.saturating_add(nbytes);
    }

    /// Set chunk size for all streams. Values outside the RTMP-valid inbound
    /// range are ignored (peers must negotiate via `read_set_chunk_size`).
    pub fn set_all_chunk_size(&mut self, chunk_size: u32) {
        if chunk_size < DEFAULT_CHUNK_SIZE || chunk_size > MAX_INBOUND_CHUNK_SIZE {
            return;
        }
        self.default_chunk_size = chunk_size;
        for stream in &mut self.streams {
            if stream.in_use {
                stream.chunk_size = chunk_size;
            }
        }
    }

    /// Check if a stream can grow its reassembly buffer.
    pub fn can_grow_reassembly(&self, cs: &ChunkStream, additional: u32) -> Result<()> {
        if self.reassembly_bytes_in_use + additional as usize > self.max_reassembly_bytes {
            return Err(ErrorCode::Chunk);
        }
        let _ = cs;
        Ok(())
    }

    /// Reset a specific stream.
    pub fn reset_stream(&mut self, csid: u32) {
        if let Some(idx) = self.csid_index.get(&csid).copied() {
            if self.streams[idx].in_use && self.streams[idx].csid == csid {
                self.release_stream_reassembly(idx);
                self.streams[idx].reset(self.default_chunk_size);
            }
        }
    }

    /// Destroy the registry.
    pub fn destroy(&mut self) {
        self.streams.clear();
        self.csid_index.clear();
        self.reassembly_bytes_in_use = 0;
        self.initialized = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_all_chunk_size_ignores_out_of_range_values() {
        let mut reg = ChunkRegistry::new();
        reg.set_all_chunk_size(1);
        assert_eq!(reg.default_chunk_size, DEFAULT_CHUNK_SIZE);
        reg.set_all_chunk_size(MAX_INBOUND_CHUNK_SIZE + 1);
        assert_eq!(reg.default_chunk_size, DEFAULT_CHUNK_SIZE);
        reg.set_all_chunk_size(256);
        assert_eq!(reg.default_chunk_size, 256);
    }

    #[test]
    fn rejects_opening_too_many_active_csids() {
        let mut reg = ChunkRegistry::new();
        reg.max_active_csids = 2;

        assert!(reg.get_or_create(1).is_ok());
        assert!(reg.get_or_create(2).is_ok());
        assert!(matches!(reg.get_or_create(3), Err(ErrorCode::Chunk)));
    }

    #[test]
    fn completed_messages_do_not_pin_multi_megabyte_reassembly_capacity() {
        use crate::chunk::reader::{ChunkMessage, chunk_read};
        use crate::chunk::writer::chunk_write;

        let payload = vec![0xAB_u8; 256 * 1024];
        let mut reg = ChunkRegistry::new();
        reg.max_active_csids = 4;

        for csid in 3..7u32 {
            let msg = ChunkMessage {
                csid,
                fmt: 0,
                timestamp: 0,
                msg_length: payload.len() as u32,
                msg_type_id: 0x09,
                msg_stream_id: 1,
                is_complete: false,
            };
            let mut wire = Buffer::new();
            chunk_write(&mut wire, &msg, &payload, payload.len(), 128).unwrap();

            let mut out_msg = ChunkMessage::default();
            let mut ptr = std::ptr::null();
            let mut len = 0;
            loop {
                let rc = chunk_read(&mut wire, &mut reg, None, &mut out_msg, &mut ptr, &mut len)
                    .unwrap();
                if rc == 1 {
                    break;
                }
                assert_eq!(rc, 0);
            }
        }

        let retained: usize = reg
            .streams
            .iter()
            .filter(|s| s.in_use)
            .map(|s| s.reassembly_buf.capacity())
            .sum();
        assert!(
            retained <= 4 * crate::buffer::BUFFER_RESET_CAPACITY,
            "retained {retained} bytes of reassembly capacity"
        );
    }

    #[test]
    fn reset_releases_last_payload_capacity() {
        let mut stream = ChunkStream::default();
        stream.last_payload = vec![0u8; 256 * 1024];
        assert!(stream.last_payload.capacity() >= 256 * 1024);

        stream.reset(DEFAULT_CHUNK_SIZE);

        assert!(stream.last_payload.is_empty());
        assert_eq!(stream.last_payload.capacity(), 0);
    }
}
