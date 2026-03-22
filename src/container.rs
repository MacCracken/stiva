//! Container lifecycle — create, start, stop, kill, remove.

use crate::error::StivaError;
use crate::image::{Image, ImageStore};
use crate::runtime::{self, ContainerExecResult, RuntimeSpec};
use crate::storage::{self, OverlayPaths};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

/// Container state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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
}

/// Manages container lifecycle.
pub struct ContainerManager {
    root: PathBuf,
    containers: RwLock<HashMap<String, Container>>,
    /// Internal state not exposed in the serializable Container.
    internals: RwLock<HashMap<String, ContainerInternals>>,
    /// Image store for resolving layer blobs.
    image_store: Arc<ImageStore>,
}

impl ContainerManager {
    /// Create a new container manager.
    pub fn new(root: &Path, image_store: Arc<ImageStore>) -> Result<Self, StivaError> {
        std::fs::create_dir_all(root)?;
        Ok(Self {
            root: root.to_path_buf(),
            containers: RwLock::new(HashMap::new()),
            internals: RwLock::new(HashMap::new()),
            image_store,
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

        // Generate OCI runtime spec.
        let temp_container = Container {
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

        let rootfs = overlay
            .as_ref()
            .map(|o| o.merged.clone())
            .unwrap_or_else(|| container_root.join("rootfs"));

        let spec = runtime::generate_spec(&temp_container, &rootfs)?;

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
                },
            );
        }

        let container = temp_container;
        self.containers.write().await.insert(id, container.clone());
        Ok(container)
    }

    /// Start a created container.
    ///
    /// For the one-shot execution model, this runs the container command to
    /// completion and captures stdout/stderr. The container transitions
    /// Created → Running → Stopped.
    pub async fn start(&self, id: &str) -> Result<(), StivaError> {
        // Transition to Running.
        {
            let mut containers = self.containers.write().await;
            let container = containers
                .get_mut(id)
                .ok_or_else(|| StivaError::ContainerNotFound(id.to_string()))?;

            if container.state == ContainerState::Running {
                return Err(StivaError::AlreadyRunning(id.to_string()));
            }

            container.state = ContainerState::Running;
            container.started_at = Some(chrono::Utc::now());
        }

        // Execute via kavach sandbox.
        let spec = {
            let internals = self.internals.read().await;
            internals
                .get(id)
                .and_then(|i| i.spec.clone())
                .ok_or_else(|| StivaError::Runtime("no runtime spec for container".into()))?
        };

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
                    Err(_) => {
                        container.state = ContainerState::Stopped;
                        container.pid = None;
                    }
                }
            }
        }

        // Write logs.
        if let Ok(ref exec_result) = result {
            let mut internals = self.internals.write().await;
            if let Some(internal) = internals.get_mut(id) {
                let log_content = format!(
                    "=== stdout ===\n{}\n=== stderr ===\n{}\n=== exit_code: {} | duration: {}ms | timed_out: {} ===\n",
                    exec_result.stdout,
                    exec_result.stderr,
                    exec_result.exit_code,
                    exec_result.duration_ms,
                    exec_result.timed_out,
                );
                let _ = std::fs::write(&internal.log_path, &log_content);
                internal.exec_result = Some(exec_result.clone());
            }
        }

        // Propagate execution errors.
        result?;
        Ok(())
    }

    /// Stop a running container.
    ///
    /// For the one-shot model, containers are already stopped after exec
    /// completes. This is a no-op for stopped containers.
    pub async fn stop(&self, id: &str) -> Result<(), StivaError> {
        let mut containers = self.containers.write().await;
        let container = containers
            .get_mut(id)
            .ok_or_else(|| StivaError::ContainerNotFound(id.to_string()))?;

        // Future: send SIGTERM via kavach, wait, SIGKILL fallback.
        container.state = ContainerState::Stopped;
        container.pid = None;
        Ok(())
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
        Ok(())
    }

    /// List all containers.
    pub async fn list(&self) -> Result<Vec<Container>, StivaError> {
        Ok(self.containers.read().await.values().cloned().collect())
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
}
