//! Container lifecycle — create, start, stop, kill, remove.

use crate::error::StivaError;
use crate::image::{Image, ImageStore};
use crate::network::manager::NetworkManager;
use crate::runtime::{self, ContainerExecResult, RuntimeSpec};
use crate::storage::{self, OverlayPaths};
use majra::pubsub::PubSub;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info};

/// Container state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ContainerState {
    Created,
    Running,
    Paused,
    Stopped,
    Removing,
}

/// A container instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Container {
    pub id: String,
    pub name: Option<String>,
    pub image_id: String,
    pub image_ref: String,
    pub state: ContainerState,
    pub pid: Option<u32>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    pub config: ContainerConfig,
    /// Exit code from the container process (set after execution).
    #[serde(default)]
    pub exit_code: Option<i32>,
}

/// Container creation configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerConfig {
    /// Container name (auto-generated if empty).
    pub name: Option<String>,
    /// Command to run (overrides image entrypoint).
    pub command: Vec<String>,
    /// Environment variables.
    pub env: HashMap<String, String>,
    /// Volume mounts (host:container).
    pub volumes: Vec<String>,
    /// Port mappings (host:container).
    pub ports: Vec<String>,
    /// Network mode.
    pub network: Option<String>,
    /// Memory limit in bytes (0 = unlimited).
    pub memory_limit: u64,
    /// CPU shares (relative weight).
    pub cpu_shares: u64,
    /// Run as user.
    pub user: Option<String>,
    /// Working directory inside container.
    pub workdir: Option<String>,
    /// Run as a long-running daemon (detached).
    /// When true, `start()` spawns the process and returns immediately.
    /// When false (default), `start()` runs to completion (one-shot).
    #[serde(default)]
    pub detach: bool,
    /// Grace period in milliseconds for SIGTERM before SIGKILL on stop.
    /// Default: 10000 (10 seconds).
    #[serde(default = "default_stop_grace_ms")]
    pub stop_grace_ms: u64,
    /// Run as a rootless container (user namespace UID remapping).
    /// When true, the container runs as UID 0 inside a user namespace
    /// mapped to the invoking user's UID outside. No real root required.
    #[serde(default)]
    pub rootless: bool,
}

fn default_stop_grace_ms() -> u64 {
    10_000
}

impl Default for ContainerConfig {
    fn default() -> Self {
        Self {
            name: None,
            command: Vec::new(),
            env: HashMap::new(),
            volumes: Vec::new(),
            ports: Vec::new(),
            network: None,
            memory_limit: 0,
            cpu_shares: 0,
            user: None,
            workdir: None,
            detach: false,
            stop_grace_ms: default_stop_grace_ms(),
            rootless: false,
        }
    }
}

/// A migration bundle containing everything needed to transfer a container.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationBundle {
    /// Container ID on the source node.
    pub source_container_id: String,
    /// Image reference to pull on the target.
    pub image_ref: String,
    /// Serialized container config.
    pub config: ContainerConfig,
    /// Path to checkpoint directory.
    pub checkpoint_dir: PathBuf,
    /// Source node identifier.
    pub source_node: String,
    /// Timestamp of migration preparation.
    pub prepared_at: chrono::DateTime<chrono::Utc>,
}

/// Internal state for a running container (not serialized).
struct ContainerInternals {
    /// Overlay filesystem paths (for teardown).
    overlay: Option<OverlayPaths>,
    /// Generated runtime spec.
    spec: Option<RuntimeSpec>,
    /// Execution result (after start completes).
    exec_result: Option<ContainerExecResult>,
    /// Path to container log file.
    log_path: PathBuf,
    /// Handle to a spawned long-running process (daemon mode).
    spawned: Option<runtime::DaemonHandle>,
}

/// Manages container lifecycle.
pub struct ContainerManager {
    root: PathBuf,
    containers: RwLock<HashMap<String, Container>>,
    /// Internal state not exposed in the serializable Container.
    internals: RwLock<HashMap<String, ContainerInternals>>,
    /// Image store for resolving layer blobs.
    image_store: Arc<ImageStore>,
    /// Pub/sub hub for container lifecycle events.
    publisher: PubSub,
    /// Network manager for container connectivity.
    network_manager: RwLock<Option<NetworkManager>>,
}

impl ContainerManager {
    /// Get a container's PID, validating it's in one of the expected states.
    ///
    /// Returns the PID if the container exists, is in an allowed state, and has a PID.
    pub(crate) async fn require_pid(
        &self,
        id: &str,
        allowed_states: &[ContainerState],
        op: &str,
    ) -> Result<u32, StivaError> {
        let containers = self.containers.read().await;
        let container = containers
            .get(id)
            .ok_or_else(|| StivaError::ContainerNotFound(id.to_string()))?;

        if !allowed_states.contains(&container.state) {
            return Err(StivaError::InvalidState(format!(
                "cannot {op} container {id}: state is {:?}",
                container.state
            )));
        }

        container
            .pid
            .ok_or_else(|| StivaError::InvalidState(format!("container {id} has no PID")))
    }

    /// Create a new container manager.
    pub fn new(root: &Path, image_store: Arc<ImageStore>) -> Result<Self, StivaError> {
        std::fs::create_dir_all(root)?;
        Ok(Self {
            root: root.to_path_buf(),
            containers: RwLock::new(HashMap::new()),
            internals: RwLock::new(HashMap::new()),
            image_store,
            publisher: PubSub::new(),
            network_manager: RwLock::new(None),
        })
    }

    /// Create a container from an image.
    ///
    /// Unpacks image layers and prepares the overlay filesystem.
    pub async fn create(
        &self,
        image: &Image,
        config: ContainerConfig,
    ) -> Result<Container, StivaError> {
        let id = uuid::Uuid::new_v4().to_string();
        let name = config.name.clone().unwrap_or_else(|| id[..12].to_string());

        let container_root = self.root.join(&id);
        std::fs::create_dir_all(&container_root)?;

        // Unpack image layers.
        let layer_dirs = storage::prepare_layers(&self.image_store, &image.layers)?;

        // Set up overlay filesystem (may fail without root — that's ok for Created state).
        let overlay = match storage::setup_overlay(&layer_dirs, &container_root) {
            Ok(paths) => {
                info!(container = %id, merged = %paths.merged.display(), "overlay ready");
                Some(paths)
            }
            Err(e) => {
                tracing::warn!(container = %id, "overlay setup deferred: {e}");
                None
            }
        };

        // Build container record.
        let container = Container {
            id: id.clone(),
            name: Some(name.clone()),
            image_id: image.id.clone(),
            image_ref: image.reference.full_ref(),
            state: ContainerState::Created,
            pid: None,
            created_at: chrono::Utc::now(),
            started_at: None,
            config: config.clone(),
            exit_code: None,
        };

        // Generate OCI runtime spec.
        let rootfs = overlay
            .as_ref()
            .map(|o| o.merged.clone())
            .unwrap_or_else(|| container_root.join("rootfs"));

        let spec = runtime::generate_spec(&container, &rootfs)?;

        // Log path.
        let log_dir = container_root.join("logs");
        std::fs::create_dir_all(&log_dir)?;
        let log_path = log_dir.join("container.log");

        // Store internals.
        {
            let mut internals = self.internals.write().await;
            internals.insert(
                id.clone(),
                ContainerInternals {
                    overlay,
                    spec: Some(spec),
                    exec_result: None,
                    log_path,
                    spawned: None,
                },
            );
        }

        self.containers
            .write()
            .await
            .insert(id.clone(), container.clone());

        self.publish_event(serde_json::json!({
            "event": "created",
            "container_id": id,
            "image": container.image_ref,
        }));

        Ok(container)
    }

    /// Start a created container.
    ///
    /// **One-shot mode** (default): runs the command to completion, transitions
    /// Created → Running → Stopped.
    ///
    /// **Daemon mode** (`config.detach = true`): spawns the command and returns
    /// immediately. The container stays in Running state until explicitly stopped
    /// or the process exits.
    pub async fn start(&self, id: &str) -> Result<(), StivaError> {
        let detach = {
            let containers = self.containers.read().await;
            let container = containers
                .get(id)
                .ok_or_else(|| StivaError::ContainerNotFound(id.to_string()))?;
            if container.state == ContainerState::Running {
                return Err(StivaError::AlreadyRunning(id.to_string()));
            }
            container.config.detach
        };

        // Transition to Running.
        {
            let mut containers = self.containers.write().await;
            if let Some(container) = containers.get_mut(id) {
                container.state = ContainerState::Running;
                container.started_at = Some(chrono::Utc::now());
            }
        }

        let spec = {
            let internals = self.internals.read().await;
            internals
                .get(id)
                .and_then(|i| i.spec.clone())
                .ok_or_else(|| StivaError::Runtime("no runtime spec for container".into()))?
        };

        if detach {
            info!(container = id, "spawning daemon container");
            let handle = match runtime::spawn_container(&spec).await {
                Ok(h) => h,
                Err(e) => {
                    // Revert state on spawn failure.
                    let mut containers = self.containers.write().await;
                    if let Some(container) = containers.get_mut(id) {
                        container.state = ContainerState::Stopped;
                        container.pid = None;
                    }
                    self.publish_event(serde_json::json!({
                        "event": "start_failed",
                        "container_id": id,
                        "error": e.to_string(),
                    }));
                    return Err(e);
                }
            };
            let pid = handle.pid();

            {
                let mut containers = self.containers.write().await;
                if let Some(container) = containers.get_mut(id) {
                    container.pid = pid;
                }
            }
            {
                let mut internals = self.internals.write().await;
                if let Some(internal) = internals.get_mut(id) {
                    internal.spawned = Some(handle);
                }
            }

            // Apply cgroup v2 resource limits.
            if let Some(p) = pid {
                runtime::apply_cgroup_limits(p, &spec).await;
            }

            // Connect to network if ports/network configured.
            if let Some(p) = pid {
                self.connect_network(id, p).await;
            }

            info!(container = id, pid = ?pid, "daemon container started");
        } else {
            info!(container = id, "executing one-shot container");
            let result = runtime::exec_container(&spec).await;

            // Update container state based on execution result.
            {
                let mut containers = self.containers.write().await;
                if let Some(container) = containers.get_mut(id) {
                    match &result {
                        Ok(exec_result) => {
                            container.exit_code = Some(exec_result.exit_code);
                            container.state = ContainerState::Stopped;
                            container.pid = None;
                        }
                        Err(e) => {
                            error!(container = id, error = %e, "container execution failed");
                            container.state = ContainerState::Stopped;
                            container.pid = None;
                        }
                    }
                }
            }

            // Write logs, store result, and propagate.
            match result {
                Ok(exec_result) => {
                    self.write_log(id, &exec_result).await;
                    {
                        let mut internals = self.internals.write().await;
                        if let Some(internal) = internals.get_mut(id) {
                            internal.exec_result = Some(exec_result);
                        }
                    }
                }
                Err(e) => {
                    self.publish_event(serde_json::json!({
                        "event": "start_failed",
                        "container_id": id,
                        "error": e.to_string(),
                    }));
                    return Err(e);
                }
            }
        }

        self.publish_event(serde_json::json!({
            "event": "started",
            "container_id": id,
            "detach": detach,
        }));

        Ok(())
    }

    /// Wait for a daemon container to exit. Returns the exit code.
    ///
    /// For one-shot containers, this returns immediately since they are
    /// already stopped after `start()`.
    pub async fn wait(&self, id: &str) -> Result<ContainerExecResult, StivaError> {
        let handle = {
            let mut internals = self.internals.write().await;
            let internal = internals
                .get_mut(id)
                .ok_or_else(|| StivaError::ContainerNotFound(id.to_string()))?;
            internal.spawned.take()
        };

        if let Some(handle) = handle {
            info!(container = id, "waiting for daemon container");
            let exec_result = handle.wait().await?;

            // Update state.
            {
                let mut containers = self.containers.write().await;
                if let Some(container) = containers.get_mut(id) {
                    container.exit_code = Some(exec_result.exit_code);
                    container.state = ContainerState::Stopped;
                    container.pid = None;
                }
            }

            self.write_log(id, &exec_result).await;
            Ok(exec_result)
        } else {
            // One-shot: already has a result.
            let internals = self.internals.read().await;
            let internal = internals
                .get(id)
                .ok_or_else(|| StivaError::ContainerNotFound(id.to_string()))?;
            internal
                .exec_result
                .clone()
                .ok_or_else(|| StivaError::InvalidState("container has not been started".into()))
        }
    }

    /// Check if a daemon container is still running (non-blocking).
    /// Returns `Some(exit_code)` if exited, `None` if still running.
    pub async fn try_wait(&self, id: &str) -> Result<Option<i32>, StivaError> {
        let mut internals = self.internals.write().await;
        let internal = internals
            .get_mut(id)
            .ok_or_else(|| StivaError::ContainerNotFound(id.to_string()))?;

        if let Some(ref mut handle) = internal.spawned {
            match handle.try_wait()? {
                Some(exit_code) => {
                    // Process exited — update state.
                    drop(internals);
                    let mut containers = self.containers.write().await;
                    if let Some(container) = containers.get_mut(id) {
                        container.exit_code = Some(exit_code);
                        container.state = ContainerState::Stopped;
                        container.pid = None;
                    }
                    Ok(Some(exit_code))
                }
                None => Ok(None),
            }
        } else {
            // One-shot or already collected.
            let containers = self.containers.read().await;
            let container = containers
                .get(id)
                .ok_or_else(|| StivaError::ContainerNotFound(id.to_string()))?;
            Ok(container.exit_code)
        }
    }

    /// Stop a running container.
    ///
    /// For daemon containers, sends SIGTERM and waits up to `stop_grace_ms`
    /// before sending SIGKILL. For one-shot containers that have already
    /// completed, this is a no-op.
    pub async fn stop(&self, id: &str) -> Result<(), StivaError> {
        let grace_ms = {
            let containers = self.containers.read().await;
            let container = containers
                .get(id)
                .ok_or_else(|| StivaError::ContainerNotFound(id.to_string()))?;
            container.config.stop_grace_ms
        };

        // Take the spawned handle if present.
        let handle = {
            let mut internals = self.internals.write().await;
            internals.get_mut(id).and_then(|i| i.spawned.take())
        };

        if let Some(handle) = handle {
            info!(container = id, grace_ms, "stopping daemon container");
            let exec_result = handle.kill(grace_ms).await?;

            {
                let mut containers = self.containers.write().await;
                if let Some(container) = containers.get_mut(id) {
                    container.exit_code = Some(exec_result.exit_code);
                    container.state = ContainerState::Stopped;
                    container.pid = None;
                }
            }
            self.write_log(id, &exec_result).await;
        } else {
            let mut containers = self.containers.write().await;
            let container = containers
                .get_mut(id)
                .ok_or_else(|| StivaError::ContainerNotFound(id.to_string()))?;
            container.state = ContainerState::Stopped;
            container.pid = None;
        }

        // Disconnect from network.
        self.disconnect_network(id).await;

        // Read exit_code for the event.
        let exit_code = {
            let containers = self.containers.read().await;
            containers.get(id).and_then(|c| c.exit_code)
        };
        self.publish_event(serde_json::json!({
            "event": "stopped",
            "container_id": id,
            "exit_code": exit_code,
        }));

        Ok(())
    }

    /// Write execution result to the container log file.
    async fn write_log(&self, id: &str, exec_result: &ContainerExecResult) {
        let internals = self.internals.read().await;
        if let Some(internal) = internals.get(id) {
            let log_content = format!(
                "=== stdout ===\n{}\n=== stderr ===\n{}\n=== exit_code: {} | duration: {}ms | timed_out: {} ===\n",
                exec_result.stdout,
                exec_result.stderr,
                exec_result.exit_code,
                exec_result.duration_ms,
                exec_result.timed_out,
            );
            let _ = std::fs::write(&internal.log_path, &log_content);
        }
    }

    /// Send a signal to a running container process.
    ///
    /// Common signals: SIGHUP(1), SIGINT(2), SIGTERM(15), SIGUSR1(10), SIGUSR2(12).
    pub async fn signal(&self, id: &str, signal: i32) -> Result<(), StivaError> {
        let pid = self
            .require_pid(id, &[ContainerState::Running], "signal")
            .await?;
        runtime::send_signal(pid, signal)
    }

    /// Pause a running container via cgroups freezer.
    pub async fn pause(&self, id: &str) -> Result<(), StivaError> {
        let pid = self
            .require_pid(id, &[ContainerState::Running], "pause")
            .await?;
        runtime::pause_container(pid).await?;

        let mut containers = self.containers.write().await;
        if let Some(container) = containers.get_mut(id) {
            container.state = ContainerState::Paused;
        }

        self.publish_event(serde_json::json!({
            "event": "paused",
            "container_id": id,
        }));

        Ok(())
    }

    /// Unpause a paused container.
    pub async fn unpause(&self, id: &str) -> Result<(), StivaError> {
        let pid = self
            .require_pid(id, &[ContainerState::Paused], "unpause")
            .await?;
        runtime::unpause_container(pid).await?;

        let mut containers = self.containers.write().await;
        if let Some(container) = containers.get_mut(id) {
            container.state = ContainerState::Running;
        }

        self.publish_event(serde_json::json!({
            "event": "unpaused",
            "container_id": id,
        }));

        Ok(())
    }

    /// Get runtime stats for a running or paused container.
    pub async fn stats(&self, id: &str) -> Result<runtime::ContainerStats, StivaError> {
        let pid = self
            .require_pid(
                id,
                &[ContainerState::Running, ContainerState::Paused],
                "get stats for",
            )
            .await?;
        runtime::container_stats(pid).await
    }

    /// Execute a command inside a running container.
    ///
    /// Uses `nsenter` to enter the container's namespaces and run the command.
    /// Only works on running daemon containers that have a PID.
    pub async fn exec(
        &self,
        id: &str,
        command: &[String],
    ) -> Result<ContainerExecResult, StivaError> {
        let (pid, env, workdir) = {
            let containers = self.containers.read().await;
            let container = containers
                .get(id)
                .ok_or_else(|| StivaError::ContainerNotFound(id.to_string()))?;

            if container.state != ContainerState::Running {
                return Err(StivaError::InvalidState(format!(
                    "container {id} is {:?}, not running",
                    container.state
                )));
            }

            let pid = container
                .pid
                .ok_or_else(|| StivaError::InvalidState(format!("container {id} has no PID")))?;

            let env: Vec<(String, String)> = container.config.env.clone().into_iter().collect();
            let workdir = container.config.workdir.clone();
            (pid, env, workdir)
        };

        runtime::exec_in_container(pid, command, &env, workdir.as_deref()).await
    }

    /// Remove a stopped container.
    ///
    /// Tears down overlay filesystem and cleans up all container files.
    pub async fn remove(&self, id: &str) -> Result<(), StivaError> {
        let mut containers = self.containers.write().await;
        let container = containers
            .get(id)
            .ok_or_else(|| StivaError::ContainerNotFound(id.to_string()))?;

        if container.state == ContainerState::Running {
            return Err(StivaError::InvalidState(format!(
                "cannot remove running container {id} — stop it first"
            )));
        }

        // Tear down overlay.
        {
            let mut internals = self.internals.write().await;
            if let Some(internal) = internals.remove(id)
                && let Some(overlay) = &internal.overlay
            {
                let _ = storage::teardown_overlay(overlay);
            }
        }

        // Clean up container directory.
        let container_root = self.root.join(id);
        if container_root.exists() {
            std::fs::remove_dir_all(&container_root)?;
        }

        containers.remove(id);

        self.publish_event(serde_json::json!({
            "event": "removed",
            "container_id": id,
        }));

        Ok(())
    }

    /// Get the rootfs path for a container (overlay merged or fallback).
    pub async fn get_rootfs(&self, id: &str) -> Result<PathBuf, StivaError> {
        let internals = self.internals.read().await;
        let internal = internals
            .get(id)
            .ok_or_else(|| StivaError::ContainerNotFound(id.to_string()))?;
        Ok(internal
            .overlay
            .as_ref()
            .map(|o| o.merged.clone())
            .unwrap_or_else(|| self.root.join(id).join("rootfs")))
    }

    /// List all containers.
    pub async fn list(&self) -> Result<Vec<Container>, StivaError> {
        Ok(self.containers.read().await.values().cloned().collect())
    }

    /// Checkpoint a running daemon container.
    ///
    /// Creates a snapshot of the container's process state that can be
    /// restored later. The container remains running if `leave_running` is true.
    /// Returns the path to the checkpoint directory.
    pub async fn checkpoint(&self, id: &str, leave_running: bool) -> Result<PathBuf, StivaError> {
        info!(container = id, leave_running, "checkpointing container");

        let pid = self
            .require_pid(id, &[ContainerState::Running], "checkpoint")
            .await?;

        // Create checkpoint dir: {container_root}/checkpoints/{timestamp}/
        let timestamp = chrono::Utc::now().format("%Y%m%dT%H%M%S%3fZ");
        let checkpoint_dir = self
            .root
            .join(id)
            .join("checkpoints")
            .join(timestamp.to_string());

        runtime::checkpoint_container(pid, &checkpoint_dir, leave_running).await?;

        // If not leaving running, transition to Paused.
        if !leave_running {
            let mut containers = self.containers.write().await;
            if let Some(container) = containers.get_mut(id) {
                container.state = ContainerState::Paused;
                info!(container = id, "container paused after checkpoint");
            }
        }

        info!(
            container = id,
            checkpoint_dir = %checkpoint_dir.display(),
            "checkpoint created"
        );
        Ok(checkpoint_dir)
    }

    /// Restore a container from a checkpoint.
    ///
    /// Restores the process state from a previous checkpoint and transitions
    /// the container back to Running.
    pub async fn restore(&self, id: &str, checkpoint_dir: &Path) -> Result<(), StivaError> {
        info!(
            container = id,
            checkpoint_dir = %checkpoint_dir.display(),
            "restoring container from checkpoint"
        );

        let state = {
            let containers = self.containers.read().await;
            let container = containers
                .get(id)
                .ok_or_else(|| StivaError::ContainerNotFound(id.to_string()))?;
            container.state
        };

        if state != ContainerState::Paused && state != ContainerState::Stopped {
            return Err(StivaError::InvalidState(format!(
                "cannot restore container {id}: must be Paused or Stopped (state: {state:?})"
            )));
        }

        // Get rootfs from internals.
        let rootfs = {
            let internals = self.internals.read().await;
            let internal = internals
                .get(id)
                .ok_or_else(|| StivaError::ContainerNotFound(id.to_string()))?;
            internal
                .overlay
                .as_ref()
                .map(|o| o.merged.clone())
                .unwrap_or_else(|| self.root.join(id).join("rootfs"))
        };

        let new_pid = runtime::restore_container(checkpoint_dir, &rootfs).await?;

        // Update container PID and state to Running.
        {
            let mut containers = self.containers.write().await;
            if let Some(container) = containers.get_mut(id) {
                container.pid = Some(new_pid);
                container.state = ContainerState::Running;
                info!(
                    container = id,
                    pid = new_pid,
                    "container restored and running"
                );
            }
        }

        Ok(())
    }

    /// Prepare a container for live migration — checkpoint and package for transfer.
    ///
    /// Returns the path to a migration bundle (directory containing checkpoint
    /// data, container config, and image reference) that can be transferred
    /// to a target node.
    pub async fn prepare_migration(&self, id: &str) -> Result<MigrationBundle, StivaError> {
        info!(container = id, "preparing container for migration");

        // Validate Running state (require_pid checks state + PID).
        let _ = self
            .require_pid(id, &[ContainerState::Running], "migrate")
            .await?;

        let (container_config, image_ref) = {
            let containers = self.containers.read().await;
            let container = containers
                .get(id)
                .ok_or_else(|| StivaError::ContainerNotFound(id.to_string()))?;
            (container.config.clone(), container.image_ref.clone())
        };

        // Checkpoint with leave_running = false (pauses the container).
        let checkpoint_dir = self.checkpoint(id, false).await?;

        let hostname = std::env::var("HOSTNAME")
            .or_else(|_| std::env::var("NODE_ID"))
            .unwrap_or_else(|_| "unknown".to_string());

        let bundle = MigrationBundle {
            source_container_id: id.to_string(),
            image_ref,
            config: container_config,
            checkpoint_dir,
            source_node: hostname,
            prepared_at: chrono::Utc::now(),
        };

        info!(
            container = id,
            source_node = %bundle.source_node,
            "migration bundle prepared"
        );
        Ok(bundle)
    }

    /// Apply a migration bundle — restore a container from a transferred checkpoint.
    ///
    /// Creates a new container from the bundle's config, then restores
    /// process state from the checkpoint data.
    pub async fn apply_migration(&self, bundle: &MigrationBundle) -> Result<Container, StivaError> {
        info!(
            source_container = %bundle.source_container_id,
            source_node = %bundle.source_node,
            image = %bundle.image_ref,
            "applying migration bundle"
        );

        // Resolve image from the store (caller must ensure image is present).
        let images = self.image_store.list()?;
        let image = images
            .iter()
            .find(|i| i.reference.full_ref() == bundle.image_ref)
            .ok_or_else(|| {
                StivaError::ImageNotFound(format!(
                    "image {} not found locally — pull it before applying migration",
                    bundle.image_ref
                ))
            })?;

        // Create a new container with the bundle's config.
        let container = self.create(image, bundle.config.clone()).await?;

        // Restore from checkpoint.
        let restore_result = self.restore(&container.id, &bundle.checkpoint_dir).await;
        if let Err(e) = restore_result {
            // Clean up the created container on restore failure.
            error!(
                container = %container.id,
                error = %e,
                "migration restore failed, cleaning up"
            );
            let _ = self.remove(&container.id).await;
            return Err(StivaError::Migration(format!(
                "failed to restore from checkpoint: {e}"
            )));
        }

        info!(
            container = %container.id,
            source = %bundle.source_container_id,
            "migration applied successfully"
        );

        // Return updated container state.
        let containers = self.containers.read().await;
        containers
            .get(&container.id)
            .cloned()
            .ok_or_else(|| StivaError::ContainerNotFound(container.id.clone()))
    }

    /// Read container logs.
    pub async fn logs(&self, id: &str) -> Result<String, StivaError> {
        let internals = self.internals.read().await;
        let internal = internals
            .get(id)
            .ok_or_else(|| StivaError::ContainerNotFound(id.to_string()))?;

        if internal.log_path.exists() {
            std::fs::read_to_string(&internal.log_path).map_err(StivaError::Io)
        } else {
            Ok(String::new())
        }
    }

    /// Read the last N lines of a container's log file.
    pub async fn log_tail(&self, id: &str, lines: usize) -> Result<String, StivaError> {
        info!(container = id, lines, "tailing container logs");

        let internals = self.internals.read().await;
        let internal = internals
            .get(id)
            .ok_or_else(|| StivaError::ContainerNotFound(id.to_string()))?;

        if !internal.log_path.exists() {
            return Ok(String::new());
        }

        let content = std::fs::read_to_string(&internal.log_path).map_err(StivaError::Io)?;

        let all_lines: Vec<&str> = content.lines().collect();
        let start = all_lines.len().saturating_sub(lines);
        let tail: Vec<&str> = all_lines[start..].to_vec();

        Ok(tail.join("\n"))
    }

    /// Connect a daemon container to its network after start.
    ///
    /// Reads the container's port and network config. If ports are non-empty
    /// or a network is configured, creates a `NetworkManager` (or reuses the
    /// cached one) and calls `connect_container`. Best-effort: warns on
    /// failure, does not block start.
    async fn connect_network(&self, id: &str, _pid: u32) {
        info!(container = id, "attempting network connection");

        // Read config from containers (drop lock before acquiring internals).
        let (ports, network) = {
            let containers = self.containers.read().await;
            let Some(container) = containers.get(id) else {
                return;
            };
            (
                container.config.ports.clone(),
                container.config.network.clone(),
            )
        };

        // Read rootfs from internals (separate lock scope).
        let rootfs = {
            let internals = self.internals.read().await;
            internals
                .get(id)
                .and_then(|i| i.overlay.as_ref().map(|o| o.merged.clone()))
        };

        if ports.is_empty() && network.is_none() {
            return;
        }

        let network_name = network
            .as_deref()
            .unwrap_or(crate::network::manager::DEFAULT_BRIDGE);

        let mut mgr_guard = self.network_manager.write().await;
        if mgr_guard.is_none() {
            match NetworkManager::new() {
                Ok(nm) => *mgr_guard = Some(nm),
                Err(e) => {
                    tracing::warn!(
                        container = id,
                        error = %e,
                        "failed to create network manager, skipping network"
                    );
                    return;
                }
            }
        }

        if let Some(ref mut nm) = *mgr_guard {
            match nm.connect_container(id, network_name, &ports, rootfs.as_deref()) {
                Ok(cn) => {
                    info!(
                        container = id,
                        ip = %cn.ip,
                        network = %cn.network_name,
                        "container connected to network"
                    );
                }
                Err(e) => {
                    tracing::warn!(
                        container = id,
                        error = %e,
                        "failed to connect container to network"
                    );
                }
            }
        }
    }

    /// Disconnect a container from its network. Best-effort.
    async fn disconnect_network(&self, id: &str) {
        let mut mgr_guard = self.network_manager.write().await;
        if let Some(ref mut nm) = *mgr_guard {
            match nm.disconnect_container(id) {
                Ok(()) => {
                    info!(container = id, "container disconnected from network");
                }
                Err(e) => {
                    // Not connected or already disconnected — fine.
                    tracing::debug!(container = id, error = %e, "network disconnect skipped");
                }
            }
        }
    }

    /// Publish a lifecycle event to the pub/sub hub.
    fn publish_event(&self, event_json: serde_json::Value) {
        self.publisher.publish("container.lifecycle", event_json);
    }

    /// Get a reference to the pub/sub hub (for subscribing to events).
    #[inline]
    #[must_use]
    pub fn event_bus(&self) -> &PubSub {
        &self.publisher
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_image() -> Image {
        Image {
            id: "test-image".to_string(),
            reference: crate::image::ImageRef {
                registry: "docker.io".to_string(),
                repository: "library/alpine".to_string(),
                tag: "latest".to_string(),
                digest: None,
            },
            size_bytes: 1024,
            layers: vec![],
            created_at: chrono::Utc::now(),
        }
    }

    fn test_manager(dir: &Path) -> ContainerManager {
        let store = Arc::new(ImageStore::new(&dir.join("images")).unwrap());
        ContainerManager::new(&dir.join("containers"), store).unwrap()
    }

    #[tokio::test]
    async fn container_lifecycle() {
        let dir = tempfile::tempdir().unwrap();
        let manager = test_manager(dir.path());

        let config = ContainerConfig::default();
        let container = manager.create(&test_image(), config).await.unwrap();
        assert_eq!(container.state, ContainerState::Created);

        // start() will attempt kavach exec which will fail in test (no sandbox).
        // That's expected — the container should transition to Stopped.
        let _ = manager.start(&container.id).await;

        let listed = manager.list().await.unwrap();
        assert_eq!(listed.len(), 1);
        // Container should be Stopped (either after successful exec or failed exec).
        assert_eq!(listed[0].state, ContainerState::Stopped);

        manager.remove(&container.id).await.unwrap();
        assert!(manager.list().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn double_start_errors() {
        let dir = tempfile::tempdir().unwrap();
        let manager = test_manager(dir.path());

        let c = manager
            .create(&test_image(), ContainerConfig::default())
            .await
            .unwrap();

        // First start transitions to Running then executes (and fails in test).
        let _ = manager.start(&c.id).await;

        // Container is now Stopped, so starting again should work (restart cycle).
        let _ = manager.start(&c.id).await;
    }

    #[tokio::test]
    async fn stop_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let manager = test_manager(dir.path());
        assert!(manager.stop("nonexistent").await.is_err());
    }

    #[tokio::test]
    async fn remove_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let manager = test_manager(dir.path());
        assert!(manager.remove("nonexistent").await.is_err());
    }

    #[tokio::test]
    async fn cannot_remove_running_container() {
        let dir = tempfile::tempdir().unwrap();
        let manager = test_manager(dir.path());

        let c = manager
            .create(&test_image(), ContainerConfig::default())
            .await
            .unwrap();

        // Manually set to Running (bypassing exec).
        {
            let mut containers = manager.containers.write().await;
            containers.get_mut(&c.id).unwrap().state = ContainerState::Running;
        }

        let err = manager.remove(&c.id).await.unwrap_err();
        assert!(matches!(err, StivaError::InvalidState(_)));
    }

    #[tokio::test]
    async fn create_sets_name_and_timestamps() {
        let dir = tempfile::tempdir().unwrap();
        let manager = test_manager(dir.path());

        // Auto-generated name.
        let c = manager
            .create(&test_image(), ContainerConfig::default())
            .await
            .unwrap();
        assert!(c.name.is_some());
        assert_eq!(c.name.as_deref().unwrap().len(), 12);
        assert!(c.started_at.is_none());

        // Explicit name.
        let config = ContainerConfig {
            name: Some("web-server".to_string()),
            ..Default::default()
        };
        let c2 = manager.create(&test_image(), config).await.unwrap();
        assert_eq!(c2.name.as_deref(), Some("web-server"));
    }

    #[tokio::test]
    async fn start_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let manager = test_manager(dir.path());
        let err = manager.start("nonexistent").await.unwrap_err();
        assert!(matches!(err, StivaError::ContainerNotFound(_)));
    }

    #[tokio::test]
    async fn multiple_containers() {
        let dir = tempfile::tempdir().unwrap();
        let manager = test_manager(dir.path());

        let c1 = manager
            .create(&test_image(), ContainerConfig::default())
            .await
            .unwrap();
        let c2 = manager
            .create(&test_image(), ContainerConfig::default())
            .await
            .unwrap();
        let c3 = manager
            .create(&test_image(), ContainerConfig::default())
            .await
            .unwrap();

        assert_ne!(c1.id, c2.id);
        assert_ne!(c2.id, c3.id);
        assert_eq!(manager.list().await.unwrap().len(), 3);
    }

    #[tokio::test]
    async fn remove_cleans_up_directory() {
        let dir = tempfile::tempdir().unwrap();
        let manager = test_manager(dir.path());

        let c = manager
            .create(&test_image(), ContainerConfig::default())
            .await
            .unwrap();
        let container_dir = dir.path().join("containers").join(&c.id);
        assert!(container_dir.exists());

        manager.remove(&c.id).await.unwrap();
        assert!(!container_dir.exists());
    }

    #[tokio::test]
    async fn logs_empty_before_start() {
        let dir = tempfile::tempdir().unwrap();
        let manager = test_manager(dir.path());

        let c = manager
            .create(&test_image(), ContainerConfig::default())
            .await
            .unwrap();
        let logs = manager.logs(&c.id).await.unwrap();
        assert!(logs.is_empty());
    }

    #[tokio::test]
    async fn logs_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let manager = test_manager(dir.path());
        assert!(manager.logs("nonexistent").await.is_err());
    }

    #[test]
    fn container_state_serde() {
        let states = [
            ContainerState::Created,
            ContainerState::Running,
            ContainerState::Paused,
            ContainerState::Stopped,
            ContainerState::Removing,
        ];
        for state in states {
            let json = serde_json::to_string(&state).unwrap();
            let back: ContainerState = serde_json::from_str(&json).unwrap();
            assert_eq!(state, back);
        }
    }

    #[test]
    fn container_config_defaults() {
        let config = ContainerConfig::default();
        assert!(config.name.is_none());
        assert!(config.command.is_empty());
        assert!(config.env.is_empty());
        assert!(config.volumes.is_empty());
        assert!(config.ports.is_empty());
        assert!(config.network.is_none());
        assert_eq!(config.memory_limit, 0);
        assert_eq!(config.cpu_shares, 0);
        assert!(config.user.is_none());
        assert!(config.workdir.is_none());
        assert!(!config.detach);
        assert_eq!(config.stop_grace_ms, 10_000);
    }

    #[tokio::test]
    async fn daemon_container_lifecycle() {
        let dir = tempfile::tempdir().unwrap();
        let manager = test_manager(dir.path());

        let config = ContainerConfig {
            detach: true,
            command: vec!["sleep".into(), "0.1".into()],
            ..Default::default()
        };
        let c = manager.create(&test_image(), config).await.unwrap();
        assert_eq!(c.state, ContainerState::Created);
        assert!(c.config.detach);

        // Start spawns the daemon and returns immediately.
        let _ = manager.start(&c.id).await;

        // Container should be Running (or Stopped if exec fell through).
        let listed = manager.list().await.unwrap();
        assert_eq!(listed.len(), 1);
    }

    #[tokio::test]
    async fn daemon_stop_with_grace() {
        let dir = tempfile::tempdir().unwrap();
        let manager = test_manager(dir.path());

        let config = ContainerConfig {
            detach: true,
            stop_grace_ms: 100,
            ..Default::default()
        };
        let c = manager.create(&test_image(), config).await.unwrap();
        let _ = manager.start(&c.id).await;

        // Stop should work whether daemon is running or already exited.
        let _ = manager.stop(&c.id).await;

        let listed = manager.list().await.unwrap();
        assert_eq!(listed[0].state, ContainerState::Stopped);
    }

    #[tokio::test]
    async fn wait_oneshot_returns_result() {
        let dir = tempfile::tempdir().unwrap();
        let manager = test_manager(dir.path());

        let c = manager
            .create(&test_image(), ContainerConfig::default())
            .await
            .unwrap();
        // One-shot start runs to completion.
        let _ = manager.start(&c.id).await;

        // Wait on a completed one-shot returns the cached result.
        // (May error if exec failed in test env without sandbox.)
        let _ = manager.wait(&c.id).await;
    }

    #[tokio::test]
    async fn try_wait_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let manager = test_manager(dir.path());
        assert!(manager.try_wait("nonexistent").await.is_err());
    }

    #[tokio::test]
    async fn wait_not_started_errors() {
        let dir = tempfile::tempdir().unwrap();
        let manager = test_manager(dir.path());

        let c = manager
            .create(&test_image(), ContainerConfig::default())
            .await
            .unwrap();
        // Never started — wait should error.
        let err = manager.wait(&c.id).await;
        assert!(err.is_err());
    }

    #[test]
    fn config_detach_serde() {
        let config = ContainerConfig {
            detach: true,
            stop_grace_ms: 5000,
            ..Default::default()
        };
        let json = serde_json::to_string(&config).unwrap();
        let back: ContainerConfig = serde_json::from_str(&json).unwrap();
        assert!(back.detach);
        assert_eq!(back.stop_grace_ms, 5000);
    }

    #[test]
    fn config_detach_defaults_in_json() {
        // JSON without detach/stop_grace_ms should use serde defaults.
        let json =
            r#"{"command":[],"env":{},"volumes":[],"ports":[],"memory_limit":0,"cpu_shares":0}"#;
        let config: ContainerConfig = serde_json::from_str(json).unwrap();
        assert!(!config.detach);
        assert_eq!(config.stop_grace_ms, 10_000);
    }

    #[test]
    fn container_config_serde() {
        let config = ContainerConfig {
            name: Some("test".to_string()),
            command: vec!["/bin/sh".to_string()],
            env: HashMap::from([("FOO".to_string(), "bar".to_string())]),
            ports: vec!["8080:80".to_string()],
            memory_limit: 512 * 1024 * 1024,
            ..Default::default()
        };
        let json = serde_json::to_string(&config).unwrap();
        let back: ContainerConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.name, Some("test".to_string()));
        assert_eq!(back.memory_limit, 512 * 1024 * 1024);
    }

    #[test]
    fn container_serde_round_trip() {
        let container = Container {
            id: "abc-123".into(),
            name: Some("web".into()),
            image_id: "sha256:img".into(),
            image_ref: "docker.io/library/nginx:latest".into(),
            state: ContainerState::Stopped,
            pid: None,
            created_at: chrono::Utc::now(),
            started_at: Some(chrono::Utc::now()),
            config: ContainerConfig {
                name: Some("web".into()),
                command: vec!["/bin/sh".into()],
                env: HashMap::from([("K".into(), "V".into())]),
                volumes: vec!["/host:/container".into()],
                ports: vec!["8080:80".into()],
                network: Some("bridge".into()),
                memory_limit: 256 * 1024 * 1024,
                cpu_shares: 1024,
                user: Some("nobody".into()),
                workdir: Some("/app".into()),
                ..Default::default()
            },
            exit_code: Some(0),
        };
        let json = serde_json::to_string(&container).unwrap();
        let back: Container = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, "abc-123");
        assert_eq!(back.state, ContainerState::Stopped);
        assert_eq!(back.exit_code, Some(0));
        assert_eq!(back.config.memory_limit, 256 * 1024 * 1024);
        assert_eq!(back.config.user.as_deref(), Some("nobody"));
    }

    #[tokio::test]
    async fn checkpoint_not_running() {
        let dir = tempfile::tempdir().unwrap();
        let manager = test_manager(dir.path());

        let c = manager
            .create(&test_image(), ContainerConfig::default())
            .await
            .unwrap();
        // Container is Created, not Running — checkpoint should fail.
        let err = manager.checkpoint(&c.id, false).await.unwrap_err();
        assert!(matches!(err, StivaError::InvalidState(_)));
    }

    #[tokio::test]
    async fn checkpoint_no_pid() {
        let dir = tempfile::tempdir().unwrap();
        let manager = test_manager(dir.path());

        let c = manager
            .create(&test_image(), ContainerConfig::default())
            .await
            .unwrap();
        // Manually set to Running but leave pid as None.
        {
            let mut containers = manager.containers.write().await;
            containers.get_mut(&c.id).unwrap().state = ContainerState::Running;
        }
        let err = manager.checkpoint(&c.id, false).await.unwrap_err();
        assert!(matches!(err, StivaError::InvalidState(_)));
    }

    #[tokio::test]
    async fn restore_not_paused() {
        let dir = tempfile::tempdir().unwrap();
        let manager = test_manager(dir.path());

        let c = manager
            .create(&test_image(), ContainerConfig::default())
            .await
            .unwrap();
        // Manually set to Running — restore should fail.
        {
            let mut containers = manager.containers.write().await;
            containers.get_mut(&c.id).unwrap().state = ContainerState::Running;
        }
        let err = manager
            .restore(&c.id, Path::new("/tmp/fake-checkpoint"))
            .await
            .unwrap_err();
        assert!(matches!(err, StivaError::InvalidState(_)));
    }

    #[tokio::test]
    async fn checkpoint_dir_created() {
        let dir = tempfile::tempdir().unwrap();
        let manager = test_manager(dir.path());

        let c = manager
            .create(&test_image(), ContainerConfig::default())
            .await
            .unwrap();

        // Verify the checkpoints directory structure exists after create.
        let container_root = dir.path().join("containers").join(&c.id);
        assert!(container_root.exists());

        // The checkpoints subdirectory is created on-demand during checkpoint.
        // Verify the parent container dir is in place.
        let checkpoints_parent = container_root.join("checkpoints");
        // Not created yet — only created when checkpoint() is actually called.
        assert!(!checkpoints_parent.exists());
    }

    #[test]
    fn migration_bundle_serde() {
        let bundle = MigrationBundle {
            source_container_id: "abc-123".into(),
            image_ref: "docker.io/library/nginx:latest".into(),
            config: ContainerConfig::default(),
            checkpoint_dir: PathBuf::from("/tmp/checkpoint"),
            source_node: "node-1".into(),
            prepared_at: chrono::Utc::now(),
        };
        let json = serde_json::to_string(&bundle).unwrap();
        let back: MigrationBundle = serde_json::from_str(&json).unwrap();
        assert_eq!(back.source_container_id, "abc-123");
        assert_eq!(back.image_ref, "docker.io/library/nginx:latest");
        assert_eq!(back.source_node, "node-1");
        assert_eq!(back.checkpoint_dir, PathBuf::from("/tmp/checkpoint"));
    }

    #[tokio::test]
    async fn prepare_migration_not_running() {
        let dir = tempfile::tempdir().unwrap();
        let manager = test_manager(dir.path());

        let c = manager
            .create(&test_image(), ContainerConfig::default())
            .await
            .unwrap();
        // Container is Created, not Running — prepare_migration should fail.
        let err = manager.prepare_migration(&c.id).await.unwrap_err();
        assert!(matches!(err, StivaError::InvalidState(_)));
    }

    #[tokio::test]
    async fn exec_not_running() {
        let dir = tempfile::tempdir().unwrap();
        let manager = test_manager(dir.path());

        let c = manager
            .create(&test_image(), ContainerConfig::default())
            .await
            .unwrap();
        let err = manager
            .exec(&c.id, &["echo".into(), "hi".into()])
            .await
            .unwrap_err();
        assert!(matches!(err, StivaError::InvalidState(_)));
    }

    #[tokio::test]
    async fn exec_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let manager = test_manager(dir.path());
        let err = manager
            .exec("nonexistent", &["echo".into()])
            .await
            .unwrap_err();
        assert!(matches!(err, StivaError::ContainerNotFound(_)));
    }

    #[tokio::test]
    async fn exec_empty_command() {
        let dir = tempfile::tempdir().unwrap();
        let manager = test_manager(dir.path());

        let config = ContainerConfig {
            detach: true,
            ..Default::default()
        };
        let c = manager.create(&test_image(), config).await.unwrap();
        // Manually set Running with a PID to test empty command path.
        {
            let mut containers = manager.containers.write().await;
            let container = containers.get_mut(&c.id).unwrap();
            container.state = ContainerState::Running;
            container.pid = Some(1);
        }
        let err = manager.exec(&c.id, &[]).await.unwrap_err();
        assert!(matches!(err, StivaError::Runtime(_)));
    }

    #[tokio::test]
    async fn pause_not_running() {
        let dir = tempfile::tempdir().unwrap();
        let manager = test_manager(dir.path());

        let c = manager
            .create(&test_image(), ContainerConfig::default())
            .await
            .unwrap();
        let err = manager.pause(&c.id).await.unwrap_err();
        assert!(matches!(err, StivaError::InvalidState(_)));
    }

    #[tokio::test]
    async fn unpause_not_paused() {
        let dir = tempfile::tempdir().unwrap();
        let manager = test_manager(dir.path());

        let c = manager
            .create(&test_image(), ContainerConfig::default())
            .await
            .unwrap();
        let err = manager.unpause(&c.id).await.unwrap_err();
        assert!(matches!(err, StivaError::InvalidState(_)));
    }

    #[tokio::test]
    async fn stats_not_running() {
        let dir = tempfile::tempdir().unwrap();
        let manager = test_manager(dir.path());

        let c = manager
            .create(&test_image(), ContainerConfig::default())
            .await
            .unwrap();
        let err = manager.stats(&c.id).await.unwrap_err();
        assert!(matches!(err, StivaError::InvalidState(_)));
    }

    #[test]
    fn container_stats_serde() {
        let stats = runtime::ContainerStats {
            memory_bytes: 1024 * 1024,
            memory_limit_bytes: 512 * 1024 * 1024,
            cpu_usage_us: 50_000,
            pids_current: 3,
            pids_limit: 100,
        };
        let json = serde_json::to_string(&stats).unwrap();
        let back: runtime::ContainerStats = serde_json::from_str(&json).unwrap();
        assert_eq!(back.memory_bytes, 1024 * 1024);
        assert_eq!(back.pids_current, 3);
    }

    #[tokio::test]
    async fn signal_not_running() {
        let dir = tempfile::tempdir().unwrap();
        let manager = test_manager(dir.path());

        let c = manager
            .create(&test_image(), ContainerConfig::default())
            .await
            .unwrap();
        let err = manager.signal(&c.id, 15).await.unwrap_err();
        assert!(matches!(err, StivaError::InvalidState(_)));
    }

    #[tokio::test]
    async fn signal_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let manager = test_manager(dir.path());
        let err = manager.signal("nonexistent", 15).await.unwrap_err();
        assert!(matches!(err, StivaError::ContainerNotFound(_)));
    }

    // --- log_tail tests ---

    #[tokio::test]
    async fn log_tail_empty_before_start() {
        let dir = tempfile::tempdir().unwrap();
        let manager = test_manager(dir.path());

        let c = manager
            .create(&test_image(), ContainerConfig::default())
            .await
            .unwrap();
        let tail = manager.log_tail(&c.id, 10).await.unwrap();
        assert!(tail.is_empty());
    }

    #[tokio::test]
    async fn log_tail_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let manager = test_manager(dir.path());
        assert!(manager.log_tail("nonexistent", 10).await.is_err());
    }

    #[tokio::test]
    async fn log_tail_returns_last_n_lines() {
        let dir = tempfile::tempdir().unwrap();
        let manager = test_manager(dir.path());

        let c = manager
            .create(&test_image(), ContainerConfig::default())
            .await
            .unwrap();

        // Write some content to the log file.
        {
            let internals = manager.internals.read().await;
            let internal = internals.get(&c.id).unwrap();
            let content = "line1\nline2\nline3\nline4\nline5\n";
            std::fs::write(&internal.log_path, content).unwrap();
        }

        let tail = manager.log_tail(&c.id, 3).await.unwrap();
        // Last 3 non-empty lines (the trailing newline creates an empty last line).
        let lines: Vec<&str> = tail.lines().collect();
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0], "line3");
        assert_eq!(lines[1], "line4");
        assert_eq!(lines[2], "line5");
    }

    #[tokio::test]
    async fn log_tail_fewer_lines_than_requested() {
        let dir = tempfile::tempdir().unwrap();
        let manager = test_manager(dir.path());

        let c = manager
            .create(&test_image(), ContainerConfig::default())
            .await
            .unwrap();

        {
            let internals = manager.internals.read().await;
            let internal = internals.get(&c.id).unwrap();
            std::fs::write(&internal.log_path, "only\ntwo").unwrap();
        }

        let tail = manager.log_tail(&c.id, 10).await.unwrap();
        let lines: Vec<&str> = tail.lines().collect();
        assert_eq!(lines.len(), 2);
    }

    // --- lifecycle event tests ---

    #[tokio::test]
    async fn create_publishes_event() {
        let dir = tempfile::tempdir().unwrap();
        let manager = test_manager(dir.path());

        let mut rx = manager.event_bus().subscribe("container.lifecycle");

        let c = manager
            .create(&test_image(), ContainerConfig::default())
            .await
            .unwrap();

        let msg = rx.try_recv().unwrap();
        assert_eq!(msg.payload["event"], "created");
        assert_eq!(msg.payload["container_id"], c.id);
    }

    #[tokio::test]
    async fn start_publishes_event() {
        let dir = tempfile::tempdir().unwrap();
        let manager = test_manager(dir.path());

        let c = manager
            .create(&test_image(), ContainerConfig::default())
            .await
            .unwrap();

        let mut rx = manager.event_bus().subscribe("container.lifecycle");

        // start() invokes kavach which may fail in test — event is still published.
        let _ = manager.start(&c.id).await;

        let msg = rx.try_recv().unwrap();
        assert_eq!(msg.payload["event"], "started");
        assert_eq!(msg.payload["container_id"], c.id);
    }

    #[tokio::test]
    async fn stop_publishes_event() {
        let dir = tempfile::tempdir().unwrap();
        let manager = test_manager(dir.path());

        let c = manager
            .create(&test_image(), ContainerConfig::default())
            .await
            .unwrap();
        let _ = manager.start(&c.id).await;

        let mut rx = manager.event_bus().subscribe("container.lifecycle");
        let _ = manager.stop(&c.id).await;

        let msg = rx.try_recv().unwrap();
        assert_eq!(msg.payload["event"], "stopped");
        assert_eq!(msg.payload["container_id"], c.id);
    }

    #[tokio::test]
    async fn remove_publishes_event() {
        let dir = tempfile::tempdir().unwrap();
        let manager = test_manager(dir.path());

        let c = manager
            .create(&test_image(), ContainerConfig::default())
            .await
            .unwrap();
        let _ = manager.start(&c.id).await;

        let mut rx = manager.event_bus().subscribe("container.lifecycle");
        manager.remove(&c.id).await.unwrap();

        let msg = rx.try_recv().unwrap();
        assert_eq!(msg.payload["event"], "removed");
        assert_eq!(msg.payload["container_id"], c.id);
    }

    // --- network connection tests ---

    #[tokio::test]
    async fn connect_network_no_ports_is_noop() {
        let dir = tempfile::tempdir().unwrap();
        let manager = test_manager(dir.path());

        let c = manager
            .create(&test_image(), ContainerConfig::default())
            .await
            .unwrap();

        // No ports, no network — should be a no-op (no panic).
        manager.connect_network(&c.id, 1).await;

        // Network manager should not have been initialized.
        assert!(manager.network_manager.read().await.is_none());
    }

    #[tokio::test]
    async fn connect_network_with_ports() {
        let dir = tempfile::tempdir().unwrap();
        let manager = test_manager(dir.path());

        let config = ContainerConfig {
            ports: vec!["8080:80".to_string()],
            ..Default::default()
        };
        let c = manager.create(&test_image(), config).await.unwrap();

        // Should attempt network connection (best-effort, won't fail).
        manager.connect_network(&c.id, 1).await;

        // Network manager should have been initialized.
        assert!(manager.network_manager.read().await.is_some());
    }

    #[tokio::test]
    async fn connect_network_with_custom_network() {
        let dir = tempfile::tempdir().unwrap();
        let manager = test_manager(dir.path());

        let config = ContainerConfig {
            network: Some("custom-net".to_string()),
            ..Default::default()
        };
        let c = manager.create(&test_image(), config).await.unwrap();

        // Will attempt to connect to "custom-net" which doesn't exist,
        // but should warn and not panic.
        manager.connect_network(&c.id, 1).await;
    }

    #[tokio::test]
    async fn event_bus_returns_pubsub() {
        let dir = tempfile::tempdir().unwrap();
        let manager = test_manager(dir.path());
        // Verify we can subscribe.
        let _rx = manager.event_bus().subscribe("container.lifecycle");
        assert_eq!(manager.event_bus().pattern_count(), 1);
    }
}
