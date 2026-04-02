//! Multi-container orchestration — ansamblu — multi-service orchestration.
//!
//! Parses TOML ansamblu files, resolves service dependency ordering via
//! majra's DAG scheduler, and orchestrates container lifecycle.

use crate::container::ContainerConfig;
use crate::error::StivaError;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use tracing::info;

// ---------------------------------------------------------------------------
// Ansamblu file types
// ---------------------------------------------------------------------------

/// A ansamblu file definition (TOML-based, not YAML).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnsambluFile {
    pub services: HashMap<String, ServiceDef>,
    #[serde(default)]
    pub networks: HashMap<String, NetworkDef>,
    #[serde(default)]
    pub volumes: HashMap<String, VolumeDef>,
}

/// A service definition within an ansamblu file.
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
    #[serde(default)]
    pub restart: Option<RestartPolicy>,
    #[serde(default)]
    pub health_check: Option<HealthCheck>,
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

// Re-export RestartPolicy from health module (shared between ansamblu and health).
pub use crate::health::RestartPolicy;

// ---------------------------------------------------------------------------
// Health checks
// ---------------------------------------------------------------------------

/// Container health check configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheck {
    /// Command to run inside the container to check health.
    pub command: Vec<String>,
    /// Interval between checks in seconds.
    #[serde(default = "default_interval")]
    pub interval_secs: u64,
    /// Timeout for each check in seconds.
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
    /// Number of consecutive failures before unhealthy.
    #[serde(default = "default_retries")]
    pub retries: u32,
}

fn default_interval() -> u64 {
    30
}
fn default_timeout() -> u64 {
    5
}
fn default_retries() -> u32 {
    3
}

// ---------------------------------------------------------------------------
// Ansamblu session
// ---------------------------------------------------------------------------

/// A running ansamblu session tracking deployed services.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnsambluSession {
    /// Unique session ID.
    pub id: String,
    /// Service name → container IDs (multiple if replicas > 1).
    pub services: HashMap<String, Vec<String>>,
    /// Networks created for this session.
    pub networks: Vec<String>,
    /// Ordered list of service names (startup order from DAG sort).
    pub startup_order: Vec<String>,
    /// When the session was created.
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Rolling update configuration for a service.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollingUpdateConfig {
    /// Maximum number of replicas that can be created above the desired count.
    #[serde(default = "default_max_surge")]
    pub max_surge: u32,
    /// Maximum number of replicas that can be unavailable during update.
    #[serde(default = "default_max_unavailable")]
    pub max_unavailable: u32,
    /// Delay between replacing replicas (seconds).
    #[serde(default = "default_update_delay")]
    pub delay_secs: u64,
}

fn default_max_surge() -> u32 {
    1
}
fn default_max_unavailable() -> u32 {
    1
}
fn default_update_delay() -> u64 {
    10
}

impl Default for RollingUpdateConfig {
    fn default() -> Self {
        Self {
            max_surge: 1,
            max_unavailable: 1,
            delay_secs: 10,
        }
    }
}

/// Plan for a rolling update — describes which replicas to create/destroy.
#[derive(Debug, Clone)]
pub struct RollingUpdatePlan {
    /// Service being updated.
    pub service_name: String,
    /// Old container IDs to remove (in order).
    pub old_containers: Vec<String>,
    /// Number of new replicas to create.
    pub new_replica_count: u32,
    /// New image reference.
    pub new_image: String,
    /// Rolling update settings.
    pub config: RollingUpdateConfig,
}

/// Plan a rolling update for a service.
///
/// Compares the current session state with a new service definition
/// and produces a plan describing which containers to replace.
#[must_use = "rolling update plan should be executed"]
pub fn plan_rolling_update(
    session: &AnsambluSession,
    service_name: &str,
    new_service: &ServiceDef,
) -> Result<RollingUpdatePlan, StivaError> {
    let old_containers = session
        .services
        .get(service_name)
        .cloned()
        .unwrap_or_default();

    let new_count = replica_count(new_service);

    Ok(RollingUpdatePlan {
        service_name: service_name.to_string(),
        old_containers,
        new_replica_count: new_count,
        new_image: new_service.image.clone(),
        config: RollingUpdateConfig::default(),
    })
}

/// Compute scale actions for a service.
///
/// Returns `(to_add, to_remove)` — number of replicas to create/destroy.
#[must_use]
pub fn compute_scale(
    session: &AnsambluSession,
    service_name: &str,
    desired: u32,
) -> (u32, Vec<String>) {
    let current = session
        .services
        .get(service_name)
        .map(|ids| ids.len() as u32)
        .unwrap_or(0);

    if desired > current {
        (desired - current, vec![])
    } else if desired < current {
        let remove_count = (current - desired) as usize;
        let to_remove: Vec<String> = session
            .services
            .get(service_name)
            .map(|ids| ids.iter().rev().take(remove_count).cloned().collect())
            .unwrap_or_default();
        (0, to_remove)
    } else {
        (0, vec![])
    }
}

/// Aggregate logs from all replicas of a service.
#[must_use]
pub fn service_container_ids(session: &AnsambluSession, service_name: &str) -> Vec<String> {
    session
        .services
        .get(service_name)
        .cloned()
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

/// Parse a ansamblu file from TOML.
#[must_use = "parsing returns a new AnsambluFile"]
pub fn parse_ansamblu(toml_str: &str) -> Result<AnsambluFile, StivaError> {
    toml::from_str(toml_str)
        .map_err(|e| StivaError::Ansamblu(format!("invalid ansamblu file: {e}")))
}

// ---------------------------------------------------------------------------
// DAG dependency resolution
// ---------------------------------------------------------------------------

/// Build a majra DAG from ansamblu service dependencies.
#[must_use]
pub fn build_dag(spec: &AnsambluFile) -> majra::queue::Dag {
    let mut edges = HashMap::new();
    for (name, service) in &spec.services {
        edges.insert(name.clone(), service.depends_on.clone());
    }
    majra::queue::Dag { edges }
}

/// Resolve service startup order via topological sort.
pub fn resolve_startup_order(spec: &AnsambluFile) -> Result<Vec<String>, StivaError> {
    let dag = build_dag(spec);
    let order = majra::queue::DagScheduler::topological_sort(&dag)
        .map_err(|e| StivaError::Ansamblu(format!("dependency cycle detected: {e}")))?;
    info!(
        services = order.len(),
        order = ?order,
        "resolved service startup order"
    );
    Ok(order)
}

// ---------------------------------------------------------------------------
// ServiceDef → ContainerConfig conversion
// ---------------------------------------------------------------------------

/// Convert a ServiceDef to a ContainerConfig for a specific replica.
#[must_use]
pub fn service_to_config(
    service_name: &str,
    service: &ServiceDef,
    replica_index: u32,
) -> ContainerConfig {
    let name = if service.replicas.unwrap_or(1) > 1 {
        format!("{service_name}-{replica_index}")
    } else {
        service_name.to_string()
    };

    ContainerConfig {
        name: Some(name),
        command: service.command.clone(),
        env: service.env.clone(),
        ports: service.ports.clone(),
        volumes: service.volumes.clone(),
        ..Default::default()
    }
}

/// Get the number of replicas for a service (default 1).
#[inline]
#[must_use]
pub fn replica_count(service: &ServiceDef) -> u32 {
    service.replicas.unwrap_or(1).max(1)
}

// ---------------------------------------------------------------------------
// Readiness tracking
// ---------------------------------------------------------------------------

/// Check which services from a DAG are ready to start given completed services.
#[must_use]
pub fn ready_services(spec: &AnsambluFile, completed: &HashSet<String>) -> Vec<String> {
    let dag = build_dag(spec);
    match majra::queue::DagScheduler::new(&dag) {
        Ok(scheduler) => scheduler.ready(completed),
        Err(_) => vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_ansamblu_toml() {
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
        let spec = parse_ansamblu(toml).unwrap();
        assert_eq!(spec.services.len(), 3);
        assert_eq!(spec.services["api"].depends_on, vec!["db"]);
        assert!(spec.volumes.contains_key("pgdata"));
    }

    #[test]
    fn parse_ansamblu_invalid_toml() {
        assert!(parse_ansamblu("not a [valid toml").is_err());
    }

    #[test]
    fn parse_ansamblu_missing_services() {
        assert!(parse_ansamblu("[networks.default]").is_err());
    }

    #[test]
    fn parse_ansamblu_minimal() {
        let toml = r#"
[services.app]
image = "alpine"
"#;
        let spec = parse_ansamblu(toml).unwrap();
        assert_eq!(spec.services.len(), 1);
        assert!(spec.services["app"].command.is_empty());
        assert!(spec.services["app"].depends_on.is_empty());
        assert!(spec.networks.is_empty());
        assert!(spec.volumes.is_empty());
    }

    #[test]
    fn parse_ansamblu_with_networks() {
        let toml = r#"
[services.app]
image = "alpine"

[networks.frontend]
driver = "bridge"
subnet = "10.0.0.0/24"
"#;
        let spec = parse_ansamblu(toml).unwrap();
        assert_eq!(spec.networks.len(), 1);
        assert_eq!(spec.networks["frontend"].driver.as_deref(), Some("bridge"));
    }

    #[test]
    fn parse_ansamblu_replicas() {
        let toml = r#"
[services.worker]
image = "worker:latest"
replicas = 3
"#;
        let spec = parse_ansamblu(toml).unwrap();
        assert_eq!(spec.services["worker"].replicas, Some(3));
    }

    #[test]
    fn parse_ansamblu_empty_string() {
        assert!(parse_ansamblu("").is_err());
    }

    #[test]
    fn parse_ansamblu_service_with_command() {
        let toml = r#"
[services.app]
image = "alpine"
command = ["/bin/sh", "-c", "echo hello"]
"#;
        let spec = parse_ansamblu(toml).unwrap();
        assert_eq!(
            spec.services["app"].command,
            vec!["/bin/sh", "-c", "echo hello"]
        );
    }

    #[test]
    fn parse_ansamblu_dependency_chain() {
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
        let spec = parse_ansamblu(toml).unwrap();
        assert_eq!(spec.services.len(), 4);
        assert_eq!(spec.services["frontend"].depends_on, vec!["api"]);
    }

    #[test]
    fn parse_ansamblu_service_all_fields() {
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
        let spec = parse_ansamblu(toml).unwrap();
        let svc = &spec.services["app"];
        assert_eq!(svc.image, "myapp:latest");
        assert_eq!(svc.replicas, Some(2));
    }

    #[test]
    fn parse_ansamblu_with_restart_policy() {
        let toml = r#"
[services.app]
image = "myapp"
restart = "always"
"#;
        let spec = parse_ansamblu(toml).unwrap();
        assert_eq!(spec.services["app"].restart, Some(RestartPolicy::Always));
    }

    #[test]
    fn parse_ansamblu_with_health_check() {
        let toml = r#"
[services.app]
image = "myapp"

[services.app.health_check]
command = ["curl", "-f", "http://localhost/health"]
interval_secs = 10
timeout_secs = 3
retries = 5
"#;
        let spec = parse_ansamblu(toml).unwrap();
        let hc = spec.services["app"].health_check.as_ref().unwrap();
        assert_eq!(hc.command, vec!["curl", "-f", "http://localhost/health"]);
        assert_eq!(hc.interval_secs, 10);
        assert_eq!(hc.retries, 5);
    }

    #[test]
    fn health_check_defaults() {
        let toml = r#"
[services.app]
image = "myapp"

[services.app.health_check]
command = ["true"]
"#;
        let spec = parse_ansamblu(toml).unwrap();
        let hc = spec.services["app"].health_check.as_ref().unwrap();
        assert_eq!(hc.interval_secs, 30);
        assert_eq!(hc.timeout_secs, 5);
        assert_eq!(hc.retries, 3);
    }

    // -- DAG resolution --

    #[test]
    fn resolve_startup_order_linear() {
        let toml = r#"
[services.frontend]
image = "nginx"
depends_on = ["api"]

[services.api]
image = "app"
depends_on = ["db"]

[services.db]
image = "postgres"
"#;
        let spec = parse_ansamblu(toml).unwrap();
        let order = resolve_startup_order(&spec).unwrap();
        let db_pos = order.iter().position(|s| s == "db").unwrap();
        let api_pos = order.iter().position(|s| s == "api").unwrap();
        let fe_pos = order.iter().position(|s| s == "frontend").unwrap();
        assert!(db_pos < api_pos);
        assert!(api_pos < fe_pos);
    }

    #[test]
    fn resolve_startup_order_no_deps() {
        let toml = r#"
[services.a]
image = "a"
[services.b]
image = "b"
[services.c]
image = "c"
"#;
        let spec = parse_ansamblu(toml).unwrap();
        let order = resolve_startup_order(&spec).unwrap();
        assert_eq!(order.len(), 3);
    }

    #[test]
    fn resolve_startup_order_diamond() {
        let toml = r#"
[services.web]
image = "web"
depends_on = ["api", "worker"]
[services.api]
image = "api"
depends_on = ["db"]
[services.worker]
image = "worker"
depends_on = ["db"]
[services.db]
image = "db"
"#;
        let spec = parse_ansamblu(toml).unwrap();
        let order = resolve_startup_order(&spec).unwrap();
        let db_pos = order.iter().position(|s| s == "db").unwrap();
        let web_pos = order.iter().position(|s| s == "web").unwrap();
        assert!(db_pos < web_pos);
    }

    // -- ServiceDef → ContainerConfig --

    #[test]
    fn service_to_config_basic() {
        let service = ServiceDef {
            image: "nginx".into(),
            command: vec![],
            env: HashMap::from([("PORT".into(), "80".into())]),
            ports: vec!["8080:80".into()],
            volumes: vec![],
            depends_on: vec![],
            replicas: None,
            restart: None,
            health_check: None,
        };
        let config = service_to_config("web", &service, 0);
        assert_eq!(config.name, Some("web".into()));
        assert_eq!(config.env["PORT"], "80");
        assert_eq!(config.ports, vec!["8080:80"]);
    }

    #[test]
    fn service_to_config_with_replicas() {
        let service = ServiceDef {
            image: "worker".into(),
            command: vec![],
            env: HashMap::new(),
            ports: vec![],
            volumes: vec![],
            depends_on: vec![],
            replicas: Some(3),
            restart: None,
            health_check: None,
        };
        let c0 = service_to_config("worker", &service, 0);
        let c1 = service_to_config("worker", &service, 1);
        let c2 = service_to_config("worker", &service, 2);
        assert_eq!(c0.name, Some("worker-0".into()));
        assert_eq!(c1.name, Some("worker-1".into()));
        assert_eq!(c2.name, Some("worker-2".into()));
    }

    #[test]
    fn replica_count_default() {
        let service = ServiceDef {
            image: "x".into(),
            command: vec![],
            env: HashMap::new(),
            ports: vec![],
            volumes: vec![],
            depends_on: vec![],
            replicas: None,
            restart: None,
            health_check: None,
        };
        assert_eq!(replica_count(&service), 1);
    }

    #[test]
    fn replica_count_zero_becomes_one() {
        let service = ServiceDef {
            image: "x".into(),
            command: vec![],
            env: HashMap::new(),
            ports: vec![],
            volumes: vec![],
            depends_on: vec![],
            replicas: Some(0),
            restart: None,
            health_check: None,
        };
        assert_eq!(replica_count(&service), 1);
    }

    // -- Ready services --

    #[test]
    fn ready_services_initial() {
        let toml = r#"
[services.db]
image = "postgres"
[services.api]
image = "app"
depends_on = ["db"]
"#;
        let spec = parse_ansamblu(toml).unwrap();
        let ready = ready_services(&spec, &HashSet::new());
        assert!(ready.contains(&"db".to_string()));
        assert!(!ready.contains(&"api".to_string()));
    }

    #[test]
    fn ready_services_after_db() {
        let toml = r#"
[services.db]
image = "postgres"
[services.api]
image = "app"
depends_on = ["db"]
"#;
        let spec = parse_ansamblu(toml).unwrap();
        let completed = HashSet::from(["db".to_string()]);
        let ready = ready_services(&spec, &completed);
        assert!(ready.contains(&"api".to_string()));
    }

    // -- Serde --

    #[test]
    fn restart_policy_serde() {
        for policy in [
            RestartPolicy::Never,
            RestartPolicy::Always,
            RestartPolicy::OnFailure { max_retries: 5 },
            RestartPolicy::UnlessStopped,
        ] {
            let json = serde_json::to_string(&policy).unwrap();
            let back: RestartPolicy = serde_json::from_str(&json).unwrap();
            assert_eq!(policy, back);
        }
    }

    #[test]
    fn restart_policy_default() {
        assert_eq!(RestartPolicy::default(), RestartPolicy::Never);
    }

    #[test]
    fn ansamblu_session_serde() {
        let session = AnsambluSession {
            id: "sess-123".into(),
            services: HashMap::from([("web".into(), vec!["c1".into()])]),
            networks: vec!["mynet".into()],
            startup_order: vec!["db".into(), "web".into()],
            created_at: chrono::Utc::now(),
        };
        let json = serde_json::to_string(&session).unwrap();
        let back: AnsambluSession = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, "sess-123");
        assert_eq!(back.startup_order, vec!["db", "web"]);
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
    }

    #[test]
    fn volume_def_default() {
        let vol = VolumeDef::default();
        assert!(vol.driver.is_none());
    }

    #[test]
    fn ansamblu_file_serde_round_trip() {
        let spec = AnsambluFile {
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
                    restart: None,
                    health_check: None,
                },
            )]),
            networks: HashMap::new(),
            volumes: HashMap::new(),
        };
        let json = serde_json::to_string(&spec).unwrap();
        let back: AnsambluFile = serde_json::from_str(&json).unwrap();
        assert_eq!(back.services.len(), 1);
        assert_eq!(back.services["web"].image, "nginx");
    }
}
