# Roadmap

Tracks planned work for stiva, organized by priority and domain.

Last updated: 2026-04-02

---

## P0 — Security

### kavach fd hardening (upstream)

CVE-2024-21626 is mitigated in stiva's `exec_in_container()` but kavach still has gaps:

- [ ] `execute_with_timeout()` — add `stdin(Stdio::null())` to prevent stdin fd inheritance
- [ ] `spawn_process()` — add `stdin(Stdio::null())` for daemon containers
- [ ] `build_command()` pre_exec hook — add fd cleanup (`close(3..N)`) before exec
- [ ] Set `O_CLOEXEC` on all file operations in kavach (audit log, namespace writes)

### Manifest digest verification

- [ ] Verify computed digest of fetched manifest matches the registry-provided `Docker-Content-Digest` header (defense-in-depth for registry MITM)

---

## P1 — OCI Spec Compliance

### Runtime spec v1.2.0

- [x] `domainname` field — added to `ContainerConfig` and `RuntimeSpec`
- [ ] `idmap` mount option — user namespace ID-mapped mounts for rootless containers
- [ ] Container/process annotations — propagate OCI annotations through to kavach
- [ ] Verify `NO_NEW_PRIVS` enforcement — confirm kavach sets `PR_SET_NO_NEW_PRIVS` in pre_exec

### Image spec v1.1.0

- [ ] `zstd` layer compression — support `application/vnd.oci.image.layer.v1.tar+zstd` in pull, push, and build
- [ ] Artifact support — OCI artifact manifest type for non-container content
- [ ] Non-distributable / foreign layers — handle layers with external URLs

### Distribution spec v1.1.0

- [ ] Chunked/resumable blob upload — `POST`+`PATCH`+`PUT` flow for large layers
- [ ] Referrers API — `GET /v2/<name>/referrers/<digest>` for artifact discovery
- [ ] Tag listing — `GET /v2/<name>/tags/list` with pagination
- [ ] Catalog endpoint — `GET /v2/_catalog` with pagination

---

## P2 — MCP & Integration

### MCP 2025-03-26

- [x] Tool annotations (readOnlyHint, destructiveHint) — added to all 9 tools
- [ ] Structured tool output — return `content` array with typed parts (`text`, `image`, `resource`) instead of flat JSON
- [ ] Streamable HTTP transport — verify bote 0.91.0 supports streamable HTTP, wire up if available
- [ ] Tool `title` annotation — add human-readable display names to all tools

### Daimon integration

- [ ] Live `handle_tool` wiring — connect MCP handlers to actual `Stiva` instance (currently returns stubs)
- [ ] MCP resource exposure — expose container logs, stats, and image list as MCP resources

---

## P3 — Networking

- [ ] IPv6 support — dual-stack IP pool, IPv6 NAT/masquerade via nein
- [ ] CNI plugin interface — optional CNI compatibility layer for external network plugins
- [ ] Network policy — per-container egress/ingress rules via nein
- [ ] DNS resolution — container-to-container DNS within ansamblu sessions

---

## P4 — Storage & Images

- [ ] Layer build cache — content-addressable build step cache keyed by (base image digest + step hash), with digest verification on reuse
- [ ] Image garbage collection — reference-counted blob cleanup for orphaned layers
- [ ] Registry credential store — persistent credential storage (not just per-session tokens)
- [ ] Multi-stage builds — `FROM ... AS builder` equivalent in Stivafile

---

## P5 — Runtime & Orchestration

### Container runtime

- [ ] CPU cgroup enforcement — write `cpu.max` in `apply_cgroup_limits()` (currently only memory + PIDs)
- [ ] IO cgroup limits — `io.max` for disk throughput control
- [ ] `domainname` wiring — pass `ContainerConfig.domainname` through kavach to set UTS domain name in container
- [ ] Container rename — `stiva rename <id> <new-name>`
- [ ] Container update — live resource limit changes on running containers

### CRIU

- [ ] Pre-dump for iterative migration — reduce downtime for large-memory containers
- [ ] Lazy migration / page server — on-demand page transfer during restore

### Ansamblu orchestration

- [ ] Rolling update — replace service replicas one at a time with health check gates
- [ ] Scale command — `stiva ansamblu scale <service> <count>` for runtime replica adjustment
- [ ] Service logs — aggregate logs across replicas

### Fleet

- [ ] Fleet health monitoring — heartbeat-based node health with automatic rescheduling
- [ ] Deployment rollback — revert to previous deployment version

---

## P6 — Developer Experience

- [ ] `stiva events` — stream lifecycle events to terminal
- [ ] `stiva diff` — show filesystem changes in a container vs its image
- [ ] Shell completions — bash/zsh/fish completions for all 28 CLI commands
- [ ] Config file — `~/.stiva/config.toml` for default registry, storage path, log level

---

## Done (1.1.0)

- [x] Dependency updates — bote 0.91.0, majra 1.0.4, 34 transitive updates
- [x] CVE-2024-21626 mitigation — fd cleanup in `exec_in_container()`
- [x] CVE-2024-24557 fix — removed tag-keyed manifest cache, exact-match lookups
- [x] RUSTSEC-2025-0067/0068 — `serde_yaml` → `serde-saphyr`
- [x] MCP annotations — all 9 tools annotated
- [x] OCI runtime-spec v1.2.0 `domainname` field
- [x] SPDX license fix — `GPL-3.0` → `GPL-3.0-or-later`
- [x] Security audit log — `docs/security-audit-log.md`
- [x] Spec compliance tracker — `docs/spec-compliance.md`
