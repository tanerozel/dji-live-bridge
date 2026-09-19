use std::{collections::HashSet, sync::Arc};

use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use tokio::{net::TcpStream, sync::Mutex};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async, tungstenite::Message};
use uuid::Uuid;

use crate::{
    config::{BroadcastDestination, FitMode, OutputLayout},
    error::{BridgeError, BridgeResult},
    platform,
    state::ObsState,
};

const KEYCHAIN_SERVICE: &str = "com.djilivebridge.app.obs-websocket";
const KEYCHAIN_ACCOUNT: &str = "obs-websocket-password";
const OBS_DOWNLOAD_URL: &str = "https://obsproject.com/download";

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

#[derive(Clone, Default)]
pub struct ObsController {
    operation_lock: Arc<Mutex<()>>,
    previous_stream_service: Arc<Mutex<Option<Value>>>,
    previous_video_settings: Arc<Mutex<Option<Value>>>,
}

impl ObsController {
    pub fn store_password(&self, password: &str) -> BridgeResult<()> {
        let entry = keyring::Entry::new(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT)
            .map_err(|error| BridgeError::Obs(format!("Keychain entry: {error}")))?;
        if password.is_empty() {
            match entry.delete_credential() {
                Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
                Err(error) => Err(BridgeError::Obs(format!("Keychain delete: {error}"))),
            }
        } else {
            entry
                .set_password(password)
                .map_err(|error| BridgeError::Obs(format!("Keychain save: {error}")))
        }
    }

    pub async fn inspect(&self, host: &str, port: u16) -> BridgeResult<ObsState> {
        let _guard = self.operation_lock.lock().await;
        let mut session = ObsSession::connect(host, port, load_password().as_deref()).await?;
        let version = session.request("GetVersion", json!({})).await?;
        let requests = available_requests(&version);
        let virtual_camera_active = if requests.contains("GetVirtualCamStatus") {
            session
                .request("GetVirtualCamStatus", json!({}))
                .await
                .ok()
                .and_then(|value| value.get("outputActive").and_then(Value::as_bool))
        } else {
            None
        };
        let stream_active = if requests.contains("GetStreamStatus") {
            session
                .request("GetStreamStatus", json!({}))
                .await
                .ok()
                .and_then(|value| value.get("outputActive").and_then(Value::as_bool))
        } else {
            None
        };
        let record_status = if requests.contains("GetRecordStatus") {
            session.request("GetRecordStatus", json!({})).await.ok()
        } else {
            None
        };
        let scene_ready = if requests.contains("GetSceneList") && requests.contains("GetInputList")
        {
            let scenes = session.request("GetSceneList", json!({})).await.ok();
            let inputs = session.request("GetInputList", json!({})).await.ok();
            scenes.is_some_and(|value| {
                value
                    .get("scenes")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .any(|scene| scene.get("sceneName").and_then(Value::as_str) == Some("DJI LIVE"))
            }) && inputs.is_some_and(|value| {
                value
                    .get("inputs")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .any(|input| {
                        input.get("inputName").and_then(Value::as_str) == Some("DJI Drone")
                    })
            })
        } else {
            false
        };
        let mut state = state_from_version(
            version,
            requests,
            virtual_camera_active,
            stream_active,
            record_status,
        );
        state.scene_ready = scene_ready;
        Ok(state)
    }

    pub async fn prepare_scene(
        &self,
        host: &str,
        port: u16,
        layout: OutputLayout,
        fit_mode: FitMode,
    ) -> BridgeResult<ObsState> {
        let _guard = self.operation_lock.lock().await;
        let mut session = ObsSession::connect(host, port, load_password().as_deref()).await?;
        let version = session.request("GetVersion", json!({})).await?;
        let requests = available_requests(&version);
        require_requests(
            &requests,
            &[
                "GetSceneList",
                "CreateScene",
                "GetInputKindList",
                "GetInputDefaultSettings",
                "GetInputList",
                "CreateInput",
                "CreateSceneItem",
                "SetInputSettings",
                "GetSceneItemId",
                "SetSceneItemTransform",
                "GetVideoSettings",
                "SetVideoSettings",
            ],
        )?;

        ensure_scene(&mut session).await?;
        ensure_media_source(&mut session).await?;
        let (canvas_width, canvas_height) = layout.dimensions();
        let current_video_settings = session.request("GetVideoSettings", json!({})).await?;
        {
            let mut previous = self.previous_video_settings.lock().await;
            if previous.is_none() {
                *previous = Some(current_video_settings);
            }
        }
        session
            .request(
                "SetVideoSettings",
                json!({
                    "baseWidth": canvas_width as u64,
                    "baseHeight": canvas_height as u64,
                    "outputWidth": canvas_width as u64,
                    "outputHeight": canvas_height as u64
                }),
            )
            .await?;
        fit_media_source(&mut session, canvas_width, canvas_height, fit_mode).await?;

        let virtual_camera_active = if requests.contains("GetVirtualCamStatus") {
            session
                .request("GetVirtualCamStatus", json!({}))
                .await
                .ok()
                .and_then(|value| value.get("outputActive").and_then(Value::as_bool))
        } else {
            None
        };
        let stream_active = None;
        let mut state = state_from_version(
            version,
            requests,
            virtual_camera_active,
            stream_active,
            None,
        );
        state.scene_ready = true;
        Ok(state)
    }

    pub async fn set_virtual_camera(
        &self,
        host: &str,
        port: u16,
        active: bool,
    ) -> BridgeResult<bool> {
        let _guard = self.operation_lock.lock().await;
        let mut session = ObsSession::connect(host, port, load_password().as_deref()).await?;
        let version = session.request("GetVersion", json!({})).await?;
        let requests = available_requests(&version);
        let request = if active {
            "StartVirtualCam"
        } else {
            "StopVirtualCam"
        };
        require_requests(&requests, &["GetVirtualCamStatus", request])?;
        let current = session.request("GetVirtualCamStatus", json!({})).await?;
        let is_active = current
            .get("outputActive")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if is_active != active {
            session.request(request, json!({})).await?;
        }
        let current = session.request("GetVirtualCamStatus", json!({})).await?;
        Ok(current
            .get("outputActive")
            .and_then(Value::as_bool)
            .unwrap_or(false))
    }

    pub fn open_or_download(&self) -> BridgeResult<()> {
        if platform::obs_installed() {
            platform::open_obs()
        } else {
            platform::open_url(OBS_DOWNLOAD_URL)
        }
    }

    pub async fn start_stream(
        &self,
        host: &str,
        port: u16,
        destination: &BroadcastDestination,
    ) -> BridgeResult<()> {
        let _guard = self.operation_lock.lock().await;
        let (server, key) = destination.rtmp_parts()?;
        let mut session = ObsSession::connect(host, port, load_password().as_deref()).await?;
        let version = session.request("GetVersion", json!({})).await?;
        let requests = available_requests(&version);
        require_requests(
            &requests,
            &[
                "GetStreamStatus",
                "GetStreamServiceSettings",
                "SetStreamServiceSettings",
                "StartStream",
            ],
        )?;
        let status = session.request("GetStreamStatus", json!({})).await?;
        if status
            .get("outputActive")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            return Err(BridgeError::Obs(
                "OBS is already streaming; service settings were not changed".into(),
            ));
        }
        let current = session
            .request("GetStreamServiceSettings", json!({}))
            .await?;
        {
            let mut previous = self.previous_stream_service.lock().await;
            if previous.is_none() {
                *previous = Some(current);
            }
        }
        session
            .request(
                "SetStreamServiceSettings",
                json!({
                    "streamServiceType": "rtmp_custom",
                    "streamServiceSettings": {
                        "server": server,
                        "key": key,
                        "use_auth": false
                    }
                }),
            )
            .await?;
        session.request("StartStream", json!({})).await?;
        Ok(())
    }

    pub async fn stop_stream_and_restore(&self, host: &str, port: u16) -> BridgeResult<()> {
        let _guard = self.operation_lock.lock().await;
        let mut session = ObsSession::connect(host, port, load_password().as_deref()).await?;
        let version = session.request("GetVersion", json!({})).await?;
        let requests = available_requests(&version);
        require_requests(
            &requests,
            &["GetStreamStatus", "StopStream", "SetStreamServiceSettings"],
        )?;
        let status = session.request("GetStreamStatus", json!({})).await?;
        if status
            .get("outputActive")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            session.request("StopStream", json!({})).await?;
        }
        if let Some(previous) = self.previous_stream_service.lock().await.take() {
            let stream_service_type = previous
                .get("streamServiceType")
                .cloned()
                .unwrap_or_else(|| json!("rtmp_custom"));
            let stream_service_settings = previous
                .get("streamServiceSettings")
                .cloned()
                .unwrap_or_else(|| json!({}));
            session
                .request(
                    "SetStreamServiceSettings",
                    json!({
                        "streamServiceType": stream_service_type,
                        "streamServiceSettings": stream_service_settings
                    }),
                )
                .await?;
        }
        Ok(())
    }

    pub async fn restore_video_settings(&self, host: &str, port: u16) -> BridgeResult<()> {
        let _guard = self.operation_lock.lock().await;
        let Some(previous) = self.previous_video_settings.lock().await.take() else {
            return Ok(());
        };
        let mut session = ObsSession::connect(host, port, load_password().as_deref()).await?;
        let version = session.request("GetVersion", json!({})).await?;
        let requests = available_requests(&version);
        require_requests(&requests, &["SetVideoSettings"])?;
        let mut settings = Map::new();
        for key in [
            "fpsNumerator",
            "fpsDenominator",
            "baseWidth",
            "baseHeight",
            "outputWidth",
            "outputHeight",
        ] {
            if let Some(value) = previous.get(key) {
                settings.insert(key.into(), value.clone());
            }
        }
        session
            .request("SetVideoSettings", Value::Object(settings))
            .await?;
        Ok(())
    }

    pub async fn set_recording(
        &self,
        host: &str,
        port: u16,
        active: bool,
    ) -> BridgeResult<Option<String>> {
        let _guard = self.operation_lock.lock().await;
        let mut session = ObsSession::connect(host, port, load_password().as_deref()).await?;
        let version = session.request("GetVersion", json!({})).await?;
        let requests = available_requests(&version);
        let action = if active { "StartRecord" } else { "StopRecord" };
        require_requests(&requests, &["GetRecordStatus", action])?;
        let status = session.request("GetRecordStatus", json!({})).await?;
        let is_active = status
            .get("outputActive")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if is_active == active {
            return Ok(status
                .get("outputPath")
                .and_then(Value::as_str)
                .map(str::to_string));
        }
        let response = session.request(action, json!({})).await?;
        Ok(response
            .get("outputPath")
            .and_then(Value::as_str)
            .map(str::to_string))
    }
}

struct ObsSession {
    socket: Socket,
}

impl ObsSession {
    async fn connect(host: &str, port: u16, password: Option<&str>) -> BridgeResult<Self> {
        if host.trim().is_empty() || port == 0 {
            return Err(BridgeError::Validation("OBS endpoint is invalid".into()));
        }
        let url = format!("ws://{host}:{port}");
        let (mut socket, _) = connect_async(&url)
            .await
            .map_err(|error| BridgeError::Obs(format!("WebSocket connection: {error}")))?;
        let hello = receive_json(&mut socket).await?;
        if hello.get("op").and_then(Value::as_u64) != Some(0) {
            return Err(BridgeError::Obs("OBS did not send protocol Hello".into()));
        }
        let data = hello
            .get("d")
            .and_then(Value::as_object)
            .ok_or_else(|| BridgeError::Obs("OBS Hello payload is invalid".into()))?;
        let rpc_version = data
            .get("rpcVersion")
            .and_then(Value::as_u64)
            .unwrap_or(1)
            .min(1);
        let mut identify = Map::new();
        identify.insert("rpcVersion".into(), json!(rpc_version));
        identify.insert("eventSubscriptions".into(), json!(0));
        if let Some(authentication) = data.get("authentication") {
            let password = password.ok_or_else(|| {
                BridgeError::Obs("OBS WebSocket requires a password; save it to Keychain".into())
            })?;
            let salt = authentication
                .get("salt")
                .and_then(Value::as_str)
                .ok_or_else(|| BridgeError::Obs("OBS auth salt missing".into()))?;
            let challenge = authentication
                .get("challenge")
                .and_then(Value::as_str)
                .ok_or_else(|| BridgeError::Obs("OBS auth challenge missing".into()))?;
            identify.insert(
                "authentication".into(),
                json!(authentication_string(password, salt, challenge)),
            );
        }
        socket
            .send(Message::Text(
                json!({"op": 1, "d": identify}).to_string().into(),
            ))
            .await
            .map_err(|error| BridgeError::Obs(format!("Identify send: {error}")))?;
        let identified = receive_json(&mut socket).await?;
        if identified.get("op").and_then(Value::as_u64) != Some(2) {
            return Err(BridgeError::Obs("OBS authentication was rejected".into()));
        }
        Ok(Self { socket })
    }

    async fn request(&mut self, request_type: &str, data: Value) -> BridgeResult<Value> {
        let request_id = Uuid::new_v4().to_string();
        self.socket
            .send(Message::Text(
                json!({
                    "op": 6,
                    "d": {
                        "requestType": request_type,
                        "requestId": request_id,
                        "requestData": data
                    }
                })
                .to_string()
                .into(),
            ))
            .await
            .map_err(|error| BridgeError::Obs(format!("{request_type}: {error}")))?;
        loop {
            let response = receive_json(&mut self.socket).await?;
            if response.get("op").and_then(Value::as_u64) != Some(7) {
                continue;
            }
            let payload = response
                .get("d")
                .and_then(Value::as_object)
                .ok_or_else(|| {
                    BridgeError::Obs(format!("{request_type}: invalid response payload"))
                })?;
            if payload.get("requestId").and_then(Value::as_str) != Some(&request_id) {
                continue;
            }
            let status = payload
                .get("requestStatus")
                .and_then(Value::as_object)
                .ok_or_else(|| BridgeError::Obs(format!("{request_type}: status missing")))?;
            if !status
                .get("result")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                let code = status.get("code").and_then(Value::as_u64).unwrap_or(0);
                let comment = status
                    .get("comment")
                    .and_then(Value::as_str)
                    .unwrap_or("request failed");
                return Err(BridgeError::Obs(format!(
                    "{request_type} failed ({code}): {comment}"
                )));
            }
            return Ok(payload
                .get("responseData")
                .cloned()
                .unwrap_or_else(|| json!({})));
        }
    }
}

async fn receive_json(socket: &mut Socket) -> BridgeResult<Value> {
    loop {
        let message = socket
            .next()
            .await
            .ok_or_else(|| BridgeError::Obs("WebSocket closed".into()))?
            .map_err(|error| BridgeError::Obs(error.to_string()))?;
        match message {
            Message::Text(text) => return Ok(serde_json::from_str(&text)?),
            Message::Close(frame) => {
                return Err(BridgeError::Obs(format!(
                    "WebSocket closed: {}",
                    frame.map_or_else(|| "no reason".into(), |value| value.reason.to_string())
                )));
            }
            Message::Ping(payload) => {
                socket
                    .send(Message::Pong(payload))
                    .await
                    .map_err(|error| BridgeError::Obs(error.to_string()))?;
            }
            _ => {}
        }
    }
}

fn authentication_string(password: &str, salt: &str, challenge: &str) -> String {
    let secret = BASE64.encode(Sha256::digest(format!("{password}{salt}").as_bytes()));
    BASE64.encode(Sha256::digest(format!("{secret}{challenge}").as_bytes()))
}

fn load_password() -> Option<String> {
    keyring::Entry::new(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT)
        .ok()?
        .get_password()
        .ok()
}

fn available_requests(version: &Value) -> HashSet<String> {
    version
        .get("availableRequests")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect()
}

fn require_requests(requests: &HashSet<String>, required: &[&str]) -> BridgeResult<()> {
    let missing: Vec<_> = required
        .iter()
        .filter(|request| !requests.contains(**request))
        .copied()
        .collect();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(BridgeError::UnsupportedAutomation(format!(
            "OBS does not advertise required request(s): {}",
            missing.join(", ")
        )))
    }
}

async fn ensure_scene(session: &mut ObsSession) -> BridgeResult<()> {
    let scenes = session.request("GetSceneList", json!({})).await?;
    let exists = scenes
        .get("scenes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .any(|scene| scene.get("sceneName").and_then(Value::as_str) == Some("DJI LIVE"));
    if !exists {
        session
            .request("CreateScene", json!({"sceneName": "DJI LIVE"}))
            .await?;
    }
    Ok(())
}

async fn ensure_media_source(session: &mut ObsSession) -> BridgeResult<()> {
    let kinds = session.request("GetInputKindList", json!({})).await?;
    let has_ffmpeg_source = kinds
        .get("inputKinds")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .any(|kind| kind.as_str() == Some("ffmpeg_source"));
    if !has_ffmpeg_source {
        return Err(BridgeError::UnsupportedAutomation(
            "OBS does not advertise the ffmpeg_source input kind".into(),
        ));
    }
    let defaults = session
        .request(
            "GetInputDefaultSettings",
            json!({"inputKind": "ffmpeg_source"}),
        )
        .await?;
    let mut settings = defaults
        .get("defaultInputSettings")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    settings.insert("input".into(), json!("rtsp://127.0.0.1:8554/drone"));
    settings.insert("is_local_file".into(), json!(false));
    settings.insert("restart_on_activate".into(), json!(true));
    settings.insert("close_when_inactive".into(), json!(false));

    let inputs = session
        .request("GetInputList", json!({"inputKind": "ffmpeg_source"}))
        .await?;
    let exists = inputs
        .get("inputs")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .any(|input| input.get("inputName").and_then(Value::as_str) == Some("DJI Drone"));
    if exists {
        session
            .request(
                "SetInputSettings",
                json!({
                    "inputName": "DJI Drone",
                    "inputSettings": settings,
                    "overlay": true
                }),
            )
            .await?;
        let item = session
            .request(
                "GetSceneItemId",
                json!({"sceneName": "DJI LIVE", "sourceName": "DJI Drone"}),
            )
            .await;
        if item.is_err() {
            session
                .request(
                    "CreateSceneItem",
                    json!({
                        "sceneName": "DJI LIVE",
                        "sourceName": "DJI Drone",
                        "sceneItemEnabled": true
                    }),
                )
                .await?;
        }
    } else {
        session
            .request(
                "CreateInput",
                json!({
                    "sceneName": "DJI LIVE",
                    "inputName": "DJI Drone",
                    "inputKind": "ffmpeg_source",
                    "inputSettings": settings,
                    "sceneItemEnabled": true
                }),
            )
            .await?;
    }
    Ok(())
}

async fn fit_media_source(
    session: &mut ObsSession,
    canvas_width: f64,
    canvas_height: f64,
    fit_mode: FitMode,
) -> BridgeResult<()> {
    let item = session
        .request(
            "GetSceneItemId",
            json!({"sceneName": "DJI LIVE", "sourceName": "DJI Drone"}),
        )
        .await?;
    let scene_item_id = item
        .get("sceneItemId")
        .and_then(Value::as_i64)
        .ok_or_else(|| BridgeError::Obs("DJI Drone scene item ID missing".into()))?;
    let bounds_type = match fit_mode {
        FitMode::Fit => "OBS_BOUNDS_SCALE_INNER",
        FitMode::Fill => "OBS_BOUNDS_SCALE_OUTER",
    };
    let transform = json!({
        "positionX": canvas_width / 2.0,
        "positionY": canvas_height / 2.0,
        "alignment": 0,
        "boundsType": bounds_type,
        "boundsAlignment": 0,
        "boundsWidth": canvas_width,
        "boundsHeight": canvas_height,
        "cropLeft": 0,
        "cropRight": 0,
        "cropTop": 0,
        "cropBottom": 0
    });
    session
        .request(
            "SetSceneItemTransform",
            json!({
                "sceneName": "DJI LIVE",
                "sceneItemId": scene_item_id,
                "sceneItemTransform": transform
            }),
        )
        .await?;
    Ok(())
}

fn state_from_version(
    version: Value,
    requests: HashSet<String>,
    virtual_camera_active: Option<bool>,
    stream_active: Option<bool>,
    record_status: Option<Value>,
) -> ObsState {
    let mut available_requests: Vec<_> = requests.into_iter().collect();
    available_requests.sort();
    ObsState {
        installed: platform::obs_installed(),
        running: true,
        connected: true,
        obs_version: version
            .get("obsVersion")
            .and_then(Value::as_str)
            .map(str::to_string),
        websocket_version: version
            .get("obsWebSocketVersion")
            .and_then(Value::as_str)
            .map(str::to_string),
        available_requests,
        virtual_camera_active,
        stream_active,
        recording_active: record_status
            .as_ref()
            .and_then(|value| value.get("outputActive"))
            .and_then(Value::as_bool),
        recording_paused: record_status
            .as_ref()
            .and_then(|value| value.get("outputPaused"))
            .and_then(Value::as_bool),
        last_recording_path: record_status
            .as_ref()
            .and_then(|value| value.get("outputPath"))
            .and_then(Value::as_str)
            .map(str::to_string),
        ..ObsState::default()
    }
}
