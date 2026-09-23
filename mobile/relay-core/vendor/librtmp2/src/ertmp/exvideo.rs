//! Enhanced RTMP v1 VideoTagHeader / FourCC parsing
//!
//! Mirrors `src/ertmp/exvideo.c`.

use crate::types::{ErrorCode, Result, VideoHeader};

/// Parse a FourCC from raw bytes.
pub fn fourcc_parse(data: &[u8]) -> Result<[u8; 5]> {
    if data.len() < 4 {
        return Err(ErrorCode::Io);
    }
    let mut fourcc = [0u8; 5];
    fourcc[..4].copy_from_slice(&data[..4]);
    Ok(fourcc)
}

fn is_composition_time_codec(fourcc: &[u8]) -> bool {
    fourcc[..4] == *b"avc1" || fourcc[..4] == *b"hvc1"
}

/// Parse an Enhanced RTMP v1 video tag header.
pub fn exvideo_parse(data: &[u8], hdr: &mut VideoHeader) -> Result<()> {
    *hdr = VideoHeader::default();

    if data.is_empty() {
        return Err(ErrorCode::Io);
    }

    let b0 = data[0];
    hdr.is_ex_header = if b0 & 0x80 != 0 { 1 } else { 0 };

    if hdr.is_ex_header == 0 {
        hdr.frame_type = (b0 >> 4) & 0x0F;
        hdr.header_size = 1;
        // Legacy FLV AVCVIDEOPACKET: AVCPacketType (1) + CompositionTime SI24.
        if (b0 & 0x0F) == 7 && data.len() >= 5 {
            let ct = ((data[2] as i32) << 16) | ((data[3] as i32) << 8) | (data[4] as i32);
            let ct = if ct & 0x00800000 != 0 {
                ct | 0xFF000000u32 as i32
            } else {
                ct
            };
            hdr.composition_time = ct as u32;
        }
        return Ok(());
    }

    hdr.frame_type = (b0 >> 4) & 0x07;
    hdr.packet_type = b0 & 0x0F;

    if data.len() < 5 {
        return Err(ErrorCode::Io);
    }

    hdr.fourcc[..4].copy_from_slice(&data[1..5]);
    hdr.header_size = 5;

    if hdr.packet_type == 1 && is_composition_time_codec(&hdr.fourcc) {
        if data.len() < 8 {
            return Err(ErrorCode::Io);
        }
        let ct = ((data[5] as i32) << 16) | ((data[6] as i32) << 8) | (data[7] as i32);
        let ct = if ct & 0x00800000 != 0 {
            ct | 0xFF000000u32 as i32
        } else {
            ct
        };
        hdr.composition_time = ct as u32;
        hdr.header_size = 8;
    }

    Ok(())
}

/// Write an Enhanced RTMP v1 video tag header. Returns bytes written, or 0 if
/// `buf` is too small, or on the legacy branch (see below). Mirrors
/// [`exvideo_parse`] in reverse.
///
/// `VideoHeader` has no field for a legacy (non-ex-header) video codec ID --
/// `exvideo_parse` never populates one either, since the legacy tag byte's
/// low nibble is a codec ID, not part of the ex-header layout this type
/// models. Writing `is_ex_header == 0` therefore cannot produce a valid
/// legacy tag byte (there is no codec ID to encode; a codec ID of 0 is not a
/// defined legacy video codec) and returns 0 instead. Callers that need a
/// legacy FLV video tag header should build one directly (see
/// `src/flv/video_tag.rs`) rather than through this enhanced-only writer.
pub fn exvideo_write(hdr: &VideoHeader, buf: &mut [u8]) -> usize {
    if hdr.is_ex_header == 0 {
        return 0;
    }

    let needs_ct = hdr.packet_type == 1 && is_composition_time_codec(&hdr.fourcc);
    let len = if needs_ct { 8 } else { 5 };
    if buf.len() < len {
        return 0;
    }

    buf[0] = 0x80 | ((hdr.frame_type & 0x07) << 4) | (hdr.packet_type & 0x0F);
    buf[1..5].copy_from_slice(&hdr.fourcc[..4]);

    if needs_ct {
        let ct = hdr.composition_time as i32;
        buf[5] = (ct >> 16) as u8;
        buf[6] = (ct >> 8) as u8;
        buf[7] = ct as u8;
    }

    len
}

/// Get the E-RTMP version string.
pub fn version_string() -> &'static str {
    "E-RTMP v1 (ExVideoTagHeader/FourCC)"
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::VideoHeader;

    #[test]
    fn parse_clears_composition_time_on_non_ct_packet_type_reuse() {
        let h264_ct = [0x91, b'a', b'v', b'c', b'1', 0x00, 0x01, 0x2C];
        let av1_seq = [0x90, b'a', b'v', b'0', b'1'];

        let mut hdr = VideoHeader::default();
        exvideo_parse(&h264_ct, &mut hdr).unwrap();
        assert_eq!(hdr.composition_time, 0x00012C);
        assert_eq!(hdr.header_size, 8);

        exvideo_parse(&av1_seq, &mut hdr).unwrap();
        assert_eq!(hdr.composition_time, 0);
        assert_eq!(hdr.header_size, 5);
        assert_eq!(&hdr.fourcc[..4], b"av01");
    }

    #[test]
    fn parse_error_leaves_header_cleared() {
        let h264_ct = [0x91, b'a', b'v', b'c', b'1', 0x00, 0x01, 0x2C];
        let mut hdr = VideoHeader::default();
        exvideo_parse(&h264_ct, &mut hdr).unwrap();

        assert!(exvideo_parse(&[0x91, b'a', b'v'], &mut hdr).is_err());
        assert_eq!(hdr.composition_time, 0);
        assert_eq!(hdr.is_ex_header, 1);
    }

    #[test]
    fn write_rejects_legacy_header() {
        // VideoHeader carries no legacy codec ID (exvideo_parse never
        // populates one either), so a legacy tag byte cannot be constructed
        // -- writing it would produce codec ID 0, which is not a valid
        // legacy video codec.
        let hdr = VideoHeader {
            is_ex_header: 0,
            frame_type: 1,
            ..Default::default()
        };
        let mut buf = [0u8; 8];
        assert_eq!(exvideo_write(&hdr, &mut buf), 0);
    }

    #[test]
    fn write_round_trips_enhanced_header_with_composition_time() {
        let mut hdr = VideoHeader {
            is_ex_header: 1,
            packet_type: 1,
            frame_type: 1,
            composition_time: 300,
            ..Default::default()
        };
        hdr.fourcc[..4].copy_from_slice(b"avc1");
        let mut buf = [0u8; 8];
        let n = exvideo_write(&hdr, &mut buf);
        assert_eq!(n, 8);

        let mut parsed = VideoHeader::default();
        exvideo_parse(&buf[..n], &mut parsed).unwrap();
        assert_eq!(parsed.composition_time, 300);
        assert_eq!(&parsed.fourcc[..4], b"avc1");
        assert_eq!(parsed.header_size, 8);
    }

    #[test]
    fn write_round_trips_enhanced_header_without_composition_time() {
        let mut hdr = VideoHeader {
            is_ex_header: 1,
            packet_type: 0,
            frame_type: 1,
            ..Default::default()
        };
        hdr.fourcc[..4].copy_from_slice(b"av01");
        let mut buf = [0u8; 8];
        let n = exvideo_write(&hdr, &mut buf);
        assert_eq!(n, 5);

        let mut parsed = VideoHeader::default();
        exvideo_parse(&buf[..n], &mut parsed).unwrap();
        assert_eq!(parsed.composition_time, 0);
        assert_eq!(parsed.header_size, 5);
        assert_eq!(&parsed.fourcc[..4], b"av01");
    }

    #[test]
    fn write_rejects_undersized_buffer() {
        let mut hdr = VideoHeader {
            is_ex_header: 1,
            packet_type: 1,
            frame_type: 1,
            ..Default::default()
        };
        hdr.fourcc[..4].copy_from_slice(b"avc1");
        let mut buf = [0u8; 4];
        assert_eq!(exvideo_write(&hdr, &mut buf), 0);
    }
}
