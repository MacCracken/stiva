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

**1184 tests** across `tests/*.tcyr`: `stiva.tcyr` (869), `runpath.tcyr` (187),
`mgmt.tcyr` (128).

## Test Organization

Tests live in `tests/*.tcyr` as `fn test_*()` functions, grouped into three
files (`stiva.tcyr`, `runpath.tcyr`, `mgmt.tcyr`) that mirror the `#[cfg(test)]`
modules of the frozen `rust-old/` oracle. Each domain below maps to its test
focus:

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

The suite is split into three files to stay under the cycc identifier-dedup cap:

| File | Focus |
|------|-------|
| `tests/stiva.tcyr` (869) | the per-module unit tests — every ported module's `#[cfg(test)]` cases (error, oci, intents, audit, convert, network, image, registry, storage, build, runtime, container, mcp, …) **plus group A**: `imagelayout` (OCI config/manifest serde, index descriptors, `_il_parse_full_ref`, platform passthrough), tar hardening (perms, long-name, base-256, traversal/symlink/DoS), and `oci-archive`/`docker-archive` load |
| `tests/runpath.tcyr` (187) | the synchronous **run path** + image store: `generate_spec`, `build_sandbox` (backend cascade / min-score), `exec_container`, `send_signal`, cgroup resolve/quota/limits, security scoring, and the image store round-trips (real `import`→`index.json`→reconstruct→`remove`/`gc`/`tag`) |
| `tests/mgmt.tcyr` (128) | orchestration/management: `ansamblu` (TOML parse, DAG ordering, rolling update, scale), `fleet` scheduling, `agent` registration records, `health` policies |

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
