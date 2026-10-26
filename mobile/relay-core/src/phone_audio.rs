//! The phone's microphone in place of the drone's sound, so the pilot can talk over the
//! broadcast. The app records and encodes AAC itself; the relay puts each frame on the drone's
//! timeline and sends it instead of DJI Fly's audio, which is silence on an RC 2.
//!
//! The drone's timestamps run on its own clock. Every video frame says which drone timestamp
//! "now" is; Wi-Fi only ever delays a frame, so the earliest-arriving frames give the estimate.
//! Microphone frames follow their own sample clock from an anchor on that estimate and are
//! moved back to it only when the two drift apart by more than [`RESYNC_MS`].
//!
//! Microphone frames go out only while a drone source runs: without its video there is no
//! timeline, and a platform connection is held open by the drone's frames alone (SOURCE_HOLD).

use librtmp2::types::FrameType;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

/// Further apart than this, the microphone's clock and the drone's are aligned again.
const RESYNC_MS: i64 = 300;
/// How far the estimate may move per video frame towards frames that arrive later than it says:
/// far more than any clock drift, far less than Wi-Fi jitter.
const OFFSET_DECAY_MS: i64 = 1;
/// An AudioSpecificConfig is two bytes for AAC-LC; anything this long is not one.
const MAX_CONFIG_BYTES: usize = 64;
const MAX_FRAME_BYTES: usize = 8 * 1024;
/// FLV audio tag header for AAC: sound format 10; the rate/size/channel bits are fixed for AAC.
const FLV_AAC: u8 = 0xAF;

static ON: AtomicBool = AtomicBool::new(false);
static FRAMES: AtomicU64 = AtomicU64::new(0);
static STATE: OnceLock<Mutex<State>> = OnceLock::new();
static EPOCH: OnceLock<Instant> = OnceLock::new();

#[derive(Default)]
struct State {
    timeline: Timeline,
    /// The microphone's FLV sequence header; None while the microphone is off.
    header: Option<Vec<u8>>,
    /// The source generation the microphone's header was last sent in.
    header_generation: Option<u64>,
    /// DJI Fly's own sequence header, sent again when the microphone is switched off.
    drone_header: Option<(u64, Vec<u8>)>,
}

/// Where the drone's timeline is now, for one source generation.
#[derive(Default)]
struct Timeline {
    generation: u64,
    /// Drone timestamp minus local milliseconds, from the earliest-arriving video.
    offset: Option<i64>,
    latest_video: u32,
    /// A microphone timestamp (µs) and the drone timestamp it was placed at.
    anchor: Option<(i64, i64)>,
    last_drone_audio: Option<u32>,
    last_phone_audio: Option<u32>,
}

impl Timeline {
    fn follow(&mut self, generation: u64) {
        if generation != self.generation {
            *self = Self {
                generation,
                ..Self::default()
            };
        }
    }

    fn video(&mut self, generation: u64, timestamp: u32, now_ms: i64) {
        self.follow(generation);
        let candidate = i64::from(timestamp) - now_ms;
        self.offset = Some(match self.offset {
            // The drone's timeline jumped back: start over from it.
            Some(offset) if candidate < offset - RESYNC_MS => candidate,
            Some(offset) if candidate < offset => (offset - OFFSET_DECAY_MS).max(candidate),
            _ => candidate,
        });
        self.latest_video = timestamp;
    }

    /// The drone timestamp of a microphone frame captured at `pts_us` that arrives at `now_ms`;
    /// None before the drone's first video frame and for a frame that would go back in time.
    fn place(&mut self, pts_us: i64, now_ms: i64) -> Option<u32> {
        let estimate = now_ms + self.offset?;
        let after = self
            .last_phone_audio
            .max(self.last_drone_audio)
            .map(|last| i64::from(last) + 1);
        let placed = match self.anchor {
            Some((anchor_pts, anchor_ts)) => {
                let placed = anchor_ts + (pts_us - anchor_pts).div_euclid(1000);
                ((placed - estimate).abs() <= RESYNC_MS).then_some(placed)
            }
            None => None,
        };
        let placed = placed.unwrap_or_else(|| {
            let start = estimate.max(0);
            self.anchor = Some((pts_us, start));
            start
        });
        // Dropped rather than moved forward, so the microphone cannot run ahead of the drone.
        if after.is_some_and(|after| placed < after) {
            return None;
        }
        let timestamp = u32::try_from(placed).unwrap_or(u32::MAX);
        self.last_phone_audio = Some(timestamp);
        Some(timestamp)
    }

    /// Whether a frame of DJI Fly's audio may go out: not while the microphone replaces it, nor
    /// one older than the microphone's last frame.
    fn drone_audio(&mut self, generation: u64, timestamp: u32, microphone_on: bool) -> bool {
        self.follow(generation);
        if microphone_on || self.last_phone_audio.is_some_and(|last| timestamp <= last) {
            return false;
        }
        self.last_drone_audio = Some(timestamp);
        true
    }
}

fn state() -> &'static Mutex<State> {
    STATE.get_or_init(|| Mutex::new(State::default()))
}

fn now_ms() -> i64 {
    let epoch = *EPOCH.get_or_init(Instant::now);
    i64::try_from(epoch.elapsed().as_millis()).unwrap_or(i64::MAX)
}

fn tag(packet_type: u8, body: &[u8]) -> Vec<u8> {
    let mut tag = Vec::with_capacity(2 + body.len());
    tag.extend_from_slice(&[FLV_AAC, packet_type]);
    tag.extend_from_slice(body);
    tag
}

/// Sends the phone's microphone from now on in place of DJI Fly's audio. `config` is the AAC
/// encoder's AudioSpecificConfig (csd-0 on Android, the magic cookie's config on iOS).
pub fn start(config: &[u8]) -> Result<(), String> {
    if config.len() < 2 || config.len() > MAX_CONFIG_BYTES {
        return Err("audio_config_invalid".to_owned());
    }
    let mut state = state().lock().map_err(|_| "state_lock_failed".to_owned())?;
    state.header = Some(tag(0, config));
    state.header_generation = None;
    state.timeline.anchor = None;
    ON.store(true, Ordering::Release);
    Ok(())
}

/// One encoded AAC frame of the microphone, captured at `pts_us` on the app's own clock.
pub fn frame(pts_us: i64, aac: &[u8]) {
    if !ON.load(Ordering::Acquire) || aac.is_empty() || aac.len() > MAX_FRAME_BYTES {
        return;
    }
    let now = now_ms();
    let placed = {
        let Ok(mut state) = state().lock() else {
            return;
        };
        let Some(header) = state.header.clone() else {
            return;
        };
        let Some(timestamp) = state.timeline.place(pts_us, now) else {
            return;
        };
        let generation = state.timeline.generation;
        // A new source started without the microphone's codec header; it goes first.
        let header = (state.header_generation != Some(generation)).then(|| {
            state.header_generation = Some(generation);
            header
        });
        (generation, timestamp, header)
    };
    let (generation, timestamp, header) = placed;
    let body = tag(1, aac);
    let mut frames = Vec::with_capacity(2);
    if let Some(header) = header.as_deref() {
        frames.push((FrameType::Audio, timestamp, header));
    }
    frames.push((FrameType::Audio, timestamp, body.as_slice()));
    if crate::inject_frames(generation, &frames) {
        FRAMES.fetch_add(1, Ordering::Relaxed);
    }
}

/// Goes back to DJI Fly's audio, starting with its codec header.
pub fn stop() {
    let restore = {
        let Ok(mut state) = state().lock() else {
            ON.store(false, Ordering::Release);
            return;
        };
        let was_on = state.header.take().is_some();
        state.header_generation = None;
        state.timeline.anchor = None;
        let timeline = &state.timeline;
        let timestamp = timeline.last_phone_audio.unwrap_or(timeline.latest_video);
        state
            .drone_header
            .as_ref()
            .filter(|(generation, _)| was_on && *generation == timeline.generation)
            .map(|(generation, header)| (*generation, timestamp, header.clone()))
    };
    // DJI Fly's frames stay dropped until its header is back in place.
    if let Some((generation, timestamp, header)) = restore {
        crate::inject_frames(generation, &[(FrameType::Audio, timestamp, &header)]);
    }
    ON.store(false, Ordering::Release);
}

pub(crate) fn is_on() -> bool {
    ON.load(Ordering::Relaxed)
}

/// Microphone frames that went out on the drone's timeline since the app started.
pub(crate) fn frames_sent() -> u64 {
    FRAMES.load(Ordering::Relaxed)
}

/// Called by the ingest for every video frame of the current source.
pub(crate) fn video_arrived(generation: u64, timestamp: u32) {
    let now = now_ms();
    if let Ok(mut state) = state().lock() {
        state.timeline.video(generation, timestamp, now);
    }
}

/// Called by the ingest for every audio frame of the current source; false drops the frame.
pub(crate) fn pass_drone_audio(generation: u64, timestamp: u32, payload: &[u8]) -> bool {
    let Ok(mut state) = state().lock() else {
        return true;
    };
    if crate::is_audio_sequence_header(payload) {
        state.drone_header = Some((generation, payload.to_vec()));
    }
    let on = ON.load(Ordering::Acquire);
    state.timeline.drone_audio(generation, timestamp, on)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn timeline_at(offset_video: &[(u32, i64)]) -> Timeline {
        let mut timeline = Timeline::default();
        for &(timestamp, now) in offset_video {
            timeline.video(0, timestamp, now);
        }
        timeline
    }

    #[test]
    fn nothing_is_placed_before_the_drone_video() {
        let mut timeline = Timeline::default();
        assert_eq!(timeline.place(0, 1_000), None);
    }

    #[test]
    fn the_earliest_video_frame_sets_the_timeline() {
        // Frame 100 arrived 40 ms late; frame 133 on time.
        let mut timeline = timeline_at(&[(100, 1_140), (133, 1_133)]);
        assert_eq!(timeline.place(0, 1_200), Some(200));
    }

    #[test]
    fn microphone_frames_follow_their_own_clock() {
        let mut timeline = timeline_at(&[(0, 1_000)]);
        assert_eq!(timeline.place(5_000_000, 1_000), Some(0));
        // Encoded in a burst: arrival times differ, capture times decide.
        assert_eq!(timeline.place(5_021_333, 1_090), Some(21));
        assert_eq!(timeline.place(5_042_666, 1_091), Some(42));
        assert_eq!(timeline.place(5_064_000, 1_092), Some(64));
    }

    #[test]
    fn a_long_drift_is_corrected() {
        let mut timeline = timeline_at(&[(0, 1_000)]);
        assert_eq!(timeline.place(0, 1_000), Some(0));
        // The microphone paused for two seconds without its clock showing it.
        assert_eq!(timeline.place(21_333, 3_000), Some(2_000));
        assert_eq!(timeline.place(42_666, 3_021), Some(2_021));
    }

    #[test]
    fn microphone_timestamps_never_go_back() {
        let mut timeline = timeline_at(&[(0, 1_000)]);
        assert!(timeline.drone_audio(0, 500, false));
        assert_eq!(timeline.place(0, 1_000), None);
        assert_eq!(timeline.place(21_333, 1_021), None);
        assert_eq!(timeline.place(510_000, 1_510), Some(510));
    }

    #[test]
    fn a_fast_microphone_clock_cannot_run_ahead() {
        let mut timeline = timeline_at(&[(0, 1_000)]);
        assert_eq!(timeline.place(0, 1_000), Some(0));
        // 400 ms of samples within no time at all.
        assert_eq!(timeline.place(400_000, 1_000), None);
        assert_eq!(timeline.place(421_333, 1_100), Some(21));
    }

    #[test]
    fn drone_audio_is_held_back_while_the_microphone_is_on() {
        let mut timeline = timeline_at(&[(0, 1_000)]);
        assert!(!timeline.drone_audio(0, 10, true));
        assert_eq!(timeline.place(0, 1_100), Some(100));
        // Switched off: DJI Fly's frames from before the microphone's last one stay out.
        assert!(!timeline.drone_audio(0, 90, false));
        assert!(timeline.drone_audio(0, 120, false));
    }

    #[test]
    fn a_new_source_starts_a_new_timeline() {
        let mut timeline = timeline_at(&[(5_000, 1_000)]);
        assert_eq!(timeline.place(0, 1_000), Some(5_000));
        timeline.video(1, 0, 2_000);
        assert_eq!(timeline.generation, 1);
        assert_eq!(timeline.place(1_000_000, 2_000), Some(0));
    }

    #[test]
    fn late_video_moves_the_estimate_only_slowly() {
        let mut timeline = timeline_at(&[(0, 1_000), (33, 1_133)]);
        assert_eq!(timeline.offset, Some(-1_000 - OFFSET_DECAY_MS));
        // A jump back of more than RESYNC_MS is a new timeline, not jitter.
        timeline.video(0, 66, 1_266);
        assert_eq!(timeline.offset, Some(-1_000 - 2 * OFFSET_DECAY_MS));
        timeline.video(0, 0, 1_400);
        assert_eq!(timeline.offset, Some(-1_400));
    }

    #[test]
    fn a_config_must_look_like_one() {
        assert!(start(&[]).is_err());
        assert!(start(&[0u8; MAX_CONFIG_BYTES + 1]).is_err());
    }

    #[test]
    fn flv_tags_carry_the_aac_header() {
        assert_eq!(tag(0, &[0x11, 0x88]), vec![0xAF, 0x00, 0x11, 0x88]);
        assert!(crate::is_audio_sequence_header(&tag(0, &[0x11, 0x88])));
        assert!(!crate::is_audio_sequence_header(&tag(1, &[0x21])));
    }
}
