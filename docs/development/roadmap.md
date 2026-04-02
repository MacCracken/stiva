# Roadmap

Tracks planned work for stiva, organized by priority and domain.

Last updated: 2026-04-02

---

## P1 — OCI Spec Compliance

### Runtime spec v1.2.0

- [ ] `idmap` mount option — user namespace ID-mapped mounts for rootless containers

### Image spec v1.1.0

- [ ] Artifact support — OCI artifact manifest type for non-container content
- [ ] Non-distributable / foreign layers — handle layers with external URLs

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

- [ ] IO cgroup limits — `io.max` for disk throughput control
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
