# Spec Compliance

Tracks external specifications stiva implements or aligns with, current conformance level, and gaps.

Last reviewed: 2026-11-06 (v3.1.2)

> **"Implemented" means the module logic exists and is tested — NOT that a container gets it.**
> Where a capability is unreachable — from the CLI, or because the value is assembled and then
> dropped at the kavach boundary — it is called out inline and listed under **Gaps**.
>
> ⚠ As of v3.1.2 those cases are: **CRIU** (not ported at all), **MCP `handle_run`**, the entire
> **bridge/NAT/DNS network stack**, **`RuntimeSpec.mounts`**, **`RuntimeSpec.namespaces`**,
> **`process.user`**, and **the image's own config**. When you add a row here, grep for a READER
> before writing "Implemented": assembling a value and delivering it are separate steps.

---

## OCI Specifications

### OCI Runtime Specification v1.2.0

- **Status**: partial
- **Implemented**:
  - Container lifecycle (create, start, stop, kill, delete)
  - Process execution with args, and `env` from the container's config through kavach's
    `config_env`. ⛔ The **image's** `Env` is not applied (roadmap v3.2.0).
  - Cgroups v2 resource limits (**memory, PIDs, CPU**) through the kavach policy, though no CLI flag
    sets one (roadmap v3.2.2)
  - Seccomp filters and `NO_NEW_PRIVS` on `run`, on both backends, and on `run -d`. ⛔ `exec` gets
    `NO_NEW_PRIVS` alone, and the OCI backend's spec keeps every capability but CAP_SYS_ADMIN and
    sets no `maskedPaths` or `readonlyPaths`. See SECURITY.md.
  - Signal forwarding
  - FD cleanup before exec (CVE-2024-21626 mitigation)
- **Gaps**:
  - **Linux namespaces.** `RuntimeSpec.namespaces` is assembled and **never read**. The OCI backend
    takes pid, ipc, uts, mount and user from kavach's spec emitter, and the process backend and
    `run -d` get net, mount and user, so on `run -d` a container shares the **host PID namespace**.
    Roadmap v3.4.1.
  - **Mounts.** `RuntimeSpec.mounts` is assembled and never read, and `mount_volumes` has no caller.
    The OCI backend mounts `/proc` and a `/dev` tmpfs itself; on `run -d` **no `/proc`, `/sys`, `/dev`
    or bind mount is established**. There is no `-v` flag on `stiva run`. ID-mapped mounts are parsed,
    never applied. Roadmap v3.2.2.
  - **`process.user`.** `RuntimeSpec.user` is assembled and never read. Roadmap v3.2.2.
    (`process.cwd` works: `stiva run -w /app` is honoured on every backend.)
  - **`domainname` (v1.2.0)** — carried as far as kavach's `SandboxConfig` and dropped there.
  - **Cgroup IO throttling** — `io.max` is written by `apply_cgroup_limits`, but
    `RuntimeSpec.io_max_bytes_per_sec` has no producer.
  - OCI runtime CLI conformance (`create`/`start`/`state`/`kill`/`delete` as separate binaries) — stiva uses library API
  - Intel RDT support — not applicable to current targets

### OCI Image Specification v1.1.0

- **Status**: conformant for storage and transfer. ⛔ **An image's config is stored, not applied.**
- **Implemented**:
  - Image manifest v2 schema 2
  - Manifest list / image index (multi-arch)
  - Content-addressable blob storage (SHA-256)
  - Layer media types: gzip and zstd (`tar+gzip`, `tar+zstd`)
  - Platform selection (OS, architecture, variant)
  - Image config (env, cmd, entrypoint, user, workdir, labels), assembled on build and stored on
    pull. Nothing reads it when a container starts: `Entrypoint`, `Cmd`, `Env`, `WorkingDir` and
    `User` are ignored (roadmap v3.2.0).
  - Whiteouts (`.wh.<name>` / `.wh..wh..opq`) — applied at layer **MERGE** time, on the
    `flatten_layers` path only, which is the unprivileged fallback taken when the overlayfs mount
    fails. They cannot be applied at unpack, since a marker names a file in a *lower* layer.
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
- **Status**: partial. ⛔ **No client can reach any of this yet** (roadmap v3.3.3).
- **Implemented**:
  - 9 tool definitions: pull, run, ps, stop, exec, build, push, inspect, ansamblu
  - JSON Schema input validation per tool
  - Tool annotations (readOnlyHint, destructiveHint, idempotentHint, openWorldHint)
  - Structured tool output (`content` array with `Text` and `Resource` typed parts)
  - Dispatch against a `Stiva` instance, for 7 of the 9 tools
  - MCP resources (`stiva://containers/{id}`, `stiva://images/{id}`)
- **Gaps**:
  - **No transport.** There is no `mcp` verb, and `mcp_handle_tool` has no caller in `src/`.
    The `dist/bote-core.cyr` profile stiva consumes contains no transport code.
  - `stiva_run` is advertised and returns an error. This is unstarted work, not a dependency.
  - `stiva_ansamblu` reports success and does nothing: `up` answers `parsed`, `down` answers `queued`.
  - Tool `title` field — not in bote's `ToolDef` struct

---

## Container Networking

### nftables (via nein)

- **Status**: **library-only — NOT REACHED by any container.**

> ⛔ **None of this is wired into the container path.** `network_manager_new()` has no callers in
> `src/`, `ContainerManager.network_manager` is never assigned, and `build_sandbox` hard-disables
> networking with `SandboxPolicy_set_network_enabled(policy, 0)`. The logic below is ported and
> unit-tested; a running container gets none of it. Roadmap v3.3.1.

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
  that print "not wired yet". Roadmap v3.4.1.
- **The only CRIU code in the tree**: `criu_available()`, a PATH scan for the `criu` binary.
- **Not ported**: `checkpoint_container`, `pre_dump_container`, `restore_container`, `restore_lazy`.
