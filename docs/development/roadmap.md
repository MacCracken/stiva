# Roadmap

v2.0.0 shipped 2026-04-02 with all P0-P6 items complete.

---

## v2.1.0 — Conformance & Runtime (shipped)

- [x] OCI runtime CLI conformance — `create`/`start`/`state`/`kill`/`delete` interface for containerd/CRI drop-in
- [x] Rootless networking — slirp4netns or pasta for unprivileged bridge networking
- [x] `rustix` evaluation — replace `nix` with `rustix` for smaller/safer syscall wrappers (stiva done, kavach pending)
- [x] Registry mirror/proxy — pull-through cache for air-gapped deployments
- [x] OCI image encryption — AES-256-GCM layer encryption/decryption (feature-gated)
- [x] Structured audit log — append-only JSON-lines log of all runtime operations for compliance

---

## v3.0.0 — Cyrius Port

Port stiva from Rust to Cyrius, following the AGNOS ecosystem port pattern
(`rust-old/` kept as the parity oracle — the bar is "matches what Rust did").
The Rust→Cyrius backlog is the GA gate for a sovereign, Cyrius-native ecosystem;
this is stiva's leg of it.

- [x] **Scaffold the port** (`cyrius port`) — Rust moved to `rust-old/` (18,622 lines, the parity oracle); Cyrius skeleton + `cyrius.cyml` + CI in place. Toolchain pinned 6.4.10.
- [x] **Foundation** — multi-module structure (kavach model): `src/lib.cyr` aggregation header, `src/main.cyr` program entry, `[lib].modules` + honest opt-in `[deps].stdlib`, `.gitignore` for `lib/`/`build/`, `tests/stiva.tcyr` harness. Green: build (no warnings) + 31 tests + bench + `fmt --check` + `lint`.
- [x] **Agent-orchestrated porting harness** — `scripts/port-workflow.js`: per-module port (rust-old oracle → `src/*.cyr`) + adversarial parity verify. Cyrius-idiom playbook embedded; batches passed as args.

Module-by-module port (bottom-up dependency order; `rust-old/src/<m>.rs` = oracle):

- [x] `error` — StivaError → STIVA_ERR_* enum + name/print (all 28 kinds, exact display strings)
- [x] `oci` — leaf surface (OciStatus, OCI_VERSION, parse_signal); OciState/to_oci_status/build_state/parse_bundle DEFERRED with `container`
- [ ] `intents`, `audit`, `convert` — dep-free, error-only coupling (first workflow batch, in flight)
- [ ] dep-free next: `agent`, `build`, `fleet`, `image`, `registry`, `storage`, `network/{bridge,dns,mod,pool,rootless}`
- [ ] dep-heavy (need AGNOS dep wiring): `runtime`+`main`→kavach · `container`→kavach/majra · `ansamblu`+`health`→majra · `network/{nat,manager}`→nein · `mcp`→bote · `encrypted`→agnodrm · `lib`→kavach/majra/nein
- [ ] **Re-wire AGNOS deps** to the Cyrius `dist/*.cyr` bundles (kavach 3.7.0, majra 2.5.0, nein 1.6.2, bote 3.0.0, agnodrm 1.4.5). Recipe derived from `dist/*.deps` sidecars; wired per-consuming-module + transitive git deps (sigil/libro/sakshi) + stdlib union. Probe-build before the first consuming module.
- [ ] **Port the `stiva` CLI** (34 subcommands) onto `src/main.cyr` (`args.cyr` dispatch)
- [ ] **Parity validation** — every module cross-checked vs Rust (the workflow's verify stage); record accepted divergences as ADRs
- [ ] **Test + benchmark parity** — port the suite (434 tests / 20 benches) and prove no regression
- [ ] **Cleanup** — `distlib` → `dist/stiva.cyr`; remove `rust-old/` after full parity (kavach precedent); version → 3.0.0; zugot recipe

---

## v3.1.0 — Agnos Container Support

Run containers on the AGNOS kernel itself (the native target, not a Linux host).
Unblocked by the agnos 1.45.x ring-3 net/socket syscall surface (#45–#57,
incl. server `sock_listen`#56 / `sock_accept`#57) — "sockets were the major
hurdle to container usage"; AGNOS is now container-network-capable.

- [ ] AGNOS kernel target — build + run stiva against the agnos syscall ABI (FS-write / exec-from-disk / sockets)
- [ ] Map kavach isolation onto the agnos sandbox primitives (no Linux namespaces/seccomp on the native target)
- [ ] Container networking over the agnos socket surface (#45–#57) — bridge/NAT/port-map without slirp/pasta
- [ ] Docker-service-sweep integration — host the server-stage workloads (agora BBS / descent MUD / web server / ark+nous server-side) in AGNOS containers
- [ ] Soak + weak-point sweep on the agnos target (connection floods, fuzzed input, resource-exhaustion incl. the kernel's 8-conn TCP / 8-listener UDP caps)

---

## Deferred — carried back from v2.1.0

Pushed back behind the Cyrius port + agnos container work; reprioritize once
v3.x lands.

- [ ] Kubernetes CRI shim — minimal CRI gRPC server wrapping stiva for k8s node integration
- [ ] Metrics export — Prometheus-compatible `/metrics` endpoint
- [ ] Ansamblu blue-green deploys — deploy new version alongside old, swap traffic
- [ ] Service mesh integration — sidecar injection for ansamblu services
- [ ] Fleet auto-scaling — adjust fleet node count based on majra queue depth
- [ ] `stiva plugin` system — loadable plugins for storage drivers, network drivers, auth providers
- [ ] Windows container support — kavach backend for Windows process isolation
</content>
</invoke>
