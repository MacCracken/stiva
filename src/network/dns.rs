//! DNS configuration for containers — resolv.conf and hosts injection.

use crate::error::StivaError;
use std::net::Ipv4Addr;
use std::path::Path;

/// Default DNS servers if host resolv.conf is unavailable.
const DEFAULT_DNS: &[&str] = &["8.8.8.8", "8.8.4.4"];

/// Read DNS servers from the host's /etc/resolv.conf.
pub fn host_dns_servers() -> Vec<Ipv4Addr> {
    match std::fs::read_to_string("/etc/resolv.conf") {
        Ok(content) => parse_resolv_conf(&content),
        Err(_) => DEFAULT_DNS.iter().filter_map(|s| s.parse().ok()).collect(),
    }
}

/// Parse nameserver entries from resolv.conf content.
pub fn parse_resolv_conf(content: &str) -> Vec<Ipv4Addr> {
    content
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if let Some(rest) = line.strip_prefix("nameserver") {
                rest.trim().parse::<Ipv4Addr>().ok()
            } else {
                None
            }
        })
        .collect()
}

/// Write /etc/resolv.conf into the container rootfs.
pub fn inject_resolv_conf(rootfs: &Path, dns_servers: &[Ipv4Addr]) -> Result<(), StivaError> {
    let etc = rootfs.join("etc");
    std::fs::create_dir_all(&etc)?;

    let servers = if dns_servers.is_empty() {
        DEFAULT_DNS
            .iter()
            .filter_map(|s| s.parse().ok())
            .collect::<Vec<Ipv4Addr>>()
    } else {
        dns_servers.to_vec()
    };

    let content: String = servers
        .iter()
        .map(|s| format!("nameserver {s}"))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";

    std::fs::write(etc.join("resolv.conf"), &content)?;
    Ok(())
}

/// Write /etc/hosts into the container rootfs.
pub fn inject_hosts(
    rootfs: &Path,
    container_ip: Ipv4Addr,
    hostname: &str,
) -> Result<(), StivaError> {
    let etc = rootfs.join("etc");
    std::fs::create_dir_all(&etc)?;

    let content = format!(
        "127.0.0.1\tlocalhost\n\
         ::1\t\tlocalhost\n\
         {container_ip}\t{hostname}\n"
    );

    std::fs::write(etc.join("hosts"), &content)?;
    Ok(())
}

/// Write /etc/hostname into the container rootfs.
pub fn inject_hostname(rootfs: &Path, hostname: &str) -> Result<(), StivaError> {
    let etc = rootfs.join("etc");
    std::fs::create_dir_all(&etc)?;
    std::fs::write(etc.join("hostname"), format!("{hostname}\n"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_resolv_conf_basic() {
        let content = "nameserver 8.8.8.8\nnameserver 8.8.4.4\n";
        let servers = parse_resolv_conf(content);
        assert_eq!(servers.len(), 2);
        assert_eq!(servers[0], "8.8.8.8".parse::<Ipv4Addr>().unwrap());
        assert_eq!(servers[1], "8.8.4.4".parse::<Ipv4Addr>().unwrap());
    }

    #[test]
    fn parse_resolv_conf_with_comments() {
        let content = "# DNS\nnameserver 1.1.1.1\nsearch example.com\nnameserver 1.0.0.1\n";
        let servers = parse_resolv_conf(content);
        assert_eq!(servers.len(), 2);
        assert_eq!(servers[0], "1.1.1.1".parse::<Ipv4Addr>().unwrap());
    }

    #[test]
    fn parse_resolv_conf_empty() {
        assert!(parse_resolv_conf("").is_empty());
    }

    #[test]
    fn parse_resolv_conf_ipv6_skipped() {
        let content = "nameserver 8.8.8.8\nnameserver ::1\n";
        let servers = parse_resolv_conf(content);
        assert_eq!(servers.len(), 1); // IPv6 skipped.
    }

    #[test]
    fn inject_resolv_conf_to_rootfs() {
        let dir = tempfile::tempdir().unwrap();
        let servers = vec![
            "1.1.1.1".parse::<Ipv4Addr>().unwrap(),
            "1.0.0.1".parse().unwrap(),
        ];
        inject_resolv_conf(dir.path(), &servers).unwrap();

        let content = std::fs::read_to_string(dir.path().join("etc/resolv.conf")).unwrap();
        assert!(content.contains("nameserver 1.1.1.1"));
        assert!(content.contains("nameserver 1.0.0.1"));
    }

    #[test]
    fn inject_resolv_conf_defaults_when_empty() {
        let dir = tempfile::tempdir().unwrap();
        inject_resolv_conf(dir.path(), &[]).unwrap();

        let content = std::fs::read_to_string(dir.path().join("etc/resolv.conf")).unwrap();
        assert!(content.contains("nameserver 8.8.8.8"));
    }

    #[test]
    fn inject_hosts_to_rootfs() {
        let dir = tempfile::tempdir().unwrap();
        inject_hosts(dir.path(), "172.17.0.2".parse().unwrap(), "mycontainer").unwrap();

        let content = std::fs::read_to_string(dir.path().join("etc/hosts")).unwrap();
        assert!(content.contains("127.0.0.1\tlocalhost"));
        assert!(content.contains("172.17.0.2\tmycontainer"));
    }

    #[test]
    fn inject_hostname_to_rootfs() {
        let dir = tempfile::tempdir().unwrap();
        inject_hostname(dir.path(), "web-server").unwrap();

        let content = std::fs::read_to_string(dir.path().join("etc/hostname")).unwrap();
        assert_eq!(content.trim(), "web-server");
    }

    #[test]
    fn host_dns_servers_returns_something() {
        let servers = host_dns_servers();
        // Should always return at least defaults.
        assert!(!servers.is_empty());
    }
}
