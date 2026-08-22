# Spec Compliance

Tracks external specifications stiva implements or aligns with, current conformance level, and gaps.

Last reviewed: 2026-08-21 (v3.0.18)

> **"Implemented" means the module logic exists and is tested — NOT that a container gets it.**
> Where a capability is unreachable — from the CLI, or because the value is assembled and then
> dropped at the kavach boundary — it is called out inline and listed under **Gaps**.
>
> ⚠ As of v3.0.18 those cases are: **CRIU** (not ported at all), **MCP `handle_run`**, the entire
> **bridge/NAT/DNS network stack**, **`RuntimeSpec.mounts`**, **`RuntimeSpec.namespaces`**, and
> **`process.cwd` / `process.user`**. A prior revision of this file claimed there were two. When
> you add a row here, grep for a READER before writing "Implemented" — assembling a value and
> delivering it are separate steps, and this tree has lost five of them at the same boundary.

---

## OCI Specifications

### OCI Runtime Specification v1.2.0

- **Status**: partial
- **Implemented**:
  - Container lifecycle (create, start, stop, kill, delete)
  - Process execution with **env and args** — the image's `process.env` reaches the payload via
    kavach 3.12.0's `config_env` (`src/runtime.cyr:880`, new in v3.0.18)
  - Cgroups v2 resource limits (**memory, PIDs, CPU**) — via the kavach policy
    (`src/runtime.cyr:802-810` → `lib/kavach.cyr:7767-7788`)
  - Seccomp filters (via kavach)
  - Signal forwarding
  - `NO_NEW_PRIVS` enforcement (explicit prctl + seccomp)
  - FD cleanup in pre_exec (CVE-2024-21626 mitigation)
- **Gaps**:
  - **Linux namespaces — net, mount and user only**, and derived by kavach from the policy rather
    than from the spec (`lib/kavach.cyr:8263-8277`). `RuntimeSpec.namespaces` is assembled
    (`src/runtime.cyr:516-532`) and **never read**; kavach has no UTS or IPC flag, and `NS_PID` has
    no producer, so on the `run -d` path a container shares the **host PID namespace**.
    (Foreground runs via the runc backend do get one.) Roadmap v3.1.0 item 3.
  - **Mounts** — `RuntimeSpec.mounts` is assembled (`src/runtime.cyr:501-514`, `:539`) and never
    read; `build_sandbox` does not forward it and `mount_volumes` (`src/storage.cyr:319`) has no
    caller. **No `/proc`, `/sys`, `/dev` or bind mount is established for a container**, and there
    is no `-v` flag on `stiva run`. ID-mapped mounts are parsed, never applied. Roadmap v3.1.0
    item 9.
  - **`process.cwd` and `process.user`** — `RuntimeSpec.workdir` / `.user` are assembled
    (`src/runtime.cyr:536-537`) and never read. stiva never calls kavach's `config_workdir`, and
    the OCI spec emitter hardcodes `"cwd":"/"` (`lib/kavach.cyr:8090-8091`). ⛔ **`stiva run -w
    /app` is accepted and silently ignored** (`src/main.cyr:255`, `:285`). Roadmap v3.1.0 item 10.
  - **`domainname` (v1.2.0)** — carried as far as kavach's `SandboxConfig`
    (`src/runtime.cyr:849` → `lib/kavach.cyr:3607`) and dropped there: the field has no reader
    anywhere in kavach 3.12.1, and no UTS namespace is created for it to apply into.
  - **Cgroup IO throttling** — `io.max` is written by `apply_cgroup_limits`
    (`src/runtime.cyr:689-692`), but `RuntimeSpec.io_max_bytes_per_sec` has no producer (zeroed at
    `src/runtime.cyr:129`, `:546`).
  - OCI runtime CLI conformance (`create`/`start`/`state`/`kill`/`delete` as separate binaries) — stiva uses library API
  - Intel RDT support — not applicable to current targets

### OCI Image Specification v1.1.0

- **Status**: conformant
- **Implemented**:
  - Image manifest v2 schema 2
  - Manifest list / image index (multi-arch)
  - Content-addressable blob storage (SHA-256)
  - Layer media types: gzip and zstd (`tar+gzip`, `tar+zstd`)
  - Platform selection (OS, architecture, variant)
  - Image config (env, cmd, entrypoint, user, workdir, labels)
  - Whiteouts (`.wh.<name>` / `.wh..wh..opq`) — applied at layer **MERGE** time, on the
    `flatten_layers` path only (`src/storage.cyr:1193`), which is the unprivileged fallback taken
    when the overlayfs mount fails (`src/container.cyr:1434-1449`). They cannot be applied at
    unpack, since a marker names a file in a *lower* layer (`src/storage.cyr:1068-1076`).
    ⚠ On the privileged overlayfs path the kernel handles them instead.
  - `diff_ids` computed over **uncompressed** layer bytes (distinct from the compressed layer digests in the manifest)
  - Artifact manifests (`artifactType`, `subject` fields)
  - Descriptor annotations

### OCI Distribution Specification v1.1.0

- **Status**: conformant
- **Implemented**:
  - Pull: manifest fetch (by tag and digest), blob download, token auth (Bearer)
  - Push: monolithic blob upload, chunked/resumable blob upload, manifest upload
  - Manifest digest verification (Docker-Content-Digest header)
  - Multi-arch manifest resolution
  - Docker Hub and GHCR auth flows
  - Tag listing (`/v2/{name}/tags/list`)
  - Catalog (`/v2/_catalog`)
  - Referrers API (`/v2/{name}/referrers/{digest}`)

---

## Model Context Protocol (MCP)

- **Target version**: 2025-03-26
- **Status**: conformant
- **Implemented**:
  - 9 tools: pull, run, ps, stop, exec, build, push, inspect, ansamblu
  - JSON Schema input validation per tool
  - JSON-RPC 2.0 transport (via bote)
  - Tool annotations (readOnlyHint, destructiveHint)
  - Structured tool output (`content` array with `Text` and `Resource` typed parts)
  - Live tool dispatch against running Stiva instance
  - MCP resources (`stiva://containers/{id}`, `stiva://images/{id}`)
- **Gaps**:
  - Tool `title` field — not in bote's `ToolDef` struct
  - `stiva_run` is advertised in `tool_list()` but returns an error instead of dispatching
    (`src/stiva_core.cyr:836-838`). ⚠ The blocker previously stated here — "needs the
    detached-run path to return without blocking" — is **stale**: that path landed in v3.0.14
    and already returns without blocking (`src/container.cyr:1536-1566` → `spawn_container`).
    This is unstarted stiva-side work, not a dependency.
  - **No JSON-RPC transport.** stiva builds `ToolDef`s and `McpResult`s against **bote-core
    3.3.2** and registers no `mcp` verb; the `dist/bote-core.cyr` profile it consumes contains
    no transport code at all. stdio / streamable HTTP are the consuming host's concern.

---

## Container Networking

### nftables (via nein)

- **Status**: **library-only — NOT REACHED by any container.**

> ⛔ **None of this is wired into the container path.** `network_manager_new()` has **zero
> callers** anywhere in `src/` (`src/network_manager.cyr:80`), `ContainerManager.network_manager`
> is initialised to 0 and never assigned (`src/container.cyr:1051`), and the connect/disconnect
> hook short-circuits on that zero (`src/container.cyr:1627`). `build_sandbox` additionally
> hard-disables networking outright — `SandboxPolicy_set_network_enabled(policy, 0)`
> (`src/runtime.cyr:811`). The logic below is ported and unit-tested; a running container gets
> none of it.

- **Implemented (library only)**:
  - Bridge networking with veth pairs
  - NAT masquerade for outbound traffic
  - DNAT port forwarding (TCP, UDP)
  - DNS injection into container rootfs
  - IP address pool management (IPv4 + IPv6 dual-stack)
  - Network policy (egress/ingress allow/deny, port restrictions)
  - Container-to-container DNS resolution (DnsRegistry)

---

## CRIU (Checkpoint/Restore)

- **Status**: ⛔ **NOT IMPLEMENTED.** `stiva checkpoint` and `stiva restore` are registered verbs
  that print "not wired yet" (`src/main.cyr:107-108`). Roadmap v3.1.0 item 3.

> ⚠ A prior revision of this file described this section as "ported, not reachable" and listed
> five implemented library capabilities. **None of them exist.** `src/runtime.cyr:1797` is titled
> "── NOT PORTED: CRIU (roadmap v3.1.0 item 3) ──" and calls it "the only `rust-old/src/runtime.rs`
> surface still missing"; grepping the tree for `checkpoint_container`, `pre_dump_container`,
> `restore_container` and `restore_lazy` returns **only those comment lines** — zero definitions,
> zero tests.

- **The only CRIU code in the tree**: `criu_available()`, a PATH scan for the `criu` binary
  (`src/runtime.cyr:392`).
- **Not ported** (`src/runtime.cyr:1797-1813`): `checkpoint_container`, `pre_dump_container`,
  `restore_container`, `restore_lazy` — checkpoint creation, restore, migration bundle
  packaging, pre-dump chaining, lazy-pages restore.
