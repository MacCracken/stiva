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
- [x] `convert` — `dockerfile_to_toml` (FROM/RUN/COPY/ENV/WORKDIR/LABEL/EXPOSE/ENTRYPOINT/USER); `compose_yaml_to_toml` landed later at v3.0.6 once bayan 1.2.0 shipped the YAML+Value layer (§G)
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
**Wave 3 (§G–§I, added 2026-07-22)** — three blockers that turned out to have landed upstream
(`scan_output`, zstd decode, YAML/compose), plus two net-new capability groups that the
kavach 3.8.x dependency graph put within reach: **scheduled/cron containers** (samay) and
**accelerator-aware placement** (ai-hwaccel).

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
  + layer tar via the tar writer (gzip only) — now Wave 2 (§F). *(zstd decode has since landed
  upstream — §G; only zstd **encode** remains out of scope.)*
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
  *(Count superseded — see the 2026-07-22 re-check below: the suite reports **1184
  assertions** across **288 test functions**; `lib/assert.cyr` tallies asserts, not tests.)*

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
- [x] **COMPLETE at v3.0.10.** Increments 0–2 landed at v3.0.8, 3–8 at v3.0.9, 9–10 at v3.0.10.
  `tests/registry.tcyr` grew 85 → **326 assertions** (mirroring `store.tcyr`'s exact 6-module
  include set), with the facade covered in the 26-module `tests/mgmt.tcyr`.
  §B adds **zero new structs** (offset-accessor enums throughout) so it provably cannot perturb
  the cycc struct-id assignment the still-open 20/21 miscompile keys on.
  - **Inc-0** index/platform JSON: `platform_from_jv` / `platform_manifest_from_jv` /
    `oci_index_from_jv` / `_reg_body_is_index`. Found and fixed a latent bug on the way in —
    `oci_manifest_from_jv` read `schemaVersion` unguarded, so a JSON **string** `"2"` silently
    became `0` and the manifest would have been mis-dispatched as schema-version zero.
  - **Inc-1** client construction, every `/v2/` URL builder, auth scopes, `Accept` sets,
    `Location`/digest-query helpers, `api_bases` mirror-fallback parity.
  - **Inc-2** token cache with an injectable clock (the `_at` seam) so expiry is testable without
    sleeping. **Mandatory divergence:** the cache key joins scope parts with `'|'`, not NUL —
    Cyrius maps are cstr-keyed, so a NUL separator truncates the key at the first part, collapsing
    every scope into one entry and silently serving a *pull* token for a *push* (401 on every push).
  - **Inc-3 (transport seam)** — `RegTransportOff` (SEND@0/SINK@8/CTX@16) + `_reg_send` /
    `_reg_download` dispatch, `_reg_http_opts` (sandhi defaults to follow=**0** and a 256 KiB
    cap — both wrong for a registry, which 307s to CDNs and serves multi-arch indexes past
    256 KiB), `_reg_headers` failing **closed** on a CR/LF-bearing value (a registry-supplied
    token reaches `Authorization`, so header smuggling is reachable by hostile input), and a
    `registry_last_error()`/`_status()` channel since the pointer-return idiom has no payload slot.
  - **Inc-4 (bearer state machine)** — `_reg_acquire_token` + `_reg_authenticated_request`'s four
    phases (cached-token attempt → unauthenticated probe → challenge+token → retry once, second
    401 terminal). The canned transport makes the oracle's wiremock cases portable offline, with
    a request log so tests assert what was **sent**: a valid cached token costs exactly 1 request,
    a stale one costs 4. `tests/registry.tcyr` 85 → 132 assertions.
    > **cycc landmine found here.** An `ImageRef` obtained from a **wrapper function** reads back
    > as garbage in this 6-module unit under 6.4.77 — `image_ref_new` writes `"reg.test"`, the
    > wrapper's caller sees `0@!Z`. It does **not** crash: it silently produced a junk token-cache
    > key, so every request re-authenticated. Caught only because a test rebuilt the key from a
    > literal and compared. Construct refs inline or as a direct call assigned to a local; never
    > through a helper. This also **retracts** v3.0.8's "the miscompile appears fixed" note.
  - **Inc-5 (manifest fetch + platform resolve)** — `registry_fetch_manifest` verifies
    `Docker-Content-Digest` against the body before parsing (the oracle never checks it), then
    classifies manifest-vs-index by `Content-Type`, reporting which via
    `registry_last_manifest_kind()`. **Divergence:** when the header is absent the oracle parses
    an index *as* a manifest and yields an image with zero layers — a silently broken pull; we
    fall back to the body shape (`_reg_body_is_index`). `registry_resolve_manifest` picks the
    entry for `current_platform()` and re-fetches **pinned to that child digest**, refusing a
    nested index rather than recursing.
  - **Inc-6 (streaming blob fetch)** — `registry_fetch_blob_to_store` streams to
    `blobs/sha256/<hex>.dl` and hashes as it lands, so resident memory is one read buffer rather
    than one layer. The digest is verified **before** the `sys_rename` that makes the blob visible
    at its content-addressed path, so a corrupt blob is never reachable by `has_blob` / GC roots /
    `verify_integrity` — stronger than the oracle, which buffers the whole blob and leaves the
    check to `store_blob`. The descriptor's `size` is a **ceiling only, never the truth** (it is
    attacker-controlled); the digest is the authority.
    > **The awkward part is auth.** sandhi's download driver reports a non-2xx as status + err
    > and never surfaces that hop's response headers, so `WWW-Authenticate` is **unreachable on
    > the streaming path**. The token therefore has to come from the buffered state machine: a
    > `HEAD` to the same URL elicits the challenge. That probe fires **only on a cold cache** —
    > in a real pull the manifest fetch already primed the same `registry|scope` key, so a layer
    > costs one request; probing unconditionally would double the request count of every layer in
    > the image. A stale cached token surfaces as a bare 401 from the stream, which re-runs the
    > buffered machine (it refreshes on its own 401) and retries the stream **once**.

    `tests/registry.tcyr` 132 → 181 assertions; the canned transport gained a download hook that
    delivers bodies in **three chunks** (a single-shot delivery would not distinguish a correct
    streaming hash from one that only hashed the last buffer) and mirrors sandhi's three-way sink
    contract exactly (`1` continue · `0` graceful stop · `<0` → `ERR_INTERNAL`). It is also
    token-aware, since a static route table cannot make one URL answer 401 then 200 across a
    re-auth.
  - **Inc-7 (pull driver)** — `image_store_pull(client, store, ref)` in `imagelayout.cyr` (include
    order: `registry.cyr` is included first, so it cannot call `image_store_add_to_index`; the
    dependency only runs one way). Resolve → store the manifest → config blob → layers → `Image`
    record → `add_to_index`. Three divergences from the oracle, all deliberate:
    - **The manifest is stored under the digest the registry served, as received.** The oracle
      re-serializes the parsed manifest with `to_vec_pretty` and stores *that* under its re-encoded
      digest (`rust-old/src/image.rs:150-152`), so `index.json` points at bytes the registry never
      served and `Docker-Content-Digest` can never be re-checked from disk. That is not a valid OCI
      image layout. This is what the new `registry_last_manifest_body/_len/_digest` channel exists
      for.
    - **Foreign layers are refused, not fetched.** The oracle follows `descriptor.urls` to an
      arbitrary external host (`image.rs:186-190`). That URL comes from the registry, so following
      it is an SSRF primitive against whatever the daemon can reach — and the request has already
      happened by the time the digest check could help.
    - **Layers download sequentially** (the oracle uses `buffer_unordered(4)`); the runtime is
      single-threaded run-to-completion, so parallel layer fetch stays v3.1.

    A failed layer aborts *without* touching `index.json`, so a half-pulled image is never listable
    or runnable; the blobs that landed stay content-addressed and are either reused by the next
    attempt or swept by `gc`.
  - **Inc-8 (push)** — transport primitives in `registry.cyr` (`registry_blob_exists`,
    `_reg_start_upload`, `_reg_push_send`, `registry_push_blob`, `registry_push_blob_chunked`,
    `registry_push_manifest`), driver `image_store_push` in `imagelayout.cyr`. Config → layers →
    manifest last, because a manifest may only be PUT once everything it references is present.
    Body-carrying legs deliberately bypass `_reg_authenticated_request`: that state machine replays
    its request up to three times, which is fine for a HEAD and catastrophic for a layer upload.
    Four divergences, each fixing an oracle defect:
    - **`registry_blob_exists` uses the PUSH scope.** The oracle probes with `repository:<r>:pull`
      (`registry.rs:555`), minting a token under the *pull* cache key that the upload legs — keyed
      `push,pull` — cannot use: a wasted round trip *and* a cold upload.
    - **One re-auth retry on a 401 from a body-carrying leg**, minted from the challenge that 401
      itself carries. The oracle never retries (`registry.rs:610-635`, `775-788`); the token TTL is
      270 s and a large layer outlasts it.
    - **`push_manifest` requires its auth probe to succeed.** The oracle discards the result
      (`let _ = …`, `registry.rs:769-773`) and PUTs regardless, turning an auth failure into an
      unauthenticated PUT and a second, more confusing 401.
    - **The upload `Location` only receives the Bearer token when it is same-origin with the API
      base.** `_reg_resolve_location` accepts an absolute URL, so a hostile registry can answer the
      upload POST with `Location: https://evil.example/x`; the oracle then PUTs the layer there
      *with* the Authorization header (`registry.rs:610-625`), handing a push-scoped token to an
      arbitrary host. sandhi already applies this policy to cross-origin *redirects*
      (`lib/sandhi.cyr:5077-5117`); the Location hop is not a redirect, so it needs it explicitly.

    `tests/registry.tcyr` 181 → 248 assertions.
  - **Inc-9 (discovery)** — `registry_list_tags` (`GET /v2/<repo>/tags/list`), `registry_catalog`
    (`GET /v2/_catalog`, sent unauthenticated because the catalog is not repository-scoped, so
    there is no scope to mint a token for), `registry_referrers`
    (`GET /v2/<repo>/referrers/<digest>`, OCI distribution v1.1.0), and
    `registry_verify_signature`.
    - An absent or non-array list field yields an **empty** vec, not an error (the oracle's
      `unwrap_or_default`); a failed *request* yields 0. Callers must distinguish "the repository
      has no tags" from "the query failed", so these are deliberately not the same value.
    - A malformed referrers entry is **skipped**, unlike a manifest's layers where a bad descriptor
      fails the whole parse. Referrers is a discovery list — one unreadable artifact must not hide
      the readable ones — whereas a missing layer is a broken image.
    - **`registry_verify_signature` returns 1 / 0 / −1**, not a bool. The oracle returns
      `Ok(false)` for unsigned and `Err` for a failed query, which callers routinely collapse; that
      is how an *unverifiable* image gets treated as merely an *unsigned* one. It also only checks
      that a cosign/notation artifact **exists** — it does not verify the signature
      cryptographically. Neither does the oracle, despite the name.
  - **Inc-10 (facade + CLI + docs)** — `stiva_pull` / `stiva_push` / `stiva_list_tags` /
    `stiva_catalog` / `stiva_verify_signature` on the `Stiva` facade, with `AUDIT_OP_PULL` /
    `AUDIT_OP_PUSH` emitted on both success and failure; `registry_client` is now always
    constructed (`stiva_with_registry` builds it from the supplied credentials/mirrors instead of
    parking them). CLI verbs `stiva pull <IMAGE>` and `stiva push <IMAGE> [TARGET]` are **live** —
    23 of 35 verbs now execute end-to-end.
    > **`push` with no TARGET resolves the image's own stored reference**, not the id parsed as
    > one. `image_ref_parse("sha256:c0ffee…")` yields `docker.io/library/sha256:c0ffee…`, so the
    > naive version would push a local — possibly private — image to Docker Hub under a nonsense
    > repository name. Matches the oracle (`lib.rs:322`).

    Facade tests live in the **26-module** `tests/mgmt.tcyr`, not only the 6-module
    `tests/registry.tcyr`: the cycc miscompile is per-compilation-unit, so a driver green in the
    small unit proves nothing about the shape `src/main.cyr` actually ships.
  - **Index dedup is now digest-aware** (`image_store_add_to_index`). `image_ref_full_ref` drops
    the digest and a digest-only reference parses with tag `latest`, so two digest-pinned pulls of
    one repository rendered under the same key and the second silently evicted the first —
    different content at one name, the first image's blobs orphaned until `gc`. A pinned add now
    replaces only the same manifest digest; an unpinned (tag) add still replaces, because a tag is
    a mutable pointer.
    > **Known limitation:** `index.json` carries the reference in the
    > `org.opencontainers.image.ref.name` annotation, which has no digest field, so a pinned
    > reference does not survive a store reload — after a restart both entries read as unpinned.
    > Making the annotation digest-aware is an on-disk format change and is deliberately not part
    > of this increment.
  - **Remaining in §B:** nothing blocking. Layer-parallel pull stays v3.1 (needs the async
    substrate); Link-header pagination for `tags/list` and `_catalog` is unimplemented in the
    oracle too and is not a parity gap.
  - **Plan change at v3.0.8:** the brief specified blobs on the *buffered* path behind a
    descriptor-derived cap and a 256 MiB refuse-loudly ceiling, **because sandhi's streaming API
    could not authenticate** (`sandhi_http_download_sink_a` hardcoded `headers = 0`). sandhi 1.9.3
    fixed that, so **Inc-6 streams layers straight to disk** with bounded resident memory instead.
    Two sandhi blockers stiva filed are now closed (see the v3.0.8 CHANGELOG): that one, and DNS
    being unable to follow CNAME chains — which made `auth.docker.io`, the Docker Hub *token
    endpoint*, unresolvable while `registry-1.docker.io` resolved fine.
  (Sequential layer download; true parallelism → v3.1.)

**C. `ContainerManager` + `Stiva` facade** — glue over the ported run path (plain maps + majra PubSub):
- [x] **COMPLETE (2026-07-24).** The manager fills the DEFERRED block in `src/container.cyr` and the
  facade fills the one in `src/stiva_core.cyr` (no new module — both files were already in
  `[lib].modules`); `Arc<RwLock<HashMap>>` collapses to a plain `vec<Container*>` + a cstr-keyed
  `internals` map, so `container_state_save`/`_load` are reused verbatim and `state.json`
  round-trips byte-for-byte. Suite **1307 → 1374** (mgmt 128→180 · runpath 187→217).
  - **Inc-1** read-only core: `container_manager_new`/`_list`/`_get`/`_cm_persist`/`_require_pid`.
  - **Inc-2** create + start one-shot: `container_manager_create` (unpack → overlay → spec → record
    → persist) + `container_manager_start` (CREATED→RUNNING→exec_container→STOPPED + exit code +
    log). `Image`/`ContainerExecResult` read by **raw offset** (`_img_layers`/`_img_id`/
    `_image_reference`; `load64(res+0/8/16)`), respecting the still-open cycc 20/21 bug.
  - **Inc-3** mutating/read ops: stop/pause/unpause/signal/remove/rename/update/restart/stats/
    logs/log_tail/wait/try_wait/get_rootfs, with the require_pid state guards. `logs`/`log_tail`/
    `get_rootfs` resolve the id-or-name to the real id before rebuilding the container-dir path
    (a name alias is never the directory name).
  - **Inc-4** lifecycle events over majra `pubsub_*` (stiva's first pub/sub consumer), topic
    `"container.lifecycle"`; created/started/stopped/paused/unpaused/removed published.
  - **Inc-5** connect/disconnect_network best-effort (no-op on the one-shot path; a real
    NetworkManager is only built on the deferred daemon path).
  - **Inc-6** the `Stiva` facade in `stiva_core.cyr` (`stiva_new`/`with_registry`/run/ps/inspect/
    stop/rm/signal/restart/pause/unpause/rename/update/stats/wait/logs/log_tail/get_rootfs/prune/
    security_score), auditing stop/rm/signal via the already-ported `stiva_audit_log_new`
    (`emit_audit` semantics). **Divergence:** facade `run` resolves the image from the LOCAL store
    (the oracle pulls from a registry first; the blocking client is §B). Required adding
    `imagelayout.cyr` to `tests/stiva.tcyr` + `tests/mgmt.tcyr` (both now 26-module, still green).
  - **Inc-7** routed the 12 live container verbs in `main.cyr` (run/ps/stop/rm/inspect/pause/
    unpause/stats/logs/wait/export) through the manager — the per-verb `container_state_load/save`
    is retired (dead `_cli_find_container` removed); the 9 image/convert verbs are untouched.
    `prune` keeps its direct path (its image-reference-aware GC is richer than the facade's).
    Smoke-verified on the binary: run→ps→inspect→wait→logs(by name)→logs --scan→stop→rm→export.
  - **Deferred to v3.1:** detached `run -d` (kavach `sandbox_spawn`); daemon `wait`/`exec`/CRIU
    (`checkpoint`/`restore`); the blocking registry client behind facade `pull`/`push`/`build` (§B).

**T1. Tier-1 CLI sweep — ✅ DONE at v3.0.11.** Six verbs whose logic was already ported but which
the binary would not call: `kill` / `restart` / `rename` (the manager had all three),
`top` (`container_top`'s /proc walk), `cp` (`copy_into_container` / `copy_from_container`), and
`completions`. **29 of 35 verbs live.** New handlers use cmdit's validators
(`cmdit_require_positionals`, `cmdit_range`) rather than hand-rolled positional checks; the older
handlers still hand-roll and should migrate opportunistically.
- **cmdit 1.1.0 → 1.2.0** was needed for `completions`: cmdit owned the verb table but exposed no
  accessor, so a consumer could not generate completions from the verbs it had just registered and
  would have had to keep a second, drifting list. Added `cmdit_verb_count` / `_name_at` /
  `_help_at` / `_is_alias` / `_canonical_at` + `cmdit_completions(h, shell)` for bash/zsh/fish.
- **`scripts/cli-smoke.sh`** — 40 assertions, the first coverage `src/main.cyr` has had. It cannot
  be included in a `.tcyr` unit (it ends in `syscall(60, …)`), so the CLI handlers were previously
  covered by nothing; the §C name-vs-id bug is what that costs. The suite found a real defect on
  its first run (`completions` writing its error to stdout, where the script goes).

**D. `exec` (nsenter) + CRIU** — fork+exec host tools:
- [ ] `_exec_capture2` dual-pipe primitive (child `close(3..)`/NO_NEW_PRIVS; parent `poll()`-drain + `waitpid`) → `exec_in_container`; CRIU `checkpoint`/`pre_dump`/`restore`/`restore_lazy` (gated by the ported `criu_available()`). (Interactive `exec -it` → v3.1.)

**E. MCP live dispatch + streaming poll-loops**:
- [ ] `handle_tool` + `handle_ps`/`stop`/`inspect`/`pull`/`push`/`exec` + `list_resources`/`read_resource` (one-shot over the facade); **`logs -f`/`events`** as foreground CLI poll-loops (file read / majra `chan_try_recv`). (`handle_run` needs detached run → v3.1; multiplexed streaming → v3.1.)

**F. `build` completion**:
- [ ] `build`'s OCI config/manifest JSON assembly + gzip layer tar (the remaining v3.0.x build item).
  zstd **decode** is no longer blocked — see §G below; encode is still gzip-only.

**Order:** **A** (OCI layout) first — the store-format prerequisite; **B** (pull writes into it)
alongside **C** (the manager spine, zero deps); **D**/**E** compose on **C**; **F** folds into
**A**/build. Almost all synchronous — the only external dependency touching this line is *filing*
the kavach spawn issue (which unblocks the v3.1 residue, not this work).

Wave 3 slots in independently: **G** is three self-contained adapters over already-vendored
code (`scan_output` needs nothing from B–F; zstd decode lands in `storage.cyr`'s existing
`unpack_layer` fallback; `convert compose` completes an already-live verb). **I**'s `stiva info`
inventory is two calls and needs nothing. The **dependency-hygiene block below is a hard
prerequisite for H and I** — both bundles must be declared and the two symbol collisions renamed
before either library is called.

### Upstream re-check (2026-07-22) — three v3.1 blockers have landed

Every "blocked on upstream" claim on this page was re-verified **by execution**, not by reading
changelogs: probes were compiled against stiva's own vendored `lib/` and run. Three of the five
external gates are gone, and the work moves onto this line.

**G. Newly unblocked — graduated from v3.1 to v3.0.x:**
- [x] **`scan_output` — LANDED 2026-07-23 (v3.0.6).** `src/runtime.cyr` `scan_output(result,
  policy)` marshals a `ContainerExecResult` into a kavach `ExecResult` (raw offsets, per the
  struct-id 20/21 workaround), calls `gate_apply`, and maps the verdict: PASS/WARN → a new
  result carrying the possibly-redacted strings; QUARANTINE/BLOCK → `0` +
  `STIVA_ERR_SANDBOX`, matching the oracle's single `Err(StivaError::Sandbox)`.
  `scan_output_last_verdict()`/`_last_findings()` expose what the oracle's collapsed `Err` hid.
  Wired as **`stiva logs --scan`** (opt-in: the oracle's per-container `scan_policy` is not yet
  round-tripped through `state.json`). **15 new assertions** in `tests/runpath.tcyr`.

  > **Two of this entry's own prescriptions were wrong**, both found by reading the code:
  >
  > 1. *"call the three scanners directly, since `gate_apply` only redacts at WARN."* Moot —
  >    BLOCK and QUARANTINE become errors anyway (oracle parity), so redaction at those levels
  >    never mattered. `gate_apply` is the correct primitive and is closer to the oracle.
  > 2. *"`logs` is already live and could be scanned today"* is right, but the more important
  >    fact was missed: **`sandbox_exec` already gate-applies internally**
  >    (`lib/kavach.cyr:9026`) and returns `0` on BLOCK/QUARANTINE. So `exec_container`'s
  >    output is *already* gated, and adding `scan_output` there would scan twice. Verified on
  >    the binary: a container echoing an AWS key never completes —
  >    `kavach: externalization blocked: blocked by gate`. `logs` is the only correct site.
- [x] **zstd layer decode — LANDED 2026-07-22.** `unpack_layer` (`src/storage.cyr`) now
  dispatches on the 4-byte zstd magic (`28 B5 2F FD`) into `_stor_unpack_zstd_bytes`, sized
  from `zstd_frame_content_size` rather than a grow-retry loop — `zstd_decompress` returns
  `-1` for *both* a short output buffer and a corrupt frame, so the two are
  indistinguishable and a retry loop cannot be driven off it. Because the frame's declared
  size is attacker-controlled, the buffer is bounded by an **8 GiB absolute** and **1000:1
  ratio** ceiling (allocation-amplification guard; the gzip path needs none, since its
  grow-loop reacts to actual decoded output). Lights up `media_oci_layer_zstd`
  (`src/registry.cyr:57`) on the pull path. **12 new assertions** in `tests/store.tcyr`:
  real round-trip, corrupt-frame rejection, and both DoS ceilings.

  > **This work first surfaced a heap buffer overflow — in the toolchain, not in stiva.**
  > sankoch **2.5.5**, vendored by the cyrius **6.4.66** snapshot stiva was pinned to, wrote
  > past `dst_cap` on malformed input: every output write stored to `_z_out + _z_outpos`
  > with no per-write bound, checking `_z_outpos > _z_outcap` only *after* a whole block had
  > been written, with `bsize` taken from the attacker-controlled block header. Proven with
  > a canary — a 32-byte frame (valid magic + `0x41` filler) declaring FCS 16961 returned
  > `-1` **and clobbered all 4096 canary bytes** past the buffer. It is what silently
  > corrupted unrelated tar/symlink/docker-archive tests when a malformed-input test was
  > first added.
  >
  > **sankoch had already fixed it** in **2.5.6** (`8b843d6`, "zstd decoder hardening",
  > 2026-07-19) — bounds checks now precede every write (`src/zstd.cyr:656, 719, 744, 863,
  > 870`). stiva was simply pinned one toolchain release below the fix. Resolution was a
  > **pin bump to cyrius 6.4.71** (sankoch 2.7.5), which also cleared the long-standing
  > wrapper/manifest drift warning. The same canary passes clean against 2.7.5.
  >
  > Lesson worth keeping: stiva consumes sankoch from `~/.cyrius/versions/<pin>/lib/`, not
  > from the sibling repo — so a stdlib security fix reaches stiva only via a toolchain pin
  > bump. Check the snapshot's vendored version, not the sibling's.

- [x] **`convert compose` / YAML — LANDED 2026-07-23 (v3.0.6).** `src/convert.cyr`
  `compose_yaml_to_toml` + `compose_last_error()` over `bayan_yaml_parse_str`, wired into the
  `convert` verb (which already defaulted to `-f compose`). All four oracle fixtures match
  byte-for-byte. **The single systematic porting hazard was ordering**: `serde_json::Map` is a
  `BTreeMap`, so the oracle emits services, networks, volumes, env-maps and depends_on-objects
  **sorted**; bayan objects are insertion-ordered. `_cv_sorted_idx` (byte-wise `_cv_key_lt`,
  matching Rust's `String` Ord) is applied at all five sites, and the document-ordered forms
  (the command/ports/volumes/depends_on **arrays** and the env **list**) deliberately are not.
  Merge keys get an explicit stiva-side reject since bayan has no `<<` handling at all — the
  `<<: *base` form dies incidentally on the alias check, but a literal `<<: x` would silently
  become a key. **69 new assertions**, in a **new 5th test unit `tests/convert.tcyr`** (the six
  dockerfile tests moved there too: `stiva.tcyr` was at 94% of cycc's identifier cap, and a
  unit including only `error` + `convert` sits at 86%). Also fixed while here: the `convert`
  read path silently truncated at 1 MiB, which on the compose path surfaces as a baffling YAML
  parse error pointing at a line the user can see is fine — it now refuses.

  <details><summary>Original entry (the analysis that unblocked it)</summary>

  `lib/bayan.cyr` is **1.2.0** (2026-07-16) and
  `bayan_yaml_parse:4545` emits into the *same* `JTAG_*`-tagged `bayan_json_v_*` graph the JSON
  parser produces — exactly the "YAML parser + JSON Value model" pair `src/convert.cyr:505-518`
  records as missing. Probe walked the full compose surface: `services` obj_len=2,
  `services.web.image`, `ports` arr_len=2, `networks.frontend.driver`,
  `ipam.config[0].subnet`, `volumes.pgdata.driver`; invalid input → 0 + error string, matching
  `rust-old/src/convert.rs:20-21`. **Scope honestly — it is a documented subset:** flow
  *mappings* (`{replicas: 2}`), block scalars (`|`), anchors/aliases and merge keys
  (`&base` / `<<: *base`), and multi-document (`---`) are all rejected with explicit errors.
  Flow *sequences* work. Error clearly on the unsupported forms rather than claiming full
  compose support. API: `bayan_yaml_parse(src)` takes a `Str`; use `bayan_yaml_parse_str(buf, len)`
  for C-strings — passing a cstr to the former compiles with an arity warning and silently
  yields a non-object.

  </details>

> **§G is now complete.** All three items that the 2026-07-22 upstream re-check graduated from
> v3.1 have landed: zstd decode (v3.0.5), `scan_output` and `convert compose` (v3.0.6).
>
> **What the v3.0.6 adversarial review changed about how to read this page.** The compose
> converter was reviewed across four dimensions with three-lens adversarial verification (103
> agents); 16 findings survived. The decisive ones came from a reviewer who **compiled the
> frozen Rust oracle and diffed its output against the binary's**, rather than reasoning about
> what the oracle would do. That caught two parity breaks on entirely ordinary compose files,
> one of which this page had explicitly (and wrongly) certified as parity:
>
> > *"YAML 1.1 booleans: bayan treats `true`/`false` only; `yes`/`no`/`on`/`off` stay strings
> > (verified). serde-saphyr agrees (YAML 1.2 core), so this is **parity**, not divergence."*
>
> serde-saphyr resolves the YAML 1.1 token family. `restart: no` — compose's documented
> default — emits nothing in the oracle and `restart = "no"` in a naive port. The claim came
> from reading bayan and *assuming* the oracle matched. **When this page says "verified",
> check whether the oracle was executed or only read.** Probing one side is half a probe.
>
> The other lasting lesson: the review also found the sort was O(n²) on reverse-ordered keys
> (236 s per MiB), and that the port inherited the oracle's complete lack of output escaping —
> a structure-forgery hole on a path whose whole job is ingesting third-party files. Parity
> with a frozen oracle is the bar for *behavior*, not for *complexity class or security
> posture*; both were fixed as deliberate, documented divergences.

### Toolchain 6.4.72 → 6.4.76, and the cycc struct-id fix that isn't (2026-07-24)

Pin bumped to **6.4.76**. It repaired two long-standing annoyances:
- **`cyrius audit`'s tests stage** now compiles and runs all five test units (it had reported
  bogus "compile error" for every one). Its **fmt stage still false-positives** — every file
  passes `cyrius fmt <f> --check` and `cyrius fmt -w` rewrites zero bytes, so the per-file
  `--check` is the reliable gate, not `audit`.
- **The compiler caps jumped**: `fn_table` 8192 → **32768** (23% used) and the identifier
  buffer 262144 → **524288** (46% used). The 90%/92% pressure that this page warned would
  block turning on the `accel` feature for §H/§I is gone — budget for the split is no longer a
  §H/§I prerequisite.

But the headline — **the cycc struct-id 20/21 ↔ SIMD-sentinel miscompile is NOT fully fixed**,
despite the "RESOLVED in 6.4.14" note far above (that fix regressed as the code grew; the whole
group-A layer carries raw-offset workarounds because of it). Re-verified 2026-07-24 by
*attempting the cleanup*: a probe in the 26-module `runpath.tcyr` unit passed and the 6-module
`repro_min.tcyr` printed `MATCH`, so the accessors were retired — and the suite then **SIGSEGV'd**
(exit 139) in `image_store_save_archive` under the **6-module `store.tcyr` unit**, where a bare
`var im: Image = p; im.id` crashes (scalar field compiled as a vector load, OOB read). All
conversions were reverted; the suite is back to 1307. **The bug is compilation-unit-shape-dependent:
a green probe in one unit proves nothing about another.** The raw-offset accessors stay until a
probe is green in *every* unit that includes the struct — the 6-module store unit especially.
This is the same "verified by execution, not by reading" lesson as the compose review, applied to
the toolchain: a passing repro in the wrong unit shape is reading, not verifying.

### Net-new capability surfaced by kavach 3.8.x — samay + ai-hwaccel (2026-07-22)

kavach 3.8.0 took `[deps.samay]`, and samay in turn takes `[deps.ai-hwaccel]`, so both bundles
now arrive in stiva's `lib/` transitively. **Neither is bloat: each covers functionality stiva
genuinely lacks** — and `rust-old` lacks it too, so these are net-new, not port regressions.

First, the negative results, so nobody re-derives them:
- **kavach's consumable API did not change at all.** `git diff 3.7.1 3.8.1 -- dist/kavach.cyr`
  is one line — the version header — and a sorted symbol-table diff between the tags is empty.
  Every kavach symbol stiva calls is unchanged; there is no breaking change from kavach.
- **The samay bridge is not reachable.** `sandbox_policy_from_samay_req` /
  `_from_samay_task` live in `kavach/src/samay_bridge.cyr`, which is **deliberately excluded
  from kavach's `[lib].modules`** so downstream consumers aren't forced to take samay. It is
  referenced only by `kavach/tests/samay_integration.tcyr`. `grep samay lib/kavach.cyr` → 0 hits.
  If stiva wants it, stiva reimplements its 13 lines.
- **`SandboxPolicy` gained nothing.** Still 12 fields / `SANDBOX_POLICY_SIZE = 96`
  (`kavach/src/policy.cyr:7-20`); `SandboxConfig` still 8 fields / 64
  (`kavach/src/lifecycle.cyr:81-92`). No `device`, `devices`, `device_allow`, `accel`, `gpu`,
  or `vfio` field exists anywhere in kavach. `cgroup_setup` (`kavach/src/cgroup.cyr:151-174`)
  still writes only `memory.max` / `cpu.max` / `pids.max` — no devices controller, no eBPF
  device program.

**H. Scheduled / cron containers — UNBLOCKED, net-new.** stiva has no time-triggered container
start of any kind: grepping `src/*.cyr` for `cron|schedule` matches only `fleet.cyr`, and every
hit there is *spatial* placement, not time. `rust-old` has none either. The machinery is already
vendored and complete in `lib/samay.cyr`:
- [ ] `cron_expr_parse:956` / `cron_expr_matches:1049` / `cron_expr_next_after:1081` — a full
  standard 5-field cron parser with `@shortcuts`, ranges, steps, month/DOW name lookup, and the
  Vixie `crontab(5)` DOM-vs-DOW star rule (`:801-810`, `:936`).
- [ ] `cron_scheduler_new` / `_add(name, expr, template, enabled, missed_policy):1165` /
  `_check_due:1344` / `_check_due_at` / `_remove_entry` / `_list_entries` (`:1131-1344`), with
  `struct CronEntry` / `struct CronScheduler`, a **missed-schedule policy** (`:810`) and
  catch-up counting with a cap (`_cron_count_due:1240`, `_cron_log_catchup_cap:1273`) — the hard
  part, already solved — plus full JSON persistence (`cron_scheduler_to_json_str` /
  `from_json_str`, `:1882-1883`).
- [ ] stiva-side work — two pieces samay does **not** give you:
  - **A container side table.** `CronTaskTemplate` (`:1110-1116`) carries only
    name/description/agent_id/priority/resource_requirements — **no image, command, env, mounts,
    or restart policy**, and its JSON codec (`:1778-1806`) cannot carry them either. stiva owns
    an entry-name → `ContainerConfig` table and its own persistence (alongside
    `container_state_save`/`_load`, `src/container.cyr:599`/`:623`, atomic tmp+rename), and must
    strip the `" (cron)"` suffix `_cron_task_name:1216` appends.
  - **A poll site — stiva has none.** `src/main.cyr:753-793` is a one-shot cmdit dispatcher
    ending in `syscall(SYS_EXIT, _rc)` (`:835-836`). "Single-threaded run-to-completion" is the
    *cyrius execution model*, not a running loop, and `--detach` is refused at
    `src/main.cyr:213-218` pending kavach `sandbox_spawn` — so an in-process cron daemon could
    only fire containers **serially in the foreground**.
  **v1 shape: a `stiva cron add/ls/rm/check` verb set driven by an external systemd timer**
  (`check` fires what is due and exits). The in-process loop lands with `run -d`. Scheduled
  short batch jobs are therefore unblocked *now*; overlapping or long-lived scheduled containers
  share the `run -d` gate (§v3.1). **Effort: medium — the parser and catch-up semantics are free.**

**I. Accelerator-aware placement + node inventory — UNBLOCKED, net-new.** `lib/ai-hwaccel.cyr`
(2.3.15, 6339 lines, 295 fns) is a **read-only hardware inventory and workload-planning library**
— it answers "what accelerators exist here, how big, how fast," across 19 `AcceleratorType`
variants via 17 backends. It is *not* a device-plumbing library (see §J).
- [ ] **Node accelerator inventory in `stiva info` / `inspect`** — `registry_detect_no_exec():3818`
  is subprocess-free (pure sysfs/syscall reads), so it is safe in a hardened context and cannot
  hang on a missing `nvidia-smi`; pair with `registry_to_summary_json:6199` (device_count /
  has_accelerator / total & accelerator memory / gpu|tpu|npu counts / warnings). **Two calls** —
  the cheapest way to turn the vendored 207 KB into shipped functionality.
- [ ] **An accelerator dimension in `fleet`'s placement — graft, do NOT adopt samay's type.**
  Add an `accel_profiles` vec to **stiva's own** `FleetNodeCapacity` and `accel_req` /
  `accel_min_chips` (`REQ_*` + min chips) to `DeploymentConstraints` (`src/fleet.cyr:125-126`
  filters on `min_memory_mb`/`min_cpus` only), then gate placement on
  `find_satisfying_profile(req, min_chips, profiles):5370`. The `AccelRequirement` enum
  (`REQ_NONE|GPU|TPU|GAUDI|AWS_NEURON|GPU_OR_TPU|ANY_ACCELERATOR`, `:5310`) and
  `requirement_satisfied:5333` were written for exactly this — the header comment reads "Used
  for scheduling integration." samay ships this precise pattern (`lib/samay.cyr:296` field,
  `:341` `_accel_ok`, `:351` `can_fit`), so it is copyable. **Effort: small.**

  > **Why graft rather than adopt.** samay's `NodeCapacity` is a superset only on the
  > *continuous* dimensions (fractional CPU, available-vs-total, disk, accel profiles,
  > reserve/release). It has **no `max_containers`** — which is stiva's entire fit test
  > (`free_slots`, `src/fleet.cyr:240-245`, used at `:260, :401, :429, :467, :503`) — **no node
  > status** (Ready/NotReady/Draining/Cordoned, `src/fleet.cyr:56`), **no label constraints**
  > (`:205-219`), and only best-fit-by-utilization placement (`_best_fit_node:632`) against
  > stiva's three strategies. Wholesale adoption would regress three shipped features. Two
  > further reasons not to copy its ledger verbatim: `node_capacity_release:389` is reached only
  > from `cancel_task:580`, so tasks reaching `TASK_COMPLETED`/`TASK_FAILED` **never release
  > their reservation** (a monotonic capacity leak); and `task_scheduler_preempt_if_needed:709`
  > mutates nothing, has zero callers inside samay, and never reads `TaskScheduler_nodes` — the
  > preemption surface is advisory vocabulary, not policy. Treat the collision as a signal that
  > stiva's capacity model is impoverished (it is) and fix that on stiva's own struct.
  > Conversion is clean: stiva already has fractional CPU container-side — `cpu_shares` is an
  > absolute quota (`_rt_cpu_quota` `src/runtime.cyr:653-658` → `cpu.max` `:684-688`), so
  > 1024ths-of-a-core maps directly onto samay's f64 `cpu_cores`.
- [ ] **Persist accel profiles** — `profile_to_json:6046` / `profile_from_json:6118` are a
  lossless round-trip over the bayan DOM, added in 2.3.15 *specifically* to unblock samay's
  `accel_profiles` (before it, a rebuilt TPU profile could not satisfy the `REQ_TPU{min_chips}`
  requirement it was registered for). Nothing to write for `state.json` or for a daimon/sutra
  node record.
- [ ] *(optional)* **NUMA / fabric affinity** — `profile_numa_node` → cpuset cgroup is contained;
  `sio_has_nvswitch:1212` / `_nvlink:1222` / `_ici:1234` / `sio_max_ic_bw_x1000:1244` would let
  ansamblu express "these two containers must land on NVSwitch-connected devices." Note
  `detect_interconnects` shells out and is masked off under `registry_detect_no_exec`.

**Pre-existing `fleet.cyr` defects to fix while you are in there.** All three are faithfully
inherited from the Rust oracle, so correcting them is a *deliberate* parity divergence and wants
an ADR (same treatment as the `audit`/`convert` divergences recorded at v3.0.0):
- [ ] `plan_rollback` (`src/fleet.cyr:561-587`) calls `select_migration_target` once per running
  container (`:571-572`) with **no reservation between calls**, so all N containers on a failed
  node plan onto the same target. `rust-old/src/fleet.rs:336-345` is identical. The
  reserve/release pattern §I adds is the fix.
- [ ] `DeploymentConstraints.preferred_nodes` (`src/fleet.cyr:127`, zeroed at `:139`) is never
  read — dead in `rust-old/src/fleet.rs:43` too.
- [ ] `FLEET_NODE_DRAINING` / `FLEET_NODE_CORDONED` (`src/fleet.cyr:59-60`) are defined but every
  filter tests only `== FLEET_NODE_READY` (`:258`, `:501`, `:544`) — there is no drain semantic.

> **Why this matters beyond stiva:** `daimon` — stiva's own container-management consumer —
> **already declares `[deps.ai-hwaccel] 2.3.15` and `[deps.samay] 1.0.1`**
> (`daimon/cyrius.cyml:63-76`). The layer directly above stiva already reasons about
> accelerators while stiva itself cannot express them. That is the functional gap in one line.

### Dependency hygiene — ✅ DONE 2026-07-22

All four items below are complete. Summary of what landed:

- **kavach 3.8.2** — renamed its OS-backend namer `backend_name` → `os_backend_name`
  (`src/backend.cyr:26` + 3 internal callers), leaving the bare name to ai-hwaccel; and made
  `[deps.samay]` + `[deps.ai-hwaccel]` **`optional`** behind a default-on `scheduler` feature,
  so consumers no longer inherit 279 KB for a bridge kavach does not ship. 436 assertions green.
- **stiva** — `FleetNodeCapacity`/`fleet_node_capacity_new`, `stiva_which`, both `backend_name`
  call sites → `os_backend_name`; `[deps.kavach]` → `3.8.2`; samay + ai-hwaccel declared,
  pinned, `optional`, behind a **default-off `accel` feature**; stdlib re-synced to the 6.4.66
  pin. `cyrius.lock` 83 = 83 `lib/*.cyr`, `--verify` clean. **1184 assertions green** across a
  new 4-file split (stiva 684 · store 185 · runpath 187 · mgmt 128).
- **mehman** — one-line consumer fix for the kavach rename (`src/sandbox.cyr:88`).

Two findings worth carrying forward:
1. **`stiva info` was silently wrong**, logging `intel-npu` instead of `oci` — both enums start
   at `0`. Reproduced and fixed; the corrected output is the regression check.
2. **Test-unit splitting must be by *include set*, not by test count.** Peeling 821 lines /
   39 test functions out of `stiva.tcyr` freed **0 bytes** of identifier space; dropping one
   `include` freed 4. Identifiers dedupe — the buffer is dominated by the vendored `lib/`
   bundles auto-prepended to every unit. That is why the feature gate, not the split, is the
   fix; the split is what makes §H/§I *possible* once `accel` is switched on.

Switching `accel` on (when §H/§I start) will re-cross the identifier cap — budget for further
splitting by include set at that point.

<details><summary>Original hygiene backlog (all items now closed)</summary>

- [x] **Declare the two bundles, or drop them.** `cyrius.cyml` has no `[deps.samay]` and no
  `[deps.ai-hwaccel]`, yet `lib/samay.cyr` (1.0.1) and `lib/ai-hwaccel.cyr` (2.3.15) are on disk
  with **no `cyrius.lock` entries at all** — the lock has 83 entries against 85 `lib/*.cyr`
  files, and the two gaps are exactly these files. Separately, `lib/kavach.cyr` is the **only
  one of the 83 entries that fails `sha256sum -c`** (lock records `83d87bd1…`, actual is
  `120497de…`). `.gitignore:5-8` asserts "lib/ is reproducible from the manifest + lockfile" —
  **that invariant is currently false.** If §H/§I are adopted, add explicit blocks and bump
  `[deps.kavach]` to `3.8.1` so the pin matches what resolution actually vendors; if not, the
  two bundles should not be in `lib/` at all. Either way the lock must be regenerated so lock
  and `lib/` agree.
- [x] **`tag` does not bind while `path` is present.** `cyrius.cyml:125` pins kavach `3.7.1`;
  the `path = "../kavach"` override silently wins and vendors whatever the sibling checkout is
  at. This is how 3.8.1 arrived unannounced. `cyrius.lock` records only bare
  `<sha256>  lib/<file>` lines — no dep name, version, tag, or git rev — so it is structurally
  incapable of detecting the substitution. `git diff cyrius.lock` after `cyrius deps` is the
  only reliable detector; worth a CI guard.
- [x] **Three symbol collisions — two stiva-owned, one upstream.** cycc warns on duplicate *fns*
  ("last definition wins") but is **silent on duplicate structs**, so the struct case surfaces as
  a misleading parse error in whichever file compiles last. Use the `stiva_*` prefix idiom
  already applied to `audit_log_new` / `port_mapping_new` (`cyrius.cyml:60-65`, `:79-82`).
  1. `struct NodeCapacity` + `node_capacity_new` — stiva's is 4 fields / 32 B / 4 args
     (`src/fleet.cyr:75,83`); samay's is 9 fields / 72 B / **5** args (`lib/samay.cyr:288,300`).
     → `FleetNodeCapacity` / `fleet_node_capacity_new`; 9 refs in `src/fleet.cyr`
     (`:75, :85, :221, :242, :314, :321, :343, :350, :567`) + `tests/mgmt.tcyr:527,530`.
  2. **`which(name)`** — stiva's returns a **boolean 1/0** (`src/network_rootless.cyr:287-316`);
     ai-hwaccel's returns a **heap cstring pointer** (`lib/ai-hwaccel.cyr:1698-1726`). stiva's
     only callers test `== 1` (`src/network_rootless.cyr:325,328`), so if ai-hwaccel wins,
     rootless pasta/slirp4netns detection silently reports "no backends available."
     → `stiva_which`.
  3. **`backend_name(b)` — upstream, NOT fixable in stiva, and live today.** Defined in **both**
     `lib/kavach.cyr:2465` (sandbox backends: `Backend.PROCESS`…`NOOP`) and
     `lib/ai-hwaccel.cyr:693` (17 *detection* backends: `BACKEND_CUDA`…`BACKEND_WINDOWS`).
     **Both enums start at `0`**, so if ai-hwaccel's definition wins,
     `backend_name(Backend.PROCESS)` returns `"cuda"`. stiva calls it at `src/runtime.cyr:841`
     (sandbox-selection log) and `src/runtime.cyr:955` — the latter inside `security_score()`,
     which **`stiva info` invokes**. Fix belongs upstream: ai-hwaccel should rename to
     `hw_backend_name`, exactly as it already renamed `registry_new` → `hw_registry_new` for a
     bote-core clash (`lib/ai-hwaccel.cyr:3562-3570`). File it.
     *(`path_exists` also collides — `lib/kavach.cyr:2498` vs `lib/ai-hwaccel.cyr:1385` — but
     with identical 1/0 semantics, so it is benign.)*
     Note kavach's 3.8.0 changelog claim "No symbol collisions with kavach's 442-fn surface
     (verified)" (`kavach/CHANGELOG.md:43-44`) is **false** for consumers that also pull
     ai-hwaccel; the verification did not cover the transitive closure.
  `struct AuditEntry` (`src/audit.cyr:112` vs `lib/kavach.cyr:5923`, different field offsets) is
  a fourth, currently latent only because no typed local of that name is ever declared.

</details>

### Inherited defect — the no-overlay rootfs was empty — ✅ FIXED 2026-07-25

Surfaced while smoke-testing `cp` for 3.0.11 and filed as "Known" in that CHANGELOG.
`container_manager_create` unpacked the image layers via `prepare_layers`, then — when
`setup_overlay` could not mount, **which is every unprivileged run** — fell back to a bare
`{croot}/rootfs`, `sys_mkdir`'d it, and **discarded the layer dirs**. The container ran against an
empty directory: `stiva run local/demo:v1 /bin/echo hi` could not have found `/bin/echo`.

**Exact parity with the oracle** — `rust-old/src/container.rs:366-369` does
`unwrap_or_else(|| container_root.join("rootfs"))` and never populates it — so this was
**inherited, not a port divergence**. The oracle stays frozen; the fix is a deliberate divergence.

**Fixed by `flatten_layers` (`src/storage.cyr`, net-new — no oracle counterpart):** the prepared
layer dirs are merged into `{croot}/rootfs` bottom-to-top, later layers winning, which is the
same view the overlayfs lowerdir stack would have given. Four decisions worth recording:

- **Copy, not hard-link.** Hard links would share inodes with the store's `layers/` cache, so the
  first write inside *any* container would corrupt the layer every other container reads. Docker's
  `vfs` driver copies for exactly this reason.
- **Perms-preserving, streamed.** mode via `chmod`, uid/gid via `lchown` (chown *before* chmod —
  chown clears setuid/setgid), symlinks recreated as links rather than dereferenced. An
  entrypoint that lost its exec bit in the copy would still be an unrunnable container. Bytes move
  through one shared 64 KiB buffer, so peak memory is a chunk, not the tree.
- **On by default, opt out with `STIVA_ROOTFS_FALLBACK=none`.** It costs one copy of the image per
  container (reclaimed by `stiva rm` with the rest of `{croot}`), but an empty rootfs is not a
  cheaper container — it is a broken one, so "off" is not a defensible default. Cost is now
  measured: `flatten_layers_2x60_files_4kib` ≈ 1.36 ms for ~240 KiB over 60 files.
- **A directory replaces a same-named symlink from a lower layer.** Without that, a layer shipping
  `usr → /etc` followed by one shipping `usr/passwd` would write **outside** the rootfs — the same
  escape `_stor_has_symlink_ancestor` blocks on the extraction side. Covered by
  `test_flatten_layers_no_symlink_escape`.

Tests: `flatten_layers_stacks_and_preserves_perms`, `_empty`, `_no_symlink_escape`,
`_from_prepared` (`tests/store.tcyr`) + `cm_create_rootfs_populated` (`tests/runpath.tcyr`, the
end-to-end create path). Suite **1739 → 1771**.

**Two follow-ups this opened, both still open:**

- [ ] **`diff` must handle both rootfs layouts (§ blocks the verb, which is still "not yet
  wired").** Over an overlay the changed set *is* `{croot}/upper`; a flattened rootfs has no upper
  and must be compared against the layer dirs instead. The two cannot be distinguished after the
  fact — `internals` is process-local and never persisted, and `setup_overlay` creates
  `upper`/`work`/`merged` *even when the mount fails*, so their presence proves nothing. `create`
  therefore drops a **`{croot}/.rootfs-flattened` marker**; `diff` should read it.
- [ ] **OCI whiteouts are not applied — by BOTH paths, not just the new one.** `_stor_extract_tar`
  extracts `.wh.<name>` / `.wh..wh..opq` markers literally, and overlayfs understands only its own
  char-dev-0:0 whiteouts, not the tar convention, so a file deleted in an upper layer stays visible
  either way. `flatten_layers` deliberately matches that rather than fixing it unilaterally: doing
  so would make an unprivileged rootfs differ from a privileged one for the same image. The fix
  belongs at **unpack** time (translate markers as containerd does), where it lands for both paths
  at once. `rust-old/src/storage.rs` has no whiteout handling either — inherited, like the above.

---

## v3.1.0 — Blocked & external-dependency residue

Everything on the v3.0.x line above is doable now; this is the genuine remainder, each gated on a
specific external landing (not on stiva effort). As each dependency ships they graduate
individually — there is no monolithic "async milestone" gating them together.

> **Re-checked 2026-07-22.** Three items previously listed here — `scan_output`, zstd decode,
> and `convert compose`/YAML — have **landed upstream and moved to the v3.0.x line** (§G).
> What remains is below. **State each gate by symbol, not by version number** — the version
> form has already produced one false positive.

- [ ] **Detached `run -d`** — `spawn_container`/`DaemonHandle`/live daemon log capture, and MCP
  `handle_run`. **Blocked on kavach growing a policy-threaded detached spawn**
  (`sandbox_spawn` + `spawned_wait`/`try_wait`/`kill`). *The old "blocked on kavach ≥ 3.8.0"
  wording is now a false positive: kavach shipped **3.8.0 and 3.8.1** (samay integration, then a
  dep bump) and `grep sandbox_spawn` over its `src/`, `dist/`, and `tests/` still returns
  nothing — the version was spent on other work.* kavach's roadmap carries no new target for it
  and mentions samay nowhere; treat the bridge as a one-shot proof of concept with nothing
  behind it. Do **not** ship a half-isolated interim over `persistent_spawn` — it threads no
  policy, and kavach's own roadmap says so. Once the symbol exists, the stiva side is ~10 lines.
- [ ] **Interactive `exec -it`** (TTY) + a **true multiplexed streaming server** (`select!` over
  many streams inside one task). **Blocked on cyrius stackless coroutines** (mid-body
  suspend/resume — the run-to-completion model can't express them). *Action required from
  stiva:* cyrius parks this in `roadmap-future.md:116` as an "**Unpinned follow-on** … No live
  consumer; pull forward on a real suspend-across-await need." **stiva is that consumer and has
  not filed.** Filing is the unblock lever, not waiting. Nuance: non-TTY single-stream `exec -i`
  is buildable today over kavach `persistent_send`/`persistent_read` in a blocking poll loop —
  it is the `-t` half that has no substrate (no pty helper exists anywhere in `lib/`).
- [ ] **True concurrent layer downloads** (`buffer_unordered`) — needs a multi-threaded async
  runtime; the v3.0.x pull uses a sequential loop (fine single-node).
- [ ] **J. Device / accelerator passthrough** — `--device` / `--gpus`, `/dev` node injection,
  cgroup device rules, driver-library mounts. **Blocked on kavach, and the hole is one layer
  further upstream than stiva.** `SandboxPolicy` has no device field and `cgroup_setup` writes
  no devices controller (see §I's negative results), so stiva **cannot express device
  passthrough through the 3.8.1 API under any wiring** — kavach's own samay bridge silently
  drops `ResourceReq.accel_req` / `accel_min_chips` (`samay_bridge.cyr:24-31`) for precisely
  this reason. Sequence: **(1)** file against kavach — a device-allow list on `SandboxPolicy`,
  a cgroup v2 devices gate in `cgroup_setup`, and `linux.devices` emission in its `oci_spec.cyr`.
  kavach's roadmap already wants adjacent work (`:212` USB/device selective passthrough, `:215`
  and `:254-255` GPU passthrough / VFIO / virtio-gpu — all Medium priority, unstarted, and
  scoped to the *Embassy* foreign-container plan rather than to ai-hwaccel). **(2)** stiva then
  emits OCI `linux.devices[]` + `linux.resources.devices[]` next to the existing CPU/mem/PID/IO
  cgroup writers, and uses ai-hwaccel's `REQ_*` / `profile_*` vocabulary to describe and
  validate the request.
  **ai-hwaccel does not supply the plumbing** — it never emits a device node path, never
  computes major/minor, never produces a cgroup rule or a driver-mount list, and has no
  allocation/reservation concept. It implements the **guest** half of the
  nvidia-container-toolkit contract (it *reads* `NVIDIA_VISIBLE_DEVICES`, `/.dockerenv`,
  `/proc/1/cgroup` to discover what some other runtime already set up); stiva would write the
  **host** half. NVIDIA is detected purely by parsing `nvidia-smi` CSV — `/dev/nvidia*`,
  `/dev/dri/*` and `/dev/kfd` are never scanned, so there is not even an incidental path to
  reuse. The one usable seed is `_find_pci_addr:2954` (returns a BDF sysfs path), an
  underscore-prefixed internal.
- [ ] **Device allocation ledger** — "container A holds GPU 0, don't hand it to B." Neither
  library provides it: samay's `node_capacity_reserve` decrements CPU and memory but **not**
  `accel_profiles`, so even samay's accelerator handling is match-only, never exclusive.
  ai-hwaccel supplies the matcher; the ledger is unwritten in both. A natural extension of
  stiva's existing `state.json` + majra lifecycle events, but it is design work no dependency
  hands you.

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
