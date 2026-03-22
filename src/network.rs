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
pub async fn teardown_container_network(
    _container_id: &str,
) -> Result<(), StivaError> {
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
}
