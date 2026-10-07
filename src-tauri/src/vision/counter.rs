//! Counts what the tracker currently sees and every distinct track it has
//! confirmed since the counter was last reset.

use std::collections::BTreeMap;

use serde::Serialize;

use super::tracker::TrackView;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassCount {
    pub class: String,
    pub count: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CountSummary {
    pub current_total: u32,
    pub current_by_class: Vec<ClassCount>,
    /// Distinct track ids. An animal that leaves the picture for longer than
    /// the tracker remembers is counted again when it comes back.
    pub unique_total: u32,
    pub unique_by_class: Vec<ClassCount>,
}

/// Inferences a track must survive before it counts as a distinct animal;
/// shorter tracks are mostly re-detections of one already counted.
pub const UNIQUE_MIN_HITS: u32 = 3;

#[derive(Default)]
pub struct ObjectCounter {
    /// Every confirmed track id, with the class it was last labelled as.
    seen: BTreeMap<u64, usize>,
}

impl ObjectCounter {
    pub fn reset(&mut self) {
        self.seen.clear();
    }

    pub fn update(&mut self, visible: &[TrackView], labels: &[&str]) -> CountSummary {
        for track in visible {
            if track.hits >= UNIQUE_MIN_HITS || self.seen.contains_key(&track.id) {
                self.seen.insert(track.id, track.class_id);
            }
        }
        let current = tally(visible.iter().map(|track| track.class_id), labels);
        let unique = tally(self.seen.values().copied(), labels);
        CountSummary {
            current_total: visible.len() as u32,
            current_by_class: current,
            unique_total: self.seen.len() as u32,
            unique_by_class: unique,
        }
    }
}

/// Per-class counts, most numerous first.
fn tally(classes: impl Iterator<Item = usize>, labels: &[&str]) -> Vec<ClassCount> {
    let mut counts: BTreeMap<usize, u32> = BTreeMap::new();
    for class in classes {
        *counts.entry(class).or_default() += 1;
    }
    let mut result: Vec<ClassCount> = counts
        .into_iter()
        .map(|(class, count)| ClassCount {
            class: labels.get(class).copied().unwrap_or("object").to_string(),
            count,
        })
        .collect();
    result.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.class.cmp(&b.class)));
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vision::detector::BoundingBox;

    fn track(id: u64, class_id: usize) -> TrackView {
        TrackView {
            id,
            class_id,
            confidence: 0.9,
            bbox: BoundingBox {
                x: 0.0,
                y: 0.0,
                width: 0.1,
                height: 0.1,
            },
            velocity: (0.0, 0.0),
            seen_at: 0.0,
            hits: 5,
        }
    }

    #[test]
    fn counts_current_and_unique_animals() {
        let labels = ["a", "sheep", "cow"];
        let mut counter = ObjectCounter::default();
        let first = counter.update(&[track(1, 2), track(2, 1), track(3, 1)], &labels);
        assert_eq!(first.current_total, 3);
        assert_eq!(first.current_by_class[0].class, "sheep");
        assert_eq!(first.current_by_class[0].count, 2);

        // Track 3 left, track 4 arrived, track 1 is seen again.
        let second = counter.update(&[track(1, 2), track(4, 2)], &labels);
        assert_eq!(second.current_total, 2);
        assert_eq!(second.unique_total, 4);
        assert_eq!(
            second.unique_by_class,
            vec![
                ClassCount {
                    class: "cow".into(),
                    count: 2
                },
                ClassCount {
                    class: "sheep".into(),
                    count: 2
                },
            ]
        );

        counter.reset();
        assert_eq!(counter.update(&[], &labels).unique_total, 0);
    }

    #[test]
    fn short_lived_tracks_are_shown_but_not_counted_as_unique() {
        let labels = ["cow"];
        let mut counter = ObjectCounter::default();
        let mut young = track(1, 0);
        young.hits = 2;
        let summary = counter.update(&[young.clone()], &labels);
        assert_eq!(summary.current_total, 1);
        assert_eq!(summary.unique_total, 0);
        young.hits = 3;
        assert_eq!(counter.update(&[young], &labels).unique_total, 1);
    }

    #[test]
    fn a_relabelled_track_is_counted_once() {
        let labels = ["cat", "dog"];
        let mut counter = ObjectCounter::default();
        counter.update(&[track(7, 0)], &labels);
        let summary = counter.update(&[track(7, 1)], &labels);
        assert_eq!(summary.unique_total, 1);
        assert_eq!(summary.unique_by_class[0].class, "dog");
    }
}
