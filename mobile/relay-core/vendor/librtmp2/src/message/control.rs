//! RTMP control message encoder/decoder
//!
//! Mirrors `src/message/control.h` and `src/message/control.c`.

use crate::buffer::Buffer;
use crate::bytes::ntoh32;
use crate::types::ErrorCode;
use crate::types::Result;

/* Control message types */
pub const CTRL_SET_CHUNK_SIZE: u8 = 0x01;
pub const CTRL_ABORT_MESSAGE: u8 = 0x02;
pub const CTRL_ACKNOWLEDGEMENT: u8 = 0x03;
pub const CTRL_USER_CONTROL: u8 = 0x04;
pub const CTRL_WINDOW_ACK_SIZE: u8 = 0x05;
pub const CTRL_SET_PEER_BANDWIDTH: u8 = 0x06;

/* User Control event types */
pub const UCTRL_STREAM_BEGIN: u16 = 0x00;
pub const UCTRL_STREAM_EOF: u16 = 0x01;
pub const UCTRL_STREAM_DRY: u16 = 0x02;
pub const UCTRL_SET_BUFFER_LENGTH: u16 = 0x03;
pub const UCTRL_STREAM_IS_RECORDED: u16 = 0x04;
pub const UCTRL_PING_REQUEST: u16 = 0x06;
pub const UCTRL_PING_RESPONSE: u16 = 0x07;

/// Minimum inbound chunk size accepted from peers. Values below the RTMP default
/// (128) enable extreme per-byte allocation/parsing amplification during
/// message reassembly (a 4 MiB message becomes millions of chunk iterations).
const MIN_CHUNK_SIZE: u32 = 128;
const MAX_CHUNK_SIZE: u32 = 0xFFFFFF;
/// Upper bound for peer-requested inbound chunk sizes (memory churn protection).
pub const MAX_INBOUND_CHUNK_SIZE: u32 = 65536;
/// Reject peer WindowAckSize values below this to prevent ACK-per-byte amplification.
pub const MIN_WINDOW_ACK_SIZE: u32 = 1024;

/* ── Encoder ── */

/// Write a SetChunkSize control message.
pub fn write_set_chunk_size(buf: &mut Buffer, chunk_size: u32) -> Result<()> {
    buf.write(&[CTRL_SET_CHUNK_SIZE])
        .map_err(|_| ErrorCode::Internal)?;
    buf.write(&chunk_size.to_be_bytes())
        .map_err(|_| ErrorCode::Internal)?;
    Ok(())
}

/// Write an AbortMessage control message.
pub fn write_abort_message(buf: &mut Buffer, csid: u32) -> Result<()> {
    buf.write(&[CTRL_ABORT_MESSAGE])
        .map_err(|_| ErrorCode::Internal)?;
    buf.write(&csid.to_be_bytes())
        .map_err(|_| ErrorCode::Internal)?;
    Ok(())
}

/// Write an Acknowledgement control message.
pub fn write_acknowledgement(buf: &mut Buffer, sequence_number: u32) -> Result<()> {
    buf.write(&[CTRL_ACKNOWLEDGEMENT])
        .map_err(|_| ErrorCode::Internal)?;
    buf.write(&sequence_number.to_be_bytes())
        .map_err(|_| ErrorCode::Internal)?;
    Ok(())
}

/// Write a WindowAckSize control message.
pub fn write_window_ack_size(buf: &mut Buffer, window_size: u32) -> Result<()> {
    buf.write(&[CTRL_WINDOW_ACK_SIZE])
        .map_err(|_| ErrorCode::Internal)?;
    buf.write(&window_size.to_be_bytes())
        .map_err(|_| ErrorCode::Internal)?;
    Ok(())
}

/// Write a SetPeerBandwidth control message.
pub fn write_set_peer_bandwidth(buf: &mut Buffer, window_size: u32, limit_type: u8) -> Result<()> {
    buf.write(&[CTRL_SET_PEER_BANDWIDTH])
        .map_err(|_| ErrorCode::Internal)?;
    buf.write(&window_size.to_be_bytes())
        .map_err(|_| ErrorCode::Internal)?;
    buf.write(&[limit_type]).map_err(|_| ErrorCode::Internal)?;
    Ok(())
}

/// Write a User Control Stream Begin event.
pub fn write_user_control_stream_begin(buf: &mut Buffer, stream_id: u32) -> Result<()> {
    buf.write(&UCTRL_STREAM_BEGIN.to_be_bytes())
        .map_err(|_| ErrorCode::Internal)?;
    buf.write(&stream_id.to_be_bytes())
        .map_err(|_| ErrorCode::Internal)?;
    Ok(())
}

/// Write a User Control Stream EOF event.
pub fn write_user_control_stream_eof(buf: &mut Buffer, stream_id: u32) -> Result<()> {
    buf.write(&UCTRL_STREAM_EOF.to_be_bytes())
        .map_err(|_| ErrorCode::Internal)?;
    buf.write(&stream_id.to_be_bytes())
        .map_err(|_| ErrorCode::Internal)?;
    Ok(())
}

/// Write a User Control SetBufferLength event.
pub fn write_user_control_set_buffer_length(
    buf: &mut Buffer,
    stream_id: u32,
    ms: u32,
) -> Result<()> {
    buf.write(&UCTRL_SET_BUFFER_LENGTH.to_be_bytes())
        .map_err(|_| ErrorCode::Internal)?;
    buf.write(&stream_id.to_be_bytes())
        .map_err(|_| ErrorCode::Internal)?;
    buf.write(&ms.to_be_bytes())
        .map_err(|_| ErrorCode::Internal)?;
    Ok(())
}

/// Write a User Control Ping Request event (4-byte opaque timestamp/token).
pub fn write_user_control_ping_request(buf: &mut Buffer, timestamp: u32) -> Result<()> {
    buf.write(&UCTRL_PING_REQUEST.to_be_bytes())
        .map_err(|_| ErrorCode::Internal)?;
    buf.write(&timestamp.to_be_bytes())
        .map_err(|_| ErrorCode::Internal)?;
    Ok(())
}

/// Write a User Control Ping Response event (echoes the request timestamp/token).
pub fn write_user_control_ping_response(buf: &mut Buffer, timestamp: u32) -> Result<()> {
    buf.write(&UCTRL_PING_RESPONSE.to_be_bytes())
        .map_err(|_| ErrorCode::Internal)?;
    buf.write(&timestamp.to_be_bytes())
        .map_err(|_| ErrorCode::Internal)?;
    Ok(())
}

/* ── Decoder ── */

/// Read a SetChunkSize message.
pub fn read_set_chunk_size(data: &[u8]) -> Result<u32> {
    if data.len() < 4 {
        return Err(ErrorCode::Protocol);
    }
    let cs = ntoh32(data);
    if cs < MIN_CHUNK_SIZE {
        return Err(ErrorCode::Protocol);
    }
    Ok(cs.min(MAX_INBOUND_CHUNK_SIZE))
}

/// Read an AbortMessage.
pub fn read_abort_message(data: &[u8]) -> Result<u32> {
    if data.len() < 4 {
        return Err(ErrorCode::Protocol);
    }
    Ok(ntoh32(data))
}

/// Read an Acknowledgement size.
pub fn read_acknowledgement_size(data: &[u8]) -> Result<u32> {
    if data.len() < 4 {
        return Err(ErrorCode::Protocol);
    }
    Ok(ntoh32(data))
}

/// Read a WindowAckSize.
pub fn read_window_ack_size(data: &[u8]) -> Result<u32> {
    if data.len() < 4 {
        return Err(ErrorCode::Protocol);
    }
    let win = ntoh32(data);
    if win > 0 && win < MIN_WINDOW_ACK_SIZE {
        return Err(ErrorCode::Protocol);
    }
    Ok(win)
}

/// Read a SetPeerBandwidth.
pub fn read_set_peer_bandwidth(data: &[u8]) -> Result<(u32, u8)> {
    if data.len() < 5 {
        return Err(ErrorCode::Protocol);
    }
    Ok((ntoh32(data), data[4]))
}

/// Read a User Control event.
pub fn read_user_control(data: &[u8], param2: bool) -> Result<(u16, u32, Option<u32>)> {
    if data.len() < 6 {
        return Err(ErrorCode::Protocol);
    }
    let event_type = ((data[0] as u16) << 8) | (data[1] as u16);
    let param1 = ntoh32(&data[2..]);
    let p2 = if param2 && data.len() >= 10 {
        Some(ntoh32(&data[6..]))
    } else {
        None
    };
    Ok((event_type, param1, p2))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_chunk_size_round_trips_and_is_big_endian() {
        let mut buf = Buffer::new();
        write_set_chunk_size(&mut buf, 4096).unwrap();
        assert_eq!(buf.peek(), &[CTRL_SET_CHUNK_SIZE, 0x00, 0x00, 0x10, 0x00]);
        assert_eq!(read_set_chunk_size(&buf.peek()[1..]).unwrap(), 4096);
    }

    #[test]
    fn set_chunk_size_rejects_zero_and_clamps_large_values() {
        assert!(read_set_chunk_size(&[0, 0, 0, 0]).is_err());
        assert!(read_set_chunk_size(&1u32.to_be_bytes()).is_err());
        assert_eq!(read_set_chunk_size(&128u32.to_be_bytes()).unwrap(), 128);
        assert_eq!(
            read_set_chunk_size(&[0xFF, 0xFF, 0xFF, 0xFF]).unwrap(),
            MAX_INBOUND_CHUNK_SIZE
        );
    }

    #[test]
    fn window_ack_size_round_trips() {
        let mut buf = Buffer::new();
        write_window_ack_size(&mut buf, 2_500_000).unwrap();
        assert_eq!(read_window_ack_size(&buf.peek()[1..]).unwrap(), 2_500_000);
    }

    #[test]
    fn window_ack_size_rejects_amplification_values() {
        assert!(read_window_ack_size(&1u32.to_be_bytes()).is_err());
        assert!(read_window_ack_size(&1023u32.to_be_bytes()).is_err());
        assert_eq!(read_window_ack_size(&0u32.to_be_bytes()).unwrap(), 0);
        assert_eq!(read_window_ack_size(&1024u32.to_be_bytes()).unwrap(), 1024);
    }

    #[test]
    fn abort_message_rejects_short_input() {
        // A truncated AbortMessage (fewer than 4 bytes) must be rejected by
        // the decoder itself rather than relying on callers to pre-check the
        // length — `ntoh32()` indexes data[0..4] directly and panics on
        // shorter slices, so this was a reachable DoS if any caller forwarded
        // the payload without its own length guard.
        assert!(read_abort_message(&[]).is_err());
        assert!(read_abort_message(&[0x00]).is_err());
        assert!(read_abort_message(&[0x00, 0x00, 0x00]).is_err());
        assert_eq!(read_abort_message(&7u32.to_be_bytes()).unwrap(), 7);
    }

    #[test]
    fn acknowledgement_size_rejects_short_input() {
        assert!(read_acknowledgement_size(&[]).is_err());
        assert!(read_acknowledgement_size(&[0x00, 0x00]).is_err());
        assert_eq!(
            read_acknowledgement_size(&123u32.to_be_bytes()).unwrap(),
            123
        );
    }

    #[test]
    fn window_ack_size_rejects_short_input() {
        // Same landmine as abort/acknowledgement above: ntoh32() indexes
        // data[0..4] directly, so this decoder must reject short input itself
        // rather than relying on every caller to pre-check the length.
        assert!(read_window_ack_size(&[]).is_err());
        assert!(read_window_ack_size(&[0x00, 0x00, 0x00]).is_err());
        assert_eq!(
            read_window_ack_size(&(MIN_WINDOW_ACK_SIZE).to_be_bytes()).unwrap(),
            MIN_WINDOW_ACK_SIZE
        );
    }

    #[test]
    fn set_peer_bandwidth_round_trips() {
        let mut buf = Buffer::new();
        write_set_peer_bandwidth(&mut buf, 2_500_000, 2).unwrap();
        let (window, limit_type) = read_set_peer_bandwidth(&buf.peek()[1..]).unwrap();
        assert_eq!(window, 2_500_000);
        assert_eq!(limit_type, 2);
    }

    #[test]
    fn user_control_stream_begin_wire_format_is_big_endian() {
        let mut buf = Buffer::new();
        write_user_control_stream_begin(&mut buf, 1).unwrap();
        assert_eq!(buf.peek(), &[0x00, 0x00, 0x00, 0x00, 0x00, 0x01]);
        let (event_type, stream_id, _) = read_user_control(buf.peek(), false).unwrap();
        assert_eq!(event_type, UCTRL_STREAM_BEGIN);
        assert_eq!(stream_id, 1);
    }

    #[test]
    fn user_control_set_buffer_length_round_trips_both_params() {
        let mut buf = Buffer::new();
        write_user_control_set_buffer_length(&mut buf, 1, 3000).unwrap();
        let (event_type, stream_id, ms) = read_user_control(buf.peek(), true).unwrap();
        assert_eq!(event_type, UCTRL_SET_BUFFER_LENGTH);
        assert_eq!(stream_id, 1);
        assert_eq!(ms, Some(3000));
    }

    #[test]
    fn user_control_ping_round_trips() {
        let mut buf = Buffer::new();
        write_user_control_ping_request(&mut buf, 0x1234_5678).unwrap();
        let (event_type, token, _) = read_user_control(buf.peek(), false).unwrap();
        assert_eq!(event_type, UCTRL_PING_REQUEST);
        assert_eq!(token, 0x1234_5678);

        buf.reset();
        write_user_control_ping_response(&mut buf, 0x1234_5678).unwrap();
        let (event_type, token, _) = read_user_control(buf.peek(), false).unwrap();
        assert_eq!(event_type, UCTRL_PING_RESPONSE);
        assert_eq!(token, 0x1234_5678);
    }
}
