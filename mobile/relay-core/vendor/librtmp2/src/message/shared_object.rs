//! RTMP Shared Object message (type `0x10` AMF3 / `0x13` AMF0) envelope
//! parsing and writing.
//!
//! Mirrors the Adobe RTMP 1.0 specification §7.1 "Shared Object Message":
//! a UTF-8 name, a version, a persistence-flag word, a reserved word, then a
//! stream of events (`type: UI8`, `size: UI32`, `data: <size> bytes`). Event
//! payload contents depend on `event_type` and are left as opaque bytes here
//! — this layer only frames the envelope; interpreting `Change`/`SendMessage`
//! payloads as AMF values is left to callers that need shared-object
//! semantics, consistent with this crate's "deliver the event, not the
//! policy" design (see `CLAUDE.md`).

use crate::buffer::Buffer;
use crate::types::{ErrorCode, Result};

/// Maximum shared-object name length accepted on read.
const MAX_SO_NAME_BYTES: usize = 256;
/// Maximum events accepted in one shared-object message.
const MAX_SO_EVENTS: usize = 256;
/// Maximum bytes accepted for a single event's data blob.
const MAX_SO_EVENT_DATA_BYTES: usize = 64 * 1024;

/// Shared object event type, per the Adobe RTMP 1.0 specification §7.1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SharedObjectEventType {
    /// Client requests to start using (subscribe to) the shared object.
    Use,
    /// Client requests to stop using the shared object.
    Release,
    /// Client requests an attribute change.
    RequestChange,
    /// Server broadcasts an attribute change to subscribers.
    Change,
    /// Server confirms an attribute change to the requester.
    Success,
    /// An RPC-style message sent through the shared object.
    SendMessage,
    /// Server status/error report for a shared-object operation.
    Status,
    /// All attributes were cleared.
    Clear,
    /// An attribute was removed.
    Remove,
    /// Client requests an attribute removal.
    RequestRemove,
    /// Server confirms a successful `Use`.
    UseSuccess,
    /// A type byte this implementation does not recognize. Preserved as-is
    /// so callers can still forward/inspect it instead of the message being
    /// rejected outright (mirrors this crate's ModEx unknown-type handling).
    Unknown(u8),
}

impl SharedObjectEventType {
    fn from_byte(b: u8) -> Self {
        match b {
            1 => Self::Use,
            2 => Self::Release,
            3 => Self::RequestChange,
            4 => Self::Change,
            5 => Self::Success,
            6 => Self::SendMessage,
            7 => Self::Status,
            8 => Self::Clear,
            9 => Self::Remove,
            10 => Self::RequestRemove,
            11 => Self::UseSuccess,
            other => Self::Unknown(other),
        }
    }

    fn to_byte(self) -> u8 {
        match self {
            Self::Use => 1,
            Self::Release => 2,
            Self::RequestChange => 3,
            Self::Change => 4,
            Self::Success => 5,
            Self::SendMessage => 6,
            Self::Status => 7,
            Self::Clear => 8,
            Self::Remove => 9,
            Self::RequestRemove => 10,
            Self::UseSuccess => 11,
            Self::Unknown(b) => b,
        }
    }
}

/// One event inside a shared-object message body.
#[derive(Debug, Clone)]
pub struct SharedObjectEvent {
    pub event_type: SharedObjectEventType,
    /// Event payload; format depends on `event_type` (e.g. an AMF-encoded
    /// name/value pair for `Change`/`RequestChange`/`Success`, a list of AMF
    /// values for `SendMessage`, empty for `Use`/`Release`/`Clear`/`UseSuccess`).
    pub data: Vec<u8>,
}

/// A parsed shared-object message body (name/version/flags header + events).
#[derive(Debug, Clone, Default)]
pub struct SharedObjectMessage {
    pub name: String,
    pub version: u32,
    /// Persistence/type flags, passed through unmodified. [`is_persistent`]
    /// reads bit 0; that specific bit position has not been independently
    /// verified against a real encoder (see "Known Limitations" in
    /// `docs/protocol-mapping-ertmp-v1.md`) -- callers that need the exact
    /// semantics should also inspect the raw value themselves.
    ///
    /// [`is_persistent`]: SharedObjectMessage::is_persistent
    pub flags: u32,
    pub events: Vec<SharedObjectEvent>,
}

impl SharedObjectMessage {
    pub fn is_persistent(&self) -> bool {
        self.flags & 0x01 != 0
    }
}

/// Parse a shared-object message body (the RTMP message payload for type
/// `0x10`/`0x13`, with any leading AMF3 `0x00` marker byte already stripped
/// by the caller). Every length field is bounds-checked against the
/// remaining buffer before use, per this crate's parser-safety rule: unknown
/// or malformed input is rejected, never trusted.
pub fn parse(data: &[u8]) -> Result<SharedObjectMessage> {
    let mut buf = Buffer::from_slice(data);

    let name_len = read_u16(&mut buf)? as usize;
    if name_len > MAX_SO_NAME_BYTES || buf.available() < name_len {
        return Err(ErrorCode::Amf);
    }
    let mut name_bytes = vec![0u8; name_len];
    buf.read(&mut name_bytes).map_err(|_| ErrorCode::Amf)?;
    // Reject invalid UTF-8 outright rather than lossily substituting U+FFFD:
    // distinct wire names must not collapse onto the same `name` value, since
    // hosts use it for their own attribute-sync/persistence policy.
    let name = String::from_utf8(name_bytes).map_err(|_| ErrorCode::Amf)?;

    let version = read_u32(&mut buf)?;
    let flags = read_u32(&mut buf)?;
    let _reserved = read_u32(&mut buf)?;

    let mut events = Vec::new();
    while buf.available() > 0 {
        if events.len() >= MAX_SO_EVENTS {
            return Err(ErrorCode::Amf);
        }
        let mut ty_byte = [0u8; 1];
        buf.read(&mut ty_byte).map_err(|_| ErrorCode::Amf)?;
        let event_len = read_u32(&mut buf)? as usize;
        if event_len > MAX_SO_EVENT_DATA_BYTES || buf.available() < event_len {
            return Err(ErrorCode::Amf);
        }
        let mut event_data = vec![0u8; event_len];
        buf.read(&mut event_data).map_err(|_| ErrorCode::Amf)?;
        events.push(SharedObjectEvent {
            event_type: SharedObjectEventType::from_byte(ty_byte[0]),
            data: event_data,
        });
    }

    Ok(SharedObjectMessage {
        name,
        version,
        flags,
        events,
    })
}

/// Write a shared-object message body (mirrors [`parse`]). Rejects a message
/// `parse` could not have produced (oversized name, too many events, an
/// oversized event payload) so a round trip through `write` -> `parse` never
/// silently drops data a receiver would otherwise reject or truncate.
pub fn write(msg: &SharedObjectMessage, buf: &mut Buffer) -> Result<()> {
    if msg.name.len() > MAX_SO_NAME_BYTES {
        return Err(ErrorCode::Amf);
    }
    if msg.events.len() > MAX_SO_EVENTS {
        return Err(ErrorCode::Amf);
    }
    buf.write(&(msg.name.len() as u16).to_be_bytes())
        .map_err(|_| ErrorCode::Internal)?;
    buf.write(msg.name.as_bytes())
        .map_err(|_| ErrorCode::Internal)?;
    buf.write(&msg.version.to_be_bytes())
        .map_err(|_| ErrorCode::Internal)?;
    buf.write(&msg.flags.to_be_bytes())
        .map_err(|_| ErrorCode::Internal)?;
    buf.write(&0u32.to_be_bytes())
        .map_err(|_| ErrorCode::Internal)?; // reserved

    for event in &msg.events {
        if event.data.len() > MAX_SO_EVENT_DATA_BYTES {
            return Err(ErrorCode::Amf);
        }
        buf.write(&[event.event_type.to_byte()])
            .map_err(|_| ErrorCode::Internal)?;
        buf.write(&(event.data.len() as u32).to_be_bytes())
            .map_err(|_| ErrorCode::Internal)?;
        buf.write(&event.data).map_err(|_| ErrorCode::Internal)?;
    }
    Ok(())
}

fn read_u16(buf: &mut Buffer) -> Result<u16> {
    let mut b = [0u8; 2];
    buf.read(&mut b).map_err(|_| ErrorCode::Amf)?;
    Ok(u16::from_be_bytes(b))
}

fn read_u32(buf: &mut Buffer) -> Result<u32> {
    let mut b = [0u8; 4];
    buf.read(&mut b).map_err(|_| ErrorCode::Amf)?;
    Ok(u32::from_be_bytes(b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_header_with_no_events() {
        let msg = SharedObjectMessage {
            name: "chat".to_string(),
            version: 1,
            flags: 0x01,
            events: Vec::new(),
        };
        let mut buf = Buffer::new();
        write(&msg, &mut buf).unwrap();

        let parsed = parse(buf.peek()).unwrap();
        assert_eq!(parsed.name, "chat");
        assert_eq!(parsed.version, 1);
        assert!(parsed.is_persistent());
        assert!(parsed.events.is_empty());
    }

    #[test]
    fn round_trips_events_with_known_and_unknown_types() {
        let msg = SharedObjectMessage {
            name: "scoreboard".to_string(),
            version: 3,
            flags: 0,
            events: vec![
                SharedObjectEvent {
                    event_type: SharedObjectEventType::Use,
                    data: Vec::new(),
                },
                SharedObjectEvent {
                    event_type: SharedObjectEventType::Change,
                    data: vec![0xDE, 0xAD, 0xBE, 0xEF],
                },
                SharedObjectEvent {
                    event_type: SharedObjectEventType::Unknown(200),
                    data: vec![1, 2, 3],
                },
            ],
        };
        let mut buf = Buffer::new();
        write(&msg, &mut buf).unwrap();

        let parsed = parse(buf.peek()).unwrap();
        assert_eq!(parsed.events.len(), 3);
        assert_eq!(parsed.events[0].event_type, SharedObjectEventType::Use);
        assert!(parsed.events[0].data.is_empty());
        assert_eq!(parsed.events[1].event_type, SharedObjectEventType::Change);
        assert_eq!(parsed.events[1].data, vec![0xDE, 0xAD, 0xBE, 0xEF]);
        assert_eq!(
            parsed.events[2].event_type,
            SharedObjectEventType::Unknown(200)
        );
        assert_eq!(parsed.events[2].data, vec![1, 2, 3]);
    }

    #[test]
    fn rejects_name_length_exceeding_remaining_buffer() {
        let mut buf = Buffer::new();
        buf.write(&500u16.to_be_bytes()).unwrap();
        buf.write(b"short").unwrap();
        assert!(parse(buf.peek()).is_err());
    }

    #[test]
    fn rejects_event_length_exceeding_remaining_buffer() {
        let mut buf = Buffer::new();
        buf.write(&4u16.to_be_bytes()).unwrap();
        buf.write(b"test").unwrap();
        buf.write(&1u32.to_be_bytes()).unwrap(); // version
        buf.write(&0u32.to_be_bytes()).unwrap(); // flags
        buf.write(&0u32.to_be_bytes()).unwrap(); // reserved
        buf.write(&[SharedObjectEventType::Change.to_byte()])
            .unwrap();
        buf.write(&1_000_000u32.to_be_bytes()).unwrap(); // bogus oversized length
        assert!(parse(buf.peek()).is_err());
    }

    #[test]
    fn rejects_truncated_header() {
        let mut buf = Buffer::new();
        buf.write(&4u16.to_be_bytes()).unwrap();
        buf.write(b"test").unwrap();
        // Missing version/flags/reserved.
        assert!(parse(buf.peek()).is_err());
    }

    #[test]
    fn rejects_invalid_utf8_name_instead_of_substituting() {
        let mut buf = Buffer::new();
        let invalid_name = [0xFFu8, 0xFE];
        buf.write(&(invalid_name.len() as u16).to_be_bytes())
            .unwrap();
        buf.write(&invalid_name).unwrap();
        buf.write(&0u32.to_be_bytes()).unwrap(); // version
        buf.write(&0u32.to_be_bytes()).unwrap(); // flags
        buf.write(&0u32.to_be_bytes()).unwrap(); // reserved
        assert!(parse(buf.peek()).is_err());
    }

    #[test]
    fn write_rejects_more_events_than_parse_would_accept() {
        let events = (0..(MAX_SO_EVENTS + 1))
            .map(|_| SharedObjectEvent {
                event_type: SharedObjectEventType::Use,
                data: Vec::new(),
            })
            .collect();
        let msg = SharedObjectMessage {
            name: "chat".to_string(),
            version: 1,
            flags: 0,
            events,
        };
        let mut buf = Buffer::new();
        assert!(write(&msg, &mut buf).is_err());
    }
}
