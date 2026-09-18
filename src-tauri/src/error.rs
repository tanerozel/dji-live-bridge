use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum BridgeError {
    #[error("Configuration error: {0}")]
    Config(String),
    #[error("Network discovery failed: {0}")]
    Network(String),
    #[error("Process error: {0}")]
    Process(String),
    #[error("MediaMTX error: {0}")]
    MediaMtx(String),
    #[error("FFmpeg error: {0}")]
    Ffmpeg(String),
    #[error("OBS error: {0}")]
    Obs(String),
    #[error("Virtual camera error: {0}")]
    VirtualCamera(String),
    #[error("Invalid workflow transition: {0}")]
    InvalidTransition(String),
    #[error("Unsupported automation: {0}")]
    UnsupportedAutomation(String),
    #[error("Validation error: {0}")]
    Validation(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorPayload {
    pub code: &'static str,
    pub message: String,
    pub action: String,
}

impl From<BridgeError> for ErrorPayload {
    fn from(value: BridgeError) -> Self {
        let (code, action) = match &value {
            BridgeError::Config(_) => ("CONFIG", "Review the saved application settings."),
            BridgeError::Network(_) => (
                "NETWORK",
                "Select an active LAN interface and verify Local Network permission.",
            ),
            BridgeError::Process(_) => (
                "PROCESS",
                "Open Diagnostics and inspect the supervised process status.",
            ),
            BridgeError::MediaMtx(_) => (
                "MEDIAMTX",
                "Verify port availability and reinstall the pinned MediaMTX sidecar.",
            ),
            BridgeError::Ffmpeg(_) => (
                "FFMPEG",
                "Install a supported FFmpeg build and run Diagnostics again.",
            ),
            BridgeError::Obs(_) => (
                "OBS",
                "Start OBS, enable obs-websocket, and verify host, port, and password.",
            ),
            BridgeError::VirtualCamera(_) => (
                "VIRTUAL_CAMERA",
                "Install the app in /Applications, enable the camera in System Settings, then retry.",
            ),
            BridgeError::InvalidTransition(_) => (
                "INVALID_TRANSITION",
                "Return to the previous workflow step and retry.",
            ),
            BridgeError::UnsupportedAutomation(_) => (
                "UNSUPPORTED_AUTOMATION",
                "Complete this step manually in the destination application.",
            ),
            BridgeError::Validation(_) => {
                ("VALIDATION", "Correct the highlighted value and retry.")
            }
            BridgeError::Io(_) => ("IO", "Check file permissions and available disk space."),
            BridgeError::Http(_) => (
                "HTTP",
                "Verify the local service is running and its loopback port is available.",
            ),
            BridgeError::Serialization(_) => (
                "SERIALIZATION",
                "Reset the affected local setting and retry.",
            ),
        };

        Self {
            code,
            message: redact_secrets(&value.to_string()),
            action: action.to_string(),
        }
    }
}

pub type BridgeResult<T> = Result<T, BridgeError>;

pub fn redact_secrets(input: &str) -> String {
    let mut output = input.to_string();
    for marker in ["stream_key=", "key=", "password=", "pass="] {
        let mut search_from = 0;
        while let Some(relative) = output[search_from..].to_ascii_lowercase().find(marker) {
            let start = search_from + relative + marker.len();
            let end = output[start..]
                .find(|character: char| character.is_whitespace() || character == '&')
                .map_or(output.len(), |offset| start + offset);
            output.replace_range(start..end, "[REDACTED]");
            search_from = start + "[REDACTED]".len();
        }
    }
    for scheme in ["rtmp://", "rtmps://"] {
        let mut search_from = 0;
        while let Some(relative) = output[search_from..].find(scheme) {
            let url_start = search_from + relative;
            let url_end = output[url_start..]
                .find(|character: char| {
                    character.is_whitespace()
                        || matches!(character, '\'' | '"' | '\\' | ')' | ']' | '}')
                })
                .map_or(output.len(), |offset| url_start + offset);
            let Some(fragment_offset) = output[url_start..url_end].find('#') else {
                search_from = url_end;
                continue;
            };
            let secret_start = url_start + fragment_offset + 1;
            output.replace_range(secret_start..url_end, "[REDACTED]");
            search_from = secret_start + "[REDACTED]".len();
        }
    }
    output
}
