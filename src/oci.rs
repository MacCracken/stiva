//! OCI runtime CLI conformance — `create`/`start`/`state`/`kill`/`delete`.
//!
//! Bridges the OCI runtime specification's CLI interface to stiva's
//! [`ContainerManager`](crate::container::ContainerManager). Enables use
//! as a containerd/CRI-compatible runtime.

use crate::container::{ContainerConfig, ContainerState};
use crate::error::StivaError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use tracing::info;

/// OCI runtime spec version we conform to.
pub const OCI_VERSION: &str = "1.2.0";

/// OCI runtime state — output of the `state` command.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OciState {
    #[serde(rename = "ociVersion")]
    pub oci_version: String,
    pub id: String,
    pub status: OciStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    pub bundle: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<HashMap<String, String>>,
}

/// OCI container status (spec-defined values).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum OciStatus {
    #[serde(rename = "creating")]
    Creating,
    #[serde(rename = "created")]
    Created,
    #[serde(rename = "running")]
    Running,
    #[serde(rename = "stopped")]
    Stopped,
}

/// Map internal container state to OCI spec status.
#[must_use]
#[inline]
pub fn to_oci_status(state: ContainerState) -> OciStatus {
    match state {
        ContainerState::Created => OciStatus::Created,
        ContainerState::Running | ContainerState::Paused => OciStatus::Running,
        ContainerState::Stopped | ContainerState::Removing => OciStatus::Stopped,
    }
}

/// Build the OCI state JSON for a container.
#[must_use]
pub fn build_state(container: &crate::container::Container, bundle_path: &str) -> OciState {
    OciState {
        oci_version: OCI_VERSION.to_string(),
        id: container.id.clone(),
        status: to_oci_status(container.state),
        pid: container.pid,
        bundle: bundle_path.to_string(),
        annotations: None,
    }
}

/// Parse an OCI bundle's `config.json` into a [`ContainerConfig`].
///
/// Reads the OCI runtime specification JSON from `{bundle_path}/config.json`
/// and extracts the fields stiva needs.
pub fn parse_bundle(bundle_path: &Path) -> Result<ContainerConfig, StivaError> {
    let config_path = bundle_path.join("config.json");
    info!(config = %config_path.display(), "parsing OCI bundle");

    let content = std::fs::read_to_string(&config_path).map_err(|e| {
        StivaError::OciBundle(format!("failed to read {}: {e}", config_path.display()))
    })?;

    let spec: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| StivaError::OciBundle(format!("invalid config.json: {e}")))?;

    let mut config = ContainerConfig::default();

    // Process args.
    if let Some(args) = spec
        .get("process")
        .and_then(|p| p.get("args"))
        .and_then(|a| a.as_array())
    {
        config.command = args
            .iter()
            .filter_map(|v| v.as_str())
            .map(String::from)
            .collect();
    }

    // Process env.
    if let Some(env) = spec
        .get("process")
        .and_then(|p| p.get("env"))
        .and_then(|e| e.as_array())
    {
        for item in env {
            if let Some(s) = item.as_str()
                && let Some((k, v)) = s.split_once('=')
            {
                config.env.insert(k.to_string(), v.to_string());
            }
        }
    }

    // Process user.
    if let Some(uid) = spec
        .get("process")
        .and_then(|p| p.get("user"))
        .and_then(|u| u.get("uid"))
        .and_then(|u| u.as_u64())
        && uid != 0
    {
        config.rootless = true;
    }

    // Domainname (OCI spec uses "hostname", mapped to stiva's domainname).
    if let Some(domainname) = spec.get("hostname").and_then(|h| h.as_str()) {
        config.domainname = Some(domainname.to_string());
    }

    // Resource limits.
    if let Some(memory) = spec
        .get("linux")
        .and_then(|l| l.get("resources"))
        .and_then(|r| r.get("memory"))
        .and_then(|m| m.get("limit"))
        .and_then(|l| l.as_u64())
    {
        config.memory_limit = memory;
    }

    if let Some(pids) = spec
        .get("linux")
        .and_then(|l| l.get("resources"))
        .and_then(|r| r.get("pids"))
        .and_then(|p| p.get("limit"))
        .and_then(|l| l.as_u64())
    {
        config.max_pids = u32::try_from(pids).unwrap_or(u32::MAX);
    }

    // Daemon mode for OCI runtime — containers are always daemon-style.
    config.detach = true;

    Ok(config)
}

/// Parse a signal string — accepts both names ("SIGTERM", "TERM") and numbers ("15").
#[must_use = "returns the signal number"]
pub fn parse_signal(s: &str) -> Result<i32, StivaError> {
    // Try numeric first.
    if let Ok(n) = s.parse::<i32>() {
        if n > 0 && n < 65 {
            return Ok(n);
        }
        return Err(StivaError::OciBundle(format!("invalid signal number: {n}")));
    }

    // Strip optional "SIG" prefix.
    let name = s.strip_prefix("SIG").unwrap_or(s);
    match name {
        "HUP" => Ok(1),
        "INT" => Ok(2),
        "QUIT" => Ok(3),
        "ILL" => Ok(4),
        "TRAP" => Ok(5),
        "ABRT" | "ABORT" => Ok(6),
        "BUS" => Ok(7),
        "FPE" => Ok(8),
        "KILL" => Ok(9),
        "USR1" => Ok(10),
        "SEGV" => Ok(11),
        "USR2" => Ok(12),
        "PIPE" => Ok(13),
        "ALRM" | "ALARM" => Ok(14),
        "TERM" => Ok(15),
        "STKFLT" => Ok(16),
        "CHLD" | "CHILD" => Ok(17),
        "CONT" => Ok(18),
        "STOP" => Ok(19),
        "TSTP" => Ok(20),
        "TTIN" => Ok(21),
        "TTOU" => Ok(22),
        _ => Err(StivaError::OciBundle(format!("unknown signal: {s}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oci_state_serde() {
        let state = OciState {
            oci_version: OCI_VERSION.to_string(),
            id: "test-container".into(),
            status: OciStatus::Running,
            pid: Some(1234),
            bundle: "/run/containers/test".into(),
            annotations: None,
        };
        let json = serde_json::to_string(&state).unwrap();
        assert!(json.contains("\"ociVersion\":\"1.2.0\""));
        assert!(json.contains("\"status\":\"running\""));
        let back: OciState = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, "test-container");
        assert_eq!(back.status, OciStatus::Running);
    }

    #[test]
    fn oci_status_all_variants() {
        assert_eq!(to_oci_status(ContainerState::Created), OciStatus::Created);
        assert_eq!(to_oci_status(ContainerState::Running), OciStatus::Running);
        assert_eq!(to_oci_status(ContainerState::Paused), OciStatus::Running);
        assert_eq!(to_oci_status(ContainerState::Stopped), OciStatus::Stopped);
        assert_eq!(to_oci_status(ContainerState::Removing), OciStatus::Stopped);
    }

    #[test]
    fn parse_signal_numeric() {
        assert_eq!(parse_signal("15").unwrap(), 15);
        assert_eq!(parse_signal("9").unwrap(), 9);
        assert_eq!(parse_signal("1").unwrap(), 1);
    }

    #[test]
    fn parse_signal_names() {
        assert_eq!(parse_signal("SIGTERM").unwrap(), 15);
        assert_eq!(parse_signal("SIGKILL").unwrap(), 9);
        assert_eq!(parse_signal("TERM").unwrap(), 15);
        assert_eq!(parse_signal("HUP").unwrap(), 1);
        assert_eq!(parse_signal("SIGUSR1").unwrap(), 10);
    }

    #[test]
    fn parse_signal_invalid() {
        assert!(parse_signal("SIGFOO").is_err());
        assert!(parse_signal("0").is_err());
        assert!(parse_signal("999").is_err());
    }

    #[test]
    fn parse_bundle_minimal() {
        let dir = tempfile::tempdir().unwrap();
        let config = serde_json::json!({
            "ociVersion": "1.2.0",
            "process": {
                "args": ["/bin/sh", "-c", "echo hello"],
                "env": ["PATH=/usr/bin", "HOME=/root"]
            },
            "root": { "path": "rootfs" }
        });
        std::fs::write(
            dir.path().join("config.json"),
            serde_json::to_string(&config).unwrap(),
        )
        .unwrap();

        let cfg = parse_bundle(dir.path()).unwrap();
        assert_eq!(cfg.command, vec!["/bin/sh", "-c", "echo hello"]);
        assert_eq!(cfg.env.get("PATH"), Some(&"/usr/bin".to_string()));
        assert!(cfg.detach);
    }

    #[test]
    fn parse_bundle_with_resources() {
        let dir = tempfile::tempdir().unwrap();
        let config = serde_json::json!({
            "ociVersion": "1.2.0",
            "process": {
                "args": ["/bin/true"]
            },
            "root": { "path": "rootfs" },
            "hostname": "test-host",
            "linux": {
                "resources": {
                    "memory": { "limit": 268435456 },
                    "pids": { "limit": 100 }
                }
            }
        });
        std::fs::write(
            dir.path().join("config.json"),
            serde_json::to_string(&config).unwrap(),
        )
        .unwrap();

        let cfg = parse_bundle(dir.path()).unwrap();
        assert_eq!(cfg.memory_limit, 268_435_456);
        assert_eq!(cfg.max_pids, 100);
        assert_eq!(cfg.domainname.as_deref(), Some("test-host"));
    }

    #[test]
    fn parse_bundle_missing_config() {
        let dir = tempfile::tempdir().unwrap();
        assert!(parse_bundle(dir.path()).is_err());
    }

    #[test]
    fn build_state_json() {
        let container = crate::container::Container {
            id: "abc123".into(),
            name: Some("test".into()),
            image_ref: "".into(),
            image_id: "sha256:dead".into(),
            state: ContainerState::Running,
            pid: Some(42),
            created_at: chrono::Utc::now(),
            started_at: Some(chrono::Utc::now()),
            config: ContainerConfig::default(),
            exit_code: None,
        };
        let state = build_state(&container, "/run/bundle");
        assert_eq!(state.oci_version, "1.2.0");
        assert_eq!(state.status, OciStatus::Running);
        assert_eq!(state.pid, Some(42));
        assert_eq!(state.bundle, "/run/bundle");
    }
}
