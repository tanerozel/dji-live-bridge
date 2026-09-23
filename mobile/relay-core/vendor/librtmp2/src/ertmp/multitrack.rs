//! E-RTMP v2 multitrack streaming
//!
//! Mirrors `src/ertmp/multitrack.c`.

use crate::types::{ErrorCode, Multitrack, Result};

/// Parse a multitrack descriptor.
pub fn multitrack_parse(mt: &mut Multitrack, data: &[u8]) -> Result<()> {
    *mt = Multitrack::default();

    if data.len() < 9 {
        return Err(ErrorCode::Io);
    }

    if data[0] != 0x00 {
        return Err(ErrorCode::Protocol);
    }

    // The 0x00 marker identifies an AMF0_NUMBER: the 8 payload bytes are the
    // big-endian bit pattern of an IEEE-754 double, not a raw integer.
    let type_val = f64::from_be_bytes(data[1..9].try_into().unwrap());

    mt.track_type = if type_val == 0.0 {
        crate::types::MultitrackType::Audio
    } else if type_val == 1.0 {
        crate::types::MultitrackType::Video
    } else {
        crate::types::MultitrackType::Metadata
    };

    let name_offset = 9;
    if name_offset + 2 > data.len() {
        return Err(ErrorCode::Io);
    }

    let name_len = ((data[name_offset] as usize) << 8) | (data[name_offset + 1] as usize);
    if name_offset + 2 + name_len > data.len() {
        return Err(ErrorCode::Io);
    }

    let copy_len = name_len.min(63);
    mt.track_name[..copy_len].copy_from_slice(&data[name_offset + 2..name_offset + 2 + copy_len]);
    mt.track_name[copy_len] = 0;

    Ok(())
}

/// Write a multitrack descriptor. Returns bytes written.
pub fn multitrack_write(mt: &Multitrack, buf: &mut [u8]) -> usize {
    let name_len = mt.track_name.iter().position(|&b| b == 0).unwrap_or(63);
    let needed = 1 + 8 + 2 + name_len;
    if buf.len() < needed {
        return 0;
    }

    let mut offset = 0;
    // AMF0_NUMBER marker
    buf[offset] = 0x00;
    offset += 1;

    // 8-byte AMF0_NUMBER (IEEE-754 double bit pattern), big-endian.
    buf[offset..offset + 8].copy_from_slice(&(mt.track_type as u8 as f64).to_be_bytes());
    offset += 8;

    // AMF0_STRING: 2-byte length + N bytes
    buf[offset] = (name_len >> 8) as u8;
    buf[offset + 1] = name_len as u8;
    offset += 2;
    buf[offset..offset + name_len].copy_from_slice(&mt.track_name[..name_len]);
    offset += name_len;

    offset
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::MultitrackType;

    #[test]
    fn video_track_type_round_trips_as_amf0_double() {
        let mut mt = Multitrack {
            track_type: MultitrackType::Video,
            track_name: [0; 64],
        };
        mt.track_name[..3].copy_from_slice(b"cam");

        let mut buf = [0u8; 128];
        let n = multitrack_write(&mt, &mut buf);
        assert!(n > 0);

        // A spec-compliant AMF0 encoding of 1.0 is the IEEE-754 bit pattern
        // 0x3FF0000000000000, not the raw integer 1.
        assert_eq!(
            &buf[1..9],
            &[0x3F, 0xF0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]
        );

        let mut out = Multitrack::default();
        multitrack_parse(&mut out, &buf[..n]).unwrap();
        assert_eq!(out.track_type, MultitrackType::Video);
        assert_eq!(&out.track_name[..3], b"cam");
    }

    #[test]
    fn audio_track_type_round_trips() {
        let mt = Multitrack {
            track_type: MultitrackType::Audio,
            track_name: [0; 64],
        };
        let mut buf = [0u8; 128];
        let n = multitrack_write(&mt, &mut buf);

        let mut out = Multitrack::default();
        multitrack_parse(&mut out, &buf[..n]).unwrap();
        assert_eq!(out.track_type, MultitrackType::Audio);
    }

    #[test]
    fn parse_error_leaves_descriptor_cleared() {
        let mut mt = Multitrack {
            track_type: MultitrackType::Video,
            track_name: {
                let mut name = [0u8; 64];
                name[..7].copy_from_slice(b"stale-x");
                name
            },
        };
        assert!(multitrack_parse(&mut mt, &[0x00; 8]).is_err());
        assert_eq!(mt.track_type, MultitrackType::Audio);
        assert_eq!(mt.track_name[0], 0);
    }
}
