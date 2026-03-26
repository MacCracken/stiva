# Architecture

## Dependency Stack

```
stiva (this crate)
  ├── kavach (sandbox: seccomp, Landlock, namespaces, OCI spec, gVisor, Firecracker, WASM)
  ├── majra (job queue, heartbeat FSM, pub/sub, relay)
  └── nein (nftables firewall, NAT, port mapping)
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
              ├──────────┬─────────────────┤
              │  kavach  │     majra       │
              │ (sandbox)│  (queue/fleet)  │
              └──────────┴─────────────────┘
```

## Modules

| Module | Description |
|--------|-------------|
| `image` | OCI image pull, push, build, store, layer management |
| `container` | Container lifecycle, state persistence, events |
| `runtime` | OCI spec generation, kavach integration, cgroups, CRIU |
| `network` | Bridge networks, NAT, DNS, IP pools, port mapping |
| `storage` | Overlay filesystem, volume mounts, layer unpacking |
| `registry` | OCI distribution client (pull + push), token auth |
| `build` | TOML-based image builds (Stivafile) |
| `ansamblu` | Multi-container orchestration, DAG ordering |
| `health` | Heartbeat monitoring, restart policies |
| `fleet` | Edge fleet scheduling (spread, bin-pack, pinned) |
| `agent` | Daimon agent registration |
| `mcp` | MCP tools for AI agent integration |
| `encrypted` | LUKS + dm-verity (optional, feature-gated) |
| `intents` | Agnoshi intent stubs |

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
