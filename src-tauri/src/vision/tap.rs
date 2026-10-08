//! The detector's view of the drone: a separate FFmpeg reads the same
//! `/drone` RTSP path every other output reads, decodes it (hardware decode on
//! macOS), drops to at most `InferenceRate::MAX_FPS` and shrinks each picture
//! to fit the model input before the app ever sees it. A 1080p picture never
//! crosses into the app; ~0.3 MB pictures do.
//!
//! Pictures arrive as PPM (it carries its own size) over a local socket, and
//! only the newest one is kept: if inference falls behind, older pictures are
//! dropped instead of queueing. Nothing here can slow the stream down —
//! MediaMTX gives each reader its own queue.

use std::{
    ffi::OsString,
    io::{BufRead, BufReader},
    net::{Ipv4Addr, TcpListener},
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, AtomicU32, Ordering},
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

use super::{detector::RgbFrame, models::InputResize, settings::InferenceRate};
use crate::{
    error::{BridgeError, BridgeResult},
    ffmpeg,
    process::{ProcessSpec, ProcessSupervisor, RestartPolicy},
};

pub const TAP_PROCESS_NAME: &str = "vision-tap";

/// Serialize tap starts with app shutdown without waiting for model loading.
/// Once closed, no rate adjustment or pipeline load may start another child.
#[derive(Default)]
pub struct TapLifecycle {
    closing: AtomicBool,
    gate: tokio::sync::Mutex<()>,
}

impl TapLifecycle {
    pub async fn close(&self) {
        self.closing.store(true, Ordering::SeqCst);
        // A start already holding the gate finishes before the app's
        // supervisor takes its final list of children to stop.
        let _guard = self.gate.lock().await;
    }
}

/// Holds the newest picture; replacing an unread one counts as a drop.
pub struct FrameSlot {
    state: Mutex<SlotState>,
    ready: Condvar,
    requested_fps: AtomicU32,
}

impl Default for FrameSlot {
    fn default() -> Self {
        Self {
            state: Mutex::default(),
            ready: Condvar::default(),
            requested_fps: AtomicU32::new(InferenceRate::MAX_FPS),
        }
    }
}

#[derive(Default)]
struct SlotState {
    frame: Option<RgbFrame>,
    dropped: u64,
    /// At most two spare buffers, in addition to the pending picture and
    /// the one being detected. No queue of old pictures is ever retained.
    buffers: Vec<Vec<u8>>,
}

impl FrameSlot {
    pub fn put(&self, frame: RgbFrame) {
        let mut state = self.state.lock().expect("frame slot lock");
        if let Some(previous) = state.frame.replace(frame) {
            state.dropped += 1;
            recycle_buffer(&mut state, previous.pixels);
        }
        self.ready.notify_one();
    }

    /// Takes the newest picture, waiting up to `timeout` for one.
    pub fn take(&self, timeout: Duration) -> Option<RgbFrame> {
        let state = self.state.lock().expect("frame slot lock");
        let (mut state, _) = self
            .ready
            .wait_timeout_while(state, timeout, |state| state.frame.is_none())
            .expect("frame slot lock");
        state.frame.take()
    }

    pub fn dropped(&self) -> u64 {
        self.state.lock().expect("frame slot lock").dropped
    }

    pub fn recycle(&self, frame: RgbFrame) {
        recycle_buffer(
            &mut self.state.lock().expect("frame slot lock"),
            frame.pixels,
        );
    }

    fn take_buffer(&self) -> Vec<u8> {
        self.state
            .lock()
            .expect("frame slot lock")
            .buffers
            .pop()
            .unwrap_or_default()
    }

    pub fn request_fps(&self, fps: u32) {
        self.requested_fps
            .store(fps.clamp(1, InferenceRate::MAX_FPS), Ordering::Relaxed);
    }

    fn requested_fps(&self) -> u32 {
        self.requested_fps.load(Ordering::Relaxed)
    }
}

fn recycle_buffer(state: &mut SlotState, pixels: Vec<u8>) {
    if state.buffers.len() < 2 {
        state.buffers.push(pixels);
    }
}

pub struct FrameTap {
    stop: Arc<AtomicBool>,
    reader: Option<JoinHandle<()>>,
    rate_control: Option<tokio::task::JoinHandle<()>>,
}

impl FrameTap {
    pub async fn start(
        supervisor: &ProcessSupervisor,
        input_size: u32,
        resize: InputResize,
        slot: Arc<FrameSlot>,
        lifecycle: Arc<TapLifecycle>,
    ) -> BridgeResult<Self> {
        let ffmpeg = ffmpeg::locate("ffmpeg")
            .ok_or_else(|| BridgeError::Ffmpeg("FFmpeg is unavailable".into()))?;
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
        listener.set_nonblocking(true)?;
        let port = listener.local_addr()?.port();
        let stop = Arc::new(AtomicBool::new(false));
        let reader = {
            let stop = stop.clone();
            let slot = slot.clone();
            std::thread::Builder::new()
                .name("vision-tap-reader".into())
                .spawn(move || receive(listener, input_size, &slot, &stop))?
        };
        let mut tap = Self {
            stop,
            reader: Some(reader),
            rate_control: None,
        };
        let fps = slot.requested_fps();
        let process = ProcessSpec {
            name: TAP_PROCESS_NAME.into(),
            executable: ffmpeg,
            args: tap_args(port, input_size, resize, fps),
            restart_policy: RestartPolicy::OnFailure,
        };
        {
            let _guard = lifecycle.gate.lock().await;
            if lifecycle.closing.load(Ordering::SeqCst) {
                return Err(BridgeError::Vision("the app is quitting".into()));
            }
            supervisor.start(process.clone()).await?;
        }
        let stop = tap.stop.clone();
        let supervisor = supervisor.clone();
        tap.rate_control = Some(tokio::spawn(async move {
            let mut rate = TapRate::new(fps);
            loop {
                tokio::time::sleep(Duration::from_secs(1)).await;
                if stop.load(Ordering::Relaxed) {
                    break;
                }
                let requested = slot.requested_fps();
                if !rate.should_change(requested, Instant::now()) {
                    continue;
                }
                let mut updated = process.clone();
                updated.args = tap_args(port, input_size, resize, requested);
                // Only the tap reconnects: the model, tracker and outputs
                // stay alive, and the reader accepts the new connection.
                let _guard = lifecycle.gate.lock().await;
                if stop.load(Ordering::Relaxed) || lifecycle.closing.load(Ordering::SeqCst) {
                    break;
                }
                match supervisor.start(updated).await {
                    Ok(()) => rate.changed(requested, Instant::now()),
                    Err(error) => {
                        tracing::warn!(%error, "could not adjust AI Vision tap rate");
                        // Avoid repeatedly spawning a failing FFmpeg.
                        rate.changed(rate.fps, Instant::now());
                    }
                }
            }
        }));
        Ok(tap)
    }

    pub async fn stop(mut self, supervisor: &ProcessSupervisor) {
        self.stop.store(true, Ordering::Relaxed);
        // Cancel and join before stopping FFmpeg: the rate controller must
        // never start a new child after shutdown has stopped the last one.
        if let Some(control) = self.rate_control.take() {
            control.abort();
            let _ = control.await;
        }
        // Ending FFmpeg closes the socket, which ends the reader.
        supervisor.stop(TAP_PROCESS_NAME).await.ok();
        if let Some(reader) = self.reader.take() {
            let _ = tokio::task::spawn_blocking(move || reader.join()).await;
        }
    }
}

impl Drop for FrameTap {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(control) = self.rate_control.take() {
            control.abort();
        }
    }
}

/// Restart only after a stable, material change, so fluctuating inference
/// times cannot make FFmpeg continuously reconnect to the drone.
struct TapRate {
    fps: u32,
    requested: u32,
    stable_samples: u32,
    changed_at: Instant,
}

impl TapRate {
    fn new(fps: u32) -> Self {
        Self {
            fps,
            requested: fps,
            stable_samples: 0,
            changed_at: Instant::now(),
        }
    }

    fn should_change(&mut self, requested: u32, now: Instant) -> bool {
        if requested != self.requested {
            self.requested = requested;
            self.stable_samples = 0;
        }
        self.stable_samples += 1;
        let material = self.fps.abs_diff(requested) >= (self.fps / 4).max(1);
        material
            && self.stable_samples >= 3
            && now.duration_since(self.changed_at) >= Duration::from_secs(10)
    }

    fn changed(&mut self, fps: u32, now: Instant) {
        self.fps = fps;
        self.changed_at = now;
        self.stable_samples = 0;
    }
}

fn tap_args(port: u16, input_size: u32, resize: InputResize, fps: u32) -> Vec<OsString> {
    let scale = match resize {
        InputResize::Letterbox => format!(
            "scale={input_size}:{input_size}:force_original_aspect_ratio=decrease:flags=bilinear"
        ),
        InputResize::Stretch => format!("scale={input_size}:{input_size}:flags=bilinear"),
    };
    let mut args: Vec<String> = ["-hide_banner", "-loglevel", "warning"]
        .into_iter()
        .map(str::to_string)
        .collect();
    // Decoding on the media engine leaves the CPU to the encoders. FFmpeg
    // falls back to software decoding by itself if it cannot.
    if cfg!(target_os = "macos") {
        args.extend(["-hwaccel", "videotoolbox"].map(str::to_string));
    }
    args.extend(
        [
            "-fflags",
            "nobuffer",
            "-flags",
            "low_delay",
            "-rtsp_transport",
            "tcp",
            "-i",
            "rtsp://127.0.0.1:8554/drone",
            "-map",
            "0:v:0",
            "-an",
            "-vf",
            &format!("fps={},{scale}", fps.clamp(1, InferenceRate::MAX_FPS)),
            "-c:v",
            "ppm",
            "-f",
            "image2pipe",
            &format!("tcp://127.0.0.1:{port}"),
        ]
        .map(str::to_string),
    );
    args.into_iter().map(OsString::from).collect()
}

/// Accepts the tap's connection (again after an FFmpeg restart) and moves
/// pictures into the slot until told to stop.
fn receive(listener: TcpListener, input_size: u32, slot: &FrameSlot, stop: &AtomicBool) {
    while !stop.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((stream, _)) => {
                if stream.set_nonblocking(false).is_err() {
                    continue;
                }
                let mut reader = BufReader::with_capacity(1 << 20, stream);
                while !stop.load(Ordering::Relaxed) {
                    match read_ppm_with_buffer(&mut reader, input_size, slot.take_buffer()) {
                        Ok(Some(frame)) => slot.put(frame),
                        Ok(None) => break,
                        Err(error) => {
                            tracing::warn!(%error, "AI Vision tap sent an unreadable picture");
                            break;
                        }
                    }
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(error) => {
                tracing::warn!(%error, "AI Vision tap listener failed");
                std::thread::sleep(Duration::from_millis(200));
            }
        }
    }
}

/// Reads one binary PPM (P6, maxval 255); `None` at a clean end of stream.
#[cfg(test)]
pub fn read_ppm(reader: &mut impl BufRead, max_edge: u32) -> std::io::Result<Option<RgbFrame>> {
    read_ppm_with_buffer(reader, max_edge, Vec::new())
}

fn read_ppm_with_buffer(
    reader: &mut impl BufRead,
    max_edge: u32,
    mut pixels: Vec<u8>,
) -> std::io::Result<Option<RgbFrame>> {
    let invalid =
        |message: &str| std::io::Error::new(std::io::ErrorKind::InvalidData, message.to_string());
    if reader.fill_buf()?.is_empty() {
        return Ok(None);
    }
    let magic = header_token(reader)?;
    if magic != "P6" {
        return Err(invalid("not a binary PPM"));
    }
    let width: u32 = header_token(reader)?
        .parse()
        .map_err(|_| invalid("bad width"))?;
    let height: u32 = header_token(reader)?
        .parse()
        .map_err(|_| invalid("bad height"))?;
    let maxval: u32 = header_token(reader)?
        .parse()
        .map_err(|_| invalid("bad maxval"))?;
    if width == 0 || height == 0 || width > max_edge || height > max_edge || maxval != 255 {
        return Err(invalid("unexpected picture size"));
    }
    pixels.resize(width as usize * height as usize * 3, 0);
    reader.read_exact(&mut pixels)?;
    Ok(Some(RgbFrame {
        width,
        height,
        pixels,
    }))
}

/// One whitespace-delimited header field; consumes the single whitespace
/// byte after it (the one before the pixel data, for the last field).
fn header_token(reader: &mut impl BufRead) -> std::io::Result<String> {
    let mut token = String::new();
    let mut byte = [0_u8; 1];
    loop {
        reader.read_exact(&mut byte)?;
        match byte[0] {
            b'#' if token.is_empty() => {
                let mut comment = Vec::new();
                reader.read_until(b'\n', &mut comment)?;
            }
            value if value.is_ascii_whitespace() => {
                if !token.is_empty() {
                    return Ok(token);
                }
            }
            value => {
                if token.len() > 16 {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "PPM header field too long",
                    ));
                }
                token.push(value as char);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ppm(width: u32, height: u32, fill: u8) -> Vec<u8> {
        let mut bytes = format!("P6\n{width} {height}\n255\n").into_bytes();
        bytes.extend(std::iter::repeat_n(fill, (width * height * 3) as usize));
        bytes
    }

    #[test]
    fn reads_consecutive_pictures_and_the_end_of_stream() {
        let mut stream = ppm(4, 2, 7);
        stream.extend(b"P6 # comment\n3 1 255\n");
        stream.extend([1, 2, 3, 4, 5, 6, 7, 8, 9]);
        let mut reader = std::io::Cursor::new(stream);
        let first = read_ppm(&mut reader, 416).unwrap().unwrap();
        assert_eq!((first.width, first.height), (4, 2));
        assert!(first.pixels.iter().all(|p| *p == 7));
        let second = read_ppm(&mut reader, 416).unwrap().unwrap();
        assert_eq!(second.pixels, vec![1, 2, 3, 4, 5, 6, 7, 8, 9]);
        assert!(read_ppm(&mut reader, 416).unwrap().is_none());
    }

    #[test]
    fn rejects_pictures_larger_than_the_model_input() {
        let mut reader = std::io::Cursor::new(ppm(500, 2, 0));
        assert!(read_ppm(&mut reader, 416).is_err());
    }

    #[test]
    fn the_slot_keeps_only_the_newest_picture() {
        let slot = FrameSlot::default();
        for value in 0..3 {
            slot.put(RgbFrame {
                width: 1,
                height: 1,
                pixels: vec![value; 3],
            });
        }
        assert_eq!(slot.dropped(), 2);
        let frame = slot.take(Duration::from_millis(1)).unwrap();
        assert_eq!(frame.pixels[0], 2);
        assert!(slot.take(Duration::from_millis(1)).is_none());
    }

    #[test]
    fn recycled_pixels_are_reused_and_overwritten_when_the_picture_changes_size() {
        let slot = FrameSlot::default();
        let mut input = std::io::Cursor::new(ppm(4, 2, 7));
        let first = read_ppm_with_buffer(&mut input, 416, slot.take_buffer())
            .unwrap()
            .unwrap();
        let pointer = first.pixels.as_ptr();
        slot.recycle(first);
        let mut input = std::io::Cursor::new(ppm(2, 2, 19));
        let second = read_ppm_with_buffer(&mut input, 416, slot.take_buffer())
            .unwrap()
            .unwrap();
        assert_eq!(second.pixels.as_ptr(), pointer);
        assert_eq!(second.pixels, vec![19; 12]);
        slot.recycle(second);
        let mut input = std::io::Cursor::new(ppm(4, 2, 31));
        let third = read_ppm_with_buffer(&mut input, 416, slot.take_buffer())
            .unwrap()
            .unwrap();
        assert_eq!(third.pixels.as_ptr(), pointer);
        assert_eq!(third.pixels, vec![31; 24]);
    }

    #[test]
    fn dropping_pictures_recycles_buffers_without_keeping_old_frames() {
        let slot = FrameSlot::default();
        for value in 0..20 {
            slot.put(RgbFrame {
                width: 1,
                height: 1,
                pixels: vec![value; 3],
            });
        }
        assert_eq!(slot.dropped(), 19);
        assert_eq!(slot.state.lock().unwrap().buffers.len(), 2);
        assert_eq!(slot.take(Duration::ZERO).unwrap().pixels, vec![19; 3]);
        assert!(slot.take(Duration::ZERO).is_none());
    }

    #[test]
    fn rate_changes_require_stability_and_do_not_restart_on_every_fluctuation() {
        let mut rate = TapRate::new(15);
        let ready = rate.changed_at + Duration::from_secs(10);
        assert!(!rate.should_change(5, ready));
        assert!(!rate.should_change(6, ready));
        assert!(!rate.should_change(5, ready));
        assert!(!rate.should_change(5, ready));
        assert!(rate.should_change(5, ready));
        rate.changed(5, ready);
        for _ in 0..5 {
            assert!(!rate.should_change(7, ready + Duration::from_secs(9)));
        }
        assert!(rate.should_change(7, ready + Duration::from_secs(10)));
    }

    #[tokio::test]
    async fn shutdown_waits_for_an_in_flight_start_before_final_child_cleanup() {
        let lifecycle = Arc::new(TapLifecycle::default());
        let in_flight = lifecycle.gate.lock().await;
        let close = {
            let lifecycle = lifecycle.clone();
            tokio::spawn(async move { lifecycle.close().await })
        };
        tokio::task::yield_now().await;
        assert!(lifecycle.closing.load(Ordering::SeqCst));
        assert!(!close.is_finished());
        drop(in_flight);
        close.await.unwrap();
        let _next_start = lifecycle.gate.lock().await;
        assert!(lifecycle.closing.load(Ordering::SeqCst));
    }

    #[test]
    fn tap_shrinks_before_the_app_sees_a_picture() {
        let args: Vec<String> = tap_args(5000, 416, InputResize::Letterbox, 5)
            .into_iter()
            .map(|arg| arg.into_string().unwrap())
            .collect();
        let filter = &args[args.iter().position(|a| a == "-vf").unwrap() + 1];
        assert!(filter.starts_with("fps=5,scale=416:416:force_original_aspect_ratio=decrease"));
        assert_eq!(args.last().unwrap(), "tcp://127.0.0.1:5000");
        assert!(args.iter().any(|a| a == "rtsp://127.0.0.1:8554/drone"));
        let stretched: Vec<String> = tap_args(5000, 640, InputResize::Stretch, 15)
            .into_iter()
            .map(|arg| arg.into_string().unwrap())
            .collect();
        assert!(stretched.contains(&"fps=15,scale=640:640:flags=bilinear".to_string()));
    }
}
