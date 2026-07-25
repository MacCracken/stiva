# Stiva — Claude Code Instructions

## Project Identity

**Stiva** (Romanian: stack) — OCI container runtime — image management, container lifecycle, orchestration

- **Type**: Crate with library + CLI binary (`stiva`)
- **License**: GPL-3.0-or-later
- **Toolchain**: Cyrius, pin **6.4.78** (the Rust oracle at `rust-old/` targeted MSRV 1.89)
- **Version**: SemVer, currently 3.0.10
- **Genesis repo**: [agnosticos](https://github.com/MacCracken/agnosticos)
- **Philosophy**: [AGNOS Philosophy & Intention](https://github.com/MacCracken/agnosticos/blob/main/docs/philosophy.md)
- **Standards**: [First-Party Standards](https://github.com/MacCracken/agnosticos/blob/main/docs/development/applications/first-party-standards.md)
- **Recipes**: [zugot](https://github.com/MacCracken/zugot) — takumi build recipes

## ✅ Porting Status — v3.0.10 RELEASED: Rust → Cyrius (single-node runtime; **group A complete**, v3.0.x continues, small blocked residue → v3.1)

Stiva has been ported from Rust to the **Cyrius** language (AGNOS ecosystem port
pattern). **v3.0.0 = a working single-node OCI runtime**; **v3.0.1–v3.0.4 = group A
complete**; **v3.0.5 = zstd layer decode + dependency hygiene**; **v3.0.6 = roadmap §G
complete — output scanning (`stiva logs --scan`) + `convert --format compose` + working
benchmark scripts**; **v3.0.7 = roadmap §C complete — the `ContainerManager` + `Stiva`
facade, with the 12 live container CLI verbs routed through the manager (lifecycle events over
majra pub/sub)**; **v3.0.8 = roadmap §B increments 0–2 (registry-client foundations) + toolchain
6.4.77, whose sandhi 1.9.3 fixes the DNS CNAME + streaming-download-headers blockers stiva filed**;
**v3.0.9 = roadmap §B increments 3–8 — the registry client authenticates, resolves a manifest,
streams a layer to disk, and drives a full `image_store_pull` / `image_store_push`, plus a
registry-controlled path-traversal fix in the blob store (see the 3.0.9 CHANGELOG). `pull`/`push`
are library-complete but NOT yet CLI-wired — that is §B Inc-10**; **v3.0.10 = roadmap §B COMPLETE —
Inc-9 discovery (tags/catalog/referrers/verify-signature) + Inc-10 facade & CLI, so `stiva pull`
and `stiva push` are LIVE (23 of 35 verbs), plus a digest-aware image index**. All 16 Rust modules → **26** Cyrius `src/*.cyr` domain modules (incl. the
net-new `imagelayout.cyr`) + a **35-verb CLI, 21 live** (run/ps/stop/rm/inspect/images/
rmi/tag/import/export/stats/pause/unpause/logs/wait/gc/prune/info/convert + **save/load**).
**1739 tests** across the `.tcyr` files (stiva 664 · registry 326 · runpath 217 · store 197 ·
mgmt 219 · convert 116; run via `cyrius tests tests/`), `dist/stiva.cyr` built, pin **6.4.78**.

**Group A — OCI image-layout + transfer — is COMPLETE (v3.0.1–v3.0.4)**: the local store
is now a valid **OCI image layout** (`oci-layout` + `index.json` + `blobs/sha256/`, the
ad-hoc `images.json` **retired** — the store/index/import/save layer lives in
`imagelayout.cyr`); full OCI image **config + manifest** assembly (deterministic bytes —
the old "serde HashMap ordering" worry was Rust-only; bayan objects are insertion-ordered);
a **perms-preserving USTAR tar** codec (mode/uid/gid + dir/symlink, GNU longname, base-256;
traversal/symlink/DoS-hardened); and **`save`/`load` as `oci-archive`** plus a
**`docker-archive` read** path. Each increment was adversarially reviewed. See
`docs/development/roadmap.md` for the parity snapshot + v3.0.x / v3.1 split.

The old **v3.1 async milestone was dissolved**: the async substrate (`lib/async.cyr`) is
ready and the runtime is single-threaded run-to-completion, so most of the remaining
container-orchestration surface is **blocking** work now folded onto the **v3.0.x line
(Wave 2)** — buildable now over the ported sync core. The `ContainerManager` + `Stiva`
facade landed at **v3.0.7 (§C)** and the blocking registry client completed at **v3.0.10 (§B)**;
still buildable: non-interactive `exec`, CRIU checkpoint/restore, and MCP dispatch
(ps/stop/inspect/pull/push/exec).
Only a small
**externally-blocked residue** is **v3.1 (blocked)** — genuinely gated on an external
landing: detached `run -d` (needs kavach `sandbox_spawn`), interactive `exec -it` and
true multiplexed streaming (need cyrius stackless coroutines), MCP `handle_run` (needs
`run -d`).
NOT mostly Cyrius gaps — JSON/TOML/YAML/base64 (`bayan`), HTTP/TLS (`sandhi`/`tls_native`),
gzip/zstd/xz/lz4/bzip2 (`sankoch`), async (`lib/async.cyr`) all EXIST. The three stdlib
gaps that used to gate this line — zstd layers, `scan_output`, and `convert compose` —
have all landed (v3.0.5–v3.0.6), closing roadmap §G. `rust-old/` stays the oracle for the
ported modules; the group A OCI-layout/transfer surface is **net-new**, so the OCI
**image-spec** (not rust-old) is its oracle.

> ⚠️ **cycc struct-id miscompile — STILL LIVE at 6.4.77. The "appears fixed" reading below was
> WRONG and is kept as a worked example of how this bug fools a probe.**
>
> **What actually happened (2026-07-24, during §B Inc-4).** A probe asserting typed
> `Image`/`Layer` field access against raw-offset ground truth passed in all four unit shapes,
> so 3.0.8 recorded the bug as "appears fixed". Hours later, in the *same* 6-module
> `tests/registry.tcyr` unit that probe had certified, an `ImageRef` obtained from a **wrapper
> function** read back as garbage: `image_ref_new` wrote `"reg.test"` correctly, and the
> wrapper's caller saw `0@!Z`. Both the direct-return and the return-via-local wrapper forms
> failed; constructing the ref inline in the caller worked. The symptom was **not a crash** —
> it was a cache key silently built from junk, which merely made every registry request
> re-authenticate.
>
> **Two lessons, both expensive:**
> 1. **A probe certifies the exact expression it runs, in the exact function it runs in — nothing
>    more.** Same struct, same unit, different *function context* → different answer. "Green in
>    every unit shape" is not a property this bug respects.
> 2. **It fails silently far more often than it crashes.** The 6.4.76 instance segfaulted, which
>    is why it was caught in minutes. This one produced a wrong string and a working-but-wrong
>    program; it was caught only because a test built the same cache key independently and
>    compared. Prefer assertions that reconstruct a value from a second source over assertions
>    that read it back the way the code under test does — the latter matches the garbage.
>
> **Do not attempt retirement again without a test that would fail on a silent wrong value**, not
> just one that survives a segfault.
>
> ℹ️ *(Original 3.0.8 note, now known to be wrong, retained deliberately:)*
> **cycc struct-id miscompile — APPEARS FIXED at 6.4.77; workarounds still in place.**
> cycc's struct-id **20/21 ↔ SIMD f64v2/f64v4 sentinel collision** made `Image`/`Layer`/
> `Platform` field access read garbage (or **segfault** — a scalar field compiled as a vector
> load reads out of bounds) in some function contexts, and it **moved with the compilation
> unit's struct-id assignment**, so it was present in some units and absent in others.
>
> **History, because the shape of this bug matters.** 6.4.14 "fixed" it and it regressed as the
> code grew. **6.4.76 partially fixed it, which was a trap**: the 6-module
> `docs/development/cycc-repro/repro_min.tcyr` printed `MATCH` and a probe in the 26-module
> `tests/runpath.tcyr` passed, so the accessors were retired — and the suite then **SIGSEGV'd**
> in `image_store_save_archive` under the 6-module `tests/store.tcyr` unit. All of it was
> reverted. **A green probe in one unit shape proves nothing about another.**
>
> **At 6.4.77 the probe is green in every unit shape** (verified 2026-07-24, typed
> `var im: Image = p; im.id` + `Layer.digest` against raw-offset ground truth):
> `store` (6-module — the one that crashed at 6.4.76) · `registry` (6-module) ·
> `runpath` (26-module) · `stiva` (25-module). Plus `main.cyr` builds clean.
>
> **The raw-offset accessors are nonetheless STILL IN PLACE** (`_img_id` / `_img_layers` /
> `_img_manifest_digest` / `_layer_digest`, and the `load64(res + N)` reads in
> `scan_output` / `container_manager_start`). Retiring them is a real change to hot paths that
> deserves its own increment, its own full-suite run, and the *actual conversion* attempted —
> a passing probe is necessary, not sufficient; that is exactly the 6.4.76 lesson. Until then,
> keep using the accessors in the affected paths, and treat any *new* code touching these
> structs as free to use `x.field` (§B and §C already do, and are green).

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

## Modules (16 Rust modules + the net-new `imagelayout`)

| Module | Purpose |
|--------|---------|
| `image` | Substrate: image reference parsing, `Image`/`Layer`/`ImageRef` structs, content-addressable **blob store** + sha256 digests, integrity verify (the store/index/import layer moved to `imagelayout`) |
| `imagelayout` | **Net-new (v3.0.1–v3.0.4):** OCI image-layout (`oci-layout` + `index.json` + blobs), config/manifest assembly, the index.json-backed store (load/add/remove/gc/import/tag), and `oci-archive` + `docker-archive` `save`/`load` |
| `container` | Container lifecycle, state persistence, events, restart, rename, update |
| `runtime` | OCI spec, kavach integration, cgroups (CPU/mem/PID/IO), CRIU (checkpoint/pre-dump/lazy), exec, signals, export/import, copy |
| `network/` | Bridge, NAT, DNS, IP pools (IPv4+IPv6), port mapping, network policy, container DNS registry (6 submodules) |
| `storage` | Overlay FS, volume mounts, layer unpacking (gzip + zstd); **perms-preserving USTAR tar** codec (mode/uid/gid, dir/symlink, GNU longname, base-256; traversal/symlink/DoS-hardened) |
| `registry` | OCI distribution client (pull + push + chunked upload), token auth, credential store, tag listing, catalog, referrers API |
| `build` | TOML-based image builds (Stivafile), multi-stage builds, build cache |
| `ansamblu` | Multi-container orchestration, DAG ordering, rolling updates, scaling, service logs |
| `health` | Heartbeat monitoring, restart policies |
| `fleet` | Edge fleet scheduling (spread, bin-pack, pinned), health monitoring, rollback planning |
| `agent` | Daimon agent registration |
| `mcp` | 9 MCP tools with structured output, live dispatch, resource exposure |
| `convert` | Dockerfile **and** docker-compose YAML → Stivafile TOML (compose over a documented bayan-YAML subset; output is escaped, unlike the oracle) |
| `encrypted` | LUKS + dm-verity (feature-gated) |
| `intents` | Agnoshi intent stubs |
| `error` | Error types |

CLI binary: `stiva` — 35 registered verbs, **21 live** (19 at v3.0.0 + `save`/`load` at v3.0.2; `convert` gained its compose format at v3.0.6, and `logs` a `--scan` flag); the rest print a "not yet wired" message — most are **v3.0.x (planned)** blocking glue over the sync core, a small residue is **v3.1 (blocked)** (see `docs/cli.md`).

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
- **Tests + benchmarks are the way.** 1739 Cyrius tests across `tests/*.tcyr`. Keep adding — mirror the rust-old `#[cfg(test)]` cases (group A is net-new, so its tests match the OCI spec + real round-trips).
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

**1739 Cyrius tests** across `tests/*.tcyr` (stiva 664 · registry 326 · runpath 217 · store 197 ·
mgmt 219 · convert 116), each mirroring the matching rust-old `#[cfg(test)]` cases. The suite is split
across files because a monolith hits the cycc identifier-dedup cap — split by **include
set**, not by test count.

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
