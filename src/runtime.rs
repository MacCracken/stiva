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
    Err(StivaError::Runtime(
        "runtime execution not yet implemented".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::container::ContainerConfig;

    fn test_container(config: ContainerConfig) -> Container {
        Container {
            id: "test".to_string(),
            name: None,
            image_id: "img".to_string(),
            image_ref: "alpine:latest".to_string(),
            state: crate::container::ContainerState::Created,
            pid: None,
            created_at: chrono::Utc::now(),
            started_at: None,
            config,
        }
    }

    #[test]
    fn generate_spec_defaults() {
        let container = test_container(ContainerConfig::default());
        let spec = generate_spec(&container, std::path::Path::new("/tmp/rootfs")).unwrap();
        assert_eq!(spec.command, vec!["/bin/sh"]);
        assert_eq!(spec.namespaces.len(), 5);
        assert_eq!(spec.rootfs, std::path::PathBuf::from("/tmp/rootfs"));
        assert!(spec.env.is_empty());
    }

    #[test]
    fn generate_spec_with_command() {
        let config = ContainerConfig {
            command: vec!["nginx".into(), "-g".into(), "daemon off;".into()],
            ..Default::default()
        };
        let container = test_container(config);
        let spec = generate_spec(&container, std::path::Path::new("/rootfs")).unwrap();
        assert_eq!(spec.command, vec!["nginx", "-g", "daemon off;"]);
    }

    #[test]
    fn generate_spec_with_env() {
        let mut env = std::collections::HashMap::new();
        env.insert("PORT".to_string(), "8080".to_string());
        env.insert("DEBUG".to_string(), "1".to_string());
        let config = ContainerConfig {
            env,
            ..Default::default()
        };
        let container = test_container(config);
        let spec = generate_spec(&container, std::path::Path::new("/rootfs")).unwrap();
        assert_eq!(spec.env.len(), 2);
        assert!(spec.env.contains(&"PORT=8080".to_string()));
        assert!(spec.env.contains(&"DEBUG=1".to_string()));
    }

    #[tokio::test]
    async fn exec_container_not_implemented() {
        let container = test_container(ContainerConfig::default());
        let spec = generate_spec(&container, std::path::Path::new("/rootfs")).unwrap();
        let err = exec_container(&spec).await.unwrap_err();
        assert!(matches!(err, crate::StivaError::Runtime(_)));
    }

    #[test]
    fn namespace_debug() {
        let ns = Namespace::Pid;
        let dbg = format!("{ns:?}");
        assert_eq!(dbg, "Pid");

        // Cover all variants via Debug.
        for ns in [
            Namespace::Pid,
            Namespace::Net,
            Namespace::Mount,
            Namespace::Uts,
            Namespace::Ipc,
            Namespace::User,
            Namespace::Cgroup,
        ] {
            let _ = format!("{ns:?}");
        }
    }

    #[test]
    fn namespaces_are_correct() {
        let container = test_container(ContainerConfig::default());
        let spec = generate_spec(&container, std::path::Path::new("/rootfs")).unwrap();
        assert!(matches!(spec.namespaces[0], Namespace::Pid));
        assert!(matches!(spec.namespaces[1], Namespace::Net));
        assert!(matches!(spec.namespaces[2], Namespace::Mount));
        assert!(matches!(spec.namespaces[3], Namespace::Uts));
        assert!(matches!(spec.namespaces[4], Namespace::Ipc));
    }
}
