//! Multi-container orchestration — compose-file equivalent.

use crate::error::StivaError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A compose file definition (TOML-based, not YAML).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComposeFile {
    pub services: HashMap<String, ServiceDef>,
    #[serde(default)]
    pub networks: HashMap<String, NetworkDef>,
    #[serde(default)]
    pub volumes: HashMap<String, VolumeDef>,
}

/// A service definition within a compose file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceDef {
    pub image: String,
    #[serde(default)]
    pub command: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(default)]
    pub ports: Vec<String>,
    #[serde(default)]
    pub volumes: Vec<String>,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub replicas: Option<u32>,
}

/// Network definition.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NetworkDef {
    pub driver: Option<String>,
    pub subnet: Option<String>,
}

/// Volume definition.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VolumeDef {
    pub driver: Option<String>,
}

/// Parse a compose file from TOML.
pub fn parse_compose(toml_str: &str) -> Result<ComposeFile, StivaError> {
    toml::from_str(toml_str)
        .map_err(|e| StivaError::Runtime(format!("invalid compose file: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_compose_toml() {
        let toml = r#"
[services.web]
image = "nginx:latest"
ports = ["80:80"]

[services.api]
image = "ghcr.io/maccracken/agnosticos:latest"
env = { PORT = "8090" }
depends_on = ["db"]

[services.db]
image = "postgres:16"
volumes = ["pgdata:/var/lib/postgresql/data"]
env = { POSTGRES_PASSWORD = "secret" }

[volumes.pgdata]
"#;
        let compose = parse_compose(toml).unwrap();
        assert_eq!(compose.services.len(), 3);
        assert_eq!(compose.services["api"].depends_on, vec!["db"]);
        assert!(compose.volumes.contains_key("pgdata"));
    }
}
