# Roadmap

All planned items through P6 have been completed and shipped in v2.0.0 (2026-04-02).

See [CHANGELOG.md](../../CHANGELOG.md) for the full release notes.

## Completed (v2.0.0)

### P0 — Security
- CVE-2024-21626 mitigation (fd cleanup, stdin null, kavach pre_exec hardening)
- CVE-2024-24557 fix (tag-keyed cache removed, exact-match lookups)
- RUSTSEC-2025-0067/68 (serde_yaml replaced with serde-saphyr)
- Manifest digest verification (Docker-Content-Digest)
- NO_NEW_PRIVS enforcement in pre_exec
- O_CLOEXEC audit (fd cleanup loop covers all cases)

### P1 — OCI Spec Compliance
- Runtime-spec v1.2.0: domainname, idmap mounts, annotations
- Image-spec v1.1.0: zstd layers, artifact manifests, foreign layers, descriptor annotations
- Distribution-spec v1.1.0: chunked upload, tag listing, catalog, referrers API

### P2 — MCP & Integration
- Structured tool output (ContentPart::Text, ContentPart::Resource)
- Live tool dispatch against Stiva instance
- MCP resources (stiva:// URI scheme)
- Tool annotations (readOnlyHint, destructiveHint)

### P3 — Networking
- IPv6 dual-stack (Ipv6Pool, DualStackPool)
- Network policy (egress/ingress allow/deny)
- Container DNS resolution (DnsRegistry)

### P4 — Storage & Images
- Image garbage collection (ImageStore::gc)
- Layer build cache (content-addressable, digest-keyed)
- Multi-stage builds (BuildStage, FromStage)
- Registry credential store (~/.stiva/credentials.json)

### P5 — Runtime & Orchestration
- CPU + IO cgroup enforcement
- Container rename and live update
- Rolling updates and scaling for ansamblu
- Service logs aggregation
- Fleet health monitoring and rollback planning
- CRIU pre-dump and lazy pages

### P6 — Developer Experience
- `stiva events` (lifecycle event streaming)
- `stiva diff` (overlay upper layer inspection)
- Shell completions (bash/zsh/fish)
- `stiva rename` and `stiva gc` CLI commands
- Config file (~/.stiva/config.toml)
