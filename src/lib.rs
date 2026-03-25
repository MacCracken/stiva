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
//!   └── nein (nftables firewall, NAT, port mapping)
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
//! - [`health`] — Container health monitoring via majra heartbeat
//! - [`agent`] — Daimon agent registration
//! - [`mcp`] — MCP tools for AI agent integration
//! - [`intents`] — Agnoshi intent system (stub)

pub mod agent;
#[cfg(feature = "compose")]
pub mod compose;
pub mod container;
pub mod encrypted;
pub mod health;
pub mod image;
pub mod intents;
pub mod mcp;
pub mod network;
pub mod registry;
pub mod runtime;
pub mod storage;

mod error;
pub use error::StivaError;

use std::sync::Arc;
use tracing::info;

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
        info!(
            root = %config.root_path.display(),
            images = %config.image_path.display(),
            max_containers = config.max_containers,
            "initializing stiva runtime"
        );
        let image_store = Arc::new(image::ImageStore::new(&config.image_path)?);
        let containers = Arc::new(container::ContainerManager::new(
            &config.root_path,
            Arc::clone(&image_store),
        )?);
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
        let containers = Arc::new(container::ContainerManager::new(
            &config.root_path,
            Arc::clone(&image_store),
        )?);
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
        info!(image, "stiva run");
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
        info!(container = id, "stiva stop");
        self.containers.stop(id).await
    }

    /// Remove a container.
    pub async fn rm(&self, id: &str) -> Result<(), StivaError> {
        info!(container = id, "stiva rm");
        self.containers.remove(id).await
    }

    /// List local images.
    pub async fn images(&self) -> Result<Vec<image::Image>, StivaError> {
        self.image_store.list()
    }

    /// Wait for a container to exit. Returns execution result.
    pub async fn wait(&self, id: &str) -> Result<runtime::ContainerExecResult, StivaError> {
        info!(container = id, "stiva wait");
        self.containers.wait(id).await
    }

    /// Read container logs.
    pub async fn logs(&self, id: &str) -> Result<String, StivaError> {
        self.containers.logs(id).await
    }

    /// Deploy a compose file — parse, resolve dependencies, create and start services.
    #[cfg(feature = "compose")]
    pub async fn compose_up(
        &self,
        toml_content: &str,
    ) -> Result<compose::ComposeSession, StivaError> {
        info!(
            services = toml_content.matches("[services.").count(),
            "compose up"
        );
        let compose_file = compose::parse_compose(toml_content)?;
        let startup_order = compose::resolve_startup_order(&compose_file)?;

        let session_id = uuid::Uuid::new_v4().to_string();
        let mut services: std::collections::HashMap<String, Vec<String>> =
            std::collections::HashMap::new();

        for service_name in &startup_order {
            let service = compose_file.services.get(service_name).ok_or_else(|| {
                StivaError::Compose(format!("service '{service_name}' not found"))
            })?;

            let replicas = compose::replica_count(service);
            let mut container_ids = Vec::new();

            for i in 0..replicas {
                let config = compose::service_to_config(service_name, service, i);

                // Pull image (may already be cached).
                let img = self.pull(&service.image).await?;

                // Create and start container.
                let container = self.containers.create(&img, config).await?;
                let _ = self.containers.start(&container.id).await;
                container_ids.push(container.id);
            }

            services.insert(service_name.clone(), container_ids);
        }

        Ok(compose::ComposeSession {
            id: session_id,
            services,
            networks: vec![],
            startup_order,
            created_at: chrono::Utc::now(),
        })
    }

    /// Tear down a compose session — stop and remove all containers.
    #[cfg(feature = "compose")]
    pub async fn compose_down(&self, session: &compose::ComposeSession) -> Result<(), StivaError> {
        // Stop in reverse order.
        for service_name in session.startup_order.iter().rev() {
            if let Some(ids) = session.services.get(service_name) {
                for id in ids {
                    let _ = self.containers.stop(id).await;
                    let _ = self.containers.remove(id).await;
                }
            }
        }
        Ok(())
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
        assert_eq!(
            config.image_path,
            std::path::PathBuf::from("/var/lib/agnos/images")
        );
        assert_eq!(config.max_containers, 64);
        assert_eq!(config.default_network, network::NetworkMode::Bridge);
    }

    #[test]
    fn config_serde_round_trip() {
        let config = StivaConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let back: StivaConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.root_path, config.root_path);
        assert_eq!(back.max_containers, config.max_containers);
    }

    #[tokio::test]
    async fn stiva_new_creates_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let config = StivaConfig {
            root_path: dir.path().join("containers"),
            image_path: dir.path().join("images"),
            ..Default::default()
        };
        let _stiva = Stiva::new(config).await.unwrap();
        assert!(dir.path().join("containers").exists());
        assert!(dir.path().join("images").exists());
    }

    #[tokio::test]
    async fn stiva_with_registry_config() {
        let dir = tempfile::tempdir().unwrap();
        let config = StivaConfig {
            root_path: dir.path().join("containers"),
            image_path: dir.path().join("images"),
            ..Default::default()
        };
        let reg_config = registry::RegistryConfig {
            username: Some("user".into()),
            password: Some("pass".into()),
        };
        let stiva = Stiva::with_registry(config, reg_config).await.unwrap();
        assert!(stiva.images().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn stiva_stop_nonexistent() {
        let dir = tempfile::tempdir().unwrap();
        let config = StivaConfig {
            root_path: dir.path().join("containers"),
            image_path: dir.path().join("images"),
            ..Default::default()
        };
        let stiva = Stiva::new(config).await.unwrap();
        assert!(stiva.stop("nonexistent").await.is_err());
    }

    #[tokio::test]
    async fn stiva_rm_nonexistent() {
        let dir = tempfile::tempdir().unwrap();
        let config = StivaConfig {
            root_path: dir.path().join("containers"),
            image_path: dir.path().join("images"),
            ..Default::default()
        };
        let stiva = Stiva::new(config).await.unwrap();
        assert!(stiva.rm("nonexistent").await.is_err());
    }

    #[tokio::test]
    async fn stiva_pull_invalid_ref() {
        let dir = tempfile::tempdir().unwrap();
        let config = StivaConfig {
            root_path: dir.path().join("containers"),
            image_path: dir.path().join("images"),
            ..Default::default()
        };
        let stiva = Stiva::new(config).await.unwrap();
        assert!(stiva.pull("").await.is_err());
    }

    #[test]
    fn config_custom_values_serde() {
        let config = StivaConfig {
            root_path: "/custom/containers".into(),
            image_path: "/custom/images".into(),
            default_network: network::NetworkMode::Host,
            max_containers: 128,
        };
        let json = serde_json::to_string(&config).unwrap();
        let back: StivaConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(
            back.root_path,
            std::path::PathBuf::from("/custom/containers")
        );
        assert_eq!(back.default_network, network::NetworkMode::Host);
        assert_eq!(back.max_containers, 128);
    }

    #[test]
    fn config_toml_serde() {
        let config = StivaConfig::default();
        let toml_str = toml::to_string(&config).unwrap();
        let back: StivaConfig = toml::from_str(&toml_str).unwrap();
        assert_eq!(back.root_path, config.root_path);
    }

    #[tokio::test]
    async fn stiva_images_empty_on_init() {
        let dir = tempfile::tempdir().unwrap();
        let config = StivaConfig {
            root_path: dir.path().join("containers"),
            image_path: dir.path().join("images"),
            ..Default::default()
        };
        let stiva = Stiva::new(config).await.unwrap();
        assert!(stiva.images().await.unwrap().is_empty());
        assert!(stiva.ps().await.unwrap().is_empty());
    }

    // -- mock-backed Stiva::pull and Stiva::run --

    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn test_digest(data: &[u8]) -> String {
        use sha2::{Digest, Sha256};
        let hash = Sha256::digest(data);
        format!("sha256:{}", hex::encode(hash))
    }

    async fn mock_stiva(server: &MockServer) -> (Stiva, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let image_store = Arc::new(image::ImageStore::new(&dir.path().join("images")).unwrap());
        let containers = Arc::new(
            container::ContainerManager::new(
                &dir.path().join("containers"),
                Arc::clone(&image_store),
            )
            .unwrap(),
        );
        let registry_client = Arc::new(registry::RegistryClient::with_base_url(&server.uri()));

        let stiva = Stiva {
            image_store,
            registry_client,
            containers,
            config: StivaConfig {
                root_path: dir.path().join("containers"),
                image_path: dir.path().join("images"),
                ..Default::default()
            },
        };
        (stiva, dir)
    }

    #[tokio::test]
    async fn stiva_pull_with_mock() {
        let server = MockServer::start().await;

        let config_data = br#"{"os":"linux"}"#;
        let config_digest = test_digest(config_data);
        let layer_data = b"layer";
        let layer_digest = test_digest(layer_data);

        let manifest = registry::OciManifest {
            schema_version: 2,
            media_type: None,
            config: registry::Descriptor {
                media_type: "application/vnd.oci.image.config.v1+json".into(),
                digest: config_digest.clone(),
                size: config_data.len() as u64,
            },
            layers: vec![registry::Descriptor {
                media_type: "application/vnd.oci.image.layer.v1.tar+gzip".into(),
                digest: layer_digest.clone(),
                size: layer_data.len() as u64,
            }],
        };

        Mock::given(method("GET"))
            .and(path("/v2/library/alpine/manifests/latest"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(
                serde_json::to_string(&manifest).unwrap(),
                registry::MEDIA_OCI_MANIFEST,
            ))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path(format!("/v2/library/alpine/blobs/{config_digest}")))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(config_data.to_vec()))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path(format!("/v2/library/alpine/blobs/{layer_digest}")))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(layer_data.to_vec()))
            .mount(&server)
            .await;

        let (stiva, _dir) = mock_stiva(&server).await;

        // Use a reference that resolves to the mock server's registry.
        let img_ref = format!("{}/library/alpine:latest", server.address());
        let image = stiva.pull(&img_ref).await.unwrap();
        assert_eq!(image.id, config_digest);
        assert_eq!(image.layers.len(), 1);

        let images = stiva.images().await.unwrap();
        assert_eq!(images.len(), 1);
    }

    #[tokio::test]
    async fn stiva_run_with_mock() {
        let server = MockServer::start().await;

        let config_data = br#"{"os":"linux"}"#;
        let config_digest = test_digest(config_data);

        let manifest = registry::OciManifest {
            schema_version: 2,
            media_type: None,
            config: registry::Descriptor {
                media_type: "application/vnd.oci.image.config.v1+json".into(),
                digest: config_digest.clone(),
                size: config_data.len() as u64,
            },
            layers: vec![],
        };

        Mock::given(method("GET"))
            .and(path("/v2/library/alpine/manifests/latest"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(
                serde_json::to_string(&manifest).unwrap(),
                registry::MEDIA_OCI_MANIFEST,
            ))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path(format!("/v2/library/alpine/blobs/{config_digest}")))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(config_data.to_vec()))
            .mount(&server)
            .await;

        let (stiva, _dir) = mock_stiva(&server).await;
        let img_ref = format!("{}/library/alpine:latest", server.address());

        // run() calls pull → create → start. start() invokes kavach sandbox
        // with Process backend (no crun/runc needed). For one-shot exec, the
        // container runs to completion and transitions to Stopped.
        let c = stiva
            .run(&img_ref, container::ContainerConfig::default())
            .await
            .unwrap();
        assert!(!c.id.is_empty());

        let ps = stiva.ps().await.unwrap();
        assert_eq!(ps.len(), 1);
        // One-shot exec: container has already run and stopped.
        assert_eq!(ps[0].state, container::ContainerState::Stopped);
        assert!(ps[0].started_at.is_some());
        assert!(ps[0].exit_code.is_some());
    }
}
