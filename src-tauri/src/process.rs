use std::{
    collections::{HashMap, VecDeque},
    ffi::OsString,
    path::PathBuf,
    process::Stdio,
    sync::Arc,
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::{Child, Command},
    sync::Mutex,
};

use crate::error::{BridgeError, BridgeResult, redact_secrets};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RestartPolicy {
    Never,
    OnFailure,
    Always,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProcessStatus {
    Starting,
    Running,
    BackingOff,
    Stopped,
    Failed,
    CrashLoop,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessSnapshot {
    pub name: String,
    pub pid: Option<u32>,
    pub status: ProcessStatus,
    pub restart_policy: RestartPolicy,
    pub restart_count: u32,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ProcessSpec {
    pub name: String,
    pub executable: PathBuf,
    pub args: Vec<OsString>,
    pub restart_policy: RestartPolicy,
}

struct ManagedProcess {
    spec: ProcessSpec,
    child: Option<Child>,
    snapshot: ProcessSnapshot,
    exits: VecDeque<Instant>,
    next_restart: Option<Instant>,
    stopping: bool,
}

#[derive(Clone, Default)]
pub struct ProcessSupervisor {
    processes: Arc<Mutex<HashMap<String, ManagedProcess>>>,
}

impl ProcessSupervisor {
    pub async fn start(&self, spec: ProcessSpec) -> BridgeResult<()> {
        self.stop(&spec.name).await.ok();
        let name = spec.name.clone();
        let restart_policy = spec.restart_policy;
        let child = spawn_child(&spec)?;
        let pid = child.id();
        self.processes.lock().await.insert(
            name.clone(),
            ManagedProcess {
                spec,
                child: Some(child),
                snapshot: ProcessSnapshot {
                    name,
                    pid,
                    status: ProcessStatus::Running,
                    restart_policy,
                    restart_count: 0,
                    last_error: None,
                },
                exits: VecDeque::new(),
                next_restart: None,
                stopping: false,
            },
        );
        Ok(())
    }

    pub async fn tick(&self) -> Vec<ProcessSnapshot> {
        let mut processes = self.processes.lock().await;
        let now = Instant::now();
        for process in processes.values_mut() {
            if let Some(child) = process.child.as_mut() {
                match child.try_wait() {
                    Ok(Some(exit)) => {
                        process.child = None;
                        process.snapshot.pid = None;
                        if process.stopping {
                            process.snapshot.status = ProcessStatus::Stopped;
                            continue;
                        }
                        process.exits.push_back(now);
                        while process
                            .exits
                            .front()
                            .is_some_and(|time| now.duration_since(*time) > Duration::from_secs(60))
                        {
                            process.exits.pop_front();
                        }
                        let failed = !exit.success();
                        process.snapshot.last_error = Some(format!("process exited with {exit}"));
                        let should_restart =
                            matches!(process.spec.restart_policy, RestartPolicy::Always)
                                || (failed
                                    && matches!(
                                        process.spec.restart_policy,
                                        RestartPolicy::OnFailure
                                    ));
                        if process.exits.len() >= 5 {
                            process.snapshot.status = ProcessStatus::CrashLoop;
                            process.next_restart = None;
                        } else if should_restart {
                            let exponent = process.snapshot.restart_count.min(5);
                            let delay = Duration::from_secs(1_u64 << exponent);
                            process.snapshot.status = ProcessStatus::BackingOff;
                            process.next_restart = Some(now + delay);
                        } else {
                            process.snapshot.status = if failed {
                                ProcessStatus::Failed
                            } else {
                                ProcessStatus::Stopped
                            };
                        }
                    }
                    Ok(None) => {}
                    Err(error) => {
                        process.snapshot.status = ProcessStatus::Failed;
                        process.snapshot.last_error = Some(redact_secrets(&error.to_string()));
                    }
                }
            } else if process
                .next_restart
                .is_some_and(|restart_at| restart_at <= now)
            {
                process.snapshot.status = ProcessStatus::Starting;
                match spawn_child(&process.spec) {
                    Ok(child) => {
                        process.snapshot.pid = child.id();
                        process.snapshot.status = ProcessStatus::Running;
                        process.snapshot.restart_count += 1;
                        process.snapshot.last_error = None;
                        process.child = Some(child);
                        process.next_restart = None;
                    }
                    Err(error) => {
                        process.snapshot.status = ProcessStatus::Failed;
                        process.snapshot.last_error = Some(redact_secrets(&error.to_string()));
                    }
                }
            }
        }
        processes
            .values()
            .map(|process| process.snapshot.clone())
            .collect()
    }

    pub async fn stop(&self, name: &str) -> BridgeResult<()> {
        let mut managed = self.processes.lock().await.remove(name);
        if let Some(process) = managed.as_mut() {
            process.stopping = true;
            if let Some(child) = process.child.as_mut() {
                request_stop(child);
                let deadline = Instant::now() + Duration::from_secs(3);
                loop {
                    if child.try_wait()?.is_some() {
                        break;
                    }
                    if Instant::now() >= deadline {
                        child.kill().await?;
                        let _ = child.wait().await;
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
            }
        }
        Ok(())
    }

    /// Why a managed process is no longer running, if it has already exited.
    pub async fn exit_reason(&self, name: &str) -> Option<String> {
        let mut processes = self.processes.lock().await;
        let process = processes.get_mut(name)?;
        match process.child.as_mut() {
            Some(child) => match child.try_wait() {
                Ok(Some(status)) => Some(format!("process exited with {status}")),
                _ => None,
            },
            None => process.snapshot.last_error.clone(),
        }
    }

    pub async fn shutdown_all(&self) {
        let names: Vec<_> = self.processes.lock().await.keys().cloned().collect();
        for name in names {
            if let Err(error) = self.stop(&name).await {
                tracing::warn!(process = %name, %error, "failed to stop process");
            }
        }
    }
}

/// Ask a child to exit on its own. On Unix that is SIGTERM, which lets FFmpeg
/// and MediaMTX flush and close cleanly; Windows has no such signal, so the
/// caller's kill-after-timeout path does the work there.
fn request_stop(child: &mut Child) {
    #[cfg(unix)]
    if let Some(pid) = child.id() {
        use nix::{sys::signal, unistd::Pid};
        let _ = signal::kill(Pid::from_raw(pid as i32), signal::Signal::SIGTERM);
    }
    #[cfg(windows)]
    let _ = child;
}

fn spawn_child(spec: &ProcessSpec) -> BridgeResult<Child> {
    let mut command = Command::new(&spec.executable);
    command
        .args(&spec.args)
        .kill_on_drop(true)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|error| BridgeError::Process(format!("{}: {error}", spec.name)))?;

    if let Some(stdout) = child.stdout.take() {
        let name = spec.name.clone();
        tauri::async_runtime::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                tracing::info!(process = %name, message = %redact_secrets(&line));
            }
        });
    }
    if let Some(stderr) = child.stderr.take() {
        let name = spec.name.clone();
        tauri::async_runtime::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                tracing::warn!(process = %name, message = %redact_secrets(&line));
            }
        });
    }
    Ok(child)
}
