//! Bridge and veth pair management via `ip` commands.
//!
//! All operations shell out to `ip` (iproute2). This avoids a large netlink
//! dependency and matches the pattern used by agnos-sys.

use crate::error::StivaError;
use std::net::Ipv4Addr;
use std::process::Command;
use tracing::info;

/// Run an `ip` command, returning an error on failure.
fn ip(args: &[&str]) -> Result<(), StivaError> {
    let output = Command::new("ip")
        .args(args)
        .output()
        .map_err(|e| StivaError::Network(format!("failed to run `ip {}`: {e}", args.join(" "))))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(StivaError::Network(format!(
            "`ip {}` failed: {}",
            args.join(" "),
            stderr.trim()
        )));
    }
    Ok(())
}

/// Run a sysctl command.
fn sysctl(key: &str, value: &str) -> Result<(), StivaError> {
    let output = Command::new("sysctl")
        .args(["-w", &format!("{key}={value}")])
        .output()
        .map_err(|e| StivaError::Network(format!("sysctl failed: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(StivaError::Network(format!(
            "sysctl {key}={value} failed: {}",
            stderr.trim()
        )));
    }
    Ok(())
}

/// Create a Linux bridge interface with an IP address.
pub fn create_bridge(name: &str, gateway: Ipv4Addr, prefix_len: u8) -> Result<(), StivaError> {
    info!(bridge = name, gateway = %gateway, "creating bridge");

    ip(&["link", "add", name, "type", "bridge"])?;
    ip(&[
        "addr",
        "add",
        &format!("{gateway}/{prefix_len}"),
        "dev",
        name,
    ])?;
    ip(&["link", "set", name, "up"])?;

    // Enable IP forwarding for NAT.
    enable_ip_forward()?;

    Ok(())
}

/// Delete a bridge interface.
pub fn delete_bridge(name: &str) -> Result<(), StivaError> {
    info!(bridge = name, "deleting bridge");
    ip(&["link", "set", name, "down"])?;
    ip(&["link", "delete", name, "type", "bridge"])
}

/// Create a veth pair for a container.
///
/// Returns `(host_veth, container_veth)` names.
pub fn create_veth_pair(container_id: &str) -> Result<(String, String), StivaError> {
    // Linux IFNAMSIZ = 16 (including null), so max 15 chars.
    // "ve-" prefix + 12 hex chars = 15. Use first 12 of container ID.
    let short_id = &container_id[..12.min(container_id.len())];
    let host_veth = format!("ve-{short_id}");
    let container_veth = "eth0".to_string();

    info!(host = %host_veth, container = %container_veth, "creating veth pair");

    ip(&[
        "link",
        "add",
        &host_veth,
        "type",
        "veth",
        "peer",
        "name",
        &container_veth,
    ])?;

    Ok((host_veth, container_veth))
}

/// Attach a veth interface to a bridge.
pub fn attach_to_bridge(veth: &str, bridge: &str) -> Result<(), StivaError> {
    ip(&["link", "set", veth, "master", bridge])?;
    ip(&["link", "set", veth, "up"])
}

/// Move a veth interface into a network namespace (by PID).
pub fn move_to_netns(veth: &str, pid: u32) -> Result<(), StivaError> {
    ip(&["link", "set", veth, "netns", &pid.to_string()])
}

/// Configure an interface inside a network namespace.
///
/// Uses `nsenter` to run `ip` commands inside the namespace.
pub fn configure_container_iface(
    pid: u32,
    iface: &str,
    container_ip: Ipv4Addr,
    prefix_len: u8,
    gateway: Ipv4Addr,
) -> Result<(), StivaError> {
    let pid_str = pid.to_string();
    let nsenter = |args: &[&str]| -> Result<(), StivaError> {
        let mut cmd_args = vec!["-t", pid_str.as_str(), "-n", "--", "ip"];
        cmd_args.extend_from_slice(args);

        let output = Command::new("nsenter")
            .args(&cmd_args)
            .output()
            .map_err(|e| StivaError::Network(format!("nsenter failed: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(StivaError::Network(format!(
                "nsenter ip {} failed: {}",
                args.join(" "),
                stderr.trim()
            )));
        }
        Ok(())
    };

    // Assign IP.
    nsenter(&[
        "addr",
        "add",
        &format!("{container_ip}/{prefix_len}"),
        "dev",
        iface,
    ])?;

    // Bring interface up.
    nsenter(&["link", "set", iface, "up"])?;

    // Bring loopback up.
    nsenter(&["link", "set", "lo", "up"])?;

    // Set default route via gateway.
    nsenter(&["route", "add", "default", "via", &gateway.to_string()])?;

    Ok(())
}

/// Delete a veth pair (deleting one end removes the other).
pub fn delete_veth(host_veth: &str) -> Result<(), StivaError> {
    // If already gone (container removed), ignore errors.
    let _ = ip(&["link", "delete", host_veth]);
    Ok(())
}

/// Enable IPv4 forwarding (needed for NAT).
pub fn enable_ip_forward() -> Result<(), StivaError> {
    sysctl("net.ipv4.ip_forward", "1")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn veth_name_generation() {
        // Can't actually create veths without root, but verify name format.
        let id = "abc123def456ghij";
        let short = &id[..12.min(id.len())];
        let host_veth = format!("ve-{short}");
        assert_eq!(host_veth, "ve-abc123def456");
        assert!(host_veth.len() <= 15); // IFNAMSIZ
    }

    #[test]
    fn ip_command_failure_returns_error() {
        let result = ip(&["invalid-subcommand-that-doesnt-exist"]);
        assert!(result.is_err());
    }

    #[test]
    fn veth_names_fit_ifnamsiz() {
        // "ve-" (3) + 12 chars = 15, exactly IFNAMSIZ - 1.
        for id in ["a".repeat(12), "0123456789ab".into(), "short".into()] {
            let short = &id[..12.min(id.len())];
            let name = format!("ve-{short}");
            assert!(
                name.len() <= 15,
                "name '{name}' exceeds IFNAMSIZ (len={})",
                name.len()
            );
        }
    }
}
