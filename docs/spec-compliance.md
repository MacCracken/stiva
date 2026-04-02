# Spec Compliance

Tracks external specifications stiva implements or aligns with, current conformance level, and gaps.

## Format

Each entry records:
- **Spec** and version targeted
- **Status**: `conformant`, `partial`, `planned`
- **Gaps**: known missing or incomplete areas
- **Last reviewed**: date of most recent audit

---

## OCI Specifications

### OCI Runtime Specification

- **Spec**: [opencontainers/runtime-spec](https://github.com/opencontainers/runtime-spec)
- **Target version**: v1.2.0
- **Status**: partial
- **Last reviewed**: 2026-04-02
- **Implemented**:
  - Container lifecycle (create, start, stop, kill, delete)
  - Process execution with env, args, cwd, user
  - Linux namespaces (PID, mount, net, UTS, IPC, user)
  - Cgroups v2 resource limits (memory, PIDs)
  - Seccomp filters (via kavach)
  - Bind mounts, volume mounts
  - Signal forwarding
  - Hooks: none (kavach handles pre/post-start internally)
- **Gaps**:
  - `domainname` field (added in v1.2.0) — added to `RuntimeSpec` and `ContainerConfig` (2026-04-02)
  - `idmap` mount option (v1.2.0) — not yet supported
  - Intel RDT support (v1.2.0) — not applicable to current targets
  - OCI runtime CLI conformance (`create`/`start`/`state`/`kill`/`delete` as separate binaries) — stiva uses library API, not the OCI runtime CLI interface
  - Annotations on container/process — not yet propagated
  - Personality / `NO_NEW_PRIVS` — delegated to kavach, needs verification

### OCI Image Specification

- **Spec**: [opencontainers/image-spec](https://github.com/opencontainers/image-spec)
- **Target version**: v1.1.0
- **Status**: partial
- **Last reviewed**: 2026-04-02
- **Implemented**:
  - Image manifest v2 schema 2
  - Manifest list / image index (multi-arch)
  - Content-addressable blob storage (SHA-256)
  - Layer media types: `application/vnd.oci.image.layer.v1.tar+gzip`
  - Platform selection (OS, architecture, variant)
  - Image config (env, cmd, entrypoint, user, workdir, labels)
- **Gaps**:
  - `zstd` layer compression — not yet supported (only gzip)
  - Artifact support (v1.1.0) — not implemented
  - Non-distributable / foreign layers — not handled
  - Image encryption (via `ocicrypt`) — not implemented (stiva uses agnosys LUKS at storage level instead)

### OCI Distribution Specification

- **Spec**: [opencontainers/distribution-spec](https://github.com/opencontainers/distribution-spec)
- **Target version**: v1.1.0
- **Status**: partial
- **Last reviewed**: 2026-04-02
- **Implemented**:
  - Pull: manifest fetch (by tag and digest), blob download, token auth (Bearer)
  - Push: blob upload (monolithic), manifest upload, blob existence check
  - Multi-arch manifest resolution
  - Docker Hub and GHCR auth flows
- **Gaps**:
  - Chunked/resumable blob upload — not implemented
  - Referrers API (v1.1.0) — not implemented
  - Content discovery / tag listing — not implemented
  - Pagination on catalog/tag endpoints — not implemented

---

## Model Context Protocol (MCP)

- **Spec**: [modelcontextprotocol.io](https://modelcontextprotocol.io)
- **Target version**: 2025-03-26
- **Status**: partial
- **Last reviewed**: 2026-04-02
- **Implemented**:
  - 9 tools: pull, run, ps, stop, exec, build, push, inspect, ansamblu
  - JSON Schema input validation per tool
  - JSON-RPC 2.0 transport (via bote)
- **Gaps**:
  - Tool `annotations` (readOnlyHint, destructiveHint) — added to all 9 tools (2026-04-02)
  - Structured tool output (`content` array with typed parts) — not yet adopted
  - Streamable HTTP transport — using bote's transport, needs version check

---

## Container Networking

### nftables (via nein)

- **Status**: conformant
- **Last reviewed**: 2026-04-02
- **Implemented**:
  - Bridge networking with veth pairs
  - NAT masquerade for outbound traffic
  - DNAT port forwarding (TCP, UDP)
  - DNS injection into container rootfs
  - IP address pool management (per-network CIDR allocation)
- **Gaps**:
  - CNI plugin interface — not implemented (stiva manages networking directly via nein)
  - IPv6 — not yet supported in IP pool

---

## CRIU (Checkpoint/Restore)

- **Spec**: [criu.org](https://criu.org)
- **Status**: partial
- **Last reviewed**: 2026-04-02
- **Implemented**:
  - Checkpoint creation via `criu dump`
  - Restore via `criu restore`
  - Migration bundle packaging (config + image ref + checkpoint)
- **Gaps**:
  - Lazy migration / page server — not implemented
  - Pre-dump for iterative migration — not implemented
