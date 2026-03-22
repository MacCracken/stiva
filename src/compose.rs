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
    toml::from_str(toml_str).map_err(|e| StivaError::Compose(format!("invalid compose file: {e}")))
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

    #[test]
    fn parse_compose_invalid_toml() {
        let result = parse_compose("not a [valid toml");
        assert!(result.is_err());
    }

    #[test]
    fn parse_compose_missing_services() {
        let result = parse_compose("[networks.default]");
        assert!(result.is_err());
    }

    #[test]
    fn parse_compose_minimal() {
        let toml = r#"
[services.app]
image = "alpine"
"#;
        let compose = parse_compose(toml).unwrap();
        assert_eq!(compose.services.len(), 1);
        assert!(compose.services["app"].command.is_empty());
        assert!(compose.services["app"].depends_on.is_empty());
        assert!(compose.networks.is_empty());
        assert!(compose.volumes.is_empty());
    }

    #[test]
    fn parse_compose_with_networks() {
        let toml = r#"
[services.app]
image = "alpine"

[networks.frontend]
driver = "bridge"
subnet = "10.0.0.0/24"
"#;
        let compose = parse_compose(toml).unwrap();
        assert_eq!(compose.networks.len(), 1);
        assert_eq!(
            compose.networks["frontend"].driver.as_deref(),
            Some("bridge")
        );
        assert_eq!(
            compose.networks["frontend"].subnet.as_deref(),
            Some("10.0.0.0/24")
        );
    }

    #[test]
    fn parse_compose_replicas() {
        let toml = r#"
[services.worker]
image = "worker:latest"
replicas = 3
"#;
        let compose = parse_compose(toml).unwrap();
        assert_eq!(compose.services["worker"].replicas, Some(3));
    }

    #[test]
    fn parse_compose_empty_string() {
        assert!(parse_compose("").is_err());
    }

    #[test]
    fn parse_compose_service_with_command() {
        let toml = r#"
[services.app]
image = "alpine"
command = ["/bin/sh", "-c", "echo hello"]
"#;
        let compose = parse_compose(toml).unwrap();
        assert_eq!(
            compose.services["app"].command,
            vec!["/bin/sh", "-c", "echo hello"]
        );
    }

    #[test]
    fn parse_compose_dependency_chain() {
        let toml = r#"
[services.frontend]
image = "nginx"
depends_on = ["api"]

[services.api]
image = "app:latest"
depends_on = ["db", "cache"]

[services.db]
image = "postgres:16"

[services.cache]
image = "redis:7"
"#;
        let compose = parse_compose(toml).unwrap();
        assert_eq!(compose.services.len(), 4);
        assert_eq!(compose.services["frontend"].depends_on, vec!["api"]);
        assert_eq!(compose.services["api"].depends_on, vec!["db", "cache"]);
        assert!(compose.services["db"].depends_on.is_empty());
    }

    #[test]
    fn parse_compose_service_all_fields() {
        let toml = r#"
[services.app]
image = "myapp:latest"
command = ["/start.sh"]
env = { PORT = "8080", DEBUG = "true" }
ports = ["8080:80", "443:443"]
volumes = ["/data:/app/data:ro"]
depends_on = ["db"]
replicas = 2
"#;
        let compose = parse_compose(toml).unwrap();
        let svc = &compose.services["app"];
        assert_eq!(svc.image, "myapp:latest");
        assert_eq!(svc.command, vec!["/start.sh"]);
        assert_eq!(svc.env.len(), 2);
        assert_eq!(svc.ports.len(), 2);
        assert_eq!(svc.volumes, vec!["/data:/app/data:ro"]);
        assert_eq!(svc.depends_on, vec!["db"]);
        assert_eq!(svc.replicas, Some(2));
    }

    #[test]
    fn network_def_serde() {
        let net = NetworkDef {
            driver: Some("bridge".into()),
            subnet: Some("10.0.0.0/24".into()),
        };
        let json = serde_json::to_string(&net).unwrap();
        let back: NetworkDef = serde_json::from_str(&json).unwrap();
        assert_eq!(back.driver.as_deref(), Some("bridge"));
    }

    #[test]
    fn volume_def_serde() {
        let vol = VolumeDef {
            driver: Some("local".into()),
        };
        let json = serde_json::to_string(&vol).unwrap();
        let back: VolumeDef = serde_json::from_str(&json).unwrap();
        assert_eq!(back.driver.as_deref(), Some("local"));
    }

    #[test]
    fn network_def_default() {
        let net = NetworkDef::default();
        assert!(net.driver.is_none());
        assert!(net.subnet.is_none());
    }

    #[test]
    fn volume_def_default() {
        let vol = VolumeDef::default();
        assert!(vol.driver.is_none());
    }

    #[test]
    fn compose_file_serde_round_trip() {
        let compose = ComposeFile {
            services: HashMap::from([(
                "web".to_string(),
                ServiceDef {
                    image: "nginx".to_string(),
                    command: vec![],
                    env: HashMap::new(),
                    ports: vec!["80:80".to_string()],
                    volumes: vec![],
                    depends_on: vec![],
                    replicas: None,
                },
            )]),
            networks: HashMap::new(),
            volumes: HashMap::new(),
        };
        let json = serde_json::to_string(&compose).unwrap();
        let back: ComposeFile = serde_json::from_str(&json).unwrap();
        assert_eq!(back.services.len(), 1);
        assert_eq!(back.services["web"].image, "nginx");
    }
}
