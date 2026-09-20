//! Publishes drone frames into shared memory for the Windows DirectShow filter.
//!
//! FFmpeg writes raw NV12 frames to this process's pipe; each complete frame is
//! copied into a named file mapping that the camera filter — running inside
//! TikTok LIVE Studio, OBS or any other capture app — reads.
//!
//! The layout matches `native/windows/shared_frame.h`. The sequence counter is
//! odd while a frame is being written and even when one is complete, so a
//! reader can tell whether what it copied was torn.

use std::{
    io::Read,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use windows::{
    Win32::Foundation::{CloseHandle, HANDLE},
    Win32::System::Memory::{
        CreateFileMappingW, FILE_MAP_ALL_ACCESS, MapViewOfFile, MEMORY_MAPPED_VIEW_ADDRESS,
        PAGE_READWRITE, UnmapViewOfFile,
    },
    core::w,
};

use crate::error::{BridgeError, BridgeResult};

pub const FRAME_WIDTH: u32 = 1080;
pub const FRAME_HEIGHT: u32 = 1920;
pub const FRAME_FPS: u32 = 30;
/// NV12: full-size luma plane plus half-height interleaved chroma.
pub const FRAME_BYTES: usize = (FRAME_WIDTH as usize) * (FRAME_HEIGHT as usize) * 3 / 2;

const MAGIC: u32 = 0x444A_4931; // "DJI1"
const HEADER_BYTES: usize = 32;

/// Shared memory the camera filter reads. Dropping it releases the mapping,
/// after which the filter falls back to its placeholder frame.
pub struct FrameBridge {
    mapping: HANDLE,
    view: MEMORY_MAPPED_VIEW_ADDRESS,
    sequence: u32,
}

// The mapping is owned by this struct and only written from the feed thread.
unsafe impl Send for FrameBridge {}

impl FrameBridge {
    pub fn create() -> BridgeResult<Self> {
        let total = (HEADER_BYTES + FRAME_BYTES) as u32;
        // SAFETY: a named mapping of a fixed size, released in Drop.
        let mapping = unsafe {
            CreateFileMappingW(
                HANDLE::default(),
                None,
                PAGE_READWRITE,
                0,
                total,
                w!("Local\\DJILiveBridgeFrame"),
            )
        }
        .map_err(|error| {
            BridgeError::VirtualCamera(format!("could not create the frame buffer: {error}"))
        })?;

        let view = unsafe { MapViewOfFile(mapping, FILE_MAP_ALL_ACCESS, 0, 0, total as usize) };
        if view.Value.is_null() {
            unsafe { CloseHandle(mapping).ok() };
            return Err(BridgeError::VirtualCamera(
                "could not map the frame buffer".into(),
            ));
        }

        let mut bridge = Self {
            mapping,
            view,
            sequence: 0,
        };
        bridge.write_header();
        Ok(bridge)
    }

    fn header_mut(&mut self) -> &mut [u32] {
        // SAFETY: the mapping is at least HEADER_BYTES long and 4-byte aligned.
        unsafe { std::slice::from_raw_parts_mut(self.view.Value as *mut u32, HEADER_BYTES / 4) }
    }

    fn write_header(&mut self) {
        let header = self.header_mut();
        header[0] = MAGIC;
        header[1] = FRAME_WIDTH;
        header[2] = FRAME_HEIGHT;
        header[3] = FRAME_FPS;
        header[4] = 0; // sequence: no frame yet
    }

    /// Copies one frame in, marking it incomplete while the copy runs.
    pub fn publish(&mut self, frame: &[u8]) {
        if frame.len() != FRAME_BYTES {
            return;
        }
        self.sequence = self.sequence.wrapping_add(1);
        let odd = self.sequence | 1;
        self.header_mut()[4] = odd;

        // SAFETY: the pixel area follows the header and is FRAME_BYTES long.
        unsafe {
            let pixels = (self.view.Value as *mut u8).add(HEADER_BYTES);
            std::ptr::copy_nonoverlapping(frame.as_ptr(), pixels, FRAME_BYTES);
        }

        self.sequence = odd.wrapping_add(1); // even: the frame is complete
        self.header_mut()[4] = self.sequence;
    }
}

impl Drop for FrameBridge {
    fn drop(&mut self) {
        // Tell readers the stream is gone before the memory disappears.
        self.header_mut()[4] = 0;
        unsafe {
            UnmapViewOfFile(self.view).ok();
            CloseHandle(self.mapping).ok();
        }
    }
}

/// Reads NV12 frames from the FFmpeg pipe and publishes them until the pipe
/// closes or `running` is cleared.
pub fn pump_frames<R: Read>(mut source: R, running: Arc<AtomicBool>) -> BridgeResult<()> {
    let mut bridge = FrameBridge::create()?;
    let mut frame = vec![0u8; FRAME_BYTES];
    while running.load(Ordering::Relaxed) {
        match source.read_exact(&mut frame) {
            Ok(()) => bridge.publish(&frame),
            Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(error) => {
                return Err(BridgeError::VirtualCamera(format!(
                    "the camera feed stopped: {error}"
                )));
            }
        }
    }
    Ok(())
}
