# ADR-0010: Content-addressable build cache

## Status
Accepted

## Context
Building images from a Stivafile re-executes every step on each build, even when base image and step definitions have not changed. For multi-step builds with expensive `run` commands (package installation, compilation), this wastes significant time.

## Decision
Cache build steps using a content-addressable key: `sha256(base_digest + step_index + step_json)`. Before executing a step, the builder computes the cache key from the running layer digest, step position, and serialized step definition. If a cached digest exists at `{image_root}/cache/{key}`, the builder reuses the cached layer instead of re-executing the step.

Cache entries are written after each successful step via `record_build_cache`. The running digest is updated after each step, so any change to a step (or any preceding step) produces a different key and invalidates all downstream cache entries automatically.

Cache files are stored under the image store's `cache/` directory, one file per cache key containing the resulting layer digest.

## Consequences
- **Positive**: Repeated builds with unchanged steps complete in milliseconds instead of seconds.
- **Positive**: Cache invalidation is automatic -- changing a step or its predecessors produces new keys.
- **Positive**: No separate garbage collection needed; `stiva prune` can clean the cache directory.
- **Negative**: Cache directory grows with each unique build step. Large projects may accumulate stale entries.
- **Negative**: Cache is local to the image store; no shared cache across machines.
