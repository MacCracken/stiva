//! Container networking — bridge, host, none, custom modes.

use crate::error::StivaError;
use serde::{Deserialize, Serialize};

/// Network mode for a container.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
pub enum NetworkDriver {
    Bridge,
    Overlay,
    Macvlan,
}

/// Set up networking for a container.
pub async fn setup_container_network(
    _container_id: &str,
    _mode: &NetworkMode,
) -> Result<(), StivaError> {
    // TODO: Create veth pair via agnosys netns
    // TODO: Configure bridge/NAT via nftables (nein crate, when available)
    // TODO: Assign IP from subnet pool
    Ok(())
}

/// Tear down container networking.
pub async fn teardown_container_network(_container_id: &str) -> Result<(), StivaError> {
    // TODO: Remove veth pair, release IP
    Ok(())
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

    #[tokio::test]
    async fn setup_network_stub() {
        // Stub returns Ok for all modes.
        for mode in [
            NetworkMode::Bridge,
            NetworkMode::Host,
            NetworkMode::None,
            NetworkMode::Container("abc".into()),
            NetworkMode::Custom("net0".into()),
        ] {
            setup_container_network("test-container", &mode)
                .await
                .unwrap();
        }
    }

    #[tokio::test]
    async fn teardown_network_stub() {
        teardown_container_network("test-container").await.unwrap();
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
}
