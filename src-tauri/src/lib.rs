mod app;
mod audio;
mod config;
mod diagnostics;
mod error;
mod ffmpeg;
mod mediamtx;
mod network;
mod obs;
mod platform;
mod process;
mod recording;
mod state;
mod virtual_camera;

use std::{
    path::PathBuf,
    sync::{Arc, atomic::Ordering},
};

use app::AppState;
use config::{FitMode, OutputLayout, RtmpDestinationInput};
use diagnostics::DiagnosticItem;
use error::ErrorPayload;
use ffmpeg::FfmpegCapabilities;
use ffmpeg::NativeProductionSettings;
use state::BridgeSnapshot;
use tauri::State;

#[tauri::command]
async fn get_snapshot(state: State<'_, Arc<AppState>>) -> Result<BridgeSnapshot, ErrorPayload> {
    Ok(state.state.get().await)
}

#[tauri::command]
async fn select_interface(
    app: tauri::AppHandle,
    state: State<'_, Arc<AppState>>,
    name: String,
) -> Result<(), ErrorPayload> {
    state.select_interface(&app, name).await.map_err(Into::into)
}

#[tauri::command]
async fn start_test_drone(
    state: State<'_, Arc<AppState>>,
    path: String,
) -> Result<(), ErrorPayload> {
    state
        .start_test_drone(PathBuf::from(path))
        .await
        .map_err(Into::into)
}

#[tauri::command]
async fn stop_test_drone(state: State<'_, Arc<AppState>>) -> Result<(), ErrorPayload> {
    state
        .supervisor
        .stop("test-drone")
        .await
        .map_err(Into::into)
}

#[tauri::command]
async fn activate_preview_fallback(
    app: tauri::AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<String, ErrorPayload> {
    state
        .activate_preview_fallback(&app)
        .await
        .map_err(Into::into)
}

#[tauri::command]
async fn stop_preview_fallback(state: State<'_, Arc<AppState>>) -> Result<(), ErrorPayload> {
    state
        .supervisor
        .stop("preview-transcode")
        .await
        .map_err(Into::into)
}

#[tauri::command]
async fn report_preview_status(
    app: tauri::AppHandle,
    state: State<'_, Arc<AppState>>,
    connected: bool,
    detail: Option<String>,
) -> Result<(), ErrorPayload> {
    state.report_preview_status(&app, connected, detail).await;
    Ok(())
}

#[tauri::command]
async fn save_obs_connection(
    state: State<'_, Arc<AppState>>,
    host: String,
    port: u16,
    password: String,
) -> Result<(), ErrorPayload> {
    state.obs_monitoring.store(true, Ordering::Relaxed);
    if host.trim().is_empty() || port == 0 {
        return Err(error::BridgeError::Validation("OBS endpoint is invalid".into()).into());
    }
    state
        .obs
        .store_password(&password)
        .map_err(ErrorPayload::from)?;
    let mut config = state.config.write().await;
    config.obs_host = host;
    config.obs_port = port;
    state.config_store.save(&config).map_err(Into::into)
}

#[tauri::command]
async fn open_obs(state: State<'_, Arc<AppState>>) -> Result<(), ErrorPayload> {
    state.obs_monitoring.store(true, Ordering::Relaxed);
    state.obs.open_or_download().map_err(Into::into)
}

#[tauri::command]
fn set_obs_monitoring(state: State<'_, Arc<AppState>>, active: bool) {
    state.obs_monitoring.store(active, Ordering::Relaxed);
}

/// The only external links the app can open.
const EXTERNAL_LINKS: [(&str, &str); 4] = [
    ("github", "https://github.com/tanerozel/dji-live-bridge"),
    ("linkedin", "https://www.linkedin.com/in/tanerozel"),
    ("email", "mailto:tanerozel47@gmail.com"),
    ("homebrew", "https://brew.sh"),
];

#[tauri::command]
fn open_about_link(target: String) -> Result<(), ErrorPayload> {
    let url = EXTERNAL_LINKS
        .iter()
        .find_map(|(name, url)| (*name == target).then_some(*url))
        .ok_or_else(|| {
            ErrorPayload::from(error::BridgeError::Validation(
                "unknown About link".to_string(),
            ))
        })?;
    platform::open_external(url).map_err(Into::into)
}

#[tauri::command]
fn open_tiktok_live_studio() -> Result<(), ErrorPayload> {
    platform::open_tiktok_live_studio_or_download().map_err(Into::into)
}

#[tauri::command]
async fn prepare_obs(
    app: tauri::AppHandle,
    state: State<'_, Arc<AppState>>,
    layout: OutputLayout,
    fit_mode: FitMode,
) -> Result<(), ErrorPayload> {
    state.obs_monitoring.store(true, Ordering::Relaxed);
    state
        .prepare_obs(&app, layout, fit_mode)
        .await
        .map_err(Into::into)
}

#[tauri::command]
async fn prepare_native_production(
    app: tauri::AppHandle,
    state: State<'_, Arc<AppState>>,
    settings: NativeProductionSettings,
) -> Result<(), ErrorPayload> {
    state
        .prepare_native_production(&app, settings)
        .await
        .map_err(Into::into)
}

#[tauri::command]
async fn set_obs_virtual_camera(
    app: tauri::AppHandle,
    state: State<'_, Arc<AppState>>,
    active: bool,
) -> Result<bool, ErrorPayload> {
    let config = state.config.read().await.clone();
    let active = state
        .obs
        .set_virtual_camera(&config.obs_host, config.obs_port, active)
        .await
        .map_err(ErrorPayload::from)?;
    state
        .state
        .mutate(&app, |snapshot| {
            snapshot.obs.virtual_camera_active = Some(active)
        })
        .await;
    Ok(active)
}

#[tauri::command]
async fn activate_virtual_camera_extension(
    app: tauri::AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<(), ErrorPayload> {
    state
        .activate_virtual_camera_extension(&app)
        .await
        .map_err(Into::into)
}

#[tauri::command]
async fn set_native_virtual_camera(
    app: tauri::AppHandle,
    state: State<'_, Arc<AppState>>,
    active: bool,
) -> Result<(), ErrorPayload> {
    state
        .set_native_virtual_camera(&app, active)
        .await
        .map_err(Into::into)
}

#[tauri::command]
async fn set_recording(
    app: tauri::AppHandle,
    state: State<'_, Arc<AppState>>,
    active: bool,
) -> Result<Option<String>, ErrorPayload> {
    let config = state.config.read().await.clone();
    let output_path = state
        .obs
        .set_recording(&config.obs_host, config.obs_port, active)
        .await
        .map_err(ErrorPayload::from)?;
    state
        .state
        .mutate(&app, |snapshot| {
            snapshot.obs.recording_active = Some(active);
            snapshot.obs.recording_paused = Some(false);
            if output_path.is_some() {
                snapshot.obs.last_recording_path = output_path.clone();
            }
        })
        .await;
    Ok(output_path)
}

#[tauri::command]
async fn set_native_recording(
    app: tauri::AppHandle,
    state: State<'_, Arc<AppState>>,
    active: bool,
) -> Result<Option<String>, ErrorPayload> {
    state
        .set_native_recording(&app, active)
        .await
        .map_err(Into::into)
}

#[tauri::command]
async fn upsert_rtmp_destination(
    app: tauri::AppHandle,
    state: State<'_, Arc<AppState>>,
    destination: RtmpDestinationInput,
) -> Result<String, ErrorPayload> {
    state
        .upsert_rtmp_destination(&app, destination)
        .await
        .map_err(Into::into)
}

#[tauri::command]
async fn remove_rtmp_destination(
    app: tauri::AppHandle,
    state: State<'_, Arc<AppState>>,
    id: String,
) -> Result<(), ErrorPayload> {
    state
        .remove_rtmp_destination(&app, id)
        .await
        .map_err(Into::into)
}

#[tauri::command]
async fn set_rtmp_destination_enabled(
    app: tauri::AppHandle,
    state: State<'_, Arc<AppState>>,
    id: String,
    enabled: bool,
) -> Result<(), ErrorPayload> {
    state
        .set_rtmp_destination_enabled(&app, id, enabled)
        .await
        .map_err(Into::into)
}

#[tauri::command]
async fn start_live(
    app: tauri::AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<(), ErrorPayload> {
    state.start_live(&app).await.map_err(Into::into)
}

#[tauri::command]
async fn stop_live(
    app: tauri::AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<(), ErrorPayload> {
    state.stop_live(&app).await.map_err(Into::into)
}

#[tauri::command]
async fn get_ffmpeg_capabilities() -> Result<FfmpegCapabilities, ErrorPayload> {
    Ok(ffmpeg::capabilities().await)
}

/// True when Homebrew is present, so the UI can offer a one-click install
/// instead of telling the user to open Terminal.
#[tauri::command]
fn has_homebrew() -> bool {
    ffmpeg::locate_homebrew().is_some()
}

#[tauri::command]
async fn install_ffmpeg(app: tauri::AppHandle) -> Result<FfmpegCapabilities, ErrorPayload> {
    ffmpeg::install_via_homebrew(&app).await?;
    Ok(ffmpeg::capabilities().await)
}

#[tauri::command]
async fn get_diagnostics(
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<DiagnosticItem>, ErrorPayload> {
    let snapshot = state.state.get().await;
    let capabilities = ffmpeg::capabilities().await;
    Ok(diagnostics::collect(&snapshot, &capabilities))
}

#[tauri::command]
fn get_audio_routing_notice() -> audio::AudioRoutingNotice {
    audio::routing_notice()
}

#[tauri::command]
fn get_recording_directory() -> Result<String, ErrorPayload> {
    recording::default_recording_dir()
        .map(|path| path.display().to_string())
        .map_err(Into::into)
}

pub fn run() {
    let state = AppState::build().expect("failed to initialize application state");
    let managed_state = state.clone();
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(managed_state)
        .invoke_handler(tauri::generate_handler![
            get_snapshot,
            select_interface,
            start_test_drone,
            stop_test_drone,
            activate_preview_fallback,
            stop_preview_fallback,
            report_preview_status,
            save_obs_connection,
            set_obs_monitoring,
            open_obs,
            open_tiktok_live_studio,
            open_about_link,
            prepare_obs,
            prepare_native_production,
            set_obs_virtual_camera,
            activate_virtual_camera_extension,
            set_native_virtual_camera,
            set_recording,
            set_native_recording,
            upsert_rtmp_destination,
            remove_rtmp_destination,
            set_rtmp_destination_enabled,
            start_live,
            stop_live,
            get_ffmpeg_capabilities,
            has_homebrew,
            install_ffmpeg,
            get_diagnostics,
            get_audio_routing_notice,
            get_recording_directory
        ])
        .setup({
            let state = state.clone();
            move |app| {
                initialize_logging(&state);
                let handle = app.handle().clone();
                virtual_camera::register_app_handle(handle.clone());
                tauri::async_runtime::spawn(state.clone().bootstrap(handle));
                Ok(())
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building DJI Live Bridge");

    app.run(move |app_handle, event| {
        // Quitting from the Dock/Cmd+Q/AppleScript on macOS emits Exit without
        // ExitRequested; handling only the latter orphaned the MediaMTX sidecar.
        if matches!(
            event,
            tauri::RunEvent::ExitRequested { .. } | tauri::RunEvent::Exit
        ) {
            tauri::async_runtime::block_on(state.shutdown(app_handle));
        }
    });
}

fn initialize_logging(state: &AppState) {
    let file_appender =
        tracing_appender::rolling::daily(state.config_store.logs_dir(), "bridge.jsonl");
    let (writer, guard) = tracing_appender::non_blocking(file_appender);
    Box::leak(Box::new(guard));
    let subscriber = tracing_subscriber::fmt()
        .json()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "dji_live_bridge=info".into()),
        )
        .with_writer(writer)
        .finish();
    let _ = tracing::subscriber::set_global_default(subscriber);
}
