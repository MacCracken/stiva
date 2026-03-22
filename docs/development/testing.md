# Testing Guide

## Running Tests

```bash
# All tests (default features)
cargo test

# All tests (all features including compose)
cargo test --all-features

# Specific module
cargo test --all-features image::tests

# With output
cargo test --all-features -- --nocapture
```

## Test Organization

Tests live alongside the code they test (Rust convention):

| Module | Test focus |
|--------|-----------|
| `error` | Display messages, From impls, Send+Sync |
| `image` | Reference parsing, blob store, index persistence, pull pipeline (wiremock) |
| `registry` | Auth flow, manifest fetch, blob download, platform selection (wiremock) |
| `container` | Lifecycle state machine, create/start/stop/remove, logging |
| `runtime` | Spec generation, resource limits, mount conversion |
| `storage` | Volume parsing, layer unpacking (real tar.gz), overlay dir structure |
| `network/pool` | IP allocation, release, exhaustion, subnet parsing |
| `network/nat` | Port spec parsing, nein rule generation |
| `network/dns` | resolv.conf parsing, DNS/hosts injection |
| `network/manager` | Network create/delete, container connect/disconnect |
| `compose` | TOML parsing, DAG resolution, ServiceDef→ContainerConfig |
| `health` | Heartbeat registration, restart policies, status tracking |
| `agent` | Daimon HTTP registration (wiremock) |
| `mcp` | Tool list, dispatcher, parameter validation |
| `intents` | Serde round-trips for intent types |
| `lib` | Stiva facade, config serde, mock-backed pull/run |

## Mock HTTP Testing

Registry and daimon tests use [wiremock](https://crates.io/crates/wiremock) to mock HTTP servers:

```rust
let server = MockServer::start().await;

Mock::given(method("GET"))
    .and(path("/v2/library/alpine/manifests/latest"))
    .respond_with(ResponseTemplate::new(200).set_body_raw(body, MEDIA_OCI_MANIFEST))
    .mount(&server)
    .await;

let client = RegistryClient::with_base_url(&server.uri());
```

The `RegistryClient::with_base_url()` constructor (test-only, `#[cfg(test)]`) redirects all API calls to the mock server.

## Coverage

```bash
# Run coverage
cargo tarpaulin --all-features --skip-clean --out stdout

# Target: ≥94% on testable code
# Uncoverable: Linux mount syscalls, prod-only HTTPS paths
```

## Linux-Only Code

Code guarded by `#[cfg(target_os = "linux")]` (overlay mounts, veth creation, bind mounts) cannot be tested without root. These paths are tested for:
- Directory creation (works without root)
- Error handling (mount failure returns proper error)
- Command construction (verify args without executing)
