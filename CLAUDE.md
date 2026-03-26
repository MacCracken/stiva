# Stiva — Claude Code Instructions

## Project Identity

**Stiva** (Romanian: stack) — OCI container runtime — image management, container lifecycle, orchestration

- **Type**: Crate with library + CLI binary (`stiva`)
- **License**: GPL-3.0
- **MSRV**: 1.89
- **Version**: SemVer, currently 1.0.0

## Stack

| Crate | Role |
|-------|------|
| kavach | Sandbox isolation (seccomp, Landlock, namespaces, gVisor, Firecracker, WASM) |
| majra | Job queue, heartbeat FSM, pub/sub |
| nein | nftables firewall, NAT, port mapping |
| agnosys | LUKS + dm-verity (optional, `encrypted` feature) |

All AGNOS crates are patched to local paths in `[patch.crates-io]`.

## Consumers

daimon (container management), sutra (fleet deployment)

## Modules (18)

| Module | Purpose |
|--------|---------|
| `image` | OCI image pull, push, build, store, layer management |
| `container` | Container lifecycle, state persistence, events, restart |
| `runtime` | OCI spec, kavach integration, cgroups, CRIU, exec, signals, export/import, copy |
| `network/` | Bridge, NAT, DNS, IP pools, port mapping (5 submodules) |
| `storage` | Overlay FS, volume mounts, layer unpacking |
| `registry` | OCI distribution client (pull + push), token auth |
| `build` | TOML-based image builds (Stivafile) |
| `ansamblu` | Multi-container orchestration, DAG ordering |
| `health` | Heartbeat monitoring, restart policies |
| `fleet` | Edge fleet scheduling (spread, bin-pack, pinned) |
| `agent` | Daimon agent registration |
| `mcp` | 9 MCP tools for AI agent integration |
| `encrypted` | LUKS + dm-verity (feature-gated) |
| `intents` | Agnoshi intent stubs |
| `error` | Error types |
| `main` | CLI binary (28 subcommands) |

## kavach Integration

Stiva uses these kavach features — keep them wired:

- **Sandbox** — `Sandbox::create`, `exec`, `spawn`, `destroy`
- **SpawnedProcess** — daemon containers (pid, wait, kill, try_wait)
- **SandboxPolicy** — memory, CPU, PID limits, seccomp, network
- **CredentialProxy / SecretRef** — secret injection via env var / file
- **StrengthScore / score_backend** — security scoring in inspect/info
- **ExternalizationGate** — output scanning for secrets/PII in exec/logs
- **User namespaces** — rootless containers (UID/GID mapping)

## Development Process

### P(-1): Scaffold Hardening (before any new features)

1. Test + benchmark sweep of existing code
2. Cleanliness check: `cargo fmt --check`, `cargo clippy --all-features --all-targets -- -D warnings`, `cargo audit`, `cargo deny check`
3. Get baseline benchmarks (`./scripts/bench-history.sh`)
4. Initial refactor + audit (performance, memory, security, edge cases)
5. Cleanliness check — must be clean after audit
6. Additional tests/benchmarks from observations
7. Post-audit benchmarks — prove the wins
8. Repeat audit if heavy

### Development Loop (continuous)

1. Work phase — new features, roadmap items, bug fixes
2. Cleanliness check: `cargo fmt --check`, `cargo clippy --all-features --all-targets -- -D warnings`, `cargo audit`, `cargo deny check`
3. Test + benchmark additions for new code
4. Run benchmarks (`./scripts/bench-history.sh`)
5. Audit phase — review performance, memory, security, throughput, correctness
6. Cleanliness check — must be clean after audit
7. Deeper tests/benchmarks from audit observations
8. Run benchmarks again — prove the wins
9. If audit heavy → return to step 5
10. Documentation — update CHANGELOG, roadmap, docs
11. Return to step 1

### Key Principles

- **Never skip benchmarks.** Numbers don't lie. The CSV history is the proof.
- **Tests + benchmarks are the way.** 433 tests, 20 criterion benchmarks. Keep adding.
- **Own the stack.** If an AGNOS crate wraps an external lib, depend on the AGNOS crate.
- **No magic.** Every operation is measurable, auditable, traceable.
- **`#[non_exhaustive]`** on all public enums.
- **`#[must_use]`** on all pure functions.
- **`#[inline]`** on hot-path functions.
- **`write!` over `format!`** — avoid temporary allocations.
- **Cow over clone** — borrow when you can, allocate only when you must.
- **Vec arena over HashMap** — when indices are known, direct access beats hashing.
- **Feature-gate optional deps** — consumers pull only what they need.
- **tracing on all operations** — structured logging for audit trail.
- **Persist state** — container records survive daemon restart via `state.json`.
- **Lifecycle events** — all state changes publish to majra pub/sub.

## Testing

| Category | Count |
|----------|-------|
| Library unit tests | 422 |
| Integration tests | 10 |
| Doc-tests | 1 |
| Criterion benchmarks | 20 |

```bash
cargo test --all-features                    # All tests
cargo test --all-features --test integration # Integration only
cargo bench --bench benchmarks               # Criterion benchmarks
./scripts/bench-history.sh                   # Benchmarks + CSV + trend report
./scripts/bench.sh                           # Test + build timing history
```

## DO NOT

- **Do not commit or push** — the user handles all git operations (commit, push, tag)
- **NEVER use `gh` CLI** — use `curl` to GitHub API only
- Do not add unnecessary dependencies — keep it lean
- Do not `unwrap()` or `panic!()` in library code
- Do not skip benchmarks before claiming performance improvements
