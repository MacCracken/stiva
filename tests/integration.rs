//! Integration tests for stiva — exercises the full API.

use std::sync::Arc;
use stiva::container::ContainerManager;
use stiva::container::{ContainerConfig, ContainerState};
use stiva::image::{Image, ImageRef, ImageStore};

fn test_image() -> Image {
    Image {
        id: "test-image".into(),
        reference: ImageRef {
            registry: "docker.io".into(),
            repository: "library/alpine".into(),
            tag: "latest".into(),
            digest: None,
        },
        size_bytes: 0,
        layers: vec![],
        created_at: chrono::Utc::now(),
    }
}

fn test_digest(data: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let hash = Sha256::digest(data);
    format!("sha256:{}", hex::encode(hash))
}

#[tokio::test]
async fn container_full_lifecycle() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(ImageStore::new(&dir.path().join("images")).unwrap());
    let manager =
        ContainerManager::new(&dir.path().join("containers"), Arc::clone(&store)).unwrap();

    // Create.
    let c = manager
        .create(&test_image(), ContainerConfig::default())
        .await
        .unwrap();
    assert_eq!(c.state, ContainerState::Created);

    // Start (one-shot).
    let _ = manager.start(&c.id).await;
    let listed = manager.list().await.unwrap();
    assert_eq!(listed[0].state, ContainerState::Stopped);

    // Logs.
    let _ = manager.logs(&c.id).await;

    // Remove.
    manager.remove(&c.id).await.unwrap();
    assert!(manager.list().await.unwrap().is_empty());
}

#[tokio::test]
async fn daemon_container_lifecycle() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(ImageStore::new(&dir.path().join("images")).unwrap());
    let manager =
        ContainerManager::new(&dir.path().join("containers"), Arc::clone(&store)).unwrap();

    let config = ContainerConfig {
        detach: true,
        command: vec!["sleep".into(), "0.05".into()],
        ..Default::default()
    };

    let c = manager.create(&test_image(), config).await.unwrap();
    let _ = manager.start(&c.id).await;

    // Container may be Running or Stopped depending on timing.
    let listed = manager.list().await.unwrap();
    assert_eq!(listed.len(), 1);

    // Stop (safe whether running or stopped).
    let _ = manager.stop(&c.id).await;
    manager.remove(&c.id).await.unwrap();
}

#[tokio::test]
async fn state_persists_across_manager_instances() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("containers");
    let store = Arc::new(ImageStore::new(&dir.path().join("images")).unwrap());

    // Create and start in first manager.
    {
        let manager = ContainerManager::new(&root, Arc::clone(&store)).unwrap();
        let c = manager
            .create(&test_image(), ContainerConfig::default())
            .await
            .unwrap();
        let _ = manager.start(&c.id).await;
    }

    // Restore in new manager.
    {
        let manager = ContainerManager::new(&root, Arc::clone(&store)).unwrap();
        let listed = manager.list().await.unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].state, ContainerState::Stopped);
    }
}

#[tokio::test]
async fn image_store_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let store = ImageStore::new(dir.path()).unwrap();

    let data = b"test blob content";
    let digest = test_digest(data);

    store.store_blob(&digest, data).unwrap();
    assert!(store.has_blob(&digest));

    let read_back = store.read_blob(&digest).unwrap();
    assert_eq!(read_back, data);
}

#[tokio::test]
async fn image_tag_and_rmi() {
    let dir = tempfile::tempdir().unwrap();
    let store = ImageStore::new(dir.path()).unwrap();

    let data = b"image data";
    let digest = test_digest(data);
    store.store_blob(&digest, data).unwrap();

    let image = Image {
        id: digest.clone(),
        reference: ImageRef::parse("nginx:latest").unwrap(),
        size_bytes: data.len() as u64,
        layers: vec![],
        created_at: chrono::Utc::now(),
    };
    store.save_index_pub(std::slice::from_ref(&image)).unwrap();
    assert_eq!(store.list().unwrap().len(), 1);

    // Tag creates second entry.
    let tagged = Image {
        reference: ImageRef::parse("myapp:v2").unwrap(),
        ..image.clone()
    };
    store
        .save_index_pub(&[image.clone(), tagged.clone()])
        .unwrap();
    assert_eq!(store.list().unwrap().len(), 2);

    // Remove by ID removes all.
    store.remove(&digest).unwrap();
    assert!(store.list().unwrap().is_empty());
}

#[tokio::test]
async fn export_import_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let store = ImageStore::new(&dir.path().join("store")).unwrap();

    let rootfs = dir.path().join("rootfs");
    std::fs::create_dir_all(rootfs.join("bin")).unwrap();
    std::fs::write(rootfs.join("bin/hello"), "#!/bin/sh\necho hello").unwrap();

    let tar_path = dir.path().join("export.tar");
    stiva::runtime::export_rootfs(&rootfs, &tar_path)
        .await
        .unwrap();
    assert!(tar_path.exists());

    let image = stiva::runtime::import_rootfs(&tar_path, &store, "roundtrip", "v1").unwrap();
    assert_eq!(image.reference.repository, "roundtrip");
    assert_eq!(image.layers.len(), 1);
    assert_eq!(store.list().unwrap().len(), 1);
}

#[test]
fn build_spec_parsing() {
    let spec = r#"
[image]
base = "alpine:3.19"
name = "myapp"
tag = "v1"

[[steps]]
type = "env"
key = "PORT"
value = "8080"

[config]
entrypoint = ["/start.sh"]
"#;
    let parsed = stiva::build::parse_build_spec(spec).unwrap();
    assert_eq!(parsed.image.name, "myapp");
    assert_eq!(parsed.steps.len(), 1);
    assert_eq!(parsed.config.entrypoint, vec!["/start.sh"]);
}

#[test]
fn fleet_schedule_spread() {
    use stiva::fleet::*;

    let nodes = vec![
        FleetNode {
            id: "a".into(),
            address: "10.0.0.1:8080".into(),
            labels: std::collections::HashMap::new(),
            capacity: NodeCapacity {
                memory_mb: 4096,
                cpus: 4,
                max_containers: 10,
                running_containers: 2,
            },
            status: NodeStatus::Ready,
            last_seen: chrono::Utc::now(),
        },
        FleetNode {
            id: "b".into(),
            address: "10.0.0.2:8080".into(),
            labels: std::collections::HashMap::new(),
            capacity: NodeCapacity {
                memory_mb: 4096,
                cpus: 4,
                max_containers: 10,
                running_containers: 5,
            },
            status: NodeStatus::Ready,
            last_seen: chrono::Utc::now(),
        },
    ];

    let deployment = FleetDeployment {
        id: "test".into(),
        image: "nginx".into(),
        config: ContainerConfig::default(),
        constraints: DeploymentConstraints::default(),
        replicas: 4,
        strategy: DeploymentStrategy::Spread,
        created_at: chrono::Utc::now(),
    };

    let result = schedule(&deployment, &nodes).unwrap();
    let total: u32 = result.assignments.values().sum();
    assert_eq!(total, 4);
}

#[test]
fn copy_into_and_out_of_container() {
    let dir = tempfile::tempdir().unwrap();
    let rootfs = dir.path().join("rootfs");
    std::fs::create_dir_all(&rootfs).unwrap();

    // Copy in.
    let src = dir.path().join("input.txt");
    std::fs::write(&src, "hello container").unwrap();
    stiva::runtime::copy_into_container(&rootfs, &src, std::path::Path::new("/data/input.txt"))
        .unwrap();
    assert!(rootfs.join("data/input.txt").exists());

    // Copy out.
    let dst = dir.path().join("output.txt");
    stiva::runtime::copy_from_container(&rootfs, std::path::Path::new("/data/input.txt"), &dst)
        .unwrap();
    assert_eq!(std::fs::read_to_string(&dst).unwrap(), "hello container");
}

#[tokio::test]
async fn restart_stopped_container() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(ImageStore::new(&dir.path().join("images")).unwrap());
    let manager =
        ContainerManager::new(&dir.path().join("containers"), Arc::clone(&store)).unwrap();

    let c = manager
        .create(&test_image(), ContainerConfig::default())
        .await
        .unwrap();
    let _ = manager.start(&c.id).await;

    // Should be stopped after one-shot.
    let listed = manager.list().await.unwrap();
    assert_eq!(listed[0].state, ContainerState::Stopped);

    // Restart.
    let _ = manager.restart(&c.id).await;

    // Should be stopped again after one-shot re-exec.
    let listed = manager.list().await.unwrap();
    assert_eq!(listed[0].state, ContainerState::Stopped);
}
