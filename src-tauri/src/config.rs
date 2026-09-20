use std::{fs, path::PathBuf};

use serde::{Deserialize, Serialize};
use url::Url;

use crate::error::{BridgeError, BridgeResult};

pub const APP_DIR_NAME: &str = "DJI Live Bridge";

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub enum OutputLayout {
    #[default]
    Landscape,
    Portrait,
}

impl OutputLayout {
    pub fn dimensions(self) -> (f64, f64) {
        match self {
            Self::Landscape => (1920.0, 1080.0),
            Self::Portrait => (1080.0, 1920.0),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub enum FitMode {
    #[default]
    Fit,
    Fill,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum DestinationMode {
    TikTokLiveStudio,
    TikTokRtmp,
    CustomRtmp,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum RtmpDestinationKind {
    TikTok,
    Instagram,
    YouTube,
    Facebook,
    #[default]
    Custom,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RtmpDestinationConfig {
    pub id: String,
    pub name: String,
    pub kind: RtmpDestinationKind,
    pub server: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RtmpDestinationInput {
    pub id: Option<String>,
    pub name: String,
    pub kind: RtmpDestinationKind,
    pub server: String,
    pub key: Option<String>,
    pub enabled: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProductionEngine {
    #[default]
    NativeFfmpeg,
    Obs,
}

#[derive(Debug, Clone)]
pub enum BroadcastDestination {
    TikTok { server: String, key: String },
    Instagram { server: String, key: String },
    YouTube { server: String, key: String },
    Facebook { server: String, key: String },
    Custom { server: String, key: String },
}

impl BroadcastDestination {
    pub fn rtmp_parts(&self) -> BridgeResult<(&str, &str)> {
        match self {
            Self::TikTok { server, key }
            | Self::Instagram { server, key }
            | Self::YouTube { server, key }
            | Self::Facebook { server, key }
            | Self::Custom { server, key } => {
                validate_rtmp_destination(server, key)?;
                Ok((server, key))
            }
        }
    }
}

impl BroadcastDestination {
    pub fn from_rtmp(destination: &RtmpDestinationConfig, key: String) -> Self {
        let server = destination.server.clone();
        match destination.kind {
            RtmpDestinationKind::TikTok => Self::TikTok { server, key },
            RtmpDestinationKind::Instagram => Self::Instagram { server, key },
            RtmpDestinationKind::YouTube => Self::YouTube { server, key },
            RtmpDestinationKind::Facebook => Self::Facebook { server, key },
            RtmpDestinationKind::Custom => Self::Custom { server, key },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AppConfig {
    pub selected_interface: Option<String>,
    pub obs_host: String,
    pub obs_port: u16,
    pub output_layout: OutputLayout,
    pub fit_mode: FitMode,
    pub rtmp_destinations: Vec<RtmpDestinationConfig>,
    #[serde(default, rename = "destinationMode", skip_serializing)]
    pub destination_mode: Option<DestinationMode>,
    #[serde(default, rename = "destinationServer", skip_serializing)]
    pub destination_server: Option<String>,
    pub production_engine: ProductionEngine,
    pub selected_microphone: Option<String>,
    pub microphone_muted: bool,
    pub microphone_volume_db: f64,
    pub microphone_sync_ms: u32,
    pub noise_suppression: bool,
    pub compressor: bool,
    pub limiter: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            selected_interface: None,
            obs_host: "127.0.0.1".into(),
            obs_port: 4455,
            output_layout: OutputLayout::Landscape,
            fit_mode: FitMode::Fit,
            rtmp_destinations: Vec::new(),
            destination_mode: None,
            destination_server: None,
            production_engine: ProductionEngine::NativeFfmpeg,
            selected_microphone: None,
            microphone_muted: false,
            microphone_volume_db: 0.0,
            microphone_sync_ms: 0,
            noise_suppression: true,
            compressor: true,
            limiter: true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ConfigStore {
    app_support_dir: PathBuf,
    logs_dir: PathBuf,
}

impl ConfigStore {
    pub fn discover() -> BridgeResult<Self> {
        let app_support_dir = dirs::config_dir()
            .ok_or_else(|| BridgeError::Config("Application Support directory not found".into()))?
            .join(APP_DIR_NAME);
        // macOS keeps logs in ~/Library/Logs; Windows has no such place, so they
        // live beside the rest of the app's local data.
        #[cfg(target_os = "macos")]
        let logs_root = dirs::home_dir()
            .ok_or_else(|| BridgeError::Config("Home directory not found".into()))?
            .join("Library/Logs");
        #[cfg(not(target_os = "macos"))]
        let logs_root = dirs::data_local_dir()
            .ok_or_else(|| BridgeError::Config("Local data directory not found".into()))?
            .join(APP_DIR_NAME);
        let logs_dir = logs_root.join(if cfg!(target_os = "macos") {
            APP_DIR_NAME
        } else {
            "logs"
        });
        fs::create_dir_all(&app_support_dir)?;
        fs::create_dir_all(&logs_dir)?;
        Ok(Self {
            app_support_dir,
            logs_dir,
        })
    }

    pub fn app_support_dir(&self) -> &PathBuf {
        &self.app_support_dir
    }

    pub fn logs_dir(&self) -> &PathBuf {
        &self.logs_dir
    }

    pub fn load(&self) -> BridgeResult<AppConfig> {
        let path = self.app_support_dir.join("config.json");
        if !path.exists() {
            return Ok(AppConfig::default());
        }
        let bytes = fs::read(path)?;
        Ok(serde_json::from_slice(&bytes)?)
    }

    pub fn save(&self, config: &AppConfig) -> BridgeResult<()> {
        if config.obs_host.trim().is_empty() || config.obs_port == 0 {
            return Err(BridgeError::Validation(
                "OBS host and port must be valid".into(),
            ));
        }
        if !(-60.0..=12.0).contains(&config.microphone_volume_db) {
            return Err(BridgeError::Validation(
                "Microphone volume must be between -60 dB and +12 dB".into(),
            ));
        }
        let mut ids = std::collections::HashSet::new();
        for destination in &config.rtmp_destinations {
            validate_rtmp_destination_config(destination)?;
            if !ids.insert(destination.id.as_str()) {
                return Err(BridgeError::Validation(
                    "RTMP destination IDs must be unique".into(),
                ));
            }
        }
        let temporary = self.app_support_dir.join("config.json.tmp");
        let final_path = self.app_support_dir.join("config.json");
        fs::write(&temporary, serde_json::to_vec_pretty(config)?)?;
        fs::rename(temporary, final_path)?;
        Ok(())
    }
}

pub fn validate_rtmp_destination_config(destination: &RtmpDestinationConfig) -> BridgeResult<()> {
    let name = destination.name.trim();
    if name.is_empty() || name.chars().count() > 64 {
        return Err(BridgeError::Validation(
            "Destination name must contain 1 to 64 characters".into(),
        ));
    }
    if destination.id.is_empty()
        || destination.id.len() > 96
        || !destination
            .id
            .chars()
            .all(|value| value.is_ascii_alphanumeric() || matches!(value, '-' | '_'))
    {
        return Err(BridgeError::Validation(
            "RTMP destination ID is invalid".into(),
        ));
    }
    let parsed = Url::parse(&destination.server)
        .map_err(|_| BridgeError::Validation("RTMP server URL is invalid".into()))?;
    if !matches!(parsed.scheme(), "rtmp" | "rtmps")
        || parsed.host_str().is_none()
        || parsed.username() != ""
        || parsed.password().is_some()
        || parsed.fragment().is_some()
    {
        return Err(BridgeError::Validation(
            "RTMP server must be an rtmp:// or rtmps:// URL without credentials or a key fragment"
                .into(),
        ));
    }
    Ok(())
}

pub fn rtmp_keychain_account(id: &str) -> String {
    format!("rtmp-stream-key:{id}")
}

pub fn validate_rtmp_destination(server: &str, key: &str) -> BridgeResult<()> {
    let parsed = Url::parse(server)
        .map_err(|_| BridgeError::Validation("RTMP server URL is invalid".into()))?;
    if !matches!(parsed.scheme(), "rtmp" | "rtmps") {
        return Err(BridgeError::Validation(
            "RTMP destination must use rtmp:// or rtmps://".into(),
        ));
    }
    if parsed.host_str().is_none() || parsed.username() != "" || parsed.password().is_some() {
        return Err(BridgeError::Validation(
            "RTMP destination requires a host and must not contain credentials".into(),
        ));
    }
    if parsed.fragment().is_some() {
        return Err(BridgeError::Validation(
            "RTMP server URL must not contain a stream-key fragment".into(),
        ));
    }
    if key.trim().is_empty()
        || key.contains(['\r', '\n', '#'])
        || key.chars().any(char::is_whitespace)
    {
        return Err(BridgeError::Validation("Stream key is invalid".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_destination_fields_are_read_but_not_written() {
        let mut value = serde_json::to_value(AppConfig::default()).unwrap();
        value["destinationMode"] = serde_json::json!("CustomRtmp");
        value["destinationServer"] = serde_json::json!("rtmps://example.test/live");
        let config: AppConfig = serde_json::from_value(value).unwrap();
        assert!(matches!(
            config.destination_mode,
            Some(DestinationMode::CustomRtmp)
        ));
        assert_eq!(
            config.destination_server.as_deref(),
            Some("rtmps://example.test/live")
        );

        let serialized = serde_json::to_value(config).unwrap();
        assert!(serialized.get("destinationMode").is_none());
        assert!(serialized.get("destinationServer").is_none());
    }

    #[test]
    fn destination_config_never_contains_a_stream_key() {
        let mut config = AppConfig::default();
        config.rtmp_destinations.push(RtmpDestinationConfig {
            id: "instagram-main".into(),
            name: "Instagram".into(),
            kind: RtmpDestinationKind::Instagram,
            server: "rtmps://example.test/live".into(),
            enabled: true,
        });
        let serialized = serde_json::to_string(&config).unwrap();
        assert!(serialized.contains("instagram-main"));
        assert!(!serialized.to_ascii_lowercase().contains("streamkey"));
        assert!(!serialized.contains("secret"));
    }

    #[test]
    fn destination_validation_rejects_key_fragments() {
        let destination = RtmpDestinationConfig {
            id: "target-1".into(),
            name: "Target".into(),
            kind: RtmpDestinationKind::Custom,
            server: "rtmps://example.test/live#secret".into(),
            enabled: true,
        };
        assert!(validate_rtmp_destination_config(&destination).is_err());
    }
}
