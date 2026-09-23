//! Publish flow
//!
//! Mirrors `src/session/publish.h` and `src/session/publish.c`.

use super::stream::Stream;
use crate::types::Result;

/// Begin publishing on a stream.
pub fn publish_begin(stream: &mut Stream, _stream_key: &str) -> Result<()> {
    stream.is_publishing = true;
    Ok(())
}
