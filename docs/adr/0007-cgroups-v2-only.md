# ADR-0007: Cgroups v2 only (no v1 support)

## Status
Accepted

## Context
Linux has two cgroup interfaces: v1 (hierarchical, per-controller mountpoints) and v2 (unified hierarchy). All major distributions since 2022 default to v2.

## Decision
Stiva only supports cgroups v2. All cgroup operations (resource limits, freezer, stats) read/write the unified `/sys/fs/cgroup/{path}/` hierarchy. No v1 fallback.

## Consequences
- **Positive**: Simpler code — single code path for cgroup operations.
- **Positive**: `cgroup.freeze` (v2 only) enables lightweight pause/unpause.
- **Positive**: Unified stats reading (`memory.current`, `cpu.stat`, `pids.current`).
- **Negative**: Won't work on systems running cgroups v1 only (rare as of 2026).
