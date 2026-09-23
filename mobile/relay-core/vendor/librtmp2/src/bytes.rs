//! Byte-swapping and endian helpers
//!
//! Mirrors `src/core/bytes.h` and `src/core/bytes.c`.

/// Swap bytes of a u16 (big-endian ↔ little-endian).
pub fn byteswap16(val: u16) -> u16 {
    val.swap_bytes()
}

/// Swap bytes of a u32 (big-endian ↔ little-endian).
pub fn byteswap32(val: u32) -> u32 {
    val.swap_bytes()
}

/// Swap bytes of a u64 (big-endian ↔ little-endian).
pub fn byteswap64(val: u64) -> u64 {
    val.swap_bytes()
}

/// Read a 24-bit big-endian value from a byte buffer.
pub fn ntoh24(buf: &[u8]) -> u32 {
    ((buf[0] as u32) << 16) | ((buf[1] as u32) << 8) | (buf[2] as u32)
}

/// Write a 24-bit big-endian value to a byte buffer.
pub fn hton24(buf: &mut [u8], val: u32) {
    buf[0] = ((val >> 16) & 0xFF) as u8;
    buf[1] = ((val >> 8) & 0xFF) as u8;
    buf[2] = (val & 0xFF) as u8;
}

/// Convert a u32 to big-endian (network byte order).
pub fn hton32(val: u32) -> u32 {
    val.to_be()
}

/// Read a big-endian u32 from a byte buffer.
pub fn ntoh32(buf: &[u8]) -> u32 {
    u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]])
}

/// Read a big-endian u16 from a byte buffer.
pub fn ntoh16(buf: &[u8]) -> u16 {
    u16::from_be_bytes([buf[0], buf[1]])
}

/// Convert a u16 to big-endian.
pub fn hton16(val: u16) -> u16 {
    val.to_be()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn byteswap_round_trips() {
        assert_eq!(byteswap16(byteswap16(0x1234)), 0x1234);
        assert_eq!(byteswap32(byteswap32(0x1234_5678)), 0x1234_5678);
        assert_eq!(
            byteswap64(byteswap64(0x1234_5678_9ABC_DEF0)),
            0x1234_5678_9ABC_DEF0
        );
    }

    #[test]
    fn ntoh24_reads_big_endian_24_bit() {
        assert_eq!(ntoh24(&[0x01, 0x02, 0x03]), 0x0001_0203);
    }

    #[test]
    fn hton24_writes_big_endian_24_bit() {
        let mut buf = [0u8; 3];
        hton24(&mut buf, 0x0001_0203);
        assert_eq!(buf, [0x01, 0x02, 0x03]);
    }

    #[test]
    fn ntoh32_reads_big_endian_32_bit() {
        assert_eq!(ntoh32(&[0xDE, 0xAD, 0xBE, 0xEF]), 0xDEAD_BEEF);
    }

    #[test]
    fn ntoh16_reads_big_endian_16_bit() {
        assert_eq!(ntoh16(&[0x12, 0x34]), 0x1234);
    }

    #[test]
    fn hton32_then_to_ne_bytes_matches_ntoh32() {
        // hton32 already performs the host->network swap; writing its
        // result with to_ne_bytes (not to_be_bytes) yields the correct
        // wire bytes. Mixing the two would double-swap.
        assert_eq!(ntoh32(&hton32(0xDEAD_BEEF).to_ne_bytes()), 0xDEAD_BEEF);
    }
}
