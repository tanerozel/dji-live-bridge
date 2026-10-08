//! Counts what the tracker currently sees and every distinct track it has
//! confirmed since the counter was last reset.

use std::collections::{BTreeMap, btree_map::Entry};

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
    /// Updated only when an id first counts or changes class, so reporting a
    /// frame never scans every animal observed during the entire session.
    unique_by_class: BTreeMap<usize, u32>,
}

impl ObjectCounter {
    pub fn reset(&mut self) {
        self.seen.clear();
        self.unique_by_class.clear();
    }

    pub fn update(&mut self, visible: &[TrackView], labels: &[&str]) -> CountSummary {
        for track in visible {
            match self.seen.entry(track.id) {
                Entry::Vacant(entry) if track.hits >= UNIQUE_MIN_HITS => {
                    entry.insert(track.class_id);
                    *self.unique_by_class.entry(track.class_id).or_default() += 1;
                }
                Entry::Occupied(mut entry) if *entry.get() != track.class_id => {
                    let previous = entry.insert(track.class_id);
                    let count = self
                        .unique_by_class
                        .get_mut(&previous)
                        .expect("counted track has a class tally");
                    *count -= 1;
                    if *count == 0 {
                        self.unique_by_class.remove(&previous);
                    }
                    *self.unique_by_class.entry(track.class_id).or_default() += 1;
                }
                _ => {}
            }
        }
        let current = tally(visible.iter().map(|track| track.class_id), labels);
        let unique = class_counts(
            self.unique_by_class
                .iter()
                .map(|(&class, &count)| (class, count)),
            labels,
        );
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
    class_counts(counts.into_iter(), labels)
}

fn class_counts(counts: impl Iterator<Item = (usize, u32)>, labels: &[&str]) -> Vec<ClassCount> {
    let mut result: Vec<ClassCount> = counts
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

    #[test]
    fn relabels_remove_empty_class_totals_and_preserve_absent_animals() {
        let labels = ["cat", "dog", "cow"];
        let mut counter = ObjectCounter::default();
        counter.update(&[track(1, 0), track(2, 1), track(3, 1)], &labels);
        let mut relabelled = track(1, 2);
        // An already counted id remains counted even when a caller supplies
        // fewer hits; relabelling must move, rather than duplicate, its tally.
        relabelled.hits = 1;
        let changed = counter.update(&[relabelled.clone()], &labels);
        assert_eq!(changed.unique_total, 3);
        assert_eq!(
            changed.unique_by_class,
            vec![
                ClassCount {
                    class: "dog".into(),
                    count: 2
                },
                ClassCount {
                    class: "cow".into(),
                    count: 1
                },
            ]
        );
        assert_eq!(
            counter.update(&[relabelled], &labels).unique_by_class,
            changed.unique_by_class
        );
        assert_eq!(
            counter.update(&[], &labels).unique_by_class,
            changed.unique_by_class
        );

        counter.reset();
        assert!(counter.update(&[], &labels).unique_by_class.is_empty());
        let restarted = counter.update(&[track(1, 0)], &labels);
        assert_eq!(restarted.unique_total, 1);
        assert_eq!(restarted.unique_by_class[0].class, "cat");
    }

    #[test]
    fn incremental_totals_match_a_long_session_with_relabels_and_resets() {
        let labels = ["cat", "dog", "cow", "sheep"];
        let mut counter = ObjectCounter::default();
        let mut recorded: BTreeMap<u64, usize> = BTreeMap::new();
        for frame in 0..400 {
            if frame == 250 {
                counter.reset();
                recorded.clear();
            }
            let visible: Vec<_> = (0..7)
                .map(|offset| {
                    let id = (frame * 3 + offset) as u64;
                    let mut animal = track(id, (frame + offset) % labels.len());
                    animal.hits = (frame + offset) as u32 % 5;
                    animal
                })
                .collect();
            for animal in &visible {
                if animal.hits >= UNIQUE_MIN_HITS || recorded.contains_key(&animal.id) {
                    recorded.insert(animal.id, animal.class_id);
                }
            }
            let summary = counter.update(&visible, &labels);
            assert_eq!(summary.unique_total, recorded.len() as u32);
            assert_eq!(
                summary.unique_by_class,
                tally(recorded.values().copied(), &labels)
            );
            assert_eq!(
                summary.current_by_class,
                tally(visible.iter().map(|animal| animal.class_id), &labels)
            );
        }
    }

    /// Self-contained before/after benchmark: no model, clip or running app.
    #[test]
    #[ignore]
    fn benchmark_incremental_history_count() {
        use std::{hint::black_box, time::Instant};

        let labels = super::super::settings::DetectionProfile::Animals.classes();
        let history: Vec<_> = (0..50_000)
            .map(|id| track(id, id as usize % labels.len()))
            .collect();
        let recorded: BTreeMap<_, _> = history
            .iter()
            .map(|animal| (animal.id, animal.class_id))
            .collect();
        let visible = &history[history.len() - 20..];
        let mut counter = ObjectCounter::default();
        counter.update(&history, labels);
        let iterations = 500;
        let started = Instant::now();
        for _ in 0..iterations {
            black_box(tally(recorded.values().copied(), labels));
        }
        let baseline = started.elapsed();
        let started = Instant::now();
        for _ in 0..iterations {
            black_box(counter.update(black_box(visible), labels));
        }
        let incremental = started.elapsed();
        println!(
            "counter: {iterations} frames, {} historical ids, {} visible; historical tally {:.1} ms, full incremental update {:.1} ms",
            history.len(),
            visible.len(),
            baseline.as_secs_f64() * 1000.0,
            incremental.as_secs_f64() * 1000.0,
        );
        assert_eq!(
            counter.update(&[], labels).unique_total,
            history.len() as u32
        );
    }
}
