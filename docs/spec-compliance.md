# Spec Compliance

Tracks external specifications stiva implements or aligns with, current conformance level, and gaps.

Last reviewed: 2026-04-02 (v2.0.0)

---

## OCI Specifications

### OCI Runtime Specification v1.2.0

- **Status**: partial
- **Implemented**:
  - Container lifecycle (create, start, stop, kill, delete)
  - Process execution with env, args, cwd, user
  - Linux namespaces (PID, mount, net, UTS, IPC, user)
  - Cgroups v2 resource limits (memory, PIDs, CPU, IO)
  - Seccomp filters (via kavach)
  - Bind mounts, volume mounts, ID-mapped mounts (`X-mount.idmap=`)
  - Signal forwarding
  - `domainname` field (v1.2.0) — wired through kavach pre_exec
  - `NO_NEW_PRIVS` enforcement (explicit prctl + seccomp)
  - FD cleanup in pre_exec (CVE-2024-21626 mitigation)
- **Gaps**:
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
  - Artifact manifests (`artifactType`, `subject` fields)
  - Non-distributable / foreign layers (external URL fetch)
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
  - Streamable HTTP transport (via bote 0.91.0)
- **Gaps**:
  - Tool `title` field — not yet in bote's ToolDef struct

---

## Container Networking

### nftables (via nein)

- **Status**: conformant
- **Implemented**:
  - Bridge networking with veth pairs
  - NAT masquerade for outbound traffic
  - DNAT port forwarding (TCP, UDP)
  - DNS injection into container rootfs
  - IP address pool management (IPv4 + IPv6 dual-stack)
  - Network policy (egress/ingress allow/deny, port restrictions)
  - Container-to-container DNS resolution (DnsRegistry)

---

## CRIU (Checkpoint/Restore)

- **Status**: conformant
- **Implemented**:
  - Checkpoint creation via `criu dump`
  - Restore via `criu restore`
  - Migration bundle packaging (config + image ref + checkpoint)
  - Pre-dump for iterative migration (`--prev-images-dir` chaining)
  - Lazy pages restore (`--lazy-pages` + `--page-server`)
