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
    /// Secrets to inject into the sandbox (resolved at start time).
    pub secrets: Vec<kavach::SecretRef>,
    /// Output scanning policy (None = no scanning).
    pub scan_policy: Option<kavach::ExternalizationPolicy>,
    /// Execution timeout in milliseconds (0 = no timeout).
    pub timeout_ms: u64,
    /// Preferred backend name (None = auto-select strongest).
    pub backend: Option<String>,
    /// Minimum isolation strength score (0–100).
    pub min_isolation_score: Option<u8>,
    /// Agent ID for sandbox tracking.
    pub agent_id: Option<String>,
    /// Domain name for UTS namespace (OCI runtime-spec v1.2.0).
    pub domainname: Option<String>,
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
        max_pids: if config.max_pids > 0 {
            Some(config.max_pids)
        } else {
            None
        },
        user: config.user.clone(),
        workdir,
        read_only_rootfs: false,
        mounts,
        rootless: config.rootless,
        secrets: config.secrets.clone(),
        scan_policy: config.scan_policy.clone(),
        timeout_ms: config.timeout_ms,
        backend: config.backend.clone(),
        min_isolation_score: config.min_isolation_score,
        agent_id: config.agent_id.clone(),
        domainname: config.domainname.clone(),
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
    // Convert cpu_shares to fractional cores for kavach policy.
    if let Some(shares) = spec.cpu_shares
        && shares > 0
    {
        policy.cpu_limit = Some(shares as f64 / 1024.0);
    }
    policy.network.enabled = false;
    policy.read_only_rootfs = spec.read_only_rootfs;

    // Backend selection: explicit > min_strength > default fallback.
    let backend = if let Some(ref name) = spec.backend {
        name.parse::<kavach::Backend>()
            .map_err(|_| StivaError::Runtime(format!("unknown backend: {name}")))?
    } else if let Some(min_score) = spec.min_isolation_score {
        kavach::Backend::resolve_min_strength(&policy, min_score).ok_or_else(|| {
            StivaError::Runtime(format!(
                "no backend meets minimum isolation score {min_score}"
            ))
        })?
    } else if kavach::Backend::Oci.is_available() {
        kavach::Backend::Oci
    } else {
        kavach::Backend::Process
    };
    debug!(backend = %backend, rootless = spec.rootless, "selected sandbox backend");

    let mut builder = kavach::SandboxConfig::builder()
        .backend(backend)
        .policy(policy)
        .timeout_ms(spec.timeout_ms);

    // Set agent ID if provided.
    if let Some(ref agent_id) = spec.agent_id {
        builder = builder.agent_id(agent_id);
    }

    // Attach externalization scanning policy if configured.
    if let Some(ref ext_policy) = spec.scan_policy {
        builder = builder.externalization(ext_policy.clone());
    }

    // Set UTS domain name (OCI runtime-spec v1.2.0).
    if let Some(ref dn) = spec.domainname {
        builder = builder.domainname(dn);
    }

    let mut config = builder.build();

    // Inject secrets from the RuntimeSpec into the sandbox config.
    if !spec.secrets.is_empty() {
        info!(
            count = spec.secrets.len(),
            "injecting secrets into sandbox config"
        );
        config.secrets = spec.secrets.clone();
    }

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

    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    // CVE-2024-21626: close inherited host fds before exec to prevent
    // container escape via /proc/self/fd/N pointing to host resources.
    unsafe {
        cmd.pre_exec(|| {
            for fd in 3..1024 {
                libc::close(fd);
            }
            Ok(())
        });
    }

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
// Output scanning (ExternalizationGate)
// ---------------------------------------------------------------------------

/// Scan container output for secrets/PII before returning.
///
/// Uses kavach's `ExternalizationGate` to run secrets, code, and data scanners
/// against the provided output. Returns the scan result containing findings
/// and a verdict.
#[must_use = "scan result should be inspected for findings"]
pub fn scan_output(
    output: &ContainerExecResult,
    policy: &kavach::ExternalizationPolicy,
) -> Result<ContainerExecResult, StivaError> {
    info!("scanning container output via externalization gate");

    let gate = kavach::ExternalizationGate::new();

    // Convert to kavach ExecResult for the gate.
    let kavach_result = kavach::ExecResult {
        exit_code: output.exit_code,
        stdout: output.stdout.clone(),
        stderr: output.stderr.clone(),
        duration_ms: output.duration_ms,
        timed_out: output.timed_out,
    };

    let scanned = gate.apply(kavach_result, policy).map_err(|e| {
        info!(error = %e, "output scanning blocked or quarantined result");
        StivaError::Sandbox(format!("output scan: {e}"))
    })?;

    info!("output scan passed");

    Ok(ContainerExecResult {
        exit_code: scanned.exit_code,
        stdout: scanned.stdout,
        stderr: scanned.stderr,
        duration_ms: scanned.duration_ms,
        timed_out: scanned.timed_out,
    })
}

// ---------------------------------------------------------------------------
// Backend strength scoring
// ---------------------------------------------------------------------------

/// Compute the security strength score for the current sandbox backend and policy.
///
/// Returns a kavach `StrengthScore` (0–100) reflecting the isolation strength
/// of the configured backend with policy modifiers applied.
#[must_use = "security score should be used or displayed"]
pub fn security_score() -> kavach::StrengthScore {
    let backend = if kavach::Backend::Oci.is_available() {
        kavach::Backend::Oci
    } else {
        kavach::Backend::Process
    };
    let policy = kavach::SandboxPolicy::basic();

    info!(
        backend = %backend,
        "computing security strength score"
    );

    kavach::score_backend(backend, &policy)
}

/// Compute the security strength score for a specific backend and policy.
#[must_use = "security score should be used or displayed"]
pub fn security_score_for(
    backend: kavach::Backend,
    policy: &kavach::SandboxPolicy,
) -> kavach::StrengthScore {
    kavach::score_backend(backend, policy)
}

// ---------------------------------------------------------------------------
// Container top — list processes inside a container
// ---------------------------------------------------------------------------

/// A process entry from /proc inside a container's PID namespace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessInfo {
    /// Process ID (as seen from the host).
    pub pid: u32,
    /// Parent PID.
    pub ppid: u32,
    /// Command name.
    pub comm: String,
    /// Full command line.
    pub cmdline: String,
    /// Process state (R=running, S=sleeping, etc.).
    pub state: char,
}

/// List processes inside a container by reading /proc for all children of the container PID.
pub async fn container_top(pid: u32) -> Result<Vec<ProcessInfo>, StivaError> {
    info!(pid, "listing container processes");

    let mut processes = Vec::new();

    // Read /proc to find all processes whose parent chain leads to `pid`.
    let proc_dir = tokio::fs::read_dir("/proc")
        .await
        .map_err(|e| StivaError::Runtime(format!("failed to read /proc: {e}")))?;

    let mut entries = proc_dir;
    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|e| StivaError::Runtime(format!("failed to read /proc entry: {e}")))?
    {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        // Only process numeric directories (PIDs).
        let Ok(child_pid) = name_str.parse::<u32>() else {
            continue;
        };

        // Check if this process is a descendant of our container PID.
        if !is_descendant_of(child_pid, pid) {
            continue;
        }

        if let Ok(info) = read_process_info(child_pid).await {
            processes.push(info);
        }
    }

    info!(pid, count = processes.len(), "container processes listed");
    Ok(processes)
}

/// Check if `child` is a descendant of `ancestor` by walking the PPID chain.
fn is_descendant_of(child: u32, ancestor: u32) -> bool {
    if child == ancestor {
        return true;
    }
    let mut current = child;
    for _ in 0..64 {
        // Safety limit to prevent infinite loops.
        let stat_path = format!("/proc/{current}/stat");
        let Ok(content) = std::fs::read_to_string(&stat_path) else {
            return false;
        };
        // Format: "pid (comm) state ppid ..."
        let ppid = parse_ppid_from_stat(&content);
        if ppid == ancestor {
            return true;
        }
        if ppid <= 1 {
            return false;
        }
        current = ppid;
    }
    false
}

/// Parse PPID from /proc/{pid}/stat content.
/// Format: "pid (comm) state ppid ..."
#[inline]
fn parse_ppid_from_stat(stat: &str) -> u32 {
    // Find the closing ')' of comm field, then parse fields after it.
    if let Some(after_comm) = stat.rfind(')') {
        let remainder = &stat[after_comm + 2..]; // skip ") "
        let mut fields = remainder.split_whitespace();
        let _state = fields.next();
        if let Some(ppid_str) = fields.next() {
            return ppid_str.parse().unwrap_or(0);
        }
    }
    0
}

/// Read process info from /proc/{pid}/.
async fn read_process_info(pid: u32) -> Result<ProcessInfo, StivaError> {
    let stat = tokio::fs::read_to_string(format!("/proc/{pid}/stat"))
        .await
        .map_err(|e| StivaError::Runtime(format!("failed to read /proc/{pid}/stat: {e}")))?;

    let cmdline = tokio::fs::read_to_string(format!("/proc/{pid}/cmdline"))
        .await
        .unwrap_or_default()
        .replace('\0', " ")
        .trim()
        .to_string();

    // Parse stat fields.
    let comm = stat
        .find('(')
        .and_then(|start| stat.rfind(')').map(|end| &stat[start + 1..end]))
        .unwrap_or("?")
        .to_string();

    let ppid = parse_ppid_from_stat(&stat);

    let state = stat
        .rfind(')')
        .and_then(|pos| stat[pos + 2..].chars().next())
        .unwrap_or('?');

    Ok(ProcessInfo {
        pid,
        ppid,
        comm,
        cmdline,
        state,
    })
}

// ---------------------------------------------------------------------------
// Container export / import
// ---------------------------------------------------------------------------

/// Export a container's rootfs as a tar archive.
///
/// Writes the merged overlay directory (or rootfs fallback) to a tar file.
pub async fn export_rootfs(rootfs: &Path, output: &Path) -> Result<(), StivaError> {
    info!(rootfs = %rootfs.display(), output = %output.display(), "exporting rootfs");

    let rootfs = rootfs.to_path_buf();
    let output = output.to_path_buf();

    // Use blocking task for tar creation.
    tokio::task::spawn_blocking(move || {
        let file = std::fs::File::create(&output)
            .map_err(|e| StivaError::Storage(format!("failed to create export file: {e}")))?;
        let mut archive = tar::Builder::new(file);
        archive
            .append_dir_all(".", &rootfs)
            .map_err(|e| StivaError::Storage(format!("failed to archive rootfs: {e}")))?;
        archive
            .finish()
            .map_err(|e| StivaError::Storage(format!("failed to finish export archive: {e}")))?;
        Ok::<(), StivaError>(())
    })
    .await
    .map_err(|e| StivaError::Runtime(format!("export task failed: {e}")))?
}

/// Import a tar archive as a new image layer.
///
/// Creates a single-layer image from the tar file content.
pub fn import_rootfs(
    tar_path: &Path,
    image_store: &crate::image::ImageStore,
    name: &str,
    tag: &str,
) -> Result<crate::image::Image, StivaError> {
    info!(tar = %tar_path.display(), name, tag, "importing rootfs as image");

    // Read and gzip the tar.
    let tar_data = std::fs::read(tar_path)
        .map_err(|e| StivaError::Storage(format!("failed to read import tar: {e}")))?;

    let mut gz_buf = Vec::new();
    {
        let mut encoder = flate2::write::GzEncoder::new(&mut gz_buf, flate2::Compression::fast());
        std::io::Write::write_all(&mut encoder, &tar_data)
            .map_err(|e| StivaError::Storage(format!("failed to compress import data: {e}")))?;
        encoder
            .finish()
            .map_err(|e| StivaError::Storage(format!("failed to finish gzip: {e}")))?;
    }

    // Compute digest and store.
    let layer_digest = {
        use sha2::{Digest, Sha256};
        let hash = Sha256::digest(&gz_buf);
        let mut out = String::with_capacity(71);
        out.push_str("sha256:");
        for byte in hash {
            use std::fmt::Write;
            let _ = write!(out, "{byte:02x}");
        }
        out
    };
    image_store.store_blob(&layer_digest, &gz_buf)?;

    let layer = crate::image::Layer {
        digest: layer_digest.clone(),
        size_bytes: gz_buf.len() as u64,
        media_type: "application/vnd.oci.image.layer.v1.tar+gzip".into(),
    };

    // Build minimal image config.
    let config_json = serde_json::json!({
        "os": "linux",
        "rootfs": {
            "type": "layers",
            "diff_ids": [layer_digest],
        },
    });
    let config_bytes = serde_json::to_vec_pretty(&config_json)?;
    let config_digest = {
        use sha2::{Digest, Sha256};
        let hash = Sha256::digest(&config_bytes);
        let mut out = String::with_capacity(71);
        out.push_str("sha256:");
        for byte in hash {
            use std::fmt::Write;
            let _ = write!(out, "{byte:02x}");
        }
        out
    };
    image_store.store_blob(&config_digest, &config_bytes)?;

    let image = crate::image::Image {
        id: config_digest,
        reference: crate::image::ImageRef {
            registry: "local".into(),
            repository: name.to_string(),
            tag: tag.to_string(),
            digest: None,
        },
        size_bytes: gz_buf.len() as u64,
        layers: vec![layer],
        created_at: chrono::Utc::now(),
    };

    image_store.add_to_index(&image)?;

    info!(
        id = %image.id,
        name,
        tag,
        size = gz_buf.len(),
        "import complete"
    );
    Ok(image)
}

// ---------------------------------------------------------------------------
// Container copy — files in/out
// ---------------------------------------------------------------------------

/// Copy a file from the host into a container's rootfs.
pub fn copy_into_container(
    rootfs: &Path,
    host_src: &Path,
    container_dst: &Path,
) -> Result<(), StivaError> {
    info!(
        src = %host_src.display(),
        dst = %container_dst.display(),
        "copying file into container"
    );

    let target = rootfs.join(container_dst.strip_prefix("/").unwrap_or(container_dst));

    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }

    if host_src.is_dir() {
        copy_dir_recursive(host_src, &target)?;
    } else {
        std::fs::copy(host_src, &target).map_err(|e| {
            StivaError::Storage(format!(
                "failed to copy {} → {}: {e}",
                host_src.display(),
                target.display()
            ))
        })?;
    }
    Ok(())
}

/// Copy a file from a container's rootfs to the host.
pub fn copy_from_container(
    rootfs: &Path,
    container_src: &Path,
    host_dst: &Path,
) -> Result<(), StivaError> {
    info!(
        src = %container_src.display(),
        dst = %host_dst.display(),
        "copying file from container"
    );

    let source = rootfs.join(container_src.strip_prefix("/").unwrap_or(container_src));

    if !source.exists() {
        return Err(StivaError::Storage(format!(
            "source path does not exist in container: {}",
            container_src.display()
        )));
    }

    if let Some(parent) = host_dst.parent() {
        std::fs::create_dir_all(parent)?;
    }

    if source.is_dir() {
        copy_dir_recursive(&source, host_dst)?;
    } else {
        std::fs::copy(&source, host_dst).map_err(|e| {
            StivaError::Storage(format!(
                "failed to copy {} → {}: {e}",
                source.display(),
                host_dst.display()
            ))
        })?;
    }
    Ok(())
}

/// Recursively copy a directory.
fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<(), StivaError> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let dest_path = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_recursive(&entry.path(), &dest_path)?;
        } else {
            std::fs::copy(entry.path(), &dest_path)?;
        }
    }
    Ok(())
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

    // CPU limit via cpu.max (format: "$MAX $PERIOD" where period is 100ms).
    // cpu_shares maps to fractional cores: 1024 shares = 1 core.
    if let Some(shares) = spec.cpu_shares
        && shares > 0
    {
        let period: u64 = 100_000; // 100ms in microseconds
        let quota = shares * period / 1024;
        let path = format!("{base}/cpu.max");
        let val = format!("{quota} {period}");
        if let Err(e) = tokio::fs::write(&path, &val).await {
            tracing::warn!(pid, path = %path, error = %e, "failed to write cpu.max");
        } else {
            info!(
                pid,
                cpu_shares = shares,
                quota,
                period,
                "cgroup cpu.max applied"
            );
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
            secrets: vec![],
            scan_policy: None,
            timeout_ms: 0,
            backend: None,
            min_isolation_score: None,
            agent_id: None,
            domainname: None,
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
            secrets: vec![],
            scan_policy: None,
            timeout_ms: 0,
            backend: None,
            min_isolation_score: None,
            agent_id: None,
            domainname: None,
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
            secrets: vec![],
            scan_policy: None,
            timeout_ms: 0,
            backend: None,
            min_isolation_score: None,
            agent_id: None,
            domainname: None,
        };
        // Should skip because values are 0.
        apply_cgroup_limits(1, &spec).await;
    }

    #[test]
    fn parse_ppid_from_stat_basic() {
        // Real /proc/pid/stat format
        let stat = "1234 (bash) S 1 1234 1234 0 -1 4194560";
        assert_eq!(parse_ppid_from_stat(stat), 1);
    }

    #[test]
    fn parse_ppid_from_stat_comm_with_parens() {
        // comm can contain parens: "1234 (my (app)) S 42 ..."
        let stat = "1234 (my (app)) S 42 1234 1234 0";
        assert_eq!(parse_ppid_from_stat(stat), 42);
    }

    #[test]
    fn parse_ppid_from_stat_empty() {
        assert_eq!(parse_ppid_from_stat(""), 0);
    }

    #[test]
    fn process_info_serde() {
        let info = ProcessInfo {
            pid: 123,
            ppid: 1,
            comm: "sleep".into(),
            cmdline: "sleep 60".into(),
            state: 'S',
        };
        let json = serde_json::to_string(&info).unwrap();
        let back: ProcessInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(back.pid, 123);
        assert_eq!(back.state, 'S');
    }

    #[test]
    fn copy_into_container_creates_file() {
        let dir = tempfile::tempdir().unwrap();
        let rootfs = dir.path().join("rootfs");
        std::fs::create_dir_all(&rootfs).unwrap();

        let src = dir.path().join("hello.txt");
        std::fs::write(&src, "hello").unwrap();

        copy_into_container(&rootfs, &src, Path::new("/app/hello.txt")).unwrap();
        assert_eq!(
            std::fs::read_to_string(rootfs.join("app/hello.txt")).unwrap(),
            "hello"
        );
    }

    #[test]
    fn copy_from_container_reads_file() {
        let dir = tempfile::tempdir().unwrap();
        let rootfs = dir.path().join("rootfs");
        std::fs::create_dir_all(rootfs.join("data")).unwrap();
        std::fs::write(rootfs.join("data/out.txt"), "result").unwrap();

        let dst = dir.path().join("output.txt");
        copy_from_container(&rootfs, Path::new("/data/out.txt"), &dst).unwrap();
        assert_eq!(std::fs::read_to_string(&dst).unwrap(), "result");
    }

    #[test]
    fn copy_from_container_missing_source() {
        let dir = tempfile::tempdir().unwrap();
        let rootfs = dir.path().join("rootfs");
        std::fs::create_dir_all(&rootfs).unwrap();

        let err = copy_from_container(&rootfs, Path::new("/nope"), &dir.path().join("out"));
        assert!(err.is_err());
    }

    #[test]
    fn copy_dir_recursive_works() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir_all(src.join("sub")).unwrap();
        std::fs::write(src.join("a.txt"), "a").unwrap();
        std::fs::write(src.join("sub/b.txt"), "b").unwrap();

        let dst = dir.path().join("dst");
        copy_dir_recursive(&src, &dst).unwrap();
        assert_eq!(std::fs::read_to_string(dst.join("a.txt")).unwrap(), "a");
        assert_eq!(std::fs::read_to_string(dst.join("sub/b.txt")).unwrap(), "b");
    }

    #[test]
    fn import_rootfs_creates_image() {
        let dir = tempfile::tempdir().unwrap();
        let store = crate::image::ImageStore::new(&dir.path().join("store")).unwrap();

        // Create a tar file.
        let tar_path = dir.path().join("rootfs.tar");
        {
            let file = std::fs::File::create(&tar_path).unwrap();
            let mut builder = tar::Builder::new(file);
            let data = b"hello from import";
            let mut header = tar::Header::new_gnu();
            header.set_path("hello.txt").unwrap();
            header.set_size(data.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder.append(&header, &data[..]).unwrap();
            builder.finish().unwrap();
        }

        let image = import_rootfs(&tar_path, &store, "imported", "v1").unwrap();
        assert_eq!(image.reference.repository, "imported");
        assert_eq!(image.reference.tag, "v1");
        assert_eq!(image.layers.len(), 1);

        // Should appear in index.
        let listed = store.list().unwrap();
        assert_eq!(listed.len(), 1);
    }

    #[tokio::test]
    async fn export_rootfs_creates_tar() {
        let dir = tempfile::tempdir().unwrap();
        let rootfs = dir.path().join("rootfs");
        std::fs::create_dir_all(&rootfs).unwrap();
        std::fs::write(rootfs.join("test.txt"), "export test").unwrap();

        let output = dir.path().join("export.tar");
        export_rootfs(&rootfs, &output).await.unwrap();
        assert!(output.exists());
        assert!(std::fs::metadata(&output).unwrap().len() > 0);
    }

    // -----------------------------------------------------------------
    // Credential injection tests
    // -----------------------------------------------------------------

    #[test]
    fn generate_spec_includes_secrets() {
        use crate::container::{Container, ContainerConfig, ContainerState};

        let config = ContainerConfig {
            secrets: vec![kavach::SecretRef {
                name: "API_KEY".into(),
                inject_via: kavach::credential::InjectionMethod::EnvVar {
                    var_name: "MY_API_KEY".into(),
                },
            }],
            ..Default::default()
        };
        let container = Container {
            id: "test-secrets".into(),
            name: Some("test".into()),
            image_id: "img".into(),
            image_ref: "alpine:latest".into(),
            state: ContainerState::Created,
            pid: None,
            created_at: chrono::Utc::now(),
            started_at: None,
            config,
            exit_code: None,
        };
        let spec = generate_spec(&container, Path::new("/tmp/rootfs")).unwrap();
        assert_eq!(spec.secrets.len(), 1);
        assert_eq!(spec.secrets[0].name, "API_KEY");
    }

    #[test]
    fn generate_spec_default_no_secrets() {
        use crate::container::{Container, ContainerConfig, ContainerState};

        let container = Container {
            id: "test-no-secrets".into(),
            name: Some("test".into()),
            image_id: "img".into(),
            image_ref: "alpine:latest".into(),
            state: ContainerState::Created,
            pid: None,
            created_at: chrono::Utc::now(),
            started_at: None,
            config: ContainerConfig::default(),
            exit_code: None,
        };
        let spec = generate_spec(&container, Path::new("/tmp/rootfs")).unwrap();
        assert!(spec.secrets.is_empty());
        assert!(spec.scan_policy.is_none());
    }

    // -----------------------------------------------------------------
    // Security strength scoring tests
    // -----------------------------------------------------------------

    #[test]
    fn security_score_returns_valid_range() {
        let score = security_score();
        assert!(score.value() <= 100);
        assert!(!score.label().is_empty());
    }

    #[test]
    fn security_score_for_noop_is_minimal() {
        let score = security_score_for(kavach::Backend::Noop, &kavach::SandboxPolicy::minimal());
        assert!(score.value() <= 10, "noop minimal should be low");
    }

    #[test]
    fn security_score_for_firecracker_is_high() {
        let score = security_score_for(
            kavach::Backend::Firecracker,
            &kavach::SandboxPolicy::strict(),
        );
        assert!(
            score.value() >= 90,
            "firecracker strict should be >= 90, got {}",
            score.value()
        );
    }

    #[test]
    fn security_score_display_format() {
        let score = security_score_for(kavach::Backend::Process, &kavach::SandboxPolicy::basic());
        let display = score.to_string();
        assert!(display.contains('('));
        assert!(display.contains(')'));
    }

    // -----------------------------------------------------------------
    // Output scanning tests
    // -----------------------------------------------------------------

    #[test]
    fn scan_output_clean_passes() {
        let result = ContainerExecResult {
            exit_code: 0,
            stdout: "hello world".into(),
            stderr: String::new(),
            duration_ms: 10,
            timed_out: false,
        };
        let policy = kavach::ExternalizationPolicy::default();
        let scanned = scan_output(&result, &policy).unwrap();
        assert_eq!(scanned.stdout, "hello world");
    }

    #[test]
    fn scan_output_blocks_private_key() {
        let result = ContainerExecResult {
            exit_code: 0,
            stdout: "-----BEGIN RSA PRIVATE KEY-----\nMIIEp...".into(),
            stderr: String::new(),
            duration_ms: 10,
            timed_out: false,
        };
        let policy = kavach::ExternalizationPolicy::default();
        let scanned = scan_output(&result, &policy);
        assert!(scanned.is_err(), "private key should be blocked");
    }

    #[test]
    fn scan_output_disabled_passes_everything() {
        let result = ContainerExecResult {
            exit_code: 0,
            stdout: "-----BEGIN RSA PRIVATE KEY-----".into(),
            stderr: String::new(),
            duration_ms: 10,
            timed_out: false,
        };
        let policy = kavach::ExternalizationPolicy {
            enabled: false,
            ..Default::default()
        };
        let scanned = scan_output(&result, &policy).unwrap();
        assert!(scanned.stdout.contains("BEGIN RSA PRIVATE KEY"));
    }

    #[test]
    fn scan_output_blocks_oversized() {
        let result = ContainerExecResult {
            exit_code: 0,
            stdout: "x".repeat(100),
            stderr: String::new(),
            duration_ms: 10,
            timed_out: false,
        };
        let policy = kavach::ExternalizationPolicy {
            max_artifact_size_bytes: 10,
            ..Default::default()
        };
        let scanned = scan_output(&result, &policy);
        assert!(scanned.is_err(), "oversized output should be blocked");
    }

    #[test]
    fn generate_spec_with_scan_policy() {
        use crate::container::{Container, ContainerConfig, ContainerState};

        let config = ContainerConfig {
            scan_policy: Some(kavach::ExternalizationPolicy::default()),
            ..Default::default()
        };
        let container = Container {
            id: "test-scan".into(),
            name: Some("test".into()),
            image_id: "img".into(),
            image_ref: "alpine:latest".into(),
            state: ContainerState::Created,
            pid: None,
            created_at: chrono::Utc::now(),
            started_at: None,
            config,
            exit_code: None,
        };
        let spec = generate_spec(&container, Path::new("/tmp/rootfs")).unwrap();
        assert!(spec.scan_policy.is_some());
        assert!(spec.scan_policy.as_ref().unwrap().enabled);
    }
}
