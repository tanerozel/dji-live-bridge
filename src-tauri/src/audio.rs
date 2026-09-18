use std::process::Command;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{BridgeError, BridgeResult};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioInputDevice {
    pub name: String,
    pub manufacturer: Option<String>,
    pub transport: Option<String>,
}

pub fn discover_input_devices() -> BridgeResult<Vec<AudioInputDevice>> {
    let output = Command::new("/usr/sbin/system_profiler")
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

fn nonzero_value(value: &Value) -> bool {
    match value {
        Value::Bool(value) => *value,
        Value::Number(value) => value.as_u64().unwrap_or_default() > 0,
        Value::String(value) => !matches!(value.trim(), "" | "0" | "No" | "no"),
        _ => false,
    }
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
