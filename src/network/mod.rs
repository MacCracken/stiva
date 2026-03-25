//! Container networking — bridge networks, NAT, port mapping, veth pairs, DNS.

pub mod bridge;
pub mod dns;
pub mod manager;
pub mod nat;
pub mod pool;

use serde::{Deserialize, Serialize};

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
    /// Assigned IP address.
    pub ip: std::net::Ipv4Addr,
    /// Network name.
    pub network_name: String,
    /// Host-side veth interface name.
    pub host_veth: String,
    /// Container-side veth interface name.
    pub container_veth: String,
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
            network_name: "bridge".into(),
            host_veth: "veth-abc".into(),
            container_veth: "eth0".into(),
        };
        let json = serde_json::to_string(&cn).unwrap();
        let back: ContainerNetwork = serde_json::from_str(&json).unwrap();
        assert_eq!(back.ip, "172.17.0.2".parse::<std::net::Ipv4Addr>().unwrap());
    }
}
