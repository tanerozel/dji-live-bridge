//! The detection models the app can run, and the local cache they live in.
//!
//! Models are not bundled: the app already ships ~100 MB of FFmpeg and
//! MediaMTX, and most people never turn AI Vision on. The first time it is
//! enabled the model is downloaded once (resuming a broken download), checked
//! against its SHA-256 and kept per version; from then on inference never
//! touches the network. The download is a plain GET that carries no frames,
//! detections or anything else about the user.
//!
//! Only permissively licensed weights are listed. Ultralytics YOLOv5/v8/11
//! are AGPL-3.0, which this MIT app cannot ship; YOLOX is Apache-2.0.

use std::path::{Path, PathBuf};

use serde::Serialize;
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

use super::settings::DetectionProfile;
use crate::error::{BridgeError, BridgeResult};

/// How a model lays out its output tensor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(
    dead_code,
    reason = "Ultralytics is for custom-trained models; no built-in model uses it"
)]
pub enum OutputFormat {
    /// `[1, anchors, 5 + classes]`: grid-relative box, objectness, class
    /// scores (already sigmoid). Decoded with strides 8/16/32.
    Yolox,
    /// `[1, 4 + classes, anchors]`: box centre/size in input pixels and class
    /// scores, no objectness. What `yolo export format=onnx` produces, for a
    /// custom-trained model whose licence allows shipping it.
    Ultralytics,
    /// DETR family (RT-DETR, D-FINE, RF-DETR): `logits [1, queries, labels]`
    /// and `pred_boxes [1, queries, 4]` with normalised centre/size boxes.
    /// Scores are the sigmoid of the logits; there is no grid and no NMS.
    Detr,
}

/// How FFmpeg fits the picture into the square input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputResize {
    /// Aspect ratio kept, top-left aligned, the rest padded (YOLOX training).
    Letterbox,
    /// Squashed to the square, as the DETR image processors do.
    Stretch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelOrder {
    Rgb,
    Bgr,
}

#[derive(Debug)]
pub struct ModelSpec {
    pub id: &'static str,
    pub name: &'static str,
    pub version: &'static str,
    pub url: &'static str,
    pub sha256: &'static str,
    pub size_bytes: u64,
    /// Square input edge in pixels.
    pub input_size: u32,
    pub resize: InputResize,
    pub format: OutputFormat,
    pub channel_order: ChannelOrder,
    /// Multiplier for 0–255 pixel values (YOLOX takes them unscaled).
    pub pixel_scale: f32,
    /// The value letterbox padding is filled with (0–255, before scaling).
    pub pad_value: f32,
    /// Per-channel mean/std applied after `pixel_scale`, for models trained
    /// on normalised pictures.
    pub normalize: Option<Normalize>,
    pub labels: &'static [&'static str],
    pub license: &'static str,
    /// Too slow for live video on the CPU alone (0.5–3.5 s per picture on
    /// two cores), so only offered where CoreML is available.
    pub needs_accelerator: bool,
    /// Open-vocabulary models are told what to find: one tokenised text
    /// prompt per label, fed as `input_ids`/`attention_mask`.
    pub text_prompts: Option<&'static [&'static [i64]]>,
    /// Multiplies the model's scores (capped at 1) so one threshold slider
    /// means about the same for every model.
    pub score_scale: f32,
    /// Fold the graph to the pinned input sizes once and load that copy.
    /// Some exports keep `Shape` nodes that CoreML cannot be fed otherwise.
    pub fold_shapes: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Normalize {
    pub mean: [f32; 3],
    pub std: [f32; 3],
}

/// Tokens per text prompt (CLIP's context for OWLv2); shorter prompts are
/// padded with token 0 and masked out.
pub const PROMPT_TOKENS: usize = 16;

pub const COCO_LABELS: [&str; 80] = [
    "person",
    "bicycle",
    "car",
    "motorcycle",
    "airplane",
    "bus",
    "train",
    "truck",
    "boat",
    "traffic light",
    "fire hydrant",
    "stop sign",
    "parking meter",
    "bench",
    "bird",
    "cat",
    "dog",
    "horse",
    "sheep",
    "cow",
    "elephant",
    "bear",
    "zebra",
    "giraffe",
    "backpack",
    "umbrella",
    "handbag",
    "tie",
    "suitcase",
    "frisbee",
    "skis",
    "snowboard",
    "sports ball",
    "kite",
    "baseball bat",
    "baseball glove",
    "skateboard",
    "surfboard",
    "tennis racket",
    "bottle",
    "wine glass",
    "cup",
    "fork",
    "knife",
    "spoon",
    "bowl",
    "banana",
    "apple",
    "sandwich",
    "orange",
    "broccoli",
    "carrot",
    "hot dog",
    "pizza",
    "donut",
    "cake",
    "chair",
    "couch",
    "potted plant",
    "bed",
    "dining table",
    "toilet",
    "tv",
    "laptop",
    "mouse",
    "remote",
    "keyboard",
    "cell phone",
    "microwave",
    "oven",
    "toaster",
    "sink",
    "refrigerator",
    "book",
    "clock",
    "vase",
    "scissors",
    "teddy bear",
    "hair drier",
    "toothbrush",
];

/// COCO category ids (1–90, with gaps), the label layout RF-DETR keeps.
pub const COCO91_LABELS: [&str; 91] = [
    "",
    "person",
    "bicycle",
    "car",
    "motorcycle",
    "airplane",
    "bus",
    "train",
    "truck",
    "boat",
    "traffic light",
    "fire hydrant",
    "",
    "stop sign",
    "parking meter",
    "bench",
    "bird",
    "cat",
    "dog",
    "horse",
    "sheep",
    "cow",
    "elephant",
    "bear",
    "zebra",
    "giraffe",
    "",
    "backpack",
    "umbrella",
    "",
    "",
    "handbag",
    "tie",
    "suitcase",
    "frisbee",
    "skis",
    "snowboard",
    "sports ball",
    "kite",
    "baseball bat",
    "baseball glove",
    "skateboard",
    "surfboard",
    "tennis racket",
    "bottle",
    "",
    "wine glass",
    "cup",
    "fork",
    "knife",
    "spoon",
    "bowl",
    "banana",
    "apple",
    "sandwich",
    "orange",
    "broccoli",
    "carrot",
    "hot dog",
    "pizza",
    "donut",
    "cake",
    "chair",
    "couch",
    "potted plant",
    "bed",
    "",
    "dining table",
    "",
    "",
    "toilet",
    "",
    "tv",
    "laptop",
    "mouse",
    "remote",
    "keyboard",
    "cell phone",
    "microwave",
    "oven",
    "toaster",
    "sink",
    "refrigerator",
    "",
    "book",
    "clock",
    "vase",
    "scissors",
    "teddy bear",
    "hair drier",
    "toothbrush",
];

/// YOLOX-Tiny: 5 M parameters, 32.8 COCO mAP. ~5 ms per picture on Apple
/// Silicon through CoreML.
const YOLOX_TINY: ModelSpec = ModelSpec {
    id: "yolox-tiny",
    name: "YOLOX-Tiny",
    version: "0.1.1rc0",
    url: "https://github.com/Megvii-BaseDetection/YOLOX/releases/download/0.1.1rc0/yolox_tiny.onnx",
    sha256: "427cc366d34e27ff7a03e2899b5e3671425c262ea2291f88bb942bc1cc70b0f7",
    size_bytes: 20_219_662,
    input_size: 416,
    resize: InputResize::Letterbox,
    format: OutputFormat::Yolox,
    channel_order: ChannelOrder::Bgr,
    pixel_scale: 1.0,
    pad_value: 114.0,
    normalize: None,
    labels: &COCO_LABELS,
    license: "Apache-2.0",
    needs_accelerator: false,
    text_prompts: None,
    score_scale: 1.0,
    fold_shapes: false,
};

/// YOLOX-Nano: 0.9 M parameters, 25.8 COCO mAP. ~15 ms on two CPU cores, so
/// it is the default where only the CPU is used.
const YOLOX_NANO: ModelSpec = ModelSpec {
    id: "yolox-nano",
    name: "YOLOX-Nano",
    version: "0.1.1rc0",
    url: "https://github.com/Megvii-BaseDetection/YOLOX/releases/download/0.1.1rc0/yolox_nano.onnx",
    sha256: "c789161ed43c8269fcd4e67c67eeeb4e80c622da2eb296a20bc6007bd18a0b7d",
    size_bytes: 3_659_407,
    input_size: 416,
    resize: InputResize::Letterbox,
    format: OutputFormat::Yolox,
    channel_order: ChannelOrder::Bgr,
    pixel_scale: 1.0,
    pad_value: 114.0,
    normalize: None,
    labels: &COCO_LABELS,
    license: "Apache-2.0",
    needs_accelerator: false,
    text_prompts: None,
    score_scale: 1.0,
    fold_shapes: false,
};

/// YOLOX-S at 640 px: 9 M parameters, 40.5 COCO mAP. Opt-in for high drone
/// shots, where animals are only a few pixels tall at 416 px: on real aerial
/// footage it found 14 cattle in a frame where YOLOX-Tiny found 8. ~11 ms on
/// CoreML, ~190 ms on two CPU cores.
const YOLOX_S: ModelSpec = ModelSpec {
    id: "yolox-s",
    name: "YOLOX-S",
    version: "0.1.1rc0",
    url: "https://github.com/Megvii-BaseDetection/YOLOX/releases/download/0.1.1rc0/yolox_s.onnx",
    sha256: "c5c2d13e59ae883e6af3b45daea64af4833a4951c92d116ec270d9ddbe998063",
    size_bytes: 35_858_002,
    input_size: 640,
    resize: InputResize::Letterbox,
    format: OutputFormat::Yolox,
    channel_order: ChannelOrder::Bgr,
    pixel_scale: 1.0,
    pad_value: 114.0,
    normalize: None,
    labels: &COCO_LABELS,
    license: "Apache-2.0",
    needs_accelerator: false,
    text_prompts: None,
    score_scale: 1.0,
    fold_shapes: false,
};

/// YOLOX-L at 640 px: 54 M parameters, 50.0 COCO mAP.
const YOLOX_L: ModelSpec = ModelSpec {
    id: "yolox-l",
    name: "YOLOX-L",
    version: "0.1.1rc0",
    url: "https://github.com/Megvii-BaseDetection/YOLOX/releases/download/0.1.1rc0/yolox_l.onnx",
    sha256: "7860ae79de6c89a3c1eb72ae9a2756c0ccfbe04b7791bb5880afabd97855a411",
    size_bytes: 216_746_733,
    input_size: 640,
    resize: InputResize::Letterbox,
    format: OutputFormat::Yolox,
    channel_order: ChannelOrder::Bgr,
    pixel_scale: 1.0,
    pad_value: 114.0,
    normalize: None,
    labels: &COCO_LABELS,
    license: "Apache-2.0",
    needs_accelerator: true,
    text_prompts: None,
    score_scale: 1.0,
    fold_shapes: false,
};

/// YOLOX-X at 640 px: 99 M parameters, 51.5 COCO mAP.
const YOLOX_X: ModelSpec = ModelSpec {
    id: "yolox-x",
    name: "YOLOX-X",
    version: "0.1.1rc0",
    url: "https://github.com/Megvii-BaseDetection/YOLOX/releases/download/0.1.1rc0/yolox_x.onnx",
    sha256: "c892d7aaf1c4746d8a4d675bec669a4db4f434b4ee1efb654bc9b353379c7c55",
    size_bytes: 396_142_663,
    input_size: 640,
    resize: InputResize::Letterbox,
    format: OutputFormat::Yolox,
    channel_order: ChannelOrder::Bgr,
    pixel_scale: 1.0,
    pad_value: 114.0,
    normalize: None,
    labels: &COCO_LABELS,
    license: "Apache-2.0",
    needs_accelerator: true,
    text_prompts: None,
    score_scale: 1.0,
    fold_shapes: false,
};

/// D-FINE-X pre-trained on Objects365 and fine-tuned on COCO: 59.3 COCO mAP,
/// a transformer detector that keeps small objects better than YOLO. The
/// ONNX export is the transformers.js one from the onnx-community; the
/// weights are the authors' (ustc-community), Apache-2.0.
const DFINE_X: ModelSpec = ModelSpec {
    id: "dfine-x-obj2coco",
    name: "D-FINE X",
    version: "hf-4d3a85a",
    url: "https://huggingface.co/onnx-community/dfine_x_obj2coco-ONNX/resolve/4d3a85a6c29f55a5346fabaf9728f5f3751f947a/onnx/model.onnx",
    sha256: "486ee1cba40b25f3ea8d6783f664eca88b7c12d01078984834d36834d9b2c802",
    size_bytes: 251_138_448,
    input_size: 640,
    resize: InputResize::Stretch,
    format: OutputFormat::Detr,
    channel_order: ChannelOrder::Rgb,
    pixel_scale: 1.0 / 255.0,
    pad_value: 0.0,
    normalize: None,
    labels: &COCO_LABELS,
    license: "Apache-2.0",
    needs_accelerator: true,
    text_prompts: None,
    score_scale: 1.0,
    fold_shapes: false,
};

/// RF-DETR Large (Roboflow, Apache-2.0; only Nano–Large are, XL and 2XL are
/// not). DINOv2 backbone at 560 px. Its logits are indexed by COCO category id.
const RFDETR_L: ModelSpec = ModelSpec {
    id: "rf-detr-large",
    name: "RF-DETR Large",
    version: "hf-4988fba",
    url: "https://huggingface.co/onnx-community/rfdetr_large-ONNX/resolve/4988fbacee4aa815e43c2e0666377ab26bddf1e8/onnx/model.onnx",
    sha256: "3dded29a94ddaf3835a5c982a243f8f4aec7da22d47a0c8c690ba7d7fb658d6a",
    size_bytes: 486_217_143,
    input_size: 560,
    resize: InputResize::Stretch,
    format: OutputFormat::Detr,
    channel_order: ChannelOrder::Rgb,
    pixel_scale: 1.0 / 255.0,
    pad_value: 0.0,
    normalize: None,
    labels: &COCO91_LABELS,
    license: "Apache-2.0",
    needs_accelerator: true,
    text_prompts: None,
    score_scale: 1.0,
    fold_shapes: false,
};

/// What OWLv2 is asked to find. COCO-trained models have no goat, donkey,
/// chicken, pig or deer at all (a goat comes out as "sheep" or nothing), and
/// Objects365 has no goat either; an open-vocabulary model reads the names.
pub const OWL_LABELS: [&str; 14] = [
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
];

/// `OWL_LABELS` through OWLv2's CLIP tokenizer: start, word pieces, end. The
/// bare name scored best: on a goat pen "goat" found 96 goats and no other
/// species, where "a photo of a goat" also called 4 of them cows.
const OWL_PROMPTS: [&[i64]; 14] = [
    &[49406, 9530, 49407],
    &[49406, 9629, 49407],
    &[49406, 9706, 49407],
    &[49406, 4558, 49407],
    &[49406, 21415, 49407],
    &[49406, 1929, 49407],
    &[49406, 2368, 49407],
    &[49406, 3717, 49407],
    &[49406, 6910, 49407],
    &[49406, 13822, 49407],
    &[49406, 9619, 49407],
    &[49406, 8700, 49407],
    &[49406, 3220, 35473, 49407],
    &[49406, 3329, 49407],
];

/// OWLv2 base (Google, Apache-2.0), an open-vocabulary detector: a ViT-B/16
/// at 960 px scoring 3600 patches against text prompts. It tells goats from
/// sheep, which no COCO model can. ~440 ms per picture on Apple Silicon
/// through CoreML (3.5 s on the CPU), so about one picture per second; that
/// is enough for grazing animals, and the tracker bridges the gaps. Very
/// distant animals are found but their species is a guess. Clear animals
/// score ~0.6 and empty fields stay below 0.13, hence `score_scale`.
const OWLV2_BASE: ModelSpec = ModelSpec {
    id: "owlv2-base",
    name: "OWLv2",
    version: "hf-180d6ef",
    url: "https://huggingface.co/onnx-community/owlv2-base-patch16-ensemble-ONNX/resolve/180d6ef7599bd69bb48db5c72bd49ad03ddfab80/onnx/model.onnx",
    sha256: "ca3ea10811d55cd8763058c6eaddec514bbb91cc07ab774a0db0775346aef732",
    size_bytes: 614_219_112,
    input_size: 960,
    resize: InputResize::Letterbox,
    format: OutputFormat::Detr,
    channel_order: ChannelOrder::Rgb,
    pixel_scale: 1.0 / 255.0,
    pad_value: 127.5,
    normalize: Some(Normalize {
        mean: [0.481_454_7, 0.457_827_5, 0.408_210_7],
        std: [0.268_629_5, 0.261_302_6, 0.275_777_1],
    }),
    labels: &OWL_LABELS,
    license: "Apache-2.0",
    needs_accelerator: true,
    text_prompts: Some(&OWL_PROMPTS),
    score_scale: 1.5,
    fold_shapes: true,
};

pub const CATALOG: &[ModelSpec] = &[
    YOLOX_TINY, YOLOX_NANO, YOLOX_S, YOLOX_L, YOLOX_X, DFINE_X, RFDETR_L, OWLV2_BASE,
];

/// The models this machine can run in real time.
pub fn available() -> impl Iterator<Item = &'static ModelSpec> {
    CATALOG
        .iter()
        .filter(|spec| !spec.needs_accelerator || cfg!(target_os = "macos"))
}

pub fn find(id: &str) -> Option<&'static ModelSpec> {
    available().find(|spec| spec.id == id)
}

/// macOS: OWLv2, because farm herds are goats, sheep, donkeys and poultry as
/// much as cattle, and only it knows them (a one-time 614 MB download). D-FINE
/// X stays the faster choice for cattle and horses: on real drone footage of a
/// herd it found 39 cows where YOLOX-X found 13, at ~90 ms per picture.
/// Windows runs on the CPU, where only the nano model is real time.
pub fn default_model_id() -> &'static str {
    if cfg!(target_os = "macos") {
        OWLV2_BASE.id
    } else {
        YOLOX_NANO.id
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub size_bytes: u64,
    pub license: String,
    pub downloaded: bool,
    /// Animal species it can tell apart, so the picker shows which model
    /// knows goats.
    pub species: u32,
}

#[derive(Debug, Clone)]
pub struct ModelStore {
    root: PathBuf,
}

impl ModelStore {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn path(&self, spec: &ModelSpec) -> PathBuf {
        self.root
            .join(spec.id)
            .join(spec.version)
            .join("model.onnx")
    }

    /// CoreML compiles the model on first load; caching that cuts later loads
    /// from ~0.5 s to a few milliseconds.
    pub fn compiled_cache_dir(&self, spec: &ModelSpec) -> PathBuf {
        self.root.join(spec.id).join(spec.version).join("compiled")
    }

    /// A model counts as present only once its checksum has been verified.
    pub fn is_ready(&self, spec: &ModelSpec) -> bool {
        let path = self.path(spec);
        let verified = std::fs::read_to_string(marker_path(&path))
            .is_ok_and(|hash| hash.trim() == spec.sha256);
        verified && std::fs::metadata(&path).is_ok_and(|meta| meta.len() == spec.size_bytes)
    }

    pub fn info(&self) -> Vec<ModelInfo> {
        available()
            .map(|spec| ModelInfo {
                id: spec.id.into(),
                name: spec.name.into(),
                size_bytes: spec.size_bytes,
                license: spec.license.into(),
                downloaded: self.is_ready(spec),
                species: DetectionProfile::Animals.classes_of(spec.labels).len() as u32,
            })
            .collect()
    }

    /// Downloads the model into the cache, continuing a partial file left by
    /// an earlier attempt, and verifies it before it can be used.
    pub async fn download(
        &self,
        spec: &ModelSpec,
        progress: impl Fn(u64, u64),
    ) -> BridgeResult<PathBuf> {
        let final_path = self.path(spec);
        let directory = final_path
            .parent()
            .ok_or_else(|| BridgeError::Vision("model path has no directory".into()))?;
        tokio::fs::create_dir_all(directory).await?;
        let partial = final_path.with_extension("onnx.part");

        let mut start = tokio::fs::metadata(&partial)
            .await
            .map(|meta| meta.len())
            .unwrap_or(0);
        if start > spec.size_bytes {
            tokio::fs::remove_file(&partial).await.ok();
            start = 0;
        }

        // A complete partial file (the checksum step failed to run last time)
        // goes straight to verification.
        if start < spec.size_bytes {
            let client = reqwest::Client::builder()
                .user_agent(concat!("DJI-Live-Bridge/", env!("CARGO_PKG_VERSION")))
                .connect_timeout(std::time::Duration::from_secs(15))
                .build()?;
            let mut request = client.get(spec.url);
            if start > 0 {
                request = request.header(reqwest::header::RANGE, format!("bytes={start}-"));
            }
            let mut response = request.send().await?.error_for_status()?;
            let resumed = start > 0 && response.status() == reqwest::StatusCode::PARTIAL_CONTENT;
            if !resumed {
                start = 0;
            }
            let mut file = tokio::fs::OpenOptions::new()
                .create(true)
                .write(true)
                .append(resumed)
                .truncate(!resumed)
                .open(&partial)
                .await?;
            let mut received = start;
            progress(received, spec.size_bytes);
            loop {
                // A stalled connection must fail (and be resumable), not hang.
                let chunk =
                    tokio::time::timeout(std::time::Duration::from_secs(30), response.chunk())
                        .await
                        .map_err(|_| BridgeError::Vision("model download stalled".into()))??;
                let Some(chunk) = chunk else { break };
                received += chunk.len() as u64;
                if received > spec.size_bytes {
                    drop(file);
                    tokio::fs::remove_file(&partial).await.ok();
                    return Err(BridgeError::Vision(
                        "model download is larger than expected".into(),
                    ));
                }
                file.write_all(&chunk).await?;
                progress(received, spec.size_bytes);
            }
            file.flush().await?;
            if received != spec.size_bytes {
                return Err(BridgeError::Vision(format!(
                    "model download ended early ({received} of {} bytes); it will resume",
                    spec.size_bytes
                )));
            }
        }

        let hashed = partial.clone();
        let digest = tokio::task::spawn_blocking(move || sha256_file(&hashed))
            .await
            .map_err(|error| BridgeError::Vision(format!("checksum task failed: {error}")))??;
        if digest != spec.sha256 {
            tokio::fs::remove_file(&partial).await.ok();
            return Err(BridgeError::Vision(format!(
                "{} failed its checksum and was deleted; try again",
                spec.name
            )));
        }
        tokio::fs::rename(&partial, &final_path).await?;
        tokio::fs::write(marker_path(&final_path), spec.sha256).await?;
        Ok(final_path)
    }
}

fn marker_path(model: &Path) -> PathBuf {
    model.with_extension("onnx.sha256")
}

fn sha256_file(path: &Path) -> BridgeResult<String> {
    use std::io::Read;
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 1 << 16];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_entries_are_complete() {
        for spec in CATALOG {
            assert_eq!(spec.sha256.len(), 64, "{}", spec.id);
            assert!(spec.url.starts_with("https://"), "{}", spec.id);
            assert_eq!(spec.input_size % 16, 0, "{}", spec.id);
            if let Some(prompts) = spec.text_prompts {
                assert_eq!(prompts.len(), spec.labels.len(), "{}", spec.id);
                assert!(prompts.iter().all(|p| p.len() <= PROMPT_TOKENS));
            }
        }
        assert!(find(default_model_id()).is_some());
    }

    #[test]
    fn every_model_knows_the_common_farm_animals() {
        let animals = super::super::settings::DetectionProfile::Animals.classes();
        for spec in CATALOG {
            for class in ["sheep", "cow", "horse", "dog", "bird"] {
                assert!(spec.labels.contains(&class), "{class} in {}", spec.id);
            }
            // Labels a profile does not list would never be reported.
            assert!(
                spec.labels
                    .iter()
                    .filter(|label| animals.contains(label))
                    .count()
                    >= 6,
                "{}",
                spec.id
            );
        }
        assert!(OWL_LABELS.iter().all(|label| animals.contains(label)));
        assert_eq!(COCO91_LABELS[21], "cow");
        assert_eq!(COCO91_LABELS.iter().filter(|l| !l.is_empty()).count(), 80);
    }

    #[test]
    fn a_model_without_a_verified_marker_is_not_ready() {
        let root = std::env::temp_dir().join(format!("dlb-models-{}", uuid::Uuid::new_v4()));
        let store = ModelStore::new(root.clone());
        let spec = &CATALOG[1];
        let path = store.path(spec);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, vec![0_u8; spec.size_bytes as usize]).unwrap();
        assert!(!store.is_ready(spec));
        std::fs::write(marker_path(&path), "not the hash").unwrap();
        assert!(!store.is_ready(spec));
        std::fs::write(marker_path(&path), spec.sha256).unwrap();
        assert!(store.is_ready(spec));
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn hashes_files() {
        let path = std::env::temp_dir().join(format!("dlb-hash-{}", uuid::Uuid::new_v4()));
        std::fs::write(&path, b"abc").unwrap();
        assert_eq!(
            sha256_file(&path).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        std::fs::remove_file(path).ok();
    }
}
