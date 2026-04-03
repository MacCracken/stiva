//! Rootless networking — userspace network stack for unprivileged containers.
//!
//! Uses slirp4netns or pasta to provide networking without CAP_NET_ADMIN.
//! Automatically detects available backends and selects the best option.

use crate::error::StivaError;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tracing::info;

/// Rootless network backend selection.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum RootlessNetworkBackend {
    /// Auto-detect best available backend.
    #[default]
    Auto,
    /// Use slirp4netns (SLIRP-based userspace TCP/IP).
    Slirp4netns,
    /// Use pasta (passt) — newer, higher performance.
    Pasta,
}

/// Port mapping for rootless containers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortMapping {
    pub host_port: u16,
    pub container_port: u16,
    pub protocol: String,
}

/// Handle to a running rootless network process.
pub struct RootlessNetworkHandle {
    child: tokio::process::Child,
    backend: RootlessNetworkBackend,
    api_socket: Option<PathBuf>,
}

impl RootlessNetworkHandle {
    /// The backend in use.
    #[must_use]
    #[inline]
    pub fn backend(&self) -> &RootlessNetworkBackend {
        &self.backend
    }
}

/// Check if we're running unprivileged (not root and no CAP_NET_ADMIN).
#[must_use]
#[inline]
pub fn is_unprivileged() -> bool {
    #[cfg(target_os = "linux")]
    {
        let uid = rustix::process::getuid();
        if uid.is_root() {
            return false;
        }
        // Check CAP_NET_ADMIN (bit 12) in effective capabilities.
        match std::fs::read_to_string("/proc/self/status") {
            Ok(status) => {
                for line in status.lines() {
                    if let Some(hex) = line.strip_prefix("CapEff:\t")
                        && let Ok(caps) = u64::from_str_radix(hex.trim(), 16)
                    {
                        return caps & (1 << 12) == 0; // No CAP_NET_ADMIN
                    }
                }
                true // Can't determine — assume unprivileged
            }
            Err(_) => true,
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        true
    }
}

/// Detect which rootless network backends are available on the system.
#[must_use]
pub fn available_backends() -> Vec<RootlessNetworkBackend> {
    let mut backends = Vec::new();
    if which("pasta") {
        backends.push(RootlessNetworkBackend::Pasta);
    }
    if which("slirp4netns") {
        backends.push(RootlessNetworkBackend::Slirp4netns);
    }
    backends
}

/// Select the best available backend, respecting user preference.
pub fn select_backend(
    preference: &RootlessNetworkBackend,
) -> Result<RootlessNetworkBackend, StivaError> {
    let available = available_backends();

    match preference {
        RootlessNetworkBackend::Auto => {
            // Prefer pasta (newer, faster), then slirp4netns.
            if available.contains(&RootlessNetworkBackend::Pasta) {
                Ok(RootlessNetworkBackend::Pasta)
            } else if available.contains(&RootlessNetworkBackend::Slirp4netns) {
                Ok(RootlessNetworkBackend::Slirp4netns)
            } else {
                Err(StivaError::RootlessNetwork(
                    "no rootless network backend found (install pasta or slirp4netns)".into(),
                ))
            }
        }
        specific => {
            if available.contains(specific) {
                Ok(specific.clone())
            } else {
                Err(StivaError::RootlessNetwork(format!(
                    "{specific:?} not found in PATH"
                )))
            }
        }
    }
}

/// Parse port specs (e.g., "8080:80/tcp") into PortMapping structs.
pub fn parse_port_mappings(specs: &[String]) -> Result<Vec<PortMapping>, StivaError> {
    let mut mappings = Vec::new();
    for spec in specs {
        // Format: [host_port:]container_port[/protocol]
        let (port_part, protocol) = match spec.rsplit_once('/') {
            Some((p, proto)) => (p, proto.to_string()),
            None => (spec.as_str(), "tcp".to_string()),
        };

        let (host_port, container_port) = match port_part.split_once(':') {
            Some((h, c)) => {
                let hp = h
                    .parse::<u16>()
                    .map_err(|_| StivaError::RootlessNetwork(format!("invalid host port: {h}")))?;
                let cp = c.parse::<u16>().map_err(|_| {
                    StivaError::RootlessNetwork(format!("invalid container port: {c}"))
                })?;
                (hp, cp)
            }
            None => {
                let p = port_part.parse::<u16>().map_err(|_| {
                    StivaError::RootlessNetwork(format!("invalid port: {port_part}"))
                })?;
                (p, p)
            }
        };

        mappings.push(PortMapping {
            host_port,
            container_port,
            protocol,
        });
    }
    Ok(mappings)
}

/// Start a rootless network for a container.
///
/// Spawns slirp4netns or pasta to provide networking in a user namespace.
pub async fn start_rootless_network(
    backend: &RootlessNetworkBackend,
    container_pid: u32,
    port_mappings: &[PortMapping],
) -> Result<RootlessNetworkHandle, StivaError> {
    let resolved = match backend {
        RootlessNetworkBackend::Auto => select_backend(backend)?,
        other => other.clone(),
    };
    match resolved {
        RootlessNetworkBackend::Slirp4netns => {
            start_slirp4netns(container_pid, port_mappings).await
        }
        RootlessNetworkBackend::Pasta => start_pasta(container_pid, port_mappings).await,
        RootlessNetworkBackend::Auto => unreachable!(),
    }
}

/// Stop the rootless network process.
pub async fn stop_rootless_network(handle: &mut RootlessNetworkHandle) -> Result<(), StivaError> {
    info!(backend = ?handle.backend, "stopping rootless network");
    let _ = handle.child.kill().await;
    let _ = handle.child.wait().await;

    // Clean up API socket if present.
    if let Some(ref sock) = handle.api_socket {
        let _ = std::fs::remove_file(sock);
    }
    Ok(())
}

async fn start_slirp4netns(
    pid: u32,
    port_mappings: &[PortMapping],
) -> Result<RootlessNetworkHandle, StivaError> {
    let sock_dir = std::env::temp_dir();
    let sock_path = sock_dir.join(format!("stiva-slirp-{pid}.sock"));

    info!(pid, socket = %sock_path.display(), "starting slirp4netns");

    let child = tokio::process::Command::new("slirp4netns")
        .args([
            "--configure",
            "--mtu=65520",
            "--disable-host-loopback",
            "--api-socket",
        ])
        .arg(&sock_path)
        .arg(pid.to_string())
        .arg("tap0")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| StivaError::RootlessNetwork(format!("failed to start slirp4netns: {e}")))?;

    let mut handle = RootlessNetworkHandle {
        child,
        backend: RootlessNetworkBackend::Slirp4netns,
        api_socket: Some(sock_path.clone()),
    };

    // Wait briefly for slirp4netns to initialize.
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    // Add port forwarding via API socket.
    if !port_mappings.is_empty()
        && let Err(e) = add_slirp_port_forwards(&sock_path, port_mappings).await
    {
        let _ = stop_rootless_network(&mut handle).await;
        return Err(e);
    }

    Ok(handle)
}

async fn add_slirp_port_forwards(
    sock_path: &Path,
    port_mappings: &[PortMapping],
) -> Result<(), StivaError> {
    for mapping in port_mappings {
        let proto = if mapping.protocol == "udp" {
            "udp"
        } else {
            "tcp"
        };
        let cmd = serde_json::json!({
            "execute": "add_hostfwd",
            "arguments": {
                "proto": proto,
                "host_addr": "",
                "host_port": mapping.host_port,
                "guest_port": mapping.container_port
            }
        });

        // Connect to the UNIX socket and send the command.
        use tokio::io::AsyncWriteExt;
        let mut stream = tokio::net::UnixStream::connect(sock_path)
            .await
            .map_err(|e| {
                StivaError::RootlessNetwork(format!("failed to connect to slirp4netns API: {e}"))
            })?;
        let msg = serde_json::to_vec(&cmd).map_err(|e| {
            StivaError::RootlessNetwork(format!("failed to serialize port forward command: {e}"))
        })?;
        stream.write_all(&msg).await.map_err(|e| {
            StivaError::RootlessNetwork(format!("failed to send port forward command: {e}"))
        })?;

        info!(
            proto,
            host_port = mapping.host_port,
            guest_port = mapping.container_port,
            "slirp4netns port forward added"
        );
    }
    Ok(())
}

async fn start_pasta(
    pid: u32,
    port_mappings: &[PortMapping],
) -> Result<RootlessNetworkHandle, StivaError> {
    info!(pid, "starting pasta");

    let mut cmd = tokio::process::Command::new("pasta");
    cmd.arg("--ns-ifname").arg("eth0");

    // Add port forwarding flags.
    for mapping in port_mappings {
        let flag = if mapping.protocol == "udp" {
            "-u"
        } else {
            "-t"
        };
        cmd.arg(flag)
            .arg(format!("{}:{}", mapping.host_port, mapping.container_port));
    }

    cmd.arg(pid.to_string());

    let child = cmd
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| StivaError::RootlessNetwork(format!("failed to start pasta: {e}")))?;

    Ok(RootlessNetworkHandle {
        child,
        backend: RootlessNetworkBackend::Pasta,
        api_socket: None,
    })
}

/// Check if a binary is available in PATH.
fn which(name: &str) -> bool {
    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).any(|dir| dir.join(name).is_file()))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_unprivileged_returns_bool() {
        // Just verify it doesn't panic.
        let _ = is_unprivileged();
    }

    #[test]
    fn available_backends_returns_vec() {
        let backends = available_backends();
        // Results depend on system, but should not panic.
        assert!(backends.len() <= 2);
    }

    #[test]
    fn rootless_backend_serde() {
        let auto = RootlessNetworkBackend::Auto;
        let json = serde_json::to_string(&auto).unwrap();
        let back: RootlessNetworkBackend = serde_json::from_str(&json).unwrap();
        assert_eq!(back, RootlessNetworkBackend::Auto);

        let pasta = RootlessNetworkBackend::Pasta;
        let json = serde_json::to_string(&pasta).unwrap();
        let back: RootlessNetworkBackend = serde_json::from_str(&json).unwrap();
        assert_eq!(back, RootlessNetworkBackend::Pasta);
    }

    #[test]
    fn rootless_backend_default() {
        assert_eq!(
            RootlessNetworkBackend::default(),
            RootlessNetworkBackend::Auto
        );
    }

    #[test]
    fn parse_port_mappings_simple() {
        let specs = vec!["8080:80".to_string()];
        let mappings = parse_port_mappings(&specs).unwrap();
        assert_eq!(mappings.len(), 1);
        assert_eq!(mappings[0].host_port, 8080);
        assert_eq!(mappings[0].container_port, 80);
        assert_eq!(mappings[0].protocol, "tcp");
    }

    #[test]
    fn parse_port_mappings_with_protocol() {
        let specs = vec!["5353:53/udp".to_string()];
        let mappings = parse_port_mappings(&specs).unwrap();
        assert_eq!(mappings[0].host_port, 5353);
        assert_eq!(mappings[0].container_port, 53);
        assert_eq!(mappings[0].protocol, "udp");
    }

    #[test]
    fn parse_port_mappings_single_port() {
        let specs = vec!["80".to_string()];
        let mappings = parse_port_mappings(&specs).unwrap();
        assert_eq!(mappings[0].host_port, 80);
        assert_eq!(mappings[0].container_port, 80);
    }

    #[test]
    fn parse_port_mappings_invalid() {
        let specs = vec!["not-a-port".to_string()];
        assert!(parse_port_mappings(&specs).is_err());
    }

    #[test]
    fn parse_port_mappings_multiple() {
        let specs = vec![
            "8080:80/tcp".to_string(),
            "5353:53/udp".to_string(),
            "443".to_string(),
        ];
        let mappings = parse_port_mappings(&specs).unwrap();
        assert_eq!(mappings.len(), 3);
    }
}
