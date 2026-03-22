//! Daimon agent integration — register containers as agents.
//!
//! Uses HTTP to communicate with daimon's agent registry. This avoids a direct
//! Rust dependency on agent-runtime, matching the pattern used by sutra-daimon.

use crate::container::Container;
use crate::error::StivaError;
use serde::{Deserialize, Serialize};
use tracing::info;

/// Default daimon registry URL.
pub const DEFAULT_REGISTRY_URL: &str = "http://localhost:8090";

/// Agent registration payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRegistration {
    /// Agent ID (container ID).
    pub id: String,
    /// Agent name (container name).
    pub name: String,
    /// Agent type.
    pub agent_type: String,
    /// Capabilities provided.
    pub capabilities: Vec<String>,
    /// Metadata about the container.
    pub metadata: serde_json::Value,
}

/// Agent status report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentStatus {
    pub id: String,
    pub status: String,
    pub metadata: Option<serde_json::Value>,
}

/// Build an agent registration from a container.
pub fn build_registration(container: &Container) -> AgentRegistration {
    AgentRegistration {
        id: container.id.clone(),
        name: container
            .name
            .clone()
            .unwrap_or_else(|| container.id[..12].to_string()),
        agent_type: "container".into(),
        capabilities: vec!["container-runtime".into(), "stiva".into()],
        metadata: serde_json::json!({
            "image_ref": container.image_ref,
            "image_id": container.image_id,
            "state": format!("{:?}", container.state),
            "ports": container.config.ports,
        }),
    }
}

/// Register a container as an agent with daimon.
pub async fn register_container(
    client: &reqwest::Client,
    container: &Container,
    registry_url: &str,
) -> Result<(), StivaError> {
    let registration = build_registration(container);
    let url = format!("{registry_url}/v1/agents/register");

    let resp = client
        .post(&url)
        .json(&registration)
        .send()
        .await
        .map_err(|e| StivaError::Network(format!("daimon registration failed: {e}")))?;

    if !resp.status().is_success() {
        return Err(StivaError::Network(format!(
            "daimon registration returned HTTP {}",
            resp.status()
        )));
    }

    info!(
        container = container.id.as_str(),
        name = registration.name.as_str(),
        "registered with daimon"
    );
    Ok(())
}

/// Deregister a container from daimon.
pub async fn deregister_container(
    client: &reqwest::Client,
    container_id: &str,
    registry_url: &str,
) -> Result<(), StivaError> {
    let url = format!("{registry_url}/v1/agents/{container_id}");

    let resp = client
        .delete(&url)
        .send()
        .await
        .map_err(|e| StivaError::Network(format!("daimon deregistration failed: {e}")))?;

    if !resp.status().is_success() {
        tracing::warn!(
            container = container_id,
            status = resp.status().as_u16(),
            "daimon deregistration failed"
        );
    }

    Ok(())
}

/// Report container status to daimon.
pub async fn report_status(
    client: &reqwest::Client,
    container_id: &str,
    status: &str,
    registry_url: &str,
) -> Result<(), StivaError> {
    let url = format!("{registry_url}/v1/agents/{container_id}/status");

    let body = AgentStatus {
        id: container_id.to_string(),
        status: status.to_string(),
        metadata: None,
    };

    let resp = client
        .put(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| StivaError::Network(format!("daimon status report failed: {e}")))?;

    if !resp.status().is_success() {
        tracing::warn!(
            container = container_id,
            status = resp.status().as_u16(),
            "daimon status report failed"
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::container::{ContainerConfig, ContainerState};

    fn test_container() -> Container {
        Container {
            id: "abc123def456".into(),
            name: Some("web-server".into()),
            image_id: "sha256:img".into(),
            image_ref: "docker.io/library/nginx:latest".into(),
            state: ContainerState::Running,
            pid: Some(42),
            created_at: chrono::Utc::now(),
            started_at: Some(chrono::Utc::now()),
            config: ContainerConfig {
                ports: vec!["8080:80".into()],
                ..Default::default()
            },
            exit_code: None,
        }
    }

    #[test]
    fn build_registration_from_container() {
        let container = test_container();
        let reg = build_registration(&container);
        assert_eq!(reg.id, "abc123def456");
        assert_eq!(reg.name, "web-server");
        assert_eq!(reg.agent_type, "container");
        assert!(reg.capabilities.contains(&"stiva".to_string()));
    }

    #[test]
    fn build_registration_no_name() {
        let mut container = test_container();
        container.name = None;
        let reg = build_registration(&container);
        assert_eq!(reg.name, "abc123def456"); // Falls back to first 12 of ID.
    }

    #[test]
    fn registration_serde() {
        let reg = AgentRegistration {
            id: "c1".into(),
            name: "web".into(),
            agent_type: "container".into(),
            capabilities: vec!["stiva".into()],
            metadata: serde_json::json!({}),
        };
        let json = serde_json::to_string(&reg).unwrap();
        let back: AgentRegistration = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, "c1");
    }

    #[test]
    fn agent_status_serde() {
        let status = AgentStatus {
            id: "c1".into(),
            status: "running".into(),
            metadata: Some(serde_json::json!({"exit_code": 0})),
        };
        let json = serde_json::to_string(&status).unwrap();
        let back: AgentStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(back.status, "running");
    }

    // HTTP tests would use wiremock — same pattern as registry tests.
    // Deferred to integration test suite since the pattern is identical.

    #[tokio::test]
    async fn register_with_mock() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/agents/register"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        let client = reqwest::Client::new();
        let container = test_container();
        register_container(&client, &container, &server.uri())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn deregister_with_mock() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;

        Mock::given(method("DELETE"))
            .and(path("/v1/agents/abc123def456"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        let client = reqwest::Client::new();
        deregister_container(&client, "abc123def456", &server.uri())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn report_status_with_mock() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;

        Mock::given(method("PUT"))
            .and(path("/v1/agents/abc123/status"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        let client = reqwest::Client::new();
        report_status(&client, "abc123", "running", &server.uri())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn register_failure() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/agents/register"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&server)
            .await;

        let client = reqwest::Client::new();
        let container = test_container();
        assert!(
            register_container(&client, &container, &server.uri())
                .await
                .is_err()
        );
    }
}
