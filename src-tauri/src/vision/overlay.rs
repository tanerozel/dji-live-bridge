//! Draws boxes, labels and the counter into a transparent picture that the
//! FFmpeg processes lay over the drone video, so the same overlay reaches the
//! production encode (every RTMP destination) and the virtual camera.
//!
//! Each FFmpeg reads the overlay as a second input from a local socket:
//! `yuva420p` frames at the source resolution, `OVERLAY_FPS` per second.
//! FFmpeg only waits for overlay frames as the video needs them, so a writer
//! here pushes frames as fast as it is allowed to and must never block on
//! anything but the socket: a stalled overlay would stall the video. It only
//! ever clones the latest `Scene`; inference never holds it up. When the app
//! stops feeding (it quit, or the connection broke), `eof_action=pass` lets
//! the video continue without the overlay.

use std::{
    collections::HashMap,
    io::Write,
    net::{Ipv4Addr, TcpListener, TcpStream},
    sync::{Arc, Mutex},
};

use super::{counter::CountSummary, tracker::TrackView};
use crate::error::{BridgeError, BridgeResult};

/// Detections refresh at most 15 times a second; drawing faster would only
/// move more bytes. Measured end to end through MediaMTX: the overlay trails
/// its source picture by ~0.2–0.3 s at this rate.
pub const OVERLAY_FPS: u32 = 15;
/// The overlay filter both FFmpeg pipelines use.
pub const OVERLAY_FILTER: &str = "overlay=eof_action=pass";

/// Where one FFmpeg process reads its overlay from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OverlayInput {
    pub port: u16,
    pub width: u32,
    pub height: u32,
}

impl OverlayInput {
    /// FFmpeg input options. The one-frame queue and small receive buffer
    /// keep the overlay close to real time instead of queueing seconds.
    pub fn input_args(&self) -> Vec<String> {
        [
            "-thread_queue_size",
            "1",
            "-f",
            "rawvideo",
            "-pix_fmt",
            "yuva420p",
            "-video_size",
            &format!("{}x{}", self.width, self.height),
            "-framerate",
            &OVERLAY_FPS.to_string(),
            "-i",
            &format!("tcp://127.0.0.1:{}?recv_buffer_size=65536", self.port),
        ]
        .into_iter()
        .map(str::to_string)
        .collect()
    }
}

/// Everything the overlay draws, published by the inference worker.
#[derive(Debug, Clone, Default)]
pub struct Scene {
    pub version: u64,
    pub show_boxes: bool,
    pub show_counter: bool,
    pub heading: &'static str,
    pub labels: &'static [&'static str],
    pub tracks: Vec<TrackView>,
    pub counts: CountSummary,
}

/// The latest scene; readers clone the `Arc` and never wait on the writer.
#[derive(Default)]
pub struct SceneCell(Mutex<Arc<Scene>>);

impl SceneCell {
    pub fn load(&self) -> Arc<Scene> {
        self.0.lock().expect("scene lock").clone()
    }

    pub fn store(&self, scene: Scene) {
        *self.0.lock().expect("scene lock") = Arc::new(scene);
    }
}

/// One listener per overlay size; FFmpeg connects to it as an input.
pub struct OverlayServer {
    scene: Arc<SceneCell>,
    ports: Mutex<HashMap<(u32, u32), u16>>,
}

impl OverlayServer {
    pub fn new(scene: Arc<SceneCell>) -> Self {
        Self {
            scene,
            ports: Mutex::new(HashMap::new()),
        }
    }

    /// The input an FFmpeg reading a `width`x`height` source should open.
    /// Listeners live for the rest of the session; a restarted FFmpeg simply
    /// reconnects to the same one.
    pub fn input_for(&self, width: u32, height: u32) -> BridgeResult<OverlayInput> {
        // yuva420p needs even dimensions; one pixel less is invisible.
        let (width, height) = (width & !1, height & !1);
        if !(16..=8192).contains(&width) || !(16..=8192).contains(&height) {
            return Err(BridgeError::Vision(format!(
                "unsupported overlay size {width}x{height}"
            )));
        }
        let mut ports = self.ports.lock().expect("overlay ports lock");
        if let Some(port) = ports.get(&(width, height)) {
            return Ok(OverlayInput {
                port: *port,
                width,
                height,
            });
        }
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
        let port = listener.local_addr()?.port();
        let scene = self.scene.clone();
        std::thread::Builder::new()
            .name("vision-overlay-accept".into())
            .spawn(move || {
                for stream in listener.incoming() {
                    match stream {
                        Ok(stream) => {
                            let scene = scene.clone();
                            let spawned = std::thread::Builder::new()
                                .name("vision-overlay-feed".into())
                                .spawn(move || feed(stream, width, height, &scene));
                            if let Err(error) = spawned {
                                tracing::warn!(%error, "could not start an overlay feed");
                            }
                        }
                        Err(error) => tracing::warn!(%error, "overlay listener error"),
                    }
                }
            })?;
        ports.insert((width, height), port);
        Ok(OverlayInput {
            port,
            width,
            height,
        })
    }
}

/// Writes overlay frames until FFmpeg goes away.
fn feed(stream: TcpStream, width: u32, height: u32, scene: &SceneCell) {
    let _ = stream.set_nodelay(true);
    let _ = socket2::SockRef::from(&stream).set_send_buffer_size(64 * 1024);
    let mut stream = stream;
    let mut renderer = OverlayRenderer::new(width, height);
    loop {
        renderer.render(&scene.load(), super::clock());
        if stream.write_all(renderer.frame()).is_err() {
            break;
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Rect {
    x0: u32,
    y0: u32,
    x1: u32,
    y1: u32,
}

#[derive(Debug, Clone, Copy)]
struct Yuv(u8, u8, u8);

impl Yuv {
    /// BT.709 limited range, the colour space of HD drone video.
    fn from_rgb(r: u8, g: u8, b: u8) -> Self {
        let (r, g, b) = (f32::from(r), f32::from(g), f32::from(b));
        let y = 16.0 + 0.1826 * r + 0.6142 * g + 0.0620 * b;
        let u = 128.0 - 0.1006 * r - 0.3386 * g + 0.4392 * b;
        let v = 128.0 + 0.4392 * r - 0.3989 * g - 0.0403 * b;
        Self(y.round() as u8, u.round() as u8, v.round() as u8)
    }
}

const WHITE: Yuv = Yuv(235, 128, 128);
const PANEL: Yuv = Yuv(16, 128, 128);
const PALETTE: [(u8, u8, u8); 10] = [
    (255, 193, 7),
    (76, 175, 80),
    (33, 150, 243),
    (233, 30, 99),
    (0, 188, 212),
    (255, 87, 34),
    (156, 39, 176),
    (205, 220, 57),
    (255, 99, 71),
    (121, 85, 72),
];

/// A `yuva420p` canvas redrawn in place: only what was drawn last time is
/// cleared, so an idle overlay costs a memcpy per frame and nothing else.
pub struct OverlayRenderer {
    width: u32,
    height: u32,
    frame: Vec<u8>,
    dirty: Vec<Rect>,
    drawn_version: Option<u64>,
    /// Line width and glyph scale, from the picture size.
    unit: u32,
}

impl OverlayRenderer {
    pub fn new(width: u32, height: u32) -> Self {
        let luma = (width * height) as usize;
        let chroma = ((width / 2) * (height / 2)) as usize;
        let mut frame = vec![0_u8; luma * 2 + chroma * 2];
        frame[luma..luma + chroma * 2].fill(128);
        Self {
            width,
            height,
            frame,
            dirty: Vec::new(),
            drawn_version: None,
            unit: (width.min(height) / 360).max(2),
        }
    }

    pub fn frame(&self) -> &[u8] {
        &self.frame
    }

    /// Redraws for tracker time `now`. Moving boxes are extrapolated between
    /// two inferences, so they are redrawn every frame while any are shown.
    pub fn render(&mut self, scene: &Scene, now: f64) {
        let boxes = scene.show_boxes && !scene.tracks.is_empty();
        if self.drawn_version == Some(scene.version) && !boxes {
            return;
        }
        self.clear();
        if scene.show_boxes {
            for track in &scene.tracks {
                self.draw_track(track, scene.labels, now);
            }
        }
        if scene.show_counter {
            self.draw_counter(scene);
        }
        self.drawn_version = Some(scene.version);
    }

    fn draw_track(&mut self, track: &TrackView, labels: &[&str], now: f64) {
        let bbox = track.predicted(now).clamped();
        let (w, h) = (self.width as f32, self.height as f32);
        let x0 = (bbox.x * w) as u32;
        let y0 = (bbox.y * h) as u32;
        let x1 = ((bbox.x + bbox.width) * w) as u32;
        let y1 = ((bbox.y + bbox.height) * h) as u32;
        if x1 <= x0 + 2 || y1 <= y0 + 2 {
            return;
        }
        let (r, g, b) = PALETTE[track.class_id % PALETTE.len()];
        let colour = Yuv::from_rgb(r, g, b);
        let t = self.unit;
        for edge in [
            Rect {
                x0,
                y0,
                x1,
                y1: y0 + t,
            },
            Rect {
                x0,
                y0: y1.saturating_sub(t),
                x1,
                y1,
            },
            Rect {
                x0,
                y0,
                x1: x0 + t,
                y1,
            },
            Rect {
                x0: x1.saturating_sub(t),
                y0,
                x1,
                y1,
            },
        ] {
            self.fill(edge, colour, 255, true);
        }

        let label = format!(
            "{} #{} {}%",
            title_case(labels.get(track.class_id).copied().unwrap_or("object")),
            track.id,
            (track.confidence * 100.0).round() as u32
        );
        let scale = self.unit;
        let pad = scale * 2;
        let label_height = 8 * scale + pad * 2;
        let label_width = text_width(&label, scale) + pad * 2;
        // Above the box when there is room, otherwise just inside it.
        let top = if y0 >= label_height {
            y0 - label_height
        } else {
            y0
        };
        let background = Rect {
            x0,
            y0: top,
            x1: x0 + label_width,
            y1: top + label_height,
        };
        self.fill(background, colour, 220, true);
        self.text(&label, x0 + pad, top + pad, scale);
    }

    fn draw_counter(&mut self, scene: &Scene) {
        let mut lines = vec![format!("{}: {}", scene.heading, scene.counts.current_total)];
        lines.extend(
            scene
                .counts
                .current_by_class
                .iter()
                .map(|entry| format!("{}: {}", title_case(&entry.class), entry.count)),
        );
        lines.push(format!("Unique: {}", scene.counts.unique_total));

        let scale = self.unit;
        let pad = scale * 4;
        let line_height = 8 * scale + scale * 3;
        let width = lines
            .iter()
            .map(|line| text_width(line, scale))
            .max()
            .unwrap_or(0)
            + pad * 2;
        let height = line_height * lines.len() as u32 + pad * 2 - scale * 3;
        let margin = scale * 6;
        self.fill(
            Rect {
                x0: margin,
                y0: margin,
                x1: margin + width,
                y1: margin + height,
            },
            PANEL,
            160,
            true,
        );
        for (index, line) in lines.iter().enumerate() {
            self.text(
                line,
                margin + pad,
                margin + pad + line_height * index as u32,
                scale,
            );
        }
    }

    /// White text; it sits on a background that is already marked dirty.
    fn text(&mut self, text: &str, x: u32, y: u32, scale: u32) {
        for (index, character) in text.chars().enumerate() {
            let glyph = font8x8::legacy::BASIC_LEGACY
                .get(character as usize)
                .copied()
                .unwrap_or(font8x8::legacy::BASIC_LEGACY[b'?' as usize]);
            let left = x + index as u32 * 8 * scale;
            for (row, bits) in glyph.iter().enumerate() {
                for column in 0..8 {
                    if bits & (1 << column) == 0 {
                        continue;
                    }
                    let px = left + column * scale;
                    let py = y + row as u32 * scale;
                    self.fill(
                        Rect {
                            x0: px,
                            y0: py,
                            x1: px + scale,
                            y1: py + scale,
                        },
                        WHITE,
                        255,
                        false,
                    );
                }
            }
        }
    }

    fn fill(&mut self, rect: Rect, colour: Yuv, alpha: u8, track_dirty: bool) {
        let rect = Rect {
            x0: rect.x0.min(self.width),
            y0: rect.y0.min(self.height),
            x1: rect.x1.min(self.width),
            y1: rect.y1.min(self.height),
        };
        if rect.x0 >= rect.x1 || rect.y0 >= rect.y1 {
            return;
        }
        let width = self.width as usize;
        let luma = width * self.height as usize;
        let chroma_width = width / 2;
        let chroma = chroma_width * (self.height as usize / 2);
        let alpha_offset = luma + chroma * 2;
        let (x0, x1) = (rect.x0 as usize, rect.x1 as usize);
        for y in rect.y0 as usize..rect.y1 as usize {
            self.frame[y * width + x0..y * width + x1].fill(colour.0);
            self.frame[alpha_offset + y * width + x0..alpha_offset + y * width + x1].fill(alpha);
        }
        let (cx0, cx1) = (x0 / 2, x1.div_ceil(2).min(chroma_width));
        let chroma_rows =
            rect.y0 as usize / 2..(rect.y1 as usize).div_ceil(2).min(self.height as usize / 2);
        for cy in chroma_rows {
            let row = cy * chroma_width;
            self.frame[luma + row + cx0..luma + row + cx1].fill(colour.1);
            self.frame[luma + chroma + row + cx0..luma + chroma + row + cx1].fill(colour.2);
        }
        if track_dirty {
            self.dirty.push(rect);
        }
    }

    /// Makes everything drawn last time transparent again.
    fn clear(&mut self) {
        let width = self.width as usize;
        let luma = width * self.height as usize;
        let chroma = (width / 2) * (self.height as usize / 2);
        let alpha_offset = luma + chroma * 2;
        for rect in self.dirty.drain(..) {
            for y in rect.y0 as usize..rect.y1 as usize {
                let start = alpha_offset + y * width;
                self.frame[start + rect.x0 as usize..start + rect.x1 as usize].fill(0);
            }
        }
    }
}

fn text_width(text: &str, scale: u32) -> u32 {
    text.chars().count() as u32 * 8 * scale
}

fn title_case(label: &str) -> String {
    let mut characters = label.chars();
    match characters.next() {
        Some(first) => first.to_uppercase().chain(characters).collect(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vision::{counter::ClassCount, detector::BoundingBox};

    fn alpha(renderer: &OverlayRenderer, x: u32, y: u32) -> u8 {
        let (w, h) = (renderer.width as usize, renderer.height as usize);
        let offset = w * h + (w / 2) * (h / 2) * 2;
        renderer.frame[offset + y as usize * w + x as usize]
    }

    fn scene(version: u64, tracks: Vec<TrackView>) -> Scene {
        Scene {
            version,
            show_boxes: true,
            show_counter: false,
            heading: "Animals",
            labels: &["cat", "cow"],
            tracks,
            counts: CountSummary::default(),
        }
    }

    fn cow(x: f32) -> TrackView {
        TrackView {
            id: 12,
            class_id: 1,
            confidence: 0.94,
            bbox: BoundingBox {
                x,
                y: 0.5,
                width: 0.25,
                height: 0.25,
            },
            velocity: (0.0, 0.0),
            seen_at: 0.0,
            hits: 5,
        }
    }

    #[test]
    fn frame_is_yuva420p_sized_and_transparent() {
        let renderer = OverlayRenderer::new(1920, 1080);
        assert_eq!(renderer.frame().len(), 1920 * 1080 * 5 / 2);
        assert!(
            renderer.frame()[1920 * 1080 * 3 / 2..]
                .iter()
                .all(|a| *a == 0)
        );
    }

    #[test]
    fn boxes_are_drawn_and_cleared_when_they_move() {
        let mut renderer = OverlayRenderer::new(640, 360);
        renderer.render(&scene(1, vec![cow(0.1)]), 0.0);
        // Left edge of the box at x = 64, y = 180..270.
        assert_eq!(alpha(&renderer, 64, 200), 255);
        assert_eq!(alpha(&renderer, 100, 200), 0, "the inside stays clear");
        // Label above the box.
        assert!(alpha(&renderer, 66, 170) > 0);

        renderer.render(&scene(2, vec![cow(0.5)]), 0.0);
        assert_eq!(alpha(&renderer, 64, 200), 0);
        assert_eq!(alpha(&renderer, 320, 200), 255);

        renderer.render(&scene(3, Vec::new()), 0.0);
        let (w, h) = (640_usize, 360_usize);
        assert!(renderer.frame()[w * h * 3 / 2..].iter().all(|a| *a == 0));
    }

    #[test]
    fn hidden_boxes_still_allow_the_counter() {
        let mut renderer = OverlayRenderer::new(640, 360);
        let mut counted = scene(1, vec![cow(0.5)]);
        counted.show_boxes = false;
        counted.show_counter = true;
        counted.counts = CountSummary {
            current_total: 1,
            current_by_class: vec![ClassCount {
                class: "cow".into(),
                count: 1,
            }],
            unique_total: 1,
            unique_by_class: Vec::new(),
        };
        renderer.render(&counted, 0.0);
        assert_eq!(alpha(&renderer, 320, 200), 0, "no box");
        assert_eq!(alpha(&renderer, 20, 20), 160, "counter panel");
    }

    #[test]
    fn boxes_reaching_the_edge_are_clipped() {
        let mut renderer = OverlayRenderer::new(64, 64);
        let mut track = cow(0.9);
        track.bbox.width = 0.5;
        renderer.render(&scene(1, vec![track]), 0.0);
        assert_eq!(alpha(&renderer, 63, 40), 255);
    }

    #[test]
    fn input_args_describe_the_raw_stream() {
        let input = OverlayInput {
            port: 40000,
            width: 1280,
            height: 720,
        };
        let args = input.input_args();
        assert!(args.windows(2).any(|w| w == ["-pix_fmt", "yuva420p"]));
        assert!(args.windows(2).any(|w| w == ["-video_size", "1280x720"]));
        assert_eq!(
            args.last().unwrap(),
            "tcp://127.0.0.1:40000?recv_buffer_size=65536"
        );
    }

    #[test]
    fn colours_convert_to_limited_range() {
        let white = Yuv::from_rgb(255, 255, 255);
        assert_eq!((white.0, white.1, white.2), (235, 128, 128));
        let black = Yuv::from_rgb(0, 0, 0);
        assert_eq!(black.0, 16);
    }
}
