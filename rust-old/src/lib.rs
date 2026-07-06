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
//! - [`build`] — Image building from TOML-based build specs (Stivafile)
//! - [`ansamblu`] — Multi-service orchestration (ansamblu = ensemble)
//! - [`health`] — Container health monitoring via majra heartbeat
//! - [`agent`] — Daimon agent registration
//! - [`fleet`] — Daimon edge fleet scheduling
//! - [`mcp`] — MCP tools for AI agent integration
//! - [`intents`] — Agnoshi intent system (stub)
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use stiva::{Stiva, StivaConfig};
//! use stiva::container::ContainerConfig;
//!
//! # async fn example() -> Result<(), stiva::StivaError> {
//! let stiva = Stiva::new(StivaConfig::default()).await?;
//!
//! // Pull and run
//! let container = stiva.run("alpine:latest", ContainerConfig::default()).await?;
//!
//! // Inspect
//! let info = stiva.inspect(&container.id).await?;
//! println!("state: {:?}", info.state);
//!
//! // Clean up
//! stiva.stop(&container.id).await?;
//! stiva.rm(&container.id).await?;
//! # Ok(())
//! # }

pub mod agent;
#[cfg(feature = "ansamblu")]
pub mod ansamblu;
pub mod audit;
pub mod build;
pub mod container;
pub mod convert;
pub mod encrypted;
pub mod fleet;
pub mod health;
pub mod image;
pub mod intents;
pub mod mcp;
pub mod network;
pub mod oci;
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
    audit: Option<Arc<audit::AuditLog>>,
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

    /// Path to the audit log file. `None` disables audit logging.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audit_log: Option<std::path::PathBuf>,
}

impl Default for StivaConfig {
    fn default() -> Self {
        Self {
            root_path: std::path::PathBuf::from("/var/lib/agnos/containers"),
            image_path: std::path::PathBuf::from("/var/lib/agnos/images"),
            default_network: network::NetworkMode::Bridge,
            max_containers: 64,
            audit_log: None,
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
        let audit = config
            .audit_log
            .as_ref()
            .map(|p| audit::AuditLog::new(p).map(Arc::new))
            .transpose()?;

        Ok(Self {
            image_store,
            registry_client,
            containers,
            audit,
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
        let audit = config
            .audit_log
            .as_ref()
            .map(|p| audit::AuditLog::new(p).map(Arc::new))
            .transpose()?;

        Ok(Self {
            image_store,
            registry_client,
            containers,
            audit,
            config,
        })
    }

    /// Pull an OCI image from a registry.
    pub async fn pull(&self, reference: &str) -> Result<image::Image, StivaError> {
        let parsed = image::ImageRef::parse(reference)?;
        let result = self.image_store.pull(&parsed, &self.registry_client).await;
        self.emit_audit(audit::AuditEntry::image(
            audit::AuditOperation::Pull,
            reference,
            match &result {
                Ok(_) => audit::AuditResult::Success,
                Err(e) => audit::AuditResult::Failed(e.to_string()),
            },
        ));
        result
    }

    /// Create and start a container.
    ///
    /// Returns the container with its post-start state (Stopped for one-shot,
    /// Running for daemon).
    pub async fn run(
        &self,
        image: &str,
        config: container::ContainerConfig,
    ) -> Result<container::Container, StivaError> {
        info!(image, "stiva run");
        let img = self.pull(image).await?;
        let container = self.containers.create(&img, config).await?;
        self.containers.start(&container.id).await?;
        // Return updated state (start may have changed it).
        self.containers
            .list()
            .await?
            .into_iter()
            .find(|c| c.id == container.id)
            .ok_or_else(|| StivaError::ContainerNotFound(container.id))
    }

    /// List running containers.
    pub async fn ps(&self) -> Result<Vec<container::Container>, StivaError> {
        self.containers.list().await
    }

    /// Stop a container.
    pub async fn stop(&self, id: &str) -> Result<(), StivaError> {
        info!(container = id, "stiva stop");
        let result = self.containers.stop(id).await;
        self.emit_audit(audit::AuditEntry::container(
            audit::AuditOperation::Stop,
            id,
            match &result {
                Ok(()) => audit::AuditResult::Success,
                Err(e) => audit::AuditResult::Failed(e.to_string()),
            },
        ));
        result
    }

    /// Remove a container.
    pub async fn rm(&self, id: &str) -> Result<(), StivaError> {
        info!(container = id, "stiva rm");
        let result = self.containers.remove(id).await;
        self.emit_audit(audit::AuditEntry::container(
            audit::AuditOperation::Remove,
            id,
            match &result {
                Ok(()) => audit::AuditResult::Success,
                Err(e) => audit::AuditResult::Failed(e.to_string()),
            },
        ));
        result
    }

    /// Restart a stopped container.
    pub async fn restart(&self, id: &str) -> Result<(), StivaError> {
        info!(container = id, "stiva restart");
        self.containers.restart(id).await
    }

    /// Send a signal to a running container.
    pub async fn signal(&self, id: &str, signal: i32) -> Result<(), StivaError> {
        let result = self.containers.signal(id, signal).await;
        self.emit_audit(
            audit::AuditEntry::container(
                audit::AuditOperation::Kill,
                id,
                match &result {
                    Ok(()) => audit::AuditResult::Success,
                    Err(e) => audit::AuditResult::Failed(e.to_string()),
                },
            )
            .with_metadata(serde_json::json!({"signal": signal})),
        );
        result
    }

    /// Pause a running container via cgroups freezer.
    pub async fn pause(&self, id: &str) -> Result<(), StivaError> {
        info!(container = id, "stiva pause");
        self.containers.pause(id).await
    }

    /// Unpause a paused container.
    pub async fn unpause(&self, id: &str) -> Result<(), StivaError> {
        info!(container = id, "stiva unpause");
        self.containers.unpause(id).await
    }

    /// Get runtime stats for a container (CPU, memory, PIDs).
    pub async fn stats(&self, id: &str) -> Result<runtime::ContainerStats, StivaError> {
        self.containers.stats(id).await
    }

    /// Build an image from a TOML build spec (Stivafile).
    ///
    /// `context_dir` is the directory containing files referenced by `copy` steps.
    pub async fn build(
        &self,
        toml_content: &str,
        context_dir: &std::path::Path,
    ) -> Result<image::Image, StivaError> {
        let spec = build::parse_build_spec(toml_content)?;
        info!(
            base = spec.image.base.as_str(),
            name = spec.image.name.as_str(),
            tag = spec.image.tag.as_str(),
            steps = spec.steps.len(),
            "stiva build"
        );
        build::build_image(&spec, &self.image_store, &self.registry_client, context_dir).await
    }

    /// Push a local image to a registry.
    ///
    /// If `target` is provided, the image is pushed to that reference.
    /// Otherwise, it is pushed to its original pull reference.
    pub async fn push(&self, image_id: &str, target: Option<&str>) -> Result<(), StivaError> {
        let images = self.image_store.list()?;
        let image = images
            .iter()
            .find(|i| i.id == image_id || i.reference.full_ref() == image_id)
            .ok_or_else(|| StivaError::ImageNotFound(image_id.to_string()))?;

        let target_ref = match target {
            Some(t) => image::ImageRef::parse(t)?,
            None => image.reference.clone(),
        };

        info!(image = %target_ref.full_ref(), "stiva push");
        self.image_store
            .push(image, &target_ref, &self.registry_client)
            .await
    }

    /// List local images.
    pub async fn images(&self) -> Result<Vec<image::Image>, StivaError> {
        self.image_store.list()
    }

    /// Remove a local image by ID or reference.
    ///
    /// When called with a tag reference (e.g. "nginx:latest"), removes only
    /// that tag. When called with a digest ID, removes the image and all tags.
    pub fn rmi(&self, image_id: &str) -> Result<(), StivaError> {
        info!(image = image_id, "stiva rmi");
        // Try removal by ID first (removes all tags for that digest).
        match self.image_store.remove(image_id) {
            Ok(()) => Ok(()),
            Err(StivaError::ImageNotFound(_)) => {
                // Try by reference — remove just the matching tag.
                let images = self.image_store.list()?;
                let match_ref = images
                    .iter()
                    .find(|i| i.reference.full_ref().contains(image_id))
                    .map(|i| i.reference.full_ref());
                match match_ref {
                    Some(ref_str) => {
                        let remaining: Vec<_> = images
                            .into_iter()
                            .filter(|i| i.reference.full_ref() != ref_str)
                            .collect();
                        self.image_store.save_index_pub(&remaining)
                    }
                    None => Err(StivaError::ImageNotFound(image_id.to_string())),
                }
            }
            Err(e) => Err(e),
        }
    }

    /// Tag a local image with a new reference.
    pub fn tag(&self, image_id: &str, new_ref: &str) -> Result<(), StivaError> {
        info!(image = image_id, tag = new_ref, "stiva tag");
        let parsed = image::ImageRef::parse(new_ref)?;
        let images = self.image_store.list()?;
        let image = images
            .iter()
            .find(|i| i.id == image_id || i.reference.full_ref() == image_id)
            .ok_or_else(|| StivaError::ImageNotFound(image_id.to_string()))?;

        let tagged = image::Image {
            id: image.id.clone(),
            reference: parsed,
            size_bytes: image.size_bytes,
            layers: image.layers.clone(),
            created_at: image.created_at,
        };
        self.image_store.add_to_index(&tagged)
    }

    /// Inspect a local image — returns full details.
    pub fn inspect_image(&self, image_id: &str) -> Result<image::Image, StivaError> {
        let images = self.image_store.list()?;
        images
            .into_iter()
            .find(|i| i.id == image_id || i.reference.full_ref() == image_id)
            .ok_or_else(|| StivaError::ImageNotFound(image_id.to_string()))
    }

    /// Inspect a container — returns full details.
    pub async fn inspect(&self, id: &str) -> Result<container::Container, StivaError> {
        let containers = self.containers.list().await?;
        containers
            .into_iter()
            .find(|c| c.id == id || c.name.as_deref() == Some(id))
            .ok_or_else(|| StivaError::ContainerNotFound(id.to_string()))
    }

    /// Remove all stopped containers and unreferenced images.
    pub async fn prune(&self) -> Result<(u32, u32), StivaError> {
        info!("stiva prune");
        // Remove stopped containers.
        let containers = self.containers.list().await?;
        let mut removed_containers = 0u32;
        for c in &containers {
            if c.state == container::ContainerState::Stopped {
                let _ = self.containers.remove(&c.id).await;
                removed_containers += 1;
            }
        }

        // Remove images not referenced by any remaining container.
        let remaining = self.containers.list().await?;
        let in_use: std::collections::HashSet<String> =
            remaining.iter().map(|c| c.image_id.clone()).collect();
        let images = self.image_store.list()?;
        let mut removed_images = 0u32;
        for img in &images {
            if !in_use.contains(&img.id) {
                let _ = self.image_store.remove(&img.id);
                removed_images += 1;
            }
        }

        info!(removed_containers, removed_images, "prune complete");
        Ok((removed_containers, removed_images))
    }

    /// List processes inside a running container.
    pub async fn top(&self, id: &str) -> Result<Vec<runtime::ProcessInfo>, StivaError> {
        let pid = self
            .containers
            .require_pid(id, &[container::ContainerState::Running], "top")
            .await?;
        runtime::container_top(pid).await
    }

    /// Export a container's rootfs as a tar archive.
    pub async fn export(&self, id: &str, output: &std::path::Path) -> Result<(), StivaError> {
        info!(container = id, output = %output.display(), "stiva export");
        let rootfs = self.containers.get_rootfs(id).await?;
        runtime::export_rootfs(&rootfs, output).await
    }

    /// Import a tar archive as a new local image.
    pub fn import(
        &self,
        tar_path: &std::path::Path,
        name: &str,
        tag: &str,
    ) -> Result<image::Image, StivaError> {
        info!(tar = %tar_path.display(), name, tag, "stiva import");
        runtime::import_rootfs(tar_path, &self.image_store, name, tag)
    }

    /// Copy a file from the host into a container.
    pub async fn cp_into(
        &self,
        id: &str,
        host_src: &std::path::Path,
        container_dst: &std::path::Path,
    ) -> Result<(), StivaError> {
        let rootfs = self.containers.get_rootfs(id).await?;
        runtime::copy_into_container(&rootfs, host_src, container_dst)
    }

    /// Copy a file from a container to the host.
    pub async fn cp_from(
        &self,
        id: &str,
        container_src: &std::path::Path,
        host_dst: &std::path::Path,
    ) -> Result<(), StivaError> {
        let rootfs = self.containers.get_rootfs(id).await?;
        runtime::copy_from_container(&rootfs, container_src, host_dst)
    }

    /// Wait for a container to exit. Returns execution result.
    pub async fn wait(&self, id: &str) -> Result<runtime::ContainerExecResult, StivaError> {
        info!(container = id, "stiva wait");
        self.containers.wait(id).await
    }

    /// Execute a command inside a running container.
    pub async fn exec(
        &self,
        id: &str,
        command: &[String],
    ) -> Result<runtime::ContainerExecResult, StivaError> {
        info!(container = id, command = ?command, "stiva exec");
        let result = self.containers.exec(id, command).await;
        self.emit_audit(
            audit::AuditEntry::container(
                audit::AuditOperation::Exec,
                id,
                match &result {
                    Ok(_) => audit::AuditResult::Success,
                    Err(e) => audit::AuditResult::Failed(e.to_string()),
                },
            )
            .with_metadata(serde_json::json!({"command": command})),
        );
        result
    }

    /// Read container logs.
    pub async fn logs(&self, id: &str) -> Result<String, StivaError> {
        self.containers.logs(id).await
    }

    /// Read the last N lines of container logs.
    pub async fn log_tail(&self, id: &str, lines: usize) -> Result<String, StivaError> {
        self.containers.log_tail(id, lines).await
    }

    /// Compute the security strength score for the default sandbox backend.
    ///
    /// Returns a kavach `StrengthScore` (0–100) reflecting the isolation
    /// strength of the auto-detected backend with basic policy.
    #[must_use = "security score should be used or displayed"]
    pub fn security_score(&self) -> kavach::StrengthScore {
        runtime::security_score()
    }

    /// Compute the security strength score for a container's sandbox configuration.
    ///
    /// Reads the container's backend and policy, then delegates to
    /// `kavach::score_backend()`.
    pub async fn container_security_score(
        &self,
        id: &str,
    ) -> Result<kavach::StrengthScore, StivaError> {
        let _container = self.inspect(id).await?;

        // Derive backend and policy from what build_sandbox would use.
        let backend = if kavach::Backend::Oci.is_available() {
            kavach::Backend::Oci
        } else {
            kavach::Backend::Process
        };
        let policy = kavach::SandboxPolicy::basic();

        Ok(runtime::security_score_for(backend, &policy))
    }

    /// Checkpoint a running daemon container via CRIU.
    ///
    /// Creates a snapshot of the container's process state. Returns the
    /// checkpoint directory path. If `leave_running` is false, the container
    /// transitions to Paused.
    pub async fn checkpoint(
        &self,
        id: &str,
        leave_running: bool,
    ) -> Result<std::path::PathBuf, StivaError> {
        info!(container = id, leave_running, "stiva checkpoint");
        self.containers.checkpoint(id, leave_running).await
    }

    /// Restore a container from a CRIU checkpoint.
    ///
    /// Restores process state and transitions the container back to Running.
    pub async fn restore(
        &self,
        id: &str,
        checkpoint_dir: &std::path::Path,
    ) -> Result<(), StivaError> {
        info!(container = id, checkpoint_dir = %checkpoint_dir.display(), "stiva restore");
        self.containers.restore(id, checkpoint_dir).await
    }

    /// Prepare a container for live migration — checkpoint and package for transfer.
    ///
    /// Returns a migration bundle containing everything needed to transfer
    /// the container to another node.
    pub async fn prepare_migration(
        &self,
        id: &str,
    ) -> Result<container::MigrationBundle, StivaError> {
        info!(container = id, "stiva prepare_migration");
        self.containers.prepare_migration(id).await
    }

    /// Apply a migration bundle — restore a container from a transferred checkpoint.
    ///
    /// Creates a new container from the bundle's config, then restores
    /// process state from the checkpoint data.
    pub async fn apply_migration(
        &self,
        bundle: &container::MigrationBundle,
    ) -> Result<container::Container, StivaError> {
        info!(
            source = %bundle.source_container_id,
            "stiva apply_migration"
        );
        self.containers.apply_migration(bundle).await
    }

    /// Deploy an ansamblu file — parse, resolve dependencies, create and start services.
    #[cfg(feature = "ansamblu")]
    pub async fn ansamblu_up(
        &self,
        toml_content: &str,
    ) -> Result<ansamblu::AnsambluSession, StivaError> {
        info!(
            services = toml_content.matches("[services.").count(),
            "ansamblu up"
        );
        let ansamblu_file = ansamblu::parse_ansamblu(toml_content)?;
        let startup_order = ansamblu::resolve_startup_order(&ansamblu_file)?;

        let session_id = uuid::Uuid::new_v4().to_string();
        let mut services: std::collections::HashMap<String, Vec<String>> =
            std::collections::HashMap::new();

        for service_name in &startup_order {
            let service = ansamblu_file.services.get(service_name).ok_or_else(|| {
                StivaError::Ansamblu(format!("service '{service_name}' not found"))
            })?;

            let replicas = ansamblu::replica_count(service);
            let mut container_ids = Vec::new();

            for i in 0..replicas {
                let config = ansamblu::service_to_config(service_name, service, i);

                // Pull image (may already be cached).
                let img = self.pull(&service.image).await?;

                // Create and start container.
                let container = self.containers.create(&img, config).await?;
                let _ = self.containers.start(&container.id).await;
                container_ids.push(container.id);
            }

            services.insert(service_name.clone(), container_ids);
        }

        Ok(ansamblu::AnsambluSession {
            id: session_id,
            services,
            networks: vec![],
            startup_order,
            created_at: chrono::Utc::now(),
        })
    }

    /// Tear down an ansamblu session — stop and remove all containers.
    #[cfg(feature = "ansamblu")]
    pub async fn ansamblu_down(
        &self,
        session: &ansamblu::AnsambluSession,
    ) -> Result<(), StivaError> {
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

    /// Get the pub/sub event bus for lifecycle events.
    pub fn event_bus(&self) -> &majra::pubsub::PubSub {
        self.containers.event_bus()
    }

    /// Get the rootfs path for a container (for diff/export).
    pub async fn get_rootfs(&self, id: &str) -> Result<std::path::PathBuf, StivaError> {
        self.containers.get_rootfs(id).await
    }

    /// Rename a container.
    pub async fn rename(&self, id: &str, new_name: &str) -> Result<(), StivaError> {
        info!(container = id, new_name, "stiva rename");
        self.containers.rename(id, new_name).await
    }

    /// Update resource limits on a running container.
    pub async fn update(
        &self,
        id: &str,
        memory_limit: u64,
        cpu_shares: u64,
        max_pids: u32,
    ) -> Result<(), StivaError> {
        info!(container = id, "stiva update");
        self.containers
            .update(id, memory_limit, cpu_shares, max_pids)
            .await
    }

    /// Garbage-collect unreferenced image blobs and layers.
    pub fn gc(&self) -> Result<(u32, u32), StivaError> {
        info!("stiva gc");
        self.image_store.gc()
    }

    /// Scale a service within an ansamblu session.
    ///
    /// Adjusts the replica count by creating or removing containers.
    /// Returns updated container ID list for the service.
    #[cfg(feature = "ansamblu")]
    pub async fn ansamblu_scale(
        &self,
        session: &mut ansamblu::AnsambluSession,
        service_name: &str,
        service: &ansamblu::ServiceDef,
        desired: u32,
    ) -> Result<Vec<String>, StivaError> {
        info!(service = service_name, desired, "stiva ansamblu scale");
        let (to_add, to_remove) = ansamblu::compute_scale(session, service_name, desired);

        // Remove excess replicas.
        for id in &to_remove {
            let _ = self.containers.stop(id).await;
            let _ = self.containers.remove(id).await;
        }
        if let Some(ids) = session.services.get_mut(service_name) {
            ids.retain(|id| !to_remove.contains(id));
        }

        // Add new replicas.
        let current_count = session
            .services
            .get(service_name)
            .map(|ids| ids.len() as u32)
            .unwrap_or(0);

        for i in 0..to_add {
            let config = ansamblu::service_to_config(service_name, service, current_count + i);
            let img = self.pull(&service.image).await?;
            let container = self.containers.create(&img, config).await?;
            let _ = self.containers.start(&container.id).await;
            session
                .services
                .entry(service_name.to_string())
                .or_default()
                .push(container.id);
        }

        Ok(session
            .services
            .get(service_name)
            .cloned()
            .unwrap_or_default())
    }

    /// Access the audit log (if configured).
    #[must_use]
    pub fn audit(&self) -> Option<&audit::AuditLog> {
        self.audit.as_deref()
    }

    /// Emit an audit entry (no-op if audit is disabled).
    fn emit_audit(&self, entry: audit::AuditEntry) {
        if let Some(ref log) = self.audit
            && let Err(e) = log.log(&entry)
        {
            tracing::warn!(error = %e, "failed to write audit entry");
        }
    }

    /// Get aggregated logs for all replicas of a service.
    #[cfg(feature = "ansamblu")]
    pub async fn service_logs(
        &self,
        session: &ansamblu::AnsambluSession,
        service_name: &str,
    ) -> Result<String, StivaError> {
        let ids = ansamblu::service_container_ids(session, service_name);
        let mut combined = String::new();
        for id in &ids {
            match self.containers.logs(id).await {
                Ok(logs) => {
                    combined.push_str(&format!("=== {id} ===\n{logs}\n"));
                }
                Err(e) => {
                    combined.push_str(&format!("=== {id} === (error: {e})\n"));
                }
            }
        }
        Ok(combined)
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
            ..Default::default()
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
            ..Default::default()
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
            audit: None,
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

        let manifest = registry::OciManifest::new(
            registry::Descriptor::new(
                "application/vnd.oci.image.config.v1+json",
                &config_digest,
                config_data.len() as u64,
            ),
            vec![registry::Descriptor::new(
                "application/vnd.oci.image.layer.v1.tar+gzip",
                &layer_digest,
                layer_data.len() as u64,
            )],
        );

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

        let manifest = registry::OciManifest::new(
            registry::Descriptor::new(
                "application/vnd.oci.image.config.v1+json",
                &config_digest,
                config_data.len() as u64,
            ),
            vec![],
        );

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
