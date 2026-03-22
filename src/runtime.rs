//! OCI runtime execution — bridges to kavach for process isolation.

use crate::container::Container;
use crate::error::StivaError;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tracing::{debug, error, info};

// ---------------------------------------------------------------------------
// Runtime spec types
// ---------------------------------------------------------------------------

/// OCI runtime configuration generated from container config.
#[derive(Debug, Clone)]
pub struct RuntimeSpec {
    /// Path to the container rootfs (overlay merged dir).
    pub rootfs: PathBuf,
    /// Command to execute inside the container.
    pub command: Vec<String>,
    /// Environment variables as `KEY=VALUE`.
    pub env: Vec<String>,
    /// Linux namespaces to create.
    pub namespaces: Vec<Namespace>,
    /// Memory limit in bytes (`None` = unlimited).
    pub memory_limit_bytes: Option<u64>,
    /// CPU shares (relative weight, `None` = default).
    pub cpu_shares: Option<u64>,
    /// Maximum number of PIDs (`None` = unlimited).
    pub max_pids: Option<u32>,
    /// User to run as inside the container.
    pub user: Option<String>,
    /// Working directory inside the container.
    pub workdir: String,
    /// Whether rootfs is read-only.
    pub read_only_rootfs: bool,
    /// Filesystem mounts (proc, sys, dev, volumes).
    pub mounts: Vec<SpecMount>,
}

/// A mount entry in the runtime spec.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpecMount {
    /// Source path on the host (or `None` for virtual filesystems).
    pub source: Option<PathBuf>,
    /// Destination path inside the container.
    pub destination: PathBuf,
    /// Mount type (e.g. "proc", "sysfs", "tmpfs", "bind").
    pub mount_type: String,
    /// Mount options (e.g. "nosuid", "noexec", "ro").
    pub options: Vec<String>,
}

/// Linux namespaces for container isolation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Namespace {
    Pid,
    Net,
    Mount,
    Uts,
    Ipc,
    User,
    Cgroup,
}

// ---------------------------------------------------------------------------
// Spec generation
// ---------------------------------------------------------------------------

/// Standard container mounts (/proc, /sys, /dev, etc.).
fn standard_mounts() -> Vec<SpecMount> {
    vec![
        SpecMount {
            source: None,
            destination: PathBuf::from("/proc"),
            mount_type: "proc".into(),
            options: vec!["nosuid".into(), "noexec".into(), "nodev".into()],
        },
        SpecMount {
            source: None,
            destination: PathBuf::from("/sys"),
            mount_type: "sysfs".into(),
            options: vec![
                "nosuid".into(),
                "noexec".into(),
                "nodev".into(),
                "ro".into(),
            ],
        },
        SpecMount {
            source: None,
            destination: PathBuf::from("/dev"),
            mount_type: "tmpfs".into(),
            options: vec!["nosuid".into(), "mode=755".into()],
        },
        SpecMount {
            source: None,
            destination: PathBuf::from("/dev/pts"),
            mount_type: "devpts".into(),
            options: vec![
                "nosuid".into(),
                "noexec".into(),
                "newinstance".into(),
                "mode=0620".into(),
            ],
        },
        SpecMount {
            source: None,
            destination: PathBuf::from("/dev/shm"),
            mount_type: "tmpfs".into(),
            options: vec![
                "nosuid".into(),
                "noexec".into(),
                "nodev".into(),
                "mode=1777".into(),
            ],
        },
    ]
}

/// Convert volume specs into SpecMount entries.
fn volume_mounts(volumes: &[String]) -> Result<Vec<SpecMount>, StivaError> {
    let mut mounts = Vec::new();
    for spec in volumes {
        let vol = crate::storage::parse_volume(spec)?;
        let mut options = vec!["rbind".into()];
        if vol.read_only {
            options.push("ro".into());
        }
        mounts.push(SpecMount {
            source: Some(vol.source),
            destination: vol.target,
            mount_type: "bind".into(),
            options,
        });
    }
    Ok(mounts)
}

/// Generate a full OCI runtime spec from a container and rootfs path.
pub fn generate_spec(container: &Container, rootfs: &Path) -> Result<RuntimeSpec, StivaError> {
    let config = &container.config;

    let env: Vec<String> = config.env.iter().map(|(k, v)| format!("{k}={v}")).collect();

    let command = if config.command.is_empty() {
        vec!["/bin/sh".to_string()]
    } else {
        config.command.clone()
    };

    let workdir = config.workdir.clone().unwrap_or_else(|| "/".to_string());

    // Resource limits — 0 means unlimited in ContainerConfig.
    let memory_limit_bytes = if config.memory_limit > 0 {
        Some(config.memory_limit)
    } else {
        None
    };

    let cpu_shares = if config.cpu_shares > 0 {
        Some(config.cpu_shares)
    } else {
        None
    };

    // Build mount list: standard mounts + user volumes.
    let mut mounts = standard_mounts();
    mounts.extend(volume_mounts(&config.volumes)?);

    Ok(RuntimeSpec {
        rootfs: rootfs.to_path_buf(),
        command,
        env,
        namespaces: vec![
            Namespace::Pid,
            Namespace::Net,
            Namespace::Mount,
            Namespace::Uts,
            Namespace::Ipc,
        ],
        memory_limit_bytes,
        cpu_shares,
        max_pids: None, // TODO: Add max_pids to ContainerConfig
        user: config.user.clone(),
        workdir,
        read_only_rootfs: false,
        mounts,
    })
}

// ---------------------------------------------------------------------------
// Container execution result
// ---------------------------------------------------------------------------

/// Result of executing a container command.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerExecResult {
    /// Process exit code (0 = success).
    pub exit_code: i32,
    /// Captured stdout.
    pub stdout: String,
    /// Captured stderr.
    pub stderr: String,
    /// Execution duration in milliseconds.
    pub duration_ms: u64,
    /// Whether the process was killed by timeout.
    pub timed_out: bool,
}

// ---------------------------------------------------------------------------
// kavach sandbox integration
// ---------------------------------------------------------------------------

/// Execute a container using kavach sandbox.
///
/// Converts the RuntimeSpec into a kavach SandboxConfig, creates a sandbox,
/// and runs the container command. Returns when the command completes.
pub async fn exec_container(spec: &RuntimeSpec) -> Result<ContainerExecResult, StivaError> {
    let command = spec.command.join(" ");
    info!(
        command = command.as_str(),
        rootfs = %spec.rootfs.display(),
        "executing container via kavach sandbox"
    );

    // Build kavach SandboxPolicy from spec resource limits.
    let mut policy = kavach::SandboxPolicy::basic();
    if let Some(mem) = spec.memory_limit_bytes {
        policy.memory_limit_mb = Some(mem / (1024 * 1024));
    }
    if let Some(pids) = spec.max_pids {
        policy.max_pids = Some(pids);
    }
    // Network disabled by default for containers.
    policy.network.enabled = false;
    policy.read_only_rootfs = spec.read_only_rootfs;

    let backend = if kavach::Backend::Oci.is_available() {
        kavach::Backend::Oci
    } else {
        kavach::Backend::Process
    };
    debug!(backend = %backend, "selected sandbox backend");

    // Build SandboxConfig.
    let config = kavach::SandboxConfig::builder()
        .backend(backend)
        .policy(policy)
        .timeout_ms(0) // No timeout for container execution.
        .build();

    // Create and run sandbox.
    let mut sandbox = kavach::Sandbox::create(config).await.map_err(|e| {
        error!(error = %e, "failed to create sandbox");
        StivaError::Sandbox(format!("failed to create sandbox: {e}"))
    })?;

    sandbox
        .transition(kavach::SandboxState::Running)
        .map_err(|e| StivaError::Sandbox(format!("failed to transition sandbox: {e}")))?;

    let result = sandbox.exec(&command).await.map_err(|e| {
        error!(error = %e, "sandbox execution failed");
        StivaError::Sandbox(format!("sandbox execution failed: {e}"))
    })?;

    let _ = sandbox.destroy().await;

    info!(
        exit_code = result.exit_code,
        duration_ms = result.duration_ms,
        timed_out = result.timed_out,
        "container execution complete"
    );

    Ok(ContainerExecResult {
        exit_code: result.exit_code,
        stdout: result.stdout,
        stderr: result.stderr,
        duration_ms: result.duration_ms,
        timed_out: result.timed_out,
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::container::ContainerConfig;

    fn test_container(config: ContainerConfig) -> Container {
        Container {
            id: "test".to_string(),
            name: None,
            image_id: "img".to_string(),
            image_ref: "alpine:latest".to_string(),
            state: crate::container::ContainerState::Created,
            pid: None,
            created_at: chrono::Utc::now(),
            started_at: None,
            config,
            exit_code: None,
        }
    }

    #[test]
    fn generate_spec_defaults() {
        let container = test_container(ContainerConfig::default());
        let spec = generate_spec(&container, Path::new("/tmp/rootfs")).unwrap();
        assert_eq!(spec.command, vec!["/bin/sh"]);
        assert_eq!(spec.namespaces.len(), 5);
        assert_eq!(spec.rootfs, PathBuf::from("/tmp/rootfs"));
        assert!(spec.env.is_empty());
        assert_eq!(spec.workdir, "/");
        assert!(spec.memory_limit_bytes.is_none());
        assert!(spec.cpu_shares.is_none());
        assert!(spec.max_pids.is_none());
        assert!(spec.user.is_none());
        assert!(!spec.read_only_rootfs);
    }

    #[test]
    fn generate_spec_with_command() {
        let config = ContainerConfig {
            command: vec!["nginx".into(), "-g".into(), "daemon off;".into()],
            ..Default::default()
        };
        let container = test_container(config);
        let spec = generate_spec(&container, Path::new("/rootfs")).unwrap();
        assert_eq!(spec.command, vec!["nginx", "-g", "daemon off;"]);
    }

    #[test]
    fn generate_spec_with_env() {
        let mut env = std::collections::HashMap::new();
        env.insert("PORT".to_string(), "8080".to_string());
        env.insert("DEBUG".to_string(), "1".to_string());
        let config = ContainerConfig {
            env,
            ..Default::default()
        };
        let container = test_container(config);
        let spec = generate_spec(&container, Path::new("/rootfs")).unwrap();
        assert_eq!(spec.env.len(), 2);
        assert!(spec.env.contains(&"PORT=8080".to_string()));
        assert!(spec.env.contains(&"DEBUG=1".to_string()));
    }

    #[test]
    fn generate_spec_with_resource_limits() {
        let config = ContainerConfig {
            memory_limit: 512 * 1024 * 1024, // 512MB
            cpu_shares: 1024,
            ..Default::default()
        };
        let container = test_container(config);
        let spec = generate_spec(&container, Path::new("/rootfs")).unwrap();
        assert_eq!(spec.memory_limit_bytes, Some(512 * 1024 * 1024));
        assert_eq!(spec.cpu_shares, Some(1024));
    }

    #[test]
    fn generate_spec_zero_limits_mean_unlimited() {
        let config = ContainerConfig {
            memory_limit: 0,
            cpu_shares: 0,
            ..Default::default()
        };
        let container = test_container(config);
        let spec = generate_spec(&container, Path::new("/rootfs")).unwrap();
        assert!(spec.memory_limit_bytes.is_none());
        assert!(spec.cpu_shares.is_none());
    }

    #[test]
    fn generate_spec_with_user_and_workdir() {
        let config = ContainerConfig {
            user: Some("nobody".into()),
            workdir: Some("/app".into()),
            ..Default::default()
        };
        let container = test_container(config);
        let spec = generate_spec(&container, Path::new("/rootfs")).unwrap();
        assert_eq!(spec.user.as_deref(), Some("nobody"));
        assert_eq!(spec.workdir, "/app");
    }

    #[test]
    fn generate_spec_with_volumes() {
        let config = ContainerConfig {
            volumes: vec![
                "/host/data:/container/data".into(),
                "/host/config:/etc/config:ro".into(),
            ],
            ..Default::default()
        };
        let container = test_container(config);
        let spec = generate_spec(&container, Path::new("/rootfs")).unwrap();

        // Standard mounts (5) + 2 user volumes.
        assert_eq!(spec.mounts.len(), 7);

        // Check the user volume mounts.
        let bind_mounts: Vec<_> = spec
            .mounts
            .iter()
            .filter(|m| m.mount_type == "bind")
            .collect();
        assert_eq!(bind_mounts.len(), 2);
        assert_eq!(bind_mounts[0].destination, PathBuf::from("/container/data"));
        assert!(bind_mounts[1].options.contains(&"ro".to_string()));
    }

    #[test]
    fn standard_mounts_complete() {
        let mounts = standard_mounts();
        assert_eq!(mounts.len(), 5);

        let dests: Vec<_> = mounts
            .iter()
            .map(|m| m.destination.to_str().unwrap())
            .collect();
        assert!(dests.contains(&"/proc"));
        assert!(dests.contains(&"/sys"));
        assert!(dests.contains(&"/dev"));
        assert!(dests.contains(&"/dev/pts"));
        assert!(dests.contains(&"/dev/shm"));
    }

    #[test]
    fn volume_mounts_conversion() {
        let specs = vec!["/a:/b".into(), "/c:/d:ro".into()];
        let mounts = volume_mounts(&specs).unwrap();
        assert_eq!(mounts.len(), 2);
        assert_eq!(mounts[0].mount_type, "bind");
        assert!(mounts[0].options.contains(&"rbind".to_string()));
        assert!(!mounts[0].options.contains(&"ro".to_string()));
        assert!(mounts[1].options.contains(&"ro".to_string()));
    }

    #[test]
    fn volume_mounts_invalid() {
        let specs = vec!["nocolon".into()];
        assert!(volume_mounts(&specs).is_err());
    }

    #[test]
    fn namespaces_are_correct() {
        let container = test_container(ContainerConfig::default());
        let spec = generate_spec(&container, Path::new("/rootfs")).unwrap();
        assert_eq!(spec.namespaces[0], Namespace::Pid);
        assert_eq!(spec.namespaces[1], Namespace::Net);
        assert_eq!(spec.namespaces[2], Namespace::Mount);
        assert_eq!(spec.namespaces[3], Namespace::Uts);
        assert_eq!(spec.namespaces[4], Namespace::Ipc);
    }

    #[test]
    fn namespace_serde() {
        for ns in [
            Namespace::Pid,
            Namespace::Net,
            Namespace::Mount,
            Namespace::Uts,
            Namespace::Ipc,
            Namespace::User,
            Namespace::Cgroup,
        ] {
            let json = serde_json::to_string(&ns).unwrap();
            let back: Namespace = serde_json::from_str(&json).unwrap();
            assert_eq!(ns, back);
        }
    }

    #[test]
    fn spec_mount_serde() {
        let mount = SpecMount {
            source: Some(PathBuf::from("/host")),
            destination: PathBuf::from("/container"),
            mount_type: "bind".into(),
            options: vec!["rbind".into(), "ro".into()],
        };
        let json = serde_json::to_string(&mount).unwrap();
        let back: SpecMount = serde_json::from_str(&json).unwrap();
        assert_eq!(back.destination, PathBuf::from("/container"));
        assert_eq!(back.options.len(), 2);
    }

    #[test]
    fn container_exec_result_serde() {
        let result = ContainerExecResult {
            exit_code: 0,
            stdout: "hello".into(),
            stderr: String::new(),
            duration_ms: 42,
            timed_out: false,
        };
        let json = serde_json::to_string(&result).unwrap();
        let back: ContainerExecResult = serde_json::from_str(&json).unwrap();
        assert_eq!(back.exit_code, 0);
        assert_eq!(back.stdout, "hello");
        assert_eq!(back.duration_ms, 42);
        assert!(!back.timed_out);
    }

    #[test]
    fn namespace_debug() {
        for ns in [
            Namespace::Pid,
            Namespace::Net,
            Namespace::Mount,
            Namespace::Uts,
            Namespace::Ipc,
            Namespace::User,
            Namespace::Cgroup,
        ] {
            let _ = format!("{ns:?}");
        }
    }
}
