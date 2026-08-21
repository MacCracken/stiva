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

- The **Cyrius toolchain** (`cyrius`), pinned to **6.5.33** (see `cyrius.cyml`).
- Nothing else. AGNOS dependencies are resolved **by git tag** from
  `[deps.*]` in `cyrius.cyml` — `kavach`, `majra`, `nein`, `bote`, `agnodrm`,
  `cmdit`, `samay`, `ai-hwaccel`, `sakshi`, `libro` — so `cyrius deps` works on a
  clean machine with no sibling checkouts.

> ⚠️ **Do not commit a `cyrius.lock` produced through a `path` override.** Every
> `path = "../<dep>"` line in `cyrius.cyml` is commented out deliberately: a path
> override silently **wins** over its `tag`, and the lock records no dep name or
> version under path resolution, so nothing detects the substitution. To develop a
> dep alongside stiva, uncomment its `path` line, work, then **tag and release the
> dep and bump the `tag` here** before committing the lock. CI compares the resolved
> dep bundles against the tags and fails on a mismatch.

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
- **Tested**: new code must include tests in the matching `tests/*.tcyr` file, and new
  CLI surface must be covered in `scripts/cli-smoke.sh` (a `.tcyr` file cannot include
  `main.cyr`). Each ported function mirrors its rust-old `#[cfg(test)]` cases — the bar is
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

### Three traps specific to this codebase

- **Every deferral marker in the source is tracked in the roadmap.** `cyrius lint` reports
  untracked `TODO` / `deferred` / `not yet` comments per file, and the count is expected to
  stay at **zero**. If you add one, add the matching roadmap item in the same change — or
  delete the comment if it is stale.
- **CLI verb ids are registration-ordered.** `cmdit_verb(h, ...)` returns an id derived from
  registration order and the dispatch table is indexed by it, so inserting a verb anywhere
  but the **end** of `src/main.cyr` silently renumbers every verb after it. `cli-smoke.sh` is
  what catches this.
- **The cycc struct-id ↔ SIMD-sentinel miscompile was last verified live at 6.4.78 and has
  not been re-verified at the 6.5.33 pin — assume live.** A typed
  `var x: T = p; x.field` can read garbage **silently**, and it varies per function *and* per
  compilation unit. The raw-offset accessors in hot paths (`_img_id`, `_layer_digest`,
  `load64(p + N)`, …) are deliberate workarounds — do not "clean them up". When a test needs
  to verify a struct field, reconstruct the value from a second source rather than reading it
  back the way the code under test does; the naive read matches the garbage and passes.

## Scripts

| Script | Usage |
|--------|-------|
| `scripts/version-bump.sh <version>` | Write the `VERSION` file (cyrius.cyml reads it via `${file:VERSION}`) |
| `scripts/bench.sh` | Test + build timing history |
| `scripts/bench-history.sh` | Benchmarks + CSV + trend report |
| `scripts/cli-smoke.sh` | CLI smoke assertions against the built binary (the only coverage `src/main.cyr` has) |
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
| `image` | Substrate: image ref parsing, `Image`/`Layer` structs, content-addressable **blob store** + digests, integrity verify (the OCI-layout store + `save`/`load` live in `imagelayout`) |
| `imagelayout` | **Net-new:** OCI image-layout (`oci-layout` + `index.json` + blobs), config/manifest assembly, the index.json-backed store, `oci-archive`/`docker-archive` `save`/`load`, the `image_store_pull`/`push` drivers |
| `container` | The `ContainerManager`, container lifecycle + state persistence, the `{root}/events.jsonl` log, `diff` |
| `runtime` | OCI runtime spec, kavach sandbox, cgroups v2, /proc walk, host↔rootfs copy, `exec_in_container`, `scan_output` (CRIU → v3.1) |
| `network/` | Bridge, NAT, DNS, IP pools (v4+v6), port mapping, network policy, rootless networking (slirp4netns/pasta) |
| `storage` | Overlay FS, volume mounts, gzip + zstd layer unpack with OCI whiteouts; **perms-preserving USTAR tar** (mode/uid/gid, dir/symlink, GNU longname, base-256; hardened) |
| `registry` | OCI distribution client: ref/manifest parsing + serde, credential store, token auth, streamed blob fetch, chunked upload, discovery |
| `build` | Stivafile TOML parse, build-cache key + source fingerprint, the `build_image` driver (`run` / `from_stage` steps → v3.3) |
| `ansamblu` | TOML ansamblu parse, DAG ordering, rolling-update / scaling logic |
| `health` | Restart policies, heartbeat health via majra |
| `cron` | **Net-new:** scheduled containers over samay — `{root}/cron.json`, expression validation, due-time computation |
| `fleet` | Fleet scheduling, health monitoring, rollback planning, accelerator-aware placement |
| `agent` | Daimon agent registration |
| `mcp` | 9 MCP tool defs + live dispatch over the `Stiva` facade |
| `convert` | Dockerfile **and** docker-compose YAML → Stivafile TOML |
| `encrypted` | LUKS + dm-verity (agnodrm) |
| `intents` | Intent value type + serde (the NL `parse_intent` → agnoshi) |
| `error` | Error types |

Dependencies on sibling AGNOS projects (Cyrius `dist/*.cyr` bundles):
- **kavach** — sandbox execution (process isolation, OCI backend, seccomp, Landlock)
- **majra** — DAG scheduling, heartbeat health tracking, pub/sub events
- **nein** — nftables firewall rules, NAT, port mapping
- **bote** — MCP core service (tool registry, structured output)
- **agnodrm** — LUKS + dm-verity for the `encrypted` module
- **cmdit** — CLI parsing, verb introspection, shell completions
- **samay** — cron expression parsing + scheduling for the `cron` module
- **ai-hwaccel** — accelerator inventory + placement profiles (the `accel` feature, on by default)
