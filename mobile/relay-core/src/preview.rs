//! A bounded tap on the ingest's video for the app's live preview.
//!
//! The relay never waits for the viewer: when it falls behind, queued frames are dropped and the
//! tap resumes at the next keyframe, so previewing can never slow the stream to the platform.
//! Each viewer gets a session number, so a viewer that is shutting down cannot stop or drain the
//! one that replaced it.

use std::collections::VecDeque;
use std::sync::{Condvar, Mutex, OnceLock};
use std::time::Duration;

/// About three seconds of 30 fps video.
const QUEUE_CAPACITY: usize = 90;

/// One FLV video tag body with its RTMP timestamp.
pub(crate) type PreviewFrame = (u32, Vec<u8>);

#[derive(Default)]
struct Tap {
    session: u64,
    enabled: bool,
    waiting_for_keyframe: bool,
    sequence_header: Option<Vec<u8>>,
    frames: VecDeque<PreviewFrame>,
}

impl Tap {
    /// Queues a frame; returns whether a waiting viewer should be woken.
    fn offer(&mut self, timestamp: u32, payload: &[u8]) -> bool {
        let header = crate::is_video_sequence_header(payload);
        if header {
            // Kept even while nobody watches, so a viewer that opens mid-stream can decode.
            self.sequence_header = Some(payload.to_vec());
        }
        if !self.enabled {
            return false;
        }
        if self.frames.len() >= QUEUE_CAPACITY {
            self.restart_queue();
        }
        if self.waiting_for_keyframe && !header {
            if !crate::is_video_keyframe(payload) {
                return false;
            }
            self.waiting_for_keyframe = false;
        }
        self.frames.push_back((timestamp, payload.to_vec()));
        true
    }

    fn start(&mut self) -> u64 {
        self.session = self.session.wrapping_add(1);
        self.enabled = true;
        self.restart_queue();
        self.session
    }

    fn stop(&mut self, session: u64) {
        if session == self.session {
            self.enabled = false;
            self.frames.clear();
        }
    }

    /// A new source must never be decoded with the previous source's codec header.
    fn source_ended(&mut self) {
        self.sequence_header = None;
        self.frames.clear();
        self.waiting_for_keyframe = true;
    }

    fn is_idle(&self, session: u64) -> bool {
        session == self.session && self.enabled && self.frames.is_empty()
    }

    fn take(&mut self, session: u64) -> Option<PreviewFrame> {
        if session != self.session || !self.enabled {
            return None;
        }
        self.frames.pop_front()
    }

    /// Starts over from the latest codec header and the next keyframe.
    fn restart_queue(&mut self) {
        self.frames.clear();
        self.waiting_for_keyframe = true;
        if let Some(header) = self.sequence_header.clone() {
            self.frames.push_back((0, header));
        }
    }
}

struct Preview {
    tap: Mutex<Tap>,
    ready: Condvar,
}

fn preview() -> &'static Preview {
    static PREVIEW: OnceLock<Preview> = OnceLock::new();
    PREVIEW.get_or_init(|| Preview {
        tap: Mutex::new(Tap::default()),
        ready: Condvar::new(),
    })
}

/// Offers one video tag body from the ingest callback. Cheap when nobody is watching.
pub(crate) fn offer(timestamp: u32, payload: &[u8]) {
    let preview = preview();
    let notify = preview
        .tap
        .lock()
        .map(|mut tap| tap.offer(timestamp, payload))
        .unwrap_or(false);
    if notify {
        preview.ready.notify_one();
    }
}

/// Starts a viewer session: the latest codec header first, then frames from the next keyframe.
pub(crate) fn start() -> u64 {
    preview().tap.lock().map(|mut tap| tap.start()).unwrap_or(0)
}

/// Ends [session] if it is still the current one and wakes its waiting reader.
pub(crate) fn stop(session: u64) {
    let preview = preview();
    if let Ok(mut tap) = preview.tap.lock() {
        tap.stop(session);
    }
    preview.ready.notify_all();
}

pub(crate) fn source_ended() {
    if let Ok(mut tap) = preview().tap.lock() {
        tap.source_ended();
    }
}

/// Waits up to [timeout] for the next frame of [session]. None on timeout, when the session was
/// replaced or stopped, or when the lock is poisoned.
pub(crate) fn next(session: u64, timeout: Duration) -> Option<PreviewFrame> {
    let preview = preview();
    let tap = preview.tap.lock().ok()?;
    let (mut tap, _) = preview
        .ready
        .wait_timeout_while(tap, timeout, |tap| tap.is_idle(session))
        .ok()?;
    tap.take(session)
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEADER: [u8; 5] = [0x17, 0x00, 0x00, 0x00, 0x00];
    const KEYFRAME: [u8; 5] = [0x17, 0x01, 0x00, 0x00, 0x00];
    const INTER: [u8; 5] = [0x27, 0x01, 0x00, 0x00, 0x00];

    #[test]
    fn nothing_is_queued_until_a_viewer_starts() {
        let mut tap = Tap::default();
        assert!(!tap.offer(0, &HEADER));
        assert!(!tap.offer(10, &KEYFRAME));
        assert!(tap.frames.is_empty());
        assert_eq!(tap.sequence_header.as_deref(), Some(&HEADER[..]));
    }

    #[test]
    fn a_late_viewer_gets_the_header_then_waits_for_a_keyframe() {
        let mut tap = Tap::default();
        tap.offer(0, &HEADER);
        let session = tap.start();
        assert!(!tap.offer(33, &INTER));
        assert!(tap.offer(66, &KEYFRAME));
        assert!(tap.offer(99, &INTER));
        assert_eq!(tap.take(session), Some((0, HEADER.to_vec())));
        assert_eq!(tap.take(session), Some((66, KEYFRAME.to_vec())));
        assert_eq!(tap.take(session), Some((99, INTER.to_vec())));
        assert_eq!(tap.take(session), None);
    }

    #[test]
    fn a_slow_viewer_restarts_from_the_header_and_next_keyframe() {
        let mut tap = Tap::default();
        tap.offer(0, &HEADER);
        let session = tap.start();
        tap.offer(1, &KEYFRAME);
        // Header and keyframe are already queued; fill the rest of the queue.
        for timestamp in 2..QUEUE_CAPACITY as u32 {
            tap.offer(timestamp, &INTER);
        }
        assert_eq!(tap.frames.len(), QUEUE_CAPACITY);
        assert!(!tap.offer(500, &INTER));
        assert_eq!(tap.frames.len(), 1);
        assert!(tap.offer(501, &KEYFRAME));
        assert_eq!(tap.take(session), Some((0, HEADER.to_vec())));
        assert_eq!(tap.take(session), Some((501, KEYFRAME.to_vec())));
    }

    #[test]
    fn a_replaced_session_can_neither_read_nor_stop_the_new_one() {
        let mut tap = Tap::default();
        let old = tap.start();
        let new = tap.start();
        tap.offer(0, &KEYFRAME);
        tap.stop(old);
        assert!(tap.enabled);
        assert_eq!(tap.take(old), None);
        assert_eq!(tap.take(new), Some((0, KEYFRAME.to_vec())));
        tap.stop(new);
        assert!(!tap.enabled);
    }

    #[test]
    fn an_ended_source_forgets_its_codec_header() {
        let mut tap = Tap::default();
        tap.offer(0, &HEADER);
        let session = tap.start();
        tap.source_ended();
        assert!(tap.sequence_header.is_none());
        assert_eq!(tap.take(session), None);
        assert!(!tap.offer(10, &INTER));
        assert!(tap.offer(20, &KEYFRAME));
    }
}
