//! Container networking — bridge networks, NAT, port mapping, veth pairs, DNS.

pub mod bridge;
pub mod dns;
pub mod manager;
pub mod nat;
pub mod pool;
pub mod rootless;

use crate::error::StivaError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Network mode for a container.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum NetworkMode {
    /// Isolated bridge network with NAT.
    Bridge,
    /// Share host network namespace.
    Host,
    /// No networking.
    None,
    /// Share another container's network namespace.
    Container(String),
    /// Custom named network.
    Custom(String),
}

/// A virtual network.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Network {
    pub name: String,
    pub subnet: String,
    pub gateway: String,
    pub driver: NetworkDriver,
}

/// Network driver type.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub enum NetworkDriver {
    Bridge,
    Overlay,
    Macvlan,
}

/// Result of connecting a container to a network.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerNetwork {
    /// Assigned IPv4 address.
    pub ip: std::net::Ipv4Addr,
    /// Assigned IPv6 address (if dual-stack enabled).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ipv6: Option<std::net::Ipv6Addr>,
    /// Network name.
    pub network_name: String,
    /// Host-side veth interface name.
    pub host_veth: String,
    /// Container-side veth interface name.
    pub container_veth: String,
}

// ---------------------------------------------------------------------------
// Network policy
// ---------------------------------------------------------------------------

/// Per-container network policy controlling egress and ingress.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NetworkPolicy {
    /// Allowed egress destinations (CIDR or hostname). Empty = allow all.
    #[serde(default)]
    pub egress_allow: Vec<String>,
    /// Blocked egress destinations (CIDR or hostname). Applied after allow.
    #[serde(default)]
    pub egress_deny: Vec<String>,
    /// Allowed ingress source CIDRs. Empty = allow all.
    #[serde(default)]
    pub ingress_allow: Vec<String>,
    /// Blocked ingress source CIDRs.
    #[serde(default)]
    pub ingress_deny: Vec<String>,
    /// Allowed egress ports. Empty = allow all.
    #[serde(default)]
    pub egress_ports: Vec<u16>,
    /// Rate limit outbound traffic (bytes/sec, 0 = unlimited).
    #[serde(default)]
    pub egress_rate_limit: u64,
}

impl NetworkPolicy {
    /// Returns true if any restrictions are configured.
    #[must_use]
    pub fn has_restrictions(&self) -> bool {
        !self.egress_allow.is_empty()
            || !self.egress_deny.is_empty()
            || !self.ingress_allow.is_empty()
            || !self.ingress_deny.is_empty()
            || !self.egress_ports.is_empty()
            || self.egress_rate_limit > 0
    }

    /// Generate nftables rules for this policy.
    #[must_use]
    pub fn to_nft_rules(&self, container_ip: &str) -> Vec<String> {
        let mut rules = Vec::new();

        for cidr in &self.egress_deny {
            rules.push(format!(
                "add rule inet stiva-policy forward ip saddr {container_ip} ip daddr {cidr} drop"
            ));
        }

        for cidr in &self.ingress_deny {
            rules.push(format!(
                "add rule inet stiva-policy forward ip saddr {cidr} ip daddr {container_ip} drop"
            ));
        }

        rules
    }
}

// ---------------------------------------------------------------------------
// Container DNS registry (for ansamblu service discovery)
// ---------------------------------------------------------------------------

/// Maps container/service names to IP addresses for intra-session DNS.
#[derive(Debug, Default)]
pub struct DnsRegistry {
    /// name → IPv4 address mapping.
    entries: HashMap<String, std::net::Ipv4Addr>,
}

impl DnsRegistry {
    /// Register a name→IP mapping.
    pub fn register(&mut self, name: &str, ip: std::net::Ipv4Addr) {
        self.entries.insert(name.to_string(), ip);
    }

    /// Remove a mapping.
    pub fn unregister(&mut self, name: &str) {
        self.entries.remove(name);
    }

    /// Look up an IP by name.
    #[must_use]
    pub fn resolve(&self, name: &str) -> Option<std::net::Ipv4Addr> {
        self.entries.get(name).copied()
    }

    /// Generate /etc/hosts content for all registered entries.
    #[must_use]
    pub fn to_hosts_entries(&self) -> String {
        let mut out = String::new();
        for (name, ip) in &self.entries {
            out.push_str(&format!("{ip}\t{name}\n"));
        }
        out
    }

    /// Inject all entries into a container's /etc/hosts file.
    pub fn inject_into(
        &self,
        rootfs: &std::path::Path,
        container_ip: std::net::Ipv4Addr,
        hostname: &str,
    ) -> Result<(), StivaError> {
        let etc = rootfs.join("etc");
        std::fs::create_dir_all(&etc)?;

        let mut content = format!(
            "127.0.0.1\tlocalhost\n\
             ::1\t\tlocalhost\n\
             {container_ip}\t{hostname}\n"
        );

        // Add all registered service names for container-to-container DNS.
        content.push_str(&self.to_hosts_entries());

        std::fs::write(etc.join("hosts"), &content)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn network_mode_serde() {
        let mode = NetworkMode::Bridge;
        let json = serde_json::to_string(&mode).unwrap();
        let back: NetworkMode = serde_json::from_str(&json).unwrap();
        assert_eq!(mode, back);
    }

    #[test]
    fn network_mode_container() {
        let mode = NetworkMode::Container("abc123".to_string());
        let json = serde_json::to_string(&mode).unwrap();
        let back: NetworkMode = serde_json::from_str(&json).unwrap();
        assert_eq!(mode, back);
    }

    #[test]
    fn network_mode_all_variants_serde() {
        let modes = vec![
            NetworkMode::Bridge,
            NetworkMode::Host,
            NetworkMode::None,
            NetworkMode::Container("abc".into()),
            NetworkMode::Custom("my-net".into()),
        ];
        for mode in modes {
            let json = serde_json::to_string(&mode).unwrap();
            let back: NetworkMode = serde_json::from_str(&json).unwrap();
            assert_eq!(mode, back);
        }
    }

    #[test]
    fn network_struct_serde() {
        let net = Network {
            name: "stiva-bridge0".to_string(),
            subnet: "172.17.0.0/16".to_string(),
            gateway: "172.17.0.1".to_string(),
            driver: NetworkDriver::Bridge,
        };
        let json = serde_json::to_string(&net).unwrap();
        let back: Network = serde_json::from_str(&json).unwrap();
        assert_eq!(back.name, "stiva-bridge0");
        assert_eq!(back.subnet, "172.17.0.0/16");
    }

    #[test]
    fn network_mode_custom_value() {
        let mode = NetworkMode::Custom("my-overlay".into());
        let json = serde_json::to_string(&mode).unwrap();
        let back: NetworkMode = serde_json::from_str(&json).unwrap();
        match back {
            NetworkMode::Custom(name) => assert_eq!(name, "my-overlay"),
            other => panic!("expected Custom, got {other:?}"),
        }
    }

    #[test]
    fn network_driver_variants() {
        for driver in [
            NetworkDriver::Bridge,
            NetworkDriver::Overlay,
            NetworkDriver::Macvlan,
        ] {
            let json = serde_json::to_string(&driver).unwrap();
            let _back: NetworkDriver = serde_json::from_str(&json).unwrap();
        }
    }

    #[test]
    fn container_network_serde() {
        let cn = ContainerNetwork {
            ip: "172.17.0.2".parse().unwrap(),
            ipv6: None,
            network_name: "bridge".into(),
            host_veth: "veth-abc".into(),
            container_veth: "eth0".into(),
        };
        let json = serde_json::to_string(&cn).unwrap();
        let back: ContainerNetwork = serde_json::from_str(&json).unwrap();
        assert_eq!(back.ip, "172.17.0.2".parse::<std::net::Ipv4Addr>().unwrap());
    }

    #[test]
    fn container_network_dual_stack_serde() {
        let cn = ContainerNetwork {
            ip: "172.17.0.2".parse().unwrap(),
            ipv6: Some("fd00::2".parse().unwrap()),
            network_name: "bridge".into(),
            host_veth: "veth-abc".into(),
            container_veth: "eth0".into(),
        };
        let json = serde_json::to_string(&cn).unwrap();
        let back: ContainerNetwork = serde_json::from_str(&json).unwrap();
        assert_eq!(
            back.ipv6,
            Some("fd00::2".parse::<std::net::Ipv6Addr>().unwrap())
        );
    }

    #[test]
    fn network_policy_default_no_restrictions() {
        let policy = NetworkPolicy::default();
        assert!(!policy.has_restrictions());
    }

    #[test]
    fn network_policy_with_deny() {
        let policy = NetworkPolicy {
            egress_deny: vec!["10.0.0.0/8".into()],
            ..Default::default()
        };
        assert!(policy.has_restrictions());
        let rules = policy.to_nft_rules("172.17.0.2");
        assert_eq!(rules.len(), 1);
        assert!(rules[0].contains("10.0.0.0/8"));
        assert!(rules[0].contains("drop"));
    }

    #[test]
    fn dns_registry_resolve() {
        let mut reg = DnsRegistry::default();
        reg.register("web", "172.17.0.2".parse().unwrap());
        reg.register("db", "172.17.0.3".parse().unwrap());

        assert_eq!(reg.resolve("web"), Some("172.17.0.2".parse().unwrap()));
        assert_eq!(reg.resolve("db"), Some("172.17.0.3".parse().unwrap()));
        assert_eq!(reg.resolve("missing"), None);
    }

    #[test]
    fn dns_registry_unregister() {
        let mut reg = DnsRegistry::default();
        reg.register("web", "172.17.0.2".parse().unwrap());
        reg.unregister("web");
        assert_eq!(reg.resolve("web"), None);
    }

    #[test]
    fn dns_registry_hosts_entries() {
        let mut reg = DnsRegistry::default();
        reg.register("web", "172.17.0.2".parse().unwrap());
        let hosts = reg.to_hosts_entries();
        assert!(hosts.contains("172.17.0.2\tweb"));
    }

    #[test]
    fn dns_registry_inject() {
        let dir = tempfile::tempdir().unwrap();
        let mut reg = DnsRegistry::default();
        reg.register("db", "172.17.0.3".parse().unwrap());

        reg.inject_into(dir.path(), "172.17.0.2".parse().unwrap(), "web")
            .unwrap();

        let content = std::fs::read_to_string(dir.path().join("etc/hosts")).unwrap();
        assert!(content.contains("172.17.0.2\tweb"));
        assert!(content.contains("172.17.0.3\tdb"));
    }
}
