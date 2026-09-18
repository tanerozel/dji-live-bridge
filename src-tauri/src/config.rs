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
pub enum ProductionEngine {
    #[default]
    NativeFfmpeg,
    Obs,
}

#[derive(Debug, Clone)]
pub enum BroadcastDestination {
    TikTokLiveStudio,
    TikTokRtmp { server: String, key: String },
    CustomRtmp { server: String, key: String },
}

impl BroadcastDestination {
    pub fn rtmp_parts(&self) -> BridgeResult<(&str, &str)> {
        match self {
            Self::TikTokRtmp { server, key } | Self::CustomRtmp { server, key } => {
                validate_rtmp_destination(server, key)?;
                Ok((server, key))
            }
            Self::TikTokLiveStudio => Err(BridgeError::UnsupportedAutomation(
                "TikTok LIVE Studio is available on macOS, but Go Live must be clicked manually. An OBS-free camera requires a signed and user-approved Core Media I/O Camera Extension; this build does not claim one is installed. OBS Virtual Camera remains an optional video-only route.".into(),
            )),
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
    pub destination_mode: Option<DestinationMode>,
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
            destination_mode: Some(DestinationMode::TikTokLiveStudio),
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
        let logs_dir = dirs::home_dir()
            .ok_or_else(|| BridgeError::Config("Home directory not found".into()))?
            .join("Library/Logs")
            .join(APP_DIR_NAME);
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
        let temporary = self.app_support_dir.join("config.json.tmp");
        let final_path = self.app_support_dir.join("config.json");
        fs::write(&temporary, serde_json::to_vec_pretty(config)?)?;
        fs::rename(temporary, final_path)?;
        Ok(())
    }
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
