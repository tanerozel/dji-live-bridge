//! Enhanced RTMP v1 ExAudioTagHeader parsing
//!
//! Mirrors `src/ertmp/exaudio.c`.

use super::fourcc;
use crate::types::{AudioHeader, ErrorCode, Result};

/// Parse an Enhanced RTMP v1 audio tag header.
pub fn exaudio_parse(data: &[u8], hdr: &mut AudioHeader) -> Result<()> {
    *hdr = AudioHeader::default();

    if data.is_empty() {
        return Err(ErrorCode::Io);
    }

    let b0 = data[0];

    // Disambiguate legacy SoundFormat from IsExHeader
    let is_ex =
        (b0 & 0x80 != 0) && data.len() >= 5 && fourcc::fourcc_to_audio_codec(&data[1..5]).is_ok();

    hdr.is_ex_header = if is_ex { 1 } else { 0 };

    if hdr.is_ex_header == 0 {
        // Legacy layout
        hdr.audio_codec = match (b0 >> 4) & 0x0F {
            0 => crate::types::AudioCodec::Pcm,
            1 => crate::types::AudioCodec::Adpcm,
            2 => crate::types::AudioCodec::Mp3,
            3 => crate::types::AudioCodec::PcmLe,
            4 => crate::types::AudioCodec::Nelly16k,
            5 => crate::types::AudioCodec::Nelly8k,
            6 => crate::types::AudioCodec::Nelly,
            7 => crate::types::AudioCodec::G711A,
            8 => crate::types::AudioCodec::G711U,
            10 => crate::types::AudioCodec::Aac,
            11 => crate::types::AudioCodec::Speex,
            14 => crate::types::AudioCodec::Opus,
            _ => crate::types::AudioCodec::Aac,
        };
        hdr.sample_rate = (b0 >> 2) & 0x03;
        hdr.sample_size = (b0 >> 1) & 0x01;
        hdr.channels = b0 & 0x01;
        hdr.header_size = 1;

        if hdr.audio_codec == crate::types::AudioCodec::Aac && data.len() >= 2 {
            hdr.aac_packet_type = data[1];
            hdr.header_size = 2;
        }
        return Ok(());
    }

    // Enhanced layout
    hdr.packet_type = b0 & 0x0F;
    hdr.fourcc[..4].copy_from_slice(&data[1..5]);
    hdr.header_size = 5;
    hdr.audio_codec =
        fourcc::fourcc_to_audio_codec(&data[1..5]).unwrap_or(crate::types::AudioCodec::Aac);

    Ok(())
}

/// Write an Enhanced RTMP v1 audio tag header. Returns bytes written, or 0 if
/// `buf` is too small. Mirrors [`exaudio_parse`] in reverse.
pub fn exaudio_write(hdr: &AudioHeader, buf: &mut [u8]) -> usize {
    if hdr.is_ex_header == 0 {
        let codec_nibble = match hdr.audio_codec {
            crate::types::AudioCodec::Pcm => 0,
            crate::types::AudioCodec::Adpcm => 1,
            crate::types::AudioCodec::Mp3 => 2,
            crate::types::AudioCodec::PcmLe => 3,
            crate::types::AudioCodec::Nelly16k => 4,
            crate::types::AudioCodec::Nelly8k => 5,
            crate::types::AudioCodec::Nelly => 6,
            crate::types::AudioCodec::G711A => 7,
            crate::types::AudioCodec::G711U => 8,
            crate::types::AudioCodec::Aac => 10,
            crate::types::AudioCodec::Speex => 11,
            crate::types::AudioCodec::Opus => 14,
        };
        let b0 = (codec_nibble << 4)
            | ((hdr.sample_rate & 0x03) << 2)
            | ((hdr.sample_size & 0x01) << 1)
            | (hdr.channels & 0x01);

        if hdr.audio_codec == crate::types::AudioCodec::Aac {
            if buf.len() < 2 {
                return 0;
            }
            buf[0] = b0;
            buf[1] = hdr.aac_packet_type;
            return 2;
        }
        if buf.is_empty() {
            return 0;
        }
        buf[0] = b0;
        return 1;
    }

    if buf.len() < 5 {
        return 0;
    }
    buf[0] = 0x80 | (hdr.packet_type & 0x0F);
    buf[1..5].copy_from_slice(&hdr.fourcc[..4]);
    5
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{AudioCodec, AudioHeader};

    #[test]
    fn parse_clears_aac_packet_type_on_enhanced_reuse() {
        let legacy_aac = [0xAF, 0x01];
        let enhanced_opus = [0x90, b'O', b'p', b'u', b's'];

        let mut hdr = AudioHeader::default();
        exaudio_parse(&legacy_aac, &mut hdr).unwrap();
        assert_eq!(hdr.is_ex_header, 0);
        assert_eq!(hdr.aac_packet_type, 1);

        exaudio_parse(&enhanced_opus, &mut hdr).unwrap();
        assert_eq!(hdr.is_ex_header, 1);
        assert_eq!(hdr.aac_packet_type, 0);
        assert_eq!(hdr.audio_codec, AudioCodec::Opus);
    }

    #[test]
    fn parse_ec3_enhanced_header() {
        let enhanced_ec3 = [0x90, b'e', b'c', b'-', b'3', 0x00];
        let mut hdr = AudioHeader::default();
        exaudio_parse(&enhanced_ec3, &mut hdr).unwrap();
        assert_eq!(hdr.is_ex_header, 1);
        assert_eq!(hdr.audio_codec, AudioCodec::Aac);
        assert_eq!(&hdr.fourcc[..4], b"ec-3");
    }

    #[test]
    fn parse_error_leaves_header_cleared() {
        let legacy_aac = [0xAF, 0x01];
        let mut hdr = AudioHeader::default();
        exaudio_parse(&legacy_aac, &mut hdr).unwrap();

        assert!(exaudio_parse(&[], &mut hdr).is_err());
        assert_eq!(hdr.aac_packet_type, 0);
        assert_eq!(hdr.is_ex_header, 0);
    }

    #[test]
    fn write_round_trips_legacy_aac_header() {
        let hdr = AudioHeader {
            is_ex_header: 0,
            audio_codec: AudioCodec::Aac,
            sample_rate: 3,
            sample_size: 1,
            channels: 1,
            aac_packet_type: 1,
            ..Default::default()
        };
        let mut buf = [0u8; 5];
        let n = exaudio_write(&hdr, &mut buf);
        assert_eq!(n, 2);

        let mut parsed = AudioHeader::default();
        exaudio_parse(&buf[..n], &mut parsed).unwrap();
        assert_eq!(parsed.audio_codec, AudioCodec::Aac);
        assert_eq!(parsed.aac_packet_type, 1);
        assert_eq!(parsed.is_ex_header, 0);
    }

    #[test]
    fn write_round_trips_legacy_non_aac_header() {
        let hdr = AudioHeader {
            is_ex_header: 0,
            audio_codec: AudioCodec::Mp3,
            sample_rate: 2,
            sample_size: 1,
            channels: 0,
            ..Default::default()
        };
        let mut buf = [0u8; 5];
        let n = exaudio_write(&hdr, &mut buf);
        assert_eq!(n, 1);

        let mut parsed = AudioHeader::default();
        exaudio_parse(&buf[..n], &mut parsed).unwrap();
        assert_eq!(parsed.audio_codec, AudioCodec::Mp3);
        assert_eq!(parsed.sample_rate, 2);
        assert_eq!(parsed.channels, 0);
    }

    #[test]
    fn write_round_trips_enhanced_header() {
        let mut hdr = AudioHeader {
            is_ex_header: 1,
            packet_type: 0,
            ..Default::default()
        };
        hdr.fourcc[..4].copy_from_slice(b"Opus");
        let mut buf = [0u8; 5];
        let n = exaudio_write(&hdr, &mut buf);
        assert_eq!(n, 5);

        let mut parsed = AudioHeader::default();
        exaudio_parse(&buf[..n], &mut parsed).unwrap();
        assert_eq!(parsed.is_ex_header, 1);
        assert_eq!(parsed.audio_codec, AudioCodec::Opus);
        assert_eq!(&parsed.fourcc[..4], b"Opus");
    }

    #[test]
    fn write_rejects_undersized_buffer() {
        let hdr = AudioHeader {
            is_ex_header: 0,
            audio_codec: AudioCodec::Aac,
            ..Default::default()
        };
        let mut buf = [0u8; 1];
        assert_eq!(exaudio_write(&hdr, &mut buf), 0);
    }
}
