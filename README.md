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
- **Daemon Containers** — long-running processes with spawn/wait/kill, SIGTERM→SIGKILL grace
- **Container Exec** — run commands inside running containers via nsenter
- **Signal Forwarding** — send arbitrary signals to container processes
- **Pause/Unpause** — cgroups v2 freezer for lightweight suspension
- **Container Stats** — CPU, memory, PID metrics from cgroups v2
- **TOML Image Build** — Stivafile.toml build spec (run, copy, env, workdir, label steps)
- **Image Push** — OCI distribution push with dedup (blob exists check)
- **Rootless Containers** — user namespace UID/GID remapping, no root required
- **Checkpointing** — CRIU-based checkpoint/restore for daemon containers
- **Live Migration** — checkpoint + transfer + restore across nodes
- **Fleet Scheduling** — spread, bin-pack, pinned strategies across daimon nodes
- **MCP Tools** — 9 AI agent tools (pull, run, ps, stop, compose, exec, build, push, inspect)
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

## Roadmap to 1.0

### Phase 10 — 1.0 Release (next)
- [ ] Persistent container state (survive daemon restart)
- [ ] Container restart on crash (auto-restart via health monitor)
- [ ] Streaming blob push (avoid loading full layer into memory)
- [ ] Feature-gate enforcement (declared features actually gate module compilation)
- [ ] Integration test suite (real container lifecycle with process backend)
- [ ] Doc-tests for public API
- [ ] `stiva version` / `stiva info` CLI commands
- [ ] Error message quality pass (user-facing errors in CLI)

### Completed (Phase 0–9)

<details>
<summary>Click to expand</summary>

#### Phase 9 — Usability
CLI binary (24 subcommands), container top (/proc walk), export/import (tar round-trip), container copy (files in/out), criterion benchmarks (18 benchmarks + bench-history.sh).

#### Phase 8 — Runtime Integration
Cgroups v2 resource enforcement, network wiring into container start, lifecycle events via majra pubsub, log streaming.

#### Phase 7 — Complete Runtime
Container exec (nsenter), signal forwarding, pause/unpause (cgroups v2 freezer), container stats, image management (tag/rmi/inspect), container inspect, prune, 9 MCP tools.

#### Phase 6 — Production Hardening
Checkpointing (CRIU dump/restore), live migration, daimon edge fleet scheduling, TOML image build, image push, rootless containers.

#### Phase 5 — Advanced
Long-running daemon containers (kavach spawn/wait/kill), rootless (user namespace UID/GID mapping), image push, container checkpointing, live migration, fleet integration.

#### Phase 4 — Orchestration
Compose up/down (DAG ordering via majra), health checks, restart policies, daimon agent registration, sutra module, MCP tools, intent stubs, replica support.

#### Phase 3 — Networking
Bridge networks, NAT via nein/nftables, port mapping, container DNS, custom networks, IP pool.

#### Phase 2 — Container Execution
Layer unpacking, overlay filesystem, kavach sandbox integration, OCI runtime spec, volume mounts, container logging.

#### Phase 1 — Image Pull Pipeline
OCI distribution client, bearer token auth, multi-arch manifests, content-addressable blob store, layer dedup, concurrent downloads.

#### Phase 0 — Foundation
Scaffold, image reference parser, container lifecycle state machine, OCI types, volume parsing, network modes, TOML compose parser, runtime spec generation.

</details>

## Development

### Scripts

| Script | Description |
|--------|-------------|
| `scripts/version-bump.sh <version>` | Bump version in `VERSION` and `Cargo.toml`, update `Cargo.lock` |
| `scripts/bench.sh` | Run test suite + release build, append timing results to `benches/history.log` |
| `scripts/bench.sh --history` | Show benchmark history |
| `scripts/bench.sh --clean` | Clear benchmark history |
| `scripts/bench-history.sh` | Run criterion benchmarks, append to `bench-history.csv`, generate `benchmarks.md` |

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
