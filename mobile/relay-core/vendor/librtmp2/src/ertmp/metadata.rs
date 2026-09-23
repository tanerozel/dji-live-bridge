//! E-RTMP v1 metadata / HDR / colorInfo support
//!
//! Mirrors `src/ertmp/metadata.h` and `src/ertmp/metadata.c`.

use crate::types::{ErrorCode, HdrInfo, Result};

/// Initialize HDR info with defaults.
pub fn hdr_init(hdr: &mut HdrInfo) {
    hdr.color_primaries = 1;
    hdr.transfer_chars = 1;
    hdr.matrix_coeffs = 1;
}

/// Parse HDR color info from a raw blob.
pub fn hdr_parse(data: &[u8], hdr: &mut HdrInfo) -> Result<()> {
    hdr_init(hdr);
    if data.len() < 6 {
        return Err(ErrorCode::Io);
    }
    hdr.color_primaries = ((data[0] as u16) << 8) | (data[1] as u16);
    hdr.transfer_chars = ((data[2] as u16) << 8) | (data[3] as u16);
    hdr.matrix_coeffs = ((data[4] as u16) << 8) | (data[5] as u16);
    Ok(())
}

/// Write HDR color info to a buffer. Returns bytes written.
pub fn hdr_write(hdr: &HdrInfo, buf: &mut [u8]) -> usize {
    if buf.len() < 6 {
        return 0;
    }
    buf[0] = (hdr.color_primaries >> 8) as u8;
    buf[1] = hdr.color_primaries as u8;
    buf[2] = (hdr.transfer_chars >> 8) as u8;
    buf[3] = hdr.transfer_chars as u8;
    buf[4] = (hdr.matrix_coeffs >> 8) as u8;
    buf[5] = hdr.matrix_coeffs as u8;
    6
}

/// Parse colorInfo from metadata.
pub fn metadata_colorinfo_parse(data: &[u8], hdr: &mut HdrInfo) -> Result<()> {
    hdr_init(hdr);
    if data.len() < 6 {
        return Err(ErrorCode::Io);
    }
    hdr_parse(data, hdr)
}

/// Convert a FourCC to a videocodecid UI32.
pub fn videocodecid_from_fourcc(fourcc: &[u8]) -> u32 {
    if fourcc.len() < 4 {
        return 7; // default AVC
    }
    ((fourcc[0] as u32) << 24)
        | ((fourcc[1] as u32) << 16)
        | ((fourcc[2] as u32) << 8)
        | (fourcc[3] as u32)
}

/// Stub for E-RTMP v2 capability negotiation.
pub fn caps_negotiate() -> Result<()> {
    Err(ErrorCode::Unsupported)
}
