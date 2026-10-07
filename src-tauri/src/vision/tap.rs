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
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
    time::Duration,
};

use super::{detector::RgbFrame, models::InputResize, settings::InferenceRate};
use crate::{
    error::{BridgeError, BridgeResult},
    ffmpeg,
    process::{ProcessSpec, ProcessSupervisor, RestartPolicy},
};

pub const TAP_PROCESS_NAME: &str = "vision-tap";

/// Holds the newest picture; replacing an unread one counts as a drop.
#[derive(Default)]
pub struct FrameSlot {
    state: Mutex<SlotState>,
    ready: Condvar,
}

#[derive(Default)]
struct SlotState {
    frame: Option<RgbFrame>,
    sequence: u64,
    dropped: u64,
}

impl FrameSlot {
    pub fn put(&self, frame: RgbFrame) {
        let mut state = self.state.lock().expect("frame slot lock");
        if state.frame.replace(frame).is_some() {
            state.dropped += 1;
        }
        state.sequence += 1;
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
}

pub struct FrameTap {
    stop: Arc<AtomicBool>,
    reader: Option<JoinHandle<()>>,
}

impl FrameTap {
    pub async fn start(
        supervisor: &ProcessSupervisor,
        input_size: u32,
        resize: InputResize,
        slot: Arc<FrameSlot>,
    ) -> BridgeResult<Self> {
        let ffmpeg = ffmpeg::locate("ffmpeg")
            .ok_or_else(|| BridgeError::Ffmpeg("FFmpeg is unavailable".into()))?;
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
        listener.set_nonblocking(true)?;
        let port = listener.local_addr()?.port();
        let stop = Arc::new(AtomicBool::new(false));
        let reader = {
            let stop = stop.clone();
            std::thread::Builder::new()
                .name("vision-tap-reader".into())
                .spawn(move || receive(listener, input_size, &slot, &stop))?
        };
        let tap = Self {
            stop,
            reader: Some(reader),
        };
        supervisor
            .start(ProcessSpec {
                name: TAP_PROCESS_NAME.into(),
                executable: ffmpeg,
                args: tap_args(port, input_size, resize),
                restart_policy: RestartPolicy::OnFailure,
            })
            .await?;
        Ok(tap)
    }

    pub async fn stop(mut self, supervisor: &ProcessSupervisor) {
        self.stop.store(true, Ordering::Relaxed);
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
    }
}

fn tap_args(port: u16, input_size: u32, resize: InputResize) -> Vec<OsString> {
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
            &format!("fps={},{scale}", InferenceRate::MAX_FPS),
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
                    match read_ppm(&mut reader, input_size) {
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
pub fn read_ppm(reader: &mut impl BufRead, max_edge: u32) -> std::io::Result<Option<RgbFrame>> {
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
    let mut pixels = vec![0_u8; width as usize * height as usize * 3];
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
    fn tap_shrinks_before_the_app_sees_a_picture() {
        let args: Vec<String> = tap_args(5000, 416, InputResize::Letterbox)
            .into_iter()
            .map(|arg| arg.into_string().unwrap())
            .collect();
        let filter = &args[args.iter().position(|a| a == "-vf").unwrap() + 1];
        assert!(filter.starts_with("fps=15,scale=416:416:force_original_aspect_ratio=decrease"));
        assert_eq!(args.last().unwrap(), "tcp://127.0.0.1:5000");
        assert!(args.iter().any(|a| a == "rtsp://127.0.0.1:8554/drone"));
        let stretched: Vec<String> = tap_args(5000, 640, InputResize::Stretch)
            .into_iter()
            .map(|arg| arg.into_string().unwrap())
            .collect();
        assert!(stretched.contains(&"fps=15,scale=640:640:flags=bilinear".to_string()));
    }
}
