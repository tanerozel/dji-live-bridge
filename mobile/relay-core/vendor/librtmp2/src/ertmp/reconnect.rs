//! E-RTMP v2 reconnect mechanism
//!
//! Mirrors `src/ertmp/reconnect.c`.

use crate::types::{ErrorCode, Reconnect, Result};

/// Parse a reconnect request.
pub fn reconnect_parse(rc: &mut Reconnect, data: &[u8]) -> Result<()> {
    if data.len() != 8 {
        return Err(ErrorCode::Io);
    }
    rc.replay = ((data[0] as u32) << 24)
        | ((data[1] as u32) << 16)
        | ((data[2] as u32) << 8)
        | (data[3] as u32);
    rc.limit = ((data[4] as u32) << 24)
        | ((data[5] as u32) << 16)
        | ((data[6] as u32) << 8)
        | (data[7] as u32);
    Ok(())
}

/// Write a reconnect request. Returns bytes written.
pub fn reconnect_write(rc: &Reconnect, buf: &mut [u8]) -> usize {
    if buf.len() < 8 {
        return 0;
    }
    buf[0] = (rc.replay >> 24) as u8;
    buf[1] = (rc.replay >> 16) as u8;
    buf[2] = (rc.replay >> 8) as u8;
    buf[3] = rc.replay as u8;
    buf[4] = (rc.limit >> 24) as u8;
    buf[5] = (rc.limit >> 16) as u8;
    buf[6] = (rc.limit >> 8) as u8;
    buf[7] = rc.limit as u8;
    8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_write_and_parse() {
        let rc = Reconnect {
            replay: 0x0102_0304,
            limit: 0xAABB_CCDD,
        };
        let mut buf = [0u8; 8];
        assert_eq!(reconnect_write(&rc, &mut buf), 8);

        let mut parsed = Reconnect::default();
        reconnect_parse(&mut parsed, &buf).unwrap();
        assert_eq!(parsed.replay, rc.replay);
        assert_eq!(parsed.limit, rc.limit);
    }

    #[test]
    fn parse_rejects_short_input() {
        let mut rc = Reconnect::default();
        assert!(reconnect_parse(&mut rc, &[0u8; 7]).is_err());
    }

    #[test]
    fn parse_rejects_long_input() {
        let mut rc = Reconnect::default();
        assert!(reconnect_parse(&mut rc, &[0u8; 9]).is_err());
    }

    #[test]
    fn write_rejects_short_buffer() {
        let rc = Reconnect::default();
        let mut buf = [0u8; 7];
        assert_eq!(reconnect_write(&rc, &mut buf), 0);
    }
}
