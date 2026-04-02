//! Network manager — lifecycle for bridge networks, container connectivity.

use super::pool::IpPool;
use super::{ContainerNetwork, NetworkMode};
use crate::error::StivaError;
use nein::bridge::BridgeFirewall;
use std::collections::HashMap;
use std::path::Path;
use std::process::Command;
use tracing::info;

/// Default bridge network name.
pub const DEFAULT_BRIDGE: &str = "stiva0";

/// Default bridge subnet.
pub const DEFAULT_SUBNET: &str = "172.17.0.0/16";

/// Default outbound interface for NAT.
const DEFAULT_OUTBOUND_IFACE: &str = "eth0";

/// A managed bridge network.
struct ManagedNetwork {
    pool: IpPool,
    #[allow(dead_code)]
    bridge_created: bool,
}

/// Manages container networks — bridge creation, IP allocation, veth setup, NAT.
pub struct NetworkManager {
    /// Named networks, keyed by name.
    networks: HashMap<String, ManagedNetwork>,
    /// Container → network connections.
    connections: HashMap<String, ContainerNetwork>,
    /// Outbound interface for NAT masquerade.
    outbound_iface: String,
}

impl NetworkManager {
    /// Create a new network manager with the default bridge network.
    pub fn new() -> Result<Self, StivaError> {
        Self::with_outbound(DEFAULT_OUTBOUND_IFACE)
    }

    /// Create with a specific outbound interface.
    pub fn with_outbound(outbound_iface: &str) -> Result<Self, StivaError> {
        let mut mgr = Self {
            networks: HashMap::new(),
            connections: HashMap::new(),
            outbound_iface: outbound_iface.to_string(),
        };

        // Create default bridge network (IP pool only; bridge interface requires root).
        mgr.create_network(DEFAULT_BRIDGE, DEFAULT_SUBNET)?;

        Ok(mgr)
    }

    /// Create a named network with the given subnet.
    pub fn create_network(&mut self, name: &str, subnet: &str) -> Result<(), StivaError> {
        if self.networks.contains_key(name) {
            return Err(StivaError::Network(format!(
                "network '{name}' already exists"
            )));
        }

        let pool = IpPool::new(subnet)?;

        // Try to create the bridge interface (requires root).
        let bridge_created =
            match super::bridge::create_bridge(name, pool.gateway(), pool.prefix_len()) {
                Ok(()) => {
                    info!(network = name, subnet, "bridge network created");
                    true
                }
                Err(e) => {
                    tracing::warn!(
                        network = name,
                        "bridge creation deferred (requires root): {e}"
                    );
                    false
                }
            };

        self.networks.insert(
            name.to_string(),
            ManagedNetwork {
                pool,
                bridge_created,
            },
        );

        Ok(())
    }

    /// Delete a named network.
    pub fn delete_network(&mut self, name: &str) -> Result<(), StivaError> {
        let network = self
            .networks
            .remove(name)
            .ok_or_else(|| StivaError::Network(format!("network '{name}' not found")))?;

        // Check no containers are connected.
        let connected: Vec<_> = self
            .connections
            .iter()
            .filter(|(_, cn)| cn.network_name == name)
            .map(|(id, _)| id.clone())
            .collect();

        if !connected.is_empty() {
            // Put network back.
            self.networks.insert(name.to_string(), network);
            return Err(StivaError::Network(format!(
                "cannot delete network '{name}': {} containers connected",
                connected.len()
            )));
        }

        let _ = super::bridge::delete_bridge(name);
        info!(network = name, "network deleted");
        Ok(())
    }

    /// Connect a container to a network.
    ///
    /// Allocates an IP, creates a veth pair, and sets up the connection.
    /// Port mappings are parsed and NAT rules created.
    pub fn connect_container(
        &mut self,
        container_id: &str,
        network_name: &str,
        port_specs: &[String],
        rootfs: Option<&Path>,
    ) -> Result<ContainerNetwork, StivaError> {
        let network = self
            .networks
            .get_mut(network_name)
            .ok_or_else(|| StivaError::Network(format!("network '{network_name}' not found")))?;

        // Allocate IP.
        let ip = network.pool.allocate()?;

        // Create veth pair.
        let (host_veth, container_veth) = match super::bridge::create_veth_pair(container_id) {
            Ok(pair) => pair,
            Err(e) => {
                // Keep IP allocated — the connection is still tracked.
                tracing::warn!("veth creation deferred (requires root): {e}");
                (
                    format!("ve-{}", &container_id[..12.min(container_id.len())]),
                    "eth0".into(),
                )
            }
        };

        // Attach host side to bridge.
        let _ = super::bridge::attach_to_bridge(&host_veth, network_name);

        // Parse port mappings and apply NAT rules via nein.
        if !port_specs.is_empty() {
            let bridge_cfg = super::nat::bridge_config(
                network_name,
                &network.pool.subnet(),
                &self.outbound_iface,
            );
            let mut bf = BridgeFirewall::new(bridge_cfg);
            for spec in port_specs {
                let port_spec = super::nat::parse_port_spec(spec)?;
                let mapping = super::nat::to_nein_port_mapping(&port_spec, ip);
                bf.add_port_mapping(mapping)
                    .map_err(|e| StivaError::Network(format!("port mapping error: {e}")))?;
            }
            let fw = bf.to_firewall();
            if let Err(e) = fw.validate() {
                tracing::warn!("firewall validation failed: {e}");
            } else {
                apply_nft_ruleset(&fw.render());
            }
        }

        // Inject DNS into container rootfs if available.
        if let Some(rootfs) = rootfs {
            let dns = super::dns::host_dns_servers();
            let _ = super::dns::inject_resolv_conf(rootfs, &dns);
            let hostname = container_id[..12.min(container_id.len())].to_string();
            let _ = super::dns::inject_hosts(rootfs, ip, &hostname);
            let _ = super::dns::inject_hostname(rootfs, &hostname);
        }

        let cn = ContainerNetwork {
            ip,
            network_name: network_name.to_string(),
            host_veth,
            container_veth,
        };

        self.connections
            .insert(container_id.to_string(), cn.clone());

        info!(
            container = container_id,
            ip = %ip,
            network = network_name,
            "container connected to network"
        );

        Ok(cn)
    }

    /// Disconnect a container from its network.
    pub fn disconnect_container(&mut self, container_id: &str) -> Result<(), StivaError> {
        let cn = self.connections.remove(container_id).ok_or_else(|| {
            StivaError::Network(format!(
                "container '{container_id}' not connected to any network"
            ))
        })?;

        // Release IP back to pool.
        if let Some(network) = self.networks.get_mut(&cn.network_name) {
            network.pool.release(&cn.ip);
        }

        // Delete veth pair.
        let _ = super::bridge::delete_veth(&cn.host_veth);

        info!(
            container = container_id,
            ip = %cn.ip,
            network = cn.network_name,
            "container disconnected from network"
        );

        Ok(())
    }

    /// Resolve which network to use based on NetworkMode.
    #[inline]
    #[must_use]
    pub fn resolve_network_name(&self, mode: &NetworkMode) -> Option<String> {
        match mode {
            NetworkMode::Bridge => Some(DEFAULT_BRIDGE.to_string()),
            NetworkMode::Custom(name) => Some(name.clone()),
            NetworkMode::Host | NetworkMode::None | NetworkMode::Container(_) => None,
        }
    }

    /// Get the connection info for a container.
    #[inline]
    #[must_use]
    pub fn get_connection(&self, container_id: &str) -> Option<&ContainerNetwork> {
        self.connections.get(container_id)
    }

    /// List all networks.
    #[must_use]
    pub fn list_networks(&self) -> Vec<&str> {
        self.networks.keys().map(|s| s.as_str()).collect()
    }

    /// Get the IP pool for a network (for inspection).
    #[inline]
    #[must_use]
    pub fn get_pool(&self, network_name: &str) -> Option<&IpPool> {
        self.networks.get(network_name).map(|n| &n.pool)
    }
}

/// Apply an nftables ruleset via `nft -f -` (synchronous, best-effort).
fn apply_nft_ruleset(ruleset: &str) {
    match Command::new("nft")
        .args(["-f", "-"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
    {
        Ok(mut child) => {
            if let Some(mut stdin) = child.stdin.take() {
                use std::io::Write;
                let _ = stdin.write_all(ruleset.as_bytes());
            }
            match child.wait_with_output() {
                Ok(output) if output.status.success() => {
                    tracing::debug!(bytes = ruleset.len(), "applied nftables ruleset");
                }
                Ok(output) => {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    tracing::warn!(%stderr, "nft command failed (non-fatal)");
                }
                Err(e) => {
                    tracing::warn!(%e, "nft process error (non-fatal)");
                }
            }
        }
        Err(e) => {
            tracing::warn!(%e, "nft not available (non-fatal)");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    #[test]
    fn manager_creates_default_network() {
        let mgr = NetworkManager::new().unwrap();
        let networks = mgr.list_networks();
        assert!(networks.contains(&DEFAULT_BRIDGE));
    }

    #[test]
    fn create_custom_network() {
        let mut mgr = NetworkManager::new().unwrap();
        mgr.create_network("mynet", "10.0.0.0/24").unwrap();
        assert!(mgr.list_networks().contains(&"mynet"));
    }

    #[test]
    fn create_duplicate_network_fails() {
        let mut mgr = NetworkManager::new().unwrap();
        assert!(mgr.create_network(DEFAULT_BRIDGE, "10.0.0.0/24").is_err());
    }

    #[test]
    fn delete_network() {
        let mut mgr = NetworkManager::new().unwrap();
        mgr.create_network("temp", "10.1.0.0/24").unwrap();
        mgr.delete_network("temp").unwrap();
        assert!(!mgr.list_networks().contains(&"temp"));
    }

    #[test]
    fn delete_nonexistent_network_fails() {
        let mut mgr = NetworkManager::new().unwrap();
        assert!(mgr.delete_network("nope").is_err());
    }

    #[test]
    fn connect_container_allocates_ip() {
        let mut mgr = NetworkManager::new().unwrap();
        let cn = mgr
            .connect_container("container-abc123", DEFAULT_BRIDGE, &[], None)
            .unwrap();
        assert_eq!(cn.network_name, DEFAULT_BRIDGE);
        assert_eq!(cn.ip, "172.17.0.2".parse::<Ipv4Addr>().unwrap());
    }

    #[test]
    fn connect_multiple_containers() {
        let mut mgr = NetworkManager::new().unwrap();
        let cn1 = mgr
            .connect_container("c1-abcdef123456", DEFAULT_BRIDGE, &[], None)
            .unwrap();
        let cn2 = mgr
            .connect_container("c2-abcdef123456", DEFAULT_BRIDGE, &[], None)
            .unwrap();
        assert_ne!(cn1.ip, cn2.ip);
    }

    #[test]
    fn disconnect_container_releases_ip() {
        let mut mgr = NetworkManager::new().unwrap();
        mgr.connect_container("c1-abcdef123456", DEFAULT_BRIDGE, &[], None)
            .unwrap();

        let pool_before = mgr.get_pool(DEFAULT_BRIDGE).unwrap().allocated_count();
        assert_eq!(pool_before, 1);

        mgr.disconnect_container("c1-abcdef123456").unwrap();

        let pool_after = mgr.get_pool(DEFAULT_BRIDGE).unwrap().allocated_count();
        assert_eq!(pool_after, 0);
    }

    #[test]
    fn disconnect_nonexistent_fails() {
        let mut mgr = NetworkManager::new().unwrap();
        assert!(mgr.disconnect_container("nope").is_err());
    }

    #[test]
    fn delete_network_with_connections_fails() {
        let mut mgr = NetworkManager::new().unwrap();
        mgr.create_network("busy", "10.2.0.0/24").unwrap();
        mgr.connect_container("c1-abcdef123456", "busy", &[], None)
            .unwrap();
        assert!(mgr.delete_network("busy").is_err());
    }

    #[test]
    fn connect_with_port_specs() {
        let mut mgr = NetworkManager::new().unwrap();
        let ports = vec!["8080:80".to_string(), "443:443/tcp".to_string()];
        let cn = mgr
            .connect_container("c1-abcdef123456", DEFAULT_BRIDGE, &ports, None)
            .unwrap();
        assert_eq!(cn.ip, "172.17.0.2".parse::<Ipv4Addr>().unwrap());
    }

    #[test]
    fn connect_with_invalid_port_fails() {
        let mut mgr = NetworkManager::new().unwrap();
        let ports = vec!["not-a-port".to_string()];
        assert!(
            mgr.connect_container("c1-abcdef123456", DEFAULT_BRIDGE, &ports, None)
                .is_err()
        );
    }

    #[test]
    fn connect_to_nonexistent_network_fails() {
        let mut mgr = NetworkManager::new().unwrap();
        assert!(
            mgr.connect_container("c1-abcdef123456", "nope", &[], None)
                .is_err()
        );
    }

    #[test]
    fn connect_with_dns_injection() {
        let dir = tempfile::tempdir().unwrap();
        let mut mgr = NetworkManager::new().unwrap();
        mgr.connect_container("c1-abcdef123456", DEFAULT_BRIDGE, &[], Some(dir.path()))
            .unwrap();

        // DNS files should have been injected.
        assert!(dir.path().join("etc/resolv.conf").exists());
        assert!(dir.path().join("etc/hosts").exists());
        assert!(dir.path().join("etc/hostname").exists());
    }

    #[test]
    fn resolve_network_name_bridge() {
        let mgr = NetworkManager::new().unwrap();
        assert_eq!(
            mgr.resolve_network_name(&NetworkMode::Bridge),
            Some(DEFAULT_BRIDGE.to_string())
        );
    }

    #[test]
    fn resolve_network_name_custom() {
        let mgr = NetworkManager::new().unwrap();
        assert_eq!(
            mgr.resolve_network_name(&NetworkMode::Custom("mynet".into())),
            Some("mynet".to_string())
        );
    }

    #[test]
    fn resolve_network_name_host() {
        let mgr = NetworkManager::new().unwrap();
        assert!(mgr.resolve_network_name(&NetworkMode::Host).is_none());
    }

    #[test]
    fn resolve_network_name_none() {
        let mgr = NetworkManager::new().unwrap();
        assert!(mgr.resolve_network_name(&NetworkMode::None).is_none());
    }

    #[test]
    fn get_connection_info() {
        let mut mgr = NetworkManager::new().unwrap();
        mgr.connect_container("c1-abcdef123456", DEFAULT_BRIDGE, &[], None)
            .unwrap();
        let cn = mgr.get_connection("c1-abcdef123456").unwrap();
        assert_eq!(cn.network_name, DEFAULT_BRIDGE);
    }

    #[test]
    fn get_connection_nonexistent() {
        let mgr = NetworkManager::new().unwrap();
        assert!(mgr.get_connection("nope").is_none());
    }
}
