# Stiva

> **Stiva** (Romanian: stivă — stack, pile) — OCI-compatible container runtime for AGNOS

[![License: GPL-3.0](https://img.shields.io/badge/License-GPL--3.0-blue.svg)](LICENSE)

Stiva is the container runtime that completes AGNOS's path to a full orchestration platform. It builds on [kavach](https://github.com/MacCracken/kavach) for process isolation (seccomp, Landlock, namespaces, capabilities) and [majra](https://github.com/MacCracken/majra) for scheduling primitives (priority queues, heartbeat FSM, pub/sub).

## Architecture

```
stiva (this crate)
  ├── kavach (sandbox: seccomp, Landlock, namespaces, OCI spec, gVisor, Firecracker, WASM)
  ├── majra (job queue, heartbeat FSM, pub/sub, relay)
  └── nein (network policy — planned)
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
- **Majra Integration** — priority-based container scheduling, heartbeat health tracking

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

### Phase 0 — Foundation (current)
- [x] Scaffold crate with module structure
- [x] Image reference parser (docker.io, ghcr.io, custom)
- [x] Container lifecycle state machine (Created → Running → Stopped)
- [x] Container manager with create/start/stop/remove
- [x] OCI manifest/descriptor types
- [x] Volume mount parsing
- [x] Network mode types (Bridge, Host, None, Container, Custom)
- [x] TOML compose file parser
- [x] Runtime spec generation
- [x] 17 tests passing

### Phase 1 — Image Pull Pipeline
- [ ] OCI distribution spec client (manifest fetch, blob download)
- [ ] Token auth (Docker Hub, GHCR bearer tokens)
- [ ] Multi-arch manifest list support
- [ ] Layer deduplication in content-addressable store
- [ ] Download resume + SHA-256 verification
- [ ] Image index persistence

### Phase 2 — Container Execution
- [ ] Overlay filesystem assembly from image layers
- [ ] kavach sandbox integration (process backend → container)
- [ ] OCI runtime spec generation (full spec, not just command)
- [ ] Namespace creation (pid, net, mount, uts, ipc, user)
- [ ] Cgroup resource limits (memory, CPU)
- [ ] Volume bind mounts
- [ ] Container logging (stdout/stderr capture)

### Phase 3 — Networking
- [ ] Bridge network with veth pairs (via agnosys netns)
- [ ] NAT rules via nftables (nein crate when available)
- [ ] Port mapping (host:container)
- [ ] Container DNS resolution
- [ ] Custom named networks
- [ ] Network isolation between containers

### Phase 4 — Orchestration
- [ ] Compose up/down/restart
- [ ] Service dependency ordering (DAG via majra)
- [ ] Health checks
- [ ] Restart policies (always, on-failure, unless-stopped)
- [ ] Daimon integration (register containers as agents)
- [ ] Sutra module for container deployment playbooks
- [ ] MCP tools: `stiva_run`, `stiva_ps`, `stiva_pull`, `stiva_stop`, `stiva_compose`
- [ ] Agnoshi intents

### Phase 5 — Advanced
- [ ] Build from Dockerfile/Containerfile (or TOML equivalent)
- [ ] Image push to registries
- [ ] Container checkpointing (CRIU integration)
- [ ] Live migration between nodes
- [ ] Integration with daimon edge fleet (schedule containers across nodes)
- [ ] Rootless containers (user namespace remapping)

## Reference Code

| Source | What to Reference | Path | Maturity |
|--------|------------------|------|----------|
| **Kavach** | Sandbox backends (process, gVisor, Firecracker, WASM, OCI, SGX, SEV), policy engine, lifecycle, credential proxy, strength scoring | `/home/macro/Repos/kavach/src/` | **High** — 8 backends, published to crates.io (0.21.3) |
| **Kavach** `backend/oci/` | OCI runtime spec execution, existing container isolation patterns | `/home/macro/Repos/kavach/src/backend/oci/` | **High** — foundation for stiva's runtime module |
| **Majra** | Priority queue (DAG scheduling), heartbeat FSM, pub/sub, relay, rate limiting | `/home/macro/Repos/majra/src/` | **High** — crates.io (0.21.3), benchmarked, proptested |
| **Agnosys** | Namespace creation (netns, cgroups), mount operations, seccomp, Landlock | `userland/agnos-sys/src/` | **High** — syscall bindings used across AGNOS |
| **Daimon** `sandbox_v2` | Novel sandboxing patterns (79 tests), Landlock + seccomp composition | `userland/agent-runtime/src/sandbox_v2.rs` | **High** — production sandbox code |
| **Daimon** `edge` | Fleet node management, capability routing, VRAM-aware placement | `userland/agent-runtime/src/edge.rs` | **High** — 37 tests, integration target |
| **Sutra Community** | nftables module (network rules), sysctl module | `/home/macro/Repos/sutra-community/` | **Medium-High** — reference for networking |

## How Stiva Completes the k8s Picture

See [k8s-roadmap.md](../docs/development/k8s-roadmap.md) in agnosticos.

| k8s Component | Before Stiva | After Stiva |
|---|---|---|
| Container runtime | Missing (0%) | OCI-compatible runtime |
| Pod sandbox | kavach (process-level) | kavach + stiva (full container isolation) |
| Image registry | ark packages only | OCI images + ark packages |
| Docker Compose | Not supported | `stiva compose` (TOML-based) |
| Container networking | agnosys netns only | Full bridge/NAT/custom networks |

## License

GPL-3.0 — see [LICENSE](LICENSE) for details.
