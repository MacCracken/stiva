# ADR-0001: Use kavach for sandbox abstraction

## Status
Accepted

## Context
Stiva needs process isolation for container execution. Options: direct seccomp/namespace calls, OCI runtime shelling out to runc/crun, or kavach's unified sandbox API.

## Decision
Delegate all process isolation to kavach. Stiva never calls seccomp, Landlock, or namespace syscalls directly.

## Consequences
- **Positive**: Backend-agnostic — containers can run on Process, OCI, gVisor, Firecracker, WASM, SGX, SEV, or TDX without stiva code changes.
- **Positive**: Security scoring, credential injection, and output scanning come for free.
- **Positive**: Rootless containers via kavach's user namespace UID mapping.
- **Negative**: Extra dependency. kavach must be co-released with stiva.
- **Negative**: Cannot fine-tune isolation beyond what kavach exposes.
