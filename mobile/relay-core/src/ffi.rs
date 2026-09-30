//! The C interface the iOS app calls (see `mobile/ios/RelayCore/relay_core.h`); Android uses the
//! JNI functions in `lib.rs`. Both reach the same relay.
//!
//! Strings go in as NUL-terminated UTF-8 and are only borrowed for the call. A returned string
//! or byte buffer belongs to the caller, who frees it with `djr_string_free` or
//! `djr_bytes_free`. Results carry codes, not user-facing text, exactly as over JNI.

use std::ffi::{c_char, CStr, CString};
use std::ptr;
use std::time::Duration;

/// An error code as a string the caller frees, or null for success.
fn result_string(result: Result<(), String>) -> *mut c_char {
    match result {
        Ok(()) => ptr::null_mut(),
        Err(error) => owned_string(&error),
    }
}

fn owned_string(value: &str) -> *mut c_char {
    // Codes and JSON never contain NUL; should one appear, it is dropped rather than failing.
    CString::new(value.replace('\0', ""))
        .map(CString::into_raw)
        .unwrap_or(ptr::null_mut())
}

/// # Safety
/// `value` is null or a valid NUL-terminated string.
unsafe fn borrowed(value: *const c_char) -> Result<String, String> {
    if value.is_null() {
        return Ok(String::new());
    }
    // SAFETY: guaranteed by the caller.
    unsafe { CStr::from_ptr(value) }
        .to_str()
        .map(str::to_owned)
        .map_err(|_| "ffi_argument".to_owned())
}

/// Starts receiving DJI Fly with no platform attached; returns an error code or null.
#[no_mangle]
pub extern "C" fn djr_start_receiver() -> *mut c_char {
    // A peer that resets a connection must never end the app through SIGPIPE.
    // SAFETY: changes this process's disposition of one signal only.
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_IGN);
    }
    result_string(crate::start_server())
}

/// Sends the received stream to one more platform, known by `output_id`; returns an error code
/// or null. RTMPS is verified with the system's trust store.
///
/// # Safety
/// Every argument is null or a valid NUL-terminated string.
#[no_mangle]
pub unsafe extern "C" fn djr_go_live(
    output_id: *const c_char,
    server_url: *const c_char,
    stream_key: *const c_char,
) -> *mut c_char {
    // SAFETY: guaranteed by the caller.
    let arguments = unsafe {
        (
            borrowed(output_id),
            borrowed(server_url),
            borrowed(stream_key),
        )
    };
    let (Ok(output_id), Ok(server_url), Ok(stream_key)) = arguments else {
        return owned_string("ffi_argument");
    };
    result_string(crate::set_destination_with_tls_ca(
        &output_id,
        &server_url,
        &stream_key,
        "",
    ))
}

/// Stops sending to the platform `output_id`, or to every platform when it is null or empty;
/// the drone stays connected either way.
///
/// # Safety
/// `output_id` is null or a valid NUL-terminated string.
#[no_mangle]
pub unsafe extern "C" fn djr_end_live(output_id: *const c_char) {
    // SAFETY: guaranteed by the caller.
    let output_id = unsafe { borrowed(output_id) }.unwrap_or_default();
    if output_id.is_empty() {
        crate::clear_destinations();
    } else {
        crate::clear_destination(&output_id);
    }
}

/// The relay's state as JSON (see `RelaySnapshot`); never null.
#[no_mangle]
pub extern "C" fn djr_snapshot_json() -> *mut c_char {
    owned_string(&crate::snapshot_json())
}

/// Stops every platform output and the receiver.
#[no_mangle]
pub extern "C" fn djr_stop() {
    crate::stop_server();
}

/// Frees a string returned by this library; null is ignored.
///
/// # Safety
/// `value` is null or a string this library returned and that was not freed yet.
#[no_mangle]
pub unsafe extern "C" fn djr_string_free(value: *mut c_char) {
    if !value.is_null() {
        // SAFETY: the string came from CString::into_raw above.
        drop(unsafe { CString::from_raw(value) });
    }
}

/// One video tag of the preview: its RTMP timestamp and the FLV video tag body.
#[repr(C)]
pub struct DjrPreviewFrame {
    pub timestamp: u32,
    pub data: *mut u8,
    pub length: usize,
}

/// Starts an in-app preview session and returns its number for the calls below.
#[no_mangle]
pub extern "C" fn djr_preview_start() -> u64 {
    crate::preview::start()
}

/// Waits up to `timeout_ms` for the next video tag of `session`. Returns false when nothing
/// arrived or the session ended; otherwise fills `frame`, whose data the caller frees with
/// `djr_bytes_free`.
///
/// # Safety
/// `frame` points to writable memory for one `DjrPreviewFrame`.
#[no_mangle]
pub unsafe extern "C" fn djr_preview_next(
    session: u64,
    timeout_ms: u32,
    frame: *mut DjrPreviewFrame,
) -> bool {
    if frame.is_null() {
        return false;
    }
    let Some((timestamp, payload)) =
        crate::preview::next(session, Duration::from_millis(u64::from(timeout_ms)))
    else {
        return false;
    };
    let bytes: Box<[u8]> = Box::from(&payload[..]);
    let length = bytes.len();
    let data = Box::into_raw(bytes).cast::<u8>();
    // SAFETY: `frame` is valid for writes, guaranteed by the caller.
    unsafe {
        frame.write(DjrPreviewFrame {
            timestamp,
            data,
            length,
        });
    }
    true
}

/// Frees the data of a preview frame; null is ignored.
///
/// # Safety
/// `data` and `length` come from one `djr_preview_next` call and were not freed yet.
#[no_mangle]
pub unsafe extern "C" fn djr_bytes_free(data: *mut u8, length: usize) {
    if !data.is_null() {
        // SAFETY: the buffer came from Box::into_raw of a boxed slice of `length` bytes.
        drop(unsafe { Box::from_raw(ptr::slice_from_raw_parts_mut(data, length)) });
    }
}

/// Ends the preview session `session`; a newer one is left alone.
#[no_mangle]
pub extern "C" fn djr_preview_stop(session: u64) {
    crate::preview::stop(session);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn take(value: *mut c_char) -> Option<String> {
        if value.is_null() {
            return None;
        }
        // SAFETY: returned by this library just now.
        let text = unsafe { CStr::from_ptr(value) }
            .to_string_lossy()
            .into_owned();
        unsafe { djr_string_free(value) };
        Some(text)
    }

    #[test]
    fn going_live_without_a_receiver_reports_the_code() {
        let id = CString::new("youtube").unwrap();
        let url = CString::new("rtmp://127.0.0.1:9/live").unwrap();
        let key = CString::new("target-key-1234").unwrap();
        let error = take(unsafe { djr_go_live(id.as_ptr(), url.as_ptr(), key.as_ptr()) });
        assert_eq!(error.as_deref(), Some("receiver_not_running"));
    }

    #[test]
    fn a_bad_argument_is_an_error_not_a_crash() {
        let bad = [0xFFu8, 0xFE, 0];
        let error = take(unsafe { djr_go_live(bad.as_ptr().cast(), ptr::null(), ptr::null()) });
        assert_eq!(error.as_deref(), Some("ffi_argument"));
        unsafe { djr_end_live(ptr::null()) };
        unsafe { djr_string_free(ptr::null_mut()) };
        unsafe { djr_bytes_free(ptr::null_mut(), 0) };
    }

    #[test]
    fn the_snapshot_is_json() {
        let json = take(djr_snapshot_json()).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(parsed.get("status").is_some());
    }

    #[test]
    fn an_empty_preview_returns_nothing() {
        let session = djr_preview_start();
        let mut frame = DjrPreviewFrame {
            timestamp: 0,
            data: ptr::null_mut(),
            length: 0,
        };
        assert!(!unsafe { djr_preview_next(session, 1, &mut frame) });
        djr_preview_stop(session);
    }
}
