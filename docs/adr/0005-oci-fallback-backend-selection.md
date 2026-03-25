# ADR-0005: OCI → Process backend fallback (not auto-best)

## Status
Accepted

## Context
Kavach provides `Backend::resolve_best()` which picks the strongest available backend by security score. In testing, this selected `sy-agnos` (docker-based, score 80+) which was "available" (docker in PATH) but failed to create sandboxes for stiva's container model.

## Decision
Default backend selection uses a conservative OCI → Process fallback chain. Auto-selection via `resolve_best()` or strength-based selection via `min_isolation_score` are opt-in via `ContainerConfig`:
- `backend: None` → OCI if available, else Process
- `backend: Some("firecracker")` → explicit backend
- `min_isolation_score: Some(70)` → kavach picks strongest backend meeting threshold

## Consequences
- **Positive**: Predictable default behavior. No surprise backend switches.
- **Positive**: Users can opt into stronger backends when their environment supports them.
- **Negative**: Default doesn't automatically use Firecracker/gVisor even when available. Must be explicitly requested.
