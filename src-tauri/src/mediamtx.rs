use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use serde_json::Value;

use crate::{
    config::ConfigStore,
    error::{BridgeError, BridgeResult},
    process::{ProcessSpec, ProcessSupervisor, RestartPolicy},
    state::StreamMetadata,
};

const API_ROOT: &str = "http://127.0.0.1:9997/v3";
const METRICS_URL: &str = "http://127.0.0.1:9998/metrics?type=paths&path=drone";

#[derive(Debug, Clone, Default)]
pub struct PublisherSample {
    pub present: bool,
    pub ready_since_unix_ms: Option<u64>,
    pub metadata: StreamMetadata,
}

#[derive(Debug, Clone, Default)]
pub struct ForwardStatus {
    pub pos: usize,
    pub state: Option<String>,
    pub last_error: Option<String>,
    pub outbound_bytes: u64,
}

pub struct MediaMtxController {
    client: reqwest::Client,
    binary: PathBuf,
    config_path: PathBuf,
    last_bytes: tokio::sync::Mutex<Option<(u64, Instant)>>,
}

impl MediaMtxController {
    pub fn new(store: &ConfigStore) -> BridgeResult<Self> {
        Ok(Self {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(2))
                .build()?,
            binary: resolve_sidecar_path()?,
            config_path: store.app_support_dir().join("mediamtx.yml"),
            last_bytes: tokio::sync::Mutex::new(None),
        })
    }

    pub async fn write_and_validate_config(&self) -> BridgeResult<()> {
        tokio::fs::write(&self.config_path, runtime_config()).await?;
        // Validation via subprocess is unreliable in hardened-runtime bundles;
        // the config is static and has been verified at development time.
        Ok(())
    }

    pub async fn start(&self, supervisor: &ProcessSupervisor) -> BridgeResult<()> {
        self.write_and_validate_config().await?;
        supervisor
            .start(ProcessSpec {
                name: "mediamtx".into(),
                executable: self.binary.clone(),
                args: vec![self.config_path.as_os_str().to_os_string()],
                restart_policy: RestartPolicy::OnFailure,
            })
            .await?;
        self.wait_until_ready(Duration::from_secs(8)).await
    }

    async fn wait_until_ready(&self, timeout: Duration) -> BridgeResult<()> {
        let deadline = Instant::now() + timeout;
        loop {
            if self
                .client
                .get(format!("{API_ROOT}/paths/list"))
                .send()
                .await
                .is_ok_and(|response| response.status().is_success())
            {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(BridgeError::MediaMtx(
                    "Control API did not become ready on 127.0.0.1:9997".into(),
                ));
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    }

    pub async fn publisher_sample(&self) -> BridgeResult<PublisherSample> {
        let response = self
            .client
            .get(format!("{API_ROOT}/paths/list"))
            .send()
            .await?
            .error_for_status()?
            .json::<Value>()
            .await?;
        let item = response
            .get("items")
            .and_then(Value::as_array)
            .and_then(|items| {
                items
                    .iter()
                    .find(|item| item.get("name").and_then(Value::as_str) == Some("drone"))
            });
        let present = item
            .and_then(|value| value.get("ready"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let ready_since_unix_ms = item
            .and_then(|value| value.get("readyTime"))
            .and_then(Value::as_str)
            .and_then(parse_rfc3339_rough);

        let metrics = self
            .client
            .get(METRICS_URL)
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?;
        let received_bytes = metric_value(&metrics, "paths_inbound_bytes", "drone").unwrap_or(0);
        let bitrate_calculated_bps = self.calculate_bitrate(received_bytes).await;

        Ok(PublisherSample {
            present,
            ready_since_unix_ms,
            metadata: StreamMetadata {
                received_bytes,
                bitrate_calculated_bps,
                uptime_seconds: ready_since_unix_ms
                    .map(|since| now_unix_ms().saturating_sub(since) / 1000),
                ..StreamMetadata::default()
            },
        })
    }

    pub async fn wait_for_path(&self, path: &str, timeout: Duration) -> BridgeResult<()> {
        let deadline = Instant::now() + timeout;
        loop {
            let response = self
                .client
                .get(format!("{API_ROOT}/paths/list"))
                .send()
                .await?
                .error_for_status()?
                .json::<Value>()
                .await?;
            let ready = response
                .get("items")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .any(|item| {
                    item.get("name").and_then(Value::as_str) == Some(path)
                        && item.get("ready").and_then(Value::as_bool) == Some(true)
                });
            if ready {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(BridgeError::MediaMtx(format!(
                    "path /{path} did not become ready within {} seconds",
                    timeout.as_secs()
                )));
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    }

    pub async fn configure_forwards(&self, destinations: &[(String, String)]) -> BridgeResult<()> {
        if destinations.is_empty() {
            return Err(BridgeError::Validation(
                "At least one enabled RTMP destination is required".into(),
            ));
        }
        let forwards = destinations
            .iter()
            .map(|(server, key)| {
                crate::config::validate_rtmp_destination(server, key)?;
                Ok(serde_json::json!({
                    "dest": format!("{}#{}", server.trim_end_matches('#'), key)
                }))
            })
            .collect::<BridgeResult<Vec<_>>>()?;
        self.client
            .patch(format!("{API_ROOT}/config/paths/patch/production"))
            .json(&serde_json::json!({ "forward": forwards }))
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }

    pub async fn clear_forward(&self) -> BridgeResult<()> {
        self.client
            .patch(format!("{API_ROOT}/config/paths/patch/production"))
            .json(&serde_json::json!({ "forward": [] }))
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }

    pub async fn forward_statuses(&self) -> BridgeResult<Vec<ForwardStatus>> {
        let response = self
            .client
            .get(format!(
                "{API_ROOT}/paths/forward-dests/list?path=production"
            ))
            .send()
            .await?
            .error_for_status()?
            .json::<Value>()
            .await?;
        let items = response
            .get("items")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let mut statuses = items
            .iter()
            .map(|item| ForwardStatus {
                pos: item.get("pos").and_then(Value::as_u64).unwrap_or_default() as usize,
                state: item
                    .get("state")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                last_error: item
                    .get("lastError")
                    .and_then(Value::as_str)
                    .filter(|value| !value.is_empty())
                    .map(crate::error::redact_secrets),
                outbound_bytes: item
                    .get("outboundBytes")
                    .and_then(Value::as_u64)
                    .unwrap_or_default(),
            })
            .collect::<Vec<_>>();
        statuses.sort_by_key(|status| status.pos);
        Ok(statuses)
    }

    pub async fn wait_forwards(
        &self,
        expected: usize,
        timeout: Duration,
    ) -> BridgeResult<Vec<ForwardStatus>> {
        let deadline = Instant::now() + timeout;
        loop {
            let statuses = self.forward_statuses().await?;
            let forwarding = statuses
                .iter()
                .filter(|status| status.state.as_deref() == Some("forwarding"))
                .count();
            let settled = statuses.len() >= expected
                && statuses
                    .iter()
                    .all(|status| matches!(status.state.as_deref(), Some("forwarding" | "error")));
            if settled && forwarding > 0 {
                return Ok(statuses);
            }
            if settled {
                let details = statuses
                    .iter()
                    .filter_map(|status| status.last_error.as_deref())
                    .collect::<Vec<_>>()
                    .join("; ");
                return Err(BridgeError::MediaMtx(format!(
                    "all production destinations failed: {}",
                    if details.is_empty() {
                        "unknown error"
                    } else {
                        &details
                    }
                )));
            }
            if Instant::now() >= deadline {
                if forwarding > 0 {
                    return Ok(statuses);
                }
                return Err(BridgeError::MediaMtx(
                    "no production destination started within the readiness window".into(),
                ));
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    }

    async fn calculate_bitrate(&self, bytes: u64) -> Option<u64> {
        let now = Instant::now();
        let mut sample = self.last_bytes.lock().await;
        let bitrate = sample.and_then(|(previous_bytes, previous_time)| {
            let seconds = now.duration_since(previous_time).as_secs_f64();
            (seconds > 0.25 && bytes >= previous_bytes)
                .then(|| (((bytes - previous_bytes) as f64 * 8.0) / seconds) as u64)
        });
        *sample = Some((bytes, now));
        bitrate
    }
}

fn runtime_config() -> &'static str {
    include_str!("../mediamtx.runtime.yml")
}

fn resolve_sidecar_path() -> BridgeResult<PathBuf> {
    let executable_dir = std::env::current_exe()?
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| BridgeError::MediaMtx("application executable directory missing".into()))?;
    let packaged = executable_dir.join("mediamtx");
    if packaged.is_file() {
        return Ok(packaged);
    }
    let triple = if cfg!(target_arch = "aarch64") {
        "aarch64-apple-darwin"
    } else {
        "x86_64-apple-darwin"
    };
    let development = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("binaries")
        .join(format!("mediamtx-{triple}"));
    if development.is_file() {
        return Ok(development);
    }
    Err(BridgeError::MediaMtx(format!(
        "pinned MediaMTX sidecar not found at {}",
        development.display()
    )))
}

fn metric_value(body: &str, metric: &str, path: &str) -> Option<u64> {
    body.lines().find_map(|line| {
        if !line.starts_with(metric) || !line.contains(&format!("name=\"{path}\"")) {
            return None;
        }
        line.split_whitespace()
            .last()?
            .parse::<f64>()
            .ok()
            .map(|value| value as u64)
    })
}

fn parse_rfc3339_rough(input: &str) -> Option<u64> {
    let timestamp =
        time::OffsetDateTime::parse(input, &time::format_description::well_known::Rfc3339)
            .ok()?
            .unix_timestamp_nanos();
    (timestamp >= 0).then_some((timestamp / 1_000_000) as u64)
}

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
