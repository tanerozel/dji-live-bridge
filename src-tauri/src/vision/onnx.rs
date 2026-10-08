//! Detectors on ONNX Runtime: YOLOX, the DETR family and OWLv2.
//!
//! macOS asks for the CoreML execution provider (Neural Engine/GPU) and falls
//! back to the CPU if CoreML cannot take the model. Windows uses the CPU
//! provider: DirectML needs a DirectML.dll newer than the one Windows ships,
//! which would have to be bundled and tested on real GPUs first.

use std::path::Path;

use ort::{
    session::{
        Session,
        builder::{GraphOptimizationLevel, SessionBuilder},
    },
    value::TensorRef,
};

use super::{
    detector::{
        BoundingBox, DetectOptions, Detection, ObjectDetector, RgbFrame, drop_group_boxes,
        suppress_overlaps,
    },
    models::{ChannelOrder, ModelSpec, OutputFormat, PROMPT_TOKENS},
};
use crate::error::{BridgeError, BridgeResult};

/// Boxes overlapping more than this are the same object.
const SUPPRESSION_IOU: f32 = 0.5;
/// A drone shot of a herd can hold many animals; more than this is noise.
const MAX_DETECTIONS: usize = 200;
/// Threads ONNX Runtime may use on the CPU; the rest stay with FFmpeg.
const CPU_THREADS: usize = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CoremlStrategy {
    Default,
    BinaryWeights,
    #[cfg(test)]
    FastPrediction,
}

/// Only the shape-folded macOS model needs the CoreML weight workaround.
/// CPU-only platforms keep their original graph optimizations and cache.
fn production_strategy(spec: &ModelSpec) -> CoremlStrategy {
    if cfg!(target_os = "macos") && spec.fold_shapes {
        CoremlStrategy::BinaryWeights
    } else {
        CoremlStrategy::Default
    }
}

pub struct OnnxDetector {
    session: Session,
    spec: &'static ModelSpec,
    input: ModelInput,
    /// `input_ids` and `attention_mask` of an open-vocabulary model; the
    /// prompts never change, so they are built once.
    prompts: Option<(Vec<i64>, Vec<i64>)>,
    backend: &'static str,
    #[cfg(test)]
    last_timings: DetectionTimings,
}

impl OnnxDetector {
    pub fn load(
        spec: &'static ModelSpec,
        model: &Path,
        compiled_cache: &Path,
    ) -> BridgeResult<Self> {
        Self::load_with_strategy(spec, model, compiled_cache, production_strategy(spec))
    }

    fn load_with_strategy(
        spec: &'static ModelSpec,
        model: &Path,
        compiled_cache: &Path,
        specialization: CoremlStrategy,
    ) -> BridgeResult<Self> {
        std::fs::create_dir_all(compiled_cache).ok();
        let folded;
        let model = if spec.fold_shapes {
            folded =
                fold_shapes(spec, model, compiled_cache, specialization).map_err(vision_error)?;
            folded.as_path()
        } else {
            model
        };
        #[cfg(target_os = "macos")]
        {
            match build_session(model, spec, Some(compiled_cache), specialization) {
                Ok(session) => return Ok(Self::new(session, spec, "CoreML")),
                Err(error) => {
                    tracing::warn!(%error, "CoreML could not load the model; using the CPU")
                }
            }
        }
        let session =
            build_session(model, spec, None, CoremlStrategy::Default).map_err(vision_error)?;
        Ok(Self::new(session, spec, "CPU"))
    }

    fn new(session: Session, spec: &'static ModelSpec, backend: &'static str) -> Self {
        let prompts = spec.text_prompts.map(|prompts| {
            let mut ids = vec![0; prompts.len() * PROMPT_TOKENS];
            let mut mask = vec![0; prompts.len() * PROMPT_TOKENS];
            for (row, tokens) in prompts.iter().enumerate() {
                let at = row * PROMPT_TOKENS;
                ids[at..at + tokens.len()].copy_from_slice(tokens);
                mask[at..at + tokens.len()].fill(1);
            }
            (ids, mask)
        });
        Self {
            session,
            spec,
            input: ModelInput::new(spec),
            prompts,
            backend,
            #[cfg(test)]
            last_timings: DetectionTimings::default(),
        }
    }
}

/// Writes (once) a copy of the model with every input size pinned and the
/// shape arithmetic folded into constants. The OWLv2 export computes sizes
/// with `Shape` nodes; given the original, ONNX Runtime hands CoreML a
/// partition that expects those sizes as inputs and every run fails
/// ("Feature ..._Shape_output_0 is required but not specified"). Folded,
/// CoreML takes the whole vision tower: ~440 ms instead of 3.5 s on the CPU.
fn fold_shapes(
    spec: &ModelSpec,
    model: &Path,
    cache: &Path,
    specialization: CoremlStrategy,
) -> ort::Result<std::path::PathBuf> {
    // Legacy folded graphs already contain the fused Gemms. A new filename
    // also gives CoreML a new cache key without deleting existing models.
    let stem = if specialization == CoremlStrategy::BinaryWeights {
        "folded-binary-weights-v1"
    } else {
        "folded"
    };
    let folded = cache.join(format!("{stem}.onnx"));
    if folded.is_file() {
        return Ok(folded);
    }
    // ONNX Runtime picks the format from the extension, so keep ".onnx".
    let partial = cache.join(format!("{stem}.part.onnx"));
    let builder = Session::builder()?
        .with_optimization_level(GraphOptimizationLevel::Level1)?
        .with_intra_threads(CPU_THREADS)?
        .with_optimized_model_path(&partial)?;
    let builder = keep_binary_weights(builder, specialization)?;
    pin_dimensions(builder, spec)?.commit_from_file(model)?;
    std::fs::rename(&partial, &folded)
        .map_err(|error| ort::Error::new(format!("could not keep the folded model: {error}")))?;
    Ok(folded)
}

/// Exports with dynamic input sizes (`[batch_size, 3, height, width]`, as the
/// DETR ones are) are pinned to the one size the tap delivers, and an
/// open-vocabulary model to its fixed prompts. CoreML needs that: with open
/// dimensions it either rejects the graph or, for RF-DETR, aborts the whole
/// process inside MPSGraph. Names a model does not use are ignored.
fn pin_dimensions(builder: SessionBuilder, spec: &ModelSpec) -> ort::Result<SessionBuilder> {
    let edge = i64::from(spec.input_size);
    let mut builder = builder
        .with_dimension_override("batch_size", 1)?
        .with_dimension_override("height", edge)?
        .with_dimension_override("width", edge)?;
    if let Some(prompts) = spec.text_prompts {
        builder = builder
            .with_dimension_override("image_batch_size", 1)?
            .with_dimension_override("num_channels", 3)?
            .with_dimension_override("text_batch_size", prompts.len() as i64)?
            .with_dimension_override("sequence_length", PROMPT_TOKENS as i64)?;
    }
    Ok(builder)
}

/// Pinned sizes also make D-FINE run in ~90 ms instead of ~210 ms on CoreML.
fn build_session(
    model: &Path,
    spec: &ModelSpec,
    coreml_cache: Option<&Path>,
    specialization: CoremlStrategy,
) -> ort::Result<Session> {
    let builder = Session::builder()?
        .with_optimization_level(GraphOptimizationLevel::Level3)?
        .with_intra_threads(CPU_THREADS)?;
    let builder = keep_binary_weights(builder, specialization)?;
    let builder = pin_dimensions(builder, spec)?;
    #[cfg(target_os = "macos")]
    let builder = match coreml_cache {
        Some(cache) => {
            let provider = ort::ep::CoreML::default()
                .with_model_format(ort::ep::coreml::ModelFormat::MLProgram)
                .with_static_input_shapes(true)
                .with_model_cache_dir(coreml_cache_dir(cache, specialization).display());
            let provider = match specialization {
                // Preserve the existing provider configuration, including
                // CoreML's default optimization hints.
                CoremlStrategy::Default | CoremlStrategy::BinaryWeights => provider,
                #[cfg(test)]
                CoremlStrategy::FastPrediction => provider.with_specialization_strategy(
                    ort::ep::coreml::SpecializationStrategy::FastPrediction,
                ),
            };
            builder.with_execution_providers([provider.build().error_on_failure()])?
        }
        None => builder,
    };
    #[cfg(not(target_os = "macos"))]
    let _ = (coreml_cache, specialization);
    let mut builder = builder;
    builder.commit_from_file(model)
}

/// ORT's Gemm builder embeds transposed weights as large inline constants.
/// Keeping MatMul + Add separate lets the MLProgram builder put those same
/// weights in weight.bin, avoiding expensive MIL serialization on every load.
/// Shape folding and all other graph optimizers remain enabled.
fn keep_binary_weights(
    builder: SessionBuilder,
    specialization: CoremlStrategy,
) -> ort::Result<SessionBuilder> {
    if specialization == CoremlStrategy::BinaryWeights {
        Ok(builder.with_disabled_optimizers("MatMulAddFusion")?)
    } else {
        Ok(builder)
    }
}

/// CoreML caches models independently of session options. Keep specializations
/// in separate directories so a benchmark cannot reuse the other strategy.
#[cfg(any(target_os = "macos", test))]
fn coreml_cache_dir(cache: &Path, specialization: CoremlStrategy) -> std::path::PathBuf {
    match specialization {
        // Preserve already compiled models: Default has not changed and
        // moving its cache would make users compile the whole model again.
        CoremlStrategy::Default | CoremlStrategy::BinaryWeights => cache.to_path_buf(),
        #[cfg(test)]
        CoremlStrategy::FastPrediction => cache.join("coreml-fast-prediction-v1"),
    }
}

#[cfg(test)]
#[derive(Debug, Default, Clone, Copy)]
struct DetectionTimings {
    preprocess_ms: f64,
    model_ms: f64,
    postprocess_ms: f64,
    total_ms: f64,
}

fn vision_error(error: impl std::fmt::Display) -> BridgeError {
    BridgeError::Vision(error.to_string())
}

impl ObjectDetector for OnnxDetector {
    fn input_size(&self) -> u32 {
        self.spec.input_size
    }

    fn labels(&self) -> &[&'static str] {
        self.spec.labels
    }

    fn backend(&self) -> &'static str {
        self.backend
    }

    fn detect(
        &mut self,
        frame: &RgbFrame,
        options: &DetectOptions,
    ) -> BridgeResult<Vec<Detection>> {
        #[cfg(test)]
        let started = std::time::Instant::now();
        let size = self.spec.input_size as usize;
        self.input.fill(frame)?;
        let tensor =
            TensorRef::from_array_view(([1_usize, 3, size, size], self.input.pixels.as_slice()))
                .map_err(vision_error)?;
        #[cfg(test)]
        let preprocess_ms = started.elapsed().as_secs_f64() * 1000.0;
        #[cfg(test)]
        let model_started = std::time::Instant::now();
        let outputs = match &self.prompts {
            Some((ids, mask)) => {
                let shape = [ids.len() / PROMPT_TOKENS, PROMPT_TOKENS];
                let ids =
                    TensorRef::from_array_view((shape, ids.as_slice())).map_err(vision_error)?;
                let mask =
                    TensorRef::from_array_view((shape, mask.as_slice())).map_err(vision_error)?;
                self.session.run(ort::inputs![
                    "pixel_values" => tensor,
                    "input_ids" => ids,
                    "attention_mask" => mask,
                ])
            }
            None => self.session.run(ort::inputs![tensor]),
        }
        .map_err(vision_error)?;
        #[cfg(test)]
        let model_ms = model_started.elapsed().as_secs_f64() * 1000.0;
        #[cfg(test)]
        let postprocess_started = std::time::Instant::now();
        // Decoders compare raw scores, so the threshold is unscaled first.
        let scale = self.spec.score_scale;
        let options = &DetectOptions {
            min_confidence: options.min_confidence / scale,
            wanted: options.wanted,
        };
        let labels = self.spec.labels.len();
        let tensor = |name: Option<&str>| {
            let value = match name {
                Some(name) => outputs.get(name).ok_or_else(|| {
                    BridgeError::Vision(format!("the model has no '{name}' output"))
                })?,
                None => &outputs[0],
            };
            value.try_extract_tensor::<f32>().map_err(vision_error)
        };
        let candidates = match self.spec.format {
            OutputFormat::Yolox => {
                let (shape, values) = tensor(None)?;
                decode_yolox(values, shape, size, labels, options)?
            }
            OutputFormat::Ultralytics => {
                let (shape, values) = tensor(None)?;
                decode_ultralytics(values, shape, labels, options)?
            }
            OutputFormat::Detr => {
                let (logit_shape, logits) = tensor(Some("logits"))?;
                let (box_shape, boxes) = tensor(Some("pred_boxes"))?;
                decode_detr(logits, logit_shape, boxes, box_shape, size, labels, options)?
            }
        };
        let (width, height) = (frame.width as f32, frame.height as f32);
        let detections = drop_group_boxes(suppress_overlaps(
            candidates,
            SUPPRESSION_IOU,
            MAX_DETECTIONS,
        ))
        .into_iter()
        .map(|mut detection| {
            detection.confidence = (detection.confidence * scale).min(1.0);
            // The picture sits at the top-left of the input, unscaled,
            // so input pixels divided by its size are source fractions.
            detection.bbox = BoundingBox {
                x: detection.bbox.x / width,
                y: detection.bbox.y / height,
                width: detection.bbox.width / width,
                height: detection.bbox.height / height,
            }
            .clamped();
            detection
        })
        .filter(|detection| detection.bbox.area() > 0.0)
        .collect();
        #[cfg(test)]
        {
            self.last_timings = DetectionTimings {
                preprocess_ms,
                model_ms,
                postprocess_ms: postprocess_started.elapsed().as_secs_f64() * 1000.0,
                total_ms: started.elapsed().as_secs_f64() * 1000.0,
            };
        }
        Ok(detections)
    }
}

/// The NCHW tensor and normalization are reused for the detector's lifetime.
/// Letterbox padding is untouched while the picture dimensions stay the same.
struct ModelInput {
    pixels: Vec<f32>,
    values: [[f32; 256]; 3],
    padding: [f32; 3],
    order: [usize; 3],
    size: usize,
    picture_size: Option<(u32, u32)>,
}

impl ModelInput {
    fn new(spec: &ModelSpec) -> Self {
        // Keep the original operation order, including the division, so every
        // lookup value is bit-identical to the previous per-pixel computation.
        let (mean, std) = spec.normalize.map_or(([0.0; 3], [1.0; 3]), |normalize| {
            (normalize.mean, normalize.std)
        });
        let value =
            |channel: usize, raw: f32| (raw * spec.pixel_scale - mean[channel]) / std[channel];
        Self {
            pixels: Vec::new(),
            values: std::array::from_fn(|channel| {
                std::array::from_fn(|raw| value(channel, raw as f32))
            }),
            padding: std::array::from_fn(|channel| value(channel, spec.pad_value)),
            order: match spec.channel_order {
                ChannelOrder::Rgb => [0, 1, 2],
                ChannelOrder::Bgr => [2, 1, 0],
            },
            size: spec.input_size as usize,
            picture_size: None,
        }
    }

    fn fill(&mut self, frame: &RgbFrame) -> BridgeResult<()> {
        let (width, height) = (frame.width as usize, frame.height as usize);
        if width == 0 || height == 0 || width > self.size || height > self.size {
            return Err(BridgeError::Vision(format!(
                "a {width}x{height} picture does not fit the {}x{} model input",
                self.size, self.size
            )));
        }
        if frame.pixels.len() != width * height * 3 {
            return Err(BridgeError::Vision("picture data is truncated".into()));
        }
        let plane = self.size * self.size;
        let picture_size = (frame.width, frame.height);
        if self.picture_size != Some(picture_size) {
            self.pixels.resize(plane * 3, 0.0);
            for (pixels, padding) in self.pixels.chunks_exact_mut(plane).zip(self.padding) {
                pixels.fill(padding);
            }
            self.picture_size = Some(picture_size);
        }
        for ((pixels, values), channel) in self
            .pixels
            .chunks_exact_mut(plane)
            .zip(&self.values)
            .zip(self.order)
        {
            for (target, source) in pixels
                .chunks_exact_mut(self.size)
                .take(height)
                .zip(frame.pixels.chunks_exact(width * 3))
            {
                for (target, pixel) in target[..width].iter_mut().zip(source.as_chunks::<3>().0) {
                    *target = values[usize::from(pixel[channel])];
                }
            }
        }
        Ok(())
    }
}

/// The best wanted class of one candidate, scored by `score(class)`.
fn best_class(
    labels: usize,
    options: &DetectOptions,
    score: impl Fn(usize) -> f32,
) -> Option<(usize, f32)> {
    (0..labels)
        .filter(|class| options.wanted.get(*class).copied().unwrap_or(false))
        .map(|class| (class, score(class)))
        .filter(|(_, confidence)| *confidence >= options.min_confidence)
        .max_by(|a, b| a.1.total_cmp(&b.1))
}

fn decode_yolox(
    values: &[f32],
    shape: &[i64],
    input_size: usize,
    labels: usize,
    options: &DetectOptions,
) -> BridgeResult<Vec<Detection>> {
    let strides = [8_usize, 16, 32];
    let anchors: usize = strides.iter().map(|s| (input_size / s).pow(2)).sum();
    let row = 5 + labels;
    if shape != [1, anchors as i64, row as i64] || values.len() != anchors * row {
        return Err(BridgeError::Vision(format!(
            "unexpected YOLOX output shape {shape:?}"
        )));
    }
    let mut detections = Vec::new();
    let mut anchor = 0;
    for stride in strides {
        let grid = input_size / stride;
        for grid_y in 0..grid {
            for grid_x in 0..grid {
                let candidate = &values[anchor * row..(anchor + 1) * row];
                anchor += 1;
                let objectness = candidate[4];
                if objectness < options.min_confidence {
                    continue;
                }
                let Some((class_id, confidence)) =
                    best_class(labels, options, |class| objectness * candidate[5 + class])
                else {
                    continue;
                };
                let stride = stride as f32;
                let centre_x = (candidate[0] + grid_x as f32) * stride;
                let centre_y = (candidate[1] + grid_y as f32) * stride;
                let width = candidate[2].exp() * stride;
                let height = candidate[3].exp() * stride;
                detections.push(Detection {
                    class_id,
                    confidence,
                    bbox: BoundingBox {
                        x: centre_x - width / 2.0,
                        y: centre_y - height / 2.0,
                        width,
                        height,
                    },
                });
            }
        }
    }
    Ok(detections)
}

fn decode_ultralytics(
    values: &[f32],
    shape: &[i64],
    labels: usize,
    options: &DetectOptions,
) -> BridgeResult<Vec<Detection>> {
    let rows = 4 + labels;
    let [1, channels, anchors] = shape else {
        return Err(BridgeError::Vision(format!(
            "unexpected YOLO output shape {shape:?}"
        )));
    };
    let (channels, anchors) = (*channels as usize, *anchors as usize);
    if channels != rows || values.len() != rows * anchors {
        return Err(BridgeError::Vision(format!(
            "unexpected YOLO output shape {shape:?}"
        )));
    }
    let at = |channel: usize, anchor: usize| values[channel * anchors + anchor];
    let mut detections = Vec::new();
    for anchor in 0..anchors {
        let Some((class_id, confidence)) =
            best_class(labels, options, |class| at(4 + class, anchor))
        else {
            continue;
        };
        let (width, height) = (at(2, anchor), at(3, anchor));
        detections.push(Detection {
            class_id,
            confidence,
            bbox: BoundingBox {
                x: at(0, anchor) - width / 2.0,
                y: at(1, anchor) - height / 2.0,
                width,
                height,
            },
        });
    }
    Ok(detections)
}

/// DETR outputs: one candidate per query, sigmoid class scores, boxes as
/// fractions of the input (centre x/y, width, height).
fn decode_detr(
    logits: &[f32],
    logit_shape: &[i64],
    boxes: &[f32],
    box_shape: &[i64],
    input_size: usize,
    labels: usize,
    options: &DetectOptions,
) -> BridgeResult<Vec<Detection>> {
    let [1, queries, classes] = logit_shape else {
        return Err(BridgeError::Vision(format!(
            "unexpected DETR logits shape {logit_shape:?}"
        )));
    };
    let (queries, classes) = (*queries as usize, *classes as usize);
    if classes != labels
        || logits.len() != queries * classes
        || box_shape != [1, queries as i64, 4]
        || boxes.len() != queries * 4
    {
        return Err(BridgeError::Vision(format!(
            "unexpected DETR output shapes {logit_shape:?} / {box_shape:?}"
        )));
    }
    let sigmoid = |value: f32| 1.0 / (1.0 + (-value).exp());
    let scale = input_size as f32;
    let mut detections = Vec::new();
    for query in 0..queries {
        let scores = &logits[query * classes..(query + 1) * classes];
        let Some((class_id, confidence)) =
            best_class(labels, options, |class| sigmoid(scores[class]))
        else {
            continue;
        };
        let bbox = &boxes[query * 4..query * 4 + 4];
        let (width, height) = (bbox[2] * scale, bbox[3] * scale);
        detections.push(Detection {
            class_id,
            confidence,
            bbox: BoundingBox {
                x: bbox[0] * scale - width / 2.0,
                y: bbox[1] * scale - height / 2.0,
                width,
                height,
            },
        });
    }
    Ok(detections)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vision::models::{CATALOG, COCO_LABELS};

    fn wanted(classes: &[usize]) -> Vec<bool> {
        (0..80).map(|class| classes.contains(&class)).collect()
    }

    #[test]
    fn letterbox_is_top_left_bgr_and_padded() {
        let spec = &CATALOG[0];
        let frame = RgbFrame {
            width: 2,
            height: 1,
            pixels: vec![10, 20, 30, 40, 50, 60],
        };
        let mut input = ModelInput::new(spec);
        input.fill(&frame).unwrap();
        let buffer = &input.pixels;
        let plane = 416 * 416;
        assert_eq!(buffer.len(), plane * 3);
        // BGR: blue first.
        assert_eq!(&buffer[0..2], &[30.0, 60.0]);
        assert_eq!(buffer[plane], 20.0);
        assert_eq!(buffer[2 * plane + 1], 40.0);
        assert_eq!(buffer[2], 114.0);
        assert_eq!(buffer[416], 114.0);
    }

    #[test]
    fn normalised_models_pad_and_scale_per_channel() {
        let spec = CATALOG.iter().find(|spec| spec.id == "owlv2-base").unwrap();
        let frame = RgbFrame {
            width: 1,
            height: 1,
            pixels: vec![255, 0, 128],
        };
        let mut input = ModelInput::new(spec);
        input.fill(&frame).unwrap();
        let buffer = &input.pixels;
        let plane = 960 * 960;
        let normalize = spec.normalize.unwrap();
        let expect = |channel: usize, raw: f32| {
            (raw / 255.0 - normalize.mean[channel]) / normalize.std[channel]
        };
        assert!((buffer[0] - expect(0, 255.0)).abs() < 1e-5);
        assert!((buffer[plane] - expect(1, 0.0)).abs() < 1e-5);
        assert!((buffer[2 * plane] - expect(2, 128.0)).abs() < 1e-5);
        // Padding is mid-grey, normalised like a pixel.
        assert!((buffer[1] - expect(0, 127.5)).abs() < 1e-5);
        assert!((buffer[2 * plane + 5] - expect(2, 127.5)).abs() < 1e-5);
    }

    #[test]
    fn oversized_pictures_are_rejected() {
        let frame = RgbFrame {
            width: 500,
            height: 10,
            pixels: vec![0; 500 * 10 * 3],
        };
        assert!(ModelInput::new(&CATALOG[0]).fill(&frame).is_err());
    }

    /// The previous preprocessing formula, applied independently to every
    /// tensor cell, is the bit-level reference for each catalog model.
    fn reference_input(frame: &RgbFrame, spec: &ModelSpec) -> Vec<f32> {
        let size = spec.input_size as usize;
        let plane = size * size;
        let (mean, std) = spec.normalize.map_or(([0.0; 3], [1.0; 3]), |normalize| {
            (normalize.mean, normalize.std)
        });
        (0..plane * 3)
            .map(|at| {
                let channel = at / plane;
                let (x, y) = (at % size, (at % plane) / size);
                let raw = if x < frame.width as usize && y < frame.height as usize {
                    let source_channel = match spec.channel_order {
                        ChannelOrder::Rgb => channel,
                        ChannelOrder::Bgr => 2 - channel,
                    };
                    f32::from(frame.pixels[(y * frame.width as usize + x) * 3 + source_channel])
                } else {
                    spec.pad_value
                };
                (raw * spec.pixel_scale - mean[channel]) / std[channel]
            })
            .collect()
    }

    #[test]
    fn cached_input_is_bit_identical_for_all_models_and_pixel_values() {
        // Every channel sees every u8 value, including the extrema. This covers
        // BGR, RGB, unscaled, scaled and CLIP-normalized catalog inputs.
        let frame = RgbFrame {
            width: 16,
            height: 16,
            pixels: (0..256)
                .flat_map(|value| [value as u8, (255 - value) as u8, (value ^ 0xaa) as u8])
                .collect(),
        };
        for spec in CATALOG {
            let mut input = ModelInput::new(spec);
            input.fill(&frame).unwrap();
            let expected = reference_input(&frame, spec);
            assert!(
                input
                    .pixels
                    .iter()
                    .zip(expected)
                    .all(|(actual, expected)| actual.to_bits() == expected.to_bits()),
                "{} changed its preprocessing tensor",
                spec.id
            );
        }
    }

    #[test]
    fn cached_padding_is_restored_when_frame_dimensions_change() {
        let spec = ModelSpec {
            input_size: 16,
            ..CATALOG[0]
        };
        let mut input = ModelInput::new(&spec);
        let mut allocation = None;
        for (index, (width, height)) in [(16, 16), (16, 16), (8, 4), (4, 8), (16, 16)]
            .into_iter()
            .enumerate()
        {
            let frame = RgbFrame {
                width,
                height,
                pixels: vec![(index * 40) as u8; (width * height * 3) as usize],
            };
            input.fill(&frame).unwrap();
            assert_eq!(input.pixels, reference_input(&frame, &spec));
            assert_eq!(
                *allocation.get_or_insert(input.pixels.as_ptr()),
                input.pixels.as_ptr()
            );
        }
    }

    #[test]
    fn rejected_frame_does_not_change_cached_input_or_padding() {
        let spec = ModelSpec {
            input_size: 16,
            ..CATALOG[0]
        };
        let mut input = ModelInput::new(&spec);
        let frame = RgbFrame {
            width: 8,
            height: 4,
            pixels: vec![255; 8 * 4 * 3],
        };
        input.fill(&frame).unwrap();
        let before = input.pixels.clone();
        assert!(
            input
                .fill(&RgbFrame {
                    width: 4,
                    height: 8,
                    pixels: vec![0; 1],
                })
                .is_err()
        );
        assert_eq!(input.picture_size, Some((8, 4)));
        assert_eq!(input.pixels, before);
        input.fill(&frame).unwrap();
        assert_eq!(input.pixels, reference_input(&frame, &spec));
    }

    #[test]
    fn coreml_specialization_caches_do_not_overlap() {
        let root = Path::new("compiled");
        assert_eq!(coreml_cache_dir(root, CoremlStrategy::Default), root);
        assert_ne!(
            coreml_cache_dir(root, CoremlStrategy::Default),
            coreml_cache_dir(root, CoremlStrategy::FastPrediction)
        );
    }

    #[test]
    fn binary_weights_preserve_matmul_and_migrate_the_folded_cache() {
        // Synthetic X[1,2] @ W[2,2] + B[2]; see fixtures/README.md.
        let root = std::env::temp_dir().join(format!("dlb-fold-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let source = root.join("source.onnx");
        std::fs::write(&source, include_bytes!("fixtures/matmul-add.onnx")).unwrap();
        let spec = CATALOG.iter().find(|spec| spec.id == "owlv2-base").unwrap();
        let legacy = fold_shapes(spec, &source, &root, CoremlStrategy::Default).unwrap();
        let legacy_bytes = std::fs::read(&legacy).unwrap();
        assert!(
            legacy_bytes
                .windows(6)
                .any(|bytes| bytes == b"\x22\x04Gemm")
        );
        let binary = fold_shapes(spec, &source, &root, CoremlStrategy::BinaryWeights).unwrap();
        let binary_bytes = std::fs::read(&binary).unwrap();
        assert_ne!(legacy, binary, "the old fused graph must not be reused");
        assert_eq!(std::fs::read(&legacy).unwrap(), legacy_bytes);
        assert!(
            binary_bytes
                .windows(8)
                .any(|bytes| bytes == b"\x22\x06MatMul")
        );
        assert!(
            !binary_bytes
                .windows(6)
                .any(|bytes| bytes == b"\x22\x04Gemm")
        );
        // A warm load uses the new graph even if the source is unavailable.
        std::fs::remove_file(&source).unwrap();
        assert_eq!(
            fold_shapes(spec, &source, &root, CoremlStrategy::BinaryWeights).unwrap(),
            binary
        );
        for (path, strategy) in [
            (&legacy, CoremlStrategy::Default),
            (&binary, CoremlStrategy::BinaryWeights),
        ] {
            let mut session = build_session(path, spec, None, strategy).unwrap();
            for (input, expected) in [([5.0_f32, 6.0], [24.0_f32, 36.0]), ([1.0, 1.0], [5.0, 8.0])]
            {
                let input = TensorRef::from_array_view(([1_usize, 2], input.as_slice())).unwrap();
                let outputs = session.run(ort::inputs!["X" => input]).unwrap();
                let (_, actual) = outputs["Y"].try_extract_tensor::<f32>().unwrap();
                assert_eq!(actual, expected);
            }
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn only_shape_folded_macos_models_use_the_weight_workaround() {
        let owl = CATALOG.iter().find(|spec| spec.id == "owlv2-base").unwrap();
        let yolox = crate::vision::models::find("yolox-nano").unwrap();
        assert_eq!(production_strategy(yolox), CoremlStrategy::Default);
        assert_eq!(
            production_strategy(owl),
            if cfg!(target_os = "macos") {
                CoremlStrategy::BinaryWeights
            } else {
                CoremlStrategy::Default
            }
        );
    }

    #[test]
    fn yolox_grid_offsets_and_strides_are_applied() {
        let size = 64; // 8x8 + 4x4 + 2x2 anchors
        let anchors = 64 + 16 + 4;
        let row = 85;
        let mut values = vec![0.0_f32; anchors * row];
        // Stride 16 grid, cell (x=1, y=2): anchor index 64 + 2 * 4 + 1.
        let anchor = 64 + 2 * 4 + 1;
        let candidate = &mut values[anchor * row..(anchor + 1) * row];
        candidate[0] = 0.5;
        candidate[1] = 0.25;
        candidate[2] = 0.0; // exp(0) * 16 = 16 px wide
        candidate[3] = 1.0_f32.ln(); // 16 px tall
        candidate[4] = 0.9;
        candidate[5 + 19] = 0.8; // cow
        candidate[5] = 0.95; // person (class 0), not wanted
        let detections = decode_yolox(
            &values,
            &[1, anchors as i64, row as i64],
            size,
            80,
            &DetectOptions {
                min_confidence: 0.3,
                wanted: &wanted(&[19]),
            },
        )
        .unwrap();
        assert_eq!(detections.len(), 1);
        let detection = &detections[0];
        assert_eq!(COCO_LABELS[detection.class_id], "cow");
        assert!((detection.confidence - 0.72).abs() < 1e-5);
        // centre (1.5 * 16, 2.25 * 16) = (24, 36), 16x16.
        assert_eq!(detection.bbox.x, 16.0);
        assert_eq!(detection.bbox.y, 28.0);
        assert_eq!(detection.bbox.width, 16.0);
    }

    #[test]
    fn yolox_rejects_an_unexpected_shape() {
        let options = DetectOptions {
            min_confidence: 0.3,
            wanted: &wanted(&[19]),
        };
        assert!(decode_yolox(&[0.0; 85], &[1, 1, 85], 416, 80, &options).is_err());
    }

    #[test]
    fn detr_queries_are_scored_with_sigmoid_and_scaled_to_the_input() {
        let queries = 2;
        let mut logits = vec![-10.0_f32; queries * 80];
        logits[80 + 19] = 2.0; // query 1: cow, sigmoid(2) = 0.881
        logits[80] = 5.0; // query 1: person, not wanted
        logits[18] = -3.0; // query 0: sheep, sigmoid(-3) = 0.047, too low
        let boxes = vec![0.5, 0.5, 0.1, 0.1, 0.25, 0.5, 0.1, 0.2];
        let detections = decode_detr(
            &logits,
            &[1, queries as i64, 80],
            &boxes,
            &[1, queries as i64, 4],
            640,
            80,
            &DetectOptions {
                min_confidence: 0.3,
                wanted: &wanted(&[18, 19]),
            },
        )
        .unwrap();
        assert_eq!(detections.len(), 1);
        assert_eq!(detections[0].class_id, 19);
        assert!((detections[0].confidence - 0.8808).abs() < 1e-3);
        // centre (160, 320), 64 x 128 px.
        assert_eq!(detections[0].bbox.x, 128.0);
        assert_eq!(detections[0].bbox.y, 256.0);
        assert_eq!(detections[0].bbox.height, 128.0);
    }

    #[test]
    fn detr_rejects_logits_that_do_not_match_the_labels() {
        let options = DetectOptions {
            min_confidence: 0.3,
            wanted: &wanted(&[19]),
        };
        let result = decode_detr(
            &[0.0; 91],
            &[1, 1, 91],
            &[0.0; 4],
            &[1, 1, 4],
            560,
            80,
            &options,
        );
        assert!(result.is_err());
    }

    #[test]
    fn ultralytics_layout_is_channel_major() {
        let anchors = 3;
        let mut values = vec![0.0_f32; 84 * anchors];
        let set = |values: &mut Vec<f32>, channel: usize, anchor: usize, value: f32| {
            values[channel * anchors + anchor] = value;
        };
        set(&mut values, 0, 2, 100.0);
        set(&mut values, 1, 2, 50.0);
        set(&mut values, 2, 2, 20.0);
        set(&mut values, 3, 2, 10.0);
        set(&mut values, 4 + 18, 2, 0.7); // sheep
        let detections = decode_ultralytics(
            &values,
            &[1, 84, anchors as i64],
            80,
            &DetectOptions {
                min_confidence: 0.5,
                wanted: &wanted(&[18]),
            },
        )
        .unwrap();
        assert_eq!(detections.len(), 1);
        assert_eq!(detections[0].class_id, 18);
        assert_eq!(detections[0].bbox.x, 90.0);
        assert_eq!(detections[0].bbox.y, 45.0);
    }

    /// Runs a real model on a picture and prints its animals and phase timings:
    /// `DLB_VISION_MODEL=/path/model.onnx DLB_VISION_MODEL_ID=yolox-nano
    /// DLB_VISION_PPM=/path/picture.ppm cargo test --release -- --ignored real_model`.
    /// The picture must already fit the model input (stretched for DETR).
    /// `DLB_VISION_COREML_STRATEGY=compare` benchmarks Default/FastPrediction
    /// in separate caches and verifies equivalent boxes and class scores.
    /// `default` uses the production strategy; `legacy` uses the previous
    /// fused graph. `fast-prediction` runs only that experimental hint.
    /// `binary-weights` keeps MatMul weights external; `compare-binary` verifies
    /// it against Default using distinct folded-model cache keys.
    /// `DLB_VISION_BENCH_ITERATIONS`/`DLB_VISION_BENCH_WARMUP` default to 20/3.
    #[test]
    #[ignore]
    fn real_model_finds_animals() {
        let model = std::env::var("DLB_VISION_MODEL").expect("DLB_VISION_MODEL");
        let picture = std::env::var("DLB_VISION_PPM").expect("DLB_VISION_PPM");
        let id = std::env::var("DLB_VISION_MODEL_ID").unwrap_or_else(|_| "yolox-nano".into());
        let spec = crate::vision::models::find(&id).expect("unknown DLB_VISION_MODEL_ID");
        let mut reader = std::io::BufReader::new(std::fs::File::open(picture).unwrap());
        let frame = super::super::tap::read_ppm(&mut reader, spec.input_size)
            .unwrap()
            .unwrap();
        let cache = std::env::temp_dir().join("dlb-coreml-cache").join(spec.id);
        let animals = crate::vision::settings::DetectionProfile::Animals.classes();
        let wanted: Vec<bool> = spec.labels.iter().map(|l| animals.contains(l)).collect();
        let options = DetectOptions {
            min_confidence: 0.3,
            wanted: &wanted,
        };
        let iterations = benchmark_count("DLB_VISION_BENCH_ITERATIONS", 20);
        let warmup = benchmark_count("DLB_VISION_BENCH_WARMUP", 3);
        let strategies = match std::env::var("DLB_VISION_COREML_STRATEGY")
            .unwrap_or_else(|_| "default".into())
            .as_str()
        {
            "default" => vec![production_strategy(spec)],
            "legacy" => vec![CoremlStrategy::Default],
            "fast-prediction" => vec![CoremlStrategy::FastPrediction],
            "binary-weights" => vec![CoremlStrategy::BinaryWeights],
            "compare-binary" => vec![
                CoremlStrategy::BinaryWeights,
                CoremlStrategy::Default,
                CoremlStrategy::BinaryWeights,
            ],
            "compare" => vec![CoremlStrategy::Default, CoremlStrategy::FastPrediction],
            value => panic!("unknown DLB_VISION_COREML_STRATEGY: {value}"),
        };
        let compare = strategies.len() > 1;
        let mut reference: Option<Vec<Detection>> = None;
        for specialization in strategies {
            let started = std::time::Instant::now();
            let mut detector =
                OnnxDetector::load_with_strategy(spec, Path::new(&model), &cache, specialization)
                    .unwrap();
            println!(
                "{:?} loaded in {:.3} s (backend {})",
                specialization,
                started.elapsed().as_secs_f64(),
                detector.backend()
            );
            if compare || specialization != CoremlStrategy::Default {
                assert_eq!(
                    detector.backend(),
                    "CoreML",
                    "CoreML specialization benchmark fell back to CPU"
                );
            }
            for _ in 0..warmup {
                detector.detect(&frame, &options).unwrap();
            }
            let mut samples = Vec::with_capacity(iterations);
            let mut detections = Vec::new();
            for _ in 0..iterations {
                detections = detector.detect(&frame, &options).unwrap();
                samples.push(detector.last_timings);
            }
            let mut counts = std::collections::BTreeMap::new();
            for detection in &detections {
                *counts.entry(spec.labels[detection.class_id]).or_insert(0) += 1;
            }
            println!(
                "{} on {} ({specialization:?}): {:.1} ms per picture, {} animals >= 0.3: {counts:?}",
                spec.name,
                detector.backend(),
                samples.iter().map(|sample| sample.total_ms).sum::<f64>() / iterations as f64,
                detections.len()
            );
            print_phase("preprocess", &samples, |sample| sample.preprocess_ms);
            print_phase("model", &samples, |sample| sample.model_ms);
            print_phase("postprocess", &samples, |sample| sample.postprocess_ms);
            print_phase("total", &samples, |sample| sample.total_ms);
            assert!(!detections.is_empty());
            if compare {
                if let Some(reference) = &reference {
                    assert_equivalent_detections(reference, &detections);
                } else {
                    reference = Some(detections);
                }
            }
        }
    }

    fn benchmark_count(name: &str, default: usize) -> usize {
        std::env::var(name).map_or(default, |value| {
            value
                .parse::<usize>()
                .ok()
                .filter(|count| *count > 0)
                .unwrap_or_else(|| panic!("{name} must be a positive integer"))
        })
    }

    fn print_phase(name: &str, samples: &[DetectionTimings], phase: fn(&DetectionTimings) -> f64) {
        let mut values: Vec<f64> = samples.iter().map(phase).collect();
        values.sort_by(f64::total_cmp);
        let percentile = |percent: f64| values[(values.len() as f64 * percent).ceil() as usize - 1];
        println!(
            "  {name}: p50 {:.3} ms, p95 {:.3} ms ({} samples)",
            percentile(0.5),
            percentile(0.95),
            values.len()
        );
    }

    fn assert_equivalent_detections(reference: &[Detection], actual: &[Detection]) {
        let ordered = |detections: &[Detection]| {
            let mut detections = detections.to_vec();
            detections.sort_by(|a, b| {
                a.class_id
                    .cmp(&b.class_id)
                    .then(a.bbox.x.total_cmp(&b.bbox.x))
                    .then(a.bbox.y.total_cmp(&b.bbox.y))
            });
            detections
        };
        assert_eq!(
            reference.len(),
            actual.len(),
            "specialization changed animal count"
        );
        for (reference, actual) in ordered(reference).iter().zip(ordered(actual)) {
            assert_eq!(
                reference.class_id, actual.class_id,
                "specialization changed species"
            );
            for (expected, actual) in [
                (reference.confidence, actual.confidence),
                (reference.bbox.x, actual.bbox.x),
                (reference.bbox.y, actual.bbox.y),
                (reference.bbox.width, actual.bbox.width),
                (reference.bbox.height, actual.bbox.height),
            ] {
                assert!(
                    (expected - actual).abs() <= 1e-4,
                    "specialization changed a score or box"
                );
            }
        }
    }
}
