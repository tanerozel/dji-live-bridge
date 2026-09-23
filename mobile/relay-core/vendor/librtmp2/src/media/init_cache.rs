//! Init-frame classification and Frame population for the relay hot path.

use crate::ertmp::multitrack_media::{
    is_multitrack_container, multitrack_has_keyframe, multitrack_has_sequence_start,
};
use crate::ertmp::{exaudio, exvideo, fourcc};
use crate::types::{
    AudioCodec, AudioHeader, ERTMP_PACKET_TYPE_METADATA, FourCc, Frame, FrameType, VideoCodec,
    VideoHeader,
};

/// How a relayed media frame should be treated by the init-frame cache.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheFrameKind {
    LiveOnly,
    VideoSequenceHeader,
    AudioSequenceHeader,
    VideoKeyframe,
}

/// Classify a publisher media frame for init-cache storage.
pub fn classify_cache_frame(frame_type: FrameType, payload: &[u8]) -> CacheFrameKind {
    match frame_type {
        FrameType::Video => classify_video(payload),
        FrameType::Audio => classify_audio(payload),
        FrameType::Script | FrameType::Metadata => CacheFrameKind::LiveOnly,
    }
}

fn classify_video(payload: &[u8]) -> CacheFrameKind {
    if is_multitrack_container(FrameType::Video, payload) {
        if multitrack_has_sequence_start(FrameType::Video, payload) {
            return CacheFrameKind::VideoSequenceHeader;
        }
        if multitrack_has_keyframe(payload) {
            return CacheFrameKind::VideoKeyframe;
        }
        return CacheFrameKind::LiveOnly;
    }
    let mut hdr = VideoHeader::default();
    if exvideo::exvideo_parse(payload, &mut hdr).is_err() {
        return CacheFrameKind::LiveOnly;
    }
    if hdr.is_ex_header != 0 {
        return match hdr.packet_type {
            0 => CacheFrameKind::VideoSequenceHeader,
            1 | 3 if hdr.frame_type == 1 => CacheFrameKind::VideoKeyframe,
            _ => CacheFrameKind::LiveOnly,
        };
    }
    let codec_id = payload[0] & 0x0F;
    let frame_type_nibble = (payload[0] >> 4) & 0x0F;
    if codec_id == 7 && payload.len() >= 2 && payload[1] == 0 {
        return CacheFrameKind::VideoSequenceHeader;
    }
    // Legacy AVC: nibble 1 is "keyframe" for sequence-end (AVCPacketType 2)
    // as well as coded IDR (type 1). Only NALU packets are cacheable IDRs.
    if frame_type_nibble == 1 {
        if codec_id == 7 {
            if payload.len() >= 2 && payload[1] == 1 {
                return CacheFrameKind::VideoKeyframe;
            }
            return CacheFrameKind::LiveOnly;
        }
        return CacheFrameKind::VideoKeyframe;
    }
    CacheFrameKind::LiveOnly
}

fn classify_audio(payload: &[u8]) -> CacheFrameKind {
    if is_multitrack_container(FrameType::Audio, payload)
        && multitrack_has_sequence_start(FrameType::Audio, payload)
    {
        return CacheFrameKind::AudioSequenceHeader;
    }
    let mut hdr = AudioHeader::default();
    if exaudio::exaudio_parse(payload, &mut hdr).is_err() {
        return CacheFrameKind::LiveOnly;
    }
    if hdr.is_ex_header != 0 {
        if hdr.packet_type == 0 {
            CacheFrameKind::AudioSequenceHeader
        } else {
            CacheFrameKind::LiveOnly
        }
    } else if hdr.audio_codec == AudioCodec::Aac && hdr.aac_packet_type == 0 {
        CacheFrameKind::AudioSequenceHeader
    } else {
        CacheFrameKind::LiveOnly
    }
}

/// Fill codec/header fields on a [`Frame`] from raw RTMP A/V payload bytes.
/// Fill codec/header fields for a per-track callback extracted from an E-RTMP
/// multitrack container. Inner track payloads do not include the enhanced tag
/// header or FourCC, so metadata comes from the track descriptor.
pub fn populate_multitrack_frame(
    frame: &mut Frame,
    fourcc_value: [u8; 4],
    packet_type: u8,
    video_frame_type: u8,
) {
    let mut cc = [0u8; 5];
    cc[..4].copy_from_slice(&fourcc_value);
    match frame.frame_type {
        FrameType::Video => {
            frame.video_fourcc = FourCc { cc };
            frame.video_frame_type = video_frame_type;
            if let Ok(codec) = fourcc::fourcc_to_video_codec(&fourcc_value) {
                frame.video_codec = codec;
            }
            frame.is_metadata = u8::from(packet_type == crate::types::ERTMP_PACKET_TYPE_METADATA);
        }
        FrameType::Audio => {
            frame.audio_fourcc = FourCc { cc };
            if let Ok(codec) = fourcc::fourcc_to_audio_codec(&fourcc_value) {
                frame.audio_codec = codec;
            }
            frame.is_metadata =
                u8::from(packet_type == crate::types::ERTMP_AUDIO_PACKET_TYPE_METADATA);
        }
        FrameType::Script | FrameType::Metadata => {
            frame.is_metadata = 1;
        }
    }
}
pub fn populate_av_frame(frame: &mut Frame, payload: &[u8]) {
    match frame.frame_type {
        FrameType::Video => populate_video_frame(frame, payload),
        FrameType::Audio => populate_audio_frame(frame, payload),
        FrameType::Script | FrameType::Metadata => {
            frame.is_metadata = 1;
        }
    }
}

fn populate_video_frame(frame: &mut Frame, payload: &[u8]) {
    let mut hdr = VideoHeader::default();
    if exvideo::exvideo_parse(payload, &mut hdr).is_err() {
        return;
    }
    frame.composition_time = hdr.composition_time;
    frame.video_frame_type = hdr.frame_type;
    frame.video_fourcc = FourCc { cc: hdr.fourcc };
    frame.video_codec = if hdr.is_ex_header != 0 {
        fourcc::fourcc_to_video_codec(&hdr.fourcc).unwrap_or(VideoCodec::H264)
    } else {
        legacy_video_codec(payload[0] & 0x0F)
    };

    if hdr.is_ex_header != 0 && hdr.packet_type == ERTMP_PACKET_TYPE_METADATA {
        frame.is_metadata = 1;
    }
}

/// Parse `colorInfo` HDR metadata from a raw video payload, if it carries an
/// enhanced (ex-header) Metadata packet type (`ERTMP_PACKET_TYPE_METADATA`).
/// Returns `None` for any other packet type or on a parse error -- this is
/// opportunistic, not part of `Frame` (see `docs/abi-policy.md`: `Frame`'s
/// `#[repr(C)]` layout is ABI-stable across minor/patch releases).
///
/// Per the Enhanced RTMP v1 spec, a Metadata packet's body is an AMF0-encoded
/// value, not a raw byte tuple: a `colorInfo` object containing a nested
/// `colorConfig` object with numeric `colorPrimaries` / `transferCharacteristics`
/// / `matrixCoefficients` fields. Both a top-level `colorConfig` object and one
/// nested under `colorInfo` are accepted, since the exact wrapping has not been
/// verified against a real encoder (see "Known Limitations" in
/// `docs/protocol-mapping-ertmp-v1.md`).
pub fn parse_video_metadata_hdr(payload: &[u8]) -> Option<crate::types::HdrInfo> {
    let mut hdr = VideoHeader::default();
    exvideo::exvideo_parse(payload, &mut hdr).ok()?;
    if hdr.is_ex_header == 0 || hdr.packet_type != ERTMP_PACKET_TYPE_METADATA {
        return None;
    }
    parse_color_info_amf(&payload[hdr.header_size..])
}

/// Scan an AMF0-encoded metadata value for `colorPrimaries` /
/// `transferCharacteristics` / `matrixCoefficients` numeric fields, either
/// directly on the top-level object (a bare `colorConfig`-shaped value) or
/// nested up to two levels down under `colorConfig` / `colorInfo` keys (e.g.
/// `{ colorInfo: { colorConfig: {...} } }`). Bounds-checked throughout;
/// never trusts the AMF object-key count.
fn parse_color_info_amf(data: &[u8]) -> Option<crate::types::HdrInfo> {
    use crate::amf::amf0::{self, Amf0Type};

    let mut buf = crate::buffer::Buffer::from_slice(data);
    if amf0::read_type(&mut buf).ok()? != Amf0Type::Object {
        return None;
    }
    scan_object_for_color_info(&mut buf, 2)
}

/// Reads an AMF0 object body (marker already consumed). Returns `Some` when
/// this object's own keys directly carry `colorPrimaries` /
/// `transferCharacteristics` / `matrixCoefficients`; otherwise falls back to
/// whatever a nested `colorConfig` / `colorInfo` object (bounded by
/// `depth_remaining`) yields.
fn scan_object_for_color_info(
    buf: &mut crate::buffer::Buffer,
    depth_remaining: u8,
) -> Option<crate::types::HdrInfo> {
    use crate::amf::amf0::{self, Amf0Type};
    use crate::ertmp::metadata::hdr_init;

    let mut hdr = crate::types::HdrInfo::default();
    hdr_init(&mut hdr);
    let mut found_direct = false;
    let mut nested_result = None;
    let mut keys = 0usize;
    while !amf0::is_object_end(buf) {
        keys += 1;
        if keys > amf0::MAX_OBJECT_KEYS {
            return None;
        }
        let mut key = [0u8; 64];
        let key_len = amf0::read_object_key(buf, &mut key).ok()?;
        let key_str = std::str::from_utf8(&key[..key_len]).unwrap_or("");
        let ty = amf0::read_type(buf).ok()?;

        if ty == Amf0Type::Number
            && matches!(
                key_str,
                "colorPrimaries" | "transferCharacteristics" | "matrixCoefficients"
            )
        {
            let value = amf0::read_number(buf).ok()?;
            if !value.is_finite() || !(0.0..=u16::MAX as f64).contains(&value) {
                return None;
            }
            found_direct = true;
            match key_str {
                "colorPrimaries" => hdr.color_primaries = value as u16,
                "transferCharacteristics" => hdr.transfer_chars = value as u16,
                "matrixCoefficients" => hdr.matrix_coeffs = value as u16,
                _ => unreachable!(),
            }
        } else if ty == Amf0Type::Object
            && depth_remaining > 0
            && matches!(key_str, "colorConfig" | "colorInfo")
        {
            if let Some(inner) = scan_object_for_color_info(buf, depth_remaining - 1) {
                nested_result = Some(inner);
            }
        } else {
            amf0::skip_value_after_type(buf, ty).ok()?;
        }
    }
    // Consume the trailing 0x00 0x00 0x09 object-end marker.
    let mut end = [0u8; 3];
    buf.read(&mut end).ok()?;

    if found_direct {
        Some(hdr)
    } else {
        nested_result
    }
}

fn populate_audio_frame(frame: &mut Frame, payload: &[u8]) {
    let mut hdr = AudioHeader::default();
    if exaudio::exaudio_parse(payload, &mut hdr).is_err() {
        return;
    }
    frame.audio_codec = hdr.audio_codec;
    frame.audio_sample_rate = hdr.sample_rate as u32;
    frame.audio_channels = hdr.channels;
    frame.audio_bit_depth = if hdr.sample_size != 0 { 16 } else { 8 };
    frame.audio_fourcc = FourCc { cc: hdr.fourcc };
}

fn legacy_video_codec(codec_id: u8) -> VideoCodec {
    match codec_id {
        7 => VideoCodec::H264,
        12 => VideoCodec::H265,
        13 => VideoCodec::Av1,
        14 => VideoCodec::Vp9,
        _ => VideoCodec::H264,
    }
}

/// Returns true when `payload` is an AMF0 onMetaData (or @setDataFrame/onMetaData) message.
pub fn is_on_metadata_payload(payload: &[u8]) -> bool {
    let mut buf = crate::buffer::Buffer::from_slice(payload);
    let first_byte = match buf.peek().first().copied() {
        Some(b) => b,
        None => return false,
    };
    if first_byte != crate::amf::amf0::Amf0Type::String as u8
        && first_byte != crate::amf::amf0::Amf0Type::LongString as u8
    {
        return false;
    }

    let mut name = [0u8; 64];
    let name_len = match read_data_event_name(
        &mut buf,
        first_byte == crate::amf::amf0::Amf0Type::String as u8,
        &mut name,
    ) {
        Some(n) => n,
        None => return false,
    };
    let name_str = std::str::from_utf8(&name[..name_len]).unwrap_or("");

    if name_str == "@setDataFrame" {
        let next_byte = match buf.peek().first().copied() {
            Some(b) => b,
            None => return false,
        };
        if next_byte != crate::amf::amf0::Amf0Type::String as u8
            && next_byte != crate::amf::amf0::Amf0Type::LongString as u8
        {
            return false;
        }
        let mut inner = [0u8; 64];
        let inner_len = match read_data_event_name(
            &mut buf,
            next_byte == crate::amf::amf0::Amf0Type::String as u8,
            &mut inner,
        ) {
            Some(n) => n,
            None => return false,
        };
        let inner_str = std::str::from_utf8(&inner[..inner_len]).unwrap_or("");
        inner_str == "onMetaData"
    } else {
        name_str == "onMetaData"
    }
}

fn read_data_event_name(
    buf: &mut crate::buffer::Buffer,
    is_string: bool,
    out: &mut [u8; 64],
) -> Option<usize> {
    match if is_string {
        crate::amf::amf0::read_string(buf, out)
    } else {
        crate::amf::amf0::read_long_string(buf, out)
    } {
        Ok(n) => Some(n),
        Err(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multitrack_video_callback_metadata_uses_track_descriptor() {
        let mut frame = Frame {
            frame_type: FrameType::Video,
            ..Default::default()
        };
        populate_multitrack_frame(&mut frame, *b"hvc1", 1, 1);
        assert_eq!(&frame.video_fourcc.cc[..4], b"hvc1");
        assert_eq!(frame.video_codec, VideoCodec::H265);
        assert_eq!(frame.video_frame_type, 1);
    }

    #[test]
    fn multitrack_audio_callback_metadata_uses_track_descriptor() {
        let mut frame = Frame {
            frame_type: FrameType::Audio,
            ..Default::default()
        };
        populate_multitrack_frame(&mut frame, *b"Opus", 1, 0);
        assert_eq!(&frame.audio_fourcc.cc[..4], b"Opus");
        assert_eq!(frame.audio_codec, AudioCodec::Opus);
    }
    #[test]
    fn enhanced_video_metadata_packet_marks_is_metadata() {
        let payload = [
            0x94, b'h', b'v', b'c', b'1', // ex-header, frame_type=1, packet_type=4 (Metadata)
            0x00, 0x01, 0x00, 0x02, 0x00, 0x03, // colorInfo: primaries/transfer/matrix
        ];
        let mut frame = Frame {
            frame_type: FrameType::Video,
            ..Default::default()
        };
        populate_av_frame(&mut frame, &payload);
        assert_eq!(frame.is_metadata, 1);
    }

    #[test]
    fn enhanced_video_non_metadata_packet_does_not_mark_is_metadata() {
        let payload = [0x90, b'a', b'v', b'0', b'1'];
        let mut frame = Frame {
            frame_type: FrameType::Video,
            ..Default::default()
        };
        populate_av_frame(&mut frame, &payload);
        assert_eq!(frame.is_metadata, 0);
    }

    fn amf0_color_config(primaries: f64, transfer: f64, matrix: f64) -> Vec<u8> {
        use crate::amf::amf0;
        let mut buf = crate::buffer::Buffer::new();
        amf0::write_object_begin(&mut buf).unwrap();
        amf0::write_object_key(&mut buf, "colorPrimaries").unwrap();
        amf0::write_number(&mut buf, primaries).unwrap();
        amf0::write_object_key(&mut buf, "transferCharacteristics").unwrap();
        amf0::write_number(&mut buf, transfer).unwrap();
        amf0::write_object_key(&mut buf, "matrixCoefficients").unwrap();
        amf0::write_number(&mut buf, matrix).unwrap();
        amf0::write_object_end(&mut buf).unwrap();
        buf.as_slice().to_vec()
    }

    #[test]
    fn parse_video_metadata_hdr_extracts_nested_color_info_color_config() {
        use crate::amf::amf0;
        let mut buf = crate::buffer::Buffer::new();
        amf0::write_object_begin(&mut buf).unwrap();
        amf0::write_object_key(&mut buf, "colorInfo").unwrap();
        buf.write(&amf0_color_config(1.0, 2.0, 3.0)).unwrap();
        amf0::write_object_end(&mut buf).unwrap();

        let mut payload = vec![0x94, b'h', b'v', b'c', b'1'];
        payload.extend_from_slice(buf.as_slice());

        let hdr = parse_video_metadata_hdr(&payload).unwrap();
        assert_eq!(hdr.color_primaries, 1);
        assert_eq!(hdr.transfer_chars, 2);
        assert_eq!(hdr.matrix_coeffs, 3);
    }

    #[test]
    fn parse_video_metadata_hdr_extracts_top_level_color_config() {
        let mut payload = vec![0x94, b'h', b'v', b'c', b'1'];
        payload.extend_from_slice(&amf0_color_config(4.0, 5.0, 6.0));

        let hdr = parse_video_metadata_hdr(&payload).unwrap();
        assert_eq!(hdr.color_primaries, 4);
        assert_eq!(hdr.transfer_chars, 5);
        assert_eq!(hdr.matrix_coeffs, 6);
    }

    #[test]
    fn parse_video_metadata_hdr_rejects_short_payload() {
        let payload = [0x94, b'h', b'v', b'c', b'1', 0x00, 0x01];
        assert!(parse_video_metadata_hdr(&payload).is_none());
    }

    #[test]
    fn parse_video_metadata_hdr_returns_none_for_non_metadata_packet() {
        let payload = [0x90, b'a', b'v', b'0', b'1'];
        assert!(parse_video_metadata_hdr(&payload).is_none());
    }

    #[test]
    fn parse_video_metadata_hdr_returns_none_when_no_color_config_present() {
        use crate::amf::amf0;
        let mut buf = crate::buffer::Buffer::new();
        amf0::write_object_begin(&mut buf).unwrap();
        amf0::write_object_key(&mut buf, "someOtherKey").unwrap();
        amf0::write_number(&mut buf, 42.0).unwrap();
        amf0::write_object_end(&mut buf).unwrap();

        let mut payload = vec![0x94, b'h', b'v', b'c', b'1'];
        payload.extend_from_slice(buf.as_slice());
        assert!(parse_video_metadata_hdr(&payload).is_none());
    }

    #[test]
    fn enhanced_hevc_sequence_start_is_cached() {
        let payload = [0x90, b'h', b'v', b'c', b'1', 0x01, 0x02];
        assert_eq!(
            classify_cache_frame(FrameType::Video, &payload),
            CacheFrameKind::VideoSequenceHeader
        );
    }

    #[test]
    fn enhanced_av1_keyframe_is_cached() {
        let payload = [0x91, b'a', b'v', b'0', b'1', 0xDE, 0xAD];
        assert_eq!(
            classify_cache_frame(FrameType::Video, &payload),
            CacheFrameKind::VideoKeyframe
        );
    }

    #[test]
    fn enhanced_coded_frames_x_keyframe_is_cached() {
        let payload = [0x93, b'a', b'v', b'c', b'1', 0xDE, 0xAD];
        assert_eq!(
            classify_cache_frame(FrameType::Video, &payload),
            CacheFrameKind::VideoKeyframe
        );
    }

    #[test]
    fn enhanced_opus_sequence_start_is_cached() {
        let payload = [0x90, b'O', b'p', b'u', b's'];
        assert_eq!(
            classify_cache_frame(FrameType::Audio, &payload),
            CacheFrameKind::AudioSequenceHeader
        );
    }

    #[test]
    fn multitrack_video_sequence_start_is_cached() {
        let payload = vec![
            0x86, 0x10, b'a', b'v', b'c', b'1', 0x00, 0x00, 0x00, 0x03, 0xAA, 0xBB, 0xCC,
        ];
        assert_eq!(
            classify_cache_frame(FrameType::Video, &payload),
            CacheFrameKind::VideoSequenceHeader
        );
    }

    #[test]
    fn multitrack_video_keyframe_is_cached_from_outer_header() {
        let payload = vec![
            0x96, 0x11, b'a', b'v', b'c', b'1', 0x00, 0x00, 0x00, 0x02, 0xDE, 0xAD,
        ];
        assert_eq!(
            classify_cache_frame(FrameType::Video, &payload),
            CacheFrameKind::VideoKeyframe
        );
    }

    #[test]
    fn legacy_avc_header_still_cached() {
        let payload = [0x17, 0x00, 0x01];
        assert_eq!(
            classify_cache_frame(FrameType::Video, &payload),
            CacheFrameKind::VideoSequenceHeader
        );
    }
}
