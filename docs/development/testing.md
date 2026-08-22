# Testing Guide

## Running Tests

```bash
# All tests (every tests/*.tcyr file)
cyrius tests tests/

# A single test file
cyrius test tests/stiva.tcyr

# CLI smoke assertions against the built binary
./scripts/cli-smoke.sh

# Benchmarks
cyrius bench tests/stiva.bcyr
```

**2183 tests** across `tests/*.tcyr`: `stiva.tcyr` (667), `registry.tcyr` (421),
`runpath.tcyr` (347), `mgmt.tcyr` (325), `store.tcyr` (299), `convert.tcyr` (116).
Plus **87** CLI smoke assertions and **14** benchmarks.

> These are **assertions**, not test functions — `lib/assert.cyr` increments a
> global counter per `assert*()` call and `assert_summary()` prints it.

## Test Organization

Tests live in `tests/*.tcyr` as `fn test_*()` functions, grouped into six files
that mirror the `#[cfg(test)]` modules of the frozen `rust-old/` oracle where one
exists (group A, `cron`, whiteouts and `diff` are net-new, so their oracle is the
OCI image-spec and real round-trips). Each domain below maps to its test focus:

| Module | Test focus |
|--------|-----------|
| `error` | Error-code names + exact display strings (`stiva_err_name`) |
| `image` | Reference parsing, blob store, index persistence, blob integrity verify |
| `imagelayout` | OCI config/manifest serde, index descriptors, `_il_parse_full_ref`, platform passthrough, `image_store_pull`/`push` drivers |
| `registry` | Ref/manifest parsing, `www-authenticate` parse, token cache, credential store, platform selection, chunked upload, discovery (tags/catalog/referrers) |
| `container` | Lifecycle state machine, create/start/stop/remove, logging, the event log, `diff` |
| `runtime` | Spec generation, resource limits, mount conversion, `exec_in_container`, `scan_output` |
| `storage` | Volume parsing, layer unpacking (real tar.gz + zstd), OCI whiteouts, overlay dir structure |
| `network/pool` | IP allocation, release, exhaustion, subnet parsing |
| `network/nat` | Port spec parsing, nein rule generation |
| `network/dns` | resolv.conf parsing, DNS/hosts injection |
| `network/manager` | Network create/delete, container connect/disconnect |
| `network/rootless` | slirp4netns/pasta argv construction, port-forward requests |
| `ansamblu` | TOML parsing, DAG resolution, ServiceDef→ContainerConfig |
| `health` | Heartbeat registration, restart policies, status tracking |
| `cron` | Expression validation (including the `Result`-vs-pointer trap), store round-trips, `cron_due_at` under an injected clock |
| `agent` | Daimon registration record construction |
| `mcp` | Tool list + schemas, `McpResult` shape, the sync tool handlers |
| `intents` | Externally-tagged JSON serde round-trips + strict deserialization |
| `build` | Stivafile parse, `build_cache_key` (serde-exact), source fingerprinting, cache store, the `build_image` driver |
| `fleet` | Scheduling strategies (spread, binpack, pinned), node filtering, accelerator constraints |
| `encrypted` | LUKS/verity config serde, availability checks |
| `stiva_core` | `StivaConfig` defaults, the `Stiva` facade, MCP live dispatch |

### Why the suite is split across files

The suite is split into six files to stay under the cycc identifier-dedup cap.
The binding constraint is the **set of `src/` modules a file includes**, plus the
vendored `lib/` bundles that cyrius auto-prepends to every compilation unit —
*not* the number of tests. Peeling test bodies alone does not move the cap:
identifiers dedupe, and a test only references symbols its modules already
define. Split by **include set**. (Measured 2026-07-22: moving 821 lines / 39
test functions out of `stiva.tcyr` freed 0 bytes; dropping one `include` freed 4.
Refined 2026-07-23: test bodies are cheap but not *free* — `stiva.tcyr` and
`mgmt.tcyr` share an include set yet differ by ~32 identifier bytes and ~1
`fn_table` slot per test function, and `fn_table` is the tighter cap when adding
many small tests.)

| File | Focus |
|------|-------|
| `tests/stiva.tcyr` (667) | the per-module unit tests — every ported module's `#[cfg(test)]` cases (error, oci, intents, audit, convert, network, image, registry, storage, build, runtime, container, mcp, …) |
| `tests/registry.tcyr` (421) | the registry client: auth challenge/token cache, manifest resolution, streamed blob fetch with digest verification, blob upload + manifest PUT, discovery. Includes 6 `src/` modules |
| `tests/runpath.tcyr` (355) | the synchronous **run path** + image store: `generate_spec`, `build_sandbox` (backend cascade / min-score), `exec_container`, `exec_in_container`, `send_signal`, cgroup resolve/quota/limits, security scoring, image store round-trips (real `import`→`index.json`→reconstruct→`remove`/`gc`/`tag`), `scan_output` over kavach's externalization gate, and `container_manager_diff` |
| `tests/mgmt.tcyr` (325) | orchestration/management: `ansamblu` (TOML parse, DAG ordering, rolling update, scale), `fleet` scheduling + accelerator placement, `cron`, `agent` registration records, `health` policies |
| `tests/store.tcyr` (299) | **group A**, the store/layout/archive surface: `imagelayout` (OCI config/manifest serde, index descriptors, `_il_parse_full_ref`, platform passthrough), tar hardening (perms, long-name, base-256, traversal/symlink/DoS), OCI whiteouts, `oci-archive`/`docker-archive` save+load, overlay/volume mounts, blob-store integrity. Includes only 6 `src/` modules (error, oci, image, registry, storage, imagelayout) |
| `tests/convert.tcyr` (116) | `dockerfile_to_toml` (all instruction arms) and `compose_yaml_to_toml`: the four oracle fixtures, BTreeMap-order parity at all five sorted sites, every per-field arm, the `Ok("")` cases, one test per construct bayan rejects, and one per finding from the v3.0.6 adversarial review. Includes only 2 `src/` modules (error, convert) — the cheapest unit here |

Run one group by grepping its `test_group("...")` label, or run everything with
`cyrius tests tests/`.

## Testing `src/main.cyr`

A `.tcyr` file cannot include `main.cyr` — it has its own entry point. CLI coverage
therefore lives in `./scripts/cli-smoke.sh`, which drives the **built binary** and
asserts on its stdout, stderr and exit code (87 assertions). Anything that only
exists in `main.cyr` — flag registration, verb dispatch, usage strings, exit codes —
is covered there or not at all.

**Verb ids are registration-ordered.** A verb inserted anywhere but the end of
`main.cyr`'s registration list silently renumbers every verb after it, and the smoke
script is what catches this — it is how `stiva cron ls` dispatching to `completions`
was found.

## Writing tests under the cycc struct-id miscompile

The struct-id 20/21 ↔ SIMD-sentinel miscompile was **last verified live at 6.4.78 and has
not been re-verified at the current 6.5.33 pin — assume live**: a typed
`var x: T = p; x.field` can silently read garbage, and it varies per function *and*
per compilation unit. Two consequences for tests:

- **A probe certifies the exact expression it runs, in the exact function it runs in.**
  Green in one unit shape proves nothing about another.
- **Prefer assertions that reconstruct a value from a second source** over assertions
  that read it back the way the code under test does — the latter reads the same
  garbage and passes.

Note also that stdout is buffered: a test-group header can flush while later
assertions have not, which makes "the last line printed" a misleading indicator of
where a crash happened.

## Coverage

**Uncoverable without root**: Linux mount syscalls, overlay mounts, veth creation,
live namespace entry. **Not yet implemented**: CRIU checkpoint/restore (roadmap v3.1.0
item 3).

## Linux-Only Code

Linux-only code (overlay mounts, veth creation, bind mounts) cannot be tested without root. These paths are tested for:
- Directory creation (works without root)
- Error handling (mount failure returns proper error)
- Command construction (verify args without executing)
