//! Message dispatch, aggregate decode
//!
//! Mirrors `src/message/message.h` and `src/message/message.c`.
//!
//! `decode()`/`decode_aggregate()` and the `Connection`/`Stream` traits below
//! are a generic, testable reference dispatcher kept for embedders that want
//! to drive message classification against their own connection type rather
//! than `session::conn::Conn`. The production ingest path does not go through
//! this trait object: `session::conn::Conn::handle_message()` (and, for
//! Aggregate messages specifically, `Conn::handle_aggregate()`) implements
//! the same dispatch directly against `Conn`'s concrete fields for
//! performance and to avoid the `dyn Connection` indirection on the hot path.

use super::command;
use super::control;
use crate::buffer::Buffer;
use crate::chunk::reader::{ChunkMessage, chunk_read};
use crate::chunk::state::ChunkRegistry;
use crate::types::{AudioCodec, ErrorCode, Frame, FrameType, Result, VideoCodec};

/* RTMP message type IDs */
pub const RTMP_MSG_SET_CHUNK_SIZE: u8 = 0x01;
pub const RTMP_MSG_ABORT_MESSAGE: u8 = 0x02;
pub const RTMP_MSG_ACKNOWLEDGEMENT: u8 = 0x03;
pub const RTMP_MSG_USER_CONTROL: u8 = 0x04;
pub const RTMP_MSG_WINDOW_ACK_SIZE: u8 = 0x05;
pub const RTMP_MSG_SET_PEER_BANDWIDTH: u8 = 0x06;
pub const RTMP_MSG_AUDIO: u8 = 0x08;
pub const RTMP_MSG_VIDEO: u8 = 0x09;
pub const RTMP_MSG_AMF3_DATA: u8 = 0x0F;
pub const RTMP_MSG_AMF3_SHARED_OBJECT: u8 = 0x10;
pub const RTMP_MSG_AMF3_COMMAND: u8 = 0x11;
pub const RTMP_MSG_AMF0_DATA: u8 = 0x12;
pub const RTMP_MSG_AMF0_COMMAND: u8 = 0x14;
pub const RTMP_MSG_AGGREGATE: u8 = 0x16;

const MAX_AGGREGATE_SUBTAGS: usize = 4096;

/// Connection trait for message dispatch
pub trait Connection {
    fn get_frame_callback(&self) -> Option<fn(&Frame)>;
    fn get_current_stream(&self) -> Option<&dyn Stream>;
    fn handle_command(&mut self, payload: &[u8]) -> Result<()>;
    fn set_window_ack_size(&mut self, size: u32);
    fn reset_chunk_stream(&mut self, csid: u32);
    fn set_all_chunk_size(&mut self, chunk_size: u32);
}

/// Stream trait for message dispatch
pub trait Stream {
    fn is_publishing(&self) -> bool;
}

/// Build a frame from an audio message payload.
fn deliver_audio_frame(conn: &mut dyn Connection, timestamp: u32, payload: &[u8]) {
    let mut frame = Frame {
        frame_type: FrameType::Audio,
        timestamp,
        composition_time: 0,
        size: payload.len() as u32,
        data: payload.as_ptr(),
        audio_codec: AudioCodec::Aac,
        audio_sample_rate: 0,
        audio_channels: 0,
        audio_bit_depth: 0,
        audio_fourcc: crate::types::FourCc { cc: [0; 5] },
        video_codec: VideoCodec::H264,
        video_fourcc: crate::types::FourCc { cc: [0; 5] },
        video_frame_type: 0,
        is_metadata: 0,
        track_id: u8::MAX,
    };

    if !payload.is_empty() {
        let tag = payload[0];
        frame.audio_codec = match (tag >> 4) & 0x0F {
            0 => AudioCodec::Pcm,
            1 => AudioCodec::Adpcm,
            2 => AudioCodec::Mp3,
            3 => AudioCodec::PcmLe,
            4 => AudioCodec::Nelly16k,
            5 => AudioCodec::Nelly8k,
            6 => AudioCodec::Nelly,
            7 => AudioCodec::G711A,
            8 => AudioCodec::G711U,
            10 => AudioCodec::Aac,
            11 => AudioCodec::Speex,
            14 => AudioCodec::Opus,
            _ => AudioCodec::Aac,
        };
    }

    if let Some(cb) = conn.get_frame_callback() {
        cb(&frame);
    }
}

/// Build a frame from a video message payload.
fn deliver_video_frame(conn: &mut dyn Connection, timestamp: u32, payload: &[u8]) {
    let mut frame = Frame {
        frame_type: FrameType::Video,
        timestamp,
        composition_time: 0,
        size: payload.len() as u32,
        data: payload.as_ptr(),
        audio_codec: AudioCodec::Aac,
        audio_sample_rate: 0,
        audio_channels: 0,
        audio_bit_depth: 0,
        audio_fourcc: crate::types::FourCc { cc: [0; 5] },
        video_codec: VideoCodec::H264,
        video_fourcc: crate::types::FourCc { cc: [0; 5] },
        video_frame_type: 0,
        is_metadata: 0,
        track_id: u8::MAX,
    };

    if !payload.is_empty() {
        let tag = payload[0];
        frame.video_frame_type = (tag >> 4) & 0x0F;
        frame.video_codec = match tag & 0x0F {
            1 => VideoCodec::Jpeg,
            2 => VideoCodec::Sorenson,
            3 => VideoCodec::Screen,
            4 => VideoCodec::Vp6,
            5 => VideoCodec::Vp6a,
            6 => VideoCodec::Screen2,
            7 => VideoCodec::H264,
            12 => VideoCodec::H265,
            13 => VideoCodec::Av1,
            _ => VideoCodec::H264,
        };
    }

    if let Some(cb) = conn.get_frame_callback() {
        cb(&frame);
    }
}

/// Decode an Aggregate message (type 0x16).
pub fn decode_aggregate(
    conn: &mut dyn Connection,
    chunk: &ChunkMessage,
    payload: &[u8],
) -> Result<()> {
    let mut pos = 0;
    let mut have_base = false;
    let mut base_ts: u32 = 0;
    let mut subtags = 0;

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
        if body + data_size as usize > payload.len() {
            return Err(ErrorCode::Protocol);
        }

        if !have_base {
            base_ts = ts;
            have_base = true;
        }
        // Use wrapping arithmetic: a sub-tag ts less than base_ts (malformed
        // but possible) must not cause a panic in debug or silent wrap in release.
        let out_ts = chunk.timestamp.wrapping_add(ts.wrapping_sub(base_ts));

        let is_publishing = conn
            .get_current_stream()
            .map(|s| s.is_publishing())
            .unwrap_or(false);
        if tag_type == 0x08 && is_publishing {
            deliver_audio_frame(conn, out_ts, &payload[body..body + data_size as usize]);
        } else if tag_type == 0x09 && is_publishing {
            deliver_video_frame(conn, out_ts, &payload[body..body + data_size as usize]);
        }

        pos = body + data_size as usize + 4;
    }

    Ok(())
}

/// Decode a reassembled message.
pub fn decode(conn: &mut dyn Connection, chunk: &ChunkMessage, payload: &[u8]) -> Result<()> {
    match chunk.msg_type_id {
        RTMP_MSG_SET_CHUNK_SIZE => {
            if payload.len() >= 4 {
                if let Ok(cs) = control::read_set_chunk_size(payload) {
                    conn.set_all_chunk_size(cs);
                }
            }
        }
        RTMP_MSG_ABORT_MESSAGE => {
            if payload.len() >= 4 {
                if let Ok(csid) = control::read_abort_message(payload) {
                    conn.reset_chunk_stream(csid);
                }
            }
        }
        RTMP_MSG_ACKNOWLEDGEMENT => {
            if payload.len() >= 4 {
                let _ = control::read_acknowledgement_size(payload);
            }
        }
        RTMP_MSG_WINDOW_ACK_SIZE => {
            if payload.len() >= 4 {
                if let Ok(win) = control::read_window_ack_size(payload) {
                    conn.set_window_ack_size(win);
                }
            }
        }
        RTMP_MSG_SET_PEER_BANDWIDTH => {
            if payload.len() >= 5 {
                let _ = control::read_set_peer_bandwidth(payload);
            }
        }
        RTMP_MSG_USER_CONTROL => {
            if payload.len() >= 6 {
                let has_p2 = payload.len() >= 10;
                let _ = control::read_user_control(payload, has_p2);
            }
        }
        RTMP_MSG_AUDIO => {
            if conn
                .get_current_stream()
                .map(|s| s.is_publishing())
                .unwrap_or(false)
            {
                deliver_audio_frame(conn, chunk.timestamp, payload);
            }
        }
        RTMP_MSG_VIDEO => {
            if conn
                .get_current_stream()
                .map(|s| s.is_publishing())
                .unwrap_or(false)
            {
                deliver_video_frame(conn, chunk.timestamp, payload);
            }
        }
        RTMP_MSG_AMF0_COMMAND => {
            return conn.handle_command(payload);
        }
        RTMP_MSG_AMF3_COMMAND => {
            if !payload.is_empty() && payload[0] == 0x00 {
                return conn.handle_command(&payload[1..]);
            }
            return conn.handle_command(payload);
        }
        RTMP_MSG_AMF0_DATA | RTMP_MSG_AMF3_DATA | RTMP_MSG_AMF3_SHARED_OBJECT => {
            // Data messages — handled on the production path in
            // `session::conn::Conn::handle_data_message()`.
        }
        RTMP_MSG_AGGREGATE => {
            return decode_aggregate(conn, chunk, payload);
        }
        _ => {
            // Unknown message type
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockConn {
        handled_commands: Vec<Vec<u8>>,
        window_ack_size: Option<u32>,
        reset_csid: Option<u32>,
        chunk_size: Option<u32>,
    }

    impl MockConn {
        fn new() -> Self {
            Self {
                handled_commands: Vec::new(),
                window_ack_size: None,
                reset_csid: None,
                chunk_size: None,
            }
        }
    }

    impl Connection for MockConn {
        fn get_frame_callback(&self) -> Option<fn(&Frame)> {
            None
        }
        fn get_current_stream(&self) -> Option<&dyn Stream> {
            None
        }
        fn handle_command(&mut self, payload: &[u8]) -> Result<()> {
            self.handled_commands.push(payload.to_vec());
            Ok(())
        }
        fn set_window_ack_size(&mut self, size: u32) {
            self.window_ack_size = Some(size);
        }
        fn reset_chunk_stream(&mut self, csid: u32) {
            self.reset_csid = Some(csid);
        }
        fn set_all_chunk_size(&mut self, chunk_size: u32) {
            self.chunk_size = Some(chunk_size);
        }
    }

    fn chunk_msg(msg_type_id: u8) -> ChunkMessage {
        ChunkMessage {
            csid: 3,
            fmt: 0,
            timestamp: 0,
            msg_length: 0,
            msg_type_id,
            msg_stream_id: 0,
            is_complete: true,
        }
    }

    #[test]
    fn amf0_command_is_dispatched_to_handle_command() {
        let mut conn = MockConn::new();
        let chunk = chunk_msg(RTMP_MSG_AMF0_COMMAND);
        decode(&mut conn, &chunk, b"connect-payload").unwrap();
        assert_eq!(conn.handled_commands, vec![b"connect-payload".to_vec()]);
    }

    #[test]
    fn amf3_command_is_dispatched_to_handle_command() {
        let mut conn = MockConn::new();
        let chunk = chunk_msg(RTMP_MSG_AMF3_COMMAND);
        // AMF3 command messages are prefixed with a 1-byte marker (0x00) that
        // must be stripped before the AMF0-encoded body is handed off.
        let mut payload = vec![0x00];
        payload.extend_from_slice(b"invoke-payload");
        decode(&mut conn, &chunk, &payload).unwrap();
        assert_eq!(conn.handled_commands, vec![b"invoke-payload".to_vec()]);
    }

    #[test]
    fn set_chunk_size_updates_connection() {
        let mut conn = MockConn::new();
        let chunk = chunk_msg(RTMP_MSG_SET_CHUNK_SIZE);
        decode(&mut conn, &chunk, &4096u32.to_be_bytes()).unwrap();
        assert_eq!(conn.chunk_size, Some(4096));
    }

    #[test]
    fn window_ack_size_updates_connection() {
        let mut conn = MockConn::new();
        let chunk = chunk_msg(RTMP_MSG_WINDOW_ACK_SIZE);
        decode(&mut conn, &chunk, &2_500_000u32.to_be_bytes()).unwrap();
        assert_eq!(conn.window_ack_size, Some(2_500_000));
    }

    #[test]
    fn abort_message_resets_chunk_stream() {
        let mut conn = MockConn::new();
        let chunk = chunk_msg(RTMP_MSG_ABORT_MESSAGE);
        decode(&mut conn, &chunk, &7u32.to_be_bytes()).unwrap();
        assert_eq!(conn.reset_csid, Some(7));
    }

    #[test]
    fn unknown_message_type_is_ignored() {
        let mut conn = MockConn::new();
        let chunk = chunk_msg(0xFE);
        assert!(decode(&mut conn, &chunk, b"whatever").is_ok());
        assert!(conn.handled_commands.is_empty());
    }
}
