use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use tauri::AppHandle;
use tokio::sync::RwLock;

use crate::{
    audio,
    config::{
        AppConfig, BroadcastDestination, ConfigStore, DestinationMode, FitMode, OutputLayout,
        ProductionEngine, validate_rtmp_destination,
    },
    error::{BridgeError, BridgeResult, ErrorPayload},
    ffmpeg,
    mediamtx::MediaMtxController,
    network,
    obs::ObsController,
    platform::macos,
    process::ProcessSupervisor,
    recording,
    state::{BridgeSnapshot, PreviewMode, ServiceStatus, StateStore, WorkflowState},
    virtual_camera,
};

pub struct AppState {
    pub state: StateStore,
    pub config_store: ConfigStore,
    pub config: RwLock<AppConfig>,
    pub supervisor: ProcessSupervisor,
    pub media_mtx: MediaMtxController,
    pub obs: ObsController,
}

impl AppState {
    pub fn build() -> BridgeResult<Arc<Self>> {
        let config_store = ConfigStore::discover()?;
        let config = config_store.load()?;
        let media_mtx = MediaMtxController::new(&config_store)?;
        Ok(Arc::new(Self {
            state: StateStore::new(),
            config_store,
            config: RwLock::new(config),
            supervisor: ProcessSupervisor::default(),
            media_mtx,
            obs: ObsController::default(),
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
        let mut metadata_check = Instant::now();
        let mut obs_check = Instant::now();
        let mut audio_check = Instant::now() - Duration::from_secs(30);
        let mut production_check = Instant::now();
        let mut virtual_camera_check = Instant::now() - Duration::from_secs(5);
        let mut obs_backoff = Duration::from_secs(2);
        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;
            let processes = self.supervisor.tick().await;
            self.state
                .mutate(&app, |snapshot| snapshot.processes = processes)
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
                            .mutate(&app, |snapshot| snapshot.audio_inputs = devices)
                            .await;
                    }
                    Ok(Err(error)) => tracing::debug!(%error, "audio device discovery failed"),
                    Err(error) => tracing::debug!(%error, "audio device discovery task failed"),
                }
                audio_check = Instant::now();
            }

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
                        metadata_check = Instant::now() - Duration::from_secs(10);
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
                                snapshot.preview.reason = None;
                                snapshot.metadata = Default::default();
                                snapshot.obs.stream_active = Some(false);
                                snapshot.production.active = false;
                                snapshot.production.prepared = false;
                                snapshot.production.path_status = ServiceStatus::Unavailable;
                                snapshot.production.forward_state = None;
                                snapshot.production.recording_active = false;
                                snapshot.virtual_camera.feed_active = false;
                            })
                            .await;
                    }

                    if sample.present && metadata_check.elapsed() >= Duration::from_secs(30) {
                        match ffmpeg::inspect_stream().await {
                            Ok(metadata) => {
                                self.state
                                    .mutate(&app, |snapshot| {
                                        let received = snapshot.metadata.received_bytes;
                                        let calculated = snapshot.metadata.bitrate_calculated_bps;
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

            if production_check.elapsed() >= Duration::from_secs(2) {
                let current = self.state.get().await;
                if current.production.active {
                    match self.media_mtx.forward_status().await {
                        Ok(status) => {
                            self.state
                                .mutate(&app, |snapshot| {
                                    snapshot.production.forward_state = status.state;
                                    snapshot.production.forward_error = status.last_error;
                                    snapshot.production.outbound_bytes = status.outbound_bytes;
                                })
                                .await;
                        }
                        Err(error) => tracing::warn!(%error, "production forward status failed"),
                    }
                }
                production_check = Instant::now();
            }

            if virtual_camera_check.elapsed() >= Duration::from_secs(2) {
                let feed_active = self.state.get().await.processes.iter().any(|process| {
                    process.name == virtual_camera::FEED_PROCESS_NAME
                        && matches!(
                            process.status,
                            crate::process::ProcessStatus::Starting
                                | crate::process::ProcessStatus::Running
                                | crate::process::ProcessStatus::BackingOff
                        )
                });
                let camera = virtual_camera::inspect(feed_active);
                self.state
                    .mutate(&app, |snapshot| snapshot.virtual_camera = camera)
                    .await;
                virtual_camera_check = Instant::now();
            }

            if obs_check.elapsed() >= obs_backoff {
                let config = self.config.read().await.clone();
                match self.obs.inspect(&config.obs_host, config.obs_port).await {
                    Ok(obs) => {
                        self.state.mutate(&app, |snapshot| snapshot.obs = obs).await;
                        obs_backoff = Duration::from_secs(5);
                    }
                    Err(error) => {
                        let installed = macos::obs_installed();
                        let running = macos::obs_running();
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
        let interfaces = network::discover_interfaces()?;
        let preferred = self.config.read().await.selected_interface.clone();
        let selected = network::select_interface(&interfaces, preferred.as_deref()).cloned();
        let before = self.state.get().await.lan_ipv4;
        let selected_name = selected.as_ref().map(|value| value.name.clone());
        let selected_ip = selected.as_ref().map(|value| value.ipv4.clone());
        let rtmp_url = selected_ip
            .as_ref()
            .map(|address| format!("rtmp://{address}:1935/drone"));
        self.state
            .mutate(app, |snapshot| {
                snapshot.interfaces = interfaces;
                snapshot.selected_interface = selected_name;
                snapshot.lan_ipv4 = selected_ip.clone();
                snapshot.rtmp_url = rtmp_url;
                snapshot.ip_change_warning =
                    detect_change && before.is_some() && before != selected_ip;
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
                            state.preview.reason = Some(error.to_string());
                        })
                        .await;
                    return Err(error);
                }
                self.state
                    .mutate(app, |state| {
                        state.preview.mode = PreviewMode::Transcoded;
                        state.preview.status = ServiceStatus::Starting;
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
            })
            .await;
        self.state.transition(app, WorkflowState::Ready).await
    }

    pub async fn configure_destination(
        &self,
        mode: DestinationMode,
        server: Option<String>,
        key: Option<String>,
    ) -> BridgeResult<()> {
        if matches!(mode, DestinationMode::TikTokLiveStudio) {
            let mut config = self.config.write().await;
            config.destination_mode = Some(mode);
            config.destination_server = None;
            return self.config_store.save(&config);
        }
        let server =
            server.ok_or_else(|| BridgeError::Validation("RTMP server is required".into()))?;
        let key = key.ok_or_else(|| BridgeError::Validation("Stream key is required".into()))?;
        validate_rtmp_destination(&server, &key)?;
        let entry = keyring::Entry::new("com.djilivebridge.app.destination", "rtmp-stream-key")
            .map_err(|error| BridgeError::Config(format!("Keychain entry: {error}")))?;
        entry
            .set_password(&key)
            .map_err(|error| BridgeError::Config(format!("Keychain save: {error}")))?;
        let mut config = self.config.write().await;
        config.destination_mode = Some(mode);
        config.destination_server = Some(server);
        self.config_store.save(&config)
    }

    pub async fn activate_virtual_camera_extension(&self, app: &AppHandle) -> BridgeResult<()> {
        virtual_camera::request_activation()?;
        self.state
            .mutate(app, |snapshot| {
                snapshot.virtual_camera.status = ServiceStatus::Starting;
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
            virtual_camera::start_feed(&self.supervisor).await?;
            {
                let mut config = self.config.write().await;
                config.destination_mode = Some(DestinationMode::TikTokLiveStudio);
                self.config_store.save(&config)?;
            }
            self.state
                .mutate(app, |snapshot| {
                    snapshot.virtual_camera.feed_active = true;
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
        let mode = config.destination_mode.ok_or_else(|| {
            BridgeError::Validation(
                "Configure a TikTok RTMP or Custom RTMP destination first".into(),
            )
        })?;
        if matches!(mode, DestinationMode::TikTokLiveStudio) {
            return BroadcastDestination::TikTokLiveStudio
                .rtmp_parts()
                .map(|_| ());
        }
        let server = config
            .destination_server
            .clone()
            .ok_or_else(|| BridgeError::Validation("RTMP server is missing".into()))?;
        let key = keyring::Entry::new("com.djilivebridge.app.destination", "rtmp-stream-key")
            .map_err(|error| BridgeError::Config(format!("Keychain entry: {error}")))?
            .get_password()
            .map_err(|_| BridgeError::Config("Stream key is missing from Keychain".into()))?;
        self.state.transition(app, WorkflowState::GoingLive).await?;
        let result = if config.production_engine == ProductionEngine::NativeFfmpeg {
            self.start_native_live(&config, &server, &key).await
        } else {
            let destination = match mode {
                DestinationMode::TikTokRtmp => BroadcastDestination::TikTokRtmp { server, key },
                DestinationMode::CustomRtmp => BroadcastDestination::CustomRtmp { server, key },
                DestinationMode::TikTokLiveStudio => unreachable!("handled above"),
            };
            self.obs
                .start_stream(&config.obs_host, config.obs_port, &destination)
                .await
                .map(|_| None)
        };
        match result {
            Ok(encoder) => {
                self.state
                    .mutate(app, |snapshot| {
                        if config.production_engine == ProductionEngine::NativeFfmpeg {
                            snapshot.production.active = true;
                            snapshot.production.path_status = ServiceStatus::Ready;
                            snapshot.production.encoder = encoder;
                            snapshot.production.forward_state = Some("forwarding".into());
                        } else {
                            snapshot.obs.stream_active = Some(true);
                        }
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
        server: &str,
        key: &str,
    ) -> BridgeResult<Option<String>> {
        self.media_mtx.configure_forward(server, key).await?;
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
        let has_drone_audio = self.state.get().await.metadata.audio_codec.is_some();
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
        if let Err(error) = self
            .media_mtx
            .wait_forwarding(Duration::from_secs(10))
            .await
        {
            self.supervisor.stop("native-production").await.ok();
            self.media_mtx.clear_forward().await.ok();
            return Err(error);
        }
        Ok(Some(encoder))
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
                self.state
                    .mutate(app, |snapshot| {
                        snapshot.obs.stream_active = Some(false);
                        snapshot.production.active = false;
                        snapshot.production.path_status = ServiceStatus::Unavailable;
                        snapshot.production.forward_state = None;
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
        tracing::error!(code = payload.code, message = %payload.message);
        self.state
            .mutate(app, |snapshot| snapshot.last_error = Some(payload))
            .await;
    }

    pub async fn shutdown(&self, app: &AppHandle) {
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
        let config = self.config.read().await.clone();
        if let Err(error) = self
            .obs
            .restore_video_settings(&config.obs_host, config.obs_port)
            .await
        {
            tracing::debug!(%error, "OBS video settings were not restored");
        }
        self.supervisor.shutdown_all().await;
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
