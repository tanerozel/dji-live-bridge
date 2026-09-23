//! Plaintext + optional TLS byte transport.
//!
//! Mirrors `src/core/transport.h` and `src/core/transport.c`.
//!
//! The plaintext path is always available. The TLS path is feature-gated
//! behind the "tls" feature (OpenSSL).

use crate::types::ErrorCode;
use crate::types::Result;

#[cfg(feature = "tls")]
use openssl::ssl::{
    HandshakeError, MidHandshakeSslStream, SslAcceptor, SslFiletype, SslMethod, SslStream,
    SslVerifyMode,
};
use std::net::TcpStream;
#[cfg(feature = "tls")]
use std::os::unix::io::{AsRawFd, FromRawFd};
#[cfg(feature = "tls")]
use std::sync::Arc;
#[cfg(feature = "tls")]
use std::time::{Duration, Instant};

#[cfg(feature = "tls")]
const TLS_ACCEPT_TIMEOUT_SECS: u64 = 10;

enum TransportInner {
    Plain(i32),
    #[cfg(feature = "tls")]
    Tls {
        stream: SslStream<TcpStream>,
        /// Cached raw fd, used only for identification and `fd()` / `poll()`.
        fd: i32,
    },
}

/// Transport wraps a connected socket fd and presents a single send/recv API.
///
/// The transport OWNS the file descriptor: it is closed when the transport
/// is dropped (plain: explicit `close(2)`; TLS: via `TcpStream` drop inside
/// the SSL stream).
pub struct Transport {
    inner: TransportInner,
}

/// Server-side TLS context: holds the validated SSL acceptor shared across
/// connections.
///
/// Cheaply `Clone`: it's just an `Arc` bump, so a `Server` with multiple TLS
/// listeners can hand each accepted connection its own owned handle without
/// re-validating the certificate/key per listener.
#[derive(Clone)]
pub struct TlsCtx {
    #[cfg(feature = "tls")]
    pub(crate) acceptor: Arc<SslAcceptor>,
}

/// A TLS handshake that could not complete immediately on a non-blocking
/// accepted socket. The server keeps these between `poll()` calls so one slow
/// RTMPS peer cannot block accepting plaintext peers or processing sessions.
#[cfg(feature = "tls")]
pub(crate) struct PendingTlsAccept {
    stream: MidHandshakeSslStream<TcpStream>,
    fd: i32,
    interest: TlsPollInterest,
}

#[cfg(feature = "tls")]
#[derive(Clone, Copy)]
enum TlsPollInterest {
    Read,
    Write,
}

#[cfg(feature = "tls")]
impl TlsPollInterest {
    fn poll_events(self) -> libc::c_short {
        match self {
            Self::Read => libc::POLLIN,
            Self::Write => libc::POLLOUT,
        }
    }
}

#[cfg(feature = "tls")]
fn tls_poll_interest(stream: &MidHandshakeSslStream<TcpStream>) -> TlsPollInterest {
    use openssl::ssl::ErrorCode as SslErr;
    match stream.error().code() {
        SslErr::WANT_WRITE => TlsPollInterest::Write,
        _ => TlsPollInterest::Read,
    }
}

#[cfg(feature = "tls")]
pub(crate) enum TlsAcceptOutcome {
    Complete(Transport),
    WouldBlock(PendingTlsAccept),
}

fn last_errno() -> i32 {
    std::io::Error::last_os_error().raw_os_error().unwrap_or(0)
}

impl Transport {
    /// Wrap an owned fd as a plaintext transport.
    pub fn new_plain(fd: i32) -> Self {
        Self {
            inner: TransportInner::Plain(fd),
        }
    }

    #[cfg(feature = "tls")]
    fn new_tls(stream: SslStream<TcpStream>) -> Result<Self> {
        // OpenSSL's write path doesn't set MSG_NOSIGNAL the way the plaintext
        // path does, so a peer resetting mid-write can raise SIGPIPE and kill
        // the whole host process. Ignore it once, process-wide: RTMP(S)
        // connections always report broken peers via EPIPE/an OpenSSL error
        // return, so the signal itself carries no information we need.
        //
        // Only do this if the disposition is still the default: an embedding
        // application that already installed its own SIGPIPE handler (or
        // explicitly ignored it) made that choice deliberately, and we
        // shouldn't clobber it.
        //
        // Note for embedders: SIG_IGN is inherited across fork/exec. A host
        // process that forks child processes after opening a TLS connection
        // through this library, and that relies on the default SIGPIPE
        // behavior in those children, will need to restore SIG_DFL itself.
        static SET_SIGPIPE_DISPOSITION: std::sync::Once = std::sync::Once::new();
        SET_SIGPIPE_DISPOSITION.call_once(|| unsafe {
            let current = libc::signal(libc::SIGPIPE, libc::SIG_IGN);
            if current != libc::SIG_DFL && current != libc::SIG_ERR {
                libc::signal(libc::SIGPIPE, current);
            }
        });

        let raw_fd = stream.get_ref().as_raw_fd();
        stream
            .get_ref()
            .set_nonblocking(true)
            .map_err(|_| ErrorCode::Io)?;
        Ok(Self {
            inner: TransportInner::Tls { stream, fd: raw_fd },
        })
    }

    /// Perform a blocking TLS client handshake over an already-connected
    /// `stream` (RTMPS). By default validates the server certificate against
    /// the system trust store and checks `host` against the certificate (SNI
    /// + hostname verification), matching standard TLS client behavior.
    ///
    /// - `ca_file` (PEM bundle): verification trusts *only* the CAs in this
    ///   bundle, replacing the system trust store rather than adding to it —
    ///   so a caller pinning a private CA doesn't also accept publicly
    ///   trusted certificates for the same hostname.
    /// - `insecure = true`: skip certificate verification entirely (only for
    ///   testing against self-signed deployments; never use in production).
    ///   The system default verify paths are never touched in this mode, so
    ///   a host with no usable default CA store still connects.
    ///
    /// `stream` must not already be set non-blocking; this performs a
    /// synchronous handshake, matching [`Client`](crate::client::Client)'s
    /// otherwise-blocking connect sequence. The handshake itself is bounded by
    /// a read/write timeout so a peer that completes the TCP connect but then
    /// stalls mid-handshake cannot hang the caller indefinitely (mirroring the
    /// bound `send()` places on writes post-handshake). Uses a fixed default
    /// timeout; call [`Transport::connect_tls_with_timeout`] to bound it by
    /// an existing deadline instead.
    #[cfg(feature = "tls")]
    pub fn connect_tls(
        stream: TcpStream,
        host: &str,
        ca_file: Option<&str>,
        insecure: bool,
    ) -> Result<Self> {
        Transport::connect_tls_with_timeout(
            stream,
            host,
            ca_file,
            insecure,
            Duration::from_secs(TLS_ACCEPT_TIMEOUT_SECS),
        )
    }

    /// Same as [`Transport::connect_tls`], but `timeout` bounds the whole
    /// handshake instead of the fixed default. Pass the caller's remaining
    /// connect deadline here — otherwise the TLS handshake could run well
    /// past the overall connect timeout the caller already spent on DNS/TCP
    /// connect.
    #[cfg(feature = "tls")]
    pub fn connect_tls_with_timeout(
        stream: TcpStream,
        host: &str,
        ca_file: Option<&str>,
        insecure: bool,
        timeout: Duration,
    ) -> Result<Self> {
        use openssl::ssl::{Ssl, SslContextBuilder, SslMode};
        use openssl::x509::X509;
        use openssl::x509::store::X509StoreBuilder;
        use openssl::x509::verify::X509CheckFlags;

        if timeout.is_zero() {
            return Err(ErrorCode::Timeout);
        }
        // A single fixed read/write socket timeout only bounds each
        // individual I/O call, not the handshake as a whole — a peer that
        // drip-feeds one byte just before each timeout could stall the
        // handshake far past `timeout`. Drive the handshake non-blocking
        // instead (mirroring the server-side `accept_nonblocking` pattern)
        // and poll against one absolute deadline so the wall-clock budget
        // can't be reset by partial I/O.
        let deadline = Instant::now()
            .checked_add(timeout)
            .ok_or(ErrorCode::Internal)?;
        stream.set_nonblocking(true).map_err(|_| ErrorCode::Io)?;

        let mut ctx = SslContextBuilder::new(SslMethod::tls()).map_err(|_| ErrorCode::Internal)?;
        ctx.set_cipher_list(
            "DEFAULT:!aNULL:!eNULL:!MD5:!3DES:!DES:!RC4:!IDEA:!SEED:!aDSS:!SRP:!PSK",
        )
        .map_err(|_| ErrorCode::Internal)?;
        // The non-blocking publish path (Client::try_flush_send_buffer) may
        // retry a WANT_READ/WANT_WRITE SSL_write() with a send_buffer that
        // has grown (a new frame appended after the pending one) since the
        // previous attempt. ACCEPT_MOVING_WRITE_BUFFER permits the retry
        // buffer to differ in location/length as long as the previously
        // unwritten bytes are still present at the front, which holds here
        // since Buffer only ever appends after its unread portion.
        ctx.set_mode(SslMode::ACCEPT_MOVING_WRITE_BUFFER);
        if insecure {
            // Verification is disabled outright, so don't touch the system
            // verify-path configuration at all: a minimal host without a
            // usable default CA store must still be able to connect.
            ctx.set_verify(SslVerifyMode::NONE);
        } else {
            ctx.set_verify(SslVerifyMode::PEER);
            match ca_file {
                Some(ca) => {
                    // Build a replacement trust store containing only the
                    // caller-supplied CA(s), instead of augmenting the
                    // system default store: a custom CA is meant to
                    // restrict verification, not merely extend it.
                    let pem = std::fs::read(ca).map_err(|_| ErrorCode::Internal)?;
                    let certs = X509::stack_from_pem(&pem).map_err(|_| ErrorCode::Internal)?;
                    let mut store = X509StoreBuilder::new().map_err(|_| ErrorCode::Internal)?;
                    for cert in certs {
                        store.add_cert(cert).map_err(|_| ErrorCode::Internal)?;
                    }
                    ctx.set_cert_store(store.build());
                }
                None => {
                    ctx.set_default_verify_paths()
                        .map_err(|_| ErrorCode::Internal)?;
                }
            }
        }
        let ssl_ctx = ctx.build();

        let mut ssl = Ssl::new(&ssl_ctx).map_err(|_| ErrorCode::Internal)?;
        if !insecure {
            let param = ssl.param_mut();
            param.set_hostflags(X509CheckFlags::NO_PARTIAL_WILDCARDS);
            match host.parse() {
                Ok(ip) => param.set_ip(ip).map_err(|_| ErrorCode::Internal)?,
                Err(_) => param.set_host(host).map_err(|_| ErrorCode::Internal)?,
            }
        }
        ssl.set_hostname(host).map_err(|_| ErrorCode::Internal)?;

        let mut pending = match ssl.connect(stream) {
            Ok(ssl_stream) => return Transport::new_tls(ssl_stream),
            Err(HandshakeError::WouldBlock(mid)) => mid,
            Err(_) => return Err(ErrorCode::Handshake),
        };
        loop {
            let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                return Err(ErrorCode::Timeout);
            };
            // `poll(2)`'s granularity is milliseconds; round a sub-ms
            // remainder down to an expired deadline instead of up to a full
            // 1ms wait, so the caller's absolute deadline can't be overshot.
            if remaining.as_millis() == 0 {
                return Err(ErrorCode::Timeout);
            }
            // `poll(2)`'s timeout is a 32-bit millisecond count, so a
            // remaining budget past ~24.8 days must be clamped; `rc == 0`
            // then only means "this clamped wait expired", not "the real
            // deadline passed" — loop and recheck instead of timing out
            // early.
            let timeout_ms = remaining.as_millis().min(i32::MAX as u128) as i32;
            let mut pfd = libc::pollfd {
                fd: pending.get_ref().as_raw_fd(),
                events: tls_poll_interest(&pending).poll_events(),
                revents: 0,
            };
            let rc = unsafe { libc::poll(&mut pfd, 1, timeout_ms) };
            if rc == 0 {
                if Instant::now() >= deadline {
                    return Err(ErrorCode::Timeout);
                }
                continue;
            }
            if rc < 0 {
                if std::io::Error::last_os_error().raw_os_error() == Some(libc::EINTR) {
                    continue;
                }
                return Err(ErrorCode::Io);
            }
            match pending.handshake() {
                Ok(ssl_stream) => return Transport::new_tls(ssl_stream),
                Err(HandshakeError::WouldBlock(mid)) => pending = mid,
                Err(_) => return Err(ErrorCode::Handshake),
            }
        }
    }

    /// TLS is not available in this build.
    #[cfg(not(feature = "tls"))]
    pub fn connect_tls(
        _stream: TcpStream,
        _host: &str,
        _ca_file: Option<&str>,
        _insecure: bool,
    ) -> Result<Self> {
        Err(ErrorCode::Unsupported)
    }

    /// TLS is not available in this build.
    #[cfg(not(feature = "tls"))]
    pub fn connect_tls_with_timeout(
        _stream: TcpStream,
        _host: &str,
        _ca_file: Option<&str>,
        _insecure: bool,
        _timeout: std::time::Duration,
    ) -> Result<Self> {
        Err(ErrorCode::Unsupported)
    }

    /// Return the underlying file descriptor (used for `poll(2)` and as a
    /// connection identifier; I/O is performed through this struct).
    pub fn fd(&self) -> i32 {
        match &self.inner {
            TransportInner::Plain(fd) => *fd,
            #[cfg(feature = "tls")]
            TransportInner::Tls { fd, .. } => *fd,
        }
    }

    /// Check if this transport uses TLS.
    pub fn is_tls(&self) -> bool {
        match &self.inner {
            TransportInner::Plain(_) => false,
            #[cfg(feature = "tls")]
            TransportInner::Tls { .. } => true,
        }
    }

    /// Non-blocking receive.
    ///
    /// Returns the number of bytes read (>0), 0 on clean peer shutdown, or -1
    /// on error. On -1, `again` indicates a transient would-block:
    ///   1 = wait for readable (EAGAIN / TLS WANT_READ)
    ///   2 = wait for writable (TLS WANT_WRITE during a read)
    ///   0 = fatal error.
    pub fn recv(&mut self, buf: &mut [u8], again: &mut i32) -> isize {
        match &mut self.inner {
            TransportInner::Plain(fd) => unsafe {
                let n = libc::recv(
                    *fd,
                    buf.as_mut_ptr() as *mut libc::c_void,
                    buf.len(),
                    libc::MSG_DONTWAIT,
                );
                if n < 0 {
                    let err = last_errno();
                    if err == libc::EINTR || err == libc::EAGAIN || err == libc::EWOULDBLOCK {
                        *again = 1;
                    }
                }
                n as isize
            },
            #[cfg(feature = "tls")]
            TransportInner::Tls { stream, .. } => {
                use openssl::ssl::ErrorCode as SslErr;
                match stream.ssl_read(buf) {
                    Ok(n) => n as isize,
                    Err(e) => match e.code() {
                        SslErr::WANT_READ => {
                            *again = 1;
                            -1
                        }
                        SslErr::WANT_WRITE => {
                            *again = 2;
                            -1
                        }
                        SslErr::ZERO_RETURN => 0,
                        _ => -1,
                    },
                }
            }
        }
    }

    /// Non-blocking send. Returns bytes written, or 0 when the socket is not
    /// ready. On `Ok(0)`, `again` is set to indicate the poll direction:
    ///   1 = wait for readable (TLS WANT_READ during write, e.g. renegotiation)
    ///   2 = wait for writable (EAGAIN/EWOULDBLOCK/EINTR/TLS WANT_WRITE)
    /// Used by the server poll loop so one slow peer cannot stall all connections.
    pub fn try_send(&mut self, data: &[u8], again: &mut i32) -> Result<usize> {
        if data.is_empty() {
            return Ok(0);
        }
        match &mut self.inner {
            TransportInner::Plain(fd) => {
                let n = unsafe {
                    libc::send(
                        *fd,
                        data.as_ptr() as *const libc::c_void,
                        data.len(),
                        libc::MSG_DONTWAIT | libc::MSG_NOSIGNAL,
                    )
                };
                if n < 0 {
                    let err = last_errno();
                    if err == libc::EINTR || err == libc::EAGAIN || err == libc::EWOULDBLOCK {
                        *again = 2;
                        return Ok(0);
                    }
                    return Err(ErrorCode::Io);
                }
                Ok(n as usize)
            }
            #[cfg(feature = "tls")]
            TransportInner::Tls { stream, .. } => {
                use openssl::ssl::ErrorCode as SslErr;
                match stream.ssl_write(data) {
                    Ok(n) => Ok(n),
                    Err(e) => match e.code() {
                        SslErr::WANT_WRITE => {
                            *again = 2;
                            Ok(0)
                        }
                        // TLS renegotiation: ssl_write needs read readiness.
                        SslErr::WANT_READ => {
                            *again = 1;
                            Ok(0)
                        }
                        _ => Err(ErrorCode::Io),
                    },
                }
            }
        }
    }

    /// Blocking send of the whole buffer (client-side synchronous I/O).
    ///
    /// Uses a 10-second poll timeout rather than an infinite wait so a peer
    /// that stops reading cannot block the caller indefinitely. Correctly
    /// handles TLS WANT_READ during writes (e.g. renegotiation) by polling
    /// for read readiness instead of write readiness.
    pub fn send(&mut self, data: &[u8]) -> Result<()> {
        let mut sent = 0;
        while sent < data.len() {
            let mut again = 0i32;
            let n = self.try_send(&data[sent..], &mut again)?;
            if n == 0 {
                let fd = self.fd();
                let events = if again == 1 {
                    libc::POLLIN
                } else {
                    libc::POLLOUT
                };
                let mut pfd = libc::pollfd {
                    fd,
                    events,
                    revents: 0,
                };
                let rc = unsafe { libc::poll(&mut pfd, 1, 10_000) };
                if rc == 0 {
                    return Err(ErrorCode::Timeout);
                }
                if rc < 0 {
                    return Err(ErrorCode::Io);
                }
                continue;
            }
            sent += n;
        }
        Ok(())
    }

    /// Number of decrypted bytes already buffered inside the transport (always
    /// 0 for plaintext). For TLS, `SSL_pending()` reports application data
    /// OpenSSL has already decrypted but the caller hasn't read yet -- data a
    /// `poll(2)` readiness check on the raw fd cannot see, since the kernel
    /// socket buffer it was decrypted from may already be empty.
    pub fn pending(&self) -> i32 {
        match &self.inner {
            TransportInner::Plain(_) => 0,
            #[cfg(feature = "tls")]
            TransportInner::Tls { stream, .. } => stream.ssl().pending() as i32,
        }
    }
}

impl Drop for Transport {
    fn drop(&mut self) {
        // For plain transports, explicitly close the owned fd.
        // For TLS transports, SslStream<TcpStream> closes the fd when it drops.
        match &self.inner {
            TransportInner::Plain(fd) => {
                if *fd >= 0 {
                    unsafe { libc::close(*fd) };
                }
            }
            #[cfg(feature = "tls")]
            TransportInner::Tls { .. } => {}
        }
    }
}

impl TlsCtx {
    /// Build a server TLS context from PEM cert-chain and private-key files.
    ///
    /// Validates the certificate and private key immediately; returns an error
    /// if the files cannot be read, are malformed, or the key doesn't match the
    /// certificate.
    #[cfg(feature = "tls")]
    pub fn new_server(cert_file: &str, key_file: &str) -> Result<Self> {
        use openssl::ssl::SslMode;

        let mut builder =
            SslAcceptor::mozilla_intermediate(SslMethod::tls()).map_err(|_| ErrorCode::Internal)?;
        // Non-blocking sends (Conn::flush) may retry a WANT_READ/WANT_WRITE
        // SSL_write() with a send_buffer that grew (more queued output
        // appended) since the previous attempt; see the matching client-side
        // comment in Transport::connect_tls.
        builder.set_mode(SslMode::ACCEPT_MOVING_WRITE_BUFFER);
        builder
            .set_certificate_chain_file(cert_file)
            .map_err(|_| ErrorCode::Internal)?;
        builder
            .set_private_key_file(key_file, SslFiletype::PEM)
            .map_err(|_| ErrorCode::Internal)?;
        builder
            .check_private_key()
            .map_err(|_| ErrorCode::Internal)?;
        Ok(Self {
            acceptor: Arc::new(builder.build()),
        })
    }

    /// TLS is not available in this build.
    #[cfg(not(feature = "tls"))]
    pub fn new_server(_cert_file: &str, _key_file: &str) -> Result<Self> {
        Err(ErrorCode::Unsupported)
    }

    /// Begin or finish a TLS server handshake without blocking the caller.
    ///
    /// If the peer has not provided enough handshake data yet, the returned
    /// [`PendingTlsAccept`] can be stored and retried on a later `poll()` call.
    #[cfg(feature = "tls")]
    pub(crate) fn accept_nonblocking(&self, fd: i32) -> Result<TlsAcceptOutcome> {
        let tcp = unsafe { TcpStream::from_raw_fd(fd) };
        tcp.set_nonblocking(true).map_err(|_| ErrorCode::Io)?;
        match self.acceptor.accept(tcp) {
            Ok(ssl) => Ok(TlsAcceptOutcome::Complete(Transport::new_tls(ssl)?)),
            Err(HandshakeError::WouldBlock(stream)) => {
                let interest = tls_poll_interest(&stream);
                Ok(TlsAcceptOutcome::WouldBlock(PendingTlsAccept {
                    stream,
                    fd,
                    interest,
                }))
            }
            Err(_) => Err(ErrorCode::Handshake),
        }
    }

    /// Perform a TLS server handshake on the given fd and return a TLS
    /// [`Transport`] that owns the fd.
    ///
    /// This convenience helper may block for up to 10 seconds while waiting for
    /// handshake readiness. The server's accept loop uses `accept_nonblocking`
    /// instead so one stalled RTMPS peer cannot freeze other listeners.
    ///
    /// On failure the fd is closed (via the dropped `TcpStream` inside the
    /// error value) — the caller must not close it again.
    #[cfg(feature = "tls")]
    pub fn accept(&self, fd: i32) -> Result<Transport> {
        match self.accept_nonblocking(fd)? {
            TlsAcceptOutcome::Complete(transport) => Ok(transport),
            TlsAcceptOutcome::WouldBlock(mut pending) => {
                let deadline = Instant::now() + Duration::from_secs(TLS_ACCEPT_TIMEOUT_SECS);
                loop {
                    let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                        return Err(ErrorCode::Timeout);
                    };
                    let timeout_ms = remaining.as_millis().min(i32::MAX as u128) as i32;
                    let timeout_ms = timeout_ms.max(1);
                    let mut pfd = libc::pollfd {
                        fd: pending.fd(),
                        events: pending.poll_events(),
                        revents: 0,
                    };
                    let rc = unsafe { libc::poll(&mut pfd, 1, timeout_ms) };
                    if rc == 0 {
                        return Err(ErrorCode::Timeout);
                    }
                    if rc < 0 {
                        return Err(ErrorCode::Io);
                    }
                    match pending.progress()? {
                        TlsAcceptOutcome::Complete(transport) => return Ok(transport),
                        TlsAcceptOutcome::WouldBlock(next) => pending = next,
                    }
                }
            }
        }
    }

    #[cfg(not(feature = "tls"))]
    pub fn accept(&self, _fd: i32) -> Result<Transport> {
        Err(ErrorCode::Unsupported)
    }
}

#[cfg(feature = "tls")]
impl PendingTlsAccept {
    pub(crate) fn fd(&self) -> i32 {
        self.fd
    }

    pub(crate) fn poll_events(&self) -> libc::c_short {
        self.interest.poll_events()
    }

    pub(crate) fn progress(self) -> Result<TlsAcceptOutcome> {
        let fd = self.fd;
        match self.stream.handshake() {
            Ok(ssl) => Ok(TlsAcceptOutcome::Complete(Transport::new_tls(ssl)?)),
            Err(HandshakeError::WouldBlock(stream)) => {
                let interest = tls_poll_interest(&stream);
                Ok(TlsAcceptOutcome::WouldBlock(PendingTlsAccept {
                    stream,
                    fd,
                    interest,
                }))
            }
            Err(_) => Err(ErrorCode::Handshake),
        }
    }
}

/// Check if TLS support is available.
pub fn tls_available() -> bool {
    cfg!(feature = "tls")
}

#[cfg(all(test, feature = "tls"))]
mod client_tls_tests {
    use super::*;
    use openssl::asn1::Asn1Time;
    use openssl::bn::{BigNum, MsbOption};
    use openssl::hash::MessageDigest;
    use openssl::pkey::PKey;
    use openssl::rsa::Rsa;
    use openssl::x509::extension::{BasicConstraints, SubjectAlternativeName};
    use openssl::x509::{X509, X509NameBuilder};
    use std::net::TcpListener;
    use std::os::unix::io::IntoRawFd;
    use std::sync::atomic::{AtomicU32, Ordering};

    /// Generates a self-signed cert/key pair (PEM) for `cn`, written to
    /// unique temp files so `TlsCtx::new_server`/`connect_tls` (both of
    /// which take file paths) can consume them. Returns the paths; callers
    /// should remove them when done.
    fn self_signed_cert_files(cn: &str) -> (std::path::PathBuf, std::path::PathBuf) {
        let rsa = Rsa::generate(2048).unwrap();
        let pkey = PKey::from_rsa(rsa).unwrap();

        let mut name = X509NameBuilder::new().unwrap();
        name.append_entry_by_text("CN", cn).unwrap();
        let name = name.build();

        let mut builder = X509::builder().unwrap();
        builder.set_version(2).unwrap();
        let mut sn = BigNum::new().unwrap();
        sn.rand(64, MsbOption::MAYBE_ZERO, false).unwrap();
        builder
            .set_serial_number(&sn.to_asn1_integer().unwrap())
            .unwrap();
        builder.set_subject_name(&name).unwrap();
        builder.set_issuer_name(&name).unwrap();
        builder.set_pubkey(&pkey).unwrap();
        builder
            .set_not_before(&Asn1Time::days_from_now(0).unwrap())
            .unwrap();
        builder
            .set_not_after(&Asn1Time::days_from_now(1).unwrap())
            .unwrap();
        builder
            .append_extension(BasicConstraints::new().critical().ca().build().unwrap())
            .unwrap();
        let san = SubjectAlternativeName::new()
            .dns(cn)
            .build(&builder.x509v3_context(None, None))
            .unwrap();
        builder.append_extension(san).unwrap();
        builder.sign(&pkey, MessageDigest::sha256()).unwrap();
        let cert = builder.build();

        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let base =
            std::env::temp_dir().join(format!("librtmp2-test-{}-{}-{}", std::process::id(), n, cn));
        let cert_path = base.with_extension("cert.pem");
        let key_path = base.with_extension("key.pem");
        std::fs::write(&cert_path, cert.to_pem().unwrap()).unwrap();
        std::fs::write(&key_path, pkey.private_key_to_pem_pkcs8().unwrap()).unwrap();
        (cert_path, key_path)
    }

    /// Starts a one-shot TLS echo-free accept-and-drop server on `cert_file`
    /// / `key_file`, returning its port. The handshake runs on a background
    /// thread; the caller is responsible for connecting exactly once.
    fn spawn_tls_server(cert_file: std::path::PathBuf, key_file: std::path::PathBuf) -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let ctx = TlsCtx::new_server(cert_file.to_str().unwrap(), key_file.to_str().unwrap())
            .expect("valid self-signed cert/key");
        std::thread::spawn(move || {
            if let Ok((stream, _)) = listener.accept() {
                let fd = stream.into_raw_fd();
                // Blocking accept: run the handshake to completion, ignoring
                // the outcome — the test only cares whether the *client*
                // observed success/failure.
                let _ = ctx.accept(fd);
            }
        });
        port
    }

    #[test]
    fn connect_tls_insecure_accepts_self_signed_cert_without_default_verify_paths() {
        let (cert_path, key_path) = self_signed_cert_files("insecure.test");
        let port = spawn_tls_server(cert_path.clone(), key_path.clone());

        let stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        let result = Transport::connect_tls(stream, "insecure.test", None, true);
        assert!(
            result.is_ok(),
            "insecure connect should succeed: {:?}",
            result.err()
        );

        let _ = std::fs::remove_file(cert_path);
        let _ = std::fs::remove_file(key_path);
    }

    #[test]
    fn connect_tls_with_matching_ca_file_trusts_self_signed_cert() {
        let (cert_path, key_path) = self_signed_cert_files("matching-ca.test");
        let port = spawn_tls_server(cert_path.clone(), key_path.clone());

        let stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        let result = Transport::connect_tls(
            stream,
            "matching-ca.test",
            Some(cert_path.to_str().unwrap()),
            false,
        );
        assert!(
            result.is_ok(),
            "connect with the server's own cert as ca_file should succeed: {:?}",
            result.err()
        );

        let _ = std::fs::remove_file(cert_path);
        let _ = std::fs::remove_file(key_path);
    }

    #[test]
    fn connect_tls_with_mismatched_ca_file_rejects_self_signed_cert() {
        let (server_cert, server_key) = self_signed_cert_files("mismatched-server.test");
        let (other_cert, other_key) = self_signed_cert_files("unrelated-ca.test");
        let port = spawn_tls_server(server_cert.clone(), server_key.clone());

        let stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        // `other_cert` is a valid CA file, just not one that issued the
        // server's certificate: verification must fail, proving the
        // replacement trust store isn't silently falling back to trusting
        // everything (or the system store, which wouldn't contain either
        // self-signed cert anyway).
        let result = Transport::connect_tls(
            stream,
            "mismatched-server.test",
            Some(other_cert.to_str().unwrap()),
            false,
        );
        assert_eq!(result.err(), Some(ErrorCode::Handshake));

        let _ = std::fs::remove_file(server_cert);
        let _ = std::fs::remove_file(server_key);
        let _ = std::fs::remove_file(other_cert);
        let _ = std::fs::remove_file(other_key);
    }

    #[test]
    fn connect_tls_default_mode_rejects_untrusted_self_signed_cert() {
        let (cert_path, key_path) = self_signed_cert_files("default-mode.test");
        let port = spawn_tls_server(cert_path.clone(), key_path.clone());

        let stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        // No ca_file, not insecure: a self-signed cert absent from the
        // system trust store must be rejected, same as before this change.
        let result = Transport::connect_tls(stream, "default-mode.test", None, false);
        assert_eq!(result.err(), Some(ErrorCode::Handshake));

        let _ = std::fs::remove_file(cert_path);
        let _ = std::fs::remove_file(key_path);
    }

    #[test]
    fn connect_tls_with_timeout_times_out_against_a_stalled_peer() {
        // Accept the TCP connection but never write a byte of TLS data back:
        // the client's handshake should time out on the supplied deadline
        // rather than hang, and must not take anywhere close to the fixed
        // 10s default (proving the deadline, not the socket default, is
        // what's bounding it).
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let _keep_alive = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            std::thread::sleep(Duration::from_secs(5));
            drop(stream);
        });

        let stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        let start = Instant::now();
        let result = Transport::connect_tls_with_timeout(
            stream,
            "stalled.test",
            None,
            true,
            Duration::from_millis(200),
        );
        let elapsed = start.elapsed();
        assert_eq!(result.err(), Some(ErrorCode::Timeout));
        assert!(
            elapsed < Duration::from_secs(2),
            "expected the ~200ms deadline to bound the handshake, took {:?}",
            elapsed
        );
    }
}
