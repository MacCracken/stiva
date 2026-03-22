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
