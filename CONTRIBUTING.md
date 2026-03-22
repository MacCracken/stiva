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
- `cargo-tarpaulin` — code coverage
- Local sibling repos: `kavach`, `majra`, `nein` (for `[patch.crates-io]`)

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

- **No warnings**: `cargo clippy -- -D warnings` must pass.
- **Formatted**: `cargo fmt --all -- --check` must pass.
- **Tested**: new code should include tests. Target ≥94% coverage.
- **No unnecessary deps**: avoid adding dependencies unless clearly needed.
- **Feature-gated**: the `compose` feature should gate compose-only code.

## Scripts

| Script | Usage |
|--------|-------|
| `scripts/version-bump.sh <version>` | Bump version in `VERSION` + `Cargo.toml` |
| `scripts/bench.sh` | Run test+build benchmarks, append to `benches/history.log` |
| `scripts/bench.sh --history` | View benchmark history |

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
| `image` | OCI image pull, store, layer management |
| `container` | Container lifecycle (create, start, stop, remove) |
| `runtime` | OCI runtime spec generation, kavach sandbox execution |
| `storage` | Overlay filesystem, layer unpacking, volumes |
| `network/` | Bridge networks, IP pools, NAT, DNS, veth management |
| `registry` | OCI registry client (Docker Hub, GHCR, custom) |
| `compose` | TOML-based multi-container orchestration |
| `health` | Health monitoring via majra heartbeat FSM |
| `agent` | Daimon agent registration |
| `mcp` | MCP tools for AI agent integration |
| `intents` | Agnoshi intent stubs |

Dependencies on sibling AGNOS crates:
- **kavach** — sandbox execution (process isolation, OCI backend)
- **majra** — DAG scheduling, heartbeat health tracking, pub/sub events
- **nein** — nftables firewall rules, NAT, port mapping
