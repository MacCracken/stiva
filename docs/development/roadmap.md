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
- [x] **Port the `stiva` CLI** — `src/main.cyr` uses the **cmdit** library (verb dispatch, getopt-long), NOT hand-rolled argv: all 33 subcommands registered as verbs with generated `--help`/`--version`. `convert` has ported backing (Dockerfile→Stivafile via `dockerfile_to_toml`); the container-lifecycle verbs print a clear "deferred" message (they drive the container-manager facade — reclassified to v3.0.x Wave 2, except detached `run -d` → v3.1).
- [x] **Parity validation** — every module cross-checked vs the Rust oracle by the workflow's adversarial verify stage; accepted divergences recorded above.
- [x] **Test parity** — 697 Cyrius tests (`tests/stiva.tcyr`) mirroring the Rust `#[cfg(test)]`, all green; `cyrius bench`/`fmt`/`lint` clean.
- [x] **distlib** → `dist/stiva.cyr` (8,863-line bundle); version → **3.0.0**.

> ## ✅ v3.0.0 — RELEASE: synchronous single-node runtime
>
> **Called here (2026-07-07).** stiva is a working single-node OCI runtime in
> Cyrius: it imports, runs, and manages real containers via a 19-verb CLI, with
> the algorithm-dense subsystems at 85–100% parity. Full 1:1 parity with the Rust
> oracle completes across v3.0.x Wave 2 (blocking) + the small v3.1 residue. **855 tests** (stiva 610 · runpath 171 ·
> mgmt 76 · integration 2), `dist/stiva.cyr` built, pin 6.4.19. Three cycc bugs
> found + filed upstream (identifier-dedup cap, async-parity gaps, struct
> field-name/offset collision) — the language was never modified from stiva.
>
> ### Parity snapshot (16-module audit, 2026-07-07)
> **Structural parity ≈ 61% (314 / 515 Rust surface items); only 8 true gaps.**
> The remaining surface is one coherent band — the port stopped exactly at the
> sync/async boundary, not on scattered holes.
>
> | Tier | Modules (parity %) |
> |---|---|
> | **done 85–100%** | audit 100 · network 94 · health 92 · storage 89 · ansamblu 85 |
> | **mid 46–76%** | fleet+agent 76 · image 72 · oci/intents 67 · encrypted 67 · build 64 · runtime 61 · mcp 47 · registry 46 |
> | **async-wrapped** | container 20 · core+cli 13 (the ContainerManager / Stiva-facade methods aren't ported 1:1) |
>
> **Capability ≠ shape.** container(20%)/core+cli(13%) count the async
> `ContainerManager` + 40-method `Stiva` facade as unported — structurally true —
> but the run path was **re-architected synchronously** and the CLI wired directly
> to it, so `stiva run <image>` launches a container end-to-end (verified on the
> binary). The audit measured shape parity, not capability.
>
> **Deferred by blocker:** async 123 (69%) · not-ported 17 · codec 15 · privilege
> 11 · subprocess 7 · http 6.
>
> **Live CLI (19):** run · ps · stop · rm · inspect · images · rmi · tag · import ·
> export · stats · pause · unpause · logs · wait · gc · prune · info · convert.

---

## v3.0.x — Complete the single-node runtime (synchronous / blocking — NO stackless-coroutine or external blocker)

Two waves. **Wave 1 (below)** — the original module-parity backlog: achievable with the existing
stdlib (bayan JSON/TOML, sankoch gzip, the USTAR tar writer, syscall dir-walks); each lifts a
mid-tier module. **Wave 2 (the "Reclassified" block after it)** — the surface once parked as
"v3.1 async" that the substrate-ready + run-to-completion analysis showed is actually
synchronous/blocking work (OCI layout, blocking registry client, container manager + facade,
exec/CRIU, MCP dispatch, poll-loop streaming). Together they take stiva to a full single-node
runtime — pull/run/manage/save — with only a small externally-blocked residue left in v3.1.

- [x] `oci` — `parse_bundle` / `build_state` / `to_oci_status` + `OciState` (bayan JSON;
  landed in `container.cyr`, coupled to the container types) → oci 67% → ~100%. **2026-07-18.**
- [x] `intents` — variant payload fields (Run/Stop/Pull/Ansamblu/Scale/Inspect): the
  `Intent` value type + constructors + externally-tagged JSON serde (`intent_to_jv`/
  `intent_from_jv`, `ansamblu_action_from_name`). **2026-07-18.** The serde round-trip is
  independent of the NL parser; only `parse_intent` (needs the agnoshi project) stays deferred.
- [x] `image` — `verify_integrity` (blob dir-walk + whole-blob re-hash, `image_store_verify_integrity`)
  → image 72% → ~90%. **2026-07-18.**
- [~] `build` — `build_cache_key` + `build_step_to_jv` (serde-exact tagged-enum JSON via
  bayan, hash-pinned by tests). **2026-07-18.** Remaining: OCI config/manifest JSON assembly
  + layer tar via the tar writer (gzip only) — now Wave 2 (§F); zstd stays v3.1.
- [x] `registry` — credential store (`CredentialStore` fs+JSON: default_path/load/save/set/
  get/remove/to_config) + `RegistryConfig`/`MirrorConfig`. **2026-07-18.** (the *blocking* HTTP client is now Wave 2, §B.)
- [x] `mcp` — the two fully-synchronous tool handlers `mcp_handle_build` / `mcp_handle_ansamblu`
  (parse the Stivafile/ansamblu spec, assemble a structured `McpResult`). **2026-07-18.**
  (The remaining handlers are Wave 2, §E; `handle_run` needs detached run → v3.1.)
- [x] `runtime` — `is_descendant_of` + `container_top` + `read_process_info` (/proc walk);
  `copy_into/from_container` + `copy_dir_recursive` (fs recursion); `ProcessInfo` JSON.
  **2026-07-18.** (`exec` + CRIU are Wave 2, §D; detached spawn / `run -d` → v3.1.)
- [x] `ansamblu` — `parse_ansamblu` (TOML ansamblu file → `AnsambluFile` via bayan's flat
  section model: dotted service/network/volume headers, inline-table `env`, `restart`/
  `replicas`/`health_check`) + `restart_policy_from_name`. **2026-07-18.** (Was tracked as a
  "still-achievable, not-ported" item alongside the sync backlog.)

### Toolchain + dependency refresh (2026-07-18)

- **cyrius pin 6.4.19 → 6.4.66.** Re-vendored the `[deps].stdlib` subset (`cyrius lib
  sync`). The newer compiler is stricter about struct field access: a latent test bug
  (`c.exit_code` on a `Container` whose field is `exit_status`) that 6.4.19 tolerated is
  now a hard error — fixed in `tests/stiva.tcyr`.
- **AGNOS deps → latest release tags:** kavach 3.7.0→3.7.1 · majra 2.5.0→2.5.1 · nein
  1.6.2→1.6.4 · bote 3.0.0→3.1.4 (core) · agnodrm 1.4.5→1.5.0 · sakshi 2.4.4→2.4.6 ·
  libro 2.7.10→2.8.2 · cmdit 1.1.0 (unchanged). Path-override resolution already consumed
  the newer bundles; the `cyrius.cyml` tag pins now match.
- Build + **897 tests** green (stiva 650 · runpath 171 · mgmt 76), `dist/stiva.cyr` rebuilt.
  The `fn_table` (87%) and `identifier buffer` (89%) scaling warnings persist — the
  filed cycc identifier-dedup cap is the ceiling the remaining sync backlog must fit under
  or force the compilation-unit split.

### Reclassified: the now-doable "async" surface → folded onto the v3.0.x line (2026-07-18)

A 4-subsystem, code-grounded analysis re-scoped what had been parked as "v3.1 async." The async
primitives already shipped (cyrius 6.4.33–6.4.42; toolchain 6.4.66; the async-gaps issue is
archived), the runtime is single-threaded **run-to-completion** — so `Arc<RwLock<HashMap>>`
collapses to a **plain heap map** and **blocking** primitives inside a task are fine — and
**`sandhi` ships a complete blocking HTTPS client**. So most of that surface is *synchronous/
blocking work over the already-ported core*, not an async rewrite, and belongs here. Only the
genuinely external- or coroutine-blocked residue stays in v3.1 (next).

**A. OCI image-layout + transfer** (bayan JSON + tar — makes the store Docker/Podman/skopeo-interop).
Net-new/OCI-spec-driven (rust-old had only the ad-hoc `images.json`). Staged: **A1+A2 → v3.0.1**
(landed), **A3+A4 → v3.0.2**.
- [x] **(v3.0.1)** Full OCI **image config** blob (`architecture`/`os`/`created`/`rootfs`/`history`/`config{Env,Cmd,Entrypoint,WorkingDir,User,ExposedPorts,Labels}`) — `imagelayout.cyr` `oci_config_build`; `rootfs.diff_ids` is the **uncompressed** tar digest (spec-correct). New module `src/imagelayout.cyr`.
- [x] **(v3.0.1)** OCI **manifest** blob per image (`registry.cyr` `Descriptor`/`OciManifest` + serde); **`index.json` + `oci-layout`** at the store root; the ad-hoc `images.json` is **retired** — the store is now a valid OCI image layout, reconstructed into `Image` records on load. GC roots = config + manifest + layer digests. `stiva import`→`images`→`rmi` verified end-to-end + spec-valid on disk.
- [x] **(v3.0.2)** **Perms-preserving tar** writer + reader — real mode/uid/gid + directory ('5') and symlink ('2', stored as the link) entries via a `getdents64` + `lstat` walk (`storage.cyr` `_stor_tar_collect`); the reader applies `chmod` + best-effort `lchown` + creates symlinks. uid/gid restore needs root (uncoverable rootless).
- [x] **(v3.0.2)** **`stiva save`/`load`** as **`oci-archive`** — `imagelayout.cyr` `image_store_save_archive`/`load_archive`; two new CLI verbs. Content-verified transfer (blobs re-hashed on load), round-trip validated across stores. Uncovered a **cycc struct-field miscompile** (`Image` fields read garbage in `save_archive`) — worked around with raw-offset accessors (`_img_id`/`_img_layers`/`_img_manifest_digest`/`_layer_digest`); repro/upstream-report is a spun-off task.
- [x] **(v3.0.3)** **`docker-archive` read path** — `stiva load` accepts a `docker save` tarball (`manifest.json` + config + uncompressed layer tars → config blob verbatim + gzip layers + assembled OCI manifest, indexed under `RepoTags[0]`); `imagelayout.cyr` `_il_load_docker_archive`, no new struct types. Plus the A2-review external-ref fix: `_il_parse_full_ref` treats `:` as a tag only after the last `/`, so a port-registry `ref.name` (with/without tag) round-trips (Finding 4).
- [x] **(v3.0.3)** **Tar hardening** (`storage.cyr`, A3 review follow-ups): USTAR **long-name** via the `name`+`prefix` split (paths to 255 B — real rootfs `export` works), **base-256** numeric fields (`size` > 8 GiB, `uid`/`gid` > 2²¹), and **symlink write-through** protection (`_stor_has_symlink_ancestor` + unlink-before-write) on top of the `..`/absolute-name + `..`-target rejection.
- [x] **(v3.0.4)** GNU **longname**/longlink for names > 255 B / symlink targets > 100 B (`storage.cyr` `_tar_write_long` + reader `pending_name`/`pending_link` — no path-length limit); per-image **`platform` passthrough** in index descriptors (real config `architecture`/`os`, host fallback); empty-registry `ref.name` no longer emits a leading `/`. **Group A complete.**

**B. Registry client** — a **blocking** port over `sandhi`/`tls_native` + bayan JSON, not async:
- [ ] `acquire_token`/`authenticated_request` bearer state machine → `fetch_manifest`/`fetch_blob` → the `image_store_pull` driver = live **`stiva pull`**; then `blob_exists`/`push_blob`/`push_manifest` = **`stiva push`**; `list_tags`/`catalog`/`referrers`. Token cache = plain map. Writes into the **A** layout. (Sequential layer download; true parallelism → v3.1.)

**C. `ContainerManager` + `Stiva` facade** — glue over the ported run path (plain maps + majra PubSub):
- [ ] `container_manager_new` + create/start(one-shot)/stop/wait/list/remove/rename/signal/pause/unpause/stats/logs(snapshot)/connect_network + lifecycle events; the `Stiva` facade (~40 methods); **route `main.cyr` verbs through the manager** (retiring the per-verb load/save). *(internal prereq: port `audit_log_new`.)* Detached `run -d` is the exception → v3.1.

**D. `exec` (nsenter) + CRIU** — fork+exec host tools:
- [ ] `_exec_capture2` dual-pipe primitive (child `close(3..)`/NO_NEW_PRIVS; parent `poll()`-drain + `waitpid`) → `exec_in_container`; CRIU `checkpoint`/`pre_dump`/`restore`/`restore_lazy` (gated by the ported `criu_available()`). (Interactive `exec -it` → v3.1.)

**E. MCP live dispatch + streaming poll-loops**:
- [ ] `handle_tool` + `handle_ps`/`stop`/`inspect`/`pull`/`push`/`exec` + `list_resources`/`read_resource` (one-shot over the facade); **`logs -f`/`events`** as foreground CLI poll-loops (file read / majra `chan_try_recv`). (`handle_run` needs detached run → v3.1; multiplexed streaming → v3.1.)

**F. `build` completion**:
- [ ] `build`'s OCI config/manifest JSON assembly + gzip layer tar (the remaining v3.0.x build item; zstd stays v3.1).

**Order:** **A** (OCI layout) first — the store-format prerequisite; **B** (pull writes into it)
alongside **C** (the manager spine, zero deps); **D**/**E** compose on **C**; **F** folds into
**A**/build. Almost all synchronous — the only external dependency touching this line is *filing*
the kavach spawn issue (which unblocks the v3.1 residue, not this work).

---

## v3.1.0 — Blocked & external-dependency residue

Everything on the v3.0.x line above is doable now; this is the genuine remainder, each gated on a
specific external landing (not on stiva effort). As each dependency ships they graduate
individually — there is no monolithic "async milestone" gating them together.

- [ ] **Detached `run -d`** — `spawn_container`/`DaemonHandle`/live daemon log capture, and MCP
  `handle_run`. **Blocked on kavach ≥ 3.8.0 `sandbox_spawn`** (policy-threaded detached spawn +
  `spawned_wait`/`try_wait`/`kill`; drafted and on kavach's roadmap). Do **not** ship a
  half-isolated interim over `persistent_spawn` — it threads no policy. Once kavach ships, the
  stiva side is ~10 lines (`build_sandbox` → `sandbox_spawn` → `DaemonHandle`).
- [ ] **Interactive `exec -it`** (TTY) + a **true multiplexed streaming server** (`select!` over
  many streams inside one task). **Blocked on cyrius stackless coroutines** (mid-body
  suspend/resume — the run-to-completion model can't express them). The `logs -f`/`events`
  poll-loops on the v3.0.x line cover the common cases; this is the interactive/multiplexed tier.
- [ ] **zstd** layer decode — **upstream `sankoch`** (decode-only; stiva builds gzip). **YAML** /
  `convert compose` — **upstream `bayan`** (a YAML-subset value layer).
- [ ] **True concurrent layer downloads** (`buffer_unordered`) — needs a multi-threaded async
  runtime; the v3.0.x pull uses a sequential loop (fine single-node).
- [ ] **`scan_output`** — the kavach `ExternalizationGate` dist binding for the secret/PII scan
  branch of `exec`/`logs`; the unscanned path ships on the v3.0.x line.

---

## v3.2.0 — Agnos Container Support


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
