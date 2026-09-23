//! E-RTMP v1 fourCcList + E-RTMP v2 capability layer
//!
//! Mirrors `src/ertmp/connect_caps.c`.

use crate::types::{CapsExit, ErrorCode, FOUR_CC_INFO_ALL, FourCcList, Result, VideoFourCcInfoMap};

/* ── fourCcList ── */

/// Initialize a FourCC list.
pub fn fourcc_list_init(list: &mut FourCcList) {
    list.count = 0;
}

/// Add a FourCC to the list.
pub fn fourcc_list_add(list: &mut FourCcList, cc: &[u8]) -> Result<()> {
    if list.count >= crate::types::MAX_FOURCCS {
        return Err(ErrorCode::Io);
    }
    let entry = &mut list.entries[list.count];
    *entry = Default::default();
    if cc == b"*" {
        entry.cc[0] = b'*';
    } else if cc.len() >= 4 {
        entry.cc[..4].copy_from_slice(&cc[..4]);
    } else {
        return Err(ErrorCode::Io);
    }
    list.count += 1;
    Ok(())
}

/// Parse a FourCC list from raw data.
pub fn fourcc_list_parse(list: &mut FourCcList, data: &[u8]) -> Result<usize> {
    if data.len() < 4 {
        return Err(ErrorCode::Io);
    }
    fourcc_list_init(list);

    let count = ((data[0] as u32) << 24)
        | ((data[1] as u32) << 16)
        | ((data[2] as u32) << 8)
        | (data[3] as u32);
    let count = count.min(crate::types::MAX_FOURCCS as u32);

    let mut offset = 4;
    for _ in 0..count {
        if offset + 2 > data.len() {
            break;
        }
        let slen = (((data[offset] as u16) << 8) | (data[offset + 1] as u16)) as usize;
        offset += 2;
        if !matches!(slen, 1 | 4) || offset + slen > data.len() {
            break;
        }
        fourcc_list_add(list, &data[offset..offset + slen])?;
        offset += slen;
    }
    Ok(list.count)
}

/// Write a FourCC list to a buffer. Returns bytes written.
pub fn fourcc_list_write(list: &FourCcList, buf: &mut [u8]) -> usize {
    let needed = 4
        + (0..list.count)
            .map(|i| if list.entries[i].cc[0] == b'*' { 3 } else { 6 })
            .sum::<usize>();
    if buf.len() < needed {
        return 0;
    }

    let cnt = list.count as u32;
    buf[..4].copy_from_slice(&cnt.to_be_bytes());

    let mut offset = 4;
    for i in 0..list.count {
        let wildcard = list.entries[i].cc[0] == b'*';
        let len = if wildcard { 1 } else { 4 };
        buf[offset..offset + 2].copy_from_slice(&(len as u16).to_be_bytes());
        offset += 2;
        if wildcard {
            buf[offset] = b'*';
        } else {
            buf[offset..offset + 4].copy_from_slice(&list.entries[i].cc[..4]);
        }
        offset += len;
    }
    offset
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::FourCcList;

    #[test]
    fn fourcc_list_add_rejects_short_input_instead_of_panicking() {
        let mut list = FourCcList::default();
        assert!(matches!(
            fourcc_list_add(&mut list, &[0x61, 0x76]),
            Err(ErrorCode::Io)
        ));
        assert_eq!(list.count, 0);
    }

    #[test]
    fn fourcc_list_add_accepts_exactly_four_bytes() {
        let mut list = FourCcList::default();
        assert!(fourcc_list_add(&mut list, b"av01").is_ok());
        assert_eq!(list.count, 1);
        assert_eq!(&list.entries[0].cc[..4], b"av01");
    }
}

/* ── E-RTMP v2 capsEx ── */

/// Parse capability negotiation data.
pub fn caps_exit_parse(caps: &mut CapsExit, data: &[u8]) -> Result<()> {
    if data.len() < 8 {
        return Err(ErrorCode::Io);
    }
    caps.version = 1;
    caps.video_codec_32 =
        ((data[0] as u32) << 24 | (data[1] as u32) << 16 | (data[2] as u32) << 8 | data[3] as u32)
            as i32;
    caps.audio_codec_32 =
        ((data[4] as u32) << 24 | (data[5] as u32) << 16 | (data[6] as u32) << 8 | data[7] as u32)
            as i32;
    Ok(())
}

/// Write capability negotiation data. Returns bytes written.
pub fn caps_exit_write(caps: &CapsExit, buf: &mut [u8]) -> usize {
    if buf.len() < 8 {
        return 0;
    }
    let vc = caps.video_codec_32 as u32;
    let ac = caps.audio_codec_32 as u32;
    buf[0] = (vc >> 24) as u8;
    buf[1] = (vc >> 16) as u8;
    buf[2] = (vc >> 8) as u8;
    buf[3] = vc as u8;
    buf[4] = (ac >> 24) as u8;
    buf[5] = (ac >> 16) as u8;
    buf[6] = (ac >> 8) as u8;
    buf[7] = ac as u8;
    8
}

/* ── E-RTMP v2 videoFourCcInfoMap ── */

/// Parse a video FourCC info map from the legacy binary helper representation.
/// New encodings append a UI32 capability mask to each key; the old key-only
/// representation remains accepted and is interpreted as supporting all flags.
pub fn video_fourcc_info_map_parse(map: &mut VideoFourCcInfoMap, data: &[u8]) -> Result<usize> {
    if data.len() < 4 {
        return Err(ErrorCode::Io);
    }
    *map = VideoFourCcInfoMap::default();

    let count = u32::from_be_bytes([data[0], data[1], data[2], data[3]])
        .min(crate::types::MAX_FOURCCS as u32) as usize;
    let legacy_key_only = data.len() == 4 + count * 6;
    let mut offset = 4;

    for _ in 0..count {
        if offset + 2 > data.len() {
            return Err(ErrorCode::Io);
        }
        let slen = u16::from_be_bytes([data[offset], data[offset + 1]]) as usize;
        offset += 2;
        if !matches!(slen, 1 | 4) || offset + slen > data.len() {
            return Err(ErrorCode::Io);
        }

        let entry = &mut map.entries[map.count];
        if slen == 1 && data[offset] == b'*' {
            entry.cc[0] = b'*';
        } else if slen == 4 {
            entry.cc[..4].copy_from_slice(&data[offset..offset + 4]);
        } else {
            return Err(ErrorCode::Io);
        }
        offset += slen;

        map.masks[map.count] = if legacy_key_only {
            FOUR_CC_INFO_ALL
        } else {
            if offset + 4 > data.len() {
                return Err(ErrorCode::Io);
            }
            let mask = u32::from_be_bytes([
                data[offset],
                data[offset + 1],
                data[offset + 2],
                data[offset + 3],
            ]);
            offset += 4;
            mask
        };
        map.count += 1;
    }
    Ok(map.count)
}

/// Write the binary helper representation with a UI32 capability mask per key.
pub fn video_fourcc_info_map_write(map: &VideoFourCcInfoMap, buf: &mut [u8]) -> usize {
    let needed = 4
        + (0..map.count)
            .map(|i| if map.entries[i].cc[0] == b'*' { 7 } else { 10 })
            .sum::<usize>();
    if buf.len() < needed {
        return 0;
    }

    buf[..4].copy_from_slice(&(map.count as u32).to_be_bytes());
    let mut offset = 4;
    for i in 0..map.count {
        let wildcard = map.entries[i].cc[0] == b'*';
        let len = if wildcard { 1 } else { 4 };
        buf[offset..offset + 2].copy_from_slice(&(len as u16).to_be_bytes());
        offset += 2;
        if wildcard {
            buf[offset] = b'*';
        } else {
            buf[offset..offset + 4].copy_from_slice(&map.entries[i].cc[..4]);
        }
        offset += len;
        buf[offset..offset + 4].copy_from_slice(&map.masks[i].to_be_bytes());
        offset += 4;
    }
    offset
}

#[cfg(test)]
mod video_map_tests {
    use super::*;
    use crate::types::{FOUR_CC_INFO_CAN_DECODE, FOUR_CC_INFO_CAN_FORWARD};

    #[test]
    fn fourcc_list_round_trips_wildcard() {
        let mut list = FourCcList::default();
        fourcc_list_add(&mut list, b"*").unwrap();
        let mut wire = [0u8; 32];
        let len = fourcc_list_write(&list, &mut wire);
        let mut parsed = FourCcList::default();
        fourcc_list_parse(&mut parsed, &wire[..len]).unwrap();
        assert_eq!(parsed.count, 1);
        assert_eq!(parsed.entries[0].cc[0], b'*');
    }

    #[test]
    fn video_info_map_round_trips_flags() {
        let mut map = VideoFourCcInfoMap::default();
        map.entries[0].cc[..4].copy_from_slice(b"vp09");
        map.masks[0] = FOUR_CC_INFO_CAN_DECODE | FOUR_CC_INFO_CAN_FORWARD;
        map.count = 1;

        let mut wire = [0u8; 64];
        let len = video_fourcc_info_map_write(&map, &mut wire);
        let mut parsed = VideoFourCcInfoMap::default();
        video_fourcc_info_map_parse(&mut parsed, &wire[..len]).unwrap();

        assert_eq!(parsed.count, 1);
        assert_eq!(&parsed.entries[0].cc[..4], b"vp09");
        assert_eq!(parsed.masks[0], map.masks[0]);
    }
}
