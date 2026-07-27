# Architecture

## Dependency Stack

```
stiva (this crate)
  ├── kavach     (sandbox: seccomp, Landlock, namespaces, OCI spec, gVisor, Firecracker, WASM)
  ├── majra      (job queue, heartbeat FSM, pub/sub, relay)
  ├── nein       (nftables firewall, NAT, port mapping)
  ├── bote       (MCP core service: JSON-RPC 2.0, tool registry, structured output)
  ├── agnodrm    (LUKS + dm-verity, the `encrypted` module)
  ├── cmdit      (CLI parsing, verb introspection, shell completions)
  ├── samay      (cron expressions + scheduling, the `cron` module)
  ├── ai-hwaccel (accelerator inventory + placement profiles, the `accel` feature)
  └── sakshi     (structured logging)
```

## System Diagram

```
                    ┌─────────────────────┐
                    │   Daimon (runtime)   │
                    │   Agent orchestrator │
                    └────────┬────────────┘
                             │
              ┌──────────────▼──────────────┐
              │          Stiva              │
              │   Container lifecycle       │
              │   Image pull/store/build    │
              │   Overlay FS                │
              │   OCI registry client       │
              │   Compose orchestration     │
              │   Health + restart policies │
              │   Fleet scheduling          │
              │   MCP tools + agent reg.    │
              │   Cron-scheduled containers │
              ├──────────┬─────────────────┤
              │  kavach  │     majra       │
              │ (sandbox)│  (queue/fleet)  │
              └──────────┴─────────────────┘
```

## Modules

| Module | Description |
|--------|-------------|
| `image` | Substrate: image ref parsing, `Image`/`Layer` structs, content-addressable blob store + digests, integrity verify |
| `imagelayout` | OCI image-layout (`oci-layout` + `index.json` + blobs), config/manifest assembly, the index.json-backed store, `oci-archive`/`docker-archive` save/load, the pull/push drivers (net-new) |
| `container` | The `ContainerManager`, container lifecycle, state persistence, the persisted event log, `diff` |
| `runtime` | OCI spec generation, kavach integration, cgroups v2, `exec_in_container`, output scanning, CRIU (v3.1) |
| `network` | Bridge networks, NAT, DNS, IP pools, port mapping, rootless networking (slirp4netns/pasta) |
| `storage` | Overlay filesystem, volume mounts, layer unpacking (gzip + zstd) with OCI whiteouts, perms-preserving USTAR tar codec |
| `registry` | OCI distribution client (pull + push + chunked upload), token auth, discovery |
| `build` | TOML-based image builds (Stivafile), the `build_image` driver, content-fingerprinted layer cache |
| `ansamblu` | Multi-container orchestration, DAG ordering |
| `health` | Heartbeat monitoring, restart policies |
| `cron` | Scheduled containers over samay (net-new) |
| `fleet` | Edge fleet scheduling (spread, bin-pack, pinned), accelerator-aware placement |
| `agent` | Daimon agent registration |
| `mcp` | MCP tools for AI agent integration, live dispatch over the `Stiva` facade |
| `encrypted` | LUKS + dm-verity (optional, feature-gated) |
| `intents` | Agnoshi intent stubs |
| `stiva_core` | The top-level `Stiva` facade + `StivaConfig`; MCP dispatch |
| `oci` · `audit` · `error` | OCI type substrate · audit trail · error types |

## How Stiva Completes the k8s Picture

| k8s Component | Before Stiva | After Stiva |
|---|---|---|
| Container runtime | Missing | OCI-compatible runtime |
| Pod sandbox | kavach (process-level) | kavach + stiva (full container isolation) |
| Image registry | ark packages only | OCI images + ark packages |
| Docker Compose | Not supported | `stiva ansamblu` (TOML-based) |
| Container networking | agnosys netns only | Full bridge/NAT/custom networks |
| Health/restart | Manual | Heartbeat FSM + restart policies |
| Orchestration | None | DAG-ordered ansamblu + sutra playbooks |
