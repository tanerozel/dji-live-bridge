//! Chunk writer
//!
//! Mirrors `src/chunk/chunk_writer.h`, `src/chunk/chunk_write.h`, and `src/chunk/chunk_writer.c`.

use crate::buffer::Buffer;
use crate::chunk::reader::ChunkMessage;
use crate::types::ErrorCode;
use crate::types::Result;

/// Write a full message to `out`, fragmenting the payload into chunks of at most `chunk_size` bytes.
pub fn chunk_write(
    out: &mut Buffer,
    msg: &ChunkMessage,
    payload: &[u8],
    payload_len: usize,
    chunk_size: usize,
) -> Result<()> {
    if chunk_size == 0 {
        return chunk_write(out, msg, payload, payload_len, 128);
    }

    if msg.fmt <= 1 && payload_len != msg.msg_length as usize {
        return Err(ErrorCode::Internal);
    }
    if payload_len > payload.len() {
        return Err(ErrorCode::Internal);
    }

    let csid = msg.csid;
    let fmt = msg.fmt;
    let ts = msg.timestamp;
    let ext_ts = ts >= 0xFFFFFF;

    // --- First chunk: basic header + conditional message header ---
    let hdr = basic_header(csid, fmt);
    out.write(&hdr).map_err(|_| ErrorCode::Internal)?;

    // fmt=0: timestamp+length+type+streamid (11 bytes)
    // fmt=1: timestamp+length+type         ( 7 bytes, no stream id)
    // fmt=2: timestamp only                ( 3 bytes)
    // fmt=3: no message header at all

    if fmt <= 2 {
        // timestamp (3 bytes)
        let mut ts_buf = [0u8; 3];
        hton24(&mut ts_buf, if ext_ts { 0xFFFFFF } else { ts });
        out.write(&ts_buf).map_err(|_| ErrorCode::Internal)?;
    }

    if fmt <= 1 {
        // message length (3 bytes)
        let mut len_buf = [0u8; 3];
        hton24(&mut len_buf, msg.msg_length);
        out.write(&len_buf).map_err(|_| ErrorCode::Internal)?;

        // message type id (1 byte)
        out.write(&[msg.msg_type_id])
            .map_err(|_| ErrorCode::Internal)?;
    }

    if fmt == 0 {
        // stream id (4 bytes, little-endian)
        let sid = msg.msg_stream_id;
        out.write(&[
            (sid & 0xFF) as u8,
            ((sid >> 8) & 0xFF) as u8,
            ((sid >> 16) & 0xFF) as u8,
            ((sid >> 24) & 0xFF) as u8,
        ])
        .map_err(|_| ErrorCode::Internal)?;
    }

    if ext_ts && fmt <= 2 {
        out.write(&ts.to_be_bytes())
            .map_err(|_| ErrorCode::Internal)?;
    }

    // --- Payload: fragment across multiple chunks ---
    let mut offset = 0;
    while offset < payload_len {
        let to_write = (payload_len - offset).min(chunk_size);
        out.write(&payload[offset..offset + to_write])
            .map_err(|_| ErrorCode::Internal)?;
        offset += to_write;

        if offset < payload_len {
            // Continuation chunk header (fmt=3, no message header)
            let chdr = basic_header(csid, 3);
            out.write(&chdr).map_err(|_| ErrorCode::Internal)?;
            if ext_ts {
                out.write(&ts.to_be_bytes())
                    .map_err(|_| ErrorCode::Internal)?;
            }
        }
    }

    Ok(())
}

/// Write an extended timestamp chunk (for protocol control messages).
pub fn chunk_write_extended_timestamp(out: &mut Buffer, timestamp: u32) -> Result<()> {
    // Basic header: fmt=3, csid=2 (protocol control)
    let hdr = (3u8 << 6) | 2;
    out.write(&[hdr]).map_err(|_| ErrorCode::Internal)?;

    // 4 bytes extended timestamp
    out.write(&timestamp.to_be_bytes())
        .map_err(|_| ErrorCode::Internal)?;

    Ok(())
}

/// Build a basic header for the given csid and fmt.
fn basic_header(csid: u32, fmt: u8) -> Vec<u8> {
    if csid < 64 {
        vec![(fmt << 6) | (csid as u8)]
    } else if csid < 320 {
        vec![fmt << 6, (csid - 64) as u8]
    } else {
        vec![
            (fmt << 6) | 1,
            ((csid - 64) & 0xFF) as u8,
            (((csid - 64) >> 8) & 0xFF) as u8,
        ]
    }
}

/// Write a 24-bit big-endian value.
fn hton24(buf: &mut [u8; 3], val: u32) {
    buf[0] = ((val >> 16) & 0xFF) as u8;
    buf[1] = ((val >> 8) & 0xFF) as u8;
    buf[2] = (val & 0xFF) as u8;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunk::reader::chunk_read;
    use crate::chunk::state::ChunkRegistry;

    #[test]
    fn single_chunk_round_trips_through_reader() {
        let payload = b"hello rtmp";
        let msg = ChunkMessage {
            csid: 3,
            fmt: 0,
            timestamp: 1234,
            msg_length: payload.len() as u32,
            msg_type_id: 0x14,
            msg_stream_id: 1,
            is_complete: false,
        };

        let mut wire = Buffer::new();
        chunk_write(&mut wire, &msg, payload, payload.len(), 128).unwrap();

        let mut reg = ChunkRegistry::new();
        let mut out_msg = ChunkMessage::default();
        let mut ptr = std::ptr::null();
        let mut len = 0usize;
        let rc = chunk_read(&mut wire, &mut reg, None, &mut out_msg, &mut ptr, &mut len).unwrap();

        assert_eq!(rc, 1);
        assert!(out_msg.is_complete);
        assert_eq!(out_msg.csid, 3);
        assert_eq!(out_msg.timestamp, 1234);
        assert_eq!(out_msg.msg_type_id, 0x14);
        assert_eq!(out_msg.msg_stream_id, 1);
        let received = unsafe { std::slice::from_raw_parts(ptr, len) };
        assert_eq!(received, payload);
    }

    #[test]
    fn fragmented_chunks_round_trip_through_reader() {
        let payload = vec![0xAB_u8; 300];
        let msg = ChunkMessage {
            csid: 4,
            fmt: 0,
            timestamp: 0,
            msg_length: payload.len() as u32,
            msg_type_id: 0x09,
            msg_stream_id: 1,
            is_complete: false,
        };

        let mut wire = Buffer::new();
        chunk_write(&mut wire, &msg, &payload, payload.len(), 128).unwrap();

        let mut reg = ChunkRegistry::new();
        let mut out_msg = ChunkMessage::default();
        let mut ptr = std::ptr::null();
        let mut len = 0usize;
        // chunk_write fragments the payload across multiple 128-byte
        // chunks; chunk_read consumes one chunk per call, so drive it
        // until the reassembled message is complete.
        let mut rc;
        loop {
            rc = chunk_read(&mut wire, &mut reg, None, &mut out_msg, &mut ptr, &mut len).unwrap();
            if rc == 1 || (rc == 0 && wire.available() == 0) {
                break;
            }
        }

        assert_eq!(rc, 1);
        let received = unsafe { std::slice::from_raw_parts(ptr, len) };
        assert_eq!(received, payload.as_slice());
    }

    #[test]
    fn extended_timestamp_round_trips_big_endian() {
        let payload = b"x";
        let msg = ChunkMessage {
            csid: 5,
            fmt: 0,
            timestamp: 0x0100_0000,
            msg_length: payload.len() as u32,
            msg_type_id: 0x09,
            msg_stream_id: 1,
            is_complete: false,
        };

        let mut wire = Buffer::new();
        chunk_write(&mut wire, &msg, payload, payload.len(), 128).unwrap();

        let mut reg = ChunkRegistry::new();
        let mut out_msg = ChunkMessage::default();
        let mut ptr = std::ptr::null();
        let mut len = 0usize;
        chunk_read(&mut wire, &mut reg, None, &mut out_msg, &mut ptr, &mut len).unwrap();

        assert_eq!(out_msg.timestamp, 0x0100_0000);
    }

    #[test]
    fn rejects_payload_length_mismatch_for_fmt0() {
        let payload = b"hello";
        let msg = ChunkMessage {
            csid: 3,
            fmt: 0,
            timestamp: 0,
            msg_length: 99,
            msg_type_id: 0x14,
            msg_stream_id: 1,
            is_complete: false,
        };
        let mut wire = Buffer::new();
        assert_eq!(
            chunk_write(&mut wire, &msg, payload, payload.len(), 128),
            Err(ErrorCode::Internal)
        );
    }

    #[test]
    fn fmt3_ignores_stale_msg_length_field() {
        let payload = b"continuation";
        let msg = ChunkMessage {
            csid: 3,
            fmt: 3,
            timestamp: 0,
            msg_length: 0,
            msg_type_id: 0x14,
            msg_stream_id: 1,
            is_complete: false,
        };
        let mut wire = Buffer::new();
        chunk_write(&mut wire, &msg, payload, payload.len(), 128).unwrap();
    }
}
