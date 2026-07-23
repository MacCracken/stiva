# Changelog

All notable changes to stiva are documented here.
Format follows [Keep a Changelog](https://keepachangelog.com/).

## [Unreleased] — toolchain refresh + v3.0.x sync backlog

### Fixed — dependency hygiene: three symbol collisions + an unlocked `lib/`
kavach 3.8.0 took `[deps.samay]`, and samay declares `[deps.ai-hwaccel]`. Because cyrius
auto-includes every **active** `[deps.*]` module into every compilation unit, both bundles
(~279 KB) landed in stiva's `lib/` — **undeclared and unlocked** (83 lock entries against 85
`lib/*.cyr`), pulled in by a `path = "../kavach"` override that silently beats `tag = "3.7.1"`.
They brought three last-def-wins collisions. cycc warns on duplicate *fns* but is **silent on
duplicate structs**, so the struct case surfaced as 8 bogus parse errors in `src/fleet.cyr`.

- **`backend_name` — was live, observable corruption.** ai-hwaccel defines its own over an
  unrelated enum, and **both start at `0`**, so `stiva info` logged
  `computing security strength score: intel-npu` instead of `oci`. Not fixable here: kavach
  calls it in its own error paths (`lifecycle.cyr:165`, `backend_dispatch.cyr:48`). Fixed
  upstream in **kavach 3.8.2**, which renames its OS-backend namer to `os_backend_name` and
  leaves the bare name to ai-hwaccel; stiva's two call sites updated
  (`src/runtime.cyr:841,955`). Verified: `stiva info` reports `oci` again.
- **`struct NodeCapacity` + `node_capacity_new`** → `FleetNodeCapacity` /
  `fleet_node_capacity_new` (`src/fleet.cyr`, `tests/mgmt.tcyr`). samay's is 9 fields / 72 B /
  5 args against stiva's 4 / 32 B / 4 args.
- **`which`** → `stiva_which` (`src/network_rootless.cyr`). stiva's returns a boolean `1/0`,
  ai-hwaccel's a heap cstring pointer, and both callers test `== 1` — the wrong winner makes
  rootless pasta/slirp4netns detection silently report "no backends available".
- `path_exists` also collides but is benign (identical `1/0` semantics).

### Changed — samay + ai-hwaccel are declared, pinned, and default-off
Root cause was upstream: kavach forced 279 KB on every consumer for a bridge it does not ship
(`src/samay_bridge.cyr` is excluded from its `[lib].modules`; `dist/kavach.cyr` has zero samay
references). **kavach 3.8.2** makes both deps `optional` behind a default-on `scheduler`
feature — transitive `[features]` tables are not parsed, so consumers no longer receive them.
stiva now declares both itself, pinned, `optional`, behind a **default-off `accel` feature**,
to be switched on when the roadmap §H/§I work (cron scheduling; accelerator inventory +
placement) lands. `cyrius.lock` is back to **83 entries = 83 `lib/*.cyr`**, `cyrius deps
--verify` clean — restoring the `.gitignore` invariant that `lib/` is reproducible from the
manifest + lockfile. `[deps.kavach]` now pins `3.8.2` and the stdlib subset was re-synced to
the 6.4.66 pin, clearing the `sankoch 2.5.1 (pinned: 2.5.5)` shadow warning.

### Added — `tests/store.tcyr` (4th test unit)
Activating the two bundles pushes `tests/stiva.tcyr` past cycc's identifier cap — a hard
`identifier buffer full (261893/262144)` error. The imagelayout + storage tests (39 functions,
185 assertions) now live in `tests/store.tcyr`, which includes only the 6 `src/` modules they
need instead of all 26. **1184 assertions still green**, now across four files:
stiva 684 · store 185 · runpath 187 · mgmt 128. Note the peel alone did not move the cap
(identifiers dedupe; test bodies only reference symbols the modules already define) — the
buffer is dominated by the vendored `lib/` bundles, which is why the feature gate is the fix.

### Added — group A tails → v3.0.4
The remaining OCI image-layout/transfer polish; group A is now complete.
- **GNU longname/longlink** (`storage.cyr`): the tar writer emits `'L'`/`'K'` pseudo-entries
  (`_tar_write_long`) for names > 255 B (beyond the USTAR name+prefix split) and symlink targets
  > 100 B, and the reader consumes them (`pending_name`/`pending_link`) — no path-length limit on
  `export` / layer unpack. Traversal + symlink-ancestor checks still apply to the reconstructed name.
- **Per-image platform passthrough** (`imagelayout.cyr`): `index.json` manifest descriptors now carry
  the image's **real** `architecture`/`os` (read from its config blob), so a foreign-arch image loaded
  from a docker/oci archive keeps its platform instead of being re-stamped `amd64/linux`; a missing
  config falls back to the host.
- **Empty-registry ref** (`image.cyr`): `image_ref_full_ref` no longer emits a leading `/` for a
  registry-less ref (`bareimg:v1`, not `/bareimg:v1`) — the exact inverse of `_il_parse_full_ref`.
- **Security** (adversarial review of the long-name reader): bound the reconstructed GNU long-name /
  long-link to 8 KiB (< PATH_MAX·2) and rewrote `_stor_has_symlink_ancestor` to O(n) (mutable prefix +
  temporary NUL instead of an O(n²) per-component rebuild) — a crafted multi-MB, gzip-amplified name
  can no longer drive extraction into a byte-copy/`lstat` CPU DoS. The review confirmed the size
  handling, pending-state clearing, round-trip, and writer size-accounting otherwise sound.
- **Tests**: `tar_longname` (>255 B path + >100 B symlink target round-trip + the DoS-cap guard),
  `platform_passthrough` (config arch carried + host fallback), and empty-registry cases — **1184**
  tests green.
- Hit the cycc struct-id-20/21 miscompile again on a `Platform` field read in the new descriptor
  path; sidestepped with literal host values (`normalize_arch("x86_64")`/`"linux"`).

### Added — tar hardening + docker-archive (group A follow-ups → v3.0.3)
Completes the A3/A4 review follow-ups. Hardening-first, then the new read path.
- **USTAR long-name** (`storage.cyr`): the writer splits a path across the `name`(100) + `prefix`(155)
  fields (paths up to 255 B) and the reader reconstructs it — real rootfs `export` no longer fails on
  the common >100-byte path. `_tar_find_split` picks the split; genuinely unsplittable names (single
  >100-byte component, or >255 B) still error (GNU longname is a later item).
- **Base-256 numeric fields**: `_tar_numeric` emits the GNU base-256 escape when a value overflows the
  octal field, and `_stor_parse_octal` decodes it — `size` > ~8 GiB and `uid`/`gid` > 2²¹ no longer
  silently truncate.
- **Symlink write-through protection**: extraction refuses to descend through a symlinked ancestor
  (`_stor_has_symlink_ancestor`) and unlinks an existing symlink at a file target before writing —
  closing the absolute-symlink escape the A3 review flagged (complements the `..`/absolute-name and
  `..`-target rejection already in place).
- **`docker-archive` read** (`imagelayout.cyr`): `stiva load` now accepts a `docker save` tarball
  (`manifest.json` + config + uncompressed layer tars) — stores the config blob verbatim, gzips each
  layer into an OCI gzip blob, assembles + stores an OCI manifest, and indexes under `RepoTags[0]`.
  Reuses the OCI store path with **no new struct types** (keeps clear of the cycc struct-id-20/21
  collision); a config/layer failure skips the image with an error.
- **External-ref round-trip**: `_il_parse_full_ref` now treats a `:` as a tag separator only when it
  follows the last `/`, so a port-registry ref.name (`localhost:5000/repo`, with or without a tag)
  reconstructs correctly (A2 review Finding 4).
- **Security** (adversarial review of the untrusted-input paths): the tar reader now **range-checks
  the size field** (`size < 0 || size > remaining` → reject) — a crafted base-256/overflow size no
  longer slips past the signed torn-region check into a giant `sys_write` (heap over-read/crash) or
  drives `pos` backward (OOB header reads); and `_il_load_docker_archive` now **validates the
  `manifest.json` `Config`/`Layers` names** with `_stor_name_is_safe` before opening them — a `..`/
  absolute path can no longer escape the extract dir to read an arbitrary host file (which `save`
  could then exfiltrate). The review confirmed the path-traversal, symlink-escape, name/prefix
  round-trip, and base-256 writer paths otherwise sound.
- **Tests**: `tar_hardening` (long-name round-trip, base-256, symlink-ancestor block),
  `tar_extract_rejects_bad_size`, `docker_archive_load`, `docker_archive_rejects_traversal`, and
  port-registry cases in `il_parse_full_ref` — **1173** tests green.
- Still deferred: GNU longname for names > 255 B, per-image `platform` passthrough in index
  descriptors (single-node is always `amd64/linux`).

### Added — perms-tar + oci-archive transfer (group A, A3+A4 → v3.0.2)
- **A3 — perms-preserving tar** (`storage.cyr`): `create_tar` now emits real `mode`/`uid`/`gid`
  plus **directory** (`'5'`) and **symlink** (`'2'`, stored as the link — not dereferenced) entries,
  via a `getdents64` + `lstat` walk (`_stor_tar_collect`, new `TarEntry`). The reader
  (`_stor_extract_tar`) applies `chmod` (skipped when a header carries no mode), best-effort
  `lchown` (needs root), and creates symlinks. `_tar_fill_header` generalized to
  `(name, mode, uid, gid, size, typeflag, linkname)`; the tar assembly is factored into a shared
  `_stor_write_tar`. Round-trip tested (mode 0644/0755, subdir, nested file, symlink).
- **A4 — `stiva save`/`load` as `oci-archive`** (`imagelayout.cyr`): two **new CLI verbs**.
  `save <image> <out.tar>` packs a single-image OCI layout (oci-layout + index.json + blobs) into a
  skopeo/podman-compatible `oci-archive`; `load <in.tar>` extracts it, validates the layout, and
  copies its blobs into the store **re-hashing each** (content-verified transfer), merging the index.
  Round-trip validated across two stores (byte-identical blobs). A `docker-archive` (no `oci-layout`)
  is detected and reported as unsupported — the docker→OCI read path is a **v3.0.3** follow-up.
- **cycc bug found + worked around**: the OCI-archive save path surfaced a cycc **struct-field
  miscompile** — `Image` fields read garbage in `image_store_save_archive` specifically (both 6.4.66
  and 6.4.67), while other functions read them fine. Worked around with raw-offset accessors
  (`_img_id`/`_img_layers`/`_img_manifest_digest`/`_layer_digest` in `image.cyr`). Minimal repro +
  upstream report tracked separately.
- **Hardening** (adversarial review): the tar reader now **rejects path-traversal** entry names
  (absolute / `..`) and `..`-escaping symlink targets — restores the safety rust-old got from the
  `tar` crate, for both `load` and layer unpack (a `../../etc/...` entry no longer escapes the
  destination); **chown is applied before chmod** so extracted setuid/setgid rootfs binaries keep
  their bits; `load` now **skips (with an error) any image whose config/layer blob is missing or
  fails digest verification** rather than indexing a broken image; plus a null-digest guard and a
  defensive routing of the load path's `Image.reference` reads through the raw-offset accessor.
  Known follow-ups (v3.0.3): the USTAR 100-byte name limit (needs the `prefix` field / GNU longname
  for long rootfs paths — pre-existing), base-256 numeric fields (size > 8 GiB, uid/gid > 2²¹), and
  absolute-symlink write-through during layer unpack (needs `O_NOFOLLOW` per-component extraction).
- **Tests**: `tar_perms_roundtrip` + `tar_extract_rejects_traversal` (A3) and `oci_archive_save_load`
  (A4) — **1146** tests green.

### Added — OCI image-layout (group A, A1+A2 → v3.0.1)
The local image store is now a **valid OCI image layout** (Docker/Podman/skopeo-interop),
net-new work with the OCI **image-spec** as the oracle (rust-old had only the ad-hoc
`images.json`). New module **`src/imagelayout.cyr`**.
- **A1 — OCI image config blob**: `oci_config_build` / `oci_config_object_jv` assemble a full
  `application/vnd.oci.image.config.v1+json` (`architecture`/`os`/optional `created`/`config{…}`/
  `rootfs{type,diff_ids}`/optional `history`), deterministic bytes (bayan objects are
  insertion-ordered — no HashMap-ordering problem). **`rootfs.diff_ids` is the UNCOMPRESSED tar
  digest** per spec (the rust oracle used the compressed digest — fixed here).
- **A2 — manifest + `index.json` + `oci-layout`**: each image gets an OCI **manifest blob**
  (`registry.cyr` gains `Descriptor`/`OciManifest` JSON serde + `media_oci_config`/`media_oci_layer`);
  the store root now carries an **`oci-layout`** marker and a top-level **`index.json`** (descriptor
  list annotated with `org.opencontainers.image.ref.name`). The ad-hoc **`images.json` is retired** —
  the store’s index/import/tag/remove/gc layer moved to `imagelayout.cyr` and reconstructs `Image`
  records from the on-disk manifest + config blobs. GC roots now include the manifest + config digests.
  Verified end-to-end (`stiva import`→`images`→`rmi`) and byte-validated against the OCI spec on disk.
- **Hardening** (adversarial review): `image_import` now reads the rootfs tar whole-file (was a silent
  64 MiB truncation → incomplete-but-valid-looking image); `_il_index_read_jv` reads `index.json`
  whole-file (no 16 MiB cap) and `add_to_index` **refuses to overwrite a present-but-unparseable index**
  (prevents a corrupt/edited index from orphaning every other image on the next add); `descriptor_from_jv`
  now rejects a non-integer `size` (was coerced to 0).
- **Tests/bench**: registry descriptor/manifest serde round-trips (incl. strict `size`), config-assembly +
  `_il_parse_full_ref` round-trip, a corrupt-index guard test, and the image-store tests rewritten onto the
  real import→layout path (**1106** tests green); new `oci_config_build` (~7µs) / `oci_manifest_to_jv`
  (~15µs) serialize benchmarks.
- **A3** (perms-preserving tar) and **A4** (`save`/`load` as `oci-archive`) are staged for **v3.0.2**.
  Deferred to v3.0.2 with A4 (external-ref interop): `_il_parse_full_ref` handling of a port-registry
  ref.name without a tag, an empty-registry ref.name, and per-image `platform` in index descriptors —
  all only reachable via *externally-produced* layouts (stiva writes `local/<name>:<tag>`, which round-trips).

### Docs + hardening
- **Documentation converted from the Rust/cargo era to Cyrius**: `README.md`,
  `CONTRIBUTING.md`, `docs/cli.md`, `CLAUDE.md`, and the guides/dev docs
  (quick-start, networking, security, testing, scripts) now describe the `cyrius`
  toolchain, the accurate **1033**-test count, and the Live / **v3.0.x (planned)** /
  **v3.1 (blocked)** split for every CLI verb and capability. The async library
  examples are the blocking Stiva facade, labeled "**v3.0.x (planned)** — blocking
  glue over the sync core, not an async rewrite".
- **Lint clean**: wrapped the 11 remaining `>120`-char lines in `main.cyr`/`runtime.cyr`
  → `cyrius lint` is warning-free across all `src/*.cyr`.
- **`scripts/version-bump.sh`** fixed to write only `VERSION` (the removed `Cargo.toml`
  path is gone; `cyrius.cyml` reads `version = "${file:VERSION}"`).

### Changed
- **cyrius toolchain pin 6.4.19 → 6.4.66** (`cyrius.cyml`); re-vendored the
  `[deps].stdlib` subset. The newer compiler is stricter about struct field access,
  which surfaced a latent test bug (`c.exit_code` on a `Container` whose field is
  `exit_status`), now fixed.
- **AGNOS dependency pins bumped to latest release tags:** kavach 3.7.1, majra 2.5.1,
  nein 1.6.4, bote 3.1.4 (core), agnodrm 1.5.0, sakshi 2.4.6, libro 2.8.2 (cmdit 1.1.0
  unchanged).

### Added (v3.0.x synchronous backlog — no async)
- **`oci`**: `parse_bundle` (OCI bundle `config.json` → `ContainerConfig` via bayan JSON),
  `build_state`, `to_oci_status`, and the `OciState` struct + JSON round-trip. Lands in
  `container.cyr` (coupled to the container types). oci ~67% → ~100%.
- **`image`**: `image_store_verify_integrity` — walks `blobs/sha256`, re-hashes every blob
  (whole-file read, no truncation) and reports content-address mismatches. image ~72% → ~90%.
- **`runtime`**: the `/proc` process-tree walk (`container_top`, `is_descendant_of`,
  `read_process_info`) and host↔rootfs copy (`copy_into_container`, `copy_from_container`,
  `copy_dir_recursive`), plus `ProcessInfo` JSON — all synchronous.
- **`registry`**: `CredentialStore` (persistent `~/.stiva/credentials.json` via bayan JSON —
  `default_path`/`load`/`save`/`set`/`get`/`remove`/`to_config`) + `RegistryConfig`/`MirrorConfig`.
- **`build`**: `build_cache_key` + `build_step_to_jv` — serde-exact JSON for the internally-tagged
  `BuildStep` enum (`tag="type"`, lowercase), so cache keys hash-match the Rust oracle
  (pinned by tests against a hand-built hash input).
- **`ansamblu`**: `parse_ansamblu` — TOML ansamblu file → `AnsambluFile` on bayan's flat section
  model (dotted `[services.NAME]` / `[services.NAME.health_check]` / `[networks.NAME]` /
  `[volumes.NAME]` headers, inline-table `env`, `restart`/`replicas`/`health_check` with serde
  defaults). Empty `[volumes.x]` sections (which bayan drops) are recovered by scanning the raw
  headers. Plus `restart_policy_from_name` in `health`.
- **`mcp`**: `mcp_handle_build` / `mcp_handle_ansamblu` — the two fully-synchronous tool handlers
  (parse the Stivafile / ansamblu spec via the ported parsers, assemble a structured `McpResult`).
- **`intents`**: the `Intent` value type + constructors + externally-tagged JSON serde
  (`intent_to_jv` / `intent_from_jv`, `ansamblu_action_from_name`) — the variant payloads
  (Run/Stop/Pull/Ansamblu/Scale/Inspect), independent of the still-deferred NL parser.
- **57 new parity tests** mirroring the corresponding rust-old `#[cfg(test)]` cases (plus
  regression tests for a directory-copy `is_dir` bug, OCI negative-limit handling, and strict
  deserialization of credential files, `parse_ansamblu`, and `intent_from_jv` — all surfaced by
  adversarial parity-verify passes). Suite: **1033 tests** green (stiva 734 · runpath 171 · mgmt 128).

## [3.0.0] — Cyrius Port · synchronous single-node runtime

**Milestone: a working single-node OCI runtime in Cyrius.** Ported stiva from Rust
to the **Cyrius** language (AGNOS ecosystem port pattern). All 16 Rust modules → 25
Cyrius domain modules + the CLI. The Rust crate is frozen at `rust-old/` as the
parity oracle. See `docs/development/roadmap.md` for the module-by-module ledger
and the v3.0.0 parity snapshot.

**Parity: ~61% of the Rust surface (314/515 items), 8 true gaps** — the port stops
cleanly at the sync/async boundary. Algorithm-dense modules are at 85–100% (audit
100 · network 94 · health 92 · storage 89 · ansamblu 85); the low-parity modules
(container 20 · core+cli 13) are async orchestration wrappers whose *capability* is
delivered by a synchronous re-architecture — `stiva run <image>` launches a real
container end-to-end. Full 1:1 parity (pull/push, `exec`, CRIU, MCP live dispatch)
is now **v3.0.x (planned)** blocking work over the sync core, leaving only detached
`run -d` and interactive `exec -it` as the externally-**blocked v3.1** residue.
**855 tests** across 4 files, pin 6.4.19.
Three cycc bugs found + filed upstream; the language was never modified from stiva.

**Scope**: the port covers every module's types, pure logic, and syscall-driven
surface, **plus the synchronous sandbox run path**, with **820 tests green**
(`tests/stiva.tcyr` 779 + `tests/runpath.tcyr` 41) and a clean `dist/stiva.cyr`
bundle. Most of that surface (streaming `logs -f`, sequential layer downloads, the
registry HTTP client, CRIU, nsenter `exec`, the ~40-method `Stiva` facade) is now
**v3.0.x (planned)** blocking work over the sync core; HTTP/TLS
(`sandhi`/`tls_native`) and the async runtime (`lib/async.cyr`) EXIST and are
declared in `[deps].stdlib`. Only detached `run -d` and true multiplexed streaming
remain **v3.1 (blocked)**, gated on kavach `sandbox_spawn` and cyrius stackless
coroutines (tracked in cyrius issue
`2026-07-07-async-runtime-tokio-parity-gaps.md`).

**Sandbox run path WIRED (synchronous).** An adversarial audit proved the run
path was never async-blocked — the kavach Cyrius bundle is 100% blocking (0
`async fn`). So `generate_spec`, `build_sandbox` (policy + backend cascade +
`sandbox_create` + transition-to-RUNNING), `exec_container` (`sandbox_exec` →
`ExecResult`), `send_signal`, `apply_cgroup_limits` (cgroup v2 writes), and
`security_score` are now implemented against it. stiva **launches containers in
the kavach sandbox** as of 3.0.0.

**CLI wired to the run path + container-state layer.** `stiva run <image> [cmd…]`
works end-to-end — image lookup (images.json index) → `prepare_layers` →
`setup_overlay` (with the `container_root/rootfs` fallback) → `generate_spec` →
`exec_container`, printing stdout/stderr, returning the exit code, and
**persisting the record to `state.json`** (mirrors rust-old `container.rs`
create()+start()). The synchronous **container-state layer** is ported:
`container_to_jv`/`container_from_jv` (full Container↔JSON serde via `bayan`) +
`container_state_save`/`load` (atomic tmp+rename, with the Running→Stopped restart
fixup). Live CLI verbs: **`run`, `ps` (-a), `stop`, `rm`, `inspect` (JSON),
`images`, `rmi`, `tag`, `import`, `info`, `convert`**. `exec` (nsenter), `logs`,
and the rest are **v3.0.x (planned)** blocking work; only detached `run -d` is the
**v3.1 (blocked)** residue.

**`import` + `tag` — real images, no hand-staging.** `stiva import <tar> [name]
[tag]` reads a rootfs tar, **gzips it** (`sankoch`), content-addresses it as a
layer blob (`sigil` sha256), writes a minimal OCI config blob, and indexes the
image (`image_import`, mirrors rust-old `import_rootfs`). `stiva tag <src>
<dst>` aliases a local image (`image_store_tag`). Verified end-to-end: `import` →
`run` **gunzips + untars** the imported layer into the overlay and executes it
(the gzip↔tar codec round-trip closes), so stiva runs real imported images
without a hand-written `images.json`.

**Tar WRITER (the one genuine codec gap) + `export` + `gc` + `prune`.**
`create_tar`/`export_rootfs` (`storage.cyr`) emit a byte-exact USTAR archive
(header + checksum + 512-padding + two-zero-block terminator) — verified GNU
`tar tf`/`tvf`/`xf`-readable and round-tripping through the extractor. `stiva
export <ctr> <out.tar>` tars a container rootfs; `stiva gc` sweeps unreferenced
blobs (`image_store_gc`); `stiva prune` drops Stopped containers + unreferenced
images.

**`stats` + `pause`/`unpause` — cgroup v2 (also mis-marked async).** `container_stats`
reads `memory.current`/`memory.max`/`cpu.stat:usage_usec`/`pids.{current,max}`;
`pause`/`unpause` write `cgroup.freeze` (all synchronous fs, like
`apply_cgroup_limits`). The read/parse helpers are unit-tested against fixture
files ("max"→0, usage_usec parse); the CLI verbs gate on a live PID (our one-shot
foreground containers are Stopped, so they correctly report "no running process"
— the happy path needs detached containers, v3.1).

**`logs` + `wait`.** `run` now writes each container's output to
`{root}/containers/<id>/container.log` (byte-exact `write_log` template);
`stiva logs <ctr> [-n N]` tails it (`container_log_tail` via the ported
`container_tail_start`); `stiva wait <ctr>` prints + exits with the recorded
exit code. Live CLI is now **run · ps · stop · rm · inspect · images · rmi · tag ·
import · export · stats · pause · unpause · logs · wait · gc · prune · info ·
convert** (19 verbs). Remaining **v3.0.x (planned)** (blocking, over the sync
core): `exec` (nsenter, non-interactive), `logs -f` (follow), `top`,
`checkpoint`/`restore`, `pull`/`push`, `build` (build-step exec + perms tar); only
detached `run -d` is **v3.1 (blocked)**.

Tests split into **four files** (`stiva.tcyr` 610 · `runpath.tcyr` 163 ·
`mgmt.tcyr` 76 · integration 2 = **851**), run via `cyrius tests tests/`, to stay
under the cycc identifier-dedup cap. Pin → 6.4.19.

Surfaced + filed a second cycc bug: a **struct field-name/offset collision** —
`exit_code` declared in both `Container` (@72) and `ContainerExecResult` (@0)
mis-resolved reads in a large compilation unit (silent 0). Worked around by
renaming the Cyrius field `Container.exit_code` → `exit_status` (JSON key stays
`exit_code`); filed `2026-07-07-struct-field-name-offset-collision.md`. Also
filed the identifier-dedup-cap issue (the test suite is split into
`tests/stiva.tcyr` + `tests/runpath.tcyr`, run via `cyrius tests tests/`).
Toolchain pin → 6.4.18. **847 tests green.**

**sakshi structured logging folded in.** The dropped Rust `tracing::*` surface is
restored: `sakshi_set_level(SK_INFO)` at the CLI entry, run-path + implemented
image/storage/build/audit/network/registry operations emit byte-exact log lines
(verified emitting via ring assertions). Tests split (`stiva.tcyr` +
`runpath.tcyr`, run via `cyrius tests tests/`) to stay under the cycc identifier
cap — filed as cyrius issue
`2026-07-07-lexid-dedup-cap-too-low-for-large-consumers.md`. Toolchain pin → 6.4.17.

**Gaps closed before the tag** — the achievable deferrals I'd wrongly blamed on
"no codec" (the stdlib has the codecs): **`build.parse_build_spec`** (Stivafile
TOML → BuildSpec via `bayan`), the **`image` images.json index** (load/save/add/
list/remove via `bayan` JSON), and **`storage.unpack_layer`/`prepare_layers`**
(gzip via `sankoch` + a hand-rolled USTAR tar reader). Genuine stdlib gaps that
remain (narrow): **zstd** (`sankoch` has gzip/xz/lz4/bzip2, not zstd), a **tar
writer** (build's layer builder), and a **YAML** parser (compose only). See the
roadmap's deferred-surface accounting.

### Summary
- **All 16 modules ported**: error, oci, intents, audit, convert, network
  (mod/bridge/dns/pool/rootless/nat/manager), image, registry, storage, build,
  encrypted, runtime, container, health, ansamblu, agent, fleet, mcp, and the
  crate root (`stiva_core`: StivaConfig + the deferred Stiva facade).
- **AGNOS deps wired** as Cyrius `dist/*.cyr` bundles (kavach/majra/nein/bote/
  agnodrm + sigil/libro/sakshi), probe-validated; ~37 benign cross-bundle
  "duplicate fn (last-def-wins)" warnings (shared agnos helpers).
- **CLI via cmdit** (`src/main.cyr`) — 33 subcommands as cmdit verbs (getopt-long
  + generated help), not hand-rolled. `stiva convert --format dockerfile` works
  end-to-end; the async verbs print a clear "deferred to v3.1" message.
- **820 Cyrius tests** (`stiva.tcyr` 779 + `runpath.tcyr` 41), `cyrius bench`/
  `fmt`/`lint` clean; `dist/stiva.cyr` built.
- **Synchronous sandbox run path wired** + **sakshi structured logging folded in**
  (see the scope note above).
- **Closed three achievable deferrals** with the existing stdlib (refuting the
  earlier "no codec" deferral): build Stivafile TOML parse (`bayan`), image
  images.json JSON index (`bayan`), storage gzip-tar layer unpack (`sankoch` +
  USTAR reader). Async-orchestration deferral tracked upstream via a filed cyrius
  async-parity issue.
- Surfaced + fixed a cycc compiler bug mid-port (struct-id 20/21 ↔ f64v2/f64v4
  SIMD-sentinel collision), filed upstream with a minimal repro
  (`docs/development/cycc-bug-struct-sid-20-21.cyr`) — **fixed in cyrius 6.4.14**.
- Fixed agent-introduced runtime bugs (dangling struct-literal returns, map
  key-type mismatch, single-field-struct value semantics), now in the port
  playbook (`scripts/port-workflow.js`).

### Added
- **Port scaffold** (`cyrius port`) — Rust → `rust-old/` (18,622 lines); Cyrius
  skeleton, `cyrius.cyml`, CI; toolchain pinned 6.4.10.
- **Foundation** — kavach-model multi-module layout: `src/lib.cyr` aggregation
  header, `src/main.cyr` program entry, `[lib].modules` + honest opt-in
  `[deps].stdlib`, Cyrius `.gitignore` (`lib/`, `build/`), `tests/stiva.tcyr`.
- **`src/error.cyr`** — `StivaError` → `STIVA_ERR_*` enum (28 kinds) + name/print,
  exact Rust display strings.
- **`src/oci.cyr`** — leaf OCI surface: `OciStatus`, `oci_version` (1.2.0),
  `parse_signal` (names + numbers). Container-coupled `OciState`/`build_state`/
  `parse_bundle` deferred with `container`.
- **`src/intents.cyr`** — `IntentKind`/`AnsambluAction` + serde-tag names +
  not-implemented `parse_intent`; variant payloads deferred.
- **`src/audit.cyr`** — full audit log: `AuditOperation`/`AuditResult`/
  `AuditEntry`/`AuditLog`, JSON serialize+escape+parse, flock append, reverse
  read-with-limit, `current_user`.
- **`src/convert.cyr`** — `dockerfile_to_toml` (all instruction arms);
  `compose_yaml_to_toml` deferred (needs a Cyrius YAML+Value layer).
- **`src/network_*.cyr`** — the dep-free network surface: `network_mod`
  (NetworkMode/NetworkDriver/Network/ContainerNetwork/NetworkPolicy+nft-rules/
  DnsRegistry), `network_bridge` (bridge/veth via `ip`), `network_dns` (container
  DNS registry + hosts injection), `network_pool` (IpPool/Ipv6Pool/DualStackPool
  CIDR allocation; IPv6 as 16-byte buffers), `network_rootless` (backend detect +
  port-mapping parse; async slirp4netns/pasta spawn deferred). `nat`/`manager`
  wait on the nein dep.
- **`src/image.cyr`** — OCI image: `ImageRef` parse (registry/repo/tag/digest,
  docker.io normalization) + `full_ref`, `Layer`/`Image` structs, `sha256_digest`
  (sigil SHA-256), content-addressable `ImageStore` (new/store_blob/has_blob/
  read_blob with digest verification). JSON index + async pull/push deferred.
- **`src/registry.cyr`** — OCI registry: media-type constants, `Descriptor`/
  `OciManifest`/`Platform`/`OciIndex`/`RegistryCredential` parsing,
  `parse_www_authenticate`, `normalize_arch`, `registry_host`, platform select.
  Async HTTP client + auth + credential store + mirror config deferred.
- **Agent-orchestrated porting harness** — `scripts/port-workflow.js`:
  per-module port from the oracle + adversarial parity verify against `rust-old/`.
  Its verify stage caught real gaps (ENTRYPOINT last-wins; a strstr index-vs-pointer
  bug; IPv4 leading-zero + u16 leading-`+` parse divergences; image mkdir-failure
  propagation), all fixed.
- **315 Cyrius tests** (`tests/stiva.tcyr`) mirroring the Rust `#[cfg(test)]`
  modules; all green (idempotent), plus `cyrius bench`/`fmt`/`lint` clean.
- **SHA-256 via à-la-carte sigil** — `[deps.sigil]` pulls only the hashing chain
  (`crypto_scratch`+`sha_ni`+`sha256`+`sha512`+`hex`) with `freelist`/`thread_local`/
  `atomic`, not the full bundle.

### Notes (cont.)
- Porting image+registry surfaced a **cycc bug** (struct-id 20/21 collided with
  the `f64v2`/`f64v4` SIMD sentinels → "SIMD vector has no named fields"). Root-
  caused, filed upstream with a minimal repro (`docs/development/cycc-bug-struct-sid-20-21.cyr`,
  kept for regression), and **fixed in cyrius 6.4.14**; toolchain pin bumped
  6.4.10 → 6.4.14. The port never modified the language.

### Notes
- Migrated off cargo/clippy for the project — build/test/bench via the `cyrius`
  toolchain (see the porting banner in `CLAUDE.md`). Rust survives only as the
  `rust-old/` oracle.
- Accepted divergences (audit eager-vs-lazy file open; convert ENTRYPOINT JSON
  escapes) are tracked in the roadmap for ADRs at parity-validation.

## [Unreleased]

### Added
- **OCI runtime CLI conformance** — `src/oci.rs` module with `create`/`start`/`state`/`kill`/`delete` interface for containerd/CRI drop-in; `OciState` JSON output per OCI runtime-spec v1.2.0; `parse_bundle()` reads OCI bundle `config.json`; `parse_signal()` accepts names ("SIGTERM") and numbers ("15")
- **Rootless networking** — `src/network/rootless.rs` with slirp4netns and pasta backends for unprivileged container networking; `is_unprivileged()` detects UID + CAP_NET_ADMIN; `available_backends()` auto-detects installed backends; `start_rootless_network()` spawns userspace network stack with port forwarding (slirp4netns API socket / pasta CLI flags)
- **Registry mirror/proxy** — `MirrorConfig` maps registry hostnames to ordered mirror URLs for pull-through caching in air-gapped environments; `RegistryClient::api_bases()` tries mirrors first with original registry as fallback; added `mirrors` field to `RegistryConfig`
- **OCI image layer encryption** — AES-256-GCM `encrypt_layer()` / `decrypt_layer()` behind `encrypted` feature gate; `KeySource::File` and `KeySource::EnvVar` for key material loading; `is_encrypted_media_type()` / `strip_encrypted_suffix()` helpers for `+encrypted` media type detection; added `aes-gcm` and `getrandom` as optional dependencies
- **Structured audit log** — `src/audit.rs` with append-only JSON-lines `AuditLog`; `AuditEntry` records timestamp, operation, container/image ID, user, result, and metadata; concurrent-safe via `Mutex<File>`; `AuditOperation` enum covers create/start/stop/kill/remove/exec/pull/push/checkpoint/restore; wired into `Stiva` for pull, stop, rm, signal, exec operations
- **`StivaConfig.audit_log`** — optional path to enable audit logging
- **Error variants** — `Audit`, `Encryption`, `OciBundle`, `RootlessNetwork`
- 33 new tests (467 total: 456 lib + 10 integration + 1 doc-test)

### Changed
- **`nix` → `rustix`** — replaced `nix 0.29` with `rustix 1.1` for mount, unmount, and signal syscalls; eliminated duplicate `nix` crate from lockfile (367 → 366 deps); dropped 4 unused `nix` feature flags (`sched`, `resource`, `fs`, `user`)
- **`reqwest` 0.12 → 0.13** — updated HTTP client; `rustls-tls` feature renamed to `rustls`
- **`encrypted` feature** — now also enables `aes-gcm` and `getrandom` deps

### Fixed
- **Path traversal in multi-stage builds** — `from_stage` copies now validate stage directory stays under `context_dir` via `starts_with` check
- **FD closure limit** — CVE-2024-21626 mitigation in `pre_exec` now uses `libc::sysconf(_SC_OPEN_MAX)` instead of hardcoded 1024, covering systems with `ulimit -n > 1024`
- **Build layer allocation** — `base_image.layers.clone()` + `extend(cloned)` replaced with `Vec::with_capacity` + `extend_from_slice` + move, eliminating double allocation
- **Socket write completeness** — slirp4netns API socket now uses `write_all()` instead of `try_write()` to prevent truncated port forwarding commands
- **Cryptographic nonce generation** — `rand_nonce()` now returns `Err` instead of silently falling back to timestamp-based entropy when `getrandom` fails
- **`make_verity_config` panic** — replaced `expect()` with `Result` propagation via `?`
- **OCI PID limit overflow** — `u64` → `u32` cast now uses `try_from().unwrap_or(u32::MAX)` instead of silent truncation

### Improved
- **`#[inline]`** — added to `is_descendant_of`, `max_hosts`, `broadcast` (network/pool.rs), `ImageRef::full_ref`
- **`#[must_use]`** — added to `ImageStore::list`, `check_fleet_health`, `plan_rollback`, `decrypt_layer`, `encrypt_layer`

## [2.0.1] — 2026-04-02

### Added
- **Image signature verification** — `ImageStore::verify_signature()` checks for cosign/notation signature artifacts via the referrers API on pull
- **Rootfs integrity verification** — `ImageStore::verify_integrity()` re-computes SHA-256 of all stored blobs and reports corruption (TOCTOU defense)
- **Health check probe execution** — `HealthMonitor::run_probe()` executes health check commands inside running containers via nsenter; `start_probe_loop()` runs probes on a configurable interval
- **Seccomp profile customization** — `ContainerConfig.seccomp_profile` wired through to kavach's `SandboxPolicy.seccomp_profile` (supports "basic", "strict", or custom names)
- **Log rotation** — `ContainerConfig.log_max_bytes` and `log_max_files` enable automatic log rotation with numbered backup files (`.1`, `.2`, etc.)

## [2.0.0] — 2026-04-02

### Added
- **OCI runtime-spec v1.2.0** — `domainname` field on `ContainerConfig` and `RuntimeSpec` for UTS namespace domain name; wired through kavach with `sethostname`/`setdomainname` in pre_exec (after UTS namespace, before seccomp)
- **MCP annotations** — all 9 MCP tools now include `readOnlyHint`/`destructiveHint` annotations per MCP 2025-03-26 spec (pull/ps/inspect = read-only; run/stop/ansamblu/exec/build/push = destructive)
- **CVE-2024-21626 mitigation** — fd cleanup (`close(3..1024)`) in `pre_exec` hook and `stdin(null)` in `exec_in_container()` and kavach's `execute_with_timeout()`/`spawn_process()`/`build_command()` to prevent container escape via leaked host file descriptors
- **Manifest digest verification** — `Docker-Content-Digest` header checked against computed SHA-256 on manifest pull (defense-in-depth against registry MITM)
- **CPU cgroup enforcement** — `apply_cgroup_limits()` now writes `cpu.max` (quota/period) in addition to `memory.max` and `pids.max`
- **Structured MCP output** — `McpResult` now returns `content` array with typed `ContentPart` variants (`Text`, `Resource`) per MCP 2025-03-26; resource URIs use `stiva://containers/{id}` and `stiva://images/{id}` scheme
- **Live MCP tool dispatch** — `handle_tool()` now takes `Arc<Stiva>` and calls real runtime operations (pull, run, ps, stop, exec, push, inspect) instead of returning stubs
- **MCP resources** — `list_resources()` and `read_resource()` expose containers and images as MCP resources with `stiva://` URIs
- **Container annotations** — `ContainerConfig.annotations` field for OCI key-value metadata
- **OCI artifact manifests** — `OciManifest.artifact_type` and `subject` fields for OCI v1.1.0 artifact support (signatures, SBOMs, attestations); `is_artifact()` helper method
- **Foreign layer support** — `Descriptor.urls` field for non-distributable layers; pull pipeline fetches from external URLs when present instead of registry blob API
- **ID-mapped mounts** — `X-mount.idmap=` option added to bind mounts when `rootless=true` (OCI runtime-spec v1.2.0) for proper UID/GID mapping in rootless containers
- **Descriptor annotations** — `Descriptor.annotations` field for per-layer/config metadata
- **Constructor helpers** — `Descriptor::new()`, `Descriptor::foreign()`, `OciManifest::new()` for cleaner construction
- **IPv6 networking** — `Ipv6Pool` for IPv6 address allocation, `DualStackPool` for dual-stack networks, `ContainerNetwork.ipv6` field for assigned IPv6 addresses
- **Network policy** — `NetworkPolicy` type with egress/ingress allow/deny lists, port restrictions, and rate limiting; `to_nft_rules()` generates nftables rules
- **Container DNS resolution** — `DnsRegistry` for container-to-container name resolution within ansamblu sessions; `inject_into()` writes service names to container `/etc/hosts`
- **CNI-compatible types** — network policy and dual-stack types align with CNI spec patterns
- **Image garbage collection** — `ImageStore::gc()` removes unreferenced blobs and unpacked layer directories; `Stiva::gc()` top-level API
- **Container rename** — `ContainerManager::rename()` and `Stiva::rename()` for changing container names
- **Container update** — `ContainerManager::update()` and `Stiva::update()` for live resource limit changes (memory, CPU, PIDs) on running containers
- **IO cgroup limits** — `RuntimeSpec.io_max_bytes_per_sec` field; `apply_cgroup_limits()` writes `io.max` for disk throughput control
- **Rolling updates** — `RollingUpdateConfig` (max_surge, max_unavailable, delay), `plan_rolling_update()` for ansamblu service updates
- **Ansamblu scale** — `compute_scale()` computes add/remove actions, `Stiva::ansamblu_scale()` adjusts replica count at runtime
- **Service logs** — `Stiva::service_logs()` aggregates logs across all replicas of an ansamblu service
- **Fleet health monitoring** — `check_fleet_health()` marks nodes NotReady when heartbeat expires
- **Deployment rollback** — `plan_rollback()` identifies failed nodes and plans container migrations to healthy targets
- **Layer build cache** — content-addressable cache keyed by `sha256(base_digest + step_index + step_json)`; `check_build_cache()` / `record_build_cache()` skip redundant step execution
- **Multi-stage builds** — `BuildStage` type and `FromStage` build step variant for copying artifacts between named stages (equivalent to `FROM ... AS builder`)
- **Registry credential store** — `CredentialStore` persists credentials to `~/.stiva/credentials.json` with per-registry `set()` / `get()` / `remove()` and `to_config()` for `RegistryClient`
- **CRIU pre-dump** — `pre_dump_container()` captures dirty pages incrementally with `--prev-images-dir` chaining for iterative migration
- **CRIU lazy pages** — `restore_lazy()` restores with `--lazy-pages` and `--page-server` for on-demand page transfer during live migration
- **`stiva events`** — CLI command streams container lifecycle events from majra pub/sub in real time
- **`stiva diff`** — CLI command shows filesystem changes in a container by walking the overlay upper layer (C=changed, D=deleted via whiteout)
- **Shell completions** — `stiva completions <bash|zsh|fish>` generates shell completion scripts via clap_complete
- **`stiva rename`** — CLI command for renaming containers
- **`stiva gc`** — CLI command for garbage-collecting unreferenced image blobs
- **Config file** — `~/.stiva/config.toml` loaded at startup for default registry, paths, and log level
- **Security audit log** — `docs/security-audit-log.md` tracking CVE reviews and remediation
- **Spec compliance tracker** — `docs/spec-compliance.md` tracking OCI, MCP, CRIU, and networking spec conformance
- **Roadmap** — `docs/development/roadmap.md` with prioritized work items

### Fixed
- **CVE-2024-24557 hardening** — removed unused tag-keyed manifest cache (`store_manifest_ref`) that could enable cache poisoning if read-back was added; changed image lookups from `.contains()` substring match to exact match
- **RUSTSEC-2025-0067/0068** — replaced unsound `serde_yml` with `serde-saphyr` (safe pure-Rust YAML parser)
- **SPDX license** — `GPL-3.0` → `GPL-3.0-or-later` (valid SPDX identifier)
- **kavach composite backend** — missing `tcp_bind_ports`/`tcp_connect_ports` fields in `merge_policies`

### Changed
- **Dependency updates** — bote 0.50.0 → 0.91.0, majra 1.0.3 → 1.0.4, plus 34 transitive crate updates (hyper, uuid, libc, zerocopy, wasm-bindgen, ICU crates, etc.)
- **bote dependency** — moved from local `path` dep to versioned crates.io dep (`>=0.91`) with `[patch.crates-io]` override, matching kavach/majra/nein pattern
- **YAML parser** — `serde_yaml` (deprecated) → `serde_yml` → `serde-saphyr` (maintained, safe)

## [1.0.0] — 2026-03-25

### Added
- **Persistent state** — container records saved to `state.json`, restored on manager restart; running/paused containers transition to Stopped on restart
- **Container restart** — `ContainerManager::restart()`, `Stiva::restart()`, `stiva restart` CLI; resets Stopped→Created→start()
- **Feature-gate chain** — `runtime` implies `image`+`registry`, `compose` implies `runtime`, `default = full`
- **Integration test suite** — 10 integration tests covering full lifecycle, persistence, export/import, fleet scheduling, copy
- **Doc-test** — crate-level quick start example
- **`stiva info`** — system information (version, paths, container/image counts, CRIU availability)
- **`stiva restart`** — restart stopped containers (26 CLI commands total)
- **Error quality** — user-friendly error messages in CLI (container not found, auth failed, invalid reference, etc.)
- **Credential injection** — `ContainerConfig.secrets` accepts `kavach::SecretRef` for env var / file / stdin secret injection without exposing in config; `--secret KEY=VALUE` CLI flag
- **Security scoring** — `Stiva::security_score()` and `container_security_score(id)` via `kavach::score_backend()`; shown in `stiva info` and `stiva inspect` output
- **Output scanning** — `ContainerConfig.scan_policy` enables `kavach::ExternalizationGate` on exec/logs output; blocks private keys, oversized output, PII per policy
- **`ScanBlocked` error variant** — returned when output scanning blocks container output
- 423 total tests (412 lib + 10 integration + 1 doc-test)

### Changed
- Version: 0.25.4 → 1.0.0
- `ImageStore::add_to_index` and `save_index_pub` now `pub` (were `pub(crate)`)
- `default` feature changed from `runtime` to `full`

## [0.25.4] — 2026-03-25

### Added
- **Long-running daemon containers** — `ContainerConfig.detach = true` spawns containers as background daemons via kavach `spawn()` instead of blocking `exec()`
- **Daemon lifecycle** — `ContainerManager::wait()`, `try_wait()` for daemon containers; `stop()` now sends SIGTERM with configurable grace period before SIGKILL
- **`DaemonHandle`** — wrapper around kavach `SpawnedProcess` with PID tracking, wait, kill, and try_wait
- **`Stiva::wait()`** — top-level API for waiting on container exit
- **kavach `spawn()`** — new `Sandbox::spawn()` method and `SpawnedProcess` type for non-blocking process execution with PID, wait, kill (SIGTERM→SIGKILL), and try_wait
- **`ContainerConfig.stop_grace_ms`** — configurable SIGTERM grace period (default 10s)
- **Image push** — `RegistryClient::push_blob()`, `push_manifest()`, `blob_exists()` for OCI distribution push; `ImageStore::push()` orchestrates config + layer + manifest upload with dedup; `Stiva::push()` top-level API
- **Rootless containers** — `ContainerConfig.rootless = true` enables user namespace with UID/GID remapping; kavach writes `/proc/self/uid_map` and `/proc/self/gid_map` after `unshare(CLONE_NEWUSER)` mapping host UID→0 inside; no real root required
- **`authenticated_request()`** — generic auth method supporting any HTTP method/scope, deduplicated from `authenticated_get()`
- **TOML image build** — `Stivafile` build spec with `run`, `copy`, `env`, `workdir`, `label` steps; `build::parse_build_spec()` parser, `build::build_image()` executor; `Stiva::build()` top-level API; generates OCI layers (tar+gzip) per step with SHA-256 verification
- **Container checkpointing** — `runtime::checkpoint_container()` and `restore_container()` via CRIU; `ContainerManager::checkpoint()` creates checkpoint bundles, `restore()` resumes from them; `Stiva::checkpoint()`/`restore()` top-level API
- **Live migration** — `MigrationBundle` type packages container config + image ref + checkpoint data; `ContainerManager::prepare_migration()` and `apply_migration()` for cross-node container transfer
- **Daimon edge fleet** — `fleet` module with `FleetDeployment`, `DeploymentConstraints`, `DeploymentStrategy` (Spread/BinPack/Pinned), `FleetNode`, `NodeCapacity`, `NodeStatus`; `fleet::schedule()` assigns replicas across nodes; `fleet::select_migration_target()` picks optimal migration destination
- **Container exec** — `runtime::exec_in_container()` via `nsenter` into PID/mount/net/UTS/IPC namespaces; `ContainerManager::exec()` and `Stiva::exec()` APIs
- **Signal forwarding** — `runtime::send_signal()` via nix; `ContainerManager::signal()` and `Stiva::signal()` for sending arbitrary signals (SIGHUP, SIGINT, SIGUSR1, etc.)
- **Pause/unpause** — `runtime::pause_container()`/`unpause_container()` via cgroups v2 freezer (`cgroup.freeze`); `Stiva::pause()`/`unpause()` with Paused state tracking
- **Container stats** — `runtime::container_stats()` reads memory, CPU, PIDs from cgroups v2; `ContainerStats` type; `Stiva::stats()` API
- **Image management** — `Stiva::rmi()` remove images, `tag()` create aliases, `inspect_image()` full details
- **Container inspect** — `Stiva::inspect()` by ID or name
- **Prune** — `Stiva::prune()` removes stopped containers and unreferenced images
- **MCP tools expanded** — 9 tools (+exec, build, push, inspect) with handlers
- **Cgroups v2 enforcement** — `runtime::apply_cgroup_limits()` writes `memory.max` and `pids.max` after daemon spawn; best-effort with warning on failure
- **Network wiring** — `ContainerManager` lazy-creates `NetworkManager`, auto-connects daemon containers to bridge network with port mappings and DNS injection on start
- **Lifecycle events** — majra pubsub events on create/start/stop/remove/pause/unpause; `ContainerManager::event_bus()` accessor for subscribers
- **Log streaming** — `ContainerManager::log_tail(id, lines)` reads last N lines from container log; `Stiva::log_tail()` top-level API
- **CLI binary** — `stiva` command with 24 subcommands: pull, build, push, run, ps, stop, rm, exec, top, inspect, images, rmi, tag, pause, unpause, stats, logs, kill, export, import, cp, prune, wait, checkpoint, restore
- **Container top** — `runtime::container_top()` lists processes via /proc PID tree walk; `ProcessInfo` type
- **Container export/import** — `runtime::export_rootfs()` tar archive, `runtime::import_rootfs()` creates single-layer image from tar
- **Container copy** — `runtime::copy_into_container()` / `copy_from_container()` with recursive dir support
- **Criterion benchmarks** — 18 benchmarks across imageref, volume, port, blob, ippool, fleet, build; `bench-history.sh` generates CSV + benchmarks.md trend
- 393 tests passing

### Changed
- Version bump: 0.25.3 → 0.25.4 (stiva), 0.22.3 → 0.25.3 (kavach)
- `ContainerManager::stop()` — now properly kills daemon processes with SIGTERM→SIGKILL instead of just setting state
- `runtime::exec_container` — refactored to share sandbox setup with `spawn_container` via `build_sandbox()` helper

### Improved
- **P(-1) scaffold hardening** — `#[non_exhaustive]` on all 11 public enums, `#[must_use]` on ~30 pure functions, `#[inline]` on hot-path accessors
- **`Cow` over clone** — `digest_hex()` returns `Cow<str>` avoiding allocation on every blob op
- **`write!` over `format!`** — `sha256_digest()` and env var building avoid temporary allocations

## [0.22.3] — 2026-03-22

### Added
- **Compose orchestration** — `compose_up`/`compose_down` with DAG dependency ordering via majra DagScheduler, topological sort, cycle detection
- **Restart policies** — `Always`, `OnFailure { max_retries }`, `UnlessStopped`, `Never` with restart count tracking
- **Health monitoring** — `HealthMonitor` wrapping majra `ConcurrentHeartbeatTracker`, Online→Suspect→Offline FSM
- **Health check config** — per-service command, interval, timeout, retries in compose files
- **Compose sessions** — `ComposeSession` tracking services, networks, startup order; replica support (N containers per service)
- **Daimon agent integration** — HTTP-based container registration/deregistration/status reporting (`src/agent.rs`)
- **MCP tools** — 5 tools: `stiva_pull`, `stiva_run`, `stiva_ps`, `stiva_stop`, `stiva_compose` with JSON Schema input specs (`src/mcp.rs`)
- **Sutra module** — `sutra-stiva` crate in sutra-community: pull, run, stop, rm, compose_up, compose_down
- **Agnoshi intents** — stub types for future NL→intent parsing: Run, Stop, Pull, Compose, Scale, Inspect (`src/intents.rs`)
- **PubSub integration** — majra pubsub feature enabled for container lifecycle events
- **Benchmark script** — `scripts/bench.sh` appends timestamped test/build timing to `benches/history.log`
- 290 tests passing

### Changed
- Version bump: 0.21.3 → 0.22.3 across stiva, kavach, majra, nein
- majra features: `["queue", "heartbeat"]` → `["queue", "heartbeat", "pubsub"]`

## [0.21.3] — 2026-03-21

### Added
- **Phase 0 — Foundation** — Scaffold with module structure, image reference parser, container lifecycle state machine, OCI manifest/descriptor types, volume mount parsing, network mode types, TOML compose parser, runtime spec generation
- **Phase 1 — Image Pull Pipeline** — OCI distribution spec client (manifest fetch, blob download), bearer token auth (Docker Hub, GHCR), multi-arch manifest list support, content-addressable blob store with SHA-256 verification, layer deduplication, concurrent downloads, image index persistence
- **Phase 2 — Container Execution** — Layer unpacking (tar+gzip), overlay filesystem (overlayfs on Linux), kavach sandbox integration (OCI + Process backends), full OCI runtime spec (resource limits, mounts, env, user, workdir), volume bind mounts, container logging, one-shot execution model
- **Phase 3 — Networking** — Network module restructured to submodule (pool, bridge, nat, dns, manager), IP address pool, bridge + veth management via `ip` commands, NAT + port mapping via nein, DNS injection, NetworkManager lifecycle

### Removed
- Unused dependencies: `anyhow`, `async-trait`, `oci-spec`, `tracing-subscriber`

### Fixed
- `ImageRef::parse` port-in-registry bug (`localhost:5000/image` misparsed)
- `ContainerManager::remove` used `AlreadyRunning` error instead of `InvalidState`
- `compose::parse_compose` used `Runtime` error instead of `Compose`
