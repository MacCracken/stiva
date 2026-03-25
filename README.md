# Stiva

> **Stiva** (Romanian: stivă — stack, pile) — OCI-compatible container runtime for AGNOS

[![License: GPL-3.0](https://img.shields.io/badge/License-GPL--3.0-blue.svg)](LICENSE)

Stiva is a full-featured container runtime built on [kavach](https://github.com/MacCracken/kavach) (sandbox isolation), [majra](https://github.com/MacCracken/majra) (scheduling/pub-sub), and [nein](https://github.com/MacCracken/nein) (nftables networking).

## Quick Start

### Library

```rust
use stiva::{Stiva, StivaConfig};
use stiva::container::ContainerConfig;

let stiva = Stiva::new(StivaConfig::default()).await?;

// Pull and run
let container = stiva.run("nginx:latest", ContainerConfig {
    ports: vec!["8080:80".into()],
    detach: true,
    ..Default::default()
}).await?;

// Manage
stiva.exec(&container.id, &["ls".into(), "/etc/nginx".into()]).await?;
stiva.stop(&container.id).await?;
stiva.rm(&container.id).await?;
```

### CLI

```bash
stiva pull nginx:latest
stiva run -d -p 8080:80 nginx:latest
stiva ps
stiva exec <id> ls /etc/nginx
stiva stop <id>
stiva prune
```

See [docs/cli.md](docs/cli.md) for all 26 commands.

## Features

| Category | Capabilities |
|----------|-------------|
| **Images** | Pull, push, build (Stivafile), tag, inspect, import/export |
| **Containers** | Create, start, stop, restart, exec, signal, pause/unpause, stats, top, logs |
| **Networking** | Bridge (NAT), host, custom networks, port mapping, DNS injection |
| **Storage** | Overlay FS, volume mounts, layer dedup, cgroups v2 enforcement |
| **Orchestration** | TOML compose, DAG ordering, health checks, restart policies, fleet scheduling |
| **Security** | Rootless containers, seccomp, Landlock, capability dropping, CRIU checkpoints |
| **Integration** | 9 MCP tools, daimon agent registration, lifecycle events (pub/sub), persistent state |

## Feature Flags

| Feature | Description |
|---------|-------------|
| `full` | All features (default) |
| `runtime` | Container lifecycle (implies `image` + `registry`) |
| `image` | OCI image pull and storage |
| `network` | Container networking |
| `compose` | TOML-based multi-container orchestration (implies `runtime`) |
| `registry` | OCI registry client |
| `encrypted` | LUKS + dm-verity encrypted storage |

## Documentation

| Document | Description |
|----------|-------------|
| [ADRs](docs/adr/) | Architecture decision records (8 decisions) |
| [Architecture](docs/architecture.md) | System design, module map, k8s comparison |
| [CLI Reference](docs/cli.md) | All 26 commands with examples |
| [Testing Guide](docs/development/testing.md) | Test organization, coverage, mocking |
| [Scripts](docs/development/scripts.md) | Benchmark and version scripts |
| [Changelog](CHANGELOG.md) | Release history (phases 0–10) |
| [Contributing](CONTRIBUTING.md) | Contribution guidelines |
| [Security](SECURITY.md) | Security policy |

## Development

```bash
cargo test --all-features        # 404 tests
cargo bench --bench benchmarks   # 18 criterion benchmarks
cargo clippy --all-features --all-targets -- -D warnings
./scripts/bench-history.sh       # Benchmark trend tracking
```

## License

GPL-3.0 — see [LICENSE](LICENSE).
