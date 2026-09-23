//! FourCC codec registry and dispatch for Enhanced RTMP v1
//!
//! Mirrors `src/ertmp/fourcc.c`.

use crate::types::{AudioCodec, ErrorCode, Result, VideoCodec};

/* ── Video FourCC table ── */

struct VideoFourCcEntry {
    fourcc: [u8; 4],
    codec: VideoCodec,
    name: &'static str,
}

static VIDEO_FOURCCS: &[VideoFourCcEntry] = &[
    VideoFourCcEntry {
        fourcc: *b"avc1",
        codec: VideoCodec::H264,
        name: "H.264/AVC",
    },
    VideoFourCcEntry {
        fourcc: *b"hvc1",
        codec: VideoCodec::H265,
        name: "H.265/HEVC",
    },
    VideoFourCcEntry {
        fourcc: *b"av01",
        codec: VideoCodec::Av1,
        name: "AV1",
    },
    VideoFourCcEntry {
        fourcc: *b"vp09",
        codec: VideoCodec::Vp9,
        name: "VP9",
    },
];

/* ── Audio FourCC table ── */

struct AudioFourCcEntry {
    fourcc: [u8; 4],
    codec: AudioCodec,
    name: &'static str,
}

static AUDIO_FOURCCS: &[AudioFourCcEntry] = &[
    AudioFourCcEntry {
        fourcc: *b"Opus",
        codec: AudioCodec::Opus,
        name: "Opus",
    },
    AudioFourCcEntry {
        fourcc: *b"mp4a",
        codec: AudioCodec::Aac,
        name: "AAC",
    },
    AudioFourCcEntry {
        fourcc: *b"mp3 ",
        codec: AudioCodec::Mp3,
        name: "MP3",
    },
    AudioFourCcEntry {
        fourcc: *b".mp3",
        codec: AudioCodec::Mp3,
        name: "MP3 (E-RTMP)",
    },
    // AC-3 / E-AC-3 / FLAC have no dedicated AudioCodec variant (ABI-stable
    // enum); map to Aac so enhanced headers are recognized without falling
    // through to legacy G.711.
    AudioFourCcEntry {
        fourcc: *b"ac-3",
        codec: AudioCodec::Aac,
        name: "Dolby Digital",
    },
    AudioFourCcEntry {
        fourcc: *b"ec-3",
        codec: AudioCodec::Aac,
        name: "Dolby Digital Plus",
    },
    AudioFourCcEntry {
        fourcc: *b"fLaC",
        codec: AudioCodec::Aac,
        name: "FLAC",
    },
];

/// Convert a FourCC string to a video codec.
/// Returns Err for unknown or too-short FourCCs so callers can use is_ok()
/// as a reliable enhanced-header discriminator.
pub fn fourcc_to_video_codec(fourcc: &[u8]) -> Result<VideoCodec> {
    if fourcc.len() < 4 {
        return Err(ErrorCode::Chunk);
    }
    for entry in VIDEO_FOURCCS {
        if fourcc[..4] == entry.fourcc {
            return Ok(entry.codec);
        }
    }
    Err(ErrorCode::Chunk)
}

/// Convert a FourCC string to an audio codec.
/// Returns Err for unknown or too-short FourCCs so callers can use is_ok()
/// as a reliable enhanced-header discriminator.
pub fn fourcc_to_audio_codec(fourcc: &[u8]) -> Result<AudioCodec> {
    if fourcc.len() < 4 {
        return Err(ErrorCode::Chunk);
    }
    for entry in AUDIO_FOURCCS {
        if fourcc[..4] == entry.fourcc {
            return Ok(entry.codec);
        }
    }
    Err(ErrorCode::Chunk)
}

/// Convert a video codec to its FourCC string.
pub fn video_codec_to_fourcc(codec: VideoCodec) -> &'static str {
    for entry in VIDEO_FOURCCS {
        if entry.codec == codec {
            return std::str::from_utf8(&entry.fourcc).unwrap_or("avc1");
        }
    }
    "avc1"
}

/// Convert an audio codec to its FourCC string.
pub fn audio_codec_to_fourcc(codec: AudioCodec) -> &'static str {
    for entry in AUDIO_FOURCCS {
        if entry.codec == codec {
            return std::str::from_utf8(&entry.fourcc).unwrap_or("mp4a");
        }
    }
    "mp4a"
}

/// Get the human-readable name for a video FourCC.
pub fn fourcc_video_name(fourcc: &[u8]) -> Option<&'static str> {
    if fourcc.len() < 4 {
        return None;
    }
    for entry in VIDEO_FOURCCS {
        if fourcc[..4] == entry.fourcc {
            return Some(entry.name);
        }
    }
    None
}

/// Get the human-readable name for an audio FourCC.
pub fn fourcc_audio_name(fourcc: &[u8]) -> Option<&'static str> {
    if fourcc.len() < 4 {
        return None;
    }
    for entry in AUDIO_FOURCCS {
        if fourcc[..4] == entry.fourcc {
            return Some(entry.name);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{AudioCodec, VideoCodec};

    #[test]
    fn video_fourcc_roundtrip() {
        assert_eq!(fourcc_to_video_codec(b"avc1").unwrap(), VideoCodec::H264);
        assert_eq!(fourcc_to_video_codec(b"hvc1").unwrap(), VideoCodec::H265);
        assert_eq!(fourcc_to_video_codec(b"av01").unwrap(), VideoCodec::Av1);
        assert_eq!(video_codec_to_fourcc(VideoCodec::H264), "avc1");
    }

    #[test]
    fn audio_fourcc_roundtrip() {
        assert_eq!(fourcc_to_audio_codec(b"mp4a").unwrap(), AudioCodec::Aac);
        assert_eq!(fourcc_to_audio_codec(b"Opus").unwrap(), AudioCodec::Opus);
        assert_eq!(fourcc_to_audio_codec(b"ec-3").unwrap(), AudioCodec::Aac);
        assert_eq!(audio_codec_to_fourcc(AudioCodec::Aac), "mp4a");
    }

    #[test]
    fn unknown_fourcc_returns_error() {
        assert!(fourcc_to_video_codec(b"xxxx").is_err());
        assert!(fourcc_to_audio_codec(b"zzzz").is_err());
        assert!(fourcc_to_video_codec(b"ab").is_err());
    }

    #[test]
    fn fourcc_names() {
        assert_eq!(fourcc_video_name(b"avc1"), Some("H.264/AVC"));
        assert_eq!(fourcc_audio_name(b"mp4a"), Some("AAC"));
        assert!(fourcc_video_name(b"zzzz").is_none());
    }
}
