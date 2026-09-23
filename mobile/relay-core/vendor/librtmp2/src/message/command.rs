//! RTMP command message encoder/decoder
//!
//! Mirrors `src/message/command.h` and `src/message/command.c`.

use crate::amf::amf0;
use crate::buffer::Buffer;
use crate::ertmp::connect_amf;
use crate::types::CAPS_EX_MASK_MULTITRACK;
use crate::types::ConnectInfo;
use crate::types::ErrorCode;
use crate::types::NegotiatedCaps;
use crate::types::Result;

/// Maximum key/value pairs in a connect object
const MAX_CONNECT_OBJECT_KEYS: usize = 256;

/* ── Encoder ── */

/// Build a "connect" command.
pub fn build_connect(
    buf: &mut Buffer,
    app: &str,
    tc_url: &str,
    page_url: &str,
    swf_url: &str,
    flash_ver: &str,
    audio_codecs: i32,
    video_codecs: i32,
    caps: Option<&NegotiatedCaps>,
) -> Result<()> {
    macro_rules! chk {
        ($expr:expr) => {
            $expr.map_err(|_| ErrorCode::Internal)?
        };
    }

    chk!(amf0::write_string(buf, "connect"));
    chk!(amf0::write_number(buf, 1.0));
    chk!(amf0::write_object_begin(buf));
    chk!(amf0::write_object_key(buf, "app"));
    chk!(amf0::write_string(buf, app));
    chk!(amf0::write_object_key(buf, "type"));
    chk!(amf0::write_string(buf, "nonprivate"));

    chk!(amf0::write_object_key(buf, "tcUrl"));
    chk!(amf0::write_string(buf, tc_url));
    if !page_url.is_empty() {
        chk!(amf0::write_object_key(buf, "pageUrl"));
        chk!(amf0::write_string(buf, page_url));
    }
    if !swf_url.is_empty() {
        chk!(amf0::write_object_key(buf, "swfUrl"));
        chk!(amf0::write_string(buf, swf_url));
    }
    if !flash_ver.is_empty() {
        chk!(amf0::write_object_key(buf, "flashVer"));
        chk!(amf0::write_string(buf, flash_ver));
    }

    chk!(amf0::write_object_key(buf, "audioCodecs"));
    chk!(amf0::write_number(buf, audio_codecs as f64));
    chk!(amf0::write_object_key(buf, "videoCodecs"));
    chk!(amf0::write_number(buf, video_codecs as f64));
    if let Some(caps) = caps {
        connect_amf::write_negotiated_caps(buf, caps)?;
    }
    chk!(amf0::write_object_end(buf));

    Ok(())
}

/// Build a NetConnection `_error` response.
pub fn build_error(
    buf: &mut Buffer,
    transaction_id: f64,
    code: &str,
    description: &str,
) -> Result<()> {
    amf0::write_string(buf, "_error")?;
    amf0::write_number(buf, transaction_id)?;
    amf0::write_null(buf)?;
    amf0::write_object_begin(buf)?;
    amf0::write_object_key(buf, "level")?;
    amf0::write_string(buf, "error")?;
    amf0::write_object_key(buf, "code")?;
    amf0::write_string(buf, code)?;
    amf0::write_object_key(buf, "description")?;
    amf0::write_string(buf, description)?;
    amf0::write_object_end(buf)?;
    Ok(())
}

/// Build a "releaseStream" command.
pub fn build_release_stream(buf: &mut Buffer, stream_name: &str) -> Result<()> {
    amf0::write_string(buf, "releaseStream")?;
    amf0::write_number(buf, 2.0)?;
    amf0::write_null(buf)?;
    amf0::write_string(buf, stream_name)?;
    Ok(())
}

/// Build a "createStream" command.
pub fn build_create_stream(buf: &mut Buffer, transaction_id: f64) -> Result<()> {
    amf0::write_string(buf, "createStream")?;
    amf0::write_number(buf, transaction_id)?;
    amf0::write_null(buf)?;
    Ok(())
}

/// Build a "publish" command.
pub fn build_publish(buf: &mut Buffer, stream_name: &str, publish_type: &str) -> Result<()> {
    amf0::write_string(buf, "publish")?;
    amf0::write_number(buf, 0.0)?;
    amf0::write_null(buf)?;
    amf0::write_string(buf, stream_name)?;
    if !publish_type.is_empty() {
        amf0::write_string(buf, publish_type)?;
    }
    Ok(())
}

/// Build a "play" command.
pub fn build_play(buf: &mut Buffer, stream_name: &str) -> Result<()> {
    amf0::write_string(buf, "play")?;
    amf0::write_number(buf, 0.0)?;
    amf0::write_null(buf)?;
    amf0::write_string(buf, stream_name)?;
    Ok(())
}

/// Build a "FCPublish" command.
pub fn build_fcpublish(buf: &mut Buffer, stream_name: &str) -> Result<()> {
    amf0::write_string(buf, "FCPublish")?;
    amf0::write_number(buf, 0.0)?;
    amf0::write_null(buf)?;
    amf0::write_string(buf, stream_name)?;
    Ok(())
}

/// Build a "FCUnpublish" command.
pub fn build_fcunpublish(buf: &mut Buffer, stream_name: &str) -> Result<()> {
    amf0::write_string(buf, "FCUnpublish")?;
    amf0::write_number(buf, 0.0)?;
    amf0::write_null(buf)?;
    amf0::write_string(buf, stream_name)?;
    Ok(())
}

/// Build a "deleteStream" command.
pub fn build_deletestream(buf: &mut Buffer, transaction_id: f64, stream_id: u32) -> Result<()> {
    amf0::write_string(buf, "deleteStream")?;
    amf0::write_number(buf, transaction_id)?;
    amf0::write_null(buf)?;
    amf0::write_number(buf, stream_id as f64)?;
    Ok(())
}

/// Build a createStream _result response.
pub fn build_create_stream_result(
    buf: &mut Buffer,
    transaction_id: f64,
    stream_id: f64,
) -> Result<()> {
    amf0::write_string(buf, "_result")?;
    amf0::write_number(buf, transaction_id)?;
    amf0::write_null(buf)?;
    amf0::write_number(buf, stream_id)?;
    Ok(())
}

/// Build an onStatus command.
pub fn build_onstatus(buf: &mut Buffer, level: &str, code: &str, description: &str) -> Result<()> {
    amf0::write_string(buf, "onStatus")?;
    amf0::write_number(buf, 0.0)?;
    amf0::write_null(buf)?;
    amf0::write_object_begin(buf)?;
    amf0::write_object_key(buf, "level")?;
    amf0::write_string(buf, level)?;
    amf0::write_object_key(buf, "code")?;
    amf0::write_string(buf, code)?;
    amf0::write_object_key(buf, "description")?;
    amf0::write_string(buf, description)?;
    amf0::write_object_end(buf)?;
    Ok(())
}

/// `NetConnection.Connect.ReconnectRequest` status code, per the E-RTMP v2
/// reconnect mechanism: a server-initiated `onStatus` telling the client to
/// establish a new connection (optionally to `tcUrl`) at the next media
/// boundary, then disconnect from this one.
pub const RECONNECT_REQUEST_CODE: &str = "NetConnection.Connect.ReconnectRequest";

/// Build a `NetConnection.Connect.ReconnectRequest` onStatus command.
/// `tc_url` is omitted when `None`, telling the client to reuse its current
/// connection URL (per spec).
pub fn build_reconnect_request(
    buf: &mut Buffer,
    tc_url: Option<&str>,
    description: Option<&str>,
) -> Result<()> {
    amf0::write_string(buf, "onStatus")?;
    amf0::write_number(buf, 0.0)?;
    amf0::write_null(buf)?;
    amf0::write_object_begin(buf)?;
    amf0::write_object_key(buf, "level")?;
    amf0::write_string(buf, "status")?;
    amf0::write_object_key(buf, "code")?;
    amf0::write_string(buf, RECONNECT_REQUEST_CODE)?;
    amf0::write_object_key(buf, "description")?;
    amf0::write_string(buf, description.unwrap_or("Reconnect requested"))?;
    if let Some(url) = tc_url {
        amf0::write_object_key(buf, "tcUrl")?;
        amf0::write_string(buf, url)?;
    }
    amf0::write_object_end(buf)?;
    Ok(())
}

/// A parsed `NetConnection.Connect.ReconnectRequest` onStatus event.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReconnectRequest {
    /// New URL to reconnect to. `None` means reuse the current connection URL.
    pub tc_url: Option<String>,
    pub description: Option<String>,
}

/// Read an `onStatus` command and return `Some(ReconnectRequest)` when its
/// code is [`RECONNECT_REQUEST_CODE`], `None` for any other onStatus code
/// (including other status/error events the caller does not care about).
/// Returns `Err` only for a malformed command.
pub fn read_reconnect_request(buf: &mut Buffer) -> Result<Option<ReconnectRequest>> {
    let mut name = [0u8; 64];
    let name_len = amf0::read_string(buf, &mut name)?;
    if &name[..name_len] != b"onStatus" {
        return Err(ErrorCode::Amf);
    }
    read_number_value(buf)?; // transaction id
    amf0::skip_value(buf)?; // command object (always null)

    amf0::read_object_begin(buf)?;
    let mut code = [0u8; 128];
    let mut code_len = 0usize;
    let mut description = [0u8; 256];
    let mut description_len = 0usize;
    let mut tc_url = [0u8; 512];
    let mut tc_url_len: Option<usize> = None;
    let mut keys = 0usize;
    while !amf0::is_object_end(buf) {
        keys += 1;
        if keys > amf0::MAX_OBJECT_KEYS {
            return Err(ErrorCode::Amf);
        }
        let mut key = [0u8; 256];
        let key_len = amf0::read_object_key(buf, &mut key)?;
        let key_str = std::str::from_utf8(&key[..key_len]).unwrap_or("");
        match key_str {
            "code" => code_len = amf0::read_string(buf, &mut code)?,
            "description" => description_len = amf0::read_string(buf, &mut description)?,
            "tcUrl" => tc_url_len = Some(amf0::read_string(buf, &mut tc_url)?),
            _ => amf0::skip_value(buf)?,
        }
    }
    let mut end = [0u8; 3];
    buf.read(&mut end).map_err(|_| ErrorCode::Amf)?;

    if &code[..code_len] != RECONNECT_REQUEST_CODE.as_bytes() {
        return Ok(None);
    }

    Ok(Some(ReconnectRequest {
        tc_url: tc_url_len.map(|len| String::from_utf8_lossy(&tc_url[..len]).into_owned()),
        description: if description_len > 0 {
            Some(String::from_utf8_lossy(&description[..description_len]).into_owned())
        } else {
            None
        },
    }))
}

/* ── Decoder ── */

/// Peek at the command name without consuming it.
pub fn peek_name(buf: &mut Buffer, out: &mut [u8]) -> Result<usize> {
    let saved_pos = buf.read_pos();
    let result = amf0::read_string(buf, out);
    buf.set_read_pos(saved_pos);
    result
}

/// Read a connect command.
pub fn read_connect(buf: &mut Buffer, info: &mut ConnectInfo) -> Result<()> {
    // Read command name
    let mut name = [0u8; 64];
    let name_len = amf0::read_string(buf, &mut name)?;
    info.name[..name_len].copy_from_slice(&name[..name_len]);

    // Read transaction ID
    info.transaction_id = read_number_value(buf)?;

    // Read command object
    amf0::read_object_begin(buf)?;

    // Parse key-value pairs
    let mut keys = 0;
    while !amf0::is_object_end(buf) {
        if keys >= MAX_CONNECT_OBJECT_KEYS {
            return Err(ErrorCode::Amf);
        }
        keys += 1;

        let mut key = [0u8; 256];
        let key_len = amf0::read_object_key(buf, &mut key)?;
        let key_str = std::str::from_utf8(&key[..key_len]).unwrap_or("");

        if key_str == "fourCcList" {
            let type_pos = buf.read_pos();
            if connect_amf::read_four_cc_list_amf(buf, &mut info.four_cc_list).is_ok() {
                info.has_four_cc_list = true;
            } else {
                buf.set_read_pos(type_pos);
                amf0::skip_value(buf)?;
            }
            continue;
        }
        if key_str == "capsEx" {
            let type_pos = buf.read_pos();
            if connect_amf::read_caps_ex_amf(buf, &mut info.caps_ex, &mut info.caps_ex_mask).is_ok()
            {
                info.has_caps_ex = true;
            } else {
                buf.set_read_pos(type_pos);
                amf0::skip_value(buf)?;
            }
            continue;
        }
        if key_str == "videoFourCcInfoMap" {
            let type_pos = buf.read_pos();
            if connect_amf::read_video_fourcc_info_map_amf(buf, &mut info.video_four_cc_info_map)
                .is_ok()
            {
                info.has_video_four_cc_info_map = true;
            } else {
                buf.set_read_pos(type_pos);
                amf0::skip_value(buf)?;
            }
            continue;
        }
        if key_str == "reconnect" {
            let type_pos = buf.read_pos();
            if connect_amf::read_reconnect_amf(buf, &mut info.reconnect).is_ok() {
                info.has_reconnect = true;
            } else {
                buf.set_read_pos(type_pos);
                amf0::skip_value(buf)?;
            }
            continue;
        }

        // Peek value type
        let type_pos = buf.read_pos();
        let value_type = amf0::read_type(buf)?;
        buf.set_read_pos(type_pos); // restore

        match value_type {
            amf0::Amf0Type::String => match key_str {
                "app" => read_string_checked(buf, &mut info.app)?,
                "tcUrl" => read_string_checked(buf, &mut info.tc_url)?,
                "pageUrl" => read_string_checked(buf, &mut info.page_url)?,
                "swfUrl" => read_string_checked(buf, &mut info.swf_url)?,
                "flashVer" => read_string_checked(buf, &mut info.flash_ver)?,
                _ => {
                    amf0::skip_value(buf)?;
                }
            },
            amf0::Amf0Type::Number => {
                let value = read_number_value(buf)?;
                match key_str {
                    "audioCodecs" => info.audio_codecs = value as i32,
                    "videoCodecs" => info.video_codecs = value as i32,
                    _ => {}
                }
            }
            _ => {
                amf0::skip_value(buf)?;
            }
        }
    }

    // Consume object end marker
    let mut end = [0u8; 3];
    buf.read(&mut end).map_err(|_| ErrorCode::Amf)?;

    Ok(())
}

/// Read a createStream command.
pub fn read_create_stream(buf: &mut Buffer) -> Result<f64> {
    let mut name = [0u8; 64];
    amf0::read_string(buf, &mut name)?;
    let txn = read_number_value(buf)?;
    amf0::skip_value(buf)?;
    Ok(txn)
}

/// Read a publish command.
pub fn read_publish(
    buf: &mut Buffer,
    stream_name: &mut [u8],
    publish_type: &mut [u8],
) -> Result<()> {
    let mut name = [0u8; 64];
    amf0::read_string(buf, &mut name)?;
    read_number_value(buf)?; // skip txn
    amf0::skip_value(buf)?;
    read_string_checked(buf, stream_name)?;

    // The publish type argument is optional in practice. Decode it only when a
    // client actually sent more AMF data; otherwise keep the output buffer empty.
    if buf.available() > 0 {
        let _ = read_string_checked(buf, publish_type);
    }
    Ok(())
}

/// Read a play command.
pub fn read_play(buf: &mut Buffer, stream_name: &mut [u8]) -> Result<()> {
    let mut name = [0u8; 64];
    amf0::read_string(buf, &mut name)?;
    read_number_value(buf)?; // skip txn
    amf0::skip_value(buf)?;
    read_string_checked(buf, stream_name)?;
    Ok(())
}

/// Read a `releaseStream` command. Returns the target stream name.
pub fn read_release_stream(buf: &mut Buffer, stream_name: &mut [u8]) -> Result<()> {
    let mut name = [0u8; 64];
    amf0::read_string(buf, &mut name)?;
    read_number_value(buf)?; // skip txn
    amf0::skip_value(buf)?;
    read_string_checked(buf, stream_name)?;
    Ok(())
}

/// Read a pause command. Returns the pause flag (true = pause, false = unpause).
pub fn read_pause(buf: &mut Buffer) -> Result<bool> {
    let mut name = [0u8; 64];
    amf0::read_string(buf, &mut name)?;
    read_number_value(buf)?;
    amf0::skip_value(buf)?;
    read_bool_value(buf)
}

/// Read a seek command. Returns the target time in milliseconds.
pub fn read_seek(buf: &mut Buffer) -> Result<f64> {
    let mut name = [0u8; 64];
    amf0::read_string(buf, &mut name)?;
    read_number_value(buf)?;
    amf0::skip_value(buf)?;
    read_number_value(buf)
}

/// Read receiveAudio / receiveVideo: the boolean enable flag.
pub fn read_bool_command(buf: &mut Buffer) -> Result<bool> {
    let mut name = [0u8; 64];
    amf0::read_string(buf, &mut name)?;
    read_number_value(buf)?;
    amf0::skip_value(buf)?;
    read_bool_value(buf)
}

/// Read a closeStream command. Returns `None` when the peer sends the usual
/// three-argument form (stream id comes from the RTMP message stream id).
pub fn read_close_stream(buf: &mut Buffer) -> Result<Option<u32>> {
    let mut name = [0u8; 64];
    amf0::read_string(buf, &mut name)?;
    read_number_value(buf)?;
    amf0::skip_value(buf)?;
    if buf.available() == 0 {
        return Ok(None);
    }
    let type_pos = buf.read_pos();
    let ty = amf0::read_type(buf)?;
    if ty == amf0::Amf0Type::Number {
        Ok(Some(amf0::read_number(buf)? as u32))
    } else {
        buf.set_read_pos(type_pos);
        Ok(None)
    }
}

/// Build a pause command.
pub fn build_pause(buf: &mut Buffer, pause: bool) -> Result<()> {
    amf0::write_string(buf, "pause")?;
    amf0::write_number(buf, 0.0)?;
    amf0::write_null(buf)?;
    amf0::write_boolean(buf, pause)?;
    Ok(())
}

/// Build a seek command.
pub fn build_seek(buf: &mut Buffer, millis: f64) -> Result<()> {
    amf0::write_string(buf, "seek")?;
    amf0::write_number(buf, 0.0)?;
    amf0::write_null(buf)?;
    amf0::write_number(buf, millis)?;
    Ok(())
}

/// Read a connect _result response.
pub fn read_connect_result(buf: &mut Buffer) -> Result<f64> {
    read_connect_result_with_caps(buf, None)
}

/// Read a connect `_result` and optionally capture negotiated E-RTMP caps.
pub fn read_connect_result_with_caps(
    buf: &mut Buffer,
    caps: Option<&mut NegotiatedCaps>,
) -> Result<f64> {
    let mut name = [0u8; 64];
    amf0::read_string(buf, &mut name)?;
    let txn = read_number_value(buf)?;
    amf0::skip_value(buf)?;
    if let Some(caps) = caps {
        parse_connect_result_caps(buf, caps)?;
    } else {
        amf0::skip_value(buf)?;
    }
    Ok(txn)
}

fn parse_connect_result_caps(buf: &mut Buffer, caps: &mut NegotiatedCaps) -> Result<()> {
    let type_pos = buf.read_pos();
    let ty = amf0::read_type(buf)?;
    if ty != amf0::Amf0Type::Object {
        buf.set_read_pos(type_pos);
        amf0::skip_value(buf)?;
        return Ok(());
    }
    let mut keys = 0usize;
    while !amf0::is_object_end(buf) {
        keys += 1;
        if keys > amf0::MAX_OBJECT_KEYS {
            return Err(ErrorCode::Amf);
        }
        let mut key = [0u8; 256];
        let key_len = amf0::read_object_key(buf, &mut key)?;
        let key_str = std::str::from_utf8(&key[..key_len]).unwrap_or("");
        let type_pos = buf.read_pos();
        match key_str {
            "fourCcList" => {
                if connect_amf::read_four_cc_list_amf(buf, &mut caps.four_cc_list).is_ok() {
                    caps.has_four_cc_list = true;
                } else {
                    buf.set_read_pos(type_pos);
                    amf0::skip_value(buf)?;
                }
            }
            "capsEx" => {
                if connect_amf::read_caps_ex_amf(buf, &mut caps.caps_ex, &mut caps.caps_ex_mask)
                    .is_ok()
                {
                    caps.has_caps_ex = true;
                    caps.multitrack_enabled = (caps.caps_ex_mask & CAPS_EX_MASK_MULTITRACK) != 0;
                } else {
                    buf.set_read_pos(type_pos);
                    amf0::skip_value(buf)?;
                }
            }
            "videoFourCcInfoMap" => {
                if connect_amf::read_video_fourcc_info_map_amf(
                    buf,
                    &mut caps.video_four_cc_info_map,
                )
                .is_ok()
                {
                    caps.has_video_four_cc_info_map = true;
                } else {
                    buf.set_read_pos(type_pos);
                    amf0::skip_value(buf)?;
                }
            }
            "reconnect" => {
                if connect_amf::read_reconnect_amf(buf, &mut caps.reconnect).is_ok() {
                    caps.has_reconnect = true;
                } else {
                    buf.set_read_pos(type_pos);
                    amf0::skip_value(buf)?;
                }
            }
            _ => {
                amf0::skip_value(buf)?;
            }
        }
    }
    let mut end = [0u8; 3];
    buf.read(&mut end).map_err(|_| ErrorCode::Amf)?;
    Ok(())
}

/// Read a createStream _result response.
pub fn read_create_stream_result(buf: &mut Buffer) -> Result<(f64, f64)> {
    let mut name = [0u8; 64];
    amf0::read_string(buf, &mut name)?;
    let txn = read_number_value(buf)?;
    amf0::skip_value(buf)?;
    let stream_id = read_number_value(buf)?;
    Ok((txn, stream_id))
}

/// Read an onStatus command. Returns [`ErrorCode::Auth`] when `level` is not
/// `status` (e.g. `error`/`warning`). When `level` is `status`, returns
/// `Ok(true)` if `code` matches `expected_code` (e.g. `NetStream.Publish.Start`
/// after publish) and `Ok(false)` for any other status-level code.
///
/// A real server commonly sends a transitional status (e.g.
/// `NetStream.Play.Reset`) before the terminal one -- callers must keep
/// waiting for further `onStatus` messages on `Ok(false)` rather than
/// treating it as failure.
pub fn read_onstatus(buf: &mut Buffer, expected_code: &str) -> Result<bool> {
    let mut name = [0u8; 64];
    amf0::read_string(buf, &mut name)?;
    read_number_value(buf)?;
    amf0::skip_value(buf)?;

    amf0::read_object_begin(buf)?;
    let mut level = [0u8; 32];
    let mut level_len = 0usize;
    let mut code = [0u8; 128];
    let mut code_len = 0usize;
    let mut keys = 0usize;
    while !amf0::is_object_end(buf) {
        keys += 1;
        if keys > amf0::MAX_OBJECT_KEYS {
            return Err(ErrorCode::Amf);
        }
        let mut key = [0u8; 256];
        let key_len = amf0::read_object_key(buf, &mut key)?;
        let key_str = std::str::from_utf8(&key[..key_len]).unwrap_or("");
        if key_str == "level" {
            level_len = amf0::read_string(buf, &mut level)?;
        } else if key_str == "code" {
            code_len = amf0::read_string(buf, &mut code)?;
        } else {
            amf0::skip_value(buf)?;
        }
    }

    let mut end = [0u8; 3];
    buf.read(&mut end).map_err(|_| ErrorCode::Amf)?;

    let level_str = std::str::from_utf8(&level[..level_len]).unwrap_or("");
    if level_str != "status" {
        return Err(ErrorCode::Auth);
    }
    let code_str = std::str::from_utf8(&code[..code_len]).unwrap_or("");
    Ok(code_str == expected_code)
}

/* ── Helpers ── */

fn read_number_value(buf: &mut Buffer) -> Result<f64> {
    let ty = amf0::read_type(buf)?;
    if ty != amf0::Amf0Type::Number {
        return Err(ErrorCode::Amf);
    }
    amf0::read_number(buf)
}

fn read_bool_value(buf: &mut Buffer) -> Result<bool> {
    let ty = amf0::read_type(buf)?;
    match ty {
        amf0::Amf0Type::Boolean => amf0::read_boolean(buf),
        amf0::Amf0Type::Number => Ok(amf0::read_number(buf)? != 0.0),
        _ => Err(ErrorCode::Amf),
    }
}

fn read_string_checked(buf: &mut Buffer, out: &mut [u8]) -> Result<()> {
    let mut byte = [0u8; 1];
    buf.read(&mut byte).map_err(|_| ErrorCode::Amf)?;
    if byte[0] != amf0::Amf0Type::String as u8 {
        return Err(ErrorCode::Amf);
    }
    let mut lb = [0u8; 2];
    buf.read(&mut lb).map_err(|_| ErrorCode::Amf)?;
    let slen = ((lb[0] as usize) << 8) | (lb[1] as usize);

    if buf.available() < slen {
        return Err(ErrorCode::Io);
    }

    if out.is_empty() {
        buf.drain(slen);
        return Ok(());
    }

    // Routing keys (app, stream name) and connect metadata must be stored
    // losslessly. Silently truncating long AMF strings lets two distinct peer
    // values collide on the same relay namespace.
    if slen >= out.len() {
        buf.drain(slen);
        return Err(ErrorCode::Amf);
    }

    buf.read(&mut out[..slen]).map_err(|_| ErrorCode::Amf)?;

    // The NUL byte at `out[slen]` is used as a sentinel by callers (and by
    // `decode_route_amf_string`) to find the end of the value. An embedded
    // NUL within the string content itself would let that sentinel scan
    // stop early, silently truncating the routing key and colliding two
    // distinct wire values (e.g. "live\0\xff..." and "live") onto the same
    // relay namespace. Reject embedded NULs outright.
    if out[..slen].contains(&0) {
        return Err(ErrorCode::Amf);
    }

    out[slen] = 0;
    Ok(())
}

/// Decode a NUL-terminated AMF string buffer into a relay routing key.
///
/// Routing keys must be lossless: invalid UTF-8 must not silently collapse
/// distinct wire values to the same empty namespace.
pub(crate) fn decode_route_amf_string(out: &[u8]) -> Result<String> {
    let len = out.iter().position(|&b| b == 0).unwrap_or(out.len());
    let value = std::str::from_utf8(&out[..len]).map_err(|_| ErrorCode::Amf)?;
    Ok(value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cstr(buf: &[u8]) -> &str {
        let len = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
        std::str::from_utf8(&buf[..len]).unwrap()
    }

    #[test]
    fn read_connect_parses_four_cc_list_strict_array() {
        let mut buf = Buffer::new();
        amf0::write_string(&mut buf, "connect").unwrap();
        amf0::write_number(&mut buf, 1.0).unwrap();
        amf0::write_object_begin(&mut buf).unwrap();
        amf0::write_object_key(&mut buf, "fourCcList").unwrap();
        buf.write(&[0x0A, 0x00, 0x00, 0x00, 0x01]).unwrap();
        amf0::write_string(&mut buf, "av01").unwrap();
        amf0::write_object_key(&mut buf, "app").unwrap();
        amf0::write_string(&mut buf, "live").unwrap();
        amf0::write_object_end(&mut buf).unwrap();

        let mut info = ConnectInfo::default();
        read_connect(&mut buf, &mut info).unwrap();
        let app_len = info.app.iter().position(|&b| b == 0).unwrap_or(0);
        assert_eq!(std::str::from_utf8(&info.app[..app_len]).unwrap(), "live");
        assert!(info.has_four_cc_list);
        assert_eq!(info.four_cc_list.count, 1);
        assert_eq!(&info.four_cc_list.entries[0].cc[..4], b"av01");
    }

    #[test]
    fn read_string_checked_rejects_values_that_do_not_fit_output_buffer() {
        let mut buf = Buffer::new();
        let long = "x".repeat(256);
        amf0::write_string(&mut buf, &long).unwrap();

        let mut out = [0u8; 256];
        assert_eq!(read_string_checked(&mut buf, &mut out), Err(ErrorCode::Amf));
        assert_eq!(buf.available(), 0);
    }

    #[test]
    fn read_string_checked_accepts_values_up_to_buffer_capacity_minus_nul() {
        let mut buf = Buffer::new();
        let exact = "y".repeat(255);
        amf0::write_string(&mut buf, &exact).unwrap();

        let mut out = [0u8; 256];
        read_string_checked(&mut buf, &mut out).unwrap();
        assert_eq!(cstr(&out), exact);
    }

    #[test]
    fn read_publish_rejects_stream_names_longer_than_routing_buffer() {
        let mut buf = Buffer::new();
        build_publish(&mut buf, &"z".repeat(256), "live").unwrap();

        let mut stream_name = [0u8; 256];
        let mut publish_type = [0u8; 64];
        assert_eq!(
            read_publish(&mut buf, &mut stream_name, &mut publish_type),
            Err(ErrorCode::Amf)
        );
    }

    #[test]
    fn read_string_checked_with_empty_output_buffer_does_not_panic() {
        let mut buf = Buffer::new();
        amf0::write_string(&mut buf, "hello").unwrap();

        let mut out: [u8; 0] = [];
        assert!(read_string_checked(&mut buf, &mut out).is_ok());
        // The string bytes must still be fully consumed from the buffer.
        assert_eq!(buf.available(), 0);
    }

    #[test]
    fn decode_route_amf_string_rejects_invalid_utf8() {
        let mut buf = [0u8; 8];
        buf[0] = 0xFF;
        buf[1] = 0xFE;
        assert_eq!(decode_route_amf_string(&buf), Err(ErrorCode::Amf));
    }

    #[test]
    fn decode_route_amf_string_accepts_valid_utf8() {
        let mut buf = [0u8; 16];
        buf[..5].copy_from_slice(b"live\0");
        assert_eq!(decode_route_amf_string(&buf).unwrap(), "live");
    }

    #[test]
    fn read_publish_preserves_invalid_utf8_stream_name_bytes() {
        let mut buf = Buffer::with_capacity(128);
        amf0::write_string(&mut buf, "publish").unwrap();
        amf0::write_number(&mut buf, 1.0).unwrap();
        amf0::write_null(&mut buf).unwrap();
        buf.write(&[amf0::Amf0Type::String as u8, 0x00, 0x02, 0x80, 0x81])
            .unwrap();
        amf0::write_string(&mut buf, "live").unwrap();

        let mut stream_name = [0u8; 256];
        let mut publish_type = [0u8; 64];
        read_publish(&mut buf, &mut stream_name, &mut publish_type).unwrap();
        assert_eq!(stream_name[0], 0x80);
        assert_eq!(stream_name[1], 0x81);
        assert_eq!(decode_route_amf_string(&stream_name), Err(ErrorCode::Amf));
    }

    #[test]
    fn read_publish_rejects_stream_names_with_embedded_nul() {
        // "live\0\xff" must be rejected outright, not silently truncated to
        // "live" by the NUL sentinel used to mark the end of the value.
        let mut buf = Buffer::with_capacity(128);
        amf0::write_string(&mut buf, "publish").unwrap();
        amf0::write_number(&mut buf, 1.0).unwrap();
        amf0::write_null(&mut buf).unwrap();
        buf.write(&[amf0::Amf0Type::String as u8, 0x00, 0x06])
            .unwrap();
        buf.write(b"live\0\xff").unwrap();
        amf0::write_string(&mut buf, "live").unwrap();

        let mut stream_name = [0u8; 256];
        let mut publish_type = [0u8; 64];
        assert_eq!(
            read_publish(&mut buf, &mut stream_name, &mut publish_type),
            Err(ErrorCode::Amf)
        );
    }

    #[test]
    fn publish_command_round_trips_publish_type() {
        let mut buf = Buffer::new();
        build_publish(&mut buf, "stream_key", "record").unwrap();

        let mut stream_name = [0u8; 256];
        let mut publish_type = [0u8; 64];
        read_publish(&mut buf, &mut stream_name, &mut publish_type).unwrap();

        assert_eq!(cstr(&stream_name), "stream_key");
        assert_eq!(cstr(&publish_type), "record");
    }

    #[test]
    fn read_onstatus_rejects_error_level() {
        let mut buf = Buffer::new();
        build_onstatus(
            &mut buf,
            "error",
            "NetStream.Publish.BadName",
            "Publish not authorized",
        )
        .unwrap();

        assert_eq!(
            read_onstatus(&mut buf, "NetStream.Publish.Start"),
            Err(ErrorCode::Auth)
        );
    }

    #[test]
    fn read_onstatus_accepts_status_level() {
        let mut buf = Buffer::new();
        build_onstatus(&mut buf, "status", "NetStream.Publish.Start", "Publishing").unwrap();

        assert_eq!(read_onstatus(&mut buf, "NetStream.Publish.Start"), Ok(true));
    }

    #[test]
    fn read_onstatus_reports_non_matching_status_code_as_not_matched() {
        // A transitional status-level code (e.g. what a real server sends as
        // `NetStream.Play.Reset` before `NetStream.Play.Start`) is not a
        // failure -- callers must keep waiting for the expected code rather
        // than aborting.
        let mut buf = Buffer::new();
        build_onstatus(
            &mut buf,
            "status",
            "NetStream.Publish.BadName",
            "Publish not authorized",
        )
        .unwrap();

        assert_eq!(
            read_onstatus(&mut buf, "NetStream.Publish.Start"),
            Ok(false)
        );
    }

    #[test]
    fn read_onstatus_rejects_missing_level() {
        let mut buf = Buffer::new();
        amf0::write_string(&mut buf, "onStatus").unwrap();
        amf0::write_number(&mut buf, 1.0).unwrap();
        amf0::write_null(&mut buf).unwrap();
        amf0::write_object_begin(&mut buf).unwrap();
        amf0::write_object_key(&mut buf, "code").unwrap();
        amf0::write_string(&mut buf, "NetStream.Publish.Start").unwrap();
        amf0::write_object_end(&mut buf).unwrap();

        assert_eq!(
            read_onstatus(&mut buf, "NetStream.Publish.Start"),
            Err(ErrorCode::Auth)
        );
    }

    #[test]
    fn read_onstatus_rejects_excessive_object_keys() {
        let mut buf = Buffer::new();
        amf0::write_string(&mut buf, "onStatus").unwrap();
        amf0::write_number(&mut buf, 1.0).unwrap();
        amf0::write_null(&mut buf).unwrap();
        amf0::write_object_begin(&mut buf).unwrap();
        for i in 0..=amf0::MAX_OBJECT_KEYS {
            let key = format!("k{i}");
            amf0::write_object_key(&mut buf, &key).unwrap();
            amf0::write_boolean(&mut buf, false).unwrap();
        }
        amf0::write_object_key(&mut buf, "level").unwrap();
        amf0::write_string(&mut buf, "status").unwrap();
        amf0::write_object_end(&mut buf).unwrap();

        assert_eq!(
            read_onstatus(&mut buf, "NetStream.Publish.Start"),
            Err(ErrorCode::Amf)
        );
    }

    #[test]
    fn reconnect_request_round_trips_with_tc_url_and_description() {
        let mut buf = Buffer::new();
        build_reconnect_request(&mut buf, Some("rtmp://backup/live"), Some("moving you")).unwrap();

        let req = read_reconnect_request(&mut buf).unwrap().unwrap();
        assert_eq!(req.tc_url.as_deref(), Some("rtmp://backup/live"));
        assert_eq!(req.description.as_deref(), Some("moving you"));
    }

    #[test]
    fn reconnect_request_without_tc_url_reuses_current_connection() {
        let mut buf = Buffer::new();
        build_reconnect_request(&mut buf, None, None).unwrap();

        let req = read_reconnect_request(&mut buf).unwrap().unwrap();
        assert_eq!(req.tc_url, None);
        assert!(req.description.is_some());
    }

    #[test]
    fn read_reconnect_request_ignores_unrelated_onstatus_codes() {
        let mut buf = Buffer::new();
        build_onstatus(&mut buf, "status", "NetStream.Publish.Start", "Publishing").unwrap();

        assert_eq!(read_reconnect_request(&mut buf), Ok(None));
    }

    #[test]
    fn read_reconnect_request_rejects_non_onstatus_commands() {
        let mut buf = Buffer::new();
        build_onstatus(&mut buf, "status", RECONNECT_REQUEST_CODE, "reconnect").unwrap();
        // Overwrite the command name's first byte so it no longer reads "onStatus".
        let mut bytes = buf.as_slice().to_vec();
        bytes[3] = b'X';
        let mut tampered = Buffer::from_slice(&bytes);

        assert_eq!(read_reconnect_request(&mut tampered), Err(ErrorCode::Amf));
    }

    #[test]
    fn read_close_stream_accepts_three_argument_form() {
        let mut buf = Buffer::new();
        amf0::write_string(&mut buf, "closeStream").unwrap();
        amf0::write_number(&mut buf, 2.0).unwrap();
        amf0::write_null(&mut buf).unwrap();

        assert_eq!(read_close_stream(&mut buf).unwrap(), None);
    }

    #[test]
    fn read_close_stream_accepts_explicit_stream_id() {
        let mut buf = Buffer::new();
        amf0::write_string(&mut buf, "closeStream").unwrap();
        amf0::write_number(&mut buf, 2.0).unwrap();
        amf0::write_null(&mut buf).unwrap();
        amf0::write_number(&mut buf, 7.0).unwrap();

        assert_eq!(read_close_stream(&mut buf).unwrap(), Some(7));
    }
}
