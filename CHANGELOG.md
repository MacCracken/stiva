# Changelog

All notable changes to stiva are documented here.

## [1.1.0] — 2026-04-02

### Added
- **OCI runtime-spec v1.2.0** — `domainname` field on `ContainerConfig` and `RuntimeSpec` for UTS namespace domain name; wired through kavach with `sethostname`/`setdomainname` in pre_exec (after UTS namespace, before seccomp)
- **MCP annotations** — all 9 MCP tools now include `readOnlyHint`/`destructiveHint` annotations per MCP 2025-03-26 spec (pull/ps/inspect = read-only; run/stop/ansamblu/exec/build/push = destructive)
- **CVE-2024-21626 mitigation** — fd cleanup (`close(3..1024)`) in `pre_exec` hook and `stdin(null)` in `exec_in_container()` and kavach's `execute_with_timeout()`/`spawn_process()`/`build_command()` to prevent container escape via leaked host file descriptors
- **Manifest digest verification** — `Docker-Content-Digest` header checked against computed SHA-256 on manifest pull (defense-in-depth against registry MITM)
- **CPU cgroup enforcement** — `apply_cgroup_limits()` now writes `cpu.max` (quota/period) in addition to `memory.max` and `pids.max`
- **Security audit log** — `docs/security-audit-log.md` tracking CVE reviews and remediation
- **Spec compliance tracker** — `docs/spec-compliance.md` tracking OCI, MCP, CRIU, and networking spec conformance
- **Roadmap** — `docs/development/roadmap.md` with prioritized work items

### Fixed
- **CVE-2024-24557 hardening** — removed unused tag-keyed manifest cache (`store_manifest_ref`) that could enable cache poisoning if read-back was added; changed image lookups from `.contains()` substring match to exact match
- **RUSTSEC-2025-0067/0068** — replaced unsound `serde_yml` with `serde-saphyr` (safe pure-Rust YAML parser)
- **SPDX license** — `GPL-3.0` → `GPL-3.0-or-later` (valid SPDX identifier)
- **kavach composite backend** — missing `tcp_bind_ports`/`tcp_connect_ports` fields in `merge_policies`

### Changed
- **Dependency updates** — bote 0.50.0 → 0.91.0, majra 1.0.3 → 1.0.4, plus 34 transitive crate updates (hyper, uuid, libc, zerocopy, wasm-bindgen, ICU crates, etc.)
- **bote dependency** — moved from local `path` dep to versioned crates.io dep (`>=0.91`) with `[patch.crates-io]` override, matching kavach/majra/nein pattern
- **YAML parser** — `serde_yaml` (deprecated) → `serde_yml` → `serde-saphyr` (maintained, safe)

## [1.0.0] — 2026-03-25

### Added
- **Persistent state** — container records saved to `state.json`, restored on manager restart; running/paused containers transition to Stopped on restart
- **Container restart** — `ContainerManager::restart()`, `Stiva::restart()`, `stiva restart` CLI; resets Stopped→Created→start()
- **Feature-gate chain** — `runtime` implies `image`+`registry`, `compose` implies `runtime`, `default = full`
- **Integration test suite** — 10 integration tests covering full lifecycle, persistence, export/import, fleet scheduling, copy
- **Doc-test** — crate-level quick start example
- **`stiva info`** — system information (version, paths, container/image counts, CRIU availability)
- **`stiva restart`** — restart stopped containers (26 CLI commands total)
- **Error quality** — user-friendly error messages in CLI (container not found, auth failed, invalid reference, etc.)
- **Credential injection** — `ContainerConfig.secrets` accepts `kavach::SecretRef` for env var / file / stdin secret injection without exposing in config; `--secret KEY=VALUE` CLI flag
- **Security scoring** — `Stiva::security_score()` and `container_security_score(id)` via `kavach::score_backend()`; shown in `stiva info` and `stiva inspect` output
- **Output scanning** — `ContainerConfig.scan_policy` enables `kavach::ExternalizationGate` on exec/logs output; blocks private keys, oversized output, PII per policy
- **`ScanBlocked` error variant** — returned when output scanning blocks container output
- 423 total tests (412 lib + 10 integration + 1 doc-test)

### Changed
- Version: 0.25.4 → 1.0.0
- `ImageStore::add_to_index` and `save_index_pub` now `pub` (were `pub(crate)`)
- `default` feature changed from `runtime` to `full`

## [0.25.4] — 2026-03-25

### Added
- **Long-running daemon containers** — `ContainerConfig.detach = true` spawns containers as background daemons via kavach `spawn()` instead of blocking `exec()`
- **Daemon lifecycle** — `ContainerManager::wait()`, `try_wait()` for daemon containers; `stop()` now sends SIGTERM with configurable grace period before SIGKILL
- **`DaemonHandle`** — wrapper around kavach `SpawnedProcess` with PID tracking, wait, kill, and try_wait
- **`Stiva::wait()`** — top-level API for waiting on container exit
- **kavach `spawn()`** — new `Sandbox::spawn()` method and `SpawnedProcess` type for non-blocking process execution with PID, wait, kill (SIGTERM→SIGKILL), and try_wait
- **`ContainerConfig.stop_grace_ms`** — configurable SIGTERM grace period (default 10s)
- **Image push** — `RegistryClient::push_blob()`, `push_manifest()`, `blob_exists()` for OCI distribution push; `ImageStore::push()` orchestrates config + layer + manifest upload with dedup; `Stiva::push()` top-level API
- **Rootless containers** — `ContainerConfig.rootless = true` enables user namespace with UID/GID remapping; kavach writes `/proc/self/uid_map` and `/proc/self/gid_map` after `unshare(CLONE_NEWUSER)` mapping host UID→0 inside; no real root required
- **`authenticated_request()`** — generic auth method supporting any HTTP method/scope, deduplicated from `authenticated_get()`
- **TOML image build** — `Stivafile` build spec with `run`, `copy`, `env`, `workdir`, `label` steps; `build::parse_build_spec()` parser, `build::build_image()` executor; `Stiva::build()` top-level API; generates OCI layers (tar+gzip) per step with SHA-256 verification
- **Container checkpointing** — `runtime::checkpoint_container()` and `restore_container()` via CRIU; `ContainerManager::checkpoint()` creates checkpoint bundles, `restore()` resumes from them; `Stiva::checkpoint()`/`restore()` top-level API
- **Live migration** — `MigrationBundle` type packages container config + image ref + checkpoint data; `ContainerManager::prepare_migration()` and `apply_migration()` for cross-node container transfer
- **Daimon edge fleet** — `fleet` module with `FleetDeployment`, `DeploymentConstraints`, `DeploymentStrategy` (Spread/BinPack/Pinned), `FleetNode`, `NodeCapacity`, `NodeStatus`; `fleet::schedule()` assigns replicas across nodes; `fleet::select_migration_target()` picks optimal migration destination
- **Container exec** — `runtime::exec_in_container()` via `nsenter` into PID/mount/net/UTS/IPC namespaces; `ContainerManager::exec()` and `Stiva::exec()` APIs
- **Signal forwarding** — `runtime::send_signal()` via nix; `ContainerManager::signal()` and `Stiva::signal()` for sending arbitrary signals (SIGHUP, SIGINT, SIGUSR1, etc.)
- **Pause/unpause** — `runtime::pause_container()`/`unpause_container()` via cgroups v2 freezer (`cgroup.freeze`); `Stiva::pause()`/`unpause()` with Paused state tracking
- **Container stats** — `runtime::container_stats()` reads memory, CPU, PIDs from cgroups v2; `ContainerStats` type; `Stiva::stats()` API
- **Image management** — `Stiva::rmi()` remove images, `tag()` create aliases, `inspect_image()` full details
- **Container inspect** — `Stiva::inspect()` by ID or name
- **Prune** — `Stiva::prune()` removes stopped containers and unreferenced images
- **MCP tools expanded** — 9 tools (+exec, build, push, inspect) with handlers
- **Cgroups v2 enforcement** — `runtime::apply_cgroup_limits()` writes `memory.max` and `pids.max` after daemon spawn; best-effort with warning on failure
- **Network wiring** — `ContainerManager` lazy-creates `NetworkManager`, auto-connects daemon containers to bridge network with port mappings and DNS injection on start
- **Lifecycle events** — majra pubsub events on create/start/stop/remove/pause/unpause; `ContainerManager::event_bus()` accessor for subscribers
- **Log streaming** — `ContainerManager::log_tail(id, lines)` reads last N lines from container log; `Stiva::log_tail()` top-level API
- **CLI binary** — `stiva` command with 24 subcommands: pull, build, push, run, ps, stop, rm, exec, top, inspect, images, rmi, tag, pause, unpause, stats, logs, kill, export, import, cp, prune, wait, checkpoint, restore
- **Container top** — `runtime::container_top()` lists processes via /proc PID tree walk; `ProcessInfo` type
- **Container export/import** — `runtime::export_rootfs()` tar archive, `runtime::import_rootfs()` creates single-layer image from tar
- **Container copy** — `runtime::copy_into_container()` / `copy_from_container()` with recursive dir support
- **Criterion benchmarks** — 18 benchmarks across imageref, volume, port, blob, ippool, fleet, build; `bench-history.sh` generates CSV + benchmarks.md trend
- 393 tests passing

### Changed
- Version bump: 0.25.3 → 0.25.4 (stiva), 0.22.3 → 0.25.3 (kavach)
- `ContainerManager::stop()` — now properly kills daemon processes with SIGTERM→SIGKILL instead of just setting state
- `runtime::exec_container` — refactored to share sandbox setup with `spawn_container` via `build_sandbox()` helper

### Improved
- **P(-1) scaffold hardening** — `#[non_exhaustive]` on all 11 public enums, `#[must_use]` on ~30 pure functions, `#[inline]` on hot-path accessors
- **`Cow` over clone** — `digest_hex()` returns `Cow<str>` avoiding allocation on every blob op
- **`write!` over `format!`** — `sha256_digest()` and env var building avoid temporary allocations

## [0.22.3] — 2026-03-22

### Added
- **Compose orchestration** — `compose_up`/`compose_down` with DAG dependency ordering via majra DagScheduler, topological sort, cycle detection
- **Restart policies** — `Always`, `OnFailure { max_retries }`, `UnlessStopped`, `Never` with restart count tracking
- **Health monitoring** — `HealthMonitor` wrapping majra `ConcurrentHeartbeatTracker`, Online→Suspect→Offline FSM
- **Health check config** — per-service command, interval, timeout, retries in compose files
- **Compose sessions** — `ComposeSession` tracking services, networks, startup order; replica support (N containers per service)
- **Daimon agent integration** — HTTP-based container registration/deregistration/status reporting (`src/agent.rs`)
- **MCP tools** — 5 tools: `stiva_pull`, `stiva_run`, `stiva_ps`, `stiva_stop`, `stiva_compose` with JSON Schema input specs (`src/mcp.rs`)
- **Sutra module** — `sutra-stiva` crate in sutra-community: pull, run, stop, rm, compose_up, compose_down
- **Agnoshi intents** — stub types for future NL→intent parsing: Run, Stop, Pull, Compose, Scale, Inspect (`src/intents.rs`)
- **PubSub integration** — majra pubsub feature enabled for container lifecycle events
- **Benchmark script** — `scripts/bench.sh` appends timestamped test/build timing to `benches/history.log`
- 290 tests passing

### Changed
- Version bump: 0.21.3 → 0.22.3 across stiva, kavach, majra, nein
- majra features: `["queue", "heartbeat"]` → `["queue", "heartbeat", "pubsub"]`

## [0.21.3] — 2026-03-21

### Added
- **Phase 0 — Foundation** — Scaffold with module structure, image reference parser, container lifecycle state machine, OCI manifest/descriptor types, volume mount parsing, network mode types, TOML compose parser, runtime spec generation
- **Phase 1 — Image Pull Pipeline** — OCI distribution spec client (manifest fetch, blob download), bearer token auth (Docker Hub, GHCR), multi-arch manifest list support, content-addressable blob store with SHA-256 verification, layer deduplication, concurrent downloads, image index persistence
- **Phase 2 — Container Execution** — Layer unpacking (tar+gzip), overlay filesystem (overlayfs on Linux), kavach sandbox integration (OCI + Process backends), full OCI runtime spec (resource limits, mounts, env, user, workdir), volume bind mounts, container logging, one-shot execution model
- **Phase 3 — Networking** — Network module restructured to submodule (pool, bridge, nat, dns, manager), IP address pool, bridge + veth management via `ip` commands, NAT + port mapping via nein, DNS injection, NetworkManager lifecycle

### Removed
- Unused dependencies: `anyhow`, `async-trait`, `oci-spec`, `tracing-subscriber`

### Fixed
- `ImageRef::parse` port-in-registry bug (`localhost:5000/image` misparsed)
- `ContainerManager::remove` used `AlreadyRunning` error instead of `InvalidState`
- `compose::parse_compose` used `Runtime` error instead of `Compose`
