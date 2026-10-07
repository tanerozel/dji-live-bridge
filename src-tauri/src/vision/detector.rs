//! The detector abstraction. Tracking, counting and the overlay only ever see
//! `Detection`, so another model family (MegaDetector, a custom-trained YOLO,
//! a fire/smoke model) is one more `ObjectDetector`.

use serde::Serialize;

use crate::error::BridgeResult;

/// A box in normalised coordinates: 0..1 of the source picture's width and
/// height, so it maps onto any output size.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct BoundingBox {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl BoundingBox {
    pub fn area(&self) -> f32 {
        self.width.max(0.0) * self.height.max(0.0)
    }

    pub fn iou(&self, other: &Self) -> f32 {
        let left = self.x.max(other.x);
        let top = self.y.max(other.y);
        let right = (self.x + self.width).min(other.x + other.width);
        let bottom = (self.y + self.height).min(other.y + other.height);
        let intersection = (right - left).max(0.0) * (bottom - top).max(0.0);
        let union = self.area() + other.area() - intersection;
        if union <= 0.0 {
            0.0
        } else {
            intersection / union
        }
    }

    /// Clipped to the picture.
    pub fn clamped(&self) -> Self {
        let x = self.x.clamp(0.0, 1.0);
        let y = self.y.clamp(0.0, 1.0);
        Self {
            x,
            y,
            width: (self.x + self.width).clamp(0.0, 1.0) - x,
            height: (self.y + self.height).clamp(0.0, 1.0) - y,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Detection {
    /// Index into the detector's `labels()`.
    pub class_id: usize,
    pub confidence: f32,
    pub bbox: BoundingBox,
}

/// One picture, already scaled by FFmpeg to fit inside the detector's square
/// input (aspect ratio kept). RGB, three bytes per pixel, no row padding.
#[derive(Debug, Clone)]
pub struct RgbFrame {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

pub struct DetectOptions<'a> {
    /// Candidates below this are dropped before suppression.
    pub min_confidence: f32,
    /// `wanted[class_id]`: classes to report. Others are ignored entirely.
    pub wanted: &'a [bool],
}

pub trait ObjectDetector: Send {
    /// Edge of the square input FFmpeg should scale pictures to fit.
    fn input_size(&self) -> u32;
    fn labels(&self) -> &[&'static str];
    /// Where inference actually runs, for the UI ("CoreML", "CPU").
    fn backend(&self) -> &'static str;
    fn detect(&mut self, frame: &RgbFrame, options: &DetectOptions)
    -> BridgeResult<Vec<Detection>>;
}

/// Greedy non-maximum suppression across classes: one box per object, even
/// when the model is unsure whether it is looking at a cat or a dog.
pub fn suppress_overlaps(
    mut detections: Vec<Detection>,
    iou_threshold: f32,
    limit: usize,
) -> Vec<Detection> {
    detections.sort_by(|a, b| b.confidence.total_cmp(&a.confidence));
    let mut kept: Vec<Detection> = Vec::new();
    for detection in detections {
        if kept.len() >= limit {
            break;
        }
        if kept
            .iter()
            .all(|existing| existing.bbox.iou(&detection.bbox) <= iou_threshold)
        {
            kept.push(detection);
        }
    }
    kept
}

/// Drops boxes drawn around a whole group: a box that mostly contains two or
/// more other detections is the herd, not an animal. OWLv2 scores such group
/// boxes as confidently as the animals in them, and suppression keeps them
/// because they overlap each animal only a little.
pub fn drop_group_boxes(detections: Vec<Detection>) -> Vec<Detection> {
    let inside = |inner: &BoundingBox, outer: &BoundingBox| {
        let left = inner.x.max(outer.x);
        let top = inner.y.max(outer.y);
        let right = (inner.x + inner.width).min(outer.x + outer.width);
        let bottom = (inner.y + inner.height).min(outer.y + outer.height);
        let overlap = (right - left).max(0.0) * (bottom - top).max(0.0);
        inner.area() > 0.0 && overlap >= inner.area() * 0.8
    };
    let groups: Vec<bool> = detections
        .iter()
        .enumerate()
        .map(|(index, outer)| {
            detections
                .iter()
                .enumerate()
                .filter(|(other, inner)| {
                    *other != index
                        && inner.bbox.area() < outer.bbox.area() * 0.5
                        && inside(&inner.bbox, &outer.bbox)
                })
                .count()
                >= 2
        })
        .collect();
    detections
        .into_iter()
        .zip(groups)
        .filter(|(_, group)| !group)
        .map(|(detection, _)| detection)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_box_around_several_animals_is_dropped() {
        let animal = |x: f32| Detection {
            class_id: 0,
            confidence: 0.5,
            bbox: bbox(x, 0.4, 0.05, 0.08),
        };
        let herd = Detection {
            class_id: 0,
            confidence: 0.6,
            bbox: bbox(0.28, 0.38, 0.3, 0.12),
        };
        let kept = drop_group_boxes(vec![herd.clone(), animal(0.3), animal(0.4), animal(0.5)]);
        assert_eq!(kept.len(), 3);
        assert!(!kept.contains(&herd));
        // An animal next to one other is not a group.
        let pair = drop_group_boxes(vec![herd, animal(0.3)]);
        assert_eq!(pair.len(), 2);
    }

    fn bbox(x: f32, y: f32, width: f32, height: f32) -> BoundingBox {
        BoundingBox {
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn iou_of_identical_disjoint_and_half_overlapping_boxes() {
        let a = bbox(0.0, 0.0, 0.2, 0.2);
        assert!((a.iou(&a) - 1.0).abs() < 1e-6);
        assert_eq!(a.iou(&bbox(0.5, 0.5, 0.2, 0.2)), 0.0);
        let half = a.iou(&bbox(0.1, 0.0, 0.2, 0.2));
        assert!((half - 1.0 / 3.0).abs() < 1e-6);
    }

    #[test]
    fn suppression_keeps_one_box_per_object_across_classes() {
        let detections = vec![
            Detection {
                class_id: 15,
                confidence: 0.54,
                bbox: bbox(0.1, 0.1, 0.2, 0.3),
            },
            Detection {
                class_id: 16,
                confidence: 0.55,
                bbox: bbox(0.1, 0.1, 0.21, 0.3),
            },
            Detection {
                class_id: 19,
                confidence: 0.9,
                bbox: bbox(0.6, 0.6, 0.1, 0.1),
            },
        ];
        let kept = suppress_overlaps(detections, 0.5, 10);
        assert_eq!(kept.len(), 2);
        assert_eq!(kept[0].class_id, 19);
        assert_eq!(kept[1].class_id, 16);
    }

    #[test]
    fn clamping_keeps_boxes_inside_the_picture() {
        let clamped = bbox(-0.1, 0.9, 0.3, 0.3).clamped();
        assert_eq!(clamped.x, 0.0);
        assert!((clamped.width - 0.2).abs() < 1e-6);
        assert!((clamped.height - 0.1).abs() < 1e-6);
    }
}
