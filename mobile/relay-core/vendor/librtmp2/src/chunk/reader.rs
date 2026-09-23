//! Chunk reader
//!
//! Mirrors `src/chunk/chunk_reader.h` and `src/chunk/chunk_reader.c`.

use crate::buffer::Buffer;
use crate::bytes::{hton24, ntoh32};
use crate::chunk::state::{ChunkRegistry, ChunkStream, DEFAULT_CHUNK_SIZE};
use crate::types::ErrorCode;
use crate::types::Result;

/// A chunk message (resembled from one or more chunk reads).
#[derive(Debug, Clone, Default)]
pub struct ChunkMessage {
    pub csid: u32,
    pub fmt: u8,
    pub timestamp: u32,
    pub msg_length: u32,
    pub msg_type_id: u8,
    pub msg_stream_id: u32,
    pub is_complete: bool,
}

/// Read a single chunk from the buffer, reassembling into complete messages.
///
/// Returns:
/// - Ok(1) with is_complete=true when a full message is ready
/// - Ok(0) when more data is needed
/// - Err on protocol errors
///
/// Design: the function uses a "peek-first" strategy.  All availability checks
/// happen BEFORE any bytes are consumed from `buf`.  This guarantees that every
/// `Ok(0)` return leaves the cursor exactly where it was on entry, so the next
/// call can retry without corruption.
pub fn chunk_read(
    buf: &mut Buffer,
    reg: &mut ChunkRegistry,
    _unused: Option<&()>,
    msg: &mut ChunkMessage,
    payload: &mut *const u8,
    payload_len: &mut usize,
) -> Result<i32> {
    let available = buf.available();
    if available < 1 {
        return Ok(0);
    }

    // ── Phase 1: parse header structure by peeking (no bytes consumed yet) ──

    let peek = buf.peek();

    let first = peek[0];
    let fmt = first >> 6;
    let csid_low = (first & 0x3F) as u32;

    let (csid, header_size) = match csid_low {
        0 => {
            if available < 2 {
                return Ok(0);
            }
            (peek[1] as u32 + 64, 2usize)
        }
        1 => {
            if available < 3 {
                return Ok(0);
            }
            (((peek[1] as u32) | ((peek[2] as u32) << 8)) + 64, 3usize)
        }
        n => (n, 1usize),
    };

    // Number of message-header bytes this fmt carries
    let msg_field_size: usize = match fmt {
        0 => 11, // timestamp(3) + length(3) + typeid(1) + streamid(4)
        1 => 7,  // timestamp(3) + length(3) + typeid(1)
        2 => 3,  // timestamp(3)
        3 => 0,  // inherited entirely from stream state
        _ => return Err(ErrorCode::Chunk),
    };

    let base_needed = header_size + msg_field_size;
    if available < base_needed {
        return Ok(0);
    }

    // Compressed headers (fmt 1/2/3) inherit fields from prior stream state.
    // A compressed chunk on an unknown CSID is a protocol error.
    if fmt != 0 && reg.get(csid).is_none() {
        return Err(ErrorCode::Chunk);
    }

    // Peek at the 3-byte timestamp field (for fmt 0/1/2) to decide ext_ts
    // without consuming anything.
    let ext_ts_from_header = if fmt <= 2 {
        let off = header_size;
        let ts_raw =
            ((peek[off] as u32) << 16) | ((peek[off + 1] as u32) << 8) | (peek[off + 2] as u32);
        ts_raw >= 0xFFFFFF
    } else {
        false
    };

    // For fmt=3 continuation chunks the writer re-emits the 4-byte extended
    // timestamp whenever the original message had ts >= 0xFFFFFF.  Inherit
    // the flag from the stream's stored state.
    let ext_ts_from_stream = if fmt == 3 {
        reg.get(csid)
            .map(|s| s.type0_ext_ts)
            .ok_or(ErrorCode::Chunk)?
    } else {
        false
    };

    let ext_ts = ext_ts_from_header || ext_ts_from_stream;

    // Total header bytes (basic + message fields + optional ext timestamp)
    let total_header_needed = base_needed + if ext_ts { 4 } else { 0 };
    if available < total_header_needed {
        return Ok(0);
    }

    // Determine effective_length and per-stream chunk_size/reassembly_bytes_read
    // so we can include the payload slice in the upfront availability check.
    let eff_len_for_avail: u32 = match fmt {
        0 | 1 => {
            // message length is peeked from header bytes
            let off = header_size + 3;
            ((peek[off] as u32) << 16) | ((peek[off + 1] as u32) << 8) | (peek[off + 2] as u32)
        }
        _ => reg
            .get(csid)
            .map(|s| s.type0_msg_length)
            .ok_or(ErrorCode::Chunk)?,
    };

    if eff_len_for_avail > reg.max_msg_length {
        return Err(ErrorCode::Chunk);
    }

    // fmt 0/1 start a new message; treat reassembly as empty for the upfront
    // availability check so we compute the correct first-chunk payload size.
    let (chunk_sz_for_avail, reassembly_read_for_avail) = reg
        .get(csid)
        .map(|s| {
            (
                s.chunk_size as usize,
                if fmt <= 1 {
                    0
                } else {
                    s.reassembly_bytes_read as usize
                },
            )
        })
        .unwrap_or((reg.default_chunk_size as usize, 0));

    let remaining = (eff_len_for_avail as usize).saturating_sub(reassembly_read_for_avail);
    let to_read = remaining.min(chunk_sz_for_avail);

    if available < total_header_needed + to_read {
        return Ok(0);
    }

    // ── Phase 2: all bytes confirmed present — consume them ──

    // Consume basic header
    let mut hdr = vec![0u8; header_size];
    buf.read(&mut hdr).map_err(|_| ErrorCode::Io)?;

    // Consume message header and extract fields
    let timestamp: u32;
    let msg_length: u32;
    let msg_type_id: u8;
    let msg_stream_id: u32;

    match fmt {
        0 => {
            let mut mh = [0u8; 11];
            buf.read(&mut mh).map_err(|_| ErrorCode::Io)?;
            timestamp = ((mh[0] as u32) << 16) | ((mh[1] as u32) << 8) | (mh[2] as u32);
            msg_length = ((mh[3] as u32) << 16) | ((mh[4] as u32) << 8) | (mh[5] as u32);
            msg_type_id = mh[6];
            msg_stream_id = (mh[7] as u32)
                | ((mh[8] as u32) << 8)
                | ((mh[9] as u32) << 16)
                | ((mh[10] as u32) << 24);
        }
        1 => {
            let mut mh = [0u8; 7];
            buf.read(&mut mh).map_err(|_| ErrorCode::Io)?;
            timestamp = ((mh[0] as u32) << 16) | ((mh[1] as u32) << 8) | (mh[2] as u32);
            msg_length = ((mh[3] as u32) << 16) | ((mh[4] as u32) << 8) | (mh[5] as u32);
            msg_type_id = mh[6];
            msg_stream_id = 0; // inherited from stream state
        }
        2 => {
            let mut mh = [0u8; 3];
            buf.read(&mut mh).map_err(|_| ErrorCode::Io)?;
            timestamp = ((mh[0] as u32) << 16) | ((mh[1] as u32) << 8) | (mh[2] as u32);
            msg_length = 0;
            msg_type_id = 0;
            msg_stream_id = 0;
        }
        3 => {
            timestamp = 0;
            msg_length = 0;
            msg_type_id = 0;
            msg_stream_id = 0;
        }
        _ => return Err(ErrorCode::Chunk),
    }

    // Consume extended timestamp if present
    let final_timestamp = if ext_ts {
        let mut ts_buf = [0u8; 4];
        buf.read(&mut ts_buf).map_err(|_| ErrorCode::Io)?;
        ntoh32(&ts_buf)
    } else {
        timestamp
    };

    // ── Phase 3: update stream state and reassemble ──

    // Guard against per-connection reassembly buffer exhaustion before
    // touching any stream state (avoid partial mutation on rejection).
    // fmt=0/1 will immediately discard this CSID's buffer, so exclude its
    // current bytes from the total to avoid rejecting a valid restart near
    // the per-connection limit.
    if to_read > 0 {
        let replaced = if fmt <= 1 {
            reg.get(csid)
                .map(|s| s.reassembly_buf.available())
                .unwrap_or(0)
        } else {
            0
        };
        if reg.reassembly_bytes_in_use().saturating_sub(replaced) + to_read
            > reg.max_reassembly_bytes
        {
            return Err(ErrorCode::Chunk);
        }
    }

    let stream_idx = reg.get_or_create_index(csid)?;

    // fmt 0/1 start a fresh message on this CSID; discard any partial
    // reassembly left over from an abandoned prior message.
    if fmt == 0 || fmt == 1 {
        reg.release_stream_reassembly(stream_idx);
        let stream = &mut reg.streams[stream_idx];
        stream.reassembly_bytes_read = 0;
        stream.reassembly_buf.reset();
    }

    let (effective_ts, effective_length, effective_type_id, effective_stream_id, to_read) = {
        let stream = &mut reg.streams[stream_idx];

        // fmt=2/3 inherit length/type/stream-id from prior state on this CSID.
        // After a complete message, reassembly_bytes_read is 0 but inherited
        // header fields remain — that is valid for the next constant-size frame
        // (common for AAC) or a fmt=3 header reuse per RTMP spec.
        if (fmt == 2 || fmt == 3)
            && stream.reassembly_bytes_read == 0
            && stream.type0_msg_length == 0
        {
            return Err(ErrorCode::Chunk);
        }

        // fmt=3 can be either a continuation chunk or a complete new chunk that
        // legitimately reuses the previous message header context on this CSID.
        // The chunk layer cannot distinguish an intentionally reused fmt=3 header
        // from a peer-controlled message with the same inherited metadata, so it
        // must accept fmt=3 when the CSID has valid prior state. Higher layers must
        // validate command semantics and authorization for the resulting message.
        //
        // A fmt=3 chunk is a *new-message start* (as opposed to a continuation of
        // an in-flight message) exactly when no message is currently being
        // reassembled on this CSID -- reassembly_bytes_read is only 0 between
        // messages, never mid-message.
        let fmt3_starts_new_message = fmt == 3 && stream.reassembly_bytes_read == 0;

        match fmt {
            0 => {
                stream.type0_timestamp = final_timestamp;
                stream.type0_msg_length = msg_length;
                stream.type0_msg_type_id = msg_type_id;
                stream.type0_msg_stream_id = msg_stream_id;
                stream.type0_ext_ts = ext_ts;
                stream.last_delta = 0;
            }
            1 => {
                // fmt=1 carries a timestamp DELTA (RTMP spec 5.3.1.1), not an
                // absolute value -- it must be added to the running timestamp.
                stream.type0_timestamp = stream.type0_timestamp.wrapping_add(final_timestamp);
                stream.type0_msg_length = msg_length;
                stream.type0_msg_type_id = msg_type_id;
                stream.type0_ext_ts = ext_ts;
                stream.last_delta = final_timestamp;
            }
            2 => {
                // fmt=2 also carries a timestamp DELTA, same as fmt=1.
                stream.type0_timestamp = stream.type0_timestamp.wrapping_add(final_timestamp);
                stream.type0_ext_ts = ext_ts;
                stream.last_delta = final_timestamp;
            }
            3 if fmt3_starts_new_message => {
                // Per RTMP spec 5.3.1.3, a fmt=3 chunk that starts a new message
                // (rather than continuing one) implicitly repeats the previous
                // delta -- it does not freeze the timestamp at its old value.
                stream.type0_timestamp = stream.type0_timestamp.wrapping_add(stream.last_delta);
            }
            _ => {}
        }

        let effective_ts = stream.type0_timestamp;
        let effective_length = stream.type0_msg_length;
        let effective_type_id = stream.type0_msg_type_id;
        let effective_stream_id = stream.type0_msg_stream_id;
        let chunk_size = stream.chunk_size as usize;
        let remaining =
            (effective_length as usize).saturating_sub(stream.reassembly_bytes_read as usize);
        let to_read = remaining.min(chunk_size);
        (
            effective_ts,
            effective_length,
            effective_type_id,
            effective_stream_id,
            to_read,
        )
    };

    {
        let stream = &mut reg.streams[stream_idx];
        let chunk_data = &mut stream.chunk_read_scratch;
        if chunk_data.len() != to_read {
            chunk_data.resize(to_read, 0);
        }
        buf.read(&mut chunk_data[..to_read])
            .map_err(|_| ErrorCode::Io)?;
        stream
            .reassembly_buf
            .write(&chunk_data[..to_read])
            .map_err(|_| ErrorCode::Chunk)?;
        stream.reassembly_bytes_read += to_read as u32;
    }
    reg.note_reassembly_growth(to_read);

    // Release the scratch buffer's capacity after every chunk (not just on
    // message completion) so a peer that starts many large messages across
    // distinct CSIDs and then stalls mid-message can't pin
    // `max_active_csids * MAX_INBOUND_CHUNK_SIZE` scratch bytes outside
    // `max_reassembly_bytes` accounting -- the reassembly_buf already holds
    // the same bytes and is the only copy that needs to persist. Small
    // scratch buffers are left alone (matches the `last_payload` threshold
    // pattern) so steady small-chunk traffic reuses its allocation instead
    // of paying a free/alloc cycle per chunk.
    let message_complete = {
        let stream = &mut reg.streams[stream_idx];
        stream.chunk_read_scratch.clear();
        if stream.chunk_read_scratch.capacity() > crate::buffer::BUFFER_RESET_CAPACITY {
            stream.chunk_read_scratch.shrink_to_fit();
        }
        stream.reassembly_bytes_read >= effective_length
    };

    if message_complete {
        msg.csid = csid;
        msg.fmt = fmt;
        msg.timestamp = effective_ts;
        msg.msg_length = effective_length;
        msg.msg_type_id = effective_type_id;
        msg.msg_stream_id = effective_stream_id;
        msg.is_complete = true;

        // Copy out before reset: shrinking the reassembly buffer would
        // invalidate a pointer into its storage. Reuse last_payload's
        // allocation to avoid allocator churn on every complete message.
        {
            let stream = &mut reg.streams[stream_idx];
            stream.last_payload.clear();
            stream
                .last_payload
                .extend_from_slice(stream.reassembly_buf.peek());
            *payload_len = stream.last_payload.len();
            *payload = stream.last_payload.as_ptr();
            stream.reassembly_bytes_read = 0;
        }
        reg.release_stream_reassembly(stream_idx);
        reg.streams[stream_idx].reassembly_buf.reset();

        Ok(1)
    } else {
        msg.is_complete = false;
        Ok(0)
    }
}

/// Read one RTMP chunk and return a complete message payload as an owned buffer.
pub fn chunk_read_owned(
    buf: &mut Buffer,
    reg: &mut ChunkRegistry,
    msg: &mut ChunkMessage,
) -> Result<(i32, Vec<u8>)> {
    let mut payload_ptr: *const u8 = std::ptr::null();
    let mut payload_len = 0;
    let rc = chunk_read(buf, reg, None, msg, &mut payload_ptr, &mut payload_len)?;
    let payload = if rc == 1 && msg.is_complete && payload_len > 0 && !payload_ptr.is_null() {
        // SAFETY: chunk_read only returns non-null pointers into `last_payload`,
        // which remains valid until the next complete message on this CSID.
        unsafe { std::slice::from_raw_parts(payload_ptr, payload_len).to_vec() }
    } else {
        Vec::new()
    };
    // Production callers use this owned path exclusively. Release the per-CSID
    // `last_payload` copy immediately so a peer cannot pin up to
    // `max_active_csids * max_msg_length` bytes of completed message bodies on
    // one connection.
    if rc == 1 && msg.is_complete {
        if let Some(stream) = reg.get_mut(msg.csid) {
            stream.last_payload.clear();
            if stream.last_payload.capacity() > crate::buffer::BUFFER_RESET_CAPACITY {
                stream.last_payload.shrink_to_fit();
            }
        }
    }
    Ok((rc, payload))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunk::writer::chunk_write;

    fn fmt3_wire(csid: u32, payload: &[u8]) -> Vec<u8> {
        let mut v = Vec::new();
        if csid < 64 {
            v.push((3 << 6) | csid as u8);
        } else if csid < 320 {
            v.push(3 << 6);
            v.push((csid - 64) as u8);
        } else {
            v.push((3 << 6) | 1);
            v.push(((csid - 64) & 0xFF) as u8);
            v.push((((csid - 64) >> 8) & 0xFF) as u8);
        }
        v.extend_from_slice(payload);
        v
    }

    #[test]
    fn fmt3_after_complete_message_can_start_new_message_with_inherited_header() {
        let payload = b"hello";
        let msg = ChunkMessage {
            csid: 3,
            fmt: 0,
            timestamp: 1,
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
        let mut len = 0;
        assert_eq!(
            chunk_read(&mut wire, &mut reg, None, &mut out_msg, &mut ptr, &mut len).unwrap(),
            1
        );
        assert!(out_msg.is_complete);

        let mut next = Buffer::new();
        next.write(&fmt3_wire(3, b"again")).expect("fmt3 wire");
        let result = chunk_read(&mut next, &mut reg, None, &mut out_msg, &mut ptr, &mut len);

        assert_eq!(result.unwrap(), 1);
        assert!(out_msg.is_complete);
        assert_eq!(out_msg.msg_type_id, 0x14);
        assert_eq!(out_msg.msg_stream_id, 1);
        assert_eq!(len, 5);
    }

    #[test]
    fn fmt2_after_complete_message_can_start_new_message_with_inherited_header() {
        let payload = b"done";
        let msg = ChunkMessage {
            csid: 5,
            fmt: 0,
            timestamp: 0,
            msg_length: payload.len() as u32,
            msg_type_id: 0x08,
            msg_stream_id: 1,
            is_complete: false,
        };

        let mut wire = Buffer::new();
        chunk_write(&mut wire, &msg, payload, payload.len(), 128).unwrap();

        let mut reg = ChunkRegistry::new();
        let mut out_msg = ChunkMessage::default();
        let mut ptr = std::ptr::null();
        let mut len = 0;
        chunk_read(&mut wire, &mut reg, None, &mut out_msg, &mut ptr, &mut len).unwrap();
        assert!(out_msg.is_complete);

        let mut next = Buffer::new();
        next.write(&[2 << 6 | 5, 0, 0, 1]).unwrap();
        next.write(b"next").unwrap();
        let result = chunk_read(&mut next, &mut reg, None, &mut out_msg, &mut ptr, &mut len);
        assert_eq!(result.unwrap(), 1);
        assert!(out_msg.is_complete);
        assert_eq!(out_msg.msg_type_id, 0x08);
        assert_eq!(out_msg.msg_stream_id, 1);
        assert_eq!(len, 4);
    }

    #[test]
    fn fmt1_and_fmt2_timestamps_accumulate_as_deltas_not_absolutes() {
        let payload = b"first";
        let msg = ChunkMessage {
            csid: 5,
            fmt: 0,
            timestamp: 1000,
            msg_length: payload.len() as u32,
            msg_type_id: 0x08,
            msg_stream_id: 1,
            is_complete: false,
        };
        let mut wire = Buffer::new();
        chunk_write(&mut wire, &msg, payload, payload.len(), 128).unwrap();

        let mut reg = ChunkRegistry::new();
        let mut out_msg = ChunkMessage::default();
        let mut ptr = std::ptr::null();
        let mut len = 0;
        chunk_read(&mut wire, &mut reg, None, &mut out_msg, &mut ptr, &mut len).unwrap();
        assert_eq!(out_msg.timestamp, 1000);

        // fmt=1 header: timestamp(3)=33, length(3)=4, typeid(1)=0x08.
        let mut fmt1 = Buffer::new();
        fmt1.write(&[1 << 6 | 5, 0, 0, 33, 0, 0, 4, 0x08]).unwrap();
        fmt1.write(b"next").unwrap();
        chunk_read(&mut fmt1, &mut reg, None, &mut out_msg, &mut ptr, &mut len).unwrap();
        assert_eq!(
            out_msg.timestamp, 1033,
            "fmt=1 timestamp must add the delta to the running total"
        );

        // fmt=2 header: timestamp(3)=33 (delta only; length/type inherited).
        let mut fmt2 = Buffer::new();
        fmt2.write(&[2 << 6 | 5, 0, 0, 33]).unwrap();
        fmt2.write(b"next").unwrap();
        chunk_read(&mut fmt2, &mut reg, None, &mut out_msg, &mut ptr, &mut len).unwrap();
        assert_eq!(
            out_msg.timestamp, 1066,
            "fmt=2 timestamp must add the delta to the running total"
        );

        // fmt=3 starting a brand new message (no message currently in
        // flight) implicitly repeats the last delta (33) per RTMP spec
        // 5.3.1.3 rather than freezing the timestamp at its old value.
        let mut fmt3 = Buffer::new();
        fmt3.write(&[3 << 6 | 5]).unwrap();
        fmt3.write(b"next").unwrap();
        chunk_read(&mut fmt3, &mut reg, None, &mut out_msg, &mut ptr, &mut len).unwrap();
        assert_eq!(
            out_msg.timestamp, 1099,
            "fmt=3 new-message start must repeat the previous delta"
        );
    }

    #[test]
    fn fmt2_without_prior_header_state_is_rejected() {
        let mut reg = ChunkRegistry::new();
        reg.get_or_create(5).unwrap();

        let mut next = Buffer::new();
        next.write(&[2 << 6 | 5, 0, 0, 1]).unwrap();
        next.write(b"data").unwrap();

        let mut out_msg = ChunkMessage::default();
        let mut ptr = std::ptr::null();
        let mut len = 0;
        let result = chunk_read(&mut next, &mut reg, None, &mut out_msg, &mut ptr, &mut len);
        assert!(matches!(result, Err(ErrorCode::Chunk)));
    }

    #[test]
    fn rejects_message_length_above_registry_cap() {
        let mut reg = ChunkRegistry::new();
        reg.max_msg_length = 8;

        let payload = vec![0u8; 16];
        let msg = ChunkMessage {
            csid: 3,
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
        assert!(matches!(
            chunk_read(&mut wire, &mut reg, None, &mut out_msg, &mut ptr, &mut len),
            Err(ErrorCode::Chunk)
        ));
    }

    #[test]
    fn tiny_chunk_size_multiplies_chunk_read_iterations() {
        let payload = vec![0xAB_u8; 512];
        let msg = ChunkMessage {
            csid: 4,
            fmt: 0,
            timestamp: 0,
            msg_length: payload.len() as u32,
            msg_type_id: 0x09,
            msg_stream_id: 1,
            is_complete: false,
        };

        let mut wire_small = Buffer::new();
        chunk_write(&mut wire_small, &msg, &payload, payload.len(), 1).unwrap();
        let mut wire_default = Buffer::new();
        chunk_write(&mut wire_default, &msg, &payload, payload.len(), 128).unwrap();

        let mut reg_small = ChunkRegistry::new();
        // Below the network-validated minimum; set directly to stress reader iteration.
        reg_small.default_chunk_size = 1;
        let mut reg_default = ChunkRegistry::new();

        let mut iterations_small = 0usize;
        let mut out_msg = ChunkMessage::default();
        let mut ptr = std::ptr::null();
        let mut len = 0usize;
        loop {
            let rc = chunk_read(
                &mut wire_small,
                &mut reg_small,
                None,
                &mut out_msg,
                &mut ptr,
                &mut len,
            )
            .unwrap();
            if rc == 1 {
                break;
            }
            iterations_small += 1;
            if wire_small.available() == 0 {
                panic!("incomplete reassembly at chunk_size=1");
            }
        }

        let mut iterations_default = 0usize;
        loop {
            let rc = chunk_read(
                &mut wire_default,
                &mut reg_default,
                None,
                &mut out_msg,
                &mut ptr,
                &mut len,
            )
            .unwrap();
            if rc == 1 {
                break;
            }
            iterations_default += 1;
            if wire_default.available() == 0 {
                panic!("incomplete reassembly at chunk_size=128");
            }
        }

        assert!(
            iterations_small > iterations_default * 10,
            "chunk_size=1 took {iterations_small} iterations vs {iterations_default} at 128"
        );
    }

    #[test]
    fn chunk_read_owned_releases_last_payload_after_copy() {
        use crate::chunk::state::DEFAULT_MAX_ACTIVE_CSIDS;

        let payload = vec![0xCD_u8; 64 * 1024];
        let mut reg = ChunkRegistry::new();
        reg.max_active_csids = DEFAULT_MAX_ACTIVE_CSIDS;

        for csid in 3..(3 + DEFAULT_MAX_ACTIVE_CSIDS as u32) {
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
            loop {
                let (rc, owned) = chunk_read_owned(&mut wire, &mut reg, &mut out_msg).unwrap();
                if rc == 1 {
                    assert_eq!(owned.len(), payload.len());
                    break;
                }
                assert_eq!(rc, 0);
            }
        }

        let retained: usize = reg
            .streams
            .iter()
            .filter(|s| s.in_use)
            .map(|s| s.last_payload.len())
            .sum();
        assert_eq!(
            retained, 0,
            "chunk_read_owned must not retain completed payloads in last_payload"
        );
    }
}
