use serde::{Deserialize, Serialize};
#[cfg(target_os = "macos")]
use serde_json::Value;

use crate::error::{BridgeError, BridgeResult};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioInputDevice {
    pub name: String,
    pub manufacturer: Option<String>,
    pub transport: Option<String>,
}

#[cfg(target_os = "macos")]
pub fn discover_input_devices() -> BridgeResult<Vec<AudioInputDevice>> {
    let output = std::process::Command::new("/usr/sbin/system_profiler")
        .args(["SPAudioDataType", "-json"])
        .output()?;
    if !output.status.success() {
        return Err(BridgeError::Process(
            "CoreAudio device discovery failed".into(),
        ));
    }
    let value: Value = serde_json::from_slice(&output.stdout)?;
    let mut devices = Vec::new();
    collect_inputs(&value, &mut devices);
    devices.sort_by(|left, right| left.name.cmp(&right.name));
    devices.dedup_by(|left, right| left.name == right.name);
    Ok(devices)
}

#[cfg(target_os = "macos")]
fn collect_inputs(value: &Value, devices: &mut Vec<AudioInputDevice>) {
    match value {
        Value::Array(values) => {
            for value in values {
                collect_inputs(value, devices);
            }
        }
        Value::Object(object) => {
            let is_input = object
                .get("coreaudio_device_input")
                .is_some_and(nonzero_value);
            if is_input && let Some(name) = object.get("_name").and_then(Value::as_str) {
                devices.push(AudioInputDevice {
                    name: name.to_string(),
                    manufacturer: object
                        .get("coreaudio_device_manufacturer")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    transport: object
                        .get("coreaudio_device_transport")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                });
            }
            for value in object.values() {
                collect_inputs(value, devices);
            }
        }
        _ => {}
    }
}

#[cfg(target_os = "macos")]
fn nonzero_value(value: &Value) -> bool {
    match value {
        Value::Bool(value) => *value,
        Value::Number(value) => value.as_u64().unwrap_or_default() > 0,
        Value::String(value) => !matches!(value.trim(), "" | "0" | "No" | "no"),
        _ => false,
    }
}

/// Windows has no `system_profiler`. The bundled FFmpeg already talks to
/// DirectShow, and its device listing is the authority here: the names it
/// prints are exactly the ones `audio=<name>` has to be given later.
#[cfg(target_os = "windows")]
pub fn discover_input_devices() -> BridgeResult<Vec<AudioInputDevice>> {
    let ffmpeg = crate::ffmpeg::locate("ffmpeg")
        .ok_or_else(|| BridgeError::Process("the bundled FFmpeg is missing".into()))?;
    // Listing devices is not a conversion, so FFmpeg reports the list on
    // stderr and then exits with a failure; only the output matters.
    let output = crate::console::hide_std(&mut std::process::Command::new(ffmpeg))
        .args([
            "-hide_banner",
            "-list_devices",
            "true",
            "-f",
            "dshow",
            "-i",
            "dummy",
        ])
        .output()?;

    let mut devices = parse_dshow_inputs(&String::from_utf8_lossy(&output.stderr));
    devices.sort_by(|left, right| left.name.cmp(&right.name));
    devices.dedup_by(|left, right| left.name == right.name);
    Ok(devices)
}

/// Picks the audio devices out of FFmpeg's listing, which names one device per
/// line as `"<name>" (audio)` and follows it with an `Alternative name` line.
#[cfg(target_os = "windows")]
fn parse_dshow_inputs(text: &str) -> Vec<AudioInputDevice> {
    text.lines()
        .filter_map(|line| {
            let quoted = line.trim_end().strip_suffix("(audio)")?.trim_end();
            let name = quoted[quoted.find('"')? + 1..].strip_suffix('"')?;
            (!name.is_empty()).then(|| AudioInputDevice {
                name: name.to_string(),
                manufacturer: None,
                transport: Some("DirectShow".into()),
            })
        })
        .collect()
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioRoutingNotice {
    pub virtual_camera_carries_audio: bool,
    pub physical_microphone_guidance: &'static str,
    pub virtual_audio_automation: &'static str,
}

pub fn routing_notice() -> AudioRoutingNotice {
    AudioRoutingNotice {
        virtual_camera_carries_audio: false,
        physical_microphone_guidance: "DJI Live Bridge Camera carries video only. Set the camera source audio capture to None, then select exactly one physical microphone in TikTok LIVE Studio.",
        virtual_audio_automation: "UnsupportedAutomation: no third-party virtual-audio driver is installed or configured automatically.",
    }
}

#[cfg(all(test, target_os = "windows"))]
mod tests {
    use super::parse_dshow_inputs;

    /// Real output from the bundled FFmpeg. Device names carry spaces,
    /// brackets and non-ASCII characters, and neither the video device nor the
    /// `Alternative name` lines may be mistaken for an input.
    const LISTING: &str = r#"[in#0 @ 0000024] "USB Video Aygıtı" (video)
[in#0 @ 0000024]   Alternative name "@device_pnp_\\?\usb#vid_174f"
[in#0 @ 0000024] "Mikrofon (Realtek(R) Audio)" (audio)
[in#0 @ 0000024]   Alternative name "@device_cm_{33D9A762}"
Error opening input file dummy.
"#;

    #[test]
    fn reads_only_the_audio_devices() {
        let devices = parse_dshow_inputs(LISTING);
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].name, "Mikrofon (Realtek(R) Audio)");
    }

    #[test]
    fn a_listing_without_devices_yields_nothing() {
        assert!(parse_dshow_inputs("Error opening input file dummy.\n").is_empty());
    }
}
