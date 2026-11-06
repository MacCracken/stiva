# Stiva — Claude Code Instructions

## Project Identity

**Stiva** (Romanian: stack) — OCI container runtime — image management, container lifecycle, orchestration

- **Type**: Crate with library + CLI binary (`stiva`)
- **License**: GPL-3.0-or-later
- **Toolchain**: Cyrius, pin **6.7.18** (the Rust oracle at `rust-old/` targeted MSRV 1.89)
- **Version**: SemVer, currently 3.2.0
- **Genesis repo**: [agnosticos](https://github.com/MacCracken/agnosticos)
- **Philosophy**: [AGNOS Philosophy & Intention](https://github.com/MacCracken/agnosticos/blob/main/docs/philosophy.md)
- **Standards**: [First-Party Standards](https://github.com/MacCracken/agnosticos/blob/main/docs/development/applications/first-party-standards.md)
- **Recipes**: [zugot](https://github.com/MacCracken/zugot) — takumi build recipes

## ✅ Porting Status — the Rust → Cyrius port is COMPLETE; the v3.0.x line is closed out

Stiva was ported from Rust to the **Cyrius** language (AGNOS ecosystem port pattern).
All 16 Rust modules → **27** Cyrius `src/*.cyr` domain modules (incl. the net-new
`imagelayout.cyr` and `cron.cyr`) + `lib.cyr` (aggregation header) and `main.cyr`
(entry + CLI). The CLI registers **36 verbs, 33 live**: `checkpoint` and `restore` print
"not yet wired" (they need CRIU, roadmap v3.4.1), and `restart` cannot succeed (v3.2.1).

**2892 tests** across the `.tcyr` files (stiva 667 · runpath 902 · registry 444 ·
store 387 · mgmt 354 · convert 138; run via `cyrius test tests/`), **164** CLI smoke
assertions (`./scripts/cli-smoke.sh`), 16 benchmarks, `dist/stiva.cyr` built, pin **6.7.18**.

**Every roadmap group A–K is complete.** In release order: group A = the OCI
image-layout + transfer surface (v3.0.1–v3.0.4) · §G output scanning + `convert compose`
(v3.0.6) · §C the `ContainerManager` + `Stiva` facade (v3.0.7) · §B the blocking registry
client, `pull`/`push` live (v3.0.8–v3.0.10) · the Tier-1 CLI sweep (v3.0.11) · §E MCP live
dispatch, `logs -f`, `events` over a persisted `{root}/events.jsonl` (v3.0.12–v3.0.13) ·
§K containers actually enter their rootfs (v3.0.14) · §F `build` and §D non-interactive
`exec` (v3.0.15) · OCI whiteouts, `stiva diff`, rootless networking, §H `cron`, §I
accelerator inventory + placement (v3.0.16–v3.0.17). See
`CHANGELOG.md` for what each release actually contained, and
`docs/development/roadmap.md` for what is left (v3.2 → v3.6).

**Group A (OCI image-layout + transfer) detail**, since it is net-new rather than ported:
the local store is a valid **OCI image layout** (`oci-layout` + `index.json` +
`blobs/sha256/`, the ad-hoc `images.json` **retired** — the store/index/import/save layer
lives in `imagelayout.cyr`); full OCI image **config + manifest** assembly (deterministic
bytes — the old "serde HashMap ordering" worry was Rust-only; bayan objects are
insertion-ordered); a **perms-preserving USTAR tar** codec (mode/uid/gid + dir/symlink,
GNU longname, base-256; traversal/symlink/DoS-hardened, and OCI whiteouts applied rather
than extracted literally); and **`save`/`load` as `oci-archive`** plus a
**`docker-archive` read** path. Each increment was adversarially reviewed.

The old **v3.1 async milestone was dissolved**: the async substrate (`lib/async.cyr`) is
ready and the runtime is single-threaded run-to-completion, so the container-orchestration
surface was blocking work over the ported sync core, and it all landed on the v3.0.x line.

⛔ **Read any "blocked" label sceptically.** An adversarial audit on 2026-08-21 attacked all eight
items then labelled blocked and found **zero** genuinely so — including CRIU, whose stated
kavach prerequisite was mis-diagnosed (a PID namespace is one `clone()` with constants already
in the tree), and `exec -it`, whose cyrius-coroutine prerequisite had been resolved eight
toolchain releases below the current pin. See **Blocks and claims** below for why they were
wrong. JSON/TOML/YAML/base64 (`bayan`), HTTP/TLS (`sandhi`/`tls_native`),
gzip/zstd/xz/lz4/bzip2 (`sankoch`), threads (`lib/thread.cyr`) and async (`lib/async.cyr`)
all EXIST.

`rust-old/` stays the oracle for the ported modules; the group A OCI-layout/transfer
surface, `cron.cyr`, whiteouts and `diff` are **net-new**, so the OCI **image-spec** (not
rust-old) is their oracle.

> ⚠️ **cycc struct-id miscompile — last verified live at 6.4.78; NOT re-verified at the
> current 6.7.18 pin. Assume live.** The suite is green at 6.7.18 with the workarounds in
> place, which this bug has already demonstrated is not evidence of anything. The struct-id
> **20/21 ↔ SIMD f64v2/f64v4 sentinel collision** makes typed field access
> (`var x: T = p; x.field`) read
> **garbage — usually silently**, sometimes segfault. It is **per-function** *and*
> **per-compilation-unit**.
>
> Full write-up, including the 2026-07-24 worked example of a probe that certified the bug
> fixed hours before it silently corrupted a registry cache key:
> **`docs/architecture/001-cycc-struct-id-miscompile.md`**. Read it before touching
> `Image` / `Layer` / `Platform` field access or "cleaning up" a raw-offset accessor.
>
> The short version for day-to-day work:
> - The raw-offset accessors (`_img_id` / `_img_layers` / `_img_manifest_digest` /
>   `_layer_digest`, and the `load64(p + N)` reads in `scan_output`,
>   `container_manager_start`, `_fleet_select_and_reserve`, and the accel branch of
>   `node_matches_constraints`) are **deliberate**. Leave them.
> - *New* code touching these structs is free to use `x.field` — §B, §C and later work do,
>   and are green.
> - **Do not attempt retirement without a test that would fail on a silent wrong value**, not
>   just one that survives a segfault. That is what fooled the 6.4.76 and 6.4.77 attempts.

- **The Rust crate is frozen at `rust-old/`** — it is the **parity oracle**. The
  bar for every ported module is "matches what the Rust did." Do NOT edit it.
- **Use the Cyrius toolchain, NOT cargo.** Any lingering `cargo` / `clippy` /
  `rustc` reference describes the pre-port Rust flow; Rust survives only as the
  `rust-old/` oracle. The live commands are:

  | Action | Command |
  |---|---|
  | Resolve deps | `cyrius deps` |
  | Vendor stdlib subset into `lib/` | `cyrius lib sync` (run after changing `[deps].stdlib`) |
  | Build | `cyrius build src/main.cyr build/stiva` |
  | Release build | `cyrius build --dce src/main.cyr build/stiva` (what `release.yml` ships; `cyrius test` reads only `CYRIUS_DCE=1`, not `--dce` or `[build] dce`) |
  | Run tests | `cyrius test tests/stiva.tcyr` |
  | Bench | `cyrius bench tests/stiva.bcyr` |
  | Format / lint | `cyrius fmt <file> --check` · `cyrius lint <file>` (per-file) |
  | Audit sweep | `cyrius audit` |

- **Structure**: one domain `src/*.cyr` module per Rust module (kavach model);
  `src/lib.cyr` = aggregation header; `src/main.cyr` = program entry (includes +
  CLI); `cyrius.cyml` `[lib].modules` drives `cyrius distlib` → `dist/stiva.cyr`.
- **Port process & sequencing**: `docs/development/roadmap.md` (v3.0.0). Ported
  modules and the module-by-module workflow live there; `scripts/port-workflow.js`
  is the agent-orchestrated porting harness. Canonical Cyrius exemplars to copy:
  `src/error.cyr`, `src/oci.cyr`.
- **AGNOS deps** are consumed as Cyrius `dist/*.cyr` bundles (kavach/majra/nein/
  bote-core), wired in `cyrius.cyml` `[deps.*]` as their consuming modules land.

## Stack

| Crate | Role |
|-------|------|
| kavach | Sandbox isolation (seccomp, Landlock, namespaces, gVisor, Firecracker, WASM) — pin **3.14.5** |
| majra | Job queue, heartbeat FSM, pub/sub — pin **2.13.0** |
| nein | nftables firewall, NAT, port mapping — pin **1.9.1** |
| bote-core | MCP core service (JSON-RPC 2.0, tool registry, structured output) — pin **3.8.15** |
| cmdit | CLI parsing, verb introspection, shell completions — pin **1.5.0** |
| samay | Cron expression parsing + scheduling (the `cron` module) — pin **1.5.1** |
| ai-hwaccel | Accelerator inventory + placement profiles — pin **2.5.6** |
| sakshi · libro | Structured logging · docs tooling — pins **2.5.10** / **2.11.3** |
| sigil | Full crypto bundle (SHA-256/HMAC for image digests + the AGNOS bundles' crypto) — pin **3.13.15** |

All AGNOS deps are consumed as Cyrius `dist/*.cyr` bundles, wired in `cyrius.cyml`
`[deps.*]` **by git tag**.

> ⚠️ **`[deps.sigil]` must stay declared FIRST, at the version the toolchain pin folds.** libro declares its
> own `[deps.sigil]` selecting four *thin* sub-bundles; stiva takes the full monolith, which
> already inlines all four. Claiming the name before the transitive walk reaches libro is
> what keeps both surfaces out of one compilation unit — and that is a hard build failure,
> not a warning: two copies push cycc's 16-slot `#define`/flag table to 17.
>
> ⚠ **The tag does not choose what compiles.** `sigil` and `sakshi` are also `[deps].stdlib`
> leaves, so `lib/sigil.cyr` and `lib/sakshi.cyr` are the pinned snapshot's folds (`cyrius deps`
> prints `kept (stdlib snapshot)`). A sigil fix reaches stiva when the **toolchain** pin moves.
> Keep the tag equal to the fold's `# Version:` line so the manifest says what builds: 3.0.21
> and 3.0.22 said 3.12.16 while 3.13.4 compiled. Dropping `sigil` from `[deps].stdlib` does not
> change this: measured at 6.7.9 with the tag at 3.13.15, `lib/sigil.cyr` stayed the 3.13.11 fold.

> ⚠️ **A dep's `.deps` sidecar may name stdlib leaves ONLY.** A consumer resolves every line
> in it against the pinned toolchain snapshot, so a git-dep bundle listed there is an
> assertion stiva cannot satisfy — `cyrius deps` fails outright. `cyrius distlib` excludes
> named deps automatically, but only when the `[deps.NAME]` section name matches the module
> basename. That mismatch is what broke 3.0.17's CI (nein shipped `bote-core` as a leaf; see
> the CHANGELOG). If a dep bump ever reintroduces `dep X requires 'Y' but it is not in the
> cyrius stdlib`, the fix is upstream in X's manifest, not in stiva's `[deps].stdlib`.

> ⚠️ **A name a transitive manifest declares is not yours until you claim it.** nein declares
> `[deps.bote-core]`. stiva's section used to be `[deps.bote]`, so on 3.1.1's first resolve
> `lib/bote-core.cyr` was nein's 3.8.9 while this manifest said 3.8.15, and the build and suite
> stayed green. Name a section for its module basename. `cyrius deps` prints a `not used` note for
> the pin it passes over; the byte-identity check in the re-pin rule below is what caught this one.

> ⚠️ **A default build always builds the tag (cyrius 6.7.6+).** A `path` beside `git` / `tag` is
> used only in local mode. To develop a dep alongside stiva, uncomment its `path` line and run
> with `CYRIUS_LOCAL=<dep>`, which resolves into `build/local-deps/lib/`, leaves `lib/` and
> `cyrius.lock` alone, and prints how far the sibling is past the tag. Then **tag and release the
> dep and bump the `tag` here.** Below 6.7.6 a `path` silently won over its tag, which once meant
> a green local suite and a red CI with no diff to look at, so the commented-out lines stay
> commented out.

> ⚠ **Bumping a `tag` and re-resolving `cyrius.lock` are ONE change. Never commit them apart.**
> A pin bumped without `cyrius deps` leaves the lock recording the *previous* release, and the
> CI dep-bundle gate then fails with a hash mismatch — correctly, but its message used to name
> a path override as the cause, which sent 3.0.18 looking for an override that did not exist.
> The tell is in the lock itself: its `commit <sha> <dep> <url> <tag>` line names a different
> version than the `[deps.<dep>]` tag it is supposed to describe. If a dep is not tagged yet,
> **do not bump the pin** — finish the upstream release first, or the tree cannot resolve and
> the lock cannot be regenerated. (The gate's message now names this cause first.)

> ⚠ **A dep's tag for a toolchain pin is its newest tag whose own `cyrius` pin is ≤ ours — not
> its latest tag — and a package the stdlib folds (sigil, sakshi, bayan, patra) takes the version
> the snapshot folds,** even when a newer tag is built on our pin: at 6.7.9 that is sigil 3.13.11,
> not 3.13.15. Read a fold from `~/.cyrius/versions/<pin>/lib/<name>.cyr`'s `# Version:` line. Read
> a dep's pin out of each tag (`git show <tag>:cyrius.cyml`), and check that the
> set agrees with itself: each dep's own `[deps.*]` tags should name the versions chosen for the
> others. To re-pin, `rm -rf lib cyrius.lock && cyrius deps`. ⛔ `cyrius deps --relock` keeps every
> old tag's `commit` row beside the new one, which is the stale-lock tell above. Then check that
> each row names one version and `lib/<dep>.cyr` is byte-identical to `<tag>:dist/<dep>.cyr`.

## Consumers

daimon (container management), sutra (fleet deployment)

## Modules (16 Rust modules + the net-new `imagelayout` and `cron`)

27 domain `src/*.cyr` modules, plus `lib.cyr` (aggregation header) and `main.cyr` (entry
+ CLI). `network/` is 7 files (`network_mod` + bridge/dns/manager/nat/pool/rootless);
`oci`, `audit` and `stiva_core` are support modules under the table's headings.

| Module | Purpose |
|--------|---------|
| `image` | Substrate: image reference parsing, `Image`/`Layer`/`ImageRef` structs, content-addressable **blob store** + sha256 digests, integrity verify (the store/index/import layer moved to `imagelayout`) |
| `imagelayout` | **Net-new (v3.0.1–v3.0.4):** OCI image-layout (`oci-layout` + `index.json` + blobs), config/manifest assembly, the index.json-backed store (load/add/remove/gc/import/tag), and `oci-archive` + `docker-archive` `save`/`load`; **`image_resolve`**, the one rule for what a user may type for an image; **`image_config_read`**, what an image says about running it |
| `container` | Container lifecycle, state persistence, lifecycle events (majra pub/sub **+ the rotated `{root}/events.jsonl` log behind `stiva events`**), restart, rename, update |
| `runtime` | OCI spec, kavach integration, cgroups (CPU/mem/PID/IO), CRIU (checkpoint/pre-dump/lazy), exec, signals, export/import, copy |
| `network/` | Bridge, NAT, DNS, IP pools (IPv4+IPv6), port mapping, network policy, container DNS registry, **rootless networking** (slirp4netns/pasta spawn + port forwarding) — 7 files |
| `storage` | Overlay FS, volume mounts, layer unpacking (gzip + zstd); **perms-preserving USTAR tar** codec (mode/uid/gid, dir/symlink, GNU longname, base-256; traversal/symlink/DoS-hardened); **OCI whiteouts** (`.wh.*` / `.wh..wh..opq`) applied during unpack |
| `registry` | OCI distribution client (pull + push + chunked upload), token auth, credential store, tag listing, catalog, referrers API |
| `build` | TOML-based image builds (Stivafile), multi-stage builds, build cache; the **`build_image` driver** — one gzip layer per `copy` step, content-fingerprinted cache keys, full OCI config + manifest |
| `ansamblu` | Multi-container orchestration, DAG ordering, rolling updates, scaling, service logs |
| `health` | Heartbeat monitoring, restart policies |
| `cron` | **Net-new (v3.0.17):** scheduled containers over samay — `{root}/cron.json` entry table, expression validation, `cron_due_at`, `CRON_SKIP` missed-schedule policy |
| `fleet` | Edge fleet scheduling (spread, bin-pack, pinned), health monitoring, rollback planning, **accelerator-aware placement** (`accel_profiles` / `accel_req` / `accel_min_chips` over ai-hwaccel) |
| `agent` | Daimon agent registration |
| `mcp` | 9 MCP tools with structured output, live dispatch, resource exposure |
| `convert` | Dockerfile **and** docker-compose YAML → Stivafile TOML (compose over a documented bayan-YAML subset; output is escaped, unlike the oracle) |
| `encrypted` | LUKS + dm-verity (feature-gated) |
| `intents` | Agnoshi intent stubs |
| `error` | Error types |

CLI binary: `stiva` — **36 registered verbs, 33 live**. `checkpoint` and `restore` print a
"not yet wired" message because both need CRIU (roadmap v3.4.1), and `restart` cannot succeed (see
`docs/cli.md`).

**Verb ids are registration-ordered.** `cmdit_verb(h, ...)` hands back an id derived from
registration order, and the dispatch table is indexed by it — so inserting a verb anywhere
but the end silently renumbers every verb after it (this is how `stiva cron ls` once
dispatched to `completions`). Register new verbs **last**, and use
`cmdit_verb_trailing_after(h, id, n)` for trailing-positional verbs.

## kavach Integration

Stiva uses these kavach features — keep them wired:

- **Sandbox** — `Sandbox::create`, `exec`, `spawn`, `destroy`
- **sandbox_exec_argv / sandbox_spawn_argv** — real argv for a command of two or more elements. One
  element stays a command line: `_rt_is_command_line` says why
- **SpawnedProcess** — daemon containers (pid, wait, kill, try_wait)
- **SandboxPolicy** — memory, CPU, PID limits, seccomp, network
- **SandboxConfig** — hostname, domainname (UTS namespace, OCI v1.2.0)
- **CredentialProxy / SecretRef** — secret injection via env var / file
- **StrengthScore / score_backend** — security scoring in inspect/info
- **ExternalizationGate** — output scanning for secrets/PII. A foreground `run` goes through
  `sandbox_exec_set_ext_policy` with `_rt_warn_policy`: findings warn and secrets redact, and a
  container's own `scan_policy` stands. `logs --scan` keeps the blocking default. `run -d` is not scanned
- **User namespaces** — rootless containers (UID/GID mapping)
- **NO_NEW_PRIVS** — on every path. The OCI backend, the default foreground path, gets it and the
  seccomp filter from kavach's `oci_generate_spec` since 3.14.4, and a bounded capability set and a
  masked `/proc` since 3.14.5. ⛔ `exec` has no filter, and `run -d` and the process backend hold every
  capability in a user namespace (`SECURITY.md`)
- **FD cleanup** — `_stor_cloexec_fds` marks every fd above stderr close-on-exec before a payload
  starts (CVE-2024-21626 mitigation). kavach's own sweep stops at 1024

## Development Process

### P(-1): Scaffold Hardening (before any new features)

0. Read roadmap, CHANGELOG, and open issues — know what was intended before auditing what was built
1. Test + benchmark sweep of existing code
2. Cleanliness check: `cyrius fmt <file> --check` + `cyrius lint <file>` (per changed file), then `cyrius audit`
3. Get baseline benchmarks (`./scripts/bench-history.sh`)
4. Initial refactor + audit (performance, memory, security, edge cases)
5. Cleanliness check — must be clean after audit
6. Additional tests/benchmarks from observations
7. Post-audit benchmarks — prove the wins
8. Repeat audit if heavy
9. Documentation audit — ADRs, source citations, guides, examples (see Documentation Standards in first-party-standards.md)

### Development Loop (continuous)

1. Work phase — new features, roadmap items, bug fixes
2. Cleanliness check: `cyrius fmt <file> --check` + `cyrius lint <file>` (per changed file), then `cyrius audit`
3. Test + benchmark additions for new code
4. Run benchmarks (`./scripts/bench-history.sh`)
5. Audit phase — review performance, memory, security, throughput, correctness
6. Cleanliness check — must be clean after audit
7. Deeper tests/benchmarks from audit observations
8. Run benchmarks again — prove the wins
9. If audit heavy → return to step 5
10. Documentation — update CHANGELOG, roadmap, docs, ADRs for design decisions, source citations for algorithms/formulas, update docs/sources.md, guides and examples for new API surface, verify recipe version in zugot
11. Version check — VERSION (cyrius.cyml reads it via `${file:VERSION}`) + recipe (in zugot) in sync
12. **Claim check — after ANY dep or toolchain bump.** Re-run the probes behind every `block`
    stanza in the roadmap and re-grep every symbol-absence claim. A pin move invalidates the
    conclusions drawn from the old pin, not just the version string. See **Blocks and claims** below.
13. Return to step 1

### Blocks and claims — the rule that came out of the v3.1.0 audit

An adversarial re-audit on 2026-08-21 found that **eight of eight "blocked on upstream" claims in
the v3.1.0 roadmap were wrong.** Seven items were never blocked at all. The same habit had shipped
three live defects — containers got no environment, `stiva run` was refused for most images, and
`_stor_lchown` was a latent aarch64 process-killer sitting behind a false "cannot be fixed".

The cause is mechanical: this tree writes a conclusion about a dependency once, cites a symbol as
evidence, and never re-runs the evidence. When every pin moved on 2026-08-21 the roadmap edit was
one line — the version inventory.

Four rules, binding on new work:

- **Never write "cannot", "impossible", or "not on stiva effort" without a probe.** The full
  stanza format and the seven rules are in `docs/development/roadmap.md` → **Recording a block**.
  No stanza means the item is *unstarted*, not blocked.
- **A symbol's absence is not a capability's absence.** The unstated premise is always "…and stiva
  must go through this library's API." Check it. stiva bypasses kavach for `exec`
  (`exec_in_container`) and nein for `apply` (`_network_manager_run_nft`), both
  deliberately and both documented. A library boundary here is a choice, never a wall.
- **Never type a number about a dependency** — struct sizes, field counts, version-gated
  behaviour. Read it out of `lib/` at the moment you write it, or leave it out. "`SandboxConfig`
  is 8 fields / 64 bytes" was wrong on the day it was typed.
- **A green report is not a green suite — check the EXIT CODE.** `cyrius test tests/` exits
  non-zero on any failure and prints `  FAIL:` lines; that is the signal. Do not summarise the
  run by parsing the summary lines unless you have verified the parse against a line that
  actually contains a failure. ⚠ In 3.0.19 an aggregate `awk -F'[ ,]' '{f+=$3}'` over
  `"324 passed, 1 failed (325 total)"` read the **empty field** produced by `", "` and reported
  `0 failed` regardless of the truth — so a real failure was reported as green for several
  turns, and CI caught what local verification had said was clean. Prefer `exit code`, then
  `grep -c '^  FAIL'`, and only then a count.
- **Assemble and deliver are separate steps; test the delivery.** Five `RuntimeSpec` fields were
  written and never read (`env`, `mounts`, `namespaces`, `workdir`, `user`). `env` shipped that way
  through 2175 green tests because the test asserted the spec *carried* the value, mirroring the
  oracle's own unit test, and nothing asserted a container could *read* it. When you add a field to
  a spec struct, **grep for a reader before assuming it is wired**, and assert on what the payload
  observes.

⚠ **Inherited oracle prose is not evidence.** Comments ported from `rust-old/` carry claims that
may have been false before the oracle froze — `src/intents.cyr:4` still says agnoshi "does not
exist yet", copied verbatim, false since 2026-04. Port the behaviour; re-derive the claim.

### Key Principles

- **Never skip benchmarks.** Numbers don't lie. The CSV history is the proof.
- **Tests + benchmarks are the way.** 2892 Cyrius tests across `tests/*.tcyr`. Keep adding — mirror the rust-old `#[cfg(test)]` cases (group A is net-new, so its tests match the OCI spec + real round-trips).
- **Own the stack.** If an AGNOS crate wraps an external lib, depend on the AGNOS crate.
- **No magic.** Every operation is measurable, auditable, traceable.
- **`#[non_exhaustive]`** on all public enums.
- **`#[must_use]`** on all pure functions.
- **`#[inline]`** on hot-path functions.
- **`write!` over `format!`** — avoid temporary allocations.
- **Cow over clone** — borrow when you can, allocate only when you must.
- **Vec arena over HashMap** — when indices are known, direct access beats hashing.
- **Feature-gate optional deps** — consumers pull only what they need.
- **tracing on all operations** — structured logging for audit trail.
- **Persist state** — container records survive daemon restart via `state.json`.
- **Lifecycle events** — all state changes publish to majra pub/sub.

## Documentation Structure

```
Root files (required):
  README.md, CHANGELOG.md, CLAUDE.md, CONTRIBUTING.md, SECURITY.md, CODE_OF_CONDUCT.md, LICENSE

docs/ (required):
  architecture.md — module map, data flow, consumers
  development/roadmap.md — completed, backlog, future, v1.0 criteria

docs/ (when earned):
  adr/ — architectural decision records
  guides/ — usage guides, integration patterns
  examples/ — worked examples
  standards/ — external spec conformance
  compliance/ — regulatory, audit, security compliance
  sources.md — source citations for algorithms/formulas (required for science/math crates)
```

## Testing

**2892 Cyrius tests** across `tests/*.tcyr` (stiva 667 · runpath 902 · registry 444 ·
store 387 · mgmt 354 · convert 138), each mirroring the matching rust-old `#[cfg(test)]`
cases where one exists. The suite is split across files because a monolith hits the cycc
identifier-dedup cap — split by **include set**, not by test count.

⚠ **Mirroring the oracle's tests is necessary and NOT sufficient.** The oracle's unit tests
assert what a function *assembles*; several of stiva's bugs have been in what it *delivers*
across a library boundary, which neither suite tested. `env` shipped broken through 2175 green
tests for exactly this reason. When a value crosses into kavach, assert on what the payload
observes — see **Blocks and claims** above.

⚠ **A test's `/tmp` paths are fixed**, so two `cyrius test` runs at once interfere. Do not run a suite
while a mutation pass runs in a copy of the tree.

⚠ **An image is untrusted input, and `alloc` never frees.** Its config and rootfs choose how much work
stiva does, so a loop over either needs a bound, and allocating in one is how a 40 KB `PATH` once cost
2.3 GiB. A link in a rootfs resolves against the rootfs and never the host (`_stor_rootfs_is_program`).
Refuse an image before unpacking it, so it leaves nothing for `rm` to find. Anything an image says goes
through `_img_printable` before it reaches a log or a terminal. See the 3.2.0 CHANGELOG.

⚠ **`json_v_obj_set` appends; it never replaces.** Since bayan 1.11.1 one duplicate key refuses the
whole document, and several callers read that 0 as an empty store (`container.cyr` "starting
fresh", `cron.cyr` "treating the schedule as empty"). A writer whose keys come from data must
iterate a hashmap or de-duplicate first, as `_il_kv_obj` does, and its test must parse what it wrote.

`src/main.cyr` is covered separately by `./scripts/cli-smoke.sh` (**164** assertions
against the built binary, run in CI on the default and `--dce` binaries) — the `.tcyr` files
cannot include `main.cyr`.

⚠ **A smoke gate asks the host, never stiva.** `unshare -Urm true`, not "did `stiva run` work",
which turns a broken `run` into a green skip. A `skip` is counted in the summary, and CI turns each
into a warning, so read them. ⛔ The CI step's `sudo sysctl` is for the runner: never run it on a
developer's machine, and strip it from any local replay of the step.

```bash
cyrius test tests/             # run every tests/*.tcyr (the full suite)
cyrius test tests/stiva.tcyr   # run one test file
./scripts/cli-smoke.sh         # CLI smoke assertions against build/stiva
cyrius bench tests/stiva.bcyr  # benchmarks
./scripts/bench-history.sh     # benchmarks + CSV + trend report
./scripts/bench.sh             # test + build timing history
```

## DO NOT

- **Do not commit or push** — the user handles all git operations (commit, push, tag)
- **NEVER use `gh` CLI** — use `curl` to GitHub API only
- Do not add unnecessary dependencies — keep `[deps].stdlib` honest/opt-in
- Do not return dangling struct literals or use sentinel-less error paths in library code
- Do not edit `rust-old/` — it is the frozen parity oracle
- Do not skip benchmarks before claiming performance improvements
