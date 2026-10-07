//! Lightweight multi-object tracking in the spirit of ByteTrack.
//!
//! Each inference's detections are matched to existing tracks by overlap with
//! the track's predicted position: confident detections first, then weak ones,
//! which may only keep an existing track alive (an animal half hidden by a
//! tree keeps its number). Unmatched confident detections start new tracks,
//! which are shown once a second inference confirms them.
//!
//! Matching ignores the class, and a track's label is the class with the most
//! accumulated confidence, so a model that wavers between "cat" and "dog" on
//! the same animal neither flickers nor counts it twice.

use super::detector::{BoundingBox, Detection};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrackerConfig {
    /// Detections at or above this start tracks and are counted.
    pub high_confidence: f32,
    pub match_iou: f32,
    /// Weak detections may only extend a track, never start one.
    pub weak_match_iou: f32,
    /// Last resort for small, fast boxes that no longer overlap their
    /// prediction: centre distance in box sizes (see `relative_distance`).
    pub distance_gate: f32,
    /// Matches before a track is shown.
    pub confirm_hits: u32,
    /// Seconds a lost track is kept for re-identification.
    pub max_age: f64,
    /// Seconds a lost track is still drawn, bridging a missed detection.
    pub display_age: f64,
}

impl TrackerConfig {
    pub fn with_threshold(high_confidence: f32) -> Self {
        Self {
            high_confidence,
            match_iou: 0.3,
            weak_match_iou: 0.3,
            distance_gate: 1.0,
            confirm_hits: 2,
            max_age: 3.0,
            display_age: 0.6,
        }
    }

    /// For a detector that runs every `interval` seconds. A slow model (OWLv2
    /// does about one picture per second) must not lose a box after one
    /// missed picture, nor forget an animal after three.
    pub fn new(high_confidence: f32, interval: f64) -> Self {
        let base = Self::with_threshold(high_confidence);
        Self {
            max_age: base.max_age.max(interval * 3.0),
            display_age: base.display_age.max(interval * 1.5),
            ..base
        }
    }
}

/// What the overlay, counter and UI see of a confirmed track.
#[derive(Debug, Clone, PartialEq)]
pub struct TrackView {
    pub id: u64,
    pub class_id: usize,
    pub confidence: f32,
    pub bbox: BoundingBox,
    /// Centre motion in picture fractions per second.
    pub velocity: (f32, f32),
    /// Tracker time (seconds) of the last matching detection.
    pub seen_at: f64,
    /// Inferences that matched this track so far.
    pub hits: u32,
}

impl TrackView {
    /// The box moved along its velocity to `now`, for drawing between two
    /// inferences. Bounded so a stale track never drifts off on its own.
    pub fn predicted(&self, now: f64) -> BoundingBox {
        shift(
            &self.bbox,
            self.velocity,
            (now - self.seen_at).clamp(0.0, 0.3),
        )
    }
}

#[derive(Debug, Clone)]
struct Track {
    id: u64,
    bbox: BoundingBox,
    velocity: (f32, f32),
    votes: Vec<(usize, f32)>,
    confidence: f32,
    hits: u32,
    confirmed: bool,
    last_seen: f64,
}

impl Track {
    fn class_id(&self) -> usize {
        self.votes
            .iter()
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .map_or(0, |(class, _)| *class)
    }

    fn predicted(&self, now: f64) -> BoundingBox {
        shift(
            &self.bbox,
            self.velocity,
            (now - self.last_seen).clamp(0.0, 0.5),
        )
    }

    fn absorb(&mut self, detection: &Detection, now: f64, confirm_hits: u32) {
        let elapsed = (now - self.last_seen).max(1e-3) as f32;
        let (old_x, old_y) = centre(&self.bbox);
        let (new_x, new_y) = centre(&detection.bbox);
        let measured = (
            ((new_x - old_x) / elapsed).clamp(-2.0, 2.0),
            ((new_y - old_y) / elapsed).clamp(-2.0, 2.0),
        );
        self.velocity = (
            self.velocity.0 * 0.6 + measured.0 * 0.4,
            self.velocity.1 * 0.6 + measured.1 * 0.4,
        );
        // Lean on the new detection but damp the per-inference jitter.
        let predicted = self.predicted(now);
        self.bbox = blend(&predicted, &detection.bbox, 0.7);
        self.confidence = detection.confidence;
        self.hits += 1;
        self.confirmed |= self.hits >= confirm_hits;
        self.last_seen = now;
        match self
            .votes
            .iter_mut()
            .find(|(class, _)| *class == detection.class_id)
        {
            Some((_, weight)) => *weight += detection.confidence,
            None => self.votes.push((detection.class_id, detection.confidence)),
        }
    }

    fn view(&self) -> TrackView {
        TrackView {
            id: self.id,
            class_id: self.class_id(),
            confidence: self.confidence,
            bbox: self.bbox,
            velocity: self.velocity,
            seen_at: self.last_seen,
            hits: self.hits,
        }
    }
}

pub struct ObjectTracker {
    tracks: Vec<Track>,
    next_id: u64,
    config: TrackerConfig,
}

impl ObjectTracker {
    /// Ids start at `first_id`, so they stay unique across trackers.
    pub fn new(config: TrackerConfig, first_id: u64) -> Self {
        Self {
            tracks: Vec::new(),
            next_id: first_id,
            config,
        }
    }

    pub fn next_id(&self) -> u64 {
        self.next_id
    }

    pub fn set_config(&mut self, config: TrackerConfig) {
        self.config = config;
    }

    pub fn clear(&mut self) {
        self.tracks.clear();
    }

    /// Feeds one inference's detections, taken at tracker time `now`.
    pub fn update(&mut self, detections: &[Detection], now: f64) {
        let config = self.config;
        let predicted: Vec<BoundingBox> = self
            .tracks
            .iter()
            .map(|track| track.predicted(now))
            .collect();
        let mut track_taken = vec![false; self.tracks.len()];
        let (strong, weak): (Vec<&Detection>, Vec<&Detection>) = detections
            .iter()
            .partition(|detection| detection.confidence >= config.high_confidence);

        let strong_matches = associate(&predicted, &mut track_taken, &strong, config.match_iou);
        let weak_matches = associate(&predicted, &mut track_taken, &weak, config.weak_match_iou);

        let mut strong_taken = vec![false; strong.len()];
        let mut weak_taken = vec![false; weak.len()];
        for (track, detection) in strong_matches {
            self.tracks[track].absorb(strong[detection], now, config.confirm_hits);
            strong_taken[detection] = true;
        }
        for (track, detection) in weak_matches {
            self.tracks[track].absorb(weak[detection], now, config.confirm_hits);
            weak_taken[detection] = true;
        }

        // At a few inferences a second, a small animal (or the drone) can
        // move further than its own size: nothing overlaps any more, but the
        // detection is still right next to where the track was heading.
        let leftover: Vec<(bool, usize, &Detection)> = strong
            .iter()
            .enumerate()
            .filter(|(index, _)| !strong_taken[*index])
            .map(|(index, detection)| (true, index, *detection))
            .chain(
                weak.iter()
                    .enumerate()
                    .filter(|(index, _)| !weak_taken[*index])
                    .map(|(index, detection)| (false, index, *detection)),
            )
            .collect();
        let candidates: Vec<&Detection> = leftover
            .iter()
            .map(|(_, _, detection)| *detection)
            .collect();
        for (track, candidate) in associate_by_distance(
            &predicted,
            &mut track_taken,
            &candidates,
            config.distance_gate,
        ) {
            let (strong_one, index, detection) = leftover[candidate];
            self.tracks[track].absorb(detection, now, config.confirm_hits);
            if strong_one {
                strong_taken[index] = true;
            }
        }

        // A tentative track that missed its confirming match was noise.
        let mut index = 0;
        self.tracks.retain(|track| {
            let keep =
                (track.confirmed || track_taken[index]) && now - track.last_seen <= config.max_age;
            index += 1;
            keep
        });

        for (detection, _) in strong.iter().zip(strong_taken).filter(|(_, taken)| !taken) {
            let mut track = Track {
                id: self.next_id,
                bbox: detection.bbox,
                velocity: (0.0, 0.0),
                votes: Vec::new(),
                confidence: 0.0,
                hits: 0,
                confirmed: false,
                last_seen: now,
            };
            self.next_id += 1;
            track.absorb(detection, now, config.confirm_hits);
            self.tracks.push(track);
        }
    }

    /// Confirmed tracks seen recently enough to draw and count.
    pub fn visible(&self, now: f64) -> Vec<TrackView> {
        self.tracks
            .iter()
            .filter(|track| track.confirmed && now - track.last_seen <= self.config.display_age)
            .map(Track::view)
            .collect()
    }
}

/// Greedy best-overlap-first assignment; returns (track, detection) pairs.
fn associate(
    predicted: &[BoundingBox],
    track_taken: &mut [bool],
    detections: &[&Detection],
    min_iou: f32,
) -> Vec<(usize, usize)> {
    let mut pairs = Vec::new();
    for (track, bbox) in predicted.iter().enumerate() {
        if track_taken[track] {
            continue;
        }
        for (detection, candidate) in detections.iter().enumerate() {
            let overlap = bbox.iou(&candidate.bbox);
            if overlap >= min_iou {
                pairs.push((overlap, track, detection));
            }
        }
    }
    pairs.sort_by(|a, b| b.0.total_cmp(&a.0));
    let mut detection_taken = vec![false; detections.len()];
    let mut matches = Vec::new();
    for (_, track, detection) in pairs {
        if track_taken[track] || detection_taken[detection] {
            continue;
        }
        track_taken[track] = true;
        detection_taken[detection] = true;
        matches.push((track, detection));
    }
    matches
}

/// Greedy nearest-first assignment of what IoU could not match, gated by
/// `relative_distance` and by similar box sizes.
fn associate_by_distance(
    predicted: &[BoundingBox],
    track_taken: &mut [bool],
    detections: &[&Detection],
    gate: f32,
) -> Vec<(usize, usize)> {
    let mut pairs = Vec::new();
    for (track, bbox) in predicted.iter().enumerate() {
        if track_taken[track] {
            continue;
        }
        for (detection, candidate) in detections.iter().enumerate() {
            let ratio = bbox.area() / candidate.bbox.area().max(f32::EPSILON);
            let distance = relative_distance(bbox, &candidate.bbox);
            if distance <= gate && (0.25..=4.0).contains(&ratio) {
                pairs.push((distance, track, detection));
            }
        }
    }
    pairs.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut detection_taken = vec![false; detections.len()];
    let mut matches = Vec::new();
    for (_, track, detection) in pairs {
        if track_taken[track] || detection_taken[detection] {
            continue;
        }
        track_taken[track] = true;
        detection_taken[detection] = true;
        matches.push((track, detection));
    }
    matches
}

/// Centre distance measured in box sizes per axis, so it means the same for
/// a calf far away and a cow up close, and in any picture aspect ratio.
fn relative_distance(a: &BoundingBox, b: &BoundingBox) -> f32 {
    let (ax, ay) = centre(a);
    let (bx, by) = centre(b);
    let width = ((a.width + b.width) / 2.0).max(f32::EPSILON);
    let height = ((a.height + b.height) / 2.0).max(f32::EPSILON);
    (((ax - bx) / width).powi(2) + ((ay - by) / height).powi(2)).sqrt()
}

fn centre(bbox: &BoundingBox) -> (f32, f32) {
    (bbox.x + bbox.width / 2.0, bbox.y + bbox.height / 2.0)
}

fn shift(bbox: &BoundingBox, velocity: (f32, f32), seconds: f64) -> BoundingBox {
    let seconds = seconds as f32;
    BoundingBox {
        x: bbox.x + velocity.0 * seconds,
        y: bbox.y + velocity.1 * seconds,
        ..*bbox
    }
}

fn blend(from: &BoundingBox, to: &BoundingBox, weight: f32) -> BoundingBox {
    let mix = |a: f32, b: f32| a + (b - a) * weight;
    BoundingBox {
        x: mix(from.x, to.x),
        y: mix(from.y, to.y),
        width: mix(from.width, to.width),
        height: mix(from.height, to.height),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn detection(class_id: usize, confidence: f32, x: f32, y: f32) -> Detection {
        Detection {
            class_id,
            confidence,
            bbox: BoundingBox {
                x,
                y,
                width: 0.1,
                height: 0.1,
            },
        }
    }

    fn tracker() -> ObjectTracker {
        ObjectTracker::new(TrackerConfig::with_threshold(0.6), 1)
    }

    #[test]
    fn a_moving_animal_keeps_its_id() {
        let mut tracker = tracker();
        for step in 0..20 {
            let x = 0.1 + step as f32 * 0.01;
            tracker.update(&[detection(19, 0.9, x, 0.4)], step as f64 * 0.1);
        }
        let visible = tracker.visible(1.9);
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].id, 1);
        assert!(visible[0].velocity.0 > 0.05, "{:?}", visible[0].velocity);
    }

    #[test]
    fn a_single_detection_is_not_shown_until_confirmed() {
        let mut tracker = tracker();
        tracker.update(&[detection(19, 0.9, 0.2, 0.2)], 0.0);
        assert!(tracker.visible(0.0).is_empty());
        tracker.update(&[detection(19, 0.9, 0.2, 0.2)], 0.1);
        assert_eq!(tracker.visible(0.1).len(), 1);
    }

    #[test]
    fn a_one_off_false_positive_is_dropped() {
        let mut tracker = tracker();
        tracker.update(&[detection(19, 0.9, 0.2, 0.2)], 0.0);
        tracker.update(&[], 0.1);
        tracker.update(&[detection(19, 0.9, 0.2, 0.2)], 0.2);
        tracker.update(&[detection(19, 0.9, 0.2, 0.2)], 0.3);
        // The first detection never confirmed; the next two did, as a new id.
        assert_eq!(tracker.visible(0.3)[0].id, 2);
    }

    #[test]
    fn weak_detections_keep_a_track_alive_but_never_start_one() {
        let mut tracker = tracker();
        tracker.update(&[detection(19, 0.9, 0.2, 0.2)], 0.0);
        tracker.update(&[detection(19, 0.9, 0.2, 0.2)], 0.1);
        for step in 2..30 {
            tracker.update(
                &[detection(19, 0.3, 0.2, 0.2), detection(18, 0.3, 0.7, 0.7)],
                step as f64 * 0.1,
            );
        }
        let visible = tracker.visible(2.9);
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].id, 1);
    }

    #[test]
    fn lost_tracks_disappear_after_display_age_and_expire() {
        let mut tracker = tracker();
        tracker.update(&[detection(19, 0.9, 0.2, 0.2)], 0.0);
        tracker.update(&[detection(19, 0.9, 0.2, 0.2)], 0.1);
        tracker.update(&[], 0.5);
        assert_eq!(tracker.visible(0.5).len(), 1);
        tracker.update(&[], 1.0);
        assert!(tracker.visible(1.0).is_empty());
        // Re-found within max_age: same id.
        tracker.update(&[detection(19, 0.9, 0.2, 0.2)], 1.4);
        assert_eq!(tracker.visible(1.4)[0].id, 1);
        tracker.update(&[], 4.5);
        tracker.update(&[detection(19, 0.9, 0.2, 0.2)], 4.6);
        tracker.update(&[detection(19, 0.9, 0.2, 0.2)], 4.7);
        assert_eq!(tracker.visible(4.7)[0].id, 2);
    }

    #[test]
    fn a_slow_detector_keeps_boxes_through_one_missed_picture() {
        let mut tracker = ObjectTracker::new(TrackerConfig::new(0.6, 0.9), 1);
        tracker.update(&[detection(19, 0.9, 0.2, 0.2)], 0.0);
        tracker.update(&[detection(19, 0.9, 0.2, 0.2)], 0.9);
        tracker.update(&[], 1.8);
        assert_eq!(tracker.visible(1.8).len(), 1);
        tracker.update(&[], 2.7);
        assert!(tracker.visible(2.7).is_empty());
        // A fast detector keeps the defaults.
        assert_eq!(
            TrackerConfig::new(0.6, 0.1),
            TrackerConfig::with_threshold(0.6)
        );
    }

    #[test]
    fn label_follows_the_strongest_class_vote() {
        let mut tracker = tracker();
        tracker.update(&[detection(15, 0.62, 0.2, 0.2)], 0.0);
        tracker.update(&[detection(16, 0.9, 0.2, 0.2)], 0.1);
        tracker.update(&[detection(16, 0.9, 0.2, 0.2)], 0.2);
        let visible = tracker.visible(0.2);
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].class_id, 16);
    }

    #[test]
    fn a_jump_larger_than_the_box_keeps_the_id() {
        let mut tracker = tracker();
        // 0.1-wide boxes moving 0.08 per inference: IoU 0.11, below the
        // 0.3 overlap gate, but the centre is within one box size.
        for step in 0..6 {
            let x = 0.1 + step as f32 * 0.08;
            tracker.update(&[detection(19, 0.9, x, 0.4)], step as f64 * 0.25);
        }
        let visible = tracker.visible(1.25);
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].id, 1);
    }

    #[test]
    fn two_neighbours_get_two_ids() {
        let mut tracker = tracker();
        for step in 0..3 {
            tracker.update(
                &[detection(18, 0.9, 0.10, 0.5), detection(18, 0.9, 0.22, 0.5)],
                step as f64 * 0.1,
            );
        }
        let mut ids: Vec<u64> = tracker.visible(0.2).iter().map(|t| t.id).collect();
        ids.sort();
        assert_eq!(ids, vec![1, 2]);
    }
}
