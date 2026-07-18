# Stiva — Claude Code Instructions

## Project Identity

**Stiva** (Romanian: stack) — OCI container runtime — image management, container lifecycle, orchestration

- **Type**: Crate with library + CLI binary (`stiva`)
- **License**: GPL-3.0-or-later
- **Toolchain**: Cyrius, pin **6.4.66** (the Rust oracle at `rust-old/` targeted MSRV 1.89)
- **Version**: SemVer, currently 3.0.1
- **Genesis repo**: [agnosticos](https://github.com/MacCracken/agnosticos)
- **Philosophy**: [AGNOS Philosophy & Intention](https://github.com/MacCracken/agnosticos/blob/main/docs/philosophy.md)
- **Standards**: [First-Party Standards](https://github.com/MacCracken/agnosticos/blob/main/docs/development/applications/first-party-standards.md)
- **Recipes**: [zugot](https://github.com/MacCracken/zugot) — takumi build recipes

## ✅ Porting Status — v3.0.0 RELEASED: Rust → Cyrius (single-node runtime; v3.0.x completes it, small blocked residue → v3.1)

Stiva has been ported from Rust to the **Cyrius** language (AGNOS ecosystem port
pattern). **v3.0.0 = a working single-node OCI runtime**: all 16 Rust modules → 25
Cyrius `src/*.cyr` domain modules + a **19-verb CLI** (run/ps/stop/rm/inspect/
images/rmi/tag/import/export/stats/pause/unpause/logs/wait/gc/prune/info/convert)
that imports, runs, and manages real containers end-to-end. **1033 tests** across the
`.tcyr` files (run via `cyrius tests tests/`), `dist/stiva.cyr` built, pin **6.4.66**.
Parity ≈ **67%** of the Rust surface — the port stops cleanly at the sync/async
boundary (algorithm-dense modules 85–100%; the low-parity ones are async wrappers whose
capability is delivered synchronously). The v3.0.x sync backlog is essentially complete:
`oci`, `image` (verify_integrity), `runtime` (/proc walk + host↔rootfs copy), `registry`
(credential store), `build` (build_cache_key), `ansamblu` (parse_ansamblu), `mcp` (the two
sync tool handlers), and `intents` (variant-payload serde) are ported. The only sync
remainder is `build`'s OCI config/manifest JSON assembly (embedded in the async build_image;
serde HashMap ordering is nondeterministic). See `docs/development/roadmap.md` for the parity
snapshot + v3.0.x (Wave 2) / v3.1 (blocked residue) split.

The old **v3.1 async milestone was dissolved**: the async substrate (`lib/async.cyr`) is
ready and the runtime is single-threaded run-to-completion, so most of the remaining
container-orchestration surface is **blocking** work now folded onto the **v3.0.x line
(Wave 2)** — buildable now over the ported sync core: the `ContainerManager` + `Stiva`
facade, the blocking registry client (pull/push), non-interactive `exec`, CRIU
checkpoint/restore, and MCP dispatch (ps/stop/inspect/pull/push/exec). Only a small
**externally-blocked residue** is **v3.1 (blocked)** — genuinely gated on an external
landing: detached `run -d` (needs kavach `sandbox_spawn`), interactive `exec -it` and
true multiplexed streaming (need cyrius stackless coroutines), MCP `handle_run` (needs
`run -d`), and `scan_output` (needs kavach ExternalizationGate).
NOT mostly Cyrius gaps — JSON/TOML/base64 (`bayan`), HTTP/TLS (`sandhi`/`tls_native`),
gzip/xz/lz4/bzip2 (`sankoch`), async (`lib/async.cyr`) all EXIST. Genuine stdlib gaps
are narrow, and are the **v3.1 upstream residue**: **zstd** layers (sankoch) and
`convert compose` / a **YAML** parser (bayan); the USTAR **tar writer** is now
implemented (`storage.cyr create_tar`). `rust-old/` stays the oracle until v3.1.

- **The Rust crate is frozen at `rust-old/`** — it is the **parity oracle**. The
  bar for every ported module is "matches what the Rust did." Do NOT edit it.
- **Use the Cyrius toolchain, NOT cargo.** Any lingering `cargo` / `clippy` /
  `rustc` reference describes the pre-port Rust flow; Rust survives only as the
  `rust-old/` oracle. The live commands are:

  | Action | Command |
  |---|---|
  | Resolve deps | `cyrius deps` |
  | Vendor stdlib subset into `lib/` | `cyrius lib sync` (run after changing `[deps].stdlib`) |
  | Build | `cyrius build src/main.cyr build/stiva` |
  | Run tests | `cyrius test tests/stiva.tcyr` |
  | Bench | `cyrius bench tests/stiva.bcyr` |
  | Format / lint | `cyrius fmt <file> --check` · `cyrius lint <file>` (per-file) |
  | Audit sweep | `cyrius audit` |

- **Structure**: one domain `src/*.cyr` module per Rust module (kavach model);
  `src/lib.cyr` = aggregation header; `src/main.cyr` = program entry (includes +
  CLI); `cyrius.cyml` `[lib].modules` drives `cyrius distlib` → `dist/stiva.cyr`.
- **Port process & sequencing**: `docs/development/roadmap.md` (v3.0.0). Ported
  modules and the module-by-module workflow live there; `scripts/port-workflow.js`
  is the agent-orchestrated porting harness. Canonical Cyrius exemplars to copy:
  `src/error.cyr`, `src/oci.cyr`.
- **AGNOS deps** are consumed as Cyrius `dist/*.cyr` bundles (kavach/majra/nein/
  bote/agnodrm), wired in `cyrius.cyml` `[deps.*]` as their consuming modules land.

## Stack

| Crate | Role |
|-------|------|
| kavach | Sandbox isolation (seccomp, Landlock, namespaces, gVisor, Firecracker, WASM) |
| majra | Job queue, heartbeat FSM, pub/sub |
| nein | nftables firewall, NAT, port mapping |
| bote | MCP core service (JSON-RPC 2.0, tool registry, structured output) |
| agnodrm | LUKS + dm-verity (the `encrypted` module) |

All AGNOS deps are consumed as Cyrius `dist/*.cyr` bundles, wired in `cyrius.cyml`
`[deps.*]` with local `path = "../<dep>"` overrides (sibling repos).

## Consumers

daimon (container management), sutra (fleet deployment)

## Modules (16)

| Module | Purpose |
|--------|---------|
| `image` | OCI image pull, push, build, store, layer management, GC |
| `container` | Container lifecycle, state persistence, events, restart, rename, update |
| `runtime` | OCI spec, kavach integration, cgroups (CPU/mem/PID/IO), CRIU (checkpoint/pre-dump/lazy), exec, signals, export/import, copy |
| `network/` | Bridge, NAT, DNS, IP pools (IPv4+IPv6), port mapping, network policy, container DNS registry (6 submodules) |
| `storage` | Overlay FS, volume mounts, layer unpacking (gzip + zstd) |
| `registry` | OCI distribution client (pull + push + chunked upload), token auth, credential store, tag listing, catalog, referrers API |
| `build` | TOML-based image builds (Stivafile), multi-stage builds, build cache |
| `ansamblu` | Multi-container orchestration, DAG ordering, rolling updates, scaling, service logs |
| `health` | Heartbeat monitoring, restart policies |
| `fleet` | Edge fleet scheduling (spread, bin-pack, pinned), health monitoring, rollback planning |
| `agent` | Daimon agent registration |
| `mcp` | 9 MCP tools with structured output, live dispatch, resource exposure |
| `convert` | Docker Compose YAML and Dockerfile to Stivafile TOML conversion |
| `encrypted` | LUKS + dm-verity (feature-gated) |
| `intents` | Agnoshi intent stubs |
| `error` | Error types |

CLI binary: `stiva` — 33 registered verbs, **19 live** in v3.0.0; the rest print a "not yet wired" message — most are **v3.0.x (planned)** blocking glue over the sync core, a small residue is **v3.1 (blocked)** (see `docs/cli.md`).

## kavach Integration

Stiva uses these kavach features — keep them wired:

- **Sandbox** — `Sandbox::create`, `exec`, `spawn`, `destroy`
- **SpawnedProcess** — daemon containers (pid, wait, kill, try_wait)
- **SandboxPolicy** — memory, CPU, PID limits, seccomp, network
- **SandboxConfig** — hostname, domainname (UTS namespace, OCI v1.2.0)
- **CredentialProxy / SecretRef** — secret injection via env var / file
- **StrengthScore / score_backend** — security scoring in inspect/info
- **ExternalizationGate** — output scanning for secrets/PII in exec/logs
- **User namespaces** — rootless containers (UID/GID mapping)
- **NO_NEW_PRIVS** — explicit `prctl(PR_SET_NO_NEW_PRIVS)` in pre_exec
- **FD cleanup** — `close(3..1024)` in pre_exec (CVE-2024-21626 mitigation)

## Development Process

### P(-1): Scaffold Hardening (before any new features)

0. Read roadmap, CHANGELOG, and open issues — know what was intended before auditing what was built
1. Test + benchmark sweep of existing code
2. Cleanliness check: `cyrius fmt <file> --check` + `cyrius lint <file>` (per changed file), then `cyrius audit`
3. Get baseline benchmarks (`./scripts/bench-history.sh`)
4. Initial refactor + audit (performance, memory, security, edge cases)
5. Cleanliness check — must be clean after audit
6. Additional tests/benchmarks from observations
7. Post-audit benchmarks — prove the wins
8. Repeat audit if heavy
9. Documentation audit — ADRs, source citations, guides, examples (see Documentation Standards in first-party-standards.md)

### Development Loop (continuous)

1. Work phase — new features, roadmap items, bug fixes
2. Cleanliness check: `cyrius fmt <file> --check` + `cyrius lint <file>` (per changed file), then `cyrius audit`
3. Test + benchmark additions for new code
4. Run benchmarks (`./scripts/bench-history.sh`)
5. Audit phase — review performance, memory, security, throughput, correctness
6. Cleanliness check — must be clean after audit
7. Deeper tests/benchmarks from audit observations
8. Run benchmarks again — prove the wins
9. If audit heavy → return to step 5
10. Documentation — update CHANGELOG, roadmap, docs, ADRs for design decisions, source citations for algorithms/formulas, update docs/sources.md, guides and examples for new API surface, verify recipe version in zugot
11. Version check — VERSION (cyrius.cyml reads it via `${file:VERSION}`) + recipe (in zugot) in sync
12. Return to step 1

### Key Principles

- **Never skip benchmarks.** Numbers don't lie. The CSV history is the proof.
- **Tests + benchmarks are the way.** 1033 Cyrius tests across `tests/*.tcyr`. Keep adding — mirror the rust-old `#[cfg(test)]` cases.
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

## Documentation Structure

```
Root files (required):
  README.md, CHANGELOG.md, CLAUDE.md, CONTRIBUTING.md, SECURITY.md, CODE_OF_CONDUCT.md, LICENSE

docs/ (required):
  architecture/overview.md — module map, data flow, consumers
  development/roadmap.md — completed, backlog, future, v1.0 criteria

docs/ (when earned):
  adr/ — architectural decision records
  guides/ — usage guides, integration patterns
  examples/ — worked examples
  standards/ — external spec conformance
  compliance/ — regulatory, audit, security compliance
  sources.md — source citations for algorithms/formulas (required for science/math crates)
```

## Testing

**1033 Cyrius tests** across `tests/*.tcyr` (stiva 734 · runpath 171 · mgmt 128), each
mirroring the matching rust-old `#[cfg(test)]` cases. The suite is split across files
because a monolith hits the cycc identifier-dedup cap.

```bash
cyrius tests tests/            # run every tests/*.tcyr (the full suite)
cyrius test tests/stiva.tcyr   # run one test file
cyrius bench tests/stiva.bcyr  # benchmarks
./scripts/bench-history.sh     # benchmarks + CSV + trend report
./scripts/bench.sh             # test + build timing history
```

## DO NOT

- **Do not commit or push** — the user handles all git operations (commit, push, tag)
- **NEVER use `gh` CLI** — use `curl` to GitHub API only
- Do not add unnecessary dependencies — keep `[deps].stdlib` honest/opt-in
- Do not return dangling struct literals or use sentinel-less error paths in library code
- Do not edit `rust-old/` — it is the frozen parity oracle
- Do not skip benchmarks before claiming performance improvements
