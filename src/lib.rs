//! # Stiva — Container Runtime for AGNOS
//!
//! Stiva (Romanian: stivă = stack, pile) is an OCI-compatible container runtime
//! that builds on [kavach](https://github.com/MacCracken/kavach) for process
//! isolation and [majra](https://github.com/MacCracken/majra) for scheduling.
//!
//! ## Architecture
//!
//! ```text
//! stiva (this crate)
//!   ├── kavach (sandbox: seccomp, Landlock, namespaces, OCI spec)
//!   ├── majra (job queue, heartbeat FSM, pub/sub)
//!   └── nein (network policy — planned)
//! ```
//!
//! ## Modules
//!
//! - [`image`] — OCI image pull, store, layer management, overlay FS
//! - [`container`] — Container lifecycle (create, start, stop, kill, remove)
//! - [`runtime`] — OCI runtime spec execution via kavach backends
//! - [`network`] — Container networking (bridge, host, none, custom)
//! - [`storage`] — Overlay filesystem, volume mounts, tmpfs
//! - [`registry`] — OCI registry client (Docker Hub, GHCR, custom)
//! - [`compose`] — Multi-container orchestration (compose-file equivalent)

pub mod image;
pub mod container;
pub mod runtime;
pub mod network;
pub mod storage;
pub mod registry;
#[cfg(feature = "compose")]
pub mod compose;

mod error;
pub use error::StivaError;

use std::sync::Arc;

/// Top-level entry point for the stiva runtime.
pub struct Stiva {
    image_store: Arc<image::ImageStore>,
    registry_client: Arc<registry::RegistryClient>,
    containers: Arc<container::ContainerManager>,
    #[allow(dead_code)]
    config: StivaConfig,
}

/// Configuration for the stiva runtime.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StivaConfig {
    /// Root directory for container data.
    /// Default: `/var/lib/agnos/containers/`
    pub root_path: std::path::PathBuf,

    /// Image storage directory.
    /// Default: `/var/lib/agnos/images/`
    pub image_path: std::path::PathBuf,

    /// Default network mode.
    pub default_network: network::NetworkMode,

    /// Maximum concurrent containers.
    pub max_containers: usize,
}

impl Default for StivaConfig {
    fn default() -> Self {
        Self {
            root_path: std::path::PathBuf::from("/var/lib/agnos/containers"),
            image_path: std::path::PathBuf::from("/var/lib/agnos/images"),
            default_network: network::NetworkMode::Bridge,
            max_containers: 64,
        }
    }
}

impl Stiva {
    /// Create a new stiva runtime.
    pub async fn new(config: StivaConfig) -> Result<Self, StivaError> {
        let image_store = Arc::new(image::ImageStore::new(&config.image_path)?);
        let containers = Arc::new(container::ContainerManager::new(&config.root_path)?);
        let registry_client = Arc::new(registry::RegistryClient::new());

        Ok(Self {
            image_store,
            registry_client,
            containers,
            config,
        })
    }

    /// Create a new stiva runtime with explicit registry configuration.
    pub async fn with_registry(
        config: StivaConfig,
        registry_config: registry::RegistryConfig,
    ) -> Result<Self, StivaError> {
        let image_store = Arc::new(image::ImageStore::new(&config.image_path)?);
        let containers = Arc::new(container::ContainerManager::new(&config.root_path)?);
        let registry_client = Arc::new(registry::RegistryClient::with_config(registry_config));

        Ok(Self {
            image_store,
            registry_client,
            containers,
            config,
        })
    }

    /// Pull an OCI image from a registry.
    pub async fn pull(&self, reference: &str) -> Result<image::Image, StivaError> {
        let parsed = image::ImageRef::parse(reference)?;
        self.image_store.pull(&parsed, &self.registry_client).await
    }

    /// Create and start a container.
    pub async fn run(
        &self,
        image: &str,
        config: container::ContainerConfig,
    ) -> Result<container::Container, StivaError> {
        let img = self.pull(image).await?;
        let container = self.containers.create(&img, config).await?;
        self.containers.start(&container.id).await?;
        Ok(container)
    }

    /// List running containers.
    pub async fn ps(&self) -> Result<Vec<container::Container>, StivaError> {
        self.containers.list().await
    }

    /// Stop a container.
    pub async fn stop(&self, id: &str) -> Result<(), StivaError> {
        self.containers.stop(id).await
    }

    /// Remove a container.
    pub async fn rm(&self, id: &str) -> Result<(), StivaError> {
        self.containers.remove(id).await
    }

    /// List local images.
    pub async fn images(&self) -> Result<Vec<image::Image>, StivaError> {
        self.image_store.list()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config() {
        let config = StivaConfig::default();
        assert_eq!(
            config.root_path,
            std::path::PathBuf::from("/var/lib/agnos/containers")
        );
        assert_eq!(config.max_containers, 64);
    }
}
