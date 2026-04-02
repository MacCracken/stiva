# Stiva

> **Stiva** (Romanian: stiva — stack, pile) — OCI-compatible container runtime for AGNOS

[![License: GPL-3.0-or-later](https://img.shields.io/badge/License-GPL--3.0--or--later-blue.svg)](LICENSE)

Stiva is a full-featured container runtime built on [kavach](https://github.com/MacCracken/kavach) (sandbox isolation), [majra](https://github.com/MacCracken/majra) (scheduling/pub-sub), [nein](https://github.com/MacCracken/nein) (nftables networking), and [bote](https://github.com/MacCracken/bote) (MCP integration).

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

See [docs/cli.md](docs/cli.md) for all 34 commands.

## Features

| Category | Capabilities |
|----------|-------------|
| **Images** | Pull, push, build (Stivafile), tag, inspect, import/export, GC, multi-stage builds, build cache |
| **Containers** | Create, start, stop, restart, rename, exec, signal, pause/unpause, stats, top, logs, update, diff |
| **Networking** | Bridge (NAT), host, custom networks, port mapping, DNS injection, IPv6 dual-stack, network policy |
| **Storage** | Overlay FS, volume mounts, layer dedup, cgroups v2 (CPU/mem/PID/IO), zstd + gzip layers |
| **Orchestration** | TOML ansamblu, DAG ordering, health checks, restart policies, fleet scheduling, rolling updates, scaling |
| **Security** | Rootless containers, seccomp, Landlock, NO_NEW_PRIVS, fd cleanup, CRIU checkpoints (pre-dump, lazy pages), credential store |
| **Integration** | 9 MCP tools (structured output, live dispatch), daimon agent, lifecycle events (pub/sub), persistent state, shell completions |

## Feature Flags

| Feature | Description |
|---------|-------------|
| `full` | All features (default) |
| `runtime` | Container lifecycle (implies `image` + `registry`) |
| `image` | OCI image pull and storage |
| `network` | Container networking |
| `ansamblu` | TOML-based multi-container orchestration (implies `runtime`) |
| `registry` | OCI registry client |
| `encrypted` | LUKS + dm-verity encrypted storage |

## Documentation

| Document | Description |
|----------|-------------|
| [ADRs](docs/adr/) | Architecture decision records (11 decisions) |
| [Architecture](docs/architecture.md) | System design, module map, k8s comparison |
| [CLI Reference](docs/cli.md) | All 34 commands with examples |
| [Quick Start](docs/guides/quick-start.md) | Getting started guide |
| [Networking](docs/guides/networking.md) | Network configuration guide |
| [Security](docs/guides/security.md) | Security hardening guide |
| [Testing Guide](docs/development/testing.md) | Test organization, coverage, mocking |
| [Scripts](docs/development/scripts.md) | Benchmark and version scripts |
| [Spec Compliance](docs/spec-compliance.md) | OCI, MCP, CRIU spec conformance |
| [Security Audit Log](docs/security-audit-log.md) | CVE tracking and remediation |
| [Changelog](CHANGELOG.md) | Release history |
| [Contributing](CONTRIBUTING.md) | Contribution guidelines |
| [Security](SECURITY.md) | Security policy |

## Development

```bash
cargo test --all-features        # 434 tests
cargo bench --bench benchmarks   # 20 criterion benchmarks
cargo clippy --all-features --all-targets -- -D warnings
./scripts/bench-history.sh       # Benchmark trend tracking
```

## License

GPL-3.0-or-later — see [LICENSE](LICENSE).
