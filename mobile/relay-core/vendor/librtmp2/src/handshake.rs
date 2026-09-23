//! Legacy RTMP handshake (C0/C1/C2 ↔ S0/S1/S2)
//!
//! Mirrors `src/handshake/handshake.h` and `src/handshake/handshake.c`.

use crate::buffer::Buffer;
use crate::bytes::ntoh32;
use crate::types::ErrorCode;
use crate::types::Result;

/// RTMP protocol version byte
const RTMP_VERSION: u8 = 0x03;
/// Handshake payload size
const HANDSHAKE_SIZE: usize = 1536;

/// Handshake state machine states.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub enum HandshakeState {
    ServerWaitC0 = 0,
    ServerWaitC1,
    ServerWaitC2,
    ClientWaitS0,
    ClientWaitS1,
    ClientWaitS2,
    Done,
}

/// Handshake context.
#[derive(Debug)]
pub struct Handshake {
    pub state: HandshakeState,
    pub version: u8,
    pub peer_time: u32,
    /// queued output bytes
    pub out: Buffer,
}

impl Default for Handshake {
    fn default() -> Self {
        Self {
            state: HandshakeState::ServerWaitC0,
            version: 0,
            peer_time: 0,
            out: Buffer::new(),
        }
    }
}

impl Handshake {
    /// Check if the handshake is complete.
    pub fn is_complete(&self) -> bool {
        self.state == HandshakeState::Done
    }

    /// Release the internal output buffer.
    pub fn cleanup(&mut self) {
        self.out.reset();
    }
}

/* ── SplitMix64 PRNG ── */

/// SplitMix64: a small, fast PRNG used to fill the handshake's random payload.
fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E3779B97F4A7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
    z ^ (z >> 31)
}

/// Counter mixed into the PRNG seed so calls made at the same timestamp still differ.
static RANDOM_SEED_COUNTER: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);

/// Fill `buf` with pseudo-random bytes using a seeded PRNG.
fn fill_random(buf: &mut [u8]) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let counter = RANDOM_SEED_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let mut state = now.as_secs()
        ^ (u64::from(now.subsec_nanos()) << 32)
        ^ counter.wrapping_mul(0xD1B5_4A32_D192_ED03)
        ^ (buf.len() as u64);

    let mut i = 0;
    while i < buf.len() {
        let r = splitmix64(&mut state);
        for b in 0..8 {
            if i >= buf.len() {
                break;
            }
            buf[i] = (r >> (b * 8)) as u8;
            i += 1;
        }
    }
}

/// Get the current time as a u32.
fn get_time() -> u32 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as u32
}

/// True when C1 advertises the Adobe digest/complex handshake schema.
#[allow(dead_code)]
fn c1_requests_complex_handshake(c1: &[u8]) -> bool {
    c1.len() >= HANDSHAKE_SIZE && (c1[4] != 0 || c1[5] != 0 || c1[6] != 0 || c1[7] != 0)
}

/* ── Server-side handshake ── */

/// Initialize a server-side handshake.
pub fn server_init(hs: &mut Handshake) {
    hs.state = HandshakeState::ServerWaitC0;
    hs.version = 0;
    hs.peer_time = 0;
}

/// Read C0 (version byte) from the client.
pub fn server_read_c0(hs: &mut Handshake, buf: &mut Buffer) -> Result<()> {
    let mut ver = [0u8; 1];
    buf.read(&mut ver).map_err(|_| ErrorCode::Io)?;

    if ver[0] != RTMP_VERSION {
        return Err(ErrorCode::Handshake);
    }

    hs.version = ver[0];
    hs.state = HandshakeState::ServerWaitC1;
    Ok(())
}

/// Read C1 (1536-byte handshake) from the client and queue S1+S2.
pub fn server_read_c1(hs: &mut Handshake, buf: &mut Buffer) -> Result<()> {
    if buf.available() < HANDSHAKE_SIZE {
        return Err(ErrorCode::Io);
    }

    let mut c1 = vec![0u8; HANDSHAKE_SIZE];
    buf.read(&mut c1).map_err(|_| ErrorCode::Io)?;

    hs.peer_time = ntoh32(&c1[..4]);

    // Build S1
    let mut s1 = vec![0u8; HANDSHAKE_SIZE];
    let server_time = get_time();
    s1[..4].copy_from_slice(&server_time.to_be_bytes());
    // bytes 4-7 = 0
    fill_random(&mut s1[8..]);

    // S2 echoes C1's time1 (bytes 0..4) and random; time2 (bytes 4..8) is the
    // server's own time, per RTMP spec 5.2.4.
    let mut s2 = c1.clone();
    s2[4..8].copy_from_slice(&server_time.to_be_bytes());

    hs.out.reset();
    hs.out.write(&s1).map_err(|_| ErrorCode::Internal)?;
    hs.out.write(&s2).map_err(|_| ErrorCode::Internal)?;

    hs.state = HandshakeState::ServerWaitC2;
    Ok(())
}

/// Read C2 from the client, completing the handshake.
pub fn server_read_c2(hs: &mut Handshake, buf: &mut Buffer) -> Result<()> {
    if buf.available() < HANDSHAKE_SIZE {
        return Err(ErrorCode::Io);
    }

    let mut c2 = vec![0u8; HANDSHAKE_SIZE];
    buf.read(&mut c2).map_err(|_| ErrorCode::Io)?;

    hs.state = HandshakeState::Done;
    Ok(())
}

/* ── Client-side handshake ── */

/// Initialize a client-side handshake.
pub fn client_init(hs: &mut Handshake) {
    hs.state = HandshakeState::ClientWaitS0;
    hs.version = 0;
    hs.peer_time = 0;
}

/// Generate C0+C1 to send to the server.
pub fn client_generate_c0c1(hs: &mut Handshake) -> Result<()> {
    let client_time = get_time();
    hs.peer_time = client_time;

    let mut c0c1 = vec![0u8; 1 + HANDSHAKE_SIZE];
    c0c1[0] = RTMP_VERSION;
    c0c1[1..5].copy_from_slice(&client_time.to_be_bytes());
    // bytes 5-8 = 0
    fill_random(&mut c0c1[9..]);

    hs.out.reset();
    hs.out.write(&c0c1).map_err(|_| ErrorCode::Internal)?;
    hs.state = HandshakeState::ClientWaitS1;
    Ok(())
}

/// Read S0 (version byte) from the server.
pub fn client_read_s0(hs: &mut Handshake, buf: &mut Buffer) -> Result<()> {
    let mut ver = [0u8; 1];
    buf.read(&mut ver).map_err(|_| ErrorCode::Io)?;

    if ver[0] != RTMP_VERSION {
        return Err(ErrorCode::Handshake);
    }

    hs.version = ver[0];
    hs.state = HandshakeState::ClientWaitS1;
    Ok(())
}

/// Read S1 from the server and queue C2.
pub fn client_read_s1(hs: &mut Handshake, buf: &mut Buffer) -> Result<()> {
    if buf.available() < HANDSHAKE_SIZE {
        return Err(ErrorCode::Io);
    }

    let mut s1 = vec![0u8; HANDSHAKE_SIZE];
    buf.read(&mut s1).map_err(|_| ErrorCode::Io)?;

    hs.peer_time = ntoh32(&s1[..4]);

    // C2 echoes S1's time1 (bytes 0..4) and random; time2 (bytes 4..8) is the
    // client's own time, per RTMP spec 5.2.5.
    let mut c2 = s1.clone();
    c2[4..8].copy_from_slice(&get_time().to_be_bytes());

    hs.out.reset();
    hs.out.write(&c2).map_err(|_| ErrorCode::Internal)?;
    hs.state = HandshakeState::ClientWaitS2;
    Ok(())
}

/// Read S2 from the server, completing the handshake.
pub fn client_read_s2(hs: &mut Handshake, buf: &mut Buffer) -> Result<()> {
    if buf.available() < HANDSHAKE_SIZE {
        return Err(ErrorCode::Io);
    }

    let mut s2 = vec![0u8; HANDSHAKE_SIZE];
    buf.read(&mut s2).map_err(|_| ErrorCode::Io)?;

    hs.state = HandshakeState::Done;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_client_server_handshake_completes() {
        let mut client = Handshake::default();
        let mut server = Handshake::default();
        client_init(&mut client);
        server_init(&mut server);

        // Client sends C0+C1
        client_generate_c0c1(&mut client).unwrap();
        let c0c1 = client.out.peek().to_vec();
        client.out.reset();

        // Server reads C0+C1, generates S0+S1+S2
        let mut server_in = Buffer::new();
        server_in.write(&c0c1).unwrap();
        server_read_c0(&mut server, &mut server_in).unwrap();
        server_read_c1(&mut server, &mut server_in).unwrap();
        let s1s2 = server.out.peek().to_vec();

        assert_eq!(server.state, HandshakeState::ServerWaitC2);
        assert_eq!(s1s2.len(), HANDSHAKE_SIZE * 2);

        // Client reads S0 (we only sent S1+S2 above; prepend version byte)
        let mut s0s1s2 = vec![RTMP_VERSION];
        s0s1s2.extend_from_slice(&s1s2);
        let mut client_in = Buffer::new();
        client_in.write(&s0s1s2).unwrap();
        client_read_s0(&mut client, &mut client_in).unwrap();
        client_read_s1(&mut client, &mut client_in).unwrap();
        let c2 = client.out.peek().to_vec();
        assert_eq!(client.state, HandshakeState::ClientWaitS2);

        // Server reads C2
        let mut server_in2 = Buffer::new();
        server_in2.write(&c2).unwrap();
        server_read_c2(&mut server, &mut server_in2).unwrap();
        assert!(server.is_complete());

        // Client reads S2 (echo of C1, any 1536 bytes)
        let mut client_in2 = Buffer::new();
        client_in2.write(&vec![0u8; HANDSHAKE_SIZE]).unwrap();
        client_read_s2(&mut client, &mut client_in2).unwrap();
        assert!(client.is_complete());
    }

    #[test]
    fn server_read_c0_rejects_wrong_version() {
        let mut hs = Handshake::default();
        let mut buf = Buffer::new();
        buf.write(&[0x99]).unwrap();
        assert!(server_read_c0(&mut hs, &mut buf).is_err());
    }

    #[test]
    fn s1_time_field_matches_wall_clock_in_big_endian() {
        let mut hs = Handshake::default();
        server_init(&mut hs);
        let c1 = vec![0u8; HANDSHAKE_SIZE];
        let mut buf = Buffer::new();
        buf.write(&c1).unwrap();
        let before = get_time();
        server_read_c1(&mut hs, &mut buf).unwrap();
        let after = get_time();

        let s1 = &hs.out.peek()[..HANDSHAKE_SIZE];
        let time_from_s1 = u32::from_be_bytes([s1[0], s1[1], s1[2], s1[3]]);
        assert!(time_from_s1 >= before && time_from_s1 <= after);
    }

    #[test]
    fn s2_echoes_c1_time1_and_carries_server_time_in_time2() {
        let mut hs = Handshake::default();
        server_init(&mut hs);
        let mut c1 = vec![0u8; HANDSHAKE_SIZE];
        c1[..4].copy_from_slice(&0x1234_5678u32.to_be_bytes());
        c1[8..].copy_from_slice(&[0xAB; HANDSHAKE_SIZE - 8]);
        let mut buf = Buffer::new();
        buf.write(&c1).unwrap();

        let before = get_time();
        server_read_c1(&mut hs, &mut buf).unwrap();
        let after = get_time();

        let out = hs.out.peek();
        let s2 = &out[HANDSHAKE_SIZE..HANDSHAKE_SIZE * 2];
        let time1 = u32::from_be_bytes([s2[0], s2[1], s2[2], s2[3]]);
        let time2 = u32::from_be_bytes([s2[4], s2[5], s2[6], s2[7]]);
        assert_eq!(time1, 0x1234_5678, "S2 time1 must echo C1's time1");
        assert!(
            time2 >= before && time2 <= after,
            "S2 time2 must be the server's own time"
        );
        assert_eq!(
            &s2[8..],
            &c1[8..],
            "S2 random bytes must echo C1's random bytes"
        );
    }

    #[test]
    fn complex_c1_still_gets_simple_s1s2_response() {
        let mut hs = Handshake::default();
        server_init(&mut hs);
        let mut c1 = vec![0u8; HANDSHAKE_SIZE];
        c1[..4].copy_from_slice(&0x1234_5678u32.to_be_bytes());
        c1[4..8].copy_from_slice(&1u32.to_be_bytes());
        let mut buf = Buffer::new();
        buf.write(&c1).unwrap();
        server_read_c1(&mut hs, &mut buf).unwrap();
        assert_eq!(hs.state, HandshakeState::ServerWaitC2);
        assert!(hs.out.available() >= HANDSHAKE_SIZE * 2);
    }

    #[test]
    fn c2_echoes_s1_time1_and_carries_client_time_in_time2() {
        let mut client = Handshake::default();
        client_init(&mut client);
        let mut s1 = vec![0u8; HANDSHAKE_SIZE];
        s1[..4].copy_from_slice(&0x0BAD_F00Du32.to_be_bytes());
        s1[8..].copy_from_slice(&[0xCD; HANDSHAKE_SIZE - 8]);
        let mut buf = Buffer::new();
        buf.write(&s1).unwrap();

        let before = get_time();
        client_read_s1(&mut client, &mut buf).unwrap();
        let after = get_time();

        let c2 = client.out.peek();
        let time1 = u32::from_be_bytes([c2[0], c2[1], c2[2], c2[3]]);
        let time2 = u32::from_be_bytes([c2[4], c2[5], c2[6], c2[7]]);
        assert_eq!(time1, 0x0BAD_F00D, "C2 time1 must echo S1's time1");
        assert!(
            time2 >= before && time2 <= after,
            "C2 time2 must be the client's own time"
        );
        assert_eq!(
            &c2[8..],
            &s1[8..],
            "C2 random bytes must echo S1's random bytes"
        );
    }
}
