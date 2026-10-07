//! What the user chose in the AI Vision panel. Stored in config.json next to
//! the rest of the app's settings.

use serde::{Deserialize, Serialize};

use super::models;
use crate::error::{BridgeError, BridgeResult};

/// How often a picture is handed to the detector. The drone video itself is
/// never slowed down; this only decides how fresh the boxes are.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum InferenceRate {
    /// 10 per second, slowed down whenever the model needs more than half of
    /// the time between two pictures, so a slow machine keeps CPU for video.
    #[default]
    Auto,
    Fps5,
    Fps10,
    Fps15,
}

impl InferenceRate {
    /// The fastest the frame tap ever delivers; no setting goes above it.
    pub const MAX_FPS: u32 = 15;

    fn target_fps(self) -> f64 {
        match self {
            Self::Auto | Self::Fps10 => 10.0,
            Self::Fps5 => 5.0,
            Self::Fps15 => 15.0,
        }
    }

    /// Seconds from the start of one inference to the start of the next.
    pub fn interval_seconds(self, inference_seconds: f64) -> f64 {
        let target = 1.0 / self.target_fps();
        match self {
            Self::Auto => target.max(inference_seconds * 2.0),
            _ => target,
        }
    }
}

/// The family of objects to look for. Only animals exist today; people,
/// vehicles or fire and smoke would be further profiles over the same
/// pipeline, each naming the model classes it reports.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum DetectionProfile {
    #[default]
    Animals,
}

impl DetectionProfile {
    /// Model labels this profile reports, in the order the UI lists them
    /// (farm animals first). A model reports the ones it knows: COCO models
    /// have no goat, donkey, poultry, pig or deer. Wild savanna animals are
    /// left out on purpose; they only produced false "elephants" over herds.
    pub fn classes(self) -> &'static [&'static str] {
        match self {
            Self::Animals => &[
                "goat",
                "sheep",
                "cow",
                "horse",
                "donkey",
                "dog",
                "cat",
                "chicken",
                "duck",
                "goose",
                "pig",
                "deer",
                "wild boar",
                "bird",
                "bear",
            ],
        }
    }

    /// The profile classes a model can report, in the profile's order.
    pub fn classes_of(self, labels: &[&str]) -> Vec<&'static str> {
        self.classes()
            .iter()
            .copied()
            .filter(|class| labels.contains(class))
            .collect()
    }

    /// A stable colour slot per class name, shared by the burned-in overlay
    /// and the preview, whichever model produced the label.
    pub fn colour_index(self, label: &str) -> usize {
        self.classes()
            .iter()
            .position(|class| *class == label)
            .unwrap_or(0)
    }

    /// The heading of the on-video counter ("Animals: 14").
    pub fn heading(self) -> &'static str {
        match self {
            Self::Animals => "Animals",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DetectionSettings {
    pub enabled: bool,
    pub show_boxes: bool,
    pub show_counter: bool,
    /// Detections below this are not shown or counted (0.05–0.95).
    pub confidence_threshold: f32,
    pub inference_rate: InferenceRate,
    pub model_id: String,
    pub profile: DetectionProfile,
    /// Classes of the profile to report; empty means all of them.
    pub classes: Vec<String>,
}

impl Default for DetectionSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            show_boxes: true,
            show_counter: true,
            confidence_threshold: 0.6,
            inference_rate: InferenceRate::Auto,
            model_id: models::default_model_id().into(),
            profile: DetectionProfile::Animals,
            classes: Vec::new(),
        }
    }
}

impl DetectionSettings {
    pub fn validate(&self) -> BridgeResult<()> {
        if !(0.05..=0.95).contains(&self.confidence_threshold) {
            return Err(BridgeError::Validation(
                "Confidence threshold must be between 0.05 and 0.95".into(),
            ));
        }
        if models::find(&self.model_id).is_none() {
            return Err(BridgeError::Validation(format!(
                "Unknown detection model '{}'",
                self.model_id
            )));
        }
        let known = self.profile.classes();
        if let Some(unknown) = self
            .classes
            .iter()
            .find(|class| !known.contains(&class.as_str()))
        {
            return Err(BridgeError::Validation(format!(
                "'{unknown}' is not a class of this detection profile"
            )));
        }
        Ok(())
    }

    /// Replaces whatever this version does not know with the default.
    pub fn sanitized(mut self) -> Self {
        if models::find(&self.model_id).is_none() {
            self.model_id = models::default_model_id().into();
        }
        if !(0.05..=0.95).contains(&self.confidence_threshold) {
            self.confidence_threshold = Self::default().confidence_threshold;
        }
        let known = self.profile.classes();
        self.classes.retain(|class| known.contains(&class.as_str()));
        self
    }

    /// The labels to report: the chosen classes, or the whole profile.
    pub fn active_classes(&self) -> Vec<&'static str> {
        self.profile
            .classes()
            .iter()
            .copied()
            .filter(|class| self.classes.is_empty() || self.classes.iter().any(|c| c == class))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_valid_and_off() {
        let settings = DetectionSettings::default();
        assert!(!settings.enabled);
        settings.validate().unwrap();
        assert_eq!(settings.active_classes().len(), 15);
    }

    #[test]
    fn missing_fields_fall_back_to_defaults() {
        let settings: DetectionSettings =
            serde_json::from_value(serde_json::json!({ "enabled": true })).unwrap();
        assert!(settings.enabled);
        assert!(settings.show_boxes);
        assert_eq!(settings.confidence_threshold, 0.6);
    }

    #[test]
    fn rejects_unknown_classes_and_bad_thresholds() {
        let mut settings = DetectionSettings {
            classes: vec!["person".into()],
            ..DetectionSettings::default()
        };
        assert!(settings.validate().is_err());
        settings.classes = vec!["cow".into(), "sheep".into()];
        settings.validate().unwrap();
        assert_eq!(settings.active_classes(), vec!["sheep", "cow"]);
        settings.classes = vec!["giraffe".into()];
        assert!(settings.validate().is_err());
        settings.confidence_threshold = 1.0;
        assert!(settings.validate().is_err());
    }

    #[test]
    fn sanitizing_replaces_unknown_values() {
        let settings = DetectionSettings {
            model_id: "removed-model".into(),
            confidence_threshold: 2.0,
            classes: vec!["cow".into(), "unicorn".into(), "zebra".into()],
            ..DetectionSettings::default()
        }
        .sanitized();
        settings.validate().unwrap();
        assert_eq!(settings.classes, vec!["cow".to_string()]);
    }

    #[test]
    fn models_report_only_the_animals_they_know() {
        let profile = DetectionProfile::Animals;
        let coco = profile.classes_of(&models::COCO_LABELS);
        assert_eq!(
            coco,
            vec!["sheep", "cow", "horse", "dog", "cat", "bird", "bear"]
        );
        assert_eq!(profile.classes_of(&models::OWL_LABELS)[0], "goat");
        assert_eq!(profile.colour_index("goat"), 0);
        assert_eq!(profile.colour_index("cow"), 2);
    }

    #[test]
    fn auto_rate_backs_off_for_a_slow_model() {
        assert_eq!(InferenceRate::Auto.interval_seconds(0.005), 0.1);
        assert_eq!(InferenceRate::Auto.interval_seconds(0.08), 0.16);
        assert_eq!(InferenceRate::Fps15.interval_seconds(0.08), 1.0 / 15.0);
    }
}
