# Changelog

All notable changes to stiva are documented here.

## [0.21.3] — 2026-03-21

### Added
- Initial scaffold: OCI image management (reference parser, store), container lifecycle (create/start/stop/remove), runtime spec, networking (bridge/host/none/custom), storage (overlay/volumes), OCI registry client, TOML compose
- **Phase 1 — OCI Image Pull Pipeline**
  - Registry client with OCI distribution spec (manifest fetch, blob download)
  - Bearer token auth (Docker Hub, GHCR, custom registries) with scope-based caching
  - Multi-arch manifest list support with automatic platform selection
  - Content-addressable blob store (`blobs/sha256/`) with SHA-256 verification
  - Layer deduplication — skips already-present blobs on pull and resume
  - Concurrent layer downloads (4 at a time via `buffer_unordered`)
  - Image index persistence (`images.json`) with dedup-on-re-pull and GC on remove
  - 30 tests passing
- **Phase 2 — Container Execution**
  - Layer unpacking — tar+gzip decompression to content-addressable layer directories with dedup
  - Overlay filesystem — overlayfs assembly from image layers (upper/work/merged on Linux)
  - Full OCI runtime spec generation — resource limits (memory, CPU, PIDs), user, workdir, standard mounts (/proc, /sys, /dev), volume bind mounts
  - kavach sandbox integration — OCI backend (crun/runc) with Process fallback, SandboxPolicy from container config
  - Volume bind mounts — host→container bind, tmpfs, read-only support
  - Container logging — stdout/stderr capture to per-container log files
  - One-shot execution model — container runs command to completion, transitions Created→Running→Stopped
  - Overlay teardown + blob GC on container remove
  - 181 tests passing, 98%+ coverage
- **Phase 3 — Container Networking**
  - Restructured `network.rs` → `network/` submodule (pool, bridge, nat, dns, manager)
  - IP address pool — sequential allocation within CIDR subnet, release + reuse
  - Bridge + veth management — `ip` commands for bridge creation, veth pairs, netns attachment
  - NAT + port mapping via nein crate — masquerade, DNAT, port spec parsing (`8080:80/tcp`)
  - DNS injection — resolv.conf, hosts, hostname written to container rootfs
  - NetworkManager — lifecycle for named networks, container connect/disconnect, default bridge
  - 237 tests passing
