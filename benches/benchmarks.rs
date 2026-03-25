//! Criterion benchmarks for stiva hot paths.

use criterion::{Criterion, criterion_group, criterion_main};

// ---------------------------------------------------------------------------
// Image reference parsing
// ---------------------------------------------------------------------------

fn bench_imageref_parse(c: &mut Criterion) {
    let mut group = c.benchmark_group("imageref");

    group.bench_function("simple", |b| {
        b.iter(|| stiva::image::ImageRef::parse("nginx").unwrap());
    });

    group.bench_function("tagged", |b| {
        b.iter(|| stiva::image::ImageRef::parse("nginx:1.25").unwrap());
    });

    group.bench_function("full_registry", |b| {
        b.iter(|| stiva::image::ImageRef::parse("ghcr.io/maccracken/agnosticos:latest").unwrap());
    });

    group.bench_function("with_port", |b| {
        b.iter(|| stiva::image::ImageRef::parse("localhost:5000/myapp:v1").unwrap());
    });

    group.bench_function("digest", |b| {
        b.iter(|| {
            stiva::image::ImageRef::parse("nginx@sha256:abcdef1234567890abcdef1234567890").unwrap()
        });
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// Volume parsing
// ---------------------------------------------------------------------------

fn bench_volume_parse(c: &mut Criterion) {
    let mut group = c.benchmark_group("volume");

    group.bench_function("rw", |b| {
        b.iter(|| stiva::storage::parse_volume("/data:/mnt/data").unwrap());
    });

    group.bench_function("ro", |b| {
        b.iter(|| stiva::storage::parse_volume("/config:/etc/config:ro").unwrap());
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// Port spec parsing
// ---------------------------------------------------------------------------

fn bench_port_parse(c: &mut Criterion) {
    let mut group = c.benchmark_group("port");

    group.bench_function("simple", |b| {
        b.iter(|| stiva::network::nat::parse_port_spec("8080:80").unwrap());
    });

    group.bench_function("with_proto", |b| {
        b.iter(|| stiva::network::nat::parse_port_spec("8080:80/tcp").unwrap());
    });

    group.bench_function("with_bind", |b| {
        b.iter(|| stiva::network::nat::parse_port_spec("127.0.0.1:3000:3000/tcp").unwrap());
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// Blob store (SHA-256 + write)
// ---------------------------------------------------------------------------

fn bench_blob_store(c: &mut Criterion) {
    let mut group = c.benchmark_group("blob");

    group.bench_function("store_1kb", |b| {
        let dir = tempfile::tempdir().unwrap();
        let store = stiva::image::ImageStore::new(dir.path()).unwrap();
        let data = vec![0x42u8; 1024];
        let digest = {
            use sha2::{Digest, Sha256};
            let hash = Sha256::digest(&data);
            format!("sha256:{}", hex::encode(hash))
        };

        b.iter(|| {
            // Remove blob so we re-store each iteration (not dedup skip).
            let hex = digest.strip_prefix("sha256:").unwrap();
            let path = dir.path().join("blobs").join("sha256").join(hex);
            let _ = std::fs::remove_file(&path);
            store.store_blob(&digest, &data).unwrap();
        });
    });

    group.bench_function("store_1mb", |b| {
        let dir = tempfile::tempdir().unwrap();
        let store = stiva::image::ImageStore::new(dir.path()).unwrap();
        let data = vec![0x42u8; 1024 * 1024];
        let digest = {
            use sha2::{Digest, Sha256};
            let hash = Sha256::digest(&data);
            format!("sha256:{}", hex::encode(hash))
        };

        b.iter(|| {
            let hex = digest.strip_prefix("sha256:").unwrap();
            let path = dir.path().join("blobs").join("sha256").join(hex);
            let _ = std::fs::remove_file(&path);
            store.store_blob(&digest, &data).unwrap();
        });
    });

    group.bench_function("has_blob", |b| {
        let dir = tempfile::tempdir().unwrap();
        let store = stiva::image::ImageStore::new(dir.path()).unwrap();
        let data = b"hello world";
        let digest = {
            use sha2::{Digest, Sha256};
            let hash = Sha256::digest(data);
            format!("sha256:{}", hex::encode(hash))
        };
        store.store_blob(&digest, data).unwrap();

        b.iter(|| store.has_blob(&digest));
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// IP pool allocation
// ---------------------------------------------------------------------------

fn bench_ip_pool(c: &mut Criterion) {
    let mut group = c.benchmark_group("ippool");

    group.bench_function("allocate", |b| {
        b.iter_batched(
            || stiva::network::pool::IpPool::new("10.0.0.0/24").unwrap(),
            |mut pool| {
                pool.allocate().unwrap();
            },
            criterion::BatchSize::SmallInput,
        );
    });

    group.bench_function("allocate_release_cycle", |b| {
        let mut pool = stiva::network::pool::IpPool::new("10.0.0.0/24").unwrap();
        b.iter(|| {
            let ip = pool.allocate().unwrap();
            pool.release(&ip);
        });
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// Fleet scheduling
// ---------------------------------------------------------------------------

fn bench_fleet_schedule(c: &mut Criterion) {
    use stiva::fleet::*;

    let nodes: Vec<FleetNode> = (0..100)
        .map(|i| FleetNode {
            id: format!("node-{i}"),
            address: format!("10.0.0.{i}:8080"),
            labels: std::collections::HashMap::new(),
            capacity: NodeCapacity {
                memory_mb: 4096,
                cpus: 4,
                max_containers: 20,
                running_containers: i % 10,
            },
            status: NodeStatus::Ready,
            last_seen: chrono::Utc::now(),
        })
        .collect();

    let mut group = c.benchmark_group("fleet");

    group.bench_function("spread_10_replicas", |b| {
        let deployment = FleetDeployment {
            id: "bench".into(),
            image: "nginx".into(),
            config: stiva::container::ContainerConfig::default(),
            constraints: DeploymentConstraints::default(),
            replicas: 10,
            strategy: DeploymentStrategy::Spread,
            created_at: chrono::Utc::now(),
        };
        b.iter(|| schedule(&deployment, &nodes).unwrap());
    });

    group.bench_function("binpack_10_replicas", |b| {
        let deployment = FleetDeployment {
            id: "bench".into(),
            image: "nginx".into(),
            config: stiva::container::ContainerConfig::default(),
            constraints: DeploymentConstraints::default(),
            replicas: 10,
            strategy: DeploymentStrategy::BinPack,
            created_at: chrono::Utc::now(),
        };
        b.iter(|| schedule(&deployment, &nodes).unwrap());
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// Build spec parsing
// ---------------------------------------------------------------------------

fn bench_build_parse(c: &mut Criterion) {
    let spec = r#"
[image]
base = "alpine:3.19"
name = "myapp"
tag = "v1.0"

[[steps]]
type = "run"
command = ["apk", "add", "--no-cache", "curl"]

[[steps]]
type = "copy"
source = "./app"
destination = "/app"

[[steps]]
type = "env"
key = "PORT"
value = "8080"

[[steps]]
type = "workdir"
path = "/app"

[config]
entrypoint = ["/app/start.sh"]
expose = [8080]
user = "nobody"
"#;

    c.bench_function("build/parse_spec", |b| {
        b.iter(|| stiva::build::parse_build_spec(spec).unwrap());
    });
}

criterion_group!(
    benches,
    bench_imageref_parse,
    bench_volume_parse,
    bench_port_parse,
    bench_blob_store,
    bench_ip_pool,
    bench_fleet_schedule,
    bench_build_parse,
);
criterion_main!(benches);
