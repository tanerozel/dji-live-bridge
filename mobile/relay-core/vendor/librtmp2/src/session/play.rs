//! Play flow
//!
//! Mirrors `src/session/play.h` and `src/session/play.c`.

use super::stream::Stream;
use crate::types::Result;

/// Begin playing on a stream.
pub fn play_begin(_stream: &mut Stream, _stream_name: &str) -> Result<()> {
    Ok(())
}
