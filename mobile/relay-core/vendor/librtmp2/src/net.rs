//! Small networking helpers shared by the server (bind) and client (connect) entry points.
//!
//! Mirrors `src/core/net.h` and `src/core/net.c`.

use crate::types::ErrorCode;
use crate::types::Result;

/// Split a "host:port" authority into separate host and port strings.
///
/// Accepts:
/// - "host:port"        -> host, port
/// - "host"             -> host, def_port
/// - "[v6addr]:port"    -> v6addr (brackets stripped), port
/// - "[v6addr]"         -> v6addr, def_port
/// - "fe80::1" / "::"   -> the whole string as host, def_port
/// - ":port"            -> "" (empty host = wildcard), port
///
/// `def_port` is copied into `port` whenever the input carries no port of its own.
/// Returns Ok(()) on success, or an error if a destination buffer is too small
/// or the bracketed form is malformed.
pub fn split_host_port(
    input: &str,
    host: &mut String,
    port: &mut String,
    def_port: &str,
) -> Result<()> {
    port.clear();
    port.push_str(def_port);

    if input.starts_with('[') {
        // Bracketed IPv6 literal: "[addr]" or "[addr]:port"
        let end = input.find(']').ok_or(ErrorCode::Internal)?;
        let addr = &input[1..end];
        *host = addr.to_string();
        let rest = &input[end + 1..];
        if rest.starts_with(':') {
            port.clear();
            port.push_str(&rest[1..]);
        } else if !rest.is_empty() {
            return Err(ErrorCode::Internal);
        }
        return Ok(());
    }

    // Count colons to tell "host:port" apart from a bare IPv6 literal.
    let colons = input.chars().filter(|&c| c == ':').count();

    if colons > 1 {
        // Unbracketed and multi-colon -> a bare IPv6 literal with no port.
        *host = input.to_string();
        return Ok(());
    }

    if colons == 1 {
        let (h, p) = input.split_once(':').unwrap();
        *host = h.to_string();
        port.clear();
        port.push_str(p);
        return Ok(());
    }

    // No colon: host only, default port.
    *host = input.to_string();
    Ok(())
}
