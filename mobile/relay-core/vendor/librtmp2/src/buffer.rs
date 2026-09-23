//! Growable byte buffer
//!
//! Mirrors `src/core/buffer.h` and `src/core/buffer.c`.

use crate::types::ErrorCode;
use crate::types::Result;

/// Maximum buffer size: 64 MB
pub const BUFFER_MAX_SIZE: usize = 64 * 1024 * 1024;
/// Initial buffer capacity
pub const BUFFER_INITIAL_SIZE: usize = 4096;
/// Capacity retained after [`Buffer::reset`] so short-lived large messages
/// cannot leave multi-megabyte `Vec` allocations pinned for the connection.
pub const BUFFER_RESET_CAPACITY: usize = BUFFER_INITIAL_SIZE;
/// Growth factor
const BUFFER_GROW_FACTOR: usize = 2;

/// A growable byte buffer with read/write cursors.
#[derive(Debug)]
pub struct Buffer {
    data: Vec<u8>,
    /// bytes written
    size: usize,
    /// read cursor
    read_pos: usize,
    /// whether data is owned (heap-allocated)
    owned: bool,
}

impl Default for Buffer {
    fn default() -> Self {
        Self::new()
    }
}

impl Buffer {
    /// Create a new buffer with default initial capacity.
    pub fn new() -> Self {
        Self {
            data: vec![0u8; BUFFER_INITIAL_SIZE],
            size: 0,
            read_pos: 0,
            owned: true,
        }
    }

    /// Create a new buffer with a specific initial capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            data: vec![0u8; capacity],
            size: 0,
            read_pos: 0,
            owned: true,
        }
    }

    /// Create an "unowned" buffer wrapping external data.
    pub fn from_slice(data: &[u8]) -> Self {
        Self {
            data: data.to_vec(),
            size: data.len(),
            read_pos: 0,
            owned: true,
        }
    }

    /// Create an unowned view over existing data (for stack/static buffers).
    pub fn from_static(data: &mut [u8]) -> Buffer {
        let len = data.len();
        Buffer {
            data: data.to_vec(),
            size: len,
            read_pos: 0,
            owned: false,
        }
    }

    /// Number of bytes available to read.
    pub fn available(&self) -> usize {
        self.size.saturating_sub(self.read_pos)
    }

    /// Remaining write space.
    pub fn space(&self) -> usize {
        self.data.len().saturating_sub(self.size)
    }

    /// Write data to the buffer.
    pub fn write(&mut self, data: &[u8]) -> Result<usize> {
        if data.is_empty() {
            return Ok(0);
        }

        self.compact();

        if data.len() > BUFFER_MAX_SIZE || self.size > BUFFER_MAX_SIZE - data.len() {
            return Err(ErrorCode::Internal);
        }

        let needed = self.size + data.len();
        self.ensure_capacity(needed)?;

        self.data[self.size..self.size + data.len()].copy_from_slice(data);
        self.size += data.len();
        Ok(data.len())
    }

    /// Read data from the buffer.
    pub fn read(&mut self, out: &mut [u8]) -> Result<usize> {
        if self.read_pos + out.len() > self.size {
            return Err(ErrorCode::Io);
        }
        out.copy_from_slice(&self.data[self.read_pos..self.read_pos + out.len()]);
        self.read_pos += out.len();
        Ok(out.len())
    }

    /// Peek at available data without consuming it.
    pub fn peek(&self) -> &[u8] {
        &self.data[self.read_pos..self.size]
    }

    /// Total allocated capacity (logical size plus spare write space).
    pub fn capacity(&self) -> usize {
        self.data.len()
    }

    /// Reset the buffer to empty.
    pub fn reset(&mut self) {
        self.size = 0;
        self.read_pos = 0;
        if self.owned && self.data.len() > BUFFER_RESET_CAPACITY {
            self.data.truncate(BUFFER_RESET_CAPACITY);
            self.data.shrink_to_fit();
        }
    }

    /// Drain (skip) `len` bytes from the read position.
    pub fn drain(&mut self, len: usize) {
        let available = self.available();
        self.read_pos += len.min(available);
    }

    /// Get the raw data slice.
    pub fn as_slice(&self) -> &[u8] {
        &self.data[..self.size]
    }

    /// Get the raw data slice mutably.
    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        &mut self.data[..self.size]
    }

    /// Get the write position for direct writing.
    pub fn write_pos(&self) -> usize {
        self.size
    }

    /// Set the size after direct writing.
    pub fn set_size(&mut self, size: usize) {
        self.size = size.min(self.data.len());
    }

    /// Get the current read position.
    pub fn read_pos(&self) -> usize {
        self.read_pos
    }

    /// Set the read position.
    pub fn set_read_pos(&mut self, pos: usize) {
        self.read_pos = pos.min(self.size);
    }

    /// Compact: move unread data to the front.
    fn compact(&mut self) {
        if self.read_pos == 0 {
            return;
        }
        if self.read_pos >= self.size {
            self.size = 0;
            self.read_pos = 0;
            return;
        }
        let avail = self.size - self.read_pos;
        self.data.copy_within(self.read_pos..self.size, 0);
        self.size = avail;
        self.read_pos = 0;
    }

    /// Ensure the buffer has at least `needed` capacity.
    fn ensure_capacity(&mut self, needed: usize) -> Result<()> {
        if needed <= self.data.len() {
            return Ok(());
        }
        if !self.owned {
            return Err(ErrorCode::Internal);
        }
        if needed > BUFFER_MAX_SIZE {
            return Err(ErrorCode::Internal);
        }
        let mut new_cap = self.data.len().max(BUFFER_INITIAL_SIZE);
        while new_cap < needed {
            new_cap = new_cap.saturating_mul(BUFFER_GROW_FACTOR);
            if new_cap > BUFFER_MAX_SIZE {
                new_cap = BUFFER_MAX_SIZE;
                break;
            }
        }
        self.data.resize(new_cap, 0);
        Ok(())
    }
}

impl Clone for Buffer {
    fn clone(&self) -> Self {
        Self {
            data: self.data.clone(),
            size: self.size,
            read_pos: self.read_pos,
            owned: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_then_read_round_trips() {
        let mut buf = Buffer::new();
        buf.write(b"hello world").unwrap();
        assert_eq!(buf.available(), 11);
        let mut out = [0u8; 5];
        buf.read(&mut out).unwrap();
        assert_eq!(&out, b"hello");
        assert_eq!(buf.available(), 6);
        assert_eq!(buf.peek(), b" world");
    }

    #[test]
    fn read_past_available_errors() {
        let mut buf = Buffer::new();
        buf.write(b"ab").unwrap();
        let mut out = [0u8; 3];
        assert!(buf.read(&mut out).is_err());
    }

    #[test]
    fn drain_skips_unread_bytes() {
        let mut buf = Buffer::new();
        buf.write(b"abcdef").unwrap();
        buf.drain(3);
        assert_eq!(buf.peek(), b"def");
    }

    #[test]
    fn grows_past_initial_capacity() {
        let mut buf = Buffer::new();
        let chunk = vec![0xAB_u8; BUFFER_INITIAL_SIZE * 3];
        buf.write(&chunk).unwrap();
        assert_eq!(buf.available(), chunk.len());
        assert_eq!(buf.peek(), chunk.as_slice());
    }

    #[test]
    fn reset_clears_buffer() {
        let mut buf = Buffer::new();
        buf.write(b"data").unwrap();
        buf.reset();
        assert_eq!(buf.available(), 0);
        assert_eq!(buf.peek(), b"");
    }

    #[test]
    fn reset_shrinks_oversized_capacity() {
        let mut buf = Buffer::new();
        buf.write(&vec![0u8; 1024 * 1024]).unwrap();
        assert!(buf.capacity() >= 1024 * 1024);
        buf.reset();
        assert_eq!(buf.available(), 0);
        assert!(buf.capacity() <= BUFFER_RESET_CAPACITY);
    }

    #[test]
    fn write_over_max_size_errors() {
        let mut buf = Buffer::new();
        assert!(buf.write(&vec![0u8; BUFFER_MAX_SIZE + 1]).is_err());
    }
}
