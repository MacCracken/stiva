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
    /// Whether to run rootless (user namespace with UID remapping).
    pub rootless: bool,
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
#[non_exhaustive]
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
#[must_use]
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
#[must_use = "spec generation returns a new RuntimeSpec"]
pub fn generate_spec(container: &Container, rootfs: &Path) -> Result<RuntimeSpec, StivaError> {
    let config = &container.config;

    let env: Vec<String> = config
        .env
        .iter()
        .map(|(k, v)| {
            let mut s = String::with_capacity(k.len() + 1 + v.len());
            s.push_str(k);
            s.push('=');
            s.push_str(v);
            s
        })
        .collect();

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

    let mut namespaces = vec![
        Namespace::Pid,
        Namespace::Net,
        Namespace::Mount,
        Namespace::Uts,
        Namespace::Ipc,
    ];
    if config.rootless {
        namespaces.push(Namespace::User);
    }

    Ok(RuntimeSpec {
        rootfs: rootfs.to_path_buf(),
        command,
        env,
        namespaces,
        memory_limit_bytes,
        cpu_shares,
        max_pids: None, // TODO: Add max_pids to ContainerConfig
        user: config.user.clone(),
        workdir,
        read_only_rootfs: false,
        mounts,
        rootless: config.rootless,
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

/// Build a kavach sandbox from a RuntimeSpec (shared between exec and spawn).
async fn build_sandbox(spec: &RuntimeSpec) -> Result<(kavach::Sandbox, String), StivaError> {
    let command = spec.command.join(" ");

    let mut policy = kavach::SandboxPolicy::basic();
    if let Some(mem) = spec.memory_limit_bytes {
        policy.memory_limit_mb = Some(mem / (1024 * 1024));
    }
    if let Some(pids) = spec.max_pids {
        policy.max_pids = Some(pids);
    }
    policy.network.enabled = false;
    policy.read_only_rootfs = spec.read_only_rootfs;

    let backend = if kavach::Backend::Oci.is_available() {
        kavach::Backend::Oci
    } else {
        kavach::Backend::Process
    };
    debug!(backend = %backend, rootless = spec.rootless, "selected sandbox backend");

    let config = kavach::SandboxConfig::builder()
        .backend(backend)
        .policy(policy)
        .timeout_ms(0)
        .build();

    let mut sandbox = kavach::Sandbox::create(config).await.map_err(|e| {
        error!(error = %e, "failed to create sandbox");
        StivaError::Sandbox(format!("failed to create sandbox: {e}"))
    })?;

    sandbox
        .transition(kavach::SandboxState::Running)
        .map_err(|e| StivaError::Sandbox(format!("failed to transition sandbox: {e}")))?;

    Ok((sandbox, command))
}

/// Execute a container using kavach sandbox (one-shot).
///
/// Converts the RuntimeSpec into a kavach SandboxConfig, creates a sandbox,
/// and runs the container command. Returns when the command completes.
pub async fn exec_container(spec: &RuntimeSpec) -> Result<ContainerExecResult, StivaError> {
    info!(
        command = spec.command.join(" ").as_str(),
        rootfs = %spec.rootfs.display(),
        "executing one-shot container via kavach"
    );

    let (sandbox, command) = build_sandbox(spec).await?;

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

/// A handle to a running daemon container process.
///
/// Wraps a kavach `SpawnedProcess` and the sandbox that owns it.
/// Dropping this handle will NOT kill the process — call `kill()` or `wait()`.
pub struct DaemonHandle {
    process: kavach::SpawnedProcess,
    // Keep sandbox alive so backend resources are not released.
    _sandbox: kavach::Sandbox,
}

impl DaemonHandle {
    /// Get the OS process ID of the daemon.
    #[inline]
    #[must_use]
    pub fn pid(&self) -> Option<u32> {
        self.process.pid()
    }

    /// Wait for the daemon to exit naturally.
    pub async fn wait(self) -> Result<ContainerExecResult, StivaError> {
        let result = self
            .process
            .wait()
            .await
            .map_err(|e| StivaError::Sandbox(format!("daemon wait failed: {e}")))?;
        let _ = self._sandbox.destroy().await;
        Ok(ContainerExecResult {
            exit_code: result.exit_code,
            stdout: result.stdout,
            stderr: result.stderr,
            duration_ms: result.duration_ms,
            timed_out: result.timed_out,
        })
    }

    /// Send SIGTERM, wait up to `grace_ms`, then SIGKILL.
    pub async fn kill(self, grace_ms: u64) -> Result<ContainerExecResult, StivaError> {
        let result = self
            .process
            .kill(grace_ms)
            .await
            .map_err(|e| StivaError::Sandbox(format!("daemon kill failed: {e}")))?;
        let _ = self._sandbox.destroy().await;
        Ok(ContainerExecResult {
            exit_code: result.exit_code,
            stdout: result.stdout,
            stderr: result.stderr,
            duration_ms: result.duration_ms,
            timed_out: result.timed_out,
        })
    }

    /// Non-blocking check if process is still running.
    /// Returns `Some(exit_code)` if exited, `None` if still running.
    pub fn try_wait(&mut self) -> Result<Option<i32>, StivaError> {
        self.process
            .try_wait()
            .map_err(|e| StivaError::Sandbox(format!("daemon try_wait failed: {e}")))
    }
}

/// Spawn a long-running daemon container via kavach sandbox.
///
/// Returns a `DaemonHandle` that can be used to wait, kill, or inspect the process.
pub async fn spawn_container(spec: &RuntimeSpec) -> Result<DaemonHandle, StivaError> {
    info!(
        command = spec.command.join(" ").as_str(),
        rootfs = %spec.rootfs.display(),
        "spawning daemon container via kavach"
    );

    let (sandbox, command) = build_sandbox(spec).await?;

    let process = sandbox.spawn(&command).await.map_err(|e| {
        error!(error = %e, "sandbox spawn failed");
        StivaError::Sandbox(format!("sandbox spawn failed: {e}"))
    })?;

    info!("daemon container spawned");

    Ok(DaemonHandle {
        process,
        _sandbox: sandbox,
    })
}

// ---------------------------------------------------------------------------
// Exec into running container
// ---------------------------------------------------------------------------

/// Execute a command inside a running container's namespaces via `nsenter`.
///
/// Enters the PID, mount, network, UTS, and IPC namespaces of the target
/// process and runs the command. Returns when the command completes.
pub async fn exec_in_container(
    pid: u32,
    command: &[String],
    env: &[(String, String)],
    workdir: Option<&str>,
) -> Result<ContainerExecResult, StivaError> {
    if command.is_empty() {
        return Err(StivaError::Runtime("empty exec command".into()));
    }

    let pid_str = pid.to_string();
    info!(pid, command = ?command, "exec into running container via nsenter");

    let mut cmd = tokio::process::Command::new("nsenter");
    cmd.arg("-t")
        .arg(&pid_str)
        .arg("-p") // PID namespace
        .arg("-m") // mount namespace
        .arg("-n") // network namespace
        .arg("-u") // UTS namespace
        .arg("-i") // IPC namespace
        .arg("--")
        .args(command);

    for (k, v) in env {
        cmd.env(k, v);
    }

    if let Some(wd) = workdir {
        cmd.current_dir(wd);
    }

    cmd.stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    let start = std::time::Instant::now();

    let output = cmd.output().await.map_err(|e| {
        error!(error = %e, "nsenter exec failed");
        StivaError::Runtime(format!("nsenter exec failed: {e}"))
    })?;

    let duration_ms = start.elapsed().as_millis() as u64;

    info!(
        exit_code = output.status.code().unwrap_or(-1),
        duration_ms, "exec complete"
    );

    Ok(ContainerExecResult {
        exit_code: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        duration_ms,
        timed_out: false,
    })
}

// ---------------------------------------------------------------------------
// Signal forwarding
// ---------------------------------------------------------------------------

/// Send a signal to a container process.
///
/// Common signals: SIGHUP(1), SIGINT(2), SIGQUIT(3), SIGTERM(15), SIGUSR1(10), SIGUSR2(12).
#[cfg(target_os = "linux")]
pub fn send_signal(pid: u32, signal: i32) -> Result<(), StivaError> {
    info!(pid, signal, "sending signal to container");
    let nix_signal = nix::sys::signal::Signal::try_from(signal)
        .map_err(|e| StivaError::Runtime(format!("invalid signal {signal}: {e}")))?;
    let nix_pid = nix::unistd::Pid::from_raw(pid as i32);
    nix::sys::signal::kill(nix_pid, nix_signal).map_err(|e| {
        StivaError::Runtime(format!("failed to send signal {signal} to PID {pid}: {e}"))
    })
}

#[cfg(not(target_os = "linux"))]
pub fn send_signal(_pid: u32, _signal: i32) -> Result<(), StivaError> {
    Err(StivaError::Runtime(
        "signal forwarding requires Linux".into(),
    ))
}

// ---------------------------------------------------------------------------
// Pause / unpause via cgroups freezer
// ---------------------------------------------------------------------------

/// Pause a container by freezing its cgroup.
///
/// Uses cgroups v2 freezer (`cgroup.freeze`) to suspend all processes
/// in the container's cgroup. This is instant and lightweight compared
/// to CRIU checkpointing.
pub async fn pause_container(pid: u32) -> Result<(), StivaError> {
    info!(pid, "pausing container via cgroup freezer");
    write_cgroup_file(pid, "cgroup.freeze", "1").await
}

/// Unpause a container by thawing its cgroup.
pub async fn unpause_container(pid: u32) -> Result<(), StivaError> {
    info!(pid, "unpausing container via cgroup freezer");
    write_cgroup_file(pid, "cgroup.freeze", "0").await
}

/// Resolve the cgroup v2 base path for a process.
///
/// Reads `/proc/{pid}/cgroup` and extracts the cgroup v2 path (format: `0::{path}`).
/// Returns the full sysfs path like `/sys/fs/cgroup/{path}`.
async fn resolve_cgroup_base(pid: u32) -> Result<String, StivaError> {
    let cgroup_info = tokio::fs::read_to_string(format!("/proc/{pid}/cgroup"))
        .await
        .map_err(|e| StivaError::Runtime(format!("failed to read cgroup for PID {pid}: {e}")))?;

    let cgroup_path = cgroup_info
        .lines()
        .find_map(|line| line.strip_prefix("0::"))
        .ok_or_else(|| StivaError::Runtime(format!("no cgroup v2 entry for PID {pid}")))?
        .trim();

    Ok(format!("/sys/fs/cgroup{cgroup_path}"))
}

/// Write a value to a cgroup v2 file for a process.
async fn write_cgroup_file(pid: u32, filename: &str, value: &str) -> Result<(), StivaError> {
    let base = resolve_cgroup_base(pid).await?;
    let file_path = format!("{base}/{filename}");

    tokio::fs::write(&file_path, value)
        .await
        .map_err(|e| StivaError::Runtime(format!("failed to write {value} to {file_path}: {e}")))?;

    debug!(pid, file = %file_path, value, "cgroup file written");
    Ok(())
}

// ---------------------------------------------------------------------------
// Cgroups v2 resource enforcement
// ---------------------------------------------------------------------------

/// Write cgroups v2 resource limits for a container process.
///
/// Sets memory.max, pids.max based on the RuntimeSpec limits.
/// Called after spawn for daemon containers. Best-effort: logs warnings
/// on failure but does not return errors (cgroups may not be available).
pub async fn apply_cgroup_limits(pid: u32, spec: &RuntimeSpec) {
    info!(pid, "applying cgroup v2 resource limits");

    let base = match resolve_cgroup_base(pid).await {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!(pid, error = %e, "could not resolve cgroup base, skipping limits");
            return;
        }
    };

    if let Some(mem) = spec.memory_limit_bytes
        && mem > 0
    {
        let path = format!("{base}/memory.max");
        let val = mem.to_string();
        if let Err(e) = tokio::fs::write(&path, &val).await {
            tracing::warn!(pid, path = %path, error = %e, "failed to write memory.max");
        } else {
            info!(pid, memory_max = mem, "cgroup memory.max applied");
        }
    }

    if let Some(pids) = spec.max_pids
        && pids > 0
    {
        let path = format!("{base}/pids.max");
        let val = pids.to_string();
        if let Err(e) = tokio::fs::write(&path, &val).await {
            tracing::warn!(pid, path = %path, error = %e, "failed to write pids.max");
        } else {
            info!(pid, pids_max = pids, "cgroup pids.max applied");
        }
    }
}

// ---------------------------------------------------------------------------
// Container stats (CPU / memory / PIDs)
// ---------------------------------------------------------------------------

/// Runtime statistics for a container.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerStats {
    /// Memory usage in bytes.
    pub memory_bytes: u64,
    /// Memory limit in bytes (0 = unlimited).
    pub memory_limit_bytes: u64,
    /// CPU usage in microseconds.
    pub cpu_usage_us: u64,
    /// Number of running PIDs.
    pub pids_current: u32,
    /// PID limit (0 = unlimited).
    pub pids_limit: u32,
}

/// Read runtime stats for a container process from cgroups v2.
pub async fn container_stats(pid: u32) -> Result<ContainerStats, StivaError> {
    let base = resolve_cgroup_base(pid).await?;

    let memory_bytes = read_cgroup_u64(&base, "memory.current").await.unwrap_or(0);
    let memory_limit_bytes = read_cgroup_u64(&base, "memory.max").await.unwrap_or(0);
    let cpu_usage_us = read_cpu_usage(&base).await.unwrap_or(0);
    let pids_current = read_cgroup_u64(&base, "pids.current").await.unwrap_or(0) as u32;
    let pids_limit = read_cgroup_u64(&base, "pids.max").await.unwrap_or(0) as u32;

    Ok(ContainerStats {
        memory_bytes,
        memory_limit_bytes,
        cpu_usage_us,
        pids_current,
        pids_limit,
    })
}

/// Read a single u64 from a cgroup file. Returns None on any failure.
async fn read_cgroup_u64(base: &str, filename: &str) -> Option<u64> {
    let path = format!("{base}/{filename}");
    let content = tokio::fs::read_to_string(&path).await.ok()?;
    let trimmed = content.trim();
    // "max" means unlimited — return 0.
    if trimmed == "max" {
        return Some(0);
    }
    trimmed.parse().ok()
}

/// Read CPU usage from cpu.stat (usage_usec field).
async fn read_cpu_usage(base: &str) -> Option<u64> {
    let path = format!("{base}/cpu.stat");
    let content = tokio::fs::read_to_string(&path).await.ok()?;
    for line in content.lines() {
        if let Some(val) = line.strip_prefix("usage_usec ") {
            return val.trim().parse().ok();
        }
    }
    None
}

// ---------------------------------------------------------------------------
// CRIU checkpoint / restore
// ---------------------------------------------------------------------------

/// Check if CRIU is available on the system.
///
/// Scans `PATH` for the `criu` binary, returning `true` if found.
#[must_use]
pub fn criu_available() -> bool {
    let Ok(path_var) = std::env::var("PATH") else {
        return false;
    };
    path_var
        .split(':')
        .any(|dir| Path::new(dir).join("criu").is_file())
}

/// Checkpoint a running container process via CRIU.
///
/// Dumps the process state to the given directory. The process is frozen
/// during checkpoint and optionally left running (`leave_running = true`)
/// or killed after dump.
pub async fn checkpoint_container(
    pid: u32,
    dump_dir: &Path,
    leave_running: bool,
) -> Result<(), StivaError> {
    info!(
        pid,
        dump_dir = %dump_dir.display(),
        leave_running,
        "checkpointing container via CRIU"
    );

    if !criu_available() {
        return Err(StivaError::Runtime(
            "CRIU is not available on this system".into(),
        ));
    }

    tokio::fs::create_dir_all(dump_dir).await.map_err(|e| {
        error!(error = %e, dump_dir = %dump_dir.display(), "failed to create checkpoint dir");
        StivaError::Runtime(format!("failed to create checkpoint directory: {e}"))
    })?;

    let mut cmd = tokio::process::Command::new("criu");
    cmd.arg("dump")
        .arg("--tree")
        .arg(pid.to_string())
        .arg("--images-dir")
        .arg(dump_dir)
        .arg("--shell-job");

    if leave_running {
        cmd.arg("--leave-running");
    }

    debug!(pid, dump_dir = %dump_dir.display(), leave_running, "running criu dump");

    let output = cmd.output().await.map_err(|e| {
        error!(error = %e, "failed to execute criu dump");
        StivaError::Runtime(format!("failed to execute criu dump: {e}"))
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        error!(
            pid,
            exit_code = output.status.code(),
            stderr = %stderr,
            "criu dump failed"
        );
        return Err(StivaError::Runtime(format!(
            "criu dump failed (exit {}): {stderr}",
            output.status.code().unwrap_or(-1),
        )));
    }

    info!(
        pid,
        dump_dir = %dump_dir.display(),
        "checkpoint complete"
    );
    Ok(())
}

/// Restore a container process from a CRIU checkpoint.
///
/// Returns the PID of the restored process.
pub async fn restore_container(dump_dir: &Path, rootfs: &Path) -> Result<u32, StivaError> {
    info!(
        dump_dir = %dump_dir.display(),
        rootfs = %rootfs.display(),
        "restoring container via CRIU"
    );

    if !criu_available() {
        return Err(StivaError::Runtime(
            "CRIU is not available on this system".into(),
        ));
    }

    let mut cmd = tokio::process::Command::new("criu");
    cmd.arg("restore")
        .arg("--images-dir")
        .arg(dump_dir)
        .arg("--root")
        .arg(rootfs)
        .arg("--shell-job")
        .arg("--restore-detached")
        .arg("--pidfile")
        .arg(dump_dir.join("restore.pid"));

    debug!(
        dump_dir = %dump_dir.display(),
        rootfs = %rootfs.display(),
        "running criu restore"
    );

    let output = cmd.output().await.map_err(|e| {
        error!(error = %e, "failed to execute criu restore");
        StivaError::Runtime(format!("failed to execute criu restore: {e}"))
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        error!(
            exit_code = output.status.code(),
            stderr = %stderr,
            "criu restore failed"
        );
        return Err(StivaError::Runtime(format!(
            "criu restore failed (exit {}): {stderr}",
            output.status.code().unwrap_or(-1),
        )));
    }

    // Read the PID file written by --pidfile.
    let pid_path = dump_dir.join("restore.pid");
    let pid_str = tokio::fs::read_to_string(&pid_path).await.map_err(|e| {
        error!(error = %e, path = %pid_path.display(), "failed to read restored PID");
        StivaError::Runtime(format!("failed to read restored PID file: {e}"))
    })?;

    let pid: u32 = pid_str.trim().parse().map_err(|e| {
        error!(error = %e, raw = %pid_str, "failed to parse restored PID");
        StivaError::Runtime(format!("failed to parse restored PID '{pid_str}': {e}"))
    })?;

    info!(
        pid,
        dump_dir = %dump_dir.display(),
        "restore complete"
    );
    Ok(pid)
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

    #[test]
    fn criu_available_returns_bool() {
        // Must not panic — just returns true/false based on PATH.
        let _available = criu_available();
    }

    #[tokio::test]
    async fn exec_in_container_empty_command() {
        let err = exec_in_container(1, &[], &[], None).await;
        assert!(err.is_err());
    }

    #[tokio::test]
    async fn apply_cgroup_limits_no_limits() {
        // With no limits set, apply_cgroup_limits should be a no-op (no panic).
        let spec = RuntimeSpec {
            rootfs: PathBuf::from("/tmp/rootfs"),
            command: vec!["/bin/sh".into()],
            env: vec![],
            namespaces: vec![],
            memory_limit_bytes: None,
            cpu_shares: None,
            max_pids: None,
            user: None,
            workdir: "/".into(),
            read_only_rootfs: false,
            mounts: vec![],
            rootless: false,
        };
        // PID 1 won't have a valid cgroup in test, so this should warn and return.
        apply_cgroup_limits(1, &spec).await;
    }

    #[tokio::test]
    async fn apply_cgroup_limits_with_limits() {
        // With limits set but invalid PID/cgroup, should warn and return.
        let spec = RuntimeSpec {
            rootfs: PathBuf::from("/tmp/rootfs"),
            command: vec!["/bin/sh".into()],
            env: vec![],
            namespaces: vec![],
            memory_limit_bytes: Some(256 * 1024 * 1024),
            cpu_shares: None,
            max_pids: Some(100),
            user: None,
            workdir: "/".into(),
            read_only_rootfs: false,
            mounts: vec![],
            rootless: false,
        };
        // Invalid PID — should warn but not panic.
        apply_cgroup_limits(99999999, &spec).await;
    }

    #[tokio::test]
    async fn apply_cgroup_limits_zero_values_skipped() {
        // Zero values should be skipped (no writes attempted).
        let spec = RuntimeSpec {
            rootfs: PathBuf::from("/tmp/rootfs"),
            command: vec!["/bin/sh".into()],
            env: vec![],
            namespaces: vec![],
            memory_limit_bytes: Some(0),
            cpu_shares: None,
            max_pids: Some(0),
            user: None,
            workdir: "/".into(),
            read_only_rootfs: false,
            mounts: vec![],
            rootless: false,
        };
        // Should skip because values are 0.
        apply_cgroup_limits(1, &spec).await;
    }
}
