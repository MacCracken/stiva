# Contributing to stiva

Thank you for your interest in contributing to stiva. This document covers the
development workflow, code standards, and project conventions.

Stiva is written in **Cyrius** (the AGNOS systems language) and built with the
`cyrius` toolchain — **not** cargo. The Rust crate is frozen at `rust-old/` as the
parity oracle; do not edit it.

## Development Workflow

1. **Fork** the repository on GitHub.
2. **Create a branch** from `main` for your work.
3. **Make your changes**, ensuring all checks pass.
4. **Open a pull request** against `main`.

## Prerequisites

- The **Cyrius toolchain** (`cyrius`), pinned to **6.4.66** (see `cyrius.cyml`).
- Local sibling AGNOS repos next to this one, consumed as Cyrius `dist/*.cyr`
  bundles via `[deps.*]` path overrides in `cyrius.cyml`:
  `kavach`, `majra`, `nein`, `bote`, `agnodrm` (+ transitive `sakshi`, `libro`, `cmdit`).

## Toolchain Commands

| Action | Command |
|---|---|
| Resolve deps | `cyrius deps` |
| Vendor the stdlib subset into `lib/` | `cyrius lib sync` (after editing `[deps].stdlib`) |
| Build the binary | `cyrius build src/main.cyr build/stiva` |
| Run the full test suite | `cyrius tests tests/` (all `tests/*.tcyr`) |
| Run one test file | `cyrius test tests/stiva.tcyr` |
| Benchmarks | `cyrius bench tests/stiva.bcyr` |
| Format check (per file) | `cyrius fmt <file.cyr> --check` |
| Lint (per file) | `cyrius lint <file.cyr>` |
| Project sweep (fmt/lint/docs/tests/bench) | `cyrius audit` |
| Rebuild the `dist/stiva.cyr` bundle | `cyrius distlib` |

## Code Standards

- **Formatted**: `cyrius fmt <file> --check` must pass for every changed file.
- **Lint-clean**: `cyrius lint <file>` should be warning-free (lines ≤ 120 chars).
- **Tested**: new code must include tests in the matching `tests/*.tcyr` file. Each
  ported function mirrors its rust-old `#[cfg(test)]` cases — the bar is
  "matches what the Rust did." Deserializers must be **strict** (a missing required
  field / unknown variant / wrong type is an error, not a silent default), matching serde.
- **Rebuild the bundle**: run `cyrius distlib` when you change `src/*.cyr`, and commit
  the regenerated `dist/stiva.cyr`.
- **No unnecessary deps**: keep `[deps].stdlib` honest/opt-in; only pull what a module uses.
- **Cyrius idioms**: heap-alloc struct constructors (never return a dangling struct
  literal); module-prefixed enum members (Cyrius hoists members to global constants,
  last-def-wins, so prefixes keep them unique); `0` for null/None, `-1` sentinels for
  `Option<int>`; preserve exact error-display strings from the oracle.

The `~35` benign cross-bundle "duplicate fn (last definition wins)" warnings from the
vendored AGNOS bundles are expected — they are shared agnos error helpers each bundle vendors.

## Scripts

| Script | Usage |
|--------|-------|
| `scripts/version-bump.sh <version>` | Write the `VERSION` file (cyrius.cyml reads it via `${file:VERSION}`) |
| `scripts/bench.sh` | Test + build timing history |
| `scripts/bench-history.sh` | Benchmarks + CSV + trend report |
| `scripts/port-workflow.js` | Agent-orchestrated per-module port + adversarial parity verify |

## Commit Messages

Follow conventional style:
- `add: new feature` — wholly new functionality
- `fix: bug description` — bug fix
- `update: enhancement` — improvement to existing feature
- `refactor: description` — code restructuring without behavior change

## Architecture

Stiva is a Cyrius library + CLI. One domain `src/*.cyr` module per original Rust module
(the kavach model); `src/lib.cyr` is the aggregation header, `src/main.cyr` is the
program entry (includes + CLI dispatch). `cyrius.cyml` `[lib].modules` drives
`cyrius distlib` → `dist/stiva.cyr`.

| Module | Purpose |
|--------|---------|
| `image` | OCI image store, layers, tag/import/export, GC, blob integrity verify (registry pull/push → v3.1) |
| `container` | Container lifecycle + state persistence; OCI bundle parse / state (the async `ContainerManager` → v3.1) |
| `runtime` | OCI runtime spec, kavach sandbox, cgroups v2, /proc walk, host↔rootfs copy (exec/CRIU → v3.1) |
| `network/` | Bridge, NAT, DNS, IP pools (v4+v6), port mapping, network policy |
| `storage` | Overlay FS, volume mounts, gzip layer unpack, USTAR tar writer (zstd → v3.1) |
| `registry` | OCI ref/manifest parsing, credential store (the async HTTP client → v3.1) |
| `build` | Stivafile TOML parse, build-cache key (multi-stage layer build → v3.1) |
| `ansamblu` | TOML ansamblu parse, DAG ordering, rolling-update / scaling logic |
| `health` | Restart policies, heartbeat health via majra |
| `fleet` | Fleet scheduling, health monitoring, rollback planning |
| `agent` | Daimon agent registration |
| `mcp` | 9 MCP tool defs + 2 synchronous tool handlers (live dispatch → v3.1) |
| `convert` | Dockerfile → Stivafile (compose YAML → v3.1, needs a YAML parser) |
| `encrypted` | LUKS + dm-verity (agnodrm) |
| `intents` | Intent value type + serde (the NL `parse_intent` → agnoshi) |
| `error` | Error types |

Dependencies on sibling AGNOS projects (Cyrius `dist/*.cyr` bundles):
- **kavach** — sandbox execution (process isolation, OCI backend, seccomp, Landlock)
- **majra** — DAG scheduling, heartbeat health tracking, pub/sub events
- **nein** — nftables firewall rules, NAT, port mapping
- **bote** — MCP core service (tool registry, structured output)
- **agnodrm** — LUKS + dm-verity for the `encrypted` module
