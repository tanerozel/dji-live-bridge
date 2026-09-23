//! FLV audio tag parser
//!
//! Mirrors `src/flv/audio_tag.h` and `src/flv/audio_tag.c`.

use crate::types::{AudioCodec, AudioTag, ErrorCode, Result};

/// Parse an FLV audio tag.
pub fn parse(data: &[u8], tag: &mut AudioTag) -> Result<()> {
    *tag = AudioTag::default();

    if data.is_empty() {
        return Err(ErrorCode::Internal);
    }

    tag.codec = match (data[0] >> 4) & 0x0F {
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
    tag.sample_rate = (data[0] >> 2) & 0x03;
    tag.bit_depth = (data[0] >> 1) & 0x01;
    tag.channels = data[0] & 0x01;

    if tag.codec == AudioCodec::Aac && data.len() >= 2 {
        tag.aac_packet_type = data[1];
    }

    tag.data = data.as_ptr();
    tag.size = data.len();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::AudioTag;

    #[test]
    fn parse_aac_sets_codec_and_packet_type() {
        let payload = [0xAF, 0x00];
        let mut tag = AudioTag::default();
        parse(&payload, &mut tag).unwrap();
        assert_eq!(tag.codec, AudioCodec::Aac);
        assert_eq!(tag.aac_packet_type, 0);
    }

    #[test]
    fn parse_mp3_codec_nibble() {
        let payload = [0x2F];
        let mut tag = AudioTag::default();
        parse(&payload, &mut tag).unwrap();
        assert_eq!(tag.codec, AudioCodec::Mp3);
    }

    #[test]
    fn parse_extracts_sample_rate_channels() {
        // 44100 Hz stereo AAC: rate bits 10, 16-bit, stereo
        let payload = [0xAF, 0x01];
        let mut tag = AudioTag::default();
        parse(&payload, &mut tag).unwrap();
        assert_eq!(tag.sample_rate, 3);
        assert_eq!(tag.bit_depth, 1);
        assert_eq!(tag.channels, 1);
    }

    #[test]
    fn parse_rejects_empty_payload() {
        let mut tag = AudioTag::default();
        assert_eq!(parse(&[], &mut tag), Err(ErrorCode::Internal));
    }

    #[test]
    fn parse_clears_aac_packet_type_on_non_aac_reuse() {
        let mut tag = AudioTag::default();
        parse(&[0xAF, 0x01], &mut tag).unwrap();
        assert_eq!(tag.aac_packet_type, 1);

        parse(&[0x2F], &mut tag).unwrap();
        assert_eq!(tag.codec, AudioCodec::Mp3);
        assert_eq!(tag.aac_packet_type, 0);
    }

    #[test]
    fn parse_error_leaves_tag_cleared() {
        let mut tag = AudioTag::default();
        parse(&[0xAF, 0x01], &mut tag).unwrap();
        assert_eq!(parse(&[], &mut tag), Err(ErrorCode::Internal));
        assert_eq!(tag.codec, AudioCodec::Pcm);
        assert_eq!(tag.aac_packet_type, 0);
    }
}
