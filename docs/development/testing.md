# Testing Guide

## Running Tests

```bash
# All tests (every tests/*.tcyr file)
cyrius tests tests/

# A single test file
cyrius test tests/stiva.tcyr

# Benchmarks
cyrius bench tests/stiva.bcyr
```

**1307 tests** across `tests/*.tcyr`: `stiva.tcyr` (664), `runpath.tcyr` (202),
`store.tcyr` (197), `mgmt.tcyr` (128), `convert.tcyr` (116).

> These are **assertions**, not test functions — `lib/assert.cyr` increments a
> global counter per `assert*()` call and `assert_summary()` prints it.

## Test Organization

Tests live in `tests/*.tcyr` as `fn test_*()` functions, grouped into five
files (`stiva.tcyr`, `store.tcyr`, `runpath.tcyr`, `mgmt.tcyr`, `convert.tcyr`)
that mirror the
`#[cfg(test)]` modules of the frozen `rust-old/` oracle. Each domain below maps
to its test focus:

| Module | Test focus |
|--------|-----------|
| `error` | Error-code names + exact display strings (`stiva_err_name`) |
| `image` | Reference parsing, blob store, index persistence, blob integrity verify (pull pipeline → v3.0.x (planned)) |
| `registry` | Ref/manifest parsing, `www-authenticate` parse, credential store, platform selection (blocking HTTP client → v3.0.x (planned)) |
| `container` | Lifecycle state machine, create/start/stop/remove, logging |
| `runtime` | Spec generation, resource limits, mount conversion |
| `storage` | Volume parsing, layer unpacking (real tar.gz), overlay dir structure |
| `network/pool` | IP allocation, release, exhaustion, subnet parsing |
| `network/nat` | Port spec parsing, nein rule generation |
| `network/dns` | resolv.conf parsing, DNS/hosts injection |
| `network/manager` | Network create/delete, container connect/disconnect |
| `ansamblu` | TOML parsing, DAG resolution, ServiceDef→ContainerConfig |
| `health` | Heartbeat registration, restart policies, status tracking |
| `agent` | Daimon registration record construction (live HTTP registration → v3.0.x (planned)) |
| `mcp` | Tool list + schemas, `McpResult` shape, the 2 sync tool handlers (live dispatch → v3.0.x (planned)) |
| `intents` | Externally-tagged JSON serde round-trips + strict deserialization |
| `build` | Stivafile parse, `build_cache_key` (serde-exact), cache store (layer build → v3.0.x (planned)) |
| `fleet` | Scheduling strategies (spread, binpack, pinned), node filtering |
| `encrypted` | LUKS/verity config serde, availability checks |
| `stiva_core` | `StivaConfig` defaults (the `Stiva` facade → v3.0.x (planned)) |

### Run-path + management tests (`tests/runpath.tcyr`, `tests/mgmt.tcyr`)

The suite is split into five files to stay under the cycc identifier-dedup cap.
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
| `tests/stiva.tcyr` (664) | the per-module unit tests — every ported module's `#[cfg(test)]` cases (error, oci, intents, audit, convert, network, image, registry, storage, build, runtime, container, mcp, …) |
| `tests/store.tcyr` (197) | **group A**, the store/layout/archive surface: `imagelayout` (OCI config/manifest serde, index descriptors, `_il_parse_full_ref`, platform passthrough), tar hardening (perms, long-name, base-256, traversal/symlink/DoS), `oci-archive`/`docker-archive` save+load, overlay/volume mounts, blob-store integrity. Includes only 6 `src/` modules (error, oci, image, registry, storage, imagelayout), not all 26 |
| `tests/runpath.tcyr` (202) | the synchronous **run path** + image store: `generate_spec`, `build_sandbox` (backend cascade / min-score), `exec_container`, `send_signal`, cgroup resolve/quota/limits, security scoring, and the image store round-trips (real `import`→`index.json`→reconstruct→`remove`/`gc`/`tag`); plus `scan_output` over kavach's externalization gate |
| `tests/mgmt.tcyr` (128) | orchestration/management: `ansamblu` (TOML parse, DAG ordering, rolling update, scale), `fleet` scheduling, `agent` registration records, `health` policies |
| `tests/convert.tcyr` (116) | `dockerfile_to_toml` (all instruction arms) and `compose_yaml_to_toml`: the four oracle fixtures, BTreeMap-order parity at all five sorted sites, every per-field arm, the `Ok("")` cases, one test per construct bayan rejects, and one per finding from the v3.0.6 adversarial review. Includes only 2 `src/` modules (error, convert) — the cheapest unit here (~86% of the cap) |

Run one group by grepping its `test_group("...")` label, or run everything with
`cyrius tests tests/`.

## Mock HTTP Testing — v3.0.x (planned)

Registry and daimon HTTP is folded onto the **v3.0.x line (Wave 2)** — the
blocking registry client (registry pull/push over HTTP) is buildable over the
ported sync core, just not yet wired in the Cyrius port. The mock-server tests
below come from the frozen `rust-old/` oracle and illustrate the `Stiva` facade
library API — they are **not part of the v3.0.0 Cyrius test suite**:

```rust
// v3.0.x (planned — blocking `Stiva` facade library API, not yet in the Cyrius port)
let server = MockServer::start().await;

Mock::given(method("GET"))
    .and(path("/v2/library/alpine/manifests/latest"))
    .respond_with(ResponseTemplate::new(200).set_body_raw(body, MEDIA_OCI_MANIFEST))
    .mount(&server)
    .await;

let client = RegistryClient::with_base_url(&server.uri());
```

The `RegistryClient::with_base_url()` constructor (test-only) redirects all API
calls to the mock server.

## Coverage

**Uncoverable**: Linux mount syscalls (require root), overlay mounts, veth creation, CRIU checkpoint/restore (v3.0.x (planned)), live container exec via nsenter (v3.0.x (planned)).

## Linux-Only Code

Linux-only code (overlay mounts, veth creation, bind mounts) cannot be tested without root. These paths are tested for:
- Directory creation (works without root)
- Error handling (mount failure returns proper error)
- Command construction (verify args without executing)
