use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use tauri::AppHandle;
use tokio::sync::RwLock;

use crate::{
    audio,
    config::{
        AppConfig, BroadcastDestination, ConfigStore, DestinationMode, FitMode, OutputLayout,
        ProductionEngine, RtmpDestinationConfig, RtmpDestinationInput, RtmpDestinationKind,
        rtmp_keychain_account, validate_rtmp_destination, validate_rtmp_destination_config,
    },
    error::{BridgeError, BridgeResult, ErrorPayload},
    ffmpeg,
    mediamtx::{ForwardStatus, MediaMtxController},
    network,
    obs::ObsController,
    platform,
    process::ProcessSupervisor,
    recording,
    state::{
        BridgeSnapshot, PreviewMode, ProductionState, RtmpDestinationState, ServiceStatus,
        StateStore, WorkflowState,
    },
    virtual_camera,
};

pub struct AppState {
    pub state: StateStore,
    pub config_store: ConfigStore,
    pub config: RwLock<AppConfig>,
    pub supervisor: ProcessSupervisor,
    pub media_mtx: MediaMtxController,
    pub obs: ObsController,
    pub obs_monitoring: AtomicBool,
    shutting_down: AtomicBool,
    pub active_destination_ids: RwLock<Vec<String>>,
}

impl AppState {
    pub fn build() -> BridgeResult<Arc<Self>> {
        let config_store = ConfigStore::discover()?;
        let mut config = config_store.load()?;
        migrate_legacy_destination(&mut config, &config_store)?;
        let media_mtx = MediaMtxController::new(&config_store)?;
        Ok(Arc::new(Self {
            state: StateStore::new(),
            config_store,
            config: RwLock::new(config),
            supervisor: ProcessSupervisor::default(),
            media_mtx,
            obs: ObsController::default(),
            obs_monitoring: AtomicBool::new(false),
            shutting_down: AtomicBool::new(false),
            active_destination_ids: RwLock::new(Vec::new()),
        }))
    }

    pub async fn bootstrap(self: Arc<Self>, app: AppHandle) {
        if let Err(error) = self.run_bootstrap(&app).await {
            self.publish_error(&app, error).await;
        }
        self.monitor(app).await;
    }

    async fn run_bootstrap(&self, app: &AppHandle) -> BridgeResult<()> {
        self.state.transition(app, WorkflowState::Preparing).await?;
        self.sync_destination_snapshot(app).await;
        self.refresh_network(app, false).await?;
        self.state
            .mutate(app, |snapshot| snapshot.media_mtx = ServiceStatus::Starting)
            .await;
        self.media_mtx.start(&self.supervisor).await?;
        self.state
            .mutate(app, |snapshot| snapshot.media_mtx = ServiceStatus::Ready)
            .await;
        self.state
            .transition(app, WorkflowState::WaitingForDrone)
            .await
    }

    async fn monitor(&self, app: AppHandle) {
        let mut network_check = Instant::now();
        let mut publisher_check = Instant::now() - Duration::from_secs(2);
        let mut metadata_check = Instant::now();
        let mut obs_check = Instant::now();
        let mut audio_check = Instant::now() - Duration::from_secs(30);
        let mut production_check = Instant::now();
        let mut virtual_camera_check = Instant::now() - Duration::from_secs(60);
        let mut obs_backoff = Duration::from_secs(2);
        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;
            let processes = self.supervisor.tick().await;
            self.state
                .mutate_if_changed(&app, |snapshot| {
                    if snapshot.processes == processes {
                        false
                    } else {
                        snapshot.processes = processes;
                        true
                    }
                })
                .await;

            if network_check.elapsed() >= Duration::from_secs(5) {
                if let Err(error) = self.refresh_network(&app, true).await {
                    tracing::warn!(%error, "network refresh failed");
                }
                network_check = Instant::now();
            }

            if audio_check.elapsed() >= Duration::from_secs(10) {
                match tokio::task::spawn_blocking(audio::discover_input_devices).await {
                    Ok(Ok(devices)) => {
                        self.state
                            .mutate_if_changed(&app, |snapshot| {
                                if snapshot.audio_inputs == devices {
                                    false
                                } else {
                                    snapshot.audio_inputs = devices;
                                    true
                                }
                            })
                            .await;
                    }
                    Ok(Err(error)) => tracing::debug!(%error, "audio device discovery failed"),
                    Err(error) => tracing::debug!(%error, "audio device discovery task failed"),
                }
                audio_check = Instant::now();
            }

            if publisher_check.elapsed() >= Duration::from_secs(2) {
                match self.media_mtx.publisher_sample().await {
                    Ok(sample) => {
                        let previous = self.state.get().await;
                        let became_connected = sample.present && !previous.publisher_present;
                        let became_disconnected = !sample.present && previous.publisher_present;
                        let since = sample.ready_since_unix_ms.or_else(|| {
                            if sample.present {
                                previous.publisher_since_unix_ms.or_else(|| Some(now_ms()))
                            } else {
                                None
                            }
                        });
                        self.state
                            .mutate(&app, |snapshot| {
                                snapshot.media_mtx = ServiceStatus::Ready;
                                snapshot.publisher_present = sample.present;
                                snapshot.publisher_since_unix_ms = since;
                                snapshot.metadata.received_bytes = sample.metadata.received_bytes;
                                snapshot.metadata.bitrate_calculated_bps =
                                    sample.metadata.bitrate_calculated_bps;
                                snapshot.metadata.uptime_seconds =
                                    since.map(|start| now_ms().saturating_sub(start) / 1000);
                            })
                            .await;
                        if became_connected {
                            if let Err(error) = self
                                .state
                                .transition(&app, WorkflowState::DroneConnected)
                                .await
                            {
                                tracing::warn!(%error, "publisher transition failed");
                            }
                            metadata_check = Instant::now() - Duration::from_secs(28);
                        } else if became_disconnected {
                            if previous.workflow == WorkflowState::Live {
                                let config = self.config.read().await.clone();
                                if config.production_engine == ProductionEngine::NativeFfmpeg {
                                    self.media_mtx.clear_forward().await.ok();
                                    self.supervisor.stop("native-production").await.ok();
                                } else if let Err(error) = self
                                    .obs
                                    .stop_stream_and_restore(&config.obs_host, config.obs_port)
                                    .await
                                {
                                    tracing::warn!(%error, "failed to stop OBS after publisher disconnect");
                                }
                            }
                            self.supervisor.stop("native-recording").await.ok();
                            self.supervisor.stop("preview-transcode").await.ok();
                            virtual_camera::stop_feed(&self.supervisor).await.ok();
                            self.state
                                .mutate(&app, |snapshot| {
                                    snapshot.workflow = WorkflowState::WaitingForDrone;
                                    snapshot.preview.status = ServiceStatus::Unavailable;
                                    snapshot.preview.mode = PreviewMode::Direct;
                                    snapshot.preview.reason_key = None;
                                    snapshot.preview.reason = None;
                                    snapshot.metadata = Default::default();
                                    snapshot.obs.stream_active = Some(false);
                                    snapshot.production.active = false;
                                    snapshot.production.prepared = false;
                                    snapshot.production.path_status = ServiceStatus::Unavailable;
                                    snapshot.production.forward_state = None;
                                    snapshot.production.forward_error = None;
                                    snapshot.production.outbound_bytes = 0;
                                    clear_destination_runtime(
                                        &mut snapshot.production.destinations,
                                    );
                                    snapshot.production.recording_active = false;
                                    snapshot.virtual_camera.feed_active = false;
                                })
                                .await;
                            self.active_destination_ids.write().await.clear();
                        }

                        if sample.present && metadata_check.elapsed() >= Duration::from_secs(30) {
                            match ffmpeg::inspect_stream().await {
                                Ok(metadata) => {
                                    self.state
                                        .mutate(&app, |snapshot| {
                                            let received = snapshot.metadata.received_bytes;
                                            let calculated =
                                                snapshot.metadata.bitrate_calculated_bps;
                                            let uptime = snapshot.metadata.uptime_seconds;
                                            snapshot.metadata = metadata;
                                            snapshot.metadata.received_bytes = received;
                                            snapshot.metadata.bitrate_calculated_bps = calculated;
                                            snapshot.metadata.uptime_seconds = uptime;
                                        })
                                        .await;
                                }
                                Err(error) => tracing::debug!(%error, "metadata probe pending"),
                            }
                            metadata_check = Instant::now();
                        }
                    }
                    Err(error) => {
                        self.state
                            .mutate(&app, |snapshot| {
                                snapshot.media_mtx = ServiceStatus::Failed;
                                snapshot.last_error = Some(error.into());
                            })
                            .await;
                    }
                }
                publisher_check = Instant::now();
            }

            if production_check.elapsed() >= Duration::from_secs(2) {
                let current = self.state.get().await;
                if current.production.active {
                    let active_ids = self.active_destination_ids.read().await.clone();
                    match self.media_mtx.forward_statuses().await {
                        Ok(statuses) => {
                            self.state
                                .mutate(&app, |snapshot| {
                                    apply_forward_statuses(
                                        &mut snapshot.production,
                                        &active_ids,
                                        &statuses,
                                    );
                                })
                                .await;
                        }
                        Err(error) => tracing::warn!(%error, "production forward status failed"),
                    }
                }
                production_check = Instant::now();
            }

            let camera_snapshot = self.state.get().await;
            let camera_interval =
                if camera_snapshot.virtual_camera.status == ServiceStatus::Starting {
                    Duration::from_secs(2)
                } else {
                    Duration::from_secs(30)
                };
            if virtual_camera_check.elapsed() >= camera_interval {
                let feed_active = camera_snapshot.processes.iter().any(|process| {
                    process.name == virtual_camera::FEED_PROCESS_NAME
                        && matches!(
                            process.status,
                            crate::process::ProcessStatus::Starting
                                | crate::process::ProcessStatus::Running
                                | crate::process::ProcessStatus::BackingOff
                        )
                });
                match tokio::task::spawn_blocking(move || virtual_camera::inspect(feed_active))
                    .await
                {
                    Ok(camera) => {
                        self.state
                            .mutate_if_changed(&app, |snapshot| {
                                if snapshot.virtual_camera == camera {
                                    false
                                } else {
                                    snapshot.virtual_camera = camera;
                                    true
                                }
                            })
                            .await;
                    }
                    Err(error) => tracing::debug!(%error, "virtual camera inspection task failed"),
                }
                virtual_camera_check = Instant::now();
            }

            if self.obs_monitoring.load(Ordering::Relaxed) && obs_check.elapsed() >= obs_backoff {
                let config = self.config.read().await.clone();
                match self.obs.inspect(&config.obs_host, config.obs_port).await {
                    Ok(obs) => {
                        self.state
                            .mutate_if_changed(&app, |snapshot| {
                                if snapshot.obs == obs {
                                    false
                                } else {
                                    snapshot.obs = obs;
                                    true
                                }
                            })
                            .await;
                        obs_backoff = Duration::from_secs(5);
                    }
                    Err(error) => {
                        let installed = platform::obs_installed();
                        let running = platform::obs_running();
                        self.state
                            .mutate(&app, |snapshot| {
                                snapshot.obs.installed = installed;
                                snapshot.obs.running = running;
                                snapshot.obs.connected = false;
                                snapshot.obs.last_error = Some(error.to_string());
                            })
                            .await;
                        obs_backoff = (obs_backoff * 2).min(Duration::from_secs(30));
                    }
                }
                obs_check = Instant::now();
            }
        }
    }

    pub async fn refresh_network(&self, app: &AppHandle, detect_change: bool) -> BridgeResult<()> {
        let interfaces = tokio::task::spawn_blocking(network::discover_interfaces)
            .await
            .map_err(|error| {
                BridgeError::Network(format!("Network discovery task failed: {error}"))
            })??;
        let preferred = self.config.read().await.selected_interface.clone();
        let selected = network::select_interface(&interfaces, preferred.as_deref()).cloned();
        let previous = self.state.get().await;
        let before = previous.lan_ipv4;
        let selected_name = selected.as_ref().map(|value| value.name.clone());
        let selected_ip = selected.as_ref().map(|value| value.ipv4.clone());
        let rtmp_url = selected_ip
            .as_ref()
            .map(|address| format!("rtmp://{address}:1935/drone"));
        self.state
            .mutate_if_changed(app, |snapshot| {
                let ip_change_warning = detect_change && before.is_some() && before != selected_ip;
                let changed = snapshot.interfaces != interfaces
                    || snapshot.selected_interface != selected_name
                    || snapshot.lan_ipv4 != selected_ip
                    || snapshot.rtmp_url != rtmp_url
                    || snapshot.ip_change_warning != ip_change_warning;
                if !changed {
                    return false;
                }
                snapshot.interfaces = interfaces;
                snapshot.selected_interface = selected_name;
                snapshot.lan_ipv4 = selected_ip.clone();
                snapshot.rtmp_url = rtmp_url;
                snapshot.ip_change_warning = ip_change_warning;
                true
            })
            .await;
        Ok(())
    }

    pub async fn select_interface(&self, app: &AppHandle, name: String) -> BridgeResult<()> {
        let interfaces = network::discover_interfaces()?;
        if !interfaces.iter().any(|interface| interface.name == name) {
            return Err(BridgeError::Validation(
                "Selected interface is no longer active".into(),
            ));
        }
        {
            let mut config = self.config.write().await;
            config.selected_interface = Some(name);
            self.config_store.save(&config)?;
        }
        self.refresh_network(app, true).await
    }

    pub async fn start_test_drone(&self, input: PathBuf) -> BridgeResult<()> {
        ffmpeg::start_test_drone(&self.supervisor, &input).await
    }

    pub async fn activate_preview_fallback(&self, app: &AppHandle) -> BridgeResult<String> {
        let snapshot = self.state.get().await;
        if !snapshot.publisher_present {
            return Err(BridgeError::Ffmpeg(
                "Preview fallback requires an active /drone publisher".into(),
            ));
        }
        let reason = preview_failure_reason(&snapshot);
        self.state
            .mutate(app, |state| {
                state.preview.status = ServiceStatus::Starting;
                state.preview.reason_key = Some("preview.reason.preparingFallback".into());
                state.preview.reason = Some(reason);
            })
            .await;
        match ffmpeg::start_preview_fallback(&self.supervisor).await {
            Ok(encoder) => {
                if let Err(error) = self
                    .media_mtx
                    .wait_for_path("drone-preview", Duration::from_secs(8))
                    .await
                {
                    self.supervisor.stop("preview-transcode").await.ok();
                    self.state
                        .mutate(app, |state| {
                            state.preview.status = ServiceStatus::Failed;
                            state.preview.reason_key = Some("preview.reason.failed".into());
                            state.preview.reason = Some(error.to_string());
                        })
                        .await;
                    return Err(error);
                }
                self.state
                    .mutate(app, |state| {
                        state.preview.mode = PreviewMode::Transcoded;
                        state.preview.status = ServiceStatus::Starting;
                        state.preview.reason_key = Some("preview.reason.transcodeReady".into());
                        state.preview.reason = Some(format!(
                            "Preview-only transcode is ready with {encoder} + Opus; ingest and production remain independent"
                        ));
                    })
                    .await;
                Ok("http://127.0.0.1:8889/drone-preview/whep".into())
            }
            Err(error) => {
                self.state
                    .mutate(app, |state| {
                        state.preview.status = ServiceStatus::Failed;
                        state.preview.reason_key = Some("preview.reason.failed".into());
                        state.preview.reason = Some(error.to_string());
                    })
                    .await;
                Err(error)
            }
        }
    }

    pub async fn report_preview_status(
        &self,
        app: &AppHandle,
        connected: bool,
        detail: Option<String>,
    ) {
        let current = self.state.get().await;
        let aac_notice = current.preview.mode == PreviewMode::Direct
            && current.metadata.audio_codec.as_deref() == Some("AAC");
        self.state
            .mutate(app, |snapshot| {
                snapshot.preview.status = if connected {
                    ServiceStatus::Ready
                } else {
                    ServiceStatus::Failed
                };
                snapshot.preview.reason_key = if connected && aac_notice {
                    Some("preview.reason.aacNotice".into())
                } else if detail.is_some() {
                    Some("preview.reason.failed".into())
                } else {
                    None
                };
                snapshot.preview.reason = if connected && aac_notice {
                    Some("Direct WHEP video is connected; AAC is not carried to WebRTC preview. Use preview-only H.264 + Opus fallback when preview audio is required.".into())
                } else {
                    detail
                };
            })
            .await;
    }

    pub async fn prepare_obs(
        &self,
        app: &AppHandle,
        layout: OutputLayout,
        fit_mode: FitMode,
    ) -> BridgeResult<()> {
        if !self.state.get().await.publisher_present {
            return Err(BridgeError::Obs(
                "OBS preparation requires an active drone publisher".into(),
            ));
        }
        self.state
            .transition(app, WorkflowState::PreparingObs)
            .await?;
        let config = self.config.read().await.clone();
        match self
            .obs
            .prepare_scene(&config.obs_host, config.obs_port, layout, fit_mode)
            .await
        {
            Ok(obs) => {
                {
                    let mut stored = self.config.write().await;
                    stored.production_engine = ProductionEngine::Obs;
                    stored.output_layout = layout;
                    stored.fit_mode = fit_mode;
                    self.config_store.save(&stored)?;
                }
                self.state.mutate(app, |snapshot| snapshot.obs = obs).await;
                self.state
                    .mutate(app, |snapshot| {
                        snapshot.production.engine = ProductionEngine::Obs;
                        snapshot.production.prepared = true;
                        snapshot.production.selected_microphone = None;
                    })
                    .await;
                self.state.transition(app, WorkflowState::Ready).await
            }
            Err(error) => {
                self.state
                    .transition(app, WorkflowState::DroneConnected)
                    .await?;
                Err(error)
            }
        }
    }

    pub async fn prepare_native_production(
        &self,
        app: &AppHandle,
        settings: ffmpeg::NativeProductionSettings,
    ) -> BridgeResult<()> {
        let snapshot = self.state.get().await;
        if !snapshot.publisher_present {
            return Err(BridgeError::Ffmpeg(
                "Built-in production requires an active drone publisher".into(),
            ));
        }
        if !matches!(
            snapshot.workflow,
            WorkflowState::DroneConnected | WorkflowState::Ready
        ) {
            return Err(BridgeError::InvalidTransition(
                "Built-in production can only be prepared after the drone connects".into(),
            ));
        }
        if snapshot.workflow == WorkflowState::Ready {
            self.state
                .mutate(app, |state| state.workflow = WorkflowState::DroneConnected)
                .await;
        }
        self.state
            .transition(app, WorkflowState::PreparingObs)
            .await?;
        let capabilities = ffmpeg::capabilities().await;
        if capabilities.ffmpeg_path.is_none()
            || (!capabilities.h264_videotoolbox && !capabilities.libx264)
            || !capabilities.aac
        {
            self.state
                .transition(app, WorkflowState::DroneConnected)
                .await?;
            return Err(BridgeError::Ffmpeg(
                "Built-in production requires FFmpeg with H.264 and AAC encoders".into(),
            ));
        }
        if settings.microphone.is_some() && !capabilities.avfoundation {
            self.state
                .transition(app, WorkflowState::DroneConnected)
                .await?;
            return Err(BridgeError::Ffmpeg(
                "Commentary microphone requires FFmpeg AVFoundation input support".into(),
            ));
        }
        {
            let mut config = self.config.write().await;
            config.production_engine = ProductionEngine::NativeFfmpeg;
            config.output_layout = settings.layout;
            config.fit_mode = settings.fit_mode;
            config.selected_microphone = settings.microphone.clone();
            config.microphone_muted = settings.microphone_muted;
            config.microphone_volume_db = settings.microphone_volume_db;
            config.microphone_sync_ms = settings.microphone_sync_ms;
            config.noise_suppression = settings.noise_suppression;
            config.compressor = settings.compressor;
            config.limiter = settings.limiter;
            self.config_store.save(&config)?;
        }
        self.state
            .mutate(app, |snapshot| {
                snapshot.production.engine = ProductionEngine::NativeFfmpeg;
                snapshot.production.prepared = true;
                snapshot.production.active = false;
                snapshot.production.path_status = ServiceStatus::Unavailable;
                snapshot.production.encoder = None;
                snapshot.production.selected_microphone = settings.microphone;
                snapshot.production.forward_state = None;
                snapshot.production.forward_error = None;
                snapshot.production.outbound_bytes = 0;
                clear_destination_runtime(&mut snapshot.production.destinations);
            })
            .await;
        self.state.transition(app, WorkflowState::Ready).await
    }

    async fn sync_destination_snapshot(&self, app: &AppHandle) {
        let destinations = self.config.read().await.rtmp_destinations.clone();
        self.state
            .mutate(app, |snapshot| {
                let previous = std::mem::take(&mut snapshot.production.destinations);
                snapshot.production.destinations = destinations
                    .into_iter()
                    .map(|destination| {
                        previous
                            .iter()
                            .find(|item| item.id == destination.id)
                            .cloned()
                            .map(|mut item| {
                                item.name = destination.name.clone();
                                item.kind = destination.kind;
                                item.server = destination.server.clone();
                                item.enabled = destination.enabled;
                                item
                            })
                            .unwrap_or_else(|| destination_state(&destination))
                    })
                    .collect();
            })
            .await;
    }

    pub async fn upsert_rtmp_destination(
        &self,
        app: &AppHandle,
        input: RtmpDestinationInput,
    ) -> BridgeResult<String> {
        if self.state.get().await.production.active {
            return Err(BridgeError::InvalidTransition(
                "Stop the live route before editing destinations".into(),
            ));
        }
        let name = input.name.trim().to_string();
        let server = input.server.trim().trim_end_matches('#').to_string();
        let mut config = self.config.write().await;
        let destination_id = input.id.unwrap_or_else(|| {
            format!(
                "rtmp-{:x}-{}",
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos(),
                config.rtmp_destinations.len()
            )
        });
        let existing = config
            .rtmp_destinations
            .iter()
            .position(|destination| destination.id == destination_id);
        let destination = RtmpDestinationConfig {
            id: destination_id.clone(),
            name,
            kind: input.kind,
            server,
            enabled: input.enabled,
        };
        validate_rtmp_destination_config(&destination)?;
        let key = input.key.filter(|value| !value.trim().is_empty());
        let effective_key = if let Some(key) = key.as_deref() {
            validate_rtmp_destination(&destination.server, key)?;
            key.to_string()
        } else if existing.is_some() {
            load_destination_key(&destination.id)?
        } else {
            return Err(BridgeError::Validation(
                "A stream key is required for a new destination".into(),
            ));
        };
        store_destination_key(&destination.id, &effective_key)?;
        if let Some(index) = existing {
            config.rtmp_destinations[index] = destination;
        } else {
            config.rtmp_destinations.push(destination);
        }
        self.config_store.save(&config)?;
        drop(config);
        self.sync_destination_snapshot(app).await;
        Ok(destination_id)
    }

    pub async fn remove_rtmp_destination(&self, app: &AppHandle, id: String) -> BridgeResult<()> {
        if self.state.get().await.production.active {
            return Err(BridgeError::InvalidTransition(
                "Stop the live route before removing destinations".into(),
            ));
        }
        let mut config = self.config.write().await;
        let before = config.rtmp_destinations.len();
        config
            .rtmp_destinations
            .retain(|destination| destination.id != id);
        if config.rtmp_destinations.len() == before {
            return Err(BridgeError::Validation(
                "RTMP destination was not found".into(),
            ));
        }
        self.config_store.save(&config)?;
        drop(config);
        if let Ok(entry) = keyring::Entry::new(
            "com.djilivebridge.app.destination",
            &rtmp_keychain_account(&id),
        ) {
            let _ = entry.delete_credential();
        }
        self.sync_destination_snapshot(app).await;
        Ok(())
    }

    pub async fn set_rtmp_destination_enabled(
        &self,
        app: &AppHandle,
        id: String,
        enabled: bool,
    ) -> BridgeResult<()> {
        if self.state.get().await.production.active {
            return Err(BridgeError::InvalidTransition(
                "Stop the live route before changing active destinations".into(),
            ));
        }
        let mut config = self.config.write().await;
        let destination = config
            .rtmp_destinations
            .iter_mut()
            .find(|destination| destination.id == id)
            .ok_or_else(|| BridgeError::Validation("RTMP destination was not found".into()))?;
        destination.enabled = enabled;
        self.config_store.save(&config)?;
        drop(config);
        self.sync_destination_snapshot(app).await;
        Ok(())
    }

    pub async fn activate_virtual_camera_extension(&self, app: &AppHandle) -> BridgeResult<()> {
        tokio::task::spawn_blocking(virtual_camera::request_activation)
            .await
            .map_err(|error| {
                BridgeError::VirtualCamera(format!("Camera activation task failed: {error}"))
            })??;
        self.state
            .mutate(app, |snapshot| {
                snapshot.virtual_camera.status = ServiceStatus::Starting;
                snapshot.virtual_camera.detail_key = "camera.detail.activationRequested".into();
                snapshot.virtual_camera.detail = Some(
                    "Activation requested. Approve DJI Live Bridge Camera in System Settings if macOS asks."
                        .into(),
                );
            })
            .await;
        Ok(())
    }

    pub async fn set_native_virtual_camera(
        &self,
        app: &AppHandle,
        active: bool,
    ) -> BridgeResult<()> {
        if active {
            if !self.state.get().await.publisher_present {
                return Err(BridgeError::VirtualCamera(
                    "Virtual camera requires an active DJI or Test Drone publisher".into(),
                ));
            }
            // The cached status can be up to 30 s old, and macOS deactivates the
            // extension whenever the app in /Applications is replaced. Check
            // the live state; if it is not active, start activation instead of
            // failing, and let the monitor flip it to Ready once approved.
            let camera = tokio::task::spawn_blocking(|| virtual_camera::inspect(false))
                .await
                .map_err(|error| {
                    BridgeError::VirtualCamera(format!("Camera status check failed: {error}"))
                })?;
            if camera.status != ServiceStatus::Ready {
                let awaiting_approval = camera.status == ServiceStatus::Starting;
                self.state
                    .mutate(app, |snapshot| snapshot.virtual_camera = camera)
                    .await;
                if awaiting_approval {
                    return Err(BridgeError::VirtualCamera(
                        "macOS is waiting for you to allow DJI Live Bridge Camera in System Settings → General → Login Items & Extensions → Camera Extensions".into(),
                    ));
                }
                return self.activate_virtual_camera_extension(app).await;
            }
            virtual_camera::start_feed(&self.supervisor).await?;
            self.state
                .mutate(app, |snapshot| {
                    snapshot.virtual_camera.feed_active = true;
                    snapshot.virtual_camera.detail_key = "camera.detail.feedReady".into();
                    snapshot.virtual_camera.detail = Some(
                        "Portrait video is ready. DJI Live Bridge audio is disabled; select one microphone only in TikTok LIVE Studio."
                            .into(),
                    );
                })
                .await;
        } else {
            virtual_camera::stop_feed(&self.supervisor).await?;
            self.state
                .mutate(app, |snapshot| {
                    snapshot.virtual_camera.feed_active = false;
                    snapshot.virtual_camera.detail_key = "camera.detail.feedStopped".into();
                    snapshot.virtual_camera.detail =
                        Some("Camera extension is enabled; the video feed is stopped".into());
                })
                .await;
        }
        Ok(())
    }

    pub async fn start_live(&self, app: &AppHandle) -> BridgeResult<()> {
        if self.state.get().await.workflow != WorkflowState::Ready {
            return Err(BridgeError::InvalidTransition(
                "START LIVE requires the Ready state".into(),
            ));
        }
        let config = self.config.read().await.clone();
        let enabled = config
            .rtmp_destinations
            .iter()
            .filter(|destination| destination.enabled)
            .cloned()
            .collect::<Vec<_>>();
        if enabled.is_empty() {
            return Err(BridgeError::Validation(
                "Enable at least one RTMP destination before starting".into(),
            ));
        }
        let mut destinations = Vec::with_capacity(enabled.len());
        for destination in enabled {
            let key = load_destination_key(&destination.id).map_err(|_| {
                BridgeError::Config(format!(
                    "Stream key is missing for destination '{}'",
                    destination.name
                ))
            })?;
            validate_rtmp_destination(&destination.server, &key)?;
            destinations.push((destination, key));
        }
        if config.production_engine == ProductionEngine::Obs && destinations.len() > 1 {
            return Err(BridgeError::Validation(
                "OBS streaming supports one RTMP destination here; use built-in production for multi-stream"
                    .into(),
            ));
        }
        let active_ids = destinations
            .iter()
            .map(|(destination, _)| destination.id.clone())
            .collect::<Vec<_>>();
        self.state.transition(app, WorkflowState::GoingLive).await?;
        let result = if config.production_engine == ProductionEngine::NativeFfmpeg {
            self.start_native_live(&config, &destinations).await
        } else {
            let (destination, key) = destinations
                .first()
                .expect("enabled destinations were checked above");
            let destination = BroadcastDestination::from_rtmp(destination, key.clone());
            self.obs
                .start_stream(&config.obs_host, config.obs_port, &destination)
                .await
                .map(|_| {
                    (
                        None,
                        vec![ForwardStatus {
                            pos: 0,
                            state: Some("forwarding".into()),
                            last_error: None,
                            outbound_bytes: 0,
                        }],
                    )
                })
        };
        match result {
            Ok((encoder, statuses)) => {
                *self.active_destination_ids.write().await = active_ids.clone();
                self.state
                    .mutate(app, |snapshot| {
                        if config.production_engine == ProductionEngine::NativeFfmpeg {
                            snapshot.production.active = true;
                            snapshot.production.path_status = ServiceStatus::Ready;
                            snapshot.production.encoder = encoder;
                        } else {
                            snapshot.obs.stream_active = Some(true);
                        }
                        apply_forward_statuses(&mut snapshot.production, &active_ids, &statuses);
                    })
                    .await;
                self.state.transition(app, WorkflowState::Live).await
            }
            Err(error) => {
                self.state.transition(app, WorkflowState::Ready).await?;
                Err(error)
            }
        }
    }

    async fn start_native_live(
        &self,
        config: &AppConfig,
        destinations: &[(RtmpDestinationConfig, String)],
    ) -> BridgeResult<(Option<String>, Vec<ForwardStatus>)> {
        let forwards = destinations
            .iter()
            .map(|(destination, key)| (destination.server.clone(), key.clone()))
            .collect::<Vec<_>>();
        self.media_mtx.configure_forwards(&forwards).await?;
        let settings = ffmpeg::NativeProductionSettings {
            layout: config.output_layout,
            fit_mode: config.fit_mode,
            microphone: config.selected_microphone.clone(),
            microphone_muted: config.microphone_muted,
            microphone_volume_db: config.microphone_volume_db,
            microphone_sync_ms: config.microphone_sync_ms,
            noise_suppression: config.noise_suppression,
            compressor: config.compressor,
            limiter: config.limiter,
        };
        // The periodic probe runs ~20 s after the drone connects; going live
        // before that must not replace the drone's audio with silence.
        let has_drone_audio = match ffmpeg::inspect_stream().await {
            Ok(metadata) => metadata.audio_codec.is_some(),
            Err(error) => {
                tracing::debug!(%error, "live-start probe failed; using cached metadata");
                self.state.get().await.metadata.audio_codec.is_some()
            }
        };
        let encoder =
            match ffmpeg::start_production(&self.supervisor, &settings, has_drone_audio).await {
                Ok(value) => value,
                Err(error) => {
                    self.media_mtx.clear_forward().await.ok();
                    return Err(error);
                }
            };
        if let Err(error) = self
            .media_mtx
            .wait_for_path("production", Duration::from_secs(10))
            .await
        {
            self.supervisor.stop("native-production").await.ok();
            self.media_mtx.clear_forward().await.ok();
            return Err(error);
        }
        let statuses = match self
            .media_mtx
            .wait_forwards(destinations.len(), Duration::from_secs(10))
            .await
        {
            Ok(statuses) => statuses,
            Err(error) => {
                self.supervisor.stop("native-production").await.ok();
                self.media_mtx.clear_forward().await.ok();
                return Err(error);
            }
        };
        Ok((Some(encoder), statuses))
    }

    pub async fn stop_live(&self, app: &AppHandle) -> BridgeResult<()> {
        if self.state.get().await.workflow != WorkflowState::Live {
            return Err(BridgeError::InvalidTransition(
                "STOP LIVE requires the Live state".into(),
            ));
        }
        self.state.transition(app, WorkflowState::Stopping).await?;
        let config = self.config.read().await.clone();
        let result = if config.production_engine == ProductionEngine::NativeFfmpeg {
            self.media_mtx.clear_forward().await.ok();
            self.supervisor.stop("native-production").await
        } else {
            self.obs
                .stop_stream_and_restore(&config.obs_host, config.obs_port)
                .await
        };
        match result {
            Ok(()) => {
                self.active_destination_ids.write().await.clear();
                self.state
                    .mutate(app, |snapshot| {
                        snapshot.obs.stream_active = Some(false);
                        snapshot.production.active = false;
                        snapshot.production.path_status = ServiceStatus::Unavailable;
                        snapshot.production.forward_state = None;
                        snapshot.production.forward_error = None;
                        snapshot.production.outbound_bytes = 0;
                        clear_destination_runtime(&mut snapshot.production.destinations);
                    })
                    .await;
                self.state.transition(app, WorkflowState::Ready).await
            }
            Err(error) => {
                self.state.transition(app, WorkflowState::Ready).await?;
                Err(error)
            }
        }
    }

    pub async fn set_native_recording(
        &self,
        app: &AppHandle,
        active: bool,
    ) -> BridgeResult<Option<String>> {
        let snapshot = self.state.get().await;
        if active {
            if !snapshot.publisher_present {
                return Err(BridgeError::Ffmpeg(
                    "Recording requires an active drone publisher".into(),
                ));
            }
            let path =
                recording::start_native_recording(&self.supervisor, snapshot.production.active)
                    .await?;
            let displayed = path.display().to_string();
            self.state
                .mutate(app, |state| {
                    state.production.recording_active = true;
                    state.production.recording_path = Some(displayed.clone());
                })
                .await;
            Ok(Some(displayed))
        } else {
            recording::stop_native_recording(&self.supervisor).await?;
            let path = snapshot.production.recording_path;
            self.state
                .mutate(app, |state| state.production.recording_active = false)
                .await;
            Ok(path)
        }
    }

    pub async fn publish_error(&self, app: &AppHandle, error: BridgeError) {
        let payload: ErrorPayload = error.into();
        tracing::error!(code = payload.code, message = %payload.detail);
        self.state
            .mutate(app, |snapshot| snapshot.last_error = Some(payload))
            .await;
    }

    pub async fn shutdown(&self, app: &AppHandle) {
        // macOS delivers both ExitRequested and Exit; only clean up once.
        if self.shutting_down.swap(true, Ordering::SeqCst) {
            return;
        }
        let current = self.state.get().await.workflow;
        if current.can_transition_to(WorkflowState::Stopping) {
            let _ = self.state.transition(app, WorkflowState::Stopping).await;
        }
        if current == WorkflowState::Live {
            let config = self.config.read().await.clone();
            if config.production_engine == ProductionEngine::NativeFfmpeg {
                self.media_mtx.clear_forward().await.ok();
                self.supervisor.stop("native-production").await.ok();
            } else if let Err(error) = self
                .obs
                .stop_stream_and_restore(&config.obs_host, config.obs_port)
                .await
            {
                tracing::warn!(%error, "failed to stop OBS stream and restore service");
            }
        }
        // Children first: OBS restore is network-bound and must never keep
        // MediaMTX/FFmpeg alive (orphans hold ports 1935/8554/9997).
        self.supervisor.shutdown_all().await;
        let config = self.config.read().await.clone();
        if let Err(error) = self
            .obs
            .restore_video_settings(&config.obs_host, config.obs_port)
            .await
        {
            tracing::debug!(%error, "OBS video settings were not restored");
        }
    }
}

fn preview_failure_reason(snapshot: &BridgeSnapshot) -> String {
    match (
        snapshot.metadata.video_codec.as_deref(),
        snapshot.metadata.audio_codec.as_deref(),
    ) {
        (_, Some("AAC")) => "AAC is not a browser-compatible MediaMTX WebRTC audio codec".into(),
        (Some("HEVC" | "H265"), _) => {
            "H.265 browser decoding is not available in this WebView path".into()
        }
        (Some("H264"), _) => {
            "Direct H.264 WHEP failed; likely profile/B-frame or local ICE compatibility".into()
        }
        _ => "Direct WHEP failed; codec or local ICE compatibility could not be confirmed".into(),
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn destination_state(destination: &RtmpDestinationConfig) -> RtmpDestinationState {
    RtmpDestinationState {
        id: destination.id.clone(),
        name: destination.name.clone(),
        kind: destination.kind,
        server: destination.server.clone(),
        enabled: destination.enabled,
        state: None,
        last_error: None,
        outbound_bytes: 0,
    }
}

fn clear_destination_runtime(destinations: &mut [RtmpDestinationState]) {
    for destination in destinations {
        destination.state = None;
        destination.last_error = None;
        destination.outbound_bytes = 0;
    }
}

fn apply_forward_statuses(
    production: &mut ProductionState,
    active_ids: &[String],
    statuses: &[ForwardStatus],
) {
    clear_destination_runtime(&mut production.destinations);
    for id in active_ids {
        if let Some(destination) = production
            .destinations
            .iter_mut()
            .find(|destination| &destination.id == id)
        {
            destination.state = Some("starting".into());
        }
    }
    for (id, status) in active_ids.iter().zip(statuses) {
        if let Some(destination) = production
            .destinations
            .iter_mut()
            .find(|destination| &destination.id == id)
        {
            destination.state = status.state.clone();
            destination.last_error = status.last_error.clone();
            destination.outbound_bytes = status.outbound_bytes;
        }
    }

    let active = production
        .destinations
        .iter()
        .filter(|destination| active_ids.contains(&destination.id))
        .collect::<Vec<_>>();
    let forwarding = active
        .iter()
        .filter(|destination| destination.state.as_deref() == Some("forwarding"))
        .count();
    let failed = active
        .iter()
        .filter(|destination| destination.state.as_deref() == Some("error"))
        .count();
    production.forward_state = if active.is_empty() {
        None
    } else if forwarding == active.len() {
        Some("forwarding".into())
    } else if forwarding > 0 {
        Some("partial".into())
    } else if failed == active.len() {
        Some("error".into())
    } else {
        Some("starting".into())
    };
    let errors = active
        .iter()
        .filter_map(|destination| {
            destination
                .last_error
                .as_deref()
                .map(|error| format!("{}: {error}", destination.name))
        })
        .collect::<Vec<_>>();
    production.forward_error = (!errors.is_empty()).then(|| errors.join("; "));
    production.outbound_bytes = active
        .iter()
        .map(|destination| destination.outbound_bytes)
        .sum();
}

fn load_destination_key(id: &str) -> BridgeResult<String> {
    keyring::Entry::new(
        "com.djilivebridge.app.destination",
        &rtmp_keychain_account(id),
    )
    .map_err(|error| BridgeError::Config(format!("Keychain entry: {error}")))?
    .get_password()
    .map_err(|_| BridgeError::Config("Stream key is missing from Keychain".into()))
}

fn store_destination_key(id: &str, key: &str) -> BridgeResult<()> {
    keyring::Entry::new(
        "com.djilivebridge.app.destination",
        &rtmp_keychain_account(id),
    )
    .map_err(|error| BridgeError::Config(format!("Keychain entry: {error}")))?
    .set_password(key)
    .map_err(|error| BridgeError::Config(format!("Keychain save: {error}")))
}

fn migrate_legacy_destination(
    config: &mut AppConfig,
    config_store: &ConfigStore,
) -> BridgeResult<()> {
    let Some(mode) = config.destination_mode else {
        return Ok(());
    };
    if config.rtmp_destinations.is_empty()
        && let Some(server) = config.destination_server.clone()
        && matches!(
            mode,
            DestinationMode::TikTokRtmp | DestinationMode::CustomRtmp
        )
    {
        let (id, name, kind) = match mode {
            DestinationMode::TikTokRtmp => (
                "migrated-tiktok-rtmp".to_string(),
                "TikTok".to_string(),
                RtmpDestinationKind::TikTok,
            ),
            DestinationMode::CustomRtmp => (
                "migrated-custom-rtmp".to_string(),
                "Custom RTMP".to_string(),
                RtmpDestinationKind::Custom,
            ),
            DestinationMode::TikTokLiveStudio => unreachable!(),
        };
        let destination = RtmpDestinationConfig {
            id: id.clone(),
            name,
            kind,
            server,
            enabled: true,
        };
        validate_rtmp_destination_config(&destination)?;
        if let Ok(entry) =
            keyring::Entry::new("com.djilivebridge.app.destination", "rtmp-stream-key")
            && let Ok(key) = entry.get_password()
        {
            store_destination_key(&id, &key)?;
        }
        config.rtmp_destinations.push(destination);
    }
    config.destination_mode = None;
    config.destination_server = None;
    config_store.save(config)?;
    // The legacy single-key entry is copied above; remove it so no orphaned secret remains.
    if let Ok(entry) = keyring::Entry::new("com.djilivebridge.app.destination", "rtmp-stream-key") {
        let _ = entry.delete_credential();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn destination(id: &str, name: &str) -> RtmpDestinationState {
        RtmpDestinationState {
            id: id.into(),
            name: name.into(),
            kind: RtmpDestinationKind::Custom,
            server: format!("rtmps://{id}.example.test/live"),
            enabled: true,
            state: None,
            last_error: None,
            outbound_bytes: 0,
        }
    }

    #[test]
    fn forward_failures_are_isolated_per_destination() {
        let mut production = ProductionState {
            destinations: vec![destination("one", "One"), destination("two", "Two")],
            ..ProductionState::default()
        };
        let ids = vec!["one".to_string(), "two".to_string()];
        let statuses = vec![
            ForwardStatus {
                pos: 0,
                state: Some("forwarding".into()),
                last_error: None,
                outbound_bytes: 1_000,
            },
            ForwardStatus {
                pos: 1,
                state: Some("error".into()),
                last_error: Some("connection refused".into()),
                outbound_bytes: 0,
            },
        ];

        apply_forward_statuses(&mut production, &ids, &statuses);

        assert_eq!(production.forward_state.as_deref(), Some("partial"));
        assert_eq!(production.outbound_bytes, 1_000);
        assert_eq!(
            production.destinations[0].state.as_deref(),
            Some("forwarding")
        );
        assert_eq!(production.destinations[1].state.as_deref(), Some("error"));
        assert!(
            production
                .forward_error
                .as_deref()
                .is_some_and(|error| error.contains("Two"))
        );
    }
}
