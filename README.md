# Stiva

> **Stiva** (Romanian: stivă — stack, pile) — OCI-compatible container runtime for AGNOS

[![License: GPL-3.0](https://img.shields.io/badge/License-GPL--3.0-blue.svg)](LICENSE)

Stiva is the container runtime that completes AGNOS's path to a full orchestration platform. It builds on [kavach](https://github.com/MacCracken/kavach) for process isolation (seccomp, Landlock, namespaces, capabilities) and [majra](https://github.com/MacCracken/majra) for scheduling primitives (priority queues, heartbeat FSM, pub/sub).

## Architecture

```
stiva (this crate)
  ├── kavach (sandbox: seccomp, Landlock, namespaces, OCI spec, gVisor, Firecracker, WASM)
  ├── majra (job queue, heartbeat FSM, pub/sub, relay)
  └── nein (nftables firewall, NAT, port mapping)
```

```
                    ┌─────────────────────┐
                    │   Daimon (runtime)   │
                    │   Agent orchestrator │
                    └────────┬────────────┘
                             │
              ┌──────────────▼──────────────┐
              │          Stiva              │
              │   Container lifecycle       │
              │   Image pull/store          │
              │   Overlay FS                │
              │   OCI registry client       │
              │   Compose orchestration     │
              │   Health + restart policies │
              │   MCP tools + agent reg.    │
              ├──────────┬─────────────────┤
              │  kavach  │     majra       │
              │ (sandbox)│  (queue/fleet)  │
              └──────────┴─────────────────┘
```

## Features

- **OCI Image Management** — pull, store, layer deduplication, multi-arch manifests
- **Container Lifecycle** — create, start, stop, kill, remove with full state tracking
- **Kavach Sandbox Backends** — process (seccomp + Landlock), gVisor, Firecracker, WASM, OCI, SGX, SEV
- **Overlay Filesystem** — layer-based rootfs assembly, copy-on-write
- **Container Networking** — bridge (NAT), host, none, container-shared, custom named networks
- **Volume Mounts** — bind mounts, tmpfs, named volumes
- **OCI Registry Client** — Docker Hub, GHCR, custom registries, token auth
- **TOML Compose** — multi-container orchestration using TOML (not YAML)
- **Health Monitoring** — heartbeat-based health tracking via majra FSM
- **Restart Policies** — always, on-failure (with max retries), unless-stopped
- **MCP Tools** — AI agent integration (stiva_pull, stiva_run, stiva_ps, stiva_stop, stiva_compose)
- **Daimon Integration** — register containers as agents for fleet orchestration
- **Sutra Module** — deploy containers via sutra playbooks (`sutra-stiva` crate)

## Usage

```rust
use stiva::{Stiva, StivaConfig};
use stiva::container::ContainerConfig;

let stiva = Stiva::new(StivaConfig::default()).await?;

// Pull an image
let image = stiva.pull("nginx:latest").await?;

// Run a container
let config = ContainerConfig {
    ports: vec!["8080:80".to_string()],
    ..Default::default()
};
let container = stiva.run("nginx:latest", config).await?;

// List, stop, remove
let running = stiva.ps().await?;
stiva.stop(&container.id).await?;
stiva.rm(&container.id).await?;
```

## Feature Flags

| Feature | Description |
|---------|-------------|
| `runtime` | Container lifecycle (default) |
| `image` | OCI image pull and storage |
| `network` | Container networking |
| `compose` | TOML-based multi-container orchestration |
| `registry` | OCI registry client |
| `full` | All features |

## Roadmap

### Phase 5 — Advanced (next)
- [x] Long-running daemon containers (kavach spawn/wait/kill)
- [x] Build from Dockerfile/Containerfile (or TOML equivalent)
- [x] Image push to registries
- [x] Container checkpointing (CRIU integration)
- [x] Live migration between nodes
- [x] Integration with daimon edge fleet (schedule containers across nodes)
- [x] Rootless containers (user namespace remapping)

### Completed

<details>
<summary>Phase 0–4 (click to expand)</summary>

#### Phase 0 — Foundation
Scaffold, image reference parser, container lifecycle state machine, OCI types, volume parsing, network mode types, TOML compose parser, runtime spec generation.

#### Phase 1 — Image Pull Pipeline
OCI distribution spec client, bearer token auth, multi-arch manifest list, content-addressable blob store with SHA-256 verification, layer deduplication, concurrent downloads, image index persistence.

#### Phase 2 — Container Execution
Layer unpacking (tar+gzip), overlay filesystem (overlayfs on Linux), kavach sandbox integration (OCI + Process backends), full OCI runtime spec (resource limits, mounts, env, user, workdir), volume bind mounts, container logging, one-shot execution model.

#### Phase 3 — Networking
Bridge networks with veth pairs, NAT via nein/nftables, port mapping (TCP/UDP), container DNS (resolv.conf + hosts injection), custom named networks, IP address pool with allocation/release.

#### Phase 4 — Orchestration
Compose up/down with DAG dependency ordering (majra), health checks via majra HeartbeatTracker, restart policies (Always, OnFailure, UnlessStopped, Never), daimon agent registration, sutra-stiva deployment module, 5 MCP tools, agnoshi intent stubs, replica support, PubSub events.

</details>

## Development

### Scripts

| Script | Description |
|--------|-------------|
| `scripts/version-bump.sh <version>` | Bump version in `VERSION` and `Cargo.toml`, update `Cargo.lock` |
| `scripts/bench.sh` | Run test suite + release build, append timing results to `benches/history.log` |
| `scripts/bench.sh --history` | Show benchmark history |
| `scripts/bench.sh --clean` | Clear benchmark history |

### Makefile targets

| Target | Description |
|--------|-------------|
| `make check` | Run fmt + clippy + test |
| `make fmt` | Check formatting |
| `make clippy` | Lint with zero warnings |
| `make test` | Run test suite |
| `make bench` | Run criterion benchmarks |
| `make bench-history` | Run benchmark suite and append to history log |
| `make audit` | Security audit via `cargo audit` |
| `make deny` | Supply-chain checks (license + advisory) |
| `make build` | Release build |
| `make doc` | Generate documentation |
| `make clean` | Clean build artifacts |

## How Stiva Completes the k8s Picture

See [k8s-roadmap.md](../docs/development/k8s-roadmap.md) in agnosticos.

| k8s Component | Before Stiva | After Stiva |
|---|---|---|
| Container runtime | Missing (0%) | OCI-compatible runtime |
| Pod sandbox | kavach (process-level) | kavach + stiva (full container isolation) |
| Image registry | ark packages only | OCI images + ark packages |
| Docker Compose | Not supported | `stiva compose` (TOML-based) |
| Container networking | agnosys netns only | Full bridge/NAT/custom networks |
| Health/restart | Manual | Heartbeat FSM + restart policies |
| Orchestration | None | DAG-ordered compose + sutra playbooks |

## License

GPL-3.0 — see [LICENSE](LICENSE) for details.
