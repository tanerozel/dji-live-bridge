//! E-RTMP v2 ModEx extension mechanism
//!
//! Mirrors `src/ertmp/modex.c`.

use crate::types::{ErrorCode, Modex, ModexType, Result};

const MODEX_MARKER: u8 = 0x80;

/// Parse a ModEx extension.
pub fn modex_parse(modex: &mut Modex, data: &[u8]) -> Result<()> {
    if data.is_empty() {
        return Err(ErrorCode::Io);
    }

    let marker = data[0];
    if marker & 0x80 == 0 {
        return Err(ErrorCode::Protocol);
    }

    let ty = marker & 0x7F;
    match ty {
        0 => {
            modex.modex_type = ModexType::Nop;
            modex.offset = 0;
        }
        1 => {
            if data.len() < 9 {
                return Err(ErrorCode::Io);
            }
            modex.modex_type = ModexType::Timestamp;
            modex.offset = 0;
            for i in 0..8 {
                modex.offset = (modex.offset << 8) | data[1 + i] as u64;
            }
        }
        _ => {
            // Unknown ModEx type — ignore per §16 graceful-degradation rule
            modex.modex_type = ModexType::Nop;
            modex.offset = 0;
        }
    }

    Ok(())
}

/// Write a ModEx extension. Returns bytes written, or 0 if the buffer is too small.
pub fn modex_write(modex: &Modex, buf: &mut [u8]) -> usize {
    match modex.modex_type {
        ModexType::Nop => {
            if buf.is_empty() {
                return 0;
            }
            buf[0] = MODEX_MARKER | ModexType::Nop as u8;
            1
        }
        ModexType::Timestamp => {
            if buf.len() < 9 {
                return 0;
            }
            buf[0] = MODEX_MARKER | ModexType::Timestamp as u8;
            let mut tmp = modex.offset;
            for i in (0..8).rev() {
                buf[1 + i] = (tmp & 0xFF) as u8;
                tmp >>= 8;
            }
            9
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nop_round_trips_through_write_and_parse() {
        let modex = Modex {
            modex_type: ModexType::Nop,
            offset: 0,
        };
        let mut buf = [0u8; 1];
        assert_eq!(modex_write(&modex, &mut buf), 1);

        let mut parsed = Modex::default();
        modex_parse(&mut parsed, &buf).unwrap();
        assert_eq!(parsed.modex_type, ModexType::Nop);
    }

    #[test]
    fn timestamp_round_trips_through_write_and_parse() {
        let modex = Modex {
            modex_type: ModexType::Timestamp,
            offset: 0x0102_0304_0506_0708,
        };
        let mut buf = [0u8; 9];
        assert_eq!(modex_write(&modex, &mut buf), 9);

        let mut parsed = Modex::default();
        modex_parse(&mut parsed, &buf).unwrap();
        assert_eq!(parsed.modex_type, ModexType::Timestamp);
        assert_eq!(parsed.offset, modex.offset);
    }

    #[test]
    fn parse_rejects_empty_input() {
        let mut modex = Modex::default();
        assert!(modex_parse(&mut modex, &[]).is_err());
    }

    #[test]
    fn parse_rejects_marker_without_high_bit_set() {
        let mut modex = Modex::default();
        assert!(modex_parse(&mut modex, &[0x01]).is_err());
    }

    #[test]
    fn parse_rejects_short_timestamp_payload() {
        let mut modex = Modex::default();
        assert!(modex_parse(&mut modex, &[0x81, 0, 0, 0]).is_err());
    }

    #[test]
    fn parse_degrades_unknown_type_to_nop_instead_of_erroring() {
        let mut modex = Modex::default();
        modex_parse(&mut modex, &[0xFF]).unwrap();
        assert_eq!(modex.modex_type, ModexType::Nop);
    }

    #[test]
    fn parse_unknown_type_after_timestamp_clears_stale_offset() {
        let mut modex = Modex::default();
        modex_parse(&mut modex, &[0x81, 0, 0, 0, 0, 0, 0, 0, 5]).unwrap();
        assert_eq!(modex.offset, 5);

        modex_parse(&mut modex, &[0xFF]).unwrap();
        assert_eq!(modex.modex_type, ModexType::Nop);
        assert_eq!(modex.offset, 0);
    }

    #[test]
    fn write_rejects_short_buffer() {
        let modex = Modex {
            modex_type: ModexType::Timestamp,
            offset: 1,
        };
        let mut buf = [0u8; 8];
        assert_eq!(modex_write(&modex, &mut buf), 0);

        let nop = Modex::default();
        let mut empty: [u8; 0] = [];
        assert_eq!(modex_write(&nop, &mut empty), 0);
    }
}
