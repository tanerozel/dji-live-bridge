//! FLV script data (metadata) parser
//!
//! Mirrors `src/flv/script_tag.h` and `src/flv/script_tag.c`.

use crate::amf::amf0;
use crate::buffer::Buffer;
use crate::types::{ErrorCode, Result, ScriptTag};

/// Parse an FLV script tag.
pub fn parse(data: &[u8], tag: &mut ScriptTag) -> Result<()> {
    *tag = ScriptTag::default();

    if data.len() < 2 {
        return Err(ErrorCode::Internal);
    }

    let mut buf = Buffer::from_slice(data);

    // First value is usually a string "onMetaData".
    // Peek at the type byte to choose the right reader; the reader functions
    // (read_string, read_long_string, skip_value) each consume the marker
    // themselves, so we must not call read_type() here as that would advance
    // the buffer past the marker before the reader sees it.
    let first_byte = buf.peek().first().copied().ok_or(ErrorCode::Internal)?;
    match first_byte {
        b if b == amf0::Amf0Type::String as u8 => {
            let mut name = [0u8; 64];
            let len = amf0::read_string(&mut buf, &mut name)?;
            tag.name[..len].copy_from_slice(&name[..len]);
        }
        b if b == amf0::Amf0Type::LongString as u8 => {
            let mut name = [0u8; 64];
            let len = amf0::read_long_string(&mut buf, &mut name)?;
            tag.name[..len].copy_from_slice(&name[..len]);
        }
        _ => {
            amf0::skip_value(&mut buf)?;
        }
    }

    // Second value is the metadata
    if let Ok(ty) = amf0::read_type(&mut buf) {
        if ty == amf0::Amf0Type::EcmaArray || ty == amf0::Amf0Type::Object {
            if ty == amf0::Amf0Type::EcmaArray {
                let mut count_bytes = [0u8; 4];
                buf.read(&mut count_bytes).map_err(|_| ErrorCode::Amf)?;
            }
            let mut keys = 0;
            while !amf0::is_object_end(&mut buf) {
                keys += 1;
                if keys > 256 {
                    return Err(ErrorCode::Amf);
                }
                let mut key = [0u8; 256];
                amf0::read_object_key(&mut buf, &mut key)?;
                amf0::skip_value(&mut buf)?;
            }
            let mut end = [0u8; 3];
            buf.read(&mut end).map_err(|_| ErrorCode::Amf)?;
        }
    }

    tag.data = data.as_ptr();
    tag.size = data.len();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::amf::amf0;
    use crate::types::ScriptTag;

    fn amf0_string(s: &str) -> Vec<u8> {
        let mut buf = crate::buffer::Buffer::new();
        amf0::write_string(&mut buf, s).unwrap();
        buf.as_slice().to_vec()
    }

    fn amf0_object_end() -> [u8; 3] {
        [0, 0, 0x09]
    }

    #[test]
    fn parse_on_metadata_string_name() {
        let mut payload = amf0_string("onMetaData");
        payload.push(amf0::Amf0Type::Object as u8);
        payload.extend_from_slice(&amf0_object_end());
        let mut tag = ScriptTag::default();
        parse(&payload, &mut tag).unwrap();
        let name = std::str::from_utf8(&tag.name)
            .unwrap_or("")
            .trim_end_matches('\0');
        assert_eq!(name, "onMetaData");
    }

    #[test]
    fn parse_rejects_too_short_payload() {
        let mut tag = ScriptTag::default();
        assert_eq!(parse(&[0x02], &mut tag), Err(ErrorCode::Internal));
    }

    #[test]
    fn parse_clears_name_suffix_on_shorter_reuse() {
        let mut payload = amf0_string("onMetaData");
        payload.push(amf0::Amf0Type::Object as u8);
        payload.extend_from_slice(&amf0_object_end());
        let mut tag = ScriptTag::default();
        parse(&payload, &mut tag).unwrap();

        let mut short = amf0_string("hi");
        short.push(amf0::Amf0Type::Object as u8);
        short.extend_from_slice(&amf0_object_end());
        parse(&short, &mut tag).unwrap();

        let name = std::str::from_utf8(&tag.name)
            .unwrap_or("")
            .trim_end_matches('\0');
        assert_eq!(name, "hi");
    }

    #[test]
    fn parse_error_leaves_tag_cleared() {
        let mut payload = amf0_string("onMetaData");
        payload.push(amf0::Amf0Type::Object as u8);
        payload.extend_from_slice(&amf0_object_end());
        let mut tag = ScriptTag::default();
        parse(&payload, &mut tag).unwrap();

        assert_eq!(parse(&[0x02], &mut tag), Err(ErrorCode::Internal));
        assert_eq!(tag.name, [0u8; 64]);
    }
}
