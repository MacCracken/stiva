# Contributing to stiva

Thank you for your interest in contributing to stiva. This document covers the
development workflow, code standards, and project conventions.

## Development Workflow

1. **Fork** the repository on GitHub.
2. **Create a branch** from `main` for your work.
3. **Make your changes**, ensuring all checks pass.
4. **Open a pull request** against `main`.

## Prerequisites

- Rust toolchain (MSRV: **1.89**)
- `cargo-deny` — supply chain checks
- `cargo-audit` — security advisory scanning
- `cargo-tarpaulin` — code coverage
- Local sibling repos: `kavach`, `majra`, `nein`, `bote` (for `[patch.crates-io]`)

## Makefile Targets

| Target             | Description                                      |
| ------------------ | ------------------------------------------------ |
| `make check`       | Run fmt + clippy + test (the full suite)         |
| `make fmt`         | Check formatting with `cargo fmt`                |
| `make clippy`      | Lint with `cargo clippy -D warnings`             |
| `make test`        | Run the test suite                               |
| `make bench-history` | Run benchmarks and append to history log       |
| `make deny`        | Audit dependencies with `cargo deny`             |
| `make build`       | Release build                                    |
| `make doc`         | Generate documentation                           |

## Code Standards

- **No warnings**: `cargo clippy --all-features --all-targets -- -D warnings` must pass.
- **Formatted**: `cargo fmt --check` must pass.
- **Tested**: new code should include tests. Target >=94% coverage.
- **Audited**: `cargo audit` and `cargo deny check` must pass.
- **No unnecessary deps**: avoid adding dependencies unless clearly needed.
- **Feature-gated**: the `ansamblu` feature gates orchestration code, `encrypted` gates LUKS.
- **`#[non_exhaustive]`** on all public enums.
- **`#[must_use]`** on all pure functions with meaningful return values.

## Scripts

| Script | Usage |
|--------|-------|
| `scripts/version-bump.sh <version>` | Bump version in `VERSION` + `Cargo.toml` |
| `scripts/bench.sh` | Run test+build benchmarks, append to `benches/history.log` |
| `scripts/bench-history.sh` | Run criterion benchmarks + CSV + trend report |

## Commit Messages

Follow conventional style:
- `add: new feature` — wholly new functionality
- `fix: bug description` — bug fix
- `update: enhancement` — improvement to existing feature
- `refactor: description` — code restructuring without behavior change

## Architecture

Stiva is structured as a library crate with these modules:

| Module | Purpose |
|--------|---------|
| `image` | OCI image pull, push, build, store, GC |
| `container` | Container lifecycle (create, start, stop, rename, update, remove) |
| `runtime` | OCI runtime spec generation, kavach sandbox, cgroups, CRIU |
| `storage` | Overlay filesystem, layer unpacking (gzip + zstd), volumes |
| `network/` | Bridge, NAT, DNS, IP pools (v4+v6), port mapping, network policy |
| `registry` | OCI registry client (pull, push, chunked upload, tags, catalog, referrers) |
| `build` | TOML-based builds (Stivafile), multi-stage, build cache |
| `ansamblu` | Multi-container orchestration, rolling updates, scaling |
| `health` | Health monitoring via majra heartbeat FSM |
| `fleet` | Fleet scheduling, health monitoring, rollback planning |
| `agent` | Daimon agent registration |
| `mcp` | MCP tools with structured output, live dispatch, resource exposure |
| `convert` | Docker Compose / Dockerfile to Stivafile conversion |
| `encrypted` | LUKS + dm-verity (feature-gated) |
| `intents` | Agnoshi intent stubs |

Dependencies on sibling AGNOS crates:
- **kavach** — sandbox execution (process isolation, OCI backend, seccomp, Landlock)
- **majra** — DAG scheduling, heartbeat health tracking, pub/sub events
- **nein** — nftables firewall rules, NAT, port mapping
- **bote** — MCP core service (tool registry, structured output, transport)
