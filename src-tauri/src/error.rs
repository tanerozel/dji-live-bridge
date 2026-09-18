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
    pub message_key: &'static str,
    pub detail: String,
    pub action_key: &'static str,
}

impl From<BridgeError> for ErrorPayload {
    fn from(value: BridgeError) -> Self {
        let (code, message_key, action_key) = match &value {
            BridgeError::Config(_) => ("CONFIG", "errors.message.config", "errors.action.config"),
            BridgeError::Network(_) => (
                "NETWORK",
                "errors.message.network",
                "errors.action.network",
            ),
            BridgeError::Process(_) => (
                "PROCESS",
                "errors.message.process",
                "errors.action.process",
            ),
            BridgeError::MediaMtx(_) => (
                "MEDIAMTX",
                "errors.message.mediamtx",
                "errors.action.mediamtx",
            ),
            BridgeError::Ffmpeg(_) => (
                "FFMPEG",
                "errors.message.ffmpeg",
                "errors.action.ffmpeg",
            ),
            BridgeError::Obs(_) => (
                "OBS",
                "errors.message.obs",
                "errors.action.obs",
            ),
            BridgeError::VirtualCamera(_) => (
                "VIRTUAL_CAMERA",
                "errors.message.virtualCamera",
                "errors.action.virtualCamera",
            ),
            BridgeError::InvalidTransition(_) => (
                "INVALID_TRANSITION",
                "errors.message.transition",
                "errors.action.transition",
            ),
            BridgeError::UnsupportedAutomation(_) => (
                "UNSUPPORTED_AUTOMATION",
                "errors.message.unsupported",
                "errors.action.unsupported",
            ),
            BridgeError::Validation(_) => ("VALIDATION", "errors.message.validation", "errors.action.validation"),
            BridgeError::Io(_) => ("IO", "errors.message.io", "errors.action.io"),
            BridgeError::Http(_) => (
                "HTTP",
                "errors.message.http",
                "errors.action.http",
            ),
            BridgeError::Serialization(_) => (
                "SERIALIZATION",
                "errors.message.serialization",
                "errors.action.serialization",
            ),
        };

        Self {
            code,
            message_key,
            detail: redact_secrets(&value.to_string()),
            action_key,
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
