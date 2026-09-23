//! E-RTMP v2 multitrack audio/video message parsing for the session hot path.
//!
//! Multitrack containers are relayed opaque; this module extracts per-track
//! slices and codec metadata for callbacks, authorization, and init caching.

use crate::types::FrameType;

pub const ERTMP_AUDIO_PACKET_TYPE_MULTITRACK: u8 = 5;
pub const ERTMP_VIDEO_PACKET_TYPE_MULTITRACK: u8 = 6;
/// Cap sub-tracks unpacked from a single multitrack container (mirrors
/// `message::message::MAX_AGGREGATE_SUBTAGS` and `session::conn::MAX_AGGREGATE_SUBTAGS`).
pub const MAX_MULTITRACK_SUBTRACKS: usize = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum AvMultitrackType {
    OneTrack = 0,
    ManyTracks = 1,
    ManyTracksManyCodecs = 2,
}

#[derive(Debug, Clone, Copy)]
pub struct MediaTrackSlice<'a> {
    pub track_id: u8,
    pub packet_type: u8,
    pub fourcc: [u8; 4],
    pub video_frame_type: u8,
    pub payload: &'a [u8],
}

fn read_u24(data: &[u8]) -> usize {
    ((data[0] as usize) << 16) | ((data[1] as usize) << 8) | data[2] as usize
}

pub fn is_multitrack_container(frame_type: FrameType, payload: &[u8]) -> bool {
    if payload.is_empty() || payload[0] & 0x80 == 0 {
        return false;
    }
    let expected = match frame_type {
        FrameType::Video => ERTMP_VIDEO_PACKET_TYPE_MULTITRACK,
        FrameType::Audio => ERTMP_AUDIO_PACKET_TYPE_MULTITRACK,
        _ => return false,
    };
    (payload[0] & 0x0F) == expected
}

pub fn foreach_track(
    frame_type: FrameType,
    payload: &[u8],
    mut visit: impl FnMut(&MediaTrackSlice<'_>),
) -> bool {
    if !is_multitrack_container(frame_type, payload) || payload.len() < 3 {
        return false;
    }

    let multitrack_type = (payload[1] >> 4) & 0x0F;
    let inner_packet_type = payload[1] & 0x0F;
    if multitrack_type > AvMultitrackType::ManyTracksManyCodecs as u8 {
        return false;
    }

    let video_frame_type = if frame_type == FrameType::Video {
        (payload[0] >> 4) & 0x07
    } else {
        0
    };
    let mut pos = 2usize;
    let mut shared_fourcc = [0u8; 4];

    if multitrack_type != AvMultitrackType::ManyTracksManyCodecs as u8 {
        if pos + 4 > payload.len() {
            return false;
        }
        shared_fourcc.copy_from_slice(&payload[pos..pos + 4]);
        pos += 4;
    }

    let mut tracks = Vec::new();
    loop {
        let fourcc = if multitrack_type == AvMultitrackType::ManyTracksManyCodecs as u8 {
            if pos + 4 > payload.len() {
                return false;
            }
            let mut cc = [0u8; 4];
            cc.copy_from_slice(&payload[pos..pos + 4]);
            pos += 4;
            cc
        } else {
            shared_fourcc
        };

        if pos >= payload.len() {
            return false;
        }
        let track_id = payload[pos];
        pos += 1;

        let track_size = if multitrack_type != AvMultitrackType::OneTrack as u8 {
            if pos + 3 > payload.len() {
                return false;
            }
            let size = read_u24(&payload[pos..pos + 3]);
            pos += 3;
            size
        } else {
            payload.len() - pos
        };

        // ManyTracks / ManyTracksManyCodecs allow up to 4096 sub-tracks in a
        // single ~16 KiB message. Zero-length sub-tracks pack the maximum
        // track count into the smallest wire footprint and force per-track
        // authorization and on_frame callbacks with no payload work to
        // amortize the cost — reject them as malformed.
        if multitrack_type != AvMultitrackType::OneTrack as u8 && track_size == 0 {
            return false;
        }

        if pos + track_size > payload.len() {
            return false;
        }
        if tracks.len() >= MAX_MULTITRACK_SUBTRACKS {
            return false;
        }
        tracks.push(MediaTrackSlice {
            track_id,
            packet_type: inner_packet_type,
            fourcc,
            video_frame_type,
            payload: &payload[pos..pos + track_size],
        });
        pos += track_size;

        if multitrack_type == AvMultitrackType::OneTrack as u8 {
            break;
        }
        if pos == payload.len() {
            break;
        }
    }

    if pos != payload.len() || tracks.is_empty() {
        return false;
    }
    for track in &tracks {
        visit(track);
    }
    true
}

pub fn first_track_fourcc(frame_type: FrameType, payload: &[u8]) -> Option<[u8; 4]> {
    let mut result = None;
    if foreach_track(frame_type, payload, |track| {
        if result.is_none() {
            result = Some(track.fourcc);
        }
    }) {
        result
    } else {
        None
    }
}

pub fn multitrack_has_sequence_start(frame_type: FrameType, payload: &[u8]) -> bool {
    let mut found = false;
    let valid = foreach_track(frame_type, payload, |track| {
        found |= track.packet_type == 0;
    });
    valid && found
}

pub fn multitrack_has_keyframe(payload: &[u8]) -> bool {
    if !is_multitrack_container(FrameType::Video, payload) {
        return false;
    }
    let packet_type = payload.get(1).map(|b| b & 0x0F);
    let coded = matches!(packet_type, Some(1 | 3));
    coded && ((payload[0] >> 4) & 0x07) == 1 && foreach_track(FrameType::Video, payload, |_| {})
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_many_tracks_video_message() -> Vec<u8> {
        vec![
            0x86, 0x10, b'a', b'v', b'c', b'1', 0x00, 0x00, 0x00, 0x03, 0xAA, 0xBB, 0xCC, 0x01,
            0x00, 0x00, 0x02, 0xDD, 0xEE,
        ]
    }

    #[test]
    fn detects_multitrack_container() {
        let payload = build_many_tracks_video_message();
        assert!(is_multitrack_container(FrameType::Video, &payload));
    }

    #[test]
    fn iterates_subtracks_with_shared_codec() {
        let payload = build_many_tracks_video_message();
        let mut seen = Vec::new();
        assert!(foreach_track(FrameType::Video, &payload, |track| {
            seen.push((track.track_id, track.fourcc, track.payload.to_vec()));
        }));
        assert_eq!(seen[0], (0, *b"avc1", vec![0xAA, 0xBB, 0xCC]));
        assert_eq!(seen[1], (1, *b"avc1", vec![0xDD, 0xEE]));
    }

    #[test]
    fn iterates_many_tracks_many_codecs() {
        let payload = vec![
            0x86, 0x20, b'a', b'v', b'c', b'1', 0, 0, 0, 1, 0xAA, b'h', b'v', b'c', b'1', 1, 0, 0,
            1, 0xBB,
        ];
        let mut codecs = Vec::new();
        assert!(foreach_track(FrameType::Video, &payload, |track| {
            codecs.push(track.fourcc);
        }));
        assert_eq!(codecs, vec![*b"avc1", *b"hvc1"]);
    }

    #[test]
    fn malformed_message_delivers_no_partial_tracks() {
        let mut payload = build_many_tracks_video_message();
        payload.pop();
        let mut calls = 0;
        assert!(!foreach_track(FrameType::Video, &payload, |_| calls += 1));
        assert_eq!(calls, 0);
    }

    #[test]
    fn coded_frames_x_keyframe_is_detected() {
        let payload = vec![0x96, 0x13, b'a', b'v', b'c', b'1', 0, 0, 0, 1, 0xAA];
        assert!(multitrack_has_keyframe(&payload));
    }

    fn build_many_tracks_min_payload_message(track_count: usize) -> Vec<u8> {
        let mut payload = vec![0x86, 0x10, b'a', b'v', b'c', b'1'];
        for id in 0..track_count {
            payload.push(id as u8);
            payload.extend_from_slice(&[0x00, 0x00, 0x01, 0xAA]);
        }
        payload
    }

    fn build_many_tracks_zero_payload_message(track_count: usize) -> Vec<u8> {
        let mut payload = vec![0x86, 0x10, b'a', b'v', b'c', b'1'];
        for id in 0..track_count {
            payload.push(id as u8);
            payload.extend_from_slice(&[0x00, 0x00, 0x00]);
        }
        payload
    }

    #[test]
    fn rejects_multitrack_messages_with_too_many_subtracks() {
        let at_limit = build_many_tracks_min_payload_message(MAX_MULTITRACK_SUBTRACKS);
        let mut calls = 0;
        assert!(foreach_track(FrameType::Video, &at_limit, |_| calls += 1));
        assert_eq!(calls, MAX_MULTITRACK_SUBTRACKS);

        let over_limit = build_many_tracks_min_payload_message(MAX_MULTITRACK_SUBTRACKS + 1);
        let mut over_calls = 0;
        assert!(!foreach_track(FrameType::Video, &over_limit, |_| {
            over_calls += 1
        }));
        assert_eq!(over_calls, 0);
    }

    #[test]
    fn rejects_multitrack_messages_with_zero_size_subtracks() {
        let zero_payload = build_many_tracks_zero_payload_message(2);
        let mut calls = 0;
        assert!(!foreach_track(FrameType::Video, &zero_payload, |_| calls += 1));
        assert_eq!(calls, 0);
    }
}
