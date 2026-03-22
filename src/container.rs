//! Container lifecycle — create, start, stop, kill, remove.

use crate::error::StivaError;
use crate::image::Image;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::sync::RwLock;

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

/// Manages container lifecycle.
pub struct ContainerManager {
    root: PathBuf,
    containers: RwLock<HashMap<String, Container>>,
}

impl ContainerManager {
    /// Create a new container manager.
    pub fn new(root: &Path) -> Result<Self, StivaError> {
        std::fs::create_dir_all(root)?;
        Ok(Self {
            root: root.to_path_buf(),
            containers: RwLock::new(HashMap::new()),
        })
    }

    /// Create a container from an image.
    pub async fn create(
        &self,
        image: &Image,
        config: ContainerConfig,
    ) -> Result<Container, StivaError> {
        let id = uuid::Uuid::new_v4().to_string();
        let name = config.name.clone().unwrap_or_else(|| id[..12].to_string());

        // Create container root filesystem
        let container_root = self.root.join(&id);
        std::fs::create_dir_all(&container_root)?;

        // TODO: Overlay image layers into container rootfs via storage module
        // TODO: Generate OCI runtime spec from config
        // TODO: Create kavach sandbox config

        let container = Container {
            id: id.clone(),
            name: Some(name),
            image_id: image.id.clone(),
            image_ref: image.reference.full_ref(),
            state: ContainerState::Created,
            pid: None,
            created_at: chrono::Utc::now(),
            started_at: None,
            config,
        };

        self.containers.write().await.insert(id, container.clone());
        Ok(container)
    }

    /// Start a created container.
    pub async fn start(&self, id: &str) -> Result<(), StivaError> {
        let mut containers = self.containers.write().await;
        let container = containers
            .get_mut(id)
            .ok_or_else(|| StivaError::ContainerNotFound(id.to_string()))?;

        if container.state == ContainerState::Running {
            return Err(StivaError::AlreadyRunning(id.to_string()));
        }

        // TODO: Execute via kavach sandbox
        // TODO: Set up networking via network module
        // TODO: Mount volumes via storage module
        // TODO: Record PID

        container.state = ContainerState::Running;
        container.started_at = Some(chrono::Utc::now());
        Ok(())
    }

    /// Stop a running container.
    pub async fn stop(&self, id: &str) -> Result<(), StivaError> {
        let mut containers = self.containers.write().await;
        let container = containers
            .get_mut(id)
            .ok_or_else(|| StivaError::ContainerNotFound(id.to_string()))?;

        // TODO: Send SIGTERM via kavach, wait, SIGKILL fallback
        container.state = ContainerState::Stopped;
        container.pid = None;
        Ok(())
    }

    /// Remove a stopped container.
    pub async fn remove(&self, id: &str) -> Result<(), StivaError> {
        let mut containers = self.containers.write().await;
        let container = containers
            .get(id)
            .ok_or_else(|| StivaError::ContainerNotFound(id.to_string()))?;

        if container.state == ContainerState::Running {
            return Err(StivaError::AlreadyRunning(id.to_string()));
        }

        // Clean up filesystem
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn container_lifecycle() {
        let dir = tempfile::tempdir().unwrap();
        let manager = ContainerManager::new(dir.path()).unwrap();

        let image = Image {
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
        };

        let config = ContainerConfig::default();
        let container = manager.create(&image, config).await.unwrap();
        assert_eq!(container.state, ContainerState::Created);

        manager.start(&container.id).await.unwrap();
        let listed = manager.list().await.unwrap();
        assert_eq!(listed[0].state, ContainerState::Running);

        manager.stop(&container.id).await.unwrap();
        manager.remove(&container.id).await.unwrap();
        assert!(manager.list().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn double_start_errors() {
        let dir = tempfile::tempdir().unwrap();
        let manager = ContainerManager::new(dir.path()).unwrap();

        let image = Image {
            id: "test".to_string(),
            reference: crate::image::ImageRef::parse("alpine").unwrap(),
            size_bytes: 0,
            layers: vec![],
            created_at: chrono::Utc::now(),
        };

        let c = manager.create(&image, ContainerConfig::default()).await.unwrap();
        manager.start(&c.id).await.unwrap();
        assert!(manager.start(&c.id).await.is_err());
    }
}
