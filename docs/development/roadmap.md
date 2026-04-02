# Roadmap

v2.0.0 shipped 2026-04-02 with all P0-P6 items complete.

---

## v2.1.0 — Conformance & Runtime

- [ ] OCI runtime CLI conformance — `create`/`start`/`state`/`kill`/`delete` interface for containerd/CRI drop-in
- [ ] Rootless networking — slirp4netns or pasta for unprivileged bridge networking
- [ ] `rustix` evaluation — replace `nix` with `rustix` for smaller/safer syscall wrappers (kavach + stiva)
- [ ] Registry mirror/proxy — pull-through cache for air-gapped deployments
- [ ] OCI image encryption — `ocicrypt` integration for encrypted layers
- [ ] Structured audit log — append-only log of all runtime operations for compliance

---

## v2.2.0 — Ecosystem & Scale

- [ ] Kubernetes CRI shim — minimal CRI gRPC server wrapping stiva for k8s node integration
- [ ] Metrics export — Prometheus-compatible `/metrics` endpoint
- [ ] Ansamblu blue-green deploys — deploy new version alongside old, swap traffic
- [ ] Service mesh integration — sidecar injection for ansamblu services
- [ ] Fleet auto-scaling — adjust fleet node count based on majra queue depth
- [ ] `stiva plugin` system — loadable plugins for storage drivers, network drivers, auth providers
- [ ] Windows container support — kavach backend for Windows process isolation
