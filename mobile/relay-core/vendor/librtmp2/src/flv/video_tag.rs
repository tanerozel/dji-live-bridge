//! FLV video tag parser
//!
//! Mirrors `src/flv/video_tag.h` and `src/flv/video_tag.c`.

use crate::types::{ErrorCode, Result, VideoCodec, VideoTag};

/// Parse an FLV video tag.
///
/// Returns `Err(ErrorCode::Unsupported)` if `IsExHeader` (bit 7) is set —
/// the caller must route those frames through the E-RTMP enhanced parser instead.
pub fn parse(data: &[u8], tag: &mut VideoTag) -> Result<()> {
    *tag = VideoTag::default();

    if data.is_empty() {
        return Err(ErrorCode::Internal);
    }

    // Bit 7 set means E-RTMP v1 ExVideoTagHeader: lower nibble is PacketType,
    // not a legacy codec ID. Reject so the caller uses the correct path.
    if data[0] & 0x80 != 0 {
        return Err(ErrorCode::Unsupported);
    }

    tag.frame_type = (data[0] >> 4) & 0x0F;
    tag.codec = match data[0] & 0x0F {
        1 => VideoCodec::Jpeg,
        2 => VideoCodec::Sorenson,
        3 => VideoCodec::Screen,
        4 => VideoCodec::Vp6,
        5 => VideoCodec::Vp6a,
        6 => VideoCodec::Screen2,
        7 => VideoCodec::H264,
        _ => return Err(ErrorCode::Unsupported),
    };

    if data.len() >= 5 && tag.codec == VideoCodec::H264 {
        tag.avc_packet_type = data[1];
        tag.composition_time =
            ((data[2] as u32) << 16) | ((data[3] as u32) << 8) | (data[4] as u32);
    }

    tag.data = data.as_ptr();
    tag.size = data.len();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::VideoTag;

    #[test]
    fn parse_h264_keyframe_sets_codec_and_cts() {
        let payload = [0x17, 0x01, 0x00, 0x01, 0x2C, 0xDE, 0xAD];
        let mut tag = VideoTag::default();
        parse(&payload, &mut tag).unwrap();
        assert_eq!(tag.frame_type, 1);
        assert_eq!(tag.codec, VideoCodec::H264);
        assert_eq!(tag.avc_packet_type, 1);
        assert_eq!(tag.composition_time, 0x00012C);
    }

    #[test]
    fn parse_rejects_enhanced_header() {
        let payload = [0x80, 0x00, 0x00];
        let mut tag = VideoTag::default();
        assert_eq!(parse(&payload, &mut tag), Err(ErrorCode::Unsupported));
    }

    #[test]
    fn parse_rejects_empty_payload() {
        let mut tag = VideoTag::default();
        assert_eq!(parse(&[], &mut tag), Err(ErrorCode::Internal));
    }

    #[test]
    fn parse_rejects_unknown_codec_nibble() {
        let payload = [0x18, 0x00];
        let mut tag = VideoTag::default();
        assert_eq!(parse(&payload, &mut tag), Err(ErrorCode::Unsupported));
    }

    #[test]
    fn parse_rejects_internal_enum_nibbles_not_legacy_flv() {
        for nibble in [12u8, 13, 14] {
            let payload = [0x10 | nibble, 0x00];
            let mut tag = VideoTag::default();
            assert_eq!(parse(&payload, &mut tag), Err(ErrorCode::Unsupported));
        }
    }

    #[test]
    fn parse_clears_avc_fields_on_non_h264_reuse() {
        let mut tag = VideoTag::default();
        parse(&[0x17, 0x01, 0x00, 0x01, 0x2C], &mut tag).unwrap();
        assert_eq!(tag.avc_packet_type, 1);
        assert_eq!(tag.composition_time, 0x00012C);

        parse(&[0x24, 0xDE, 0xAD], &mut tag).unwrap();
        assert_eq!(tag.codec, VideoCodec::Vp6);
        assert_eq!(tag.avc_packet_type, 0);
        assert_eq!(tag.composition_time, 0);
    }

    #[test]
    fn parse_error_leaves_tag_cleared() {
        let mut tag = VideoTag::default();
        parse(&[0x17, 0x01, 0x00, 0x01, 0x2C], &mut tag).unwrap();
        assert_eq!(parse(&[0x18, 0x00], &mut tag), Err(ErrorCode::Unsupported));
        assert_eq!(tag.codec, VideoCodec::Jpeg);
        assert_eq!(tag.avc_packet_type, 0);
        assert_eq!(tag.composition_time, 0);
    }
}
