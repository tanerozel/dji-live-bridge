//! A bounded tap on the ingest's video for the app's live preview.
//!
//! The relay never waits for the viewer: when it falls behind, queued frames are dropped and the
//! tap starts over from the current GOP, so previewing can never slow the stream to the
//! platform. The last keyframe and every frame since are kept, so a viewer that opens
//! mid-stream shows the current picture at once instead of waiting for the next keyframe.
//! Each viewer gets a session number, so a viewer that is shutting down cannot stop or drain
//! the one that replaced it.

use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::Duration;

/// Twelve seconds of 30 fps video: a full cached GOP plus slack.
const QUEUE_CAPACITY: usize = 360;
/// Ten seconds at 30 fps. A longer GOP is not replayed; a new viewer then waits for a keyframe.
const GOP_CACHE_MAX_FRAMES: usize = 300;
const GOP_CACHE_MAX_BYTES: usize = 16 * 1024 * 1024;

/// One FLV video tag body with its RTMP timestamp. Shared between the GOP cache and the queue.
pub(crate) type PreviewFrame = (u32, Arc<[u8]>);

#[derive(Default)]
struct Tap {
    session: u64,
    enabled: bool,
    waiting_for_keyframe: bool,
    sequence_header: Option<Arc<[u8]>>,
    frames: VecDeque<PreviewFrame>,
    /// The last keyframe and every frame after it; empty until a keyframe arrives.
    gop: Vec<PreviewFrame>,
    gop_bytes: usize,
}

impl Tap {
    /// Queues a frame; returns whether a waiting viewer should be woken.
    fn offer(&mut self, timestamp: u32, payload: &[u8]) -> bool {
        let header = crate::is_video_sequence_header(payload);
        let frame: Arc<[u8]> = Arc::from(payload);
        if header {
            // Kept even while nobody watches, so a viewer that opens mid-stream can decode.
            self.sequence_header = Some(Arc::clone(&frame));
        }
        // Checked before this frame joins the GOP, so a restart cannot queue it twice.
        if self.enabled && self.frames.len() >= QUEUE_CAPACITY {
            self.restart_queue();
        }
        if !header {
            self.cache_gop(timestamp, &frame, crate::is_video_keyframe(payload));
        }
        if !self.enabled {
            return false;
        }
        if self.waiting_for_keyframe && !header {
            if !crate::is_video_keyframe(payload) {
                return false;
            }
            self.waiting_for_keyframe = false;
        }
        self.frames.push_back((timestamp, frame));
        true
    }

    fn cache_gop(&mut self, timestamp: u32, frame: &Arc<[u8]>, keyframe: bool) {
        if keyframe {
            self.gop.clear();
            self.gop_bytes = 0;
        } else if self.gop.is_empty() {
            return;
        }
        if self.gop.len() >= GOP_CACHE_MAX_FRAMES
            || self.gop_bytes + frame.len() > GOP_CACHE_MAX_BYTES
        {
            self.gop.clear();
            self.gop_bytes = 0;
            return;
        }
        self.gop_bytes += frame.len();
        self.gop.push((timestamp, Arc::clone(frame)));
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
        self.gop.clear();
        self.gop_bytes = 0;
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

    /// Starts over from the latest codec header and the current GOP, or the next keyframe.
    fn restart_queue(&mut self) {
        self.frames.clear();
        if let Some(header) = &self.sequence_header {
            self.frames.push_back((0, Arc::clone(header)));
        }
        self.frames.extend(self.gop.iter().cloned());
        self.waiting_for_keyframe = self.gop.is_empty();
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

/// Offers one video tag body from the ingest callback.
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

/// Starts a viewer session: the latest codec header and the current GOP first.
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

    fn taken(tap: &mut Tap, session: u64) -> Option<(u32, Vec<u8>)> {
        tap.take(session)
            .map(|(timestamp, frame)| (timestamp, frame.to_vec()))
    }

    #[test]
    fn nothing_is_queued_until_a_viewer_starts() {
        let mut tap = Tap::default();
        assert!(!tap.offer(0, &HEADER));
        assert!(!tap.offer(10, &KEYFRAME));
        assert!(tap.frames.is_empty());
        assert_eq!(tap.sequence_header.as_deref(), Some(&HEADER[..]));
    }

    #[test]
    fn a_viewer_that_opens_mid_gop_starts_at_the_cached_keyframe() {
        let mut tap = Tap::default();
        tap.offer(0, &HEADER);
        tap.offer(33, &KEYFRAME);
        tap.offer(66, &INTER);
        let session = tap.start();
        assert!(tap.offer(99, &INTER));
        assert_eq!(taken(&mut tap, session), Some((0, HEADER.to_vec())));
        assert_eq!(taken(&mut tap, session), Some((33, KEYFRAME.to_vec())));
        assert_eq!(taken(&mut tap, session), Some((66, INTER.to_vec())));
        assert_eq!(taken(&mut tap, session), Some((99, INTER.to_vec())));
        assert_eq!(taken(&mut tap, session), None);
    }

    #[test]
    fn a_viewer_without_a_cached_keyframe_waits_for_one() {
        let mut tap = Tap::default();
        tap.offer(0, &HEADER);
        let session = tap.start();
        assert!(!tap.offer(33, &INTER));
        assert!(tap.offer(66, &KEYFRAME));
        assert_eq!(taken(&mut tap, session), Some((0, HEADER.to_vec())));
        assert_eq!(taken(&mut tap, session), Some((66, KEYFRAME.to_vec())));
    }

    #[test]
    fn a_slow_viewer_restarts_from_the_current_gop() {
        let mut tap = Tap::default();
        tap.offer(0, &HEADER);
        let session = tap.start();
        let mut timestamp = 1;
        while tap.frames.len() < QUEUE_CAPACITY {
            let frame = if timestamp % 30 == 1 {
                &KEYFRAME
            } else {
                &INTER
            };
            tap.offer(timestamp, frame);
            timestamp += 1;
        }
        // The viewer never read; the next frame starts it over at the latest keyframe.
        let last_keyframe = (1..timestamp).rev().find(|value| value % 30 == 1).unwrap();
        assert!(tap.offer(timestamp, &INTER));
        assert_eq!(taken(&mut tap, session), Some((0, HEADER.to_vec())));
        assert_eq!(
            taken(&mut tap, session),
            Some((last_keyframe, KEYFRAME.to_vec()))
        );
        assert_eq!(tap.frames.len() as u32, timestamp - last_keyframe);
    }

    #[test]
    fn a_gop_too_long_to_replay_is_not_cached() {
        let mut tap = Tap::default();
        tap.offer(0, &HEADER);
        tap.offer(1, &KEYFRAME);
        for timestamp in 2..=(GOP_CACHE_MAX_FRAMES as u32 + 1) {
            tap.offer(timestamp, &INTER);
        }
        assert!(tap.gop.is_empty());
        let session = tap.start();
        assert!(!tap.offer(500, &INTER));
        assert!(tap.offer(501, &KEYFRAME));
        assert_eq!(taken(&mut tap, session), Some((0, HEADER.to_vec())));
        assert_eq!(taken(&mut tap, session), Some((501, KEYFRAME.to_vec())));
    }

    #[test]
    fn a_replaced_session_can_neither_read_nor_stop_the_new_one() {
        let mut tap = Tap::default();
        let old = tap.start();
        let new = tap.start();
        tap.offer(0, &KEYFRAME);
        tap.stop(old);
        assert!(tap.enabled);
        assert_eq!(taken(&mut tap, old), None);
        assert_eq!(taken(&mut tap, new), Some((0, KEYFRAME.to_vec())));
        tap.stop(new);
        assert!(!tap.enabled);
    }

    #[test]
    fn an_ended_source_forgets_its_codec_header_and_gop() {
        let mut tap = Tap::default();
        tap.offer(0, &HEADER);
        tap.offer(10, &KEYFRAME);
        let session = tap.start();
        tap.source_ended();
        assert!(tap.sequence_header.is_none());
        assert!(tap.gop.is_empty());
        assert_eq!(taken(&mut tap, session), None);
        assert!(!tap.offer(20, &INTER));
        assert!(tap.offer(30, &KEYFRAME));
    }
}
