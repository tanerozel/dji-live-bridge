//! Connection state machine helpers
//!
//! Mirrors `src/session/state_machine.h` and `src/session/state_machine.c`.

use crate::types::{ConnState, ErrorCode, Result};

static STATE_NAMES: &[&str] = &[
    "TCP_ACCEPTED",
    "HANDSHAKE",
    "CONNECTED",
    "CAPS_NEGOTIATED",
    "APP_CONNECTED",
    "STREAM_CREATED",
    "PUBLISHING",
    "PLAYING",
    "CLOSING",
    "CLOSED",
];

/// Attempt a state transition. Backward transitions are rejected.
pub fn conn_transition(current: &mut ConnState, new_state: ConnState) -> Result<()> {
    if new_state < *current {
        return Err(ErrorCode::Protocol);
    }
    *current = new_state;
    Ok(())
}

/// Get the string name of a connection state.
pub fn conn_state_str(state: ConnState) -> &'static str {
    STATE_NAMES
        .get(state as usize)
        .copied()
        .unwrap_or("UNKNOWN")
}
