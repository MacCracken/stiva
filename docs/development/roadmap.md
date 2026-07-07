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
- [x] `intents` — IntentKind/AnsambluAction enums + serde-tag names + not-implemented `parse_intent` sentinel; variant payloads DEFERRED with a real agnoshi NL parser
- [x] `audit` — full AuditOperation/AuditResult/AuditEntry/AuditLog: JSON serialize + escape + line parse, flock append, reverse-read-with-limit, current_user
- [x] `convert` — `dockerfile_to_toml` (FROM/RUN/COPY/ENV/WORKDIR/LABEL/EXPOSE/ENTRYPOINT/USER); `compose_yaml_to_toml` DEFERRED (needs a Cyrius YAML+Value layer)
- [x] `network_mod` — shared types: NetworkMode(+payload)/NetworkDriver/Network/ContainerNetwork/NetworkPolicy(+nft rules)/DnsRegistry (full parity, runtime-verified)
- [x] `network_bridge` — bridge/veth setup via `ip`/sysctl; Ipv4Addr-typed + varargs-nsenter internals DEFERRED
- [x] `network_dns` — container DNS registry: register/resolve/hosts-entries/inject-into-rootfs (full parity)
- [x] `network_pool` — IpPool/Ipv6Pool/DualStackPool CIDR alloc (u128 IPv6 as 16-byte buffers); leading-zero-octet reject fixed to match Rust
- [x] `network_rootless` — RootlessNetworkBackend/PortMapping/parse_port_mappings/is_unprivileged/which/available_backends/select_backend; async slirp4netns/pasta spawn DEFERRED (needs async process runtime); `+`-sign u16 parse fixed
- [x] `image` — ImageRef parse + full_ref, Layer/Image structs, sha256_digest (sigil sha256), content-addressable ImageStore (new/store_blob/has_blob/read_blob); mkdir-failure + digest-mismatch fixes. JSON `images.json` index + async pull/push/verify DEFERRED (need a JSON codec + registry net layer).
- [x] `registry` — OCI media types, ref/digest/manifest/descriptor/platform struct parsing, `parse_www_authenticate`, `normalize_arch`, `registry_host`. Entire async HTTP client + auth + credential store + mirror config DEFERRED (need net/sandhi/TLS/JSON).
- [x] `storage` — VolumeMount/parse_volume, OverlayPaths + setup/teardown_overlay (mkdir + overlay mount/umount2 + recursive rm via getdents64), mount_volumes (bind mounts). tar/gzip/zstd layer unpack DEFERRED (no Cyrius codec).
- [x] `build` — ImageDef/BuildStep/BuildConfig types + step names, sha256 helper. TOML Stivafile parse + multi-stage tar.gz layer builder + build cache DEFERRED (need TOML/JSON + tar/gzip).
- [x] **AGNOS deps wired** (probe-validated, cc 6.4.14): full `sigil` + `kavach 3.7.0`/`majra 2.5.0`/`nein 1.6.2`/`bote 3.0.0`(core)/`agnodrm 1.4.5` + transitive `libro`/`sakshi`, stdlib union. Our `audit_log_new`/`port_mapping_new` → `stiva_*` to avoid bundle collisions. ~37 benign cross-bundle "duplicate fn (last-def-wins)" warnings (shared agnos helpers — ecosystem norm).
- [ ] dep-heavy (in progress): `encrypted`(agnodrm) · `network_nat`/`network_manager`(nein) · then `runtime`+`container`(kavach/majra, circular pair) · `health`/`ansamblu`(majra) · `mcp`(bote) · `lib` (crate root). `agent`/`fleet` need `container`.

> **✅ cycc struct-id/SIMD-sentinel blocker — RESOLVED in cyrius 6.4.14.**
> Wiring image+registry had grown the unit so a struct landed on **struct-id
> 20/21**, whose `SLTYPE = -struct_id` (-20/-21) was byte-identical to the legacy
> `f64v2`/`f64v4` SIMD sentinels; `parse_decl.cyr`'s field-access guard then
> mis-flagged `var x: ThatStruct; x.field` as a vector → "SIMD vector has no
> named fields". Root-caused + filed to
> `cyrius/docs/development/issues/2026-07-06-struct-sid-20-21-simd-sentinel-collision.md`
> with a deterministic minimal repro (`docs/development/cycc-bug-struct-sid-20-21.cyr`,
> kept for regression); fixed in the compiler at 6.4.14 (never touched the
> language from stiva). All 12 modules now wire green — **315 tests**, build/lint/
> fmt clean. Toolchain pin bumped to **6.4.14**.
>
> **sigil SHA-256 wiring:** pull the à-la-carte hashing chain via `[deps.sigil]
> modules=["src/crypto_scratch.cyr","src/sha_ni.cyr","src/sha256.cyr","src/sha512.cyr","src/hex.cyr"]`
> (+ stdlib `freelist`/`thread_local`/`atomic` for `cbank`/thread-local scratch),
> NOT the full `dist/sigil.cyr` bundle (bigger; its `audit.cyr` redefines
> `audit_log_new`). `sha512.cyr` is only pulled to satisfy hex.cyr's unused
> `sha512_hex` and keep the build warning-free.

**Accepted divergences** (found by the verify stage; track to ADRs at parity-validation):
- `audit` — `AuditLog::new` is *lazy* (file created on first append) vs Rust *eager* (create+append at construction): error surfaces at first `log`, not `new`. Also `metadata` on read is left null (round-trip inspects op/ids/result only, matching the Rust tests).
- `convert` — ENTRYPOINT JSON-array parsing is a minimal quoted-token scan; interior `\"` escapes and malformed arrays diverge from serde_json (fine for well-formed Dockerfiles; needs a real JSON layer for full parity). ENTRYPOINT last-wins + shell-form were fixed to match.
- **Idiom note**: this stdlib `strstr` returns a 0-based INDEX (`-1` absent), NOT a C pointer — the first audit port had a pointer-vs-index bug (`at + strlen(key)`) that only the integration build+test caught, not the agent's syntax-only `cyrius check`. Central build+test after each batch is mandatory.
- [x] **All 16 Rust modules ported** → 25 Cyrius domain modules (network split into 7; `lib.rs` → `stiva_core.cyr` + the aggregation header): `health`(majra), `ansamblu`(majra), `agent`, `fleet`, `mcp`(bote), plus the crate-root `stiva_core` (StivaConfig + the deferred Stiva facade). Runtime memory bugs the agents introduced (dangling struct-literal returns, map-key-type, single-field-struct semantics) fixed + baked into the port playbook.
- [x] **Re-wire AGNOS deps** to the Cyrius `dist/*.cyr` bundles (kavach 3.7.0, majra 2.5.0, nein 1.6.2, bote 3.0.0/core, agnodrm 1.4.5) + transitive `sigil`/`libro`/`sakshi` + stdlib union. Probe-validated in isolation first. `audit_log_new`/`port_mapping_new` → `stiva_*` to avoid bundle collisions.
- [x] **Port the `stiva` CLI** — `src/main.cyr` uses the **cmdit** library (verb dispatch, getopt-long), NOT hand-rolled argv: all 33 subcommands registered as verbs with generated `--help`/`--version`. `convert` has ported backing (Dockerfile→Stivafile via `dockerfile_to_toml`); the container-lifecycle verbs print a clear "deferred to v3.1" message (they drive the async facade).
- [x] **Parity validation** — every module cross-checked vs the Rust oracle by the workflow's adversarial verify stage; accepted divergences recorded above.
- [x] **Test parity** — 697 Cyrius tests (`tests/stiva.tcyr`) mirroring the Rust `#[cfg(test)]`, all green; `cyrius bench`/`fmt`/`lint` clean.
- [x] **distlib** → `dist/stiva.cyr` (8,863-line bundle); version → **3.0.0**.

> **v3.0.0 = the PORT (structure + pure logic + syscall surface). DEFERRED to
> v3.1** (needs Cyrius async runtime + tar/gzip/zstd + full HTTP/TLS/JSON codecs):
> the async container-execution surface — kavach sandbox exec/spawn, cgroups v2
> limits/stats, CRIU checkpoint/restore, tar/gzip/zstd layer pack/unpack, the
> registry HTTP pull/push client, the images.json JSON index, compose-YAML
> convert, and the async `Stiva` facade (~40 orchestration methods in
> `stiva_core.cyr`). Each is documented in a `# ── DEFERRED ──` block in its
> module. `rust-old/` stays as the parity oracle until that surface lands.

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
