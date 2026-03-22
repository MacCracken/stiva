//! OCI runtime execution — bridges to kavach for process isolation.

use crate::container::Container;
use crate::error::StivaError;

/// OCI runtime configuration generated from container config.
pub struct RuntimeSpec {
    pub rootfs: std::path::PathBuf,
    pub command: Vec<String>,
    pub env: Vec<String>,
    pub namespaces: Vec<Namespace>,
}

/// Linux namespaces for container isolation.
#[derive(Debug, Clone, Copy)]
pub enum Namespace {
    Pid,
    Net,
    Mount,
    Uts,
    Ipc,
    User,
    Cgroup,
}

/// Generate an OCI runtime spec from a container config.
pub fn generate_spec(
    container: &Container,
    rootfs: &std::path::Path,
) -> Result<RuntimeSpec, StivaError> {
    let env: Vec<String> = container
        .config
        .env
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect();

    Ok(RuntimeSpec {
        rootfs: rootfs.to_path_buf(),
        command: if container.config.command.is_empty() {
            vec!["/bin/sh".to_string()]
        } else {
            container.config.command.clone()
        },
        env,
        namespaces: vec![
            Namespace::Pid,
            Namespace::Net,
            Namespace::Mount,
            Namespace::Uts,
            Namespace::Ipc,
        ],
    })
}

/// Execute a container using kavach sandbox.
pub async fn exec_container(_spec: &RuntimeSpec) -> Result<u32, StivaError> {
    // TODO: Convert RuntimeSpec -> kavach::SandboxConfig
    // TODO: Create kavach::Sandbox with OCI backend
    // TODO: Exec and return PID
    Err(StivaError::Runtime("runtime execution not yet implemented".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::container::ContainerConfig;

    #[test]
    fn generate_spec_defaults() {
        let container = Container {
            id: "test".to_string(),
            name: None,
            image_id: "img".to_string(),
            image_ref: "alpine:latest".to_string(),
            state: crate::container::ContainerState::Created,
            pid: None,
            created_at: chrono::Utc::now(),
            started_at: None,
            config: ContainerConfig::default(),
        };

        let spec = generate_spec(&container, std::path::Path::new("/tmp/rootfs")).unwrap();
        assert_eq!(spec.command, vec!["/bin/sh"]);
        assert_eq!(spec.namespaces.len(), 5);
    }
}
