use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};
use tokio::sync::RwLock;

use crate::{
    audio::AudioInputDevice,
    config::ProductionEngine,
    error::{BridgeError, BridgeResult, ErrorPayload},
    network::NetworkInterface,
    process::ProcessSnapshot,
    virtual_camera::VirtualCameraState,
};

pub const STATE_EVENT: &str = "bridge://state";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkflowState {
    #[default]
    Idle,
    Preparing,
    WaitingForDrone,
    DroneConnected,
    PreparingObs,
    Ready,
    GoingLive,
    Live,
    Stopping,
}

impl WorkflowState {
    pub fn can_transition_to(self, next: Self) -> bool {
        use WorkflowState::*;
        matches!(
            (self, next),
            (Idle, Preparing)
                | (Preparing, WaitingForDrone)
                | (WaitingForDrone, DroneConnected)
                | (DroneConnected, WaitingForDrone)
                | (DroneConnected, PreparingObs)
                | (PreparingObs, DroneConnected)
                | (PreparingObs, Ready)
                | (Ready, WaitingForDrone)
                | (Ready, GoingLive)
                | (GoingLive, Ready)
                | (GoingLive, Live)
                | (Live, Stopping)
                | (Stopping, Ready)
                | (Ready, Stopping)
                | (DroneConnected, Stopping)
                | (WaitingForDrone, Stopping)
                | (Preparing, Stopping)
                | (Stopping, Idle)
        ) || self == next
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamMetadata {
    pub resolution: Option<String>,
    pub fps: Option<f64>,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    pub video_bitrate_bps: Option<u64>,
    pub audio_bitrate_bps: Option<u64>,
    pub bitrate_calculated_bps: Option<u64>,
    pub received_bytes: u64,
    pub uptime_seconds: Option<u64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewState {
    pub direct_whep_url: String,
    pub fallback_whep_url: String,
    pub mode: PreviewMode,
    pub status: ServiceStatus,
    pub reason_key: Option<String>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PreviewMode {
    #[default]
    Direct,
    Transcoded,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ServiceStatus {
    #[default]
    Unavailable,
    Starting,
    Ready,
    Failed,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObsState {
    pub installed: bool,
    pub running: bool,
    pub connected: bool,
    pub obs_version: Option<String>,
    pub websocket_version: Option<String>,
    pub available_requests: Vec<String>,
    pub scene_ready: bool,
    pub virtual_camera_active: Option<bool>,
    pub stream_active: Option<bool>,
    pub recording_active: Option<bool>,
    pub recording_paused: Option<bool>,
    pub last_recording_path: Option<String>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductionState {
    pub engine: ProductionEngine,
    pub prepared: bool,
    pub active: bool,
    pub path_status: ServiceStatus,
    pub encoder: Option<String>,
    pub selected_microphone: Option<String>,
    pub forward_state: Option<String>,
    pub forward_error: Option<String>,
    pub outbound_bytes: u64,
    pub recording_active: bool,
    pub recording_path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BridgeSnapshot {
    pub workflow: WorkflowState,
    pub interfaces: Vec<NetworkInterface>,
    pub selected_interface: Option<String>,
    pub lan_ipv4: Option<String>,
    pub rtmp_url: Option<String>,
    pub ip_change_warning: bool,
    pub media_mtx: ServiceStatus,
    pub publisher_present: bool,
    pub publisher_since_unix_ms: Option<u64>,
    pub metadata: StreamMetadata,
    pub preview: PreviewState,
    pub obs: ObsState,
    pub production: ProductionState,
    pub virtual_camera: VirtualCameraState,
    pub audio_inputs: Vec<AudioInputDevice>,
    pub processes: Vec<ProcessSnapshot>,
    pub last_error: Option<ErrorPayload>,
    pub updated_at_unix_ms: u64,
}

impl Default for BridgeSnapshot {
    fn default() -> Self {
        Self {
            workflow: WorkflowState::Idle,
            interfaces: Vec::new(),
            selected_interface: None,
            lan_ipv4: None,
            rtmp_url: None,
            ip_change_warning: false,
            media_mtx: ServiceStatus::Unavailable,
            publisher_present: false,
            publisher_since_unix_ms: None,
            metadata: StreamMetadata::default(),
            preview: PreviewState {
                direct_whep_url: "http://127.0.0.1:8889/drone/whep".into(),
                fallback_whep_url: "http://127.0.0.1:8889/drone-preview/whep".into(),
                reason_key: None,
                ..PreviewState::default()
            },
            obs: ObsState::default(),
            production: ProductionState::default(),
            virtual_camera: VirtualCameraState::default(),
            audio_inputs: Vec::new(),
            processes: Vec::new(),
            last_error: None,
            updated_at_unix_ms: now_ms(),
        }
    }
}

pub struct StateStore {
    snapshot: RwLock<BridgeSnapshot>,
}

impl StateStore {
    pub fn new() -> Self {
        Self {
            snapshot: RwLock::new(BridgeSnapshot::default()),
        }
    }

    pub async fn get(&self) -> BridgeSnapshot {
        self.snapshot.read().await.clone()
    }

    pub async fn mutate<F>(&self, app: &AppHandle, update: F)
    where
        F: FnOnce(&mut BridgeSnapshot),
    {
        let cloned = {
            let mut snapshot = self.snapshot.write().await;
            update(&mut snapshot);
            snapshot.updated_at_unix_ms = now_ms();
            snapshot.clone()
        };
        if let Err(error) = app.emit(STATE_EVENT, cloned) {
            tracing::warn!(%error, "failed to emit bridge state");
        }
    }

    pub async fn transition(&self, app: &AppHandle, next: WorkflowState) -> BridgeResult<()> {
        let current = self.snapshot.read().await.workflow;
        if !current.can_transition_to(next) {
            return Err(BridgeError::InvalidTransition(format!(
                "{current:?} -> {next:?}"
            )));
        }
        self.mutate(app, |snapshot| {
            snapshot.workflow = next;
            snapshot.last_error = None;
        })
        .await;
        Ok(())
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
