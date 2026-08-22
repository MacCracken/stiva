# Changelog

All notable changes to stiva are documented here.
Format follows [Keep a Changelog](https://keepachangelog.com/).

## [Unreleased]

## [3.0.19] — 2026-08-21 — `run -w` was accepted and ignored; the MCP `stiva_run` message named a blocker retired in v3.0.14

Both found by the 3.0.18 documentation audit, which verified every doc claim against the code
rather than against the previous doc. 2183 → **2186** assertions.

### Fixed — `stiva run -w /app` was accepted and silently ignored

The flag is registered (`src/main.cyr:255`), read (`:285`), stored on the config, and threaded
onto `RuntimeSpec.workdir` by `generate_spec` (`src/runtime.cyr:536`) — **and nothing read it.**
The container ran in `/` regardless. `RuntimeSpec.user` is the same shape and is still open
(roadmap v3.1.0 item 10).

This is the **fourth** bug of this exact shape — a value assembled correctly and dropped at the
kavach boundary — after `env` (3.0.18) and alongside `mounts` and `namespaces`.

⚠ **Wiring it on this side alone would not have worked, and checking that first is the point.**
`config_workdir` has existed in kavach for many releases and was **decorative**: its only reader
was the WASM backend's `--dir` preopen, so on the process and OCI backends the value was stored
and never consulted — exactly what `config_externalization` still does. Calling it would have
produced a green diff and no behaviour change. **Fixed upstream in kavach 3.12.2**
(`_spawn_enter_workdir`, applied after the chroot so the path resolves inside the container;
`process.cwd` emitted in the OCI spec instead of a hardcoded `"/"`), pinned here.

kavach fails closed: a workdir that cannot be entered `_exit`s the child with
`SPAWN_EXIT_WORKDIR` rather than running the payload from the wrong directory, which is what
runc does too.

`test_build_sandbox_threads_workdir` asserts the value crosses the boundary and is
mutation-proven; kavach's `test_confine_capture_workdir` asserts the payload's `$PWD` is
actually the requested directory.

### Fixed — the MCP `stiva_run` error named a prerequisite that shipped in v3.0.14

`stiva_run` is the one tool of nine with no handler — 8 of 9 dispatch live. It returned:

> `stiva_run unavailable: MCP run is detached, needing kavach sandbox_spawn (v3.1)`

`sandbox_spawn` exists, and stiva's own detached path has used it since **v3.0.14**:
`container_manager_start`'s `detach == 1` branch calls `spawn_container`, records the pid and
returns without blocking — which is exactly what an MCP handler needs. The message sent readers
looking for an upstream dependency that had already landed.

Now states the real gap: there is no `mcp_handle_run` yet. This is unstarted stiva-side work —
write it over the `stiva_run` facade the way `mcp_handle_exec` wraps `stiva_exec`.

## [3.0.18] — 2026-08-21 — three shipped defects: no env, no shell, and a latent aarch64 process-killer

Every one was found by adversarially re-testing claims the tree already made about itself,
and **none was on any roadmap.** Two were live on x86_64; the third would have killed the
first aarch64 build silently. 2175 → **2183** assertions across `tests/*.tcyr`.

### Fixed — `stiva run <image>` was refused for most images: `blocked command: sh`

Reproduced end to end, not inferred:

```
$ stiva run <image>          # no CMD, so the command defaults to /bin/sh
[INFO] executing one-shot container via kavach: /bin/sh
[INFO] container execution complete, exit_code=126
blocked command: sh
```

kavach's runtime guard enforces an interpreter blocklist — `sh`, `bash`, `zsh`, `dash`,
`python`, `python3`, `perl`, `ruby`, `php`, `lua`, `node`, `deno`, `bun`, `gcc` — on **every**
backend, unconditionally. `generate_spec` defaults an empty command to `/bin/sh`
(`src/runtime.cyr:481`, `:485`), so a container with no `CMD` was refused before fork. So was
any image whose entrypoint is a shell or an interpreter, which is most of them.

**Fixed upstream in kavach 3.12.1**, pinned here. The blocklist exists to stop an agent
sandbox shelling out to an interpreter *on the host*; once the sandbox has a rootfs the name
resolves **inside the container**, to a binary the caller supplied, after chroot and with
namespaces up. `runtime_guard_config_for_rootfs` clears only the blocklist, only when a
rootfs is set — sensitive-path and shell-metacharacter detection still apply inside a
container, and a rootfs-less agent sandbox is completely unchanged.

### Fixed — `_stor_lchown` was `syscall(94, …)`, which is `exit_group` on aarch64

`src/storage.cyr` issued a bare `syscall(94, path, uid, gid)` for every tar entry of every
layer unpack, `import` and `load`. 94 is `lchown` on x86_64 and **`__NR_exit_group` on
aarch64** — so the first aarch64 build would have terminated silently on the first entry of
the first image it touched. Now `sys_fchownat(AT_FDCWD, path, uid, gid, AT_SYMLINK_NOFOLLOW)`,
correct on both arches.

⚠ **It sat behind a comment asserting it could not be fixed** — *"the cyrius stdlib exposes
neither a `sys_lchown` wrapper nor a `SYS_LCHOWN`/`SYS_FCHOWNAT` per-arch constant"*. That was
false, and false at the pin in force when it was written. All four symbols are in stiva's own
`lib/`: `sys_fchownat` (`syscalls_x86_64_linux.cyr:393`, `syscalls_aarch64_linux.cyr:611`),
`SYS_FCHOWNAT` (`:116`, `:109`), `AT_FDCWD` and `AT_SYMLINK_NOFOLLOW`
(`syscalls_linux_common.cyr:38`/`:41`, `:61`/`:68`). The stdlib's own header documents this
exact use: *"with AT_FDCWD + AT_SYMLINK_NOFOLLOW it gives lchown semantics."*

This is the same class agnos's roadmap already tracks as *"Raw `syscall(N,…)` with LINUX
numbers on agnos paths"*, where `read`(0) becomes `exit`. Latent here only because x86_64 is
the sole built target.

### Fixed — containers never received the environment their image declared

An OCI image's `process.env` was parsed into `ContainerConfig.env`
(`src/container.cyr:945-964`), persisted to `state.json`, folded to `"KEY=VALUE"` and
threaded into `RuntimeSpec.env` (`src/runtime.cyr:531`) — and then **read by nothing**.
`build_sandbox` never passed it on, so every container ran with no environment beyond
whatever the backend hardcoded. An image declaring `ENV PATH=/opt/bin` or `ENV LANG=C.UTF-8`
had it silently discarded.

**A parity regression against the frozen oracle**, which builds env into the spec
(`rust-old/src/runtime.rs:176`) and applies it at exec (`:515` — `cmd.env(k, v)`). The port
reproduced the assembling half faithfully and dropped the delivering half at the kavach
boundary, where the oracle had used `std::process::Command` directly.

It could not be fixed on this side: every kavach exec path built a hardcoded empty `envp`
and `SandboxConfig` had no field to carry one. **Fixed upstream in kavach 3.12.0**
(`config_env`, honoured by the process backend, `sandbox_spawn`, and the OCI spec's
`process.env`), pinned here; `build_sandbox` now calls it.

⚠ **Why 2175 tests did not catch this, which is the more useful half of the finding.**
`test_generate_spec_env_kv` asserted that the *spec carried* the env — mirroring the oracle's
own unit test, which asserted the same thing — and it passed throughout. Neither suite ever
asserted that a container could *read* a variable. The delivery step was the one line with no
test on either side of the port.

Two tests now cover the boundary, and both were mutation-proven by reverting the fix and
confirming they go red:
- `test_build_sandbox_threads_env` — the env reaches kavach's config, not just the spec.
- `test_build_sandbox_no_env_stays_unset` — an undeclared env stays unset, so the default
  path is byte-identical to the old behaviour.

Delivery to a live payload is asserted upstream by kavach's `test_confine_capture_env`.

⚠ `RuntimeSpec.mounts` (`src/runtime.cyr:539`) and `RuntimeSpec.namespaces` (`:532`) are
**still write-only**, the same shape. `namespaces` is roadmap v3.1.0 item 3; `mounts` is not
yet on the roadmap and should be, since `standard_mounts()` builds a `/dev` entry that never
reaches a mount.

⚠ **KNOWN GAP — AGNOS-native payloads still see nothing, and it is `mounts` that blocks it.**
Cyrius's `getenv` on Linux reads `/proc/self/environ` (`lib/io.cyr:779-780`), not the stack
`envp`. On `run -d` nothing mounts `/proc` — `confine_child` chroots and mounts nothing, and
stiva's own `/proc` entry sits in the write-only `mounts` field — so `getenv` returns 0 with
no error and no log line. glibc and busybox payloads are fine, because they read the real
`envp` this release now populates. **Anything built with cyrius — daimon, sutra — is not**,
which is to say stiva's own intended workloads. Tracked in the agnos roadmap ("AGNOS-native
binaries cannot read their environment inside a container"); the fix is either cyrius
preferring the stack `envp` on Linux, or stiva wiring `mounts`. Do not assume this release
closed it for AGNOS consumers.

2175 → **2183** assertions across `tests/*.tcyr`.

### Changed — nein `1.6.6` → `1.6.10`

nein's 1.6.7–1.6.10 line closed a Rust→Cyrius port-completeness audit, deleted its
`rust-old/` tree, and ran a P(-1) hardening review that produced 1 CRITICAL / 6 HIGH /
10 MEDIUM confirmed findings. 2175 assertions and 87 CLI smoke assertions unchanged; all 13
nein functions stiva calls still exist with the same signatures.

⚠ **stiva was not exposed to any of the three HIGH findings, and the reason is worth
recording** — it is a design decision that turned out to be load-bearing rather than luck:

- **SIGPIPE killing the host process on a dead `nft`** (reproduced upstream as exit 141)
  lives in nein's `_run_nft_stdin`. stiva never pipes to nft's stdin: it writes a temp file
  and runs `nft -f <file>` (`src/network_manager.cyr:358-361`), a divergence from the Rust
  oracle taken for unrelated reasons. That path has no pipe to break.
- **`diff_compute` ignoring rule order** (a re-added deny rule landing after the accept it
  was meant to precede, leaving the host permitted) and **`_strip_handle_suffix` mis-parsing
  a rule whose comment contains ` # handle `** are both in the diff/parse surface. stiva
  calls **no** `apply_*` or `diff_*` function — verified by cross-referencing all 456 fns
  nein defines against every call site in `src/`: stiva uses 13, all render/validate/build.

The one fix that does reach stiva is the MEDIUM: `#` is now rejected by
`validate_nft_element` (`lib/nein.cyr:340`), which `firewall_validate` runs — and stiva
calls that at `src/network_manager.cyr:244`. nft treats `#` as a comment to end-of-line, so
`… ip saddr 1.2.3.4 accept # drop` applies as an **accept** with the intended verdict
silently gone. stiva feeds nein only subnets, interface names, ports and IPs, so nothing it
constructs today contains a `#`; the change is strictly a tightening.

## [3.0.17] — 2026-08-21 — single-node runtime finished (§H cron, §I accel, `diff`, rootless net) · toolchain 6.5.33

Closes the v3.0.17 roadmap line: `stiva diff`, rootless container networking,
persisted `scan_policy`, three inherited `fleet.cyr` defects, plus §H scheduled
containers and §I accelerator-aware placement — the last two unlocked by turning
the `accel` feature on by default. 2075 → **2175** tests across `tests/*.tcyr`,
**87** CLI smoke assertions, 36 registered verbs (34 live).

The release also moves the toolchain and every dependency pin, which is where it
got interesting — see the two entries directly below.

### Benchmarks — flat across the toolchain move, on a contended box

14 benchmarks, no real movement against the pre-bump run (`03a55fe`): the two OCI
serialize paths are within 1 %, everything else lands between −3.3 % and +5.7 %.

⚠ **The recorded 3.0.17 row was measured at load 0.80–1.07 and reads slightly hot
throughout.** Almost every benchmark moved *up* together by 1.6–4.7 %, which is the
box-wide-contention signature rather than a code change — the same tell cyrius
documents for its own `.21`/`.23`/`.25` incidents.

`flatten_layers_2x60_files_4kib` is the one entry that looks like a regression at
**+5.7 %** (1.458 ms → 1.541 ms). It is not: three fresh runs measured 1.420 / 1.425 /
1.477 ms, a median *below* the 3.0.16 figure. The recorded value is the high end of
run-to-run variance on a busy machine. Re-baseline on a settled box (load < 0.35) before
reading anything into these numbers.

### Changed — toolchain `6.4.78` → `6.5.33` and a full dependency refresh

Every AGNOS pin to its current tag: **kavach 3.9.3 → 3.11.15**, **majra 2.5.1 →
2.6.7**, **bote 3.1.4 → 3.3.2**, **libro 2.8.2 → 2.8.8**, **sakshi 2.4.6 →
2.4.11**, **agnodrm 1.5.0 → 1.5.1**, **ai-hwaccel 2.3.15 → 2.3.18**, **nein 1.6.4
→ 1.6.6**. `cmdit` and `samay` stay at 1.2.2 / 1.0.1 (both already current).

**`[deps.sigil]` is now declared explicitly, pinned to 3.12.9 and listed FIRST.**
stiva takes the full `dist/sigil.cyr`; libro declares its own `[deps.sigil]`
selecting four *thin* sub-bundles, and the full monolith already inlines all
four. Claiming the name ahead of the transitive walk is what stops both surfaces
landing in one compilation unit — which is not a warning-level problem: the two
copies push cycc's 16-slot `#define`/flag table to 17 and the build stops. Same
remedy nein 1.6.3 documents for the identical collision.

Three source-level adjustments the toolchain move forced:

- **`bayan_json_v_parse_str` → `_buf`** (15 call sites) and
  **`bayan_yaml_parse_str` → `_buf`** (1). bayan 1.3.0, which ships with cyrius
  6.5.0, renamed the `(buf, len)` entry points; the bodies are unchanged.
- **`Backend.*` → `KavachBackend.*`** (9 sites) — kavach 3.11.x namespaced the
  enum.
- **A samay 1.0.1 compatibility shim in `src/error.cyr`.** samay's `_parse` still
  calls the unprefixed `json_v_parse_str`, which bayan 1.3.0 removed, so the
  symbol is undefined at link time and cycc refuses to emit. The shim forwards to
  `bayan_json_v_parse_buf` (identical signature and body). It lives in
  `error.cyr` rather than beside the samay consumer in `cron.cyr` because cyrius
  auto-prepends every active `[deps.*]` module into *every* compilation unit — so
  the unresolved reference exists even in `convert.tcyr`, which includes only
  `error.cyr` + `convert.cyr`. `error.cyr` is the one module every unit includes.
  **Retire it the moment samay ≥ 1.0.2 lands** with the call updated; past that
  point it shadows rather than shims. ai-hwaccel hit the same break and fixed its
  own copy at 2.3.16; samay has had no release since.

`src/` and `tests/` were also reformatted under the 6.5.x formatter — continuation
lines now indent by two columns instead of aligning to the statement. Whitespace
only, no token changes.

### Fixed — CI could not resolve dependencies at all: `dep nein requires 'bote-core'`

The toolchain bump broke `cyrius deps` outright, before build or test:

```
error: cannot read <snapshot>/lib/bote-core.cyr
error: dep nein requires 'bote-core' but it is not in the cyrius stdlib
```

**The cause was upstream in nein, and the bump only revealed it.** `cyrius
distlib` omits a fold from a `.deps` sidecar only when the fold basename equals
the dep's `[deps.NAME]` section name. nein named that section for the repo
(`bote`) while the module is `bote-core`, so its `include "lib/bote-core.cyr"`
was emitted into **both** `dist/nein.deps` and `dist/nein-mcp.deps` as a *stdlib
leaf* — asserting a git-dep bundle ships in the toolchain snapshot.

It shipped in nein 1.6.4 and 1.6.5 and was invisible in both: through cyrius
6.5.23, `_dep_find_stdlib_dir()` returned the consumer's own half-populated
`./lib` as the stdlib for any project carrying a `src/main.cyr`, and stiva's own
`[deps.bote]` drops `lib/bote-core.cyr` there, so the bad leaf resolved by
accident. cyrius 6.5.24/6.5.25 corrected that lookup to consult the pinned
snapshot — correct, and it turned a latent packaging lie into a hard error.

Fixed in **nein 1.6.6** (`[deps.bote-core]`, matching the module basename); stiva
pins it here. Worth keeping: **reverting the toolchain alone does not fix this** —
6.4.78 with the refreshed deps fails differently, on four *false* errors
(`test`, `patra`, `regex`, `hashmap_fast` — all real stdlib modules) that are
precisely what 6.5.24 fixed. Forward was the only way out.

`cyrius.lock` is regenerated and committed: 84 files, 13 commit-pinned (12 → 13
because nein's renamed section resolves `bote-core` transitively alongside
stiva's own `bote`; both land on the same commit).

### Fixed — `src/build.cyr` and `tests/registry.tcyr` had been failing `cyrius fmt --check`
Silently, and for several releases, because `cyrius audit`'s fmt stage reports only
`FAIL: files need reformatting` without naming a file, and `cyrius fmt <file> --check` exits 1
with **no output at all**. Found by bisecting a prefix of the file against `--check`.

The rule neither tool states: **a continuation line must sit at the *same* indent as the
statement it continues, not one level deeper.** Ten continuation lines were indented the
conventional way and were wrong. Whitespace-only change; suite unchanged at 2175.

`src/storage.cyr` also carried one lint warning — a box-drawing comment rule that is 53 display
columns but **141 bytes**, and the linter counts bytes. `src/` is now fmt- and lint-clean at zero
warnings; the 163 remaining line-length warnings in `tests/` are recorded in the roadmap.

### Changed — documentation sweep: the docs described a runtime that stopped existing several releases ago
Counts were the least of it. `docs/guides/quick-start.md` told operators that `stiva pull` was
unwired, that the `Stiva` facade did not exist, and that `stiva top` and `exec` were unimplemented —
all live for releases. Worse, three guides documented **flags that do not exist**:
`stiva run -e K=V`, `-p 8080:80` and `-s DB_PASSWORD=…` are each a usage error, and the security
guide presented that last one as the working way to hand a container a credential. The truth is the
opposite and is now stated as a warning at the top of that section: **containers currently receive
no secrets at all**.

Also corrected: the Rust snippets left over from the oracle in the networking and security guides
(replaced with the Cyrius API that exists); `docs/spec-compliance.md` listing CRIU as "conformant"
when `checkpoint`/`restore` are not reachable from the CLI; and a `[Unreleased]` entry above
claiming "all 35 verbs are live" when two never were.

Counts, now derived from execution rather than memory: **36 verbs, 34 live** (`checkpoint` and
`restore` are the two unwired, both v3.1.0 item 3), **27** domain modules, **2175** tests, **87**
CLI smoke assertions, **12** ADRs, toolchain **6.4.78**.

### Added — four architecture notes, the first entries in `docs/architecture/`
That directory has been an empty placeholder since it was created. The four invariants written
into it are all ones this project has paid for at least once, and none are derivable from reading
the code:

1. **The cycc struct-id ↔ SIMD-sentinel miscompile**, why every raw-offset accessor exists, and the
   worked example of a probe that certified the bug fixed hours before it silently corrupted a
   registry cache key.
2. **CLI verb ids are registration-ordered** — inserting a verb anywhere but last renumbers every
   verb after it, silently.
3. **A `path` dep override silently wins over its `tag`**, and `cyrius.lock` records nothing that
   would reveal the substitution.
4. **A container rootfs has two possible layouts** (overlay vs flattened), so anything reasoning
   about "what changed" needs both paths or it reports a flattened container as clean.

### Added — the `accel` feature is on by default, unlocking §H and §I
`default = ["accel"]`. Both bundles were already declared and pinned; the gate was the only thing
holding them back. Cyrius `[features]` gate dependency activation, not source, so §H/§I code in
`src/` requires it on.

### Added — `stiva cron`: scheduled containers (roadmap §H)
`cron add|ls|rm|enable|disable|check`, over a new `src/cron.cyr` and `{root}/cron.json`.

The timing semantics come from samay, which already had the hard parts: a 5-field parser with
`@shortcuts`, ranges, steps and the Vixie DOM-vs-DOW rule; a scheduler with a missed-schedule
policy, catch-up counting and a firing cap; and an injected-clock `check_due_at`, so the whole
thing is deterministically testable with no wall clock.

Three things samay does not provide, which is why the module exists: `CronTaskTemplate` cannot
carry an image, command, env or mounts (nor can its JSON codec), so stiva owns the entry-name →
container-spec table; the typed cron *expression string* is not persisted either (only the seven
bitmasks), so stiva stores its own copy or `cron ls` could not show what the user typed; and there
is no poll site, since `main.cyr` is a one-shot dispatcher — `cron check` fires what is due and
exits, driven by an external timer.

**`cron_expr_parse` returns a `Result`, not a pointer.** Testing it with `!= 0` accepts every
string, because `Err(1)` is non-zero — which is exactly how `--schedule "not a cron"` was accepted
as a valid schedule during development. It now goes through `is_ok`.

`CRON_SKIP` is the policy, not `CRON_CATCHUP`: a container that missed six hours of hourly runs
because the machine was off should start **once**, not sixty times. samay logs the drop either way.
`check` persists the advanced anchors *before* running anything — otherwise a crash mid-start
re-fires the same job on every subsequent tick forever.

### Added — accelerator inventory and placement (roadmap §I)
`stiva info` now reports the node's accelerators via `registry_detect_no_exec`, which is
subprocess-free (pure sysfs and syscall reads) so it cannot hang on a missing `nvidia-smi` — the
exec-based detectors are force-stripped by the builder mask, not merely unused.

`FleetNodeCapacity` gains `accel_profiles` and `DeploymentConstraints` gains `accel_req` /
`accel_min_chips`, gating placement through ai-hwaccel's `find_satisfying_profile`. **Grafted onto
stiva's own structs rather than adopting samay's `NodeCapacity`**, which is a superset only on the
continuous dimensions — it has no `max_containers` (stiva's entire fit test), no node status and no
label constraints, so adopting it would regress three shipped features.

**A node with no known profiles fails any non-`REQ_NONE` request.** Treating "not inventoried" as
"satisfies" would place a GPU workload where it cannot run. `REQ_NONE` is the default, so every
existing caller is unaffected.

### Added — CI guard for the tag/path substitution (roadmap v3.0.17 item 4)
Second attempt, and this one is validated. A local `path = "../<dep>"` override silently wins over
the `tag` pin, and `cyrius.lock` records no dep name or version to notice with.

The first attempt used `git diff --exit-code cyrius.lock`, which **can never pass**: the lock's
FORMAT depends on the resolution mode — resolving from git tags writes `commit <sha> <name> <url>
<tag>` header lines and a different hash ordering, resolving through path overrides writes only file
hashes. It broke CI and was reverted.

This compares the sorted **set** of `<sha256>  lib/<file>` lines, ignoring the `commit` lines and
their order — the part that means the same thing in both modes, and the part that answers the real
question. Verified against synthetic locks: format and ordering differences are ignored, a changed
file hash is caught.

> **The cycc struct-id miscompile bit twice more.** Adding a field to `FleetNodeCapacity` and
> `DeploymentConstraints` made typed access to the NEW fields segfault inside
> `node_matches_constraints` — while `cap.memory_mb`, a few lines below in the same function, is
> fine. Both branches now use raw offsets. Found by bisection each time.

### Tests — 2132 → 2175
### Added — `stiva diff` (roadmap v3.0.17 item 1)
`A` added, `C` changed, `D` deleted, `(no changes)` when clean — and it handles both rootfs layouts
off the `{croot}/.rootfs-flattened` marker, since they cannot be told apart after the fact
(`internals` is process-local, and `setup_overlay` creates `upper`/`work`/`merged` even when the
mount fails).

**There is no oracle for this.** `ContainerManager::new` leaves `internals` empty
(`rust-old/src/container.rs:260`) and nothing rehydrates it, so `get_rootfs` always returns
`ContainerNotFound` and the Rust `diff` verb has never executed in the CLI. Its walk is wrong twice
over anyway: it emits only `C`/`D`, never `A`, and it looks for `.wh.` files in the overlay upper —
but overlayfs whiteouts are **character devices 0:0**, so that branch could never fire. This checks
`S_IFCHR`. A rewrite, not a port.

The change fingerprint is type + size + mode, deliberately **excluding mtime**: unpacking a layer
sets mtime from the tar, so a container that merely *read* a file would otherwise show as changed.

### Added — rootless container networking (roadmap v3.0.17 item 2)
Before this, `network_manager` warned that the bridge needed root and carried on — so an
unprivileged `stiva run`, **stiva's default mode**, produced a container with no connectivity and
only a log line. The detection half was already ported; only the spawn was missing.

**Not over `_exec_capture2`.** slirp4netns and pasta are *daemons* — they run for the container's
lifetime attached to its network namespace — so a capture primitive, which waits for the child,
would hang `run` forever. `_rl_spawn_detached` forks, execs and returns the pid without reaping;
the helper is reparented to init exactly like a detached container, and its pid is recorded in
`{croot}/rootless-net.pid` so a *later* stiva process can stop it (`internals` is in-memory, so
without the file nothing could reap it cross-process).

Wired at the **detached start** branch, the only point with a live pid. Two divergences: absolute
binary paths are resolved before exec (execve does not search `PATH`, and the child gets an empty
environment, so a bare name always fails), and when port mappings are present pasta is preferred —
it takes mappings as argv, whereas slirp4netns needs a JSON-RPC round trip against a socket that
does not exist yet. The oracle has no such preference and its slirp path sleeps 200 ms and hopes;
this retries the connect on a 500 ms ceiling. A missing helper is a warning, never a start failure.

### Fixed — `scan_policy` now persists (roadmap v3.0.17 item 3)
It serialized as `null` and read back as 0, so a per-container externalization policy never survived
a save/load — which is why `logs --scan` had to be an explicit flag. `logs` now consults the record
and treats the flag as an override. A partial object keeps kavach's defaults rather than zeroing
them: a 0 `block_threshold` would silently BLOCK every artifact, the opposite of a missing field's
intent. Absent/null still means "no policy", so every existing `state.json` upgrades silently.

### Fixed — three inherited `fleet.cyr` defects (roadmap v3.0.17 item 4)
All three are present in `rust-old/src/fleet.rs`, so correcting them is a deliberate divergence.

- **`plan_rollback` planned every container onto one node.** It called `select_migration_target`
  once per running container with no reservation between calls, so all N containers on a failed node
  targeted whichever node had the most free slots — a plan that cannot be executed.
  `_fleet_select_and_reserve` now decrements the chosen node's capacity as it goes.
- **`Draining` and `Cordoned` did nothing.** Every filter tested `== Ready`, so the two states were
  defined and then behaved like neither — cordoning a node before maintenance was a silent no-op.
  They now mean "no new work", and deliberately remain distinct from `NotReady`: a draining node is
  still healthy, so health checks must not sweep it and a rollback must not migrate off it.
- **`preferred_nodes` was never read.** Now honoured as a **preference**, not a filter — a preferred
  node that is full or fails the hard constraints loses its priority rather than blocking the
  deployment. Treating it as a filter would turn a hint into an outage.

> **The cycc struct-id 20/21 miscompile bit brand-new code here.** `_fleet_select_and_reserve`
> segfaulted on an ordinary `var n: FleetNode = node;` while the *identical* access in
> `select_migration_target`, two functions above it, is fine — the bug is per-function-context.
> Found by bisection (the two tests reaching the function crashed; the two that did not passed) and
> fixed with the documented raw-offset workaround. Worth recording that stdout buffering made the
> first diagnosis wrong: the test-group header flushed but its assertions did not, so print position
> lied about where execution died.

### Tests — 2075 → 2132
### Changed — the roadmap is a plan again, and every deferral in the source is tracked
The roadmap had accreted to 1051 lines with completed work interleaved between open items, so the
next step had to be *derived* by grepping for `- [ ]` rather than read. Rewritten to 400 lines:
completed work **deleted** (the CHANGELOG and git carry it), everything remaining assigned to a
release, and every prerequisite in another repo written down as its own work item with a named
symbol instead of as a reason to wait.

**Releases are now explicit** — v3.0.17 (finish the single-node runtime), v3.1.0 (secrets,
interactivity, mobility), v3.2.0 (aarch64 then AGNOS), v3.3.0 (orchestration surface), v3.4.0
(Windows) — and the catch-all "Deferred" section is gone; its contents were assigned to v3.3.0 and
v3.4.0.

**`cyrius lint` reported 36 untracked deferrals across 12 source files. They are now zero**, and
the sweep found two real gaps that had never appeared in any roadmap:

- **Secrets are not wired at all.** `container_config_to_jv` writes `secrets` as an empty array and
  `scan_policy` as null; `_from_jv` zeroes both; `build_sandbox` carries "no SandboxConfig setter in
  dist". `CLAUDE.md` lists kavach's CredentialProxy/SecretRef and ExternalizationGate among the
  features stiva must keep wired — neither is. Now v3.1.0 item 1, with the kavach-side setter as
  step 1.
- **Unprivileged containers have no network.** `network_manager` logs "bridge creation deferred
  (requires root)" and carries on, so stiva's *default* mode produces a container with no
  connectivity and only a warning. `network_rootless.cyr` already ports the detection half; only the
  slirp4netns/pasta spawn was missing, for want of a subprocess primitive — and `_exec_capture2`
  (shipped in 3.0.15) is that primitive. Now v3.0.17 item 2.

Also corrected in the source rather than re-tracked: `imagelayout.cyr` claimed a docker-archive was
"reported as unsupported (a v3.0.2 follow-up)" — the read path landed in **v3.0.3** and the function
handles it. `container.cyr` described the `ContainerManager` as deferred (landed §C), `health.cyr`
called `exec_in_container` deferred (landed §D), `mcp.cyr` said its handlers were still to come
(landed §E), `main.cyr` said detached `run -d` was deferred to v3.1 (landed v3.0.14), and
`runtime.cyr`'s DEFERRED block still listed `spawn_container`, `export_rootfs` and `import_rootfs`,
all of which shipped. Those comments were false, not pending.

### Fixed — OCI whiteouts were extracted literally, so deleted files stayed visible
A layer deletes a path from the layers below it by shipping a marker rather than the file:
`.wh.<name>` removes `<name>`, and `.wh..wh..opq` hides everything a directory inherited. Both were
written out as ordinary files, so a file deleted in an upper layer **stayed in the container** and
the marker appeared next to it. Every image with a `rm` in its history was affected.

Inherited: `rust-old/src/storage.rs` has no whiteout handling either, and overlayfs understands
only its own char-dev-0:0 convention — never the tar one — so the privileged path was equally wrong.

**The roadmap's proposed fix does not work, and the entry is corrected.** It called for translating
markers at unpack "where it lands for both paths at once". It cannot: the file a marker deletes
lives in a *different* layer, and unpack sees one layer's tar in isolation. The only unpack-time
representation that works is an overlayfs char-dev-0:0 whiteout, and creating one needs `CAP_MKNOD`
— which the unprivileged path, the one that always runs, does not have. Markers therefore stay in
the layer directory as that layer's metadata, and `flatten_layers` interprets them at merge time.

Applied as a **pre-pass per directory**, not inline: `getdents64` order is filesystem-dependent, so
a single streaming pass would apply `.wh.foo` before or after copying a sibling depending on the
directory's layout, and an opaque marker must take effect before any of the layer's entries land.
That also matches containerd.

Hardening that came with it:
- **A whiteout target name is validated** — it comes from an untrusted image, and `.wh.` + `../../etc`
  would otherwise hand a path outside the rootfs to the deleter. Names containing `/`, or equal to
  `.` or `..`, are refused.
- **`_stor_remove_dir_all` now opens `O_NOFOLLOW`.** Without it, deleting a path that a lower layer
  left as a symlink-to-directory recursively deletes the *target*. Every existing caller was safe by
  construction (`_stor_replace_existing` `lstat`s first), but that was a property of the callers,
  and whiteout code deletes paths an image names.

**Tests verified against the pre-fix code**, not just the fix: disabling the two call sites turns
six assertions red. Coverage includes file and directory whiteouts, opaque directories, marker
non-leak, the escape attempt, and that a whiteout hides only layers *below* it (a later layer
re-adding the name still wins). **2051 → 2075.**

## [3.0.16] — 2026-07-26 — security: four sandbox escapes closed (3.0.15 audit)

### Security — four ways out of the sandbox, all found by adversarial review of 3.0.15
A 43-agent audit of everything 3.0.15 shipped produced 37 findings; 29 survived adversarial
verification (each verifier's job was to *refute*, defaulting to "not a bug" unless they reproduced
it themselves). The four that matter most were all reproduced end to end against the shipped
binary.

**1. `exec -w` escaped the container.** The workdir is spliced onto `/proc/<pid>/root` — which it
has to be, because nsenter resolves `--wd` before `-r` — so a `..` component walked straight back
out: the process stayed chrooted into the container with its **cwd on the host**, and every
relative path the command opened read the host filesystem. Reachable by an operator typing
`stiva exec -w ../../..` and, worse, from `mcp_handle_exec`, where `workdir` is untrusted JSON from
a client that is not the host operator. `_rt_workdir_is_safe` now requires an absolute path with no
`..` component, refusing before the argv is built.

**2. A COPY source escaped the build context through a symlink.** `_bld_source_is_safe` rejected a
path that *spells* an escape (`..`, absolute) and did nothing about one that *walks* through a
symlink. `ln -s / ctx/vendor` + `source = "vendor/etc/hostname"`, or `ln -s /home/u/.ssh ctx/evil` +
`source = "evil/"` — the trailing slash makes `lstat` resolve the link, so `_stor_tar_collect` walks
the whole out-of-context tree. Both were verified reading host files into an image the user then
pushes. `_bld_path_has_symlink` now `lstat`s every prefix and refuses; resolving-then-rechecking was
rejected as TOCTOU-racy.

**3. Validation ran after the damage.** `build_copy_cache_key`'s fingerprint **walks** the source
tree, and it was computed before `build_copy_layer`'s checks — so on a cache miss an escaping path
was traversed, and its metadata folded into a key, before anything refused it. All checks moved into
`_bld_copy_step_is_safe`, called by the driver first.

**4. `..` in a COPY destination was accepted**, producing layer members named `../../etc/...`.
stiva's own extractor discards those, so the layer was merely useless *here* — but it is
**pushable**, and another runtime's extractor may be less careful.

### Fixed — `stiva exec` could not pass a flag to the command, and stole `--root`
`stiva exec c1 ls -l` failed with "unknown flag" and never ran; `stiva exec c1 -- ls -l` failed
**identically**, because cmdit's dispatcher consumed the `--` terminator without forwarding it.
And `stiva exec c1 mycmd --root /x` silently retargeted **stiva's own data root** instead of passing
the argument to `mycmd`. Fixed in cmdit 1.2.2 (`--` forwarding + `cmdit_verb_trailing_after`) and
wired here, so everything after the container id is the payload's, verbatim.

### Fixed — the rest of the audit
- **A malformed Stivafile SIGSEGV'd.** A step missing a required TOML field left a 0 in a
  `BuildStep`, which reached `str_from(0)` on the ordinary build path. Missing fields are now a
  parse error, which is what serde gives the oracle for free.
- **`_stor_tar_bytes` wrote through a failed allocation** — a context large enough to exhaust the
  arena was a NULL-deref rather than an error. Now a 16 GiB backstop plus a null check.
- **A large-output exec was reported as a signal death.** The parent closed the pipe at the cap, so
  the child took SIGPIPE and a successful command surfaced as exit 141, with the truncation silent.
  It now drains to completion and reports the real status plus `rt_last_exec_truncated()`.
- **`stiva prune` deleted the record but never the directory** — leaking one full flattened image
  copy per container, permanently: a second prune no longer saw it, `rm` said "container not
  found", and `gc` only sweeps blobs. It now removes the directory and emits the `removed` event
  the audit trail was missing.
- **`container_manager_create` leaked the same way** when `generate_spec` failed, after the layers
  were already flattened.
- **The exec stderr scratch file was `O_APPEND`** (the comment claimed `O_TRUNC`), so a leftover
  made one exec return a previous exec's stderr; and it was keyed on the container's pid, so two
  concurrent execs collided. Now per-exec, unlinked before the fork, `O_EXCL|O_NOFOLLOW`.
- **`dup2(fd, N); close(fd)`** with no `fd != N` guard destroyed the descriptor it had just
  installed when the caller started with fd 1 closed.
- **`syscall(157, …)` for `prctl` and `syscall(80, …)` for `chdir`** are x86_64-only — 157 is
  `setsid` on aarch64 and 80 is `fstat`, so NO_NEW_PRIVS was silently never set and the workdir hook
  was a no-op there. Now `sys_prctl` and `SYS_CHDIR`. The sibling `_stor_lchown` (`syscall(94, …)`,
  which is `exit_group` on aarch64 — an aarch64 `stiva load` terminates silently on the first tar
  entry) **cannot** be fixed here: the stdlib exposes no wrapper and no constant. Filed upstream;
  aarch64 is not a working target until it lands.
- **MCP `exec` silently dropped non-string array elements**, shifting every later argument, so
  `["sh","-c",7,"echo hi"]` ran a different command than the client asked for. Now rejected.
- Cache-fingerprint accuracy: the source root's **own** metadata is folded in (a `chmod` of it
  served a stale layer), and the depth-64 cap perturbs the accumulator instead of silently
  under-fingerprinting.

**One finding did not hold.** The audit reported `stiva events -f` allocating per event line; it
allocates once before the loop, and `_cli_events_scan` terminates lines in place with a comment
saying exactly that. Recorded because a verified-looking finding that is wrong is the thing this
process has to keep catching. The event-log rotation race IS real and is now documented at the
site, with the mitigation that was considered and rejected for not mitigating.

### Changed — kavach 3.9.2 → 3.9.3, cmdit 1.2.1 → 1.2.2
kavach: the OCI scratch directory `/tmp/kavach-runc-<uid>` was created with its result discarded and
then trusted, so on a shared host another user could own runc's state and both scratch files. Now
validated (real directory, owned by us, no group/other bits), with `O_EXCL|O_NOFOLLOW` opens and an
unlink-before-create. cmdit: `--` forwarding and trailing-verbatim, above.

### Tests — 2011 → 2051 unit
Regression tests for every security fix, each written to fail against the pre-fix code.

### Fixed — `network_dns_host_servers` could return an empty list, breaking its own contract
Its doc comment promised "always non-empty: defaults guarantee ≥1", but the DEFAULT_DNS fallback
fired only when the *read* failed. A `/etc/resolv.conf` that reads fine and yields no IPv4
nameserver — an IPv6-only host, a systemd stub with only `search`/`options`, an empty file — parsed
to an empty vec.

**This was a latent CI flake, not a theoretical one.** The port mirrors the oracle's
`host_dns_servers_returns_something` test, which asserts non-empty; on any host without an IPv4
resolver that test fails. It passes everywhere an IPv4 nameserver happens to be configured, which
is nearly everywhere — so it would have surfaced as a mystery failure on one runner.

Inherited: `rust-old/src/network/dns.rs:13-18` has the identical hole *and* the identical
over-claiming test, so correcting it is a deliberate divergence. `inject_resolv_conf` already
re-defaulted on empty, so no container ever got a nameserver-less `resolv.conf`; the gap was in the
contract and in callers (`network_manager`) that pass the vec elsewhere first. New tests drive the
parser with each empty-yielding input rather than relying on the host's configuration.

### Reverted — the CI lockfile guard, which could never pass
Added earlier in this cycle and **removed before it shipped**, because it broke CI on the first run.
Recorded rather than quietly dropped, since what it exposed is worth knowing.

The guard ran `git diff --exit-code cyrius.lock` after `cyrius deps`, on the theory that CI (which
has no sibling checkouts) resolves from the pinned tags, so a lock generated against a local
`path = "../<dep>"` override would no longer match.

**The theory was right; the mechanism was not. `cyrius.lock`'s FORMAT depends on how deps were
resolved.** Resolving from git writes `commit <sha> <name> <url> <tag>` header lines and orders the
file hashes differently; resolving through path overrides writes only the file hashes. So the guard
compared two structurally different files and failed unconditionally — not because anything had
drifted.

It was verified locally before being added, which is exactly why it slipped through: locally both
sides of the comparison are path-resolved, so they agree. The one configuration that matters —
tag-resolved versus path-resolved — is the one that cannot be reproduced without a network.

**The underlying gap is still open**, and `docs/development/roadmap.md` still asks for a detector:
a local override silently wins over the `tag` pin (this is how kavach 3.8.1 once arrived
unannounced), and the lock cannot notice because it records no dep name or version — the *file
hashes* it records are the only comparable part. A guard that compares the sorted set of
`<sha256>  lib/<file>` lines while ignoring the `commit` lines and their order would test the real
invariant ("the files the tags produce are the files the lock records"). That is not being shipped
here on a second guess: it needs validating against a real CI run first.

## [3.0.15] — 2026-07-26 — `build` and `exec` are live (§F, §D)

### Added — `stiva exec` is live (roadmap §D). 32 of 35 verbs
Non-interactive exec into a running container, over `nsenter`, exactly as the oracle does it
(`rust-old/src/runtime.rs:491-561`) — this path deliberately does not touch kavach, because
entering an existing container's namespaces is `execve("nsenter", …)` in a forked child and kavach
has no API for it.

**Not over kavach's `persistent_spawn`, despite the roadmap suggesting it.** That path runs
`check_command` with the default runtime guard, whose blocklist rejects base names `sh`, `bash`,
`dash`, `python`, `node` — so `stiva exec <ctr> /bin/sh`, the canonical use, would be *refused*. It
also threads an empty envp, has no working-directory hook, and sends stderr to `/dev/null`.

**`_exec_capture2` — the first dual-stream capture in the tree.** Every exec path before this
returned `stderr = ""`: kavach's `confine_capture` `dup2`s one fd onto both 1 and 2, and the
stdlib's `exec_capture` sends fd 2 to `/dev/null`. stdout comes back over a pipe, stderr into a
temp file, and **the asymmetry is the design**: draining two pipes from one thread deadlocks the
moment either fills — a child writing 64 KiB to stderr before its first stdout byte blocks forever
against a parent reading stdout. The usual fix is `poll`/`epoll`, but stiva has no `sys_poll`
wrapper on any arch and `epoll_event` is packed differently on x86_64 (data at +4, stride 12) than
aarch64 (+8, stride 16) — while the AGNOS wrappers take a different *arity* again, so an epoll
drain would not compile for the v3.2 AGNOS target and would introduce this tree's first `#ifdef`.
A regular file never blocks its writer. `test_exec_capture2_large_stderr_does_not_deadlock` pushes
~82 KiB of stderr ahead of stdout; it hangs rather than fails if this regresses, which is the
honest signal.

**Three nsenter divergences, without which exec cannot work in the mode stiva actually runs in.**
The oracle's fixed `-p -m -n -u -i` assumes a privileged daemon; unprivileged, every one of those
flags fails with `reassociate to namespaces failed: Operation not permitted`. Determined by
experiment against a live rootless container, not by reading:
- **`-U --preserve-credentials` when the target owns a different user namespace.**
  `--preserve-credentials` is required, not cosmetic: without it nsenter calls `setgroups()` on
  entry, which an unprivileged user namespace denies. With it we keep our own credentials and gain
  no capabilities inside — which is precisely why the network, UTS and IPC namespaces are *not*
  entered: joining them needs `CAP_SYS_ADMIN` in the owning user namespace. Mount + user + root is
  the reachable set, and it is the set exec needs. The privileged path still uses the oracle's full
  flag set; `_rt_userns_differs` picks between them by comparing `/proc/<pid>/ns/user` symlink
  targets, so an unreadable link falls back to *parity*.
- **`-r`**, because kavach's process backend `chroot`s rather than `pivot_root`s: entering the
  mount namespace alone leaves our root at `/` and `/app/x` reports "No such file or directory"
  for a binary plainly in the image.
- **`--wd=/proc/<pid>/root<dir>`** rather than `--wd=<dir>`, because nsenter resolves `--wd`
  *before* `-r` takes effect — a plain `--wd=/app` dies with "cannot open /app" for a directory
  that exists only in the container. Omitting the flag entirely is also wrong: the cwd then stays
  the caller's, which does not exist under the new root, and `getcwd()` inside fails.

Hardening beyond the oracle: `NO_NEW_PRIVS` before `execve` (the oracle sets it at container launch
but not here, and an exec is the same trust boundary), and stdin from `/dev/null` so the child
cannot steal the user's terminal.

`exec -it` is **not** offered, and the blocker is nearer than the coroutine work: there is no pty
helper anywhere in `lib/` or `src/` (`openpty`/`posix_openpt`/`/dev/ptmx`/`TIOCSCTTY` all return
nothing), so `-t` has no substrate even if the runtime could suspend mid-body.

CLI: `-e/--env` is registered with **`cmdit_repeat`**, not `cmdit_str` — `-e A=1 -e B=2` through a
single-value flag silently drops `A=1`. A bare `NAME` with no `=` is a usage error rather than
being passed through, since `execve` rejects the whole envp for one malformed entry. stdout goes to
stdout and stderr to stderr (the point of capturing them apart), and **the command's exit code
becomes stiva's**, the way `docker exec` behaves.

Also live: `stiva_exec` on the facade — audited under `AUDIT_OP_EXEC` with the command in the
metadata, but deliberately **not** the env values: `-e AWS_SECRET_ACCESS_KEY=…` would otherwise
write the secret into an append-only log that outlives the container. The key *names* are recorded.
MCP `stiva_exec` is live too, replacing the stub that reported the tool unavailable.

`container_manager_exec` resolves the container before calling `require_pid`, which collapses
"no such container" and "wrong state" into one `-1` — branching on that alone left a stopped
container failing **silently**, caught by the smoke suite.

### Tests — 1983 → 2003 unit, smoke 78 → 87
New benchmark `exec_capture2_trivial_child` ≈ **29 ms** (fork + two redirections + drain + waitpid
against a shell stub — process-creation bound, tracked because every `stiva exec` pays it once).
The smoke fixture now builds **two** static payloads; it was capped at one because layers over
1 MiB were written as corrupt gzip, which the 3.0.14 encoder fix resolved — so the second payload
also regression-tests that fix on a real image.

### Added — `stiva build` is live (roadmap §F). 31 of 35 verbs
`build_image` in `src/build.cyr` turns a `Stivafile` into a real OCI image: base resolve → per-step
layer → OCI config → manifest → `index.json`. Verified on the binary, not just in tests — a static
binary present *only* in the layer the build produced executes inside a container started from the
built image.

**The crux, settled by reading the oracle rather than the roadmap: `build` does not need `exec`.**
`grep -n "Sandbox|Command|exec|kavach" rust-old/src/build.rs` returns 7 hits and **all 7 are prose**
(doc comments and tracing strings). The oracle never executes a RUN step, so §F was never gated on
§D.

Five deliberate divergences, each fixing a defect rather than porting it:

1. **`rootfs.diff_ids` are the UNCOMPRESSED tar digests.** The oracle (`build.rs:401`) maps
   `all_layers.iter().map(|l| l.digest)` — the *compressed* blob digests. The OCI image-spec defines
   diff_ids over the uncompressed layer, so every image the oracle builds fails validation in any
   other tool. Asserted against a second source: the test gunzips the stored blob and re-hashes it,
   rather than reading back the value the driver wrote (the cycc 20/21 lesson — an assertion that
   reads a value the way the code under test does will match the same garbage).
2. **A manifest blob is written.** The oracle stores only a config blob then calls `add_to_index`
   (`build.rs:429-452`). In this store that produces an image that silently **vanishes**:
   `image_store_load_index` reconstructs every `Image` from its manifest blob and skips any
   descriptor whose manifest is missing. The build would report success and `stiva images` would not
   list it. The test asserts via an index *reload*, not by checking a field we just set.
3. **`architecture` and `os` are emitted.** The oracle's `OciImageConfig` (`build.rs:197-203`) has
   exactly two fields, `config` and `rootfs`. Both platform keys are required by the image-spec.
4. **RUN steps are refused, not faked.** `build_run_layer` (`build.rs:475`) runs nothing — it writes
   a marker file `.stiva/run/<idx>.cmd` containing the argv, and its own doc comment admits it is a
   placeholder. Shipping that layer would produce an image asserting a command ran when none did,
   which the user cannot detect from the output. Refusing is the honest failure.
5. **`from_stage` is refused.** The oracle's (`build.rs:353-389`) copies from
   `<context>/<stage_name>` if that directory happens to exist and skips silently otherwise, and
   `spec.stages` is parsed then **never read** by `build_image`. There is no multi-stage build to
   port.

Plus one hardening divergence: **a COPY `source` may not be absolute or contain a `..` component.**
The oracle joins it straight onto the context dir, so a `Stivafile` from an untrusted repository can
copy `/etc/shadow` into an image the user then pushes to a public registry. The check is
component-wise, so a file legitimately named `..foo` or `a..b` still works.

Supporting changes:
- **`_stor_tar_bytes`** split out of `_stor_write_tar` (`src/storage.cyr`) so a builder can gzip the
  archive directly instead of round-tripping through a temp file.
- **`_il_config_diff_ids`** (`src/imagelayout.cyr`) — the tree's only *reader* of `diff_ids`. A
  derived image's config must carry `base.diff_ids ++ new.diff_ids`, and the base's are recoverable
  only from its config blob. A base whose diff_ids cannot be read is refused rather than producing a
  config that omits every inherited layer.
- **A layer-digest → diff_id sidecar** under `{root}/cache/diffid/`. A cached layer's uncompressed
  digest is not recoverable without gunzipping the blob; the oracle never faced this because its
  diff_ids are the compressed digests it already had. Every cache failure mode — truncated entry,
  blob swept by `gc`, missing sidecar — is a **miss, never an error**, so a stale cache costs a
  rebuild rather than requiring the user to know about `rm -rf {root}/cache`.
- **`AUDIT_OP_BUILD`** — net-new, appended so every existing code and wire name is unchanged. The
  oracle's `Stiva::build` (`lib.rs:294`) audits nothing; a trail that records the pull of a base and
  the push of the result but not the build between them has a gap exactly where provenance matters.
- **`mcp_handle_build` MOVED** from `mcp.cyr` to `stiva_core.cyr` as `mcp_handle_build(s, params)`.
  It could only ever parse the spec where it was — its result literally said `status: "parsed"` —
  because a real build needs the store and registry client, and include order puts `mcp.cyr`
  upstream of the facade. Same migration, same reason, as pull/push/ps/stop/inspect.
- **`stiva_image_store`** accessor, the sibling of `stiva_registry_client`.
- CLI `-f/--file` and `-c/--context` are finally **registered**; `docs/cli.md` has documented them
  since before the verb existed. Same three read guards as `convert` (directory, read error, 1 MiB
  ceiling) — a silently truncated `Stivafile` builds a plausible-but-wrong image.

**Base resolution is local-first**, then the registry: the oracle pulls unconditionally
(`build.rs:272`), so a build against a base already in the store needs the network and races the
registry's mutable tag.

### Fixed — two defects that made a rebuild silently ship the previous image
Both were found by running the binary, not by unit tests, and each alone was enough to make
`stiva build` useless in the edit-rebuild-run loop it exists for. Together they presented as an
edited file simply never reaching the image.

**1. The build cache did not cover the source content.** `build_cache_key` hashes
`base_digest : index : step_json`, and a copy step's JSON is
`{"type":"copy","source":"app","destination":"/app"}` — the source's *content* appears nowhere in
it, so editing the context did not invalidate the cache. Inherited (`rust-old/src/build.rs:615`
has the same shape). `build_copy_cache_key` now folds in `_bld_source_fingerprint`, a
**metadata-only** walk (path, mode, uid, gid, size, mtime to the nanosecond) — one `lstat` per
entry, never a file read, because reading contents would cost as much as rebuilding the layer the
cache exists to skip. The combiner is order-independent (sum + xor + count over a per-entry FNV-1a)
since `getdents64` order is filesystem-dependent. *Limit, stated because a quietly-wrong cache is
worse than none:* an edit preserving path, size, mode, ownership **and** nanosecond mtime will not
invalidate; deleting `{root}/cache` forces a full rebuild.

**2. The output reference carried a digest, so a rebuild appended instead of replacing.** The
oracle sets `digest: Some(config_digest)` (`build.rs:441`). Under this store's digest-aware index
dedup that reads as a **pinned** add, which replaces only an entry with the same manifest digest —
so a rebuild left *two* `local/<name>:<tag>` entries and `image_store_find` (first-match-wins) kept
resolving the **original**. The rebuild reported a new image id that nothing could reach. A tag is
a mutable pointer, so its digest slot now stays empty, matching `image_import`. Nothing is lost:
the image's own id *is* the config digest.

### Changed — kavach 3.9.1 → 3.9.2
Fixes the OCI backend never reporting the container's exit code: `stiva run <image> /nope` used to
print `exit_code=0` on any host with `runc` installed, because `oci_exec` passed only a *byte count*
to `backend_capture_finish`. `stiva wait` therefore always yielded 0, `state.json`'s `exit_status`
was wrong across restarts, and `on-failure` restart policies could never observe a failure. It was
the same defect kavach 3.9.1 fixed for the PROCESS backend, applied to only one of the two — and
the OCI one is what gets *selected* whenever runc is present. Authored from here; see kavach's
3.9.2 CHANGELOG, which also covers two follow-ons found in the same pass: runc's own diagnostics
were being quarantined by kavach's secret scanner (so every runtime error read
"externalization blocked" instead of its cause), and container **stderr had been bypassing the
externalization gate entirely** because it went to `/dev/null`.

### Tests — 1906 → 1983 unit, smoke 69 → 78
`build.cyr` now also lands in `tests/registry.tcyr`: its driver needs exactly that 6-module include
set, the canned transport there is the only way to exercise a remote base without a network, and it
puts the driver in the unit shape the cycc 20/21 miscompile is known to bite — which the 26-module
unit would not prove. `test_mcp_handle_build` moved from `tests/stiva.tcyr` to `tests/mgmt.tcyr`
with the handler. New benchmark `build_copy_layer_20_files_4kib` ≈ **2.7 ms** (tar collect +
assemble + gzip + sha256 + blob store), measured because the path holds ~3× the context size in
heap with no `free`, so a regression compounds across steps rather than staying flat.

## [3.0.14] — 2026-07-25 — `run -d` is live · containers run their own filesystem (§K)

### Added — detached `run -d`, the first item off the v3.1 blocked list
kavach **3.9.0** shipped `sandbox_spawn` (authored from here — see its CHANGELOG, which also
carries a defect that made **every seccomp filter kavach ever built a zero-filled buffer**, so
`security_load_seccomp` had never once succeeded). stiva now wires it:

- `spawn_container` in `runtime.cyr` — the detached counterpart of `exec_container`, same
  `build_sandbox`, so identical policy treatment; only the waiting differs.
- The detach branch in `container_manager_start`, `container_manager_reap`, and `wait`/`stop`
  routed through the handle.
- `stiva run -d` returns the container id immediately. **30 of 35** verbs live.

**The record survives the spawning process.** `container_fixup_after_restart` rewrote
RUNNING→STOPPED for *every* record loaded from disk — correct while nothing could outlive the
process, wrong the moment something can. It now probes liveness, guarded against **pid reuse** by a
new persisted `start_ticks` (`/proc/<pid>/stat` field 22, captured at spawn): a recycled pid would
otherwise read as a live container and `stop` would signal an innocent process. `start_ticks` is
appended at offset 80 so no existing field offset moves.

**`stop` works cross-process.** `internals` is in-memory, so a second `stiva` has no
`SpawnedProcess` handle and falls back to signalling the recorded pid — SIGTERM, grace, SIGKILL,
polling the starttime-checked probe, since after reparenting to init it cannot `waitpid`. Without
this the record read `Stopped` while the container kept running, which the smoke suite now asserts
against directly.

### Filed — the remaining v3.1.0 gate
`cyrius/docs/development/issues/2026-07-25-stiva-stackless-coroutines-interactive-exec.md`.
cyrius parks stackless coroutines as an *Unpinned follow-on* with "No live consumer; pull forward
on a real suspend-across-await need" — stiva is that consumer (interactive `exec -it`, multiplexed
streaming), and the roadmap has carried "has not filed; filing is the unblock lever" as an open
action for weeks. Now on record, with the shipped workarounds documented so other consumers can
reuse them. A decline is an acceptable outcome; an indefinitely-held slot is not.

### Fixed — `stiva run` executes INSIDE the container rootfs
The defect documented as "Known" in 3.0.14. `stiva run local/demo:v1 /bin/hello`, where the binary
exists only in the image, now prints its output instead of exiting 0 having run nothing.

The stiva side is one line — `config_rootfs(cfg, sp.rootfs)` in `build_sandbox`; `RuntimeSpec`
always carried the rootfs, it simply never reached kavach. Everything else was upstream, in
**kavach 3.9.1**, and it was four defects stacked behind one another, each only visible once the
previous fell:

1. `SandboxConfig` had **no rootfs field**, so this could not be expressed at all.
2. The OCI backend (selected whenever `runc`/`crun` exists — i.e. most hosts) built its bundle
   around a fresh **empty** `/tmp` directory and ignored the sandbox.
3. That bundle declared **no user namespace or id mappings**, so an unprivileged runc refused
   outright, and **no `/proc` or `/dev` mounts**, without which runc panics in its own init.
4. `process.args` was `/bin/sh -c <string>`, making every container depend on the **image**
   shipping a shell — fatal for distroless, scratch, or a single static binary.

Plus `runc` being invoked with an empty environment, defaulting its state root to `/run/runc` and
dying with `permission denied` — into `/dev/null`, because the capture discards stderr. Fixed with
an explicit `--root`. A failed `execve` also no longer reports exit 0.

Each of these individually produced the same symptom: a container that ran nothing and reported
success. That is why it survived so long — every test used a binary that happened to exist on the
host too.

### Fixed — layers larger than 1 MiB were written as corrupt gzip (every real image)

`stiva import` of a rootfs tar bigger than ~1 MiB produced an image whose layer blob could not be
unpacked. `stiva run` then failed with `layer unpack error: sha256:…`, leaving
`{root}/layers/<digest>/` **created but empty**. 849 KB worked, 1.69 MB did not. With §K closed in
3.0.14 — containers now run their own filesystem — every real base image is far above that line,
so this was the difference between the runtime working on toy tarballs and working at all.

The failure was on the **write** side, not the read side. `unpack_layer` and `_stor_extract_tar`
are unchanged and were never at fault; they were handed a malformed blob.

**Root cause — sankoch's batch gzip encoder (cyrius 6.4.78).**
`_deflate_compress_level_inner` (`lib/sankoch.cyr:4814`) splits input into 1 MiB outer blocks
(`DEFLATE_BLOCK_SIZE`, `:4792`) and resumes each next block at `block_end`. But the per-block
encoders — `_deflate_compress_fixed_block` (`:4845`, levels 1–3) and
`_deflate_compress_dynamic_block` (`:5100`, levels 4–9) — deliberately match against the **full**
`src`, so a match beginning just below the boundary carries up to `LZ77_MAX_MATCH` (258) bytes
across it. The chunker discards that overshoot and the following block re-encodes the same bytes.
The stream therefore decodes to `src_len + overshoot`, the gzip trailer's CRC-32 no longer matches
the output, and `gzip_decompress` returns `-ERR_CHECKSUM_MISMATCH` (-5) — which `unpack_layer`
folds into its `-1` sentinel, hence the bare `layer unpack error`.

Measured on a 2 MB input: **220 bytes duplicated at offset 1048576**, decoded length 2000220. GNU
`gunzip` rejects that same stream at that same offset (`differ: char 1048797`), so it is a
genuinely malformed stream and not a sankoch decoder disagreement — and the sankoch **decoder is
correct**: a stream from GNU `gzip` round-trips byte-exact. Both Huffman paths are affected, so
dropping the compression level was not an escape.

Pre-existing, not a 3.0.13/3.0.14 regression: reproduced against `git show HEAD:src/storage.cyr`
as well as the working tree.

**Fix.** New `_stor_gzip_compress` (`src/storage.cyr`) drives sankoch's **streaming** encoder
(`gzip_enc_init` / `gzip_enc_write` / `gzip_enc_finish`) instead. That path is not affected:
`_denc_consume` (`lib/sankoch.cyr:5373`) stores the offset its LZ77 loop actually reached back into
the encoder ctx, so an overshoot carries into the next consume rather than being re-encoded. Both
of stiva's compress call sites moved onto it — `image_import` and `_il_docker_add_layer`, both in
`src/imagelayout.cyr`. `gzip_enc_init` takes sankoch's mutex and `gzip_enc_finish` is what releases
it, so the helper always runs finish, including after a write error.

**Trade.** Compressing 2 MiB costs 42.7 ms streaming vs 33.3 ms batch (~28% slower), paid once per
`import` / archive `load` — never on the container-start path. Compressed output is marginally
**smaller** (4904 vs 4943 bytes on that 2 MB input): RFC 1951 caps back-references at 32 KiB
regardless, so the 1 MiB outer chunk never bought extra match reach to lose.

**Existing images are not repaired by this.** A layer blob written by an affected stiva is corrupt
on disk and stays corrupt — its digest is over the bad bytes. Re-run `stiva import` (or re-`pull`)
for anything imported before this change. Layers **pulled from a registry** were never affected;
they come from a correct encoder, and stiva only ever verified and stored them.

**Filed upstream** as
`cyrius/docs/development/issues/2026-07-25-sankoch-batch-gzip-duplicates-bytes-at-block-boundary.md`
— the batch chunker has to resume at the position its block encoder actually reached, not at
`block_end`. The workaround here should be revisited, and the throughput reclaimed, once that lands.

### Added — multi-megabyte layer coverage
Three regression tests in `tests/store.tcyr`, each sized to cross the 1 MiB block boundary at least
twice so a single-boundary escape cannot pass: `gzip_compress_multi_mb_roundtrip` (the codec alone —
2.5 MiB in, exact length and bytes out), `unpack_layer_multi_mb` (a 2.5 MiB five-file layer blob,
every file byte-compared after extraction), and `image_import_multi_mb` (the user-visible path —
`image_import` a 2 MiB rootfs tar, then `prepare_layers`, then byte-compare). All three were
confirmed to **fail** (8 assertions) against the batch encoder before the fix. Store file:
221 → 255 assertions; suite total **1886**.

Two benchmarks in `tests/stiva.bcyr` — `gzip_compress_2mib` (42.9 ms) and `gzip_decompress_2mib`
(10.1 ms) — so the codec has a trend line rather than a one-off number, and the cost of the
streaming switch stays visible.

### Security — the gzip layer path had no decompression-bomb ceiling
`_stor_unpack_zstd_bytes` has enforced two ceilings since zstd decode landed — 8 GiB absolute and
1000:1 against the compressed length — and the comment above them argued the gzip path "is NOT
exposed this way" because its grow-loop reacts to actual decoded output rather than a declared
frame size. That is true of **allocation amplification from a forged header**, and it is the wrong
conclusion: it does not bound the loop. `_stor_unpack_gz_bytes` started at `max(blob_len * 32,
1 MiB)`, doubled on `ERR_BUFFER_TOO_SMALL`, and gave up only after **24 doublings** — so a blob
that genuinely decodes large walked the buffer toward ~16 TiB one allocation at a time, reaching
the same DoS in steps instead of in one jump. Layer blobs are attacker-influenced: `stiva pull`
takes them from an arbitrary registry, `stiva load` from an untrusted oci-archive/docker-archive.

Both codecs now size their output against one shared bound, `_stor_decomp_limit(blob_len)` —
`_STOR_MAX_RATIO` (1000:1), floored at `_STOR_MIN_OUT` (1 MiB) and capped at `_STOR_MAX_OUT`
(8 GiB). The gzip loop clamps each doubling to it and returns `-2` on reaching it; the zstd path
checks its declared `Frame_Content_Size` against it and clamps its own fallback grow-retry. Peak
live allocation is now exactly the limit and cumulative allocation across a full doubling run is
~2x it, where before both were unbounded. The `tries > 24` counter is kept as a backstop, but the
ceiling binds first (1 MiB → 8 GiB is 13 doublings).

Two notes on the shape of the bound:

- **The 1 MiB floor is load-bearing, and fixes a latent false-reject on the zstd side.** The
  guessed starting cap is 1 MiB, so applying a bare ratio would reject blobs that cannot clear it —
  a 30-byte zstd frame declaring 20 KiB of zeros (RLE reaches such ratios trivially) was rejected
  outright before this change, as was any FCS-less zstd blob under 1049 bytes.
- **On the gzip path the absolute ceiling is the operative bound.** deflate's own maximum is
  1032:1, so the ratio can only reject a near-pure run of one repeated byte — a legitimate layer
  that is essentially all zeros and larger than 1 MiB is now refused. zstd has no such limit, which
  is why the ratio carries that path.

### Changed — kavach 3.9.0 → 3.9.1
`config_rootfs`, the shared `src/confine.cyr` child-confinement sequence, rootless OCI, and the
`--root` fix. See its CHANGELOG.

### Tests — 1804 → 1906 unit, smoke 64 → 69
Multi-megabyte layer coverage and the gzip-bomb ceiling tests came in from the two spun-out
worktrees (`claude/elated-lewin-92e770`, `claude/adoring-mendel-c90f1a`); their storage.cyr changes
are complementary — one fixes the ENCODE path, the other bounds the DECODE path — and were merged
by hand where they touched the same region.

The detached lifecycle is covered end-to-end by the smoke suite, since `main.cyr` is not unit
testable: id returned immediately, a NEW process still sees it Running, the process is genuinely
alive, cross-process `stop` actually kills it, and `ps` stops reporting it.

## [3.0.13] — 2026-07-25 — §E complete (`stiva events`) · security: image-controlled writes escaped the rootfs

### Added — lifecycle events are written to disk
`container_manager_publish_event` still publishes to the majra hub, and now **also appends the
payload to `{root}/events.jsonl`** — one JSON object per line, through `file_append_locked`
(O_APPEND under an flock), so concurrent stiva processes interleave whole lines instead of
shredding each other. Rotation reuses the pair the container logs already use,
`container_should_rotate` + `rotate_logs`: `events.jsonl` → `.1` → … → `.N`, oldest dropped.
Defaults are 8 MiB × 3 generations; `STIVA_EVENTS_MAX_BYTES` and `STIVA_EVENTS_MAX_FILES` tune
them, and a non-positive value from the environment is **refused** rather than honored — `atoi`
returns 0 for junk and 0 means "never rotate", so a typo would otherwise buy an unbounded log.

The append is best-effort by contract: it never fails the lifecycle operation that produced the
event. `STIVA_EVENTS=off` turns persistence off entirely.

**The log is per-root, not per-container.** `stiva events` is a whole-runtime stream, so
per-container files would have to be discovered and merged in timestamp order; a container that
failed before its directory existed would have nowhere to write; and decisively,
`container_manager_remove` deletes `{root}/containers/{id}`, which would destroy the `removed`
event as it was being written. A log that loses the record of a removal at the moment of the
removal is not an event log.

Events gained a **`ts`** field, epoch millis from `clock_epoch_ns`. Deliberately *not*
`clock_now_ms`, which `lib/chrono.cyr:45` documents as monotonic (ns since boot): stamping an
uptime would make `stiva events --since $(date +%s)` compare a date against a boot offset and
silently match nothing. A test bounds `ts` against `clock_epoch_secs` to keep that substitution
from creeping back.

### Added — `stiva events` (30 of 35 verbs live)
`stiva events [--since SEC] [--until SEC] [-n COUNT] [-f]`, reading the log above.

**What terminates it** was the open design question. Unlike `logs -f` there is no single container
whose state ends the stream, so the plain form is bounded by default: without `-f` it dumps the
matching events and exits. With `-f` it stops on the first of `--count N`, wall clock past
`--until T`, or an interrupt; with neither bound it follows until interrupted, the way
`docker events` does. Every terminator is asserted under a `timeout` in `scripts/cli-smoke.sh` — an
`events` that hangs is precisely the failure this design exists to avoid.

`--since` / `--until` take Unix **seconds** (what `date +%s` gives) and are inclusive on both ends;
the log stores millis. An inverted window is a usage error rather than a silently empty stream,
because zero events looks exactly like "the runtime did nothing". A line whose `ts` will not parse
is **always** shown: an event stream is an audit surface, and dropping the malformed record hides
the evidence. A log ending in a torn record skips that record on stdout and says so on stderr.

Output is the stored JSON line **verbatim**, one per line — no `[HH:MM:SS.mmm] ` prefix like the
oracle's, which would break `stiva events | jq`. Hints and warnings go to stderr, so
`stiva events > file` on a fresh root yields an empty file rather than a message.

The oracle (`rust-old/src/main.rs:525`) is not a parity target here. It subscribed to the
in-process bus and looped on `rx.recv()` forever — in a one-shot CLI, against a publisher nothing
else in the process ever publishes to, so it printed nothing until Ctrl+C. This is a rewrite; the
file is what makes it observable across processes.

### Fixed — the follower tracks the log's inode, not just its size
First cut detected rotation with `size < offset` and lost the stream permanently when it missed.
This is the ordinary case, not a corner: lifecycle events are near enough fixed-width that a
rotated log grows back to *exactly* the offset the reader held, at which point `size < pos` is
false, `size > pos` is false, and the follower sits quiet forever with events accumulating in front
of it. Caught by a smoke run across a forced rotation, not by reading the code. The follower now
compares `st_ino` and restarts at the new file's beginning when the identity changes, falling back
to the size test for truncation-in-place.

### Benchmarks — what persistence costs on the run path
Persisting on every state change puts I/O on the container path, so it was measured rather than
assumed (`tests/stiva.bcyr`, which now includes the container chain):

| benchmark | result |
|---|---|
| `event_json_build` | 5.98 µs |
| `event_log_append` (open+flock+write+close, no rotation) | **7.78 µs** |
| `event_ts_parse` (read path only) | 1.58 µs |
| `flatten_layers_2x60_files_4kib` (context: one create) | 1.34 ms |

Only the append is new — the JSON was already built for the bus. A full container lifecycle
(created/started/stopped/removed) therefore pays **~31 µs** of added I/O, against the 1.34 ms a
single `flatten_layers` costs on the same create. Acceptable; `STIVA_EVENTS=off` exists regardless.

### Tests
+9 in `tests/mgmt.tcyr` (**1846** total): per-root path, one-line-per-event framing, null/empty
guards, root auto-creation, rotation at `max_files` (including that `.3` is dropped at
`max_files=2`), `max_bytes=0` meaning never-rotate, publish→persist with the wall-clock `ts` bound,
the `removed`-event-survives-`rm` case that justifies per-root, `container_event_ts` across
valid/missing/null/unparsable/torn input, and `container_event_in_range` boundaries.
+18 assertions in `scripts/cli-smoke.sh` (**61** total), including a cross-process follow: one
process follows while another runs a container, which is the property the in-memory bus could never
have.

### Superseded
The 3.0.12 "Not shipped — `events`, and why" note below. Its diagnosis was right; this release
removes the cause.

Adversarial review of everything since 3.0.10 — the Tier-1 CLI verbs, cmdit 1.2.0, §E MCP
dispatch, and the agent-authored rootfs flatten — returned **14 confirmed findings**. All are fixed.
The first is the reason this is a point release rather than a footnote.

### Security — CRITICAL: a crafted image could overwrite any file the daemon can write
`flatten_layers` (`src/storage.cyr`), the no-overlayfs rootfs path added in 3.0.12, displaced a
lower layer's entry with `sys_unlink` **and never checked that it worked**. A read-only parent
directory is enough to make that unlink fail — and a read-only directory is ordinary image content.
The surviving symlink was then `chmod`'d and either descended into or written through:

- **Arbitrary host file overwrite.** Layer 1 ships `etc/passwd -> /some/host/file`; layer 2 ships
  `etc/` at mode 0555 containing a real `etc/passwd`. The write followed the link. `O_TRUNC` on an
  existing path needs no permission on the parent, so the target only had to be writable by the
  stiva process. Reproduced end to end; `flatten_layers` returned **success**.
- **Arbitrary file creation outside the rootfs**, via the directory branch recursing through a
  surviving symlink.

Absolute symlink targets are deliberately permitted by `_stor_target_escapes`, so such a layer
extracts cleanly and the whole chain is image-author-controlled. It fires on any unprivileged
`stiva run`, because the flatten fallback *is* the rootfs path without `CAP_SYS_ADMIN`.

The 3.0.12 commit claimed this escape was closed, and its test
(`test_flatten_layers_no_symlink_escape`) genuinely passes — but only exercises a **writable**
destination parent, where the unlink succeeds and the guard is never reached.

Fixed four ways, so no single check is load-bearing: `_stor_replace_existing` now verifies the path
is actually gone and reports failure; the directory branch `lstat`s the destination and refuses to
descend unless it is a real directory; the copy opens with `O_NOFOLLOW`; and directory modes are
applied on the way **out** of the walk, which removes the read-only-parent precondition entirely.
New regression tests cover the read-only-parent case specifically — verified to FAIL against the
3.0.12 code.

### Fixed — HIGH
- **`stiva top` never worked, for any container in any state.** It passed an empty allowed-state
  list to `container_manager_require_pid`, and `_container_state_allowed` loops `i < n` — so `n=0`
  matches nothing, the inverse of "any state". The smoke test passed for the wrong reason, since a
  stopped container yields the same message either way. Now pinned by a unit test that
  distinguishes them.
- **`logs --follow` never followed.** It terminated on container state, but
  `container_fixup_after_restart` rewrites RUNNING/PAUSED to STOPPED for every record loaded from
  disk, so a fresh CLI process can never see a container as running: the loop always exited after
  ~414 ms. Against a writer appending for 3 s it captured 3 lines of 11. Termination is now
  **quiescence**-based; measured capturing 8 of 8. State becomes the better signal once `run -d`
  lands (v3.1) and there is a live pid to probe.
- **A 0555 directory anywhere in an image failed the whole `create`** — unprivileged only, because
  root has `CAP_DAC_OVERRIDE`, so it was invisible to any privileged test. Every `/nix/store` path
  is 0555. Fixed by the deferred-chmod pass above.
- **`stiva cp` passed the container-side path through unvalidated**, both directions, exiting 0.
  `..` components are now refused. (The symlink half — an image shipping `app -> /` — remains, and
  is oracle parity; tracked in the roadmap.)

### Fixed — MEDIUM / LOW
- An unreadable source file (mode 0000, legal image content) failed the entire flatten; now skipped
  with a warning, since overlayfs would simply surface it as unreadable inside the container.
- Symlink targets over 1023 bytes were silently truncated into a link to a **different** path;
  now read at `PATH_MAX` and refused if truncation is indistinguishable.
- `_cli_split_colon` split legal Linux paths containing `:` — `cp /work/a:b/f.txt out` looked for a
  container named `/work/a`. Unambiguous host paths (`/`, `./`, `../`) are now exempt, as Docker's
  `splitCpArg` does.
- `stiva cp <dir> c1:` splatted the directory over the container root; an empty container-side path
  is refused.
- `logs -n N --follow` silently ignored `-n`; it now prints the tail then follows, as docker does.
- `kill` on a missing container printed two errors, the second false.
- `scripts/cli-smoke.sh` had a genuine ~11%/run flake: it asserted `c1`'s absence against the whole
  `ps` table, and `c1` is a valid hex bigram inside a 32-hex container ID. Now asserts the name
  column only.

### Changed — cmdit 1.2.0 → 1.2.1
Two defects in the completion generator, neither of which exposed stiva (its verb and flag names
are safe literals) but both of which broke the library's contract:
- The verb-position guard failed whenever a global flag preceded the verb — `stiva --root /x <TAB>`
  completed filenames instead of verbs. Verified live against bash 5.3.
- 1.2.0's changelog claimed every interpolated value was escaped; verb and flag names were not, and
  the program name was emitted unquoted. Escaping is also **insufficient** — `compgen -W` re-expands
  each word, so a `$(…)` in a name would run at TAB time inside correct quoting. Names are now
  whitelist-filtered.

> **Release ordering:** `[deps.cmdit] tag = "1.2.1"`; cmdit 1.2.1 must be pushed and tagged before
> stiva 3.0.13 builds from a clean clone.

### Tests — review fixes: suite → 1858, smoke → 64

## [3.0.12] — 2026-07-25 — §E MCP live dispatch · `logs -f` · the empty-rootfs fix

### Added — roadmap §E: MCP tools now drive the runtime
`mcp_handle_tool` plus `mcp_handle_pull` / `_ps` / `_stop` / `_push` / `_inspect`, and the resource
layer (`mcp_list_resources` / `mcp_read_resource`). Until now `src/mcp.cyr` had the tool schema and
two facade-free handlers (build, ansamblu); everything touching a container or image was a comment
block listing what the async oracle did.

These land in **`stiva_core.cyr`, not `mcp.cyr`** — include order puts `mcp.cyr` first, so it
cannot reach the `Stiva` facade. `mcp.cyr` keeps the schema, `McpResult`, and the two facade-free
handlers; everything that drives the runtime sits downstream. Same split, and the same reason, as
the §B pull driver living in `imagelayout.cyr`. The facade gained `stiva_images` and
`stiva_inspect_image` to serve it.

**`stiva_run` and `stiva_exec` stay advertised in `tool_list()`** and return a precise
unavailable-and-why error rather than being dropped from the schema. The schema is the contract a
client caches; silently shrinking it is worse than a clear failure, and both are gated on work not
done rather than on anything a caller can fix — `run` on kavach `sandbox_spawn` (the oracle's MCP
run is detached, `mcp.rs:291-334`), `exec` on §D.

One shape divergence, deliberate: `config.command` / `env` / `ports` / `volumes` are always emitted
as `[]` / `{}` rather than omitted when empty. `_il_str_arr` omits an empty vec, which is right for
a manifest and wrong here — an MCP client reading `config.command` should find an empty array, not
a missing field it has to special-case.

### Added — `stiva logs -f`
Follows the log and terminates once the container is no longer Running, exactly as `docker logs -f`
does. A follow that hung on a stopped container would wedge any script using it, so that
termination is asserted by the smoke suite with a `timeout`.

It polls the **file**, deliberately, rather than subscribing to the lifecycle bus: the bus is a
per-`ContainerManager` in-process `pubsub_new()` (`src/container.cyr:896`) and is never persisted,
so a second process sees an empty one. The log is on disk and therefore observable across
processes.

`--scan` is **refused** alongside `--follow`. The externalization gate scans a *complete* output;
redacting a stream a chunk at a time could split a secret across two reads and let both halves
through. Failing loudly beats a redaction that silently does not hold.

### Not shipped — `events`, and why
Roadmap §E pairs `logs -f` with `events`. `events` is blocked, and not by anything external:
lifecycle events exist only in that same in-memory bus, so a one-shot `stiva events` would
construct its own manager, subscribe to its own empty publisher, and poll forever printing nothing
— strictly worse than the current "not yet wired" message. It needs events persisted (a rotated
JSONL log under the root) before the verb can mean anything. Filed rather than shipped as a verb
that cannot work.

### Fixed — containers ran with no filesystem when overlayfs was unavailable
The defect filed as "Known" in 3.0.11. `container_manager_create` unpacked the image layers with
`prepare_layers`, then — whenever `setup_overlay` could not mount, **which is every unprivileged
run** — fell back to a bare `{croot}/rootfs`, created it, and **threw the unpacked layer dirs
away**. The container started against an empty directory, so
`stiva run local/demo:v1 /bin/echo hi` was executing a path that did not exist.

This was **exact parity** with the frozen oracle
(`rust-old/src/container.rs:366-369`, `unwrap_or_else(|| container_root.join("rootfs"))`), so it
is inherited rather than a port divergence. The oracle stays frozen; the fix below is a
**deliberate divergence from it**, and the only one on the create path.

**`flatten_layers` (`src/storage.cyr`) — net-new, no oracle counterpart.** The prepared layer dirs
are merged into `{croot}/rootfs` bottom-to-top, later layers winning: the same view the overlayfs
lowerdir stack would have produced. Four things worth stating, because each is a place the obvious
implementation is wrong:

- **Copy, not hard-link.** Linking is nearly free, and it is also how you corrupt the image store:
  the links would share inodes with the store's `layers/` cache, so the first write inside *any*
  container would rewrite the layer every other container reads. Docker's `vfs` driver copies for
  the same reason.
- **Perms and symlinks are preserved.** mode via `chmod`, uid/gid via `lchown`, chown **before**
  chmod because chown clears setuid/setgid, and symlinks recreated as links rather than
  dereferenced — the ordering `_stor_extract_tar` already uses. A rootfs whose `/bin/sh` lost its
  exec bit in the copy is still an unrunnable container, so this is not cosmetic.
- **A directory displaces a same-named symlink from a lower layer** instead of being descended
  through. Without that, a layer shipping `usr → /etc` followed by one shipping `usr/passwd`
  writes **outside** the rootfs — the escape `_stor_has_symlink_ancestor` blocks on the extraction
  side, reintroduced here by the flatten. Covered by a test that asserts nothing lands in the
  symlink's target directory.
- **Bytes stream through one shared 64 KiB buffer**, not `_stor_read_file` + `file_write_all`.
  `alloc` is a bump arena with no free, so the whole-file form would hold *every file of the tree*
  in memory at once; peak is now one chunk regardless of image size.

`prepare_layers`' own `.unpacked` bookkeeping marker is skipped — it is not image content and must
not appear in a container's `/`.

**On by default; opt out with `STIVA_ROOTFS_FALLBACK=none`.** Flattening costs one copy of the
image per container (reclaimed by `stiva rm` along with the rest of `{croot}`), which is a real
cost worth being able to decline — a `create` that is never started, or a privileged host that
will mount the overlay itself, gains nothing from it. But an empty rootfs is not a cheaper
container, it is a broken one, so off is not a defensible default. `none` restores the exact
oracle behaviour; an unrecognized value warns and keeps the default rather than silently guessing
(`STIVA_ROOTFS_FALLBACK=off` would otherwise look like a broken knob). A flatten that fails
**fails the create** — a partial rootfs runs worse than no rootfs, and silently handing back a
half-populated container is how this class of bug survives to begin with.

### Added — `{croot}/.rootfs-flattened`, so `diff` can tell the two layouts apart
`diff` (still not wired) means different things over the two rootfs shapes: with an overlay the
changed set **is** `{croot}/upper`, whereas a flattened rootfs has no upper layer and has to be
compared against the layer dirs. Nothing already on disk distinguishes them — `internals` is
process-local and never persisted, and `setup_overlay` creates `upper`/`work`/`merged`
**even when the mount fails**, so their presence proves nothing about which path ran. `create`
now drops a marker file for whoever implements the verb.

### Tests — 1739 → 1804, smoke 40 → 43
MCP dispatch is covered in the **26-module** `tests/mgmt.tcyr` — the include set `src/main.cyr`
itself uses. Coverage includes the two failure contracts that matter: an unknown tool and an
unavailable-but-advertised tool must both fail *with a reason*, and `mcp_read_resource` must
distinguish an unknown URI scheme from a missing object rather than collapsing both to "not found".

### Known — OCI whiteouts are still not applied, by *either* rootfs path
`.wh.<name>` / `.wh..wh..opq` markers are extracted literally by `_stor_extract_tar`, and
overlayfs honours only its own char-dev-0:0 whiteouts rather than the tar convention, so a file
deleted in an upper layer stays visible over an overlay *and* after a flatten. `flatten_layers`
deliberately matches that instead of fixing it unilaterally — handling whiteouts only in the
fallback would make an unprivileged rootfs differ from a privileged one for the same image. The
fix belongs at unpack time, where it lands for both paths at once. `rust-old/src/storage.rs` has
no whiteout handling either. Tracked in `docs/development/roadmap.md`.

### Tests + benchmarks
**1739 → 1771.** `tests/store.tcyr` gains `flatten_layers_stacks_and_preserves_perms`,
`flatten_layers_empty` (a layerless image flattens to an empty rootfs and **succeeds** — "no
filesystem" and "the copy failed" are different facts), `flatten_layers_no_symlink_escape`, and
`flatten_layers_from_prepared` (over the real gzip unpack path). `tests/runpath.tcyr` gains
`cm_create_rootfs_populated`, which drives `container_manager_create` end-to-end and asserts
through `container_manager_get_rootfs` so it holds whether the overlay mounted or the fallback
ran. New benchmark `flatten_layers_2x60_files_4kib` — **≈1.36 ms** for ~240 KiB across 60 files
over two layers; I/O bound, so the CSV trend rather than the absolute number is the guard.

## [3.0.11] — 2026-07-25 — Tier-1 CLI sweep: 6 verbs wired · cmdit 1.2.0 · main.cyr gets coverage

### Added — six verbs whose logic was already ported, now reachable
`kill`, `restart`, `rename`, `top`, `cp`, `completions`. **29 of 35** verbs now execute
end-to-end. Every one of these was pure wiring — `container_manager_signal`/`_restart`/`_rename`,
`container_top`, and `copy_into_container`/`copy_from_container` were all ported and tested; the
binary simply would not call them.

These handlers use cmdit's own validators (`cmdit_require_positionals`, `cmdit_range`) instead of
the hand-rolled positional-count checks the older handlers use, so usage text and the error
taxonomy come from one place: `stiva kill c1 -s 99` now reports `--signal: value out of range`
from the parser rather than failing somewhere downstream.

- `kill` takes a signal **number** (1–64, default 15 = SIGTERM), matching the oracle
  (`rust-old/src/main.rs:135-142`).
- `cp` requires **exactly one** side to be `<container>:<path>`. Both sides or neither is refused
  rather than guessed — a container-to-container or host-to-host copy is not what the verb means,
  and picking one silently would do the wrong thing.
- `completions` writes the script to **stdout** and every diagnostic to **stderr**. That
  distinction is load-bearing: the output is meant to be redirected into a completions file, and a
  usage line mixed into it becomes a syntax error inside the user's shell config. `_cli_eputs` was
  added for it; the first cut of this verb wrote its error to stdout and the new smoke suite
  caught it.

### Changed — cmdit 1.1.0 → 1.2.0 (verb introspection + shell completions)
> **Release ordering:** `cyrius.cyml` now pins `[deps.cmdit] tag = "1.2.0"` and `cyrius.lock`
> carries its hash, so **cmdit 1.2.0 must be pushed and tagged before stiva 3.0.11 builds from a
> clean clone**. Local builds resolve through the `path = "../cmdit"` override and will not warn.

`completions` is driven by **cmdit's own verb table**, not a hand-kept list, so the generated
script cannot drift from the CLI: adding a verb updates the completions with no second edit.
cmdit had the table but exposed no accessor, so this needed upstream work — `cmdit_verb_count` /
`_name_at` / `_help_at` / `_is_alias` / `_canonical_at`, plus `cmdit_completions(h, shell)`
emitting bash, zsh, or fish. See the cmdit 1.2.0 CHANGELOG; the generated output is
syntax-checked against real `bash -n` and `zsh -n`.

### Fixed
- **`kill` on a non-running container failed silently.** `container_manager_signal`'s comment
  claims "detail printed by require_pid", but `container_manager_require_pid` prints only for a
  *missing* container — a container that exists in the wrong state returns a bare code. The verb
  exited 1 having said nothing at all.

### Added — `scripts/cli-smoke.sh`, the first coverage `src/main.cyr` has ever had
40 assertions over the live verbs. `main.cyr` ends in `var exit_code = main(); syscall(60, …)`, so
it cannot be included in a `.tcyr` unit — any test file that included it would run the CLI and exit
before reaching its own assertions. Every other module is covered by `cyrius tests tests/`; the CLI
handlers were covered by nothing. That gap is not theoretical: v3.0.7 shipped `logs` and
`get_rootfs` resolving the container *name* where they needed the *id*, and only an ad-hoc binary
run caught it. This suite makes that repeatable, and it found the stdout-pollution bug above on its
first run.

### Known — unrelated defect surfaced while smoke-testing `cp`
`container_manager_create` unpacks image layers via `prepare_layers`, then falls back to an
**empty** `{croot}/rootfs` when the overlay mount is unavailable — the normal case for an
unprivileged user — discarding the unpacked layers entirely. A container then runs with no
filesystem. This is **exact parity** with the oracle (`rust-old/src/container.rs:366-369`,
`unwrap_or_else(|| container_root.join("rootfs"))`), so it is inherited rather than a port
divergence, and it is out of scope for a CLI-wiring increment. Filed separately.

## [3.0.10] — 2026-07-25 — §B COMPLETE: discovery + `stiva pull` / `stiva push` are live

### Added — roadmap §B Inc-9: discovery
`registry_list_tags` (`GET /v2/<repo>/tags/list`), `registry_catalog` (`GET /v2/_catalog`),
`registry_referrers` (`GET /v2/<repo>/referrers/<digest>`, OCI distribution v1.1.0), and
`registry_verify_signature`. Three contracts worth stating, because each is a place the oracle
loses information:

- **An empty list and a failed query are different values.** An absent or non-array field yields
  an empty vec (the oracle's `unwrap_or_default`); a failed *request* yields 0. "This repository
  has no tags" must not read the same as "the question could not be asked".
- **`registry_verify_signature` returns 1 / 0 / −1**, not a bool. The oracle returns `Ok(false)`
  for unsigned and `Err` for a failed query — routinely collapsed by callers, which is how an
  *unverifiable* image comes to be treated as merely an *unsigned* one. It also only checks that a
  cosign/notation artifact **exists**; it does not verify the signature cryptographically. Neither
  does the oracle, despite the name.
- **A malformed referrers entry is skipped**, unlike a manifest's layers where a bad descriptor
  fails the whole parse. Referrers is a discovery list — one unreadable artifact should not hide
  the readable ones — whereas a missing layer is a broken image.

**Divergence:** the oracle queries referrers with `image.id`, the **config** digest
(`rust-old/src/image.rs:531`), which no cosign or notation `subject` ever carries — so its
signature lookup can only ever come back empty. We key on the **manifest** digest.

### Added — roadmap §B Inc-10: the facade and CLI
`stiva_pull` / `stiva_push` / `stiva_list_tags` / `stiva_catalog` / `stiva_verify_signature` on the
`Stiva` facade; `registry_client` is now always constructed, and `stiva_with_registry` builds it
from the supplied credentials and mirrors instead of parking them. `stiva pull <IMAGE>` and
`stiva push <IMAGE> [TARGET]` are **live** — **23 of 35** verbs now execute end-to-end.

`AUDIT_OP_PUSH` is emitted on both success and failure. That is **net-new, not parity**: the oracle
never constructs `AuditOperation::Push` anywhere, so a push left no trace in the audit log at all.
Publishing an image is precisely the operation you want a record of.

### Security — two ways an image ID could redirect a request to Docker Hub
`image_ref_parse("sha256:<64hex>")` has no digest special-case: no `@`, no `/`, so it rsplits at
the last `:` and yields `docker.io/library/sha256:<hex>`. Both new verbs accept a config-digest ID
as a lookup key, and both initially parsed that key into the target reference:

- **`stiva push <image-id>` with no TARGET** would have pushed a local — possibly private — image
  to **Docker Hub**, under a repository literally named `sha256`.
- **`stiva verify-signature <image-id>`** would have sent that repository's manifest digest to
  Docker Hub and run a `repository:library/sha256:pull` bearer exchange against any `docker.io`
  credential in the store — while reporting a genuinely signed image as unverifiable.

Both now resolve the image against the local index **first** and take the registry target from what
the store holds, never from the lookup key — matching the oracle, which sidesteps this by taking an
`&Image` (`lib.rs:322`). The second was caught only by adversarial review: the same defect, in the
sibling function, after the first had been found and fixed.

### Fixed
- **Index dedup is now digest-aware** (deferred from the 3.0.9 review). `image_ref_full_ref` drops
  the digest and a digest-only reference parses with tag `latest`, so two digest-pinned pulls of
  one repository rendered under the same key — the second silently evicting the first, leaving
  different content at one name and the first image's blobs orphaned until `gc`. A pinned add now
  replaces only the same manifest digest; an unpinned (tag) add still replaces, because a tag is a
  mutable pointer.
  **Known limitation:** `index.json` carries the reference in the
  `org.opencontainers.image.ref.name` annotation, which has no digest field, so a pinned reference
  does not survive a store reload — after a restart both entries read as unpinned. Fixing that is
  an on-disk format change, deliberately not in this increment.
- **`image_store_find` is first-match-wins**, matching the oracle's `.find()` and the two other
  resolvers in the tree (`_stiva_find_image`, `_cli_find_image`). It was last-wins, which was
  unobservable until the dedup change above made duplicate `full_ref` entries reachable — at which
  point `push <ref>` would have uploaded an image that `run`/`rmi`/`inspect` never touch.
- **`registry_catalog` honours an explicit base URL.** It hardcoded `https://` + host, so a client
  built with `registry_client_with_base_url("http://localhost:5000")` — a configuration this port
  promotes to production, where the oracle gates it behind `#[cfg(test)]` — sent a TLS ClientHello
  into a plaintext registry.

### Tests — 1656 → 1739
`tests/registry.tcyr` 282 → 326, `tests/mgmt.tcyr` 180 → 219.

Facade tests deliberately live in the **26-module** `tests/mgmt.tcyr`, whose include set is
byte-identical to `src/main.cyr`'s. The cycc struct-id miscompile is per-compilation-unit, so the
pull/push drivers being green in the 6-module `tests/registry.tcyr` proved nothing about the shape
the shipped binary actually uses. That gap was flagged in the 3.0.9 review; this closes it.

Adversarial review found **six surviving mutations** — assertions that would stay green with the
behaviour broken. All six are now caught, verified by re-running each mutation:
`stiva_pull` ignoring its own argument; `stiva_list_tags` querying a third party's repository;
`registry_verify_signature` stopping after the first referrer (a signed image whose SBOM is listed
first reads as unsigned); a 3xx accepted as success; an empty body becoming "the repository exists
with no tags"; and discovery requesting `push,pull` scope for a read. The first needed a *second*
pull of a **different** reference — asserting the URL of the only pull in the test could not
distinguish a threaded argument from a hardcoded one.

## [3.0.9] — 2026-07-25 — §B increments 3–8: pull and push, end to end · toolchain 6.4.78

### Security — a registry-controlled digest was an unvalidated filesystem path
Found by adversarial review of Inc-7/Inc-8, and probe-confirmed before fixing. `digest_hex`
stripped `sha256:` and returned the remainder verbatim; `_img_blob_path` concatenated it onto
`{root}/blobs/sha256/`; `descriptor_from_jv` required only that `digest` be a JSON *string*. Inc-7
is the first path that routes **registry-controlled** digests there, which made a manifest
declaring `"digest": "sha256:../../../../etc/shadow"` into:

- **An arbitrary-file read.** `image_store_has_blob` reported the traversal target present, so
  `_il_pull_blob` returned success **without downloading and without any digest verification** —
  `registry_fetch_blob_to_store`, which does all the checking, was never reached. The pull then
  *succeeded* and committed the image to `index.json`. A subsequent `stiva push` uploaded that
  file to the target registry.
- **An arbitrary-file truncate.** With the traversal target absent, the `.dl` scratch path was
  opened `O_WRONLY|O_CREAT|O_TRUNC`, filled with attacker bytes, and unlinked.

Fixed at both ends: `digest_is_valid` (exactly `sha256:` + 64 **lowercase** hex — accepting both
cases would give one blob two addressable paths) now gates `descriptor_from_jv`, and
`_img_blob_path` fails **closed**, returning 0 for anything malformed. Every caller was audited to
treat 0 as "no such blob".

Two more holes closed in the same pass:

- **An off-origin upload `Location` handed over the registry credentials.** `_reg_resolve_location`
  honours an absolute URL, so a hostile registry could answer the upload POST with
  `Location: https://evil.example/x`. Withholding the Bearer token there is *not* sufficient: the
  body still goes to the attacker, whose 401 challenge then drives `_reg_acquire_token` to
  authenticate against **its** realm — sending the configured registry credentials, long-lived
  under Basic auth, to a host the attacker chose. Probe measured 2× body amplification plus the
  credential leak. Off-origin upload targets are now refused outright, before any body is sent, at
  session-open *and* on the per-chunk `Location` update, with the re-auth branch gated as well.
- **A caller-pinned digest was never enforced.** The received manifest bytes were only ever
  compared against `Docker-Content-Digest` — a header the registry *chooses* whether to send.
  A reference pinned by digest (`repo@sha256:…`) now outranks it, so digest pinning, the exact
  control you reach for against a compromised registry or a mutated tag, is no longer decorative.

### Fixed — three defects that predate this release
- **`oci_manifest_from_jv` silently dropped unparseable layers.** `if (ld != 0) { vec_push(…) }`
  meant a manifest with a bad layer descriptor produced an image *quietly missing a filesystem
  layer* — a container that runs and is simply wrong, which is worse than a failed pull. Serde
  fails the whole deserialization on a bad element, so failing the manifest is also parity. Found
  only because the traversal regression test refused to go red.
- **A 60 s deadline capped every blob transfer.** sandhi's `total_ms` is a deadline for the
  *entire* stream, not an idle timeout. The 60 s control-plane budget therefore capped every layer
  at whatever fits in a minute: a 400 MB layer on a 50 Mbit/s link needs ~64 s and would fail —
  permanently and identically on every retry, since there is no resume. Blob legs now get their
  own budget.
- **A descriptor's `size` could switch the download ceiling off.** A negative size skipped the
  `expected_size > 0` guard entirely, and `i64max + 65536` wraps negative — either way unbounded.
  `descriptor_from_jv` now rejects negative and absurd sizes.

### Added — roadmap §B increments 7–8: pull and push
- **Inc-7 — `image_store_pull`** in `imagelayout.cyr` (include order puts `registry.cyr` first, so
  the driver has to sit downstream). Resolve → store the manifest → config → layers → `Image`
  record → `add_to_index`. Three deliberate divergences:
  - **The manifest is stored under the digest the registry served, as received.** The oracle
    re-serializes it with `to_vec_pretty` and stores *that* under its re-encoded digest
    (`rust-old/src/image.rs:150-152`), so `index.json` points at bytes the registry never served
    and `Docker-Content-Digest` can never be re-checked from disk — not a valid OCI image layout.
  - **Foreign layers are refused, not fetched.** The oracle follows `descriptor.urls` to an
    arbitrary external host (`image.rs:186-190`): an SSRF primitive on registry-controlled data,
    and the request has already happened by the time a digest check could help.
  - **Layers download sequentially** (the oracle uses `buffer_unordered(4)`); the runtime is
    single-threaded run-to-completion, so parallel fetch stays v3.1.

  A failed layer aborts *without* touching `index.json`, so a half-pulled image is never listable
  or runnable; landed blobs stay content-addressed for the next attempt or for `gc`.
- **Inc-8 — push.** Transport primitives in `registry.cyr`, driver `image_store_push` in
  `imagelayout.cyr`. Config → layers → manifest last, because a manifest may only be PUT once
  everything it references is present. Body-carrying legs deliberately bypass
  `_reg_authenticated_request`: that state machine replays its request up to three times, which is
  fine for a HEAD and catastrophic for a layer upload. Beyond the security fixes above, three
  oracle defects corrected:
  - `registry_blob_exists` uses the **push** scope; the oracle probes with `:pull`
    (`registry.rs:555`), minting a token under a cache key the upload legs cannot use.
  - **One re-auth retry** on a 401 from a body-carrying leg, minted from the challenge that 401
    itself carries. The oracle never retries, and the token TTL is 270 s — a large layer outlasts it.
  - `push_manifest` **requires its auth probe to succeed**; the oracle discards the result
    (`let _ = …`) and PUTs anyway, turning an auth failure into an unauthenticated PUT.
  - The manifest PUT targets the **tag**, not the digest. `_reg_manifest_url` prefers the digest,
    which is right for a fetch and wrong here: PUTting to `/manifests/sha256:…` is spec-legal but
    creates no tag, so the push would produce a repository nothing can pull by name.

`pull` and `push` are **library-complete but not yet wired to the CLI** — that is Inc-10.

### Changed — cyrius pin 6.4.77 → 6.4.78
Purely a compiler bump: `cyrius lib sync` produced a byte-identical `lib/`, and `cyrius.lock` is
unchanged, so 6.4.78 vendors the same stdlib. Full suite green on it; the pin-drift build warning
is gone.

### Tests — 1459 → 1656
`tests/registry.tcyr` 85 → 282. The canned transport gained a download hook delivering bodies in
**three chunks** (a single-shot delivery cannot distinguish a correct streaming hash from one that
only hashed the last buffer), mirrors sandhi's three-way sink contract exactly, and is token-aware
so one URL can answer 401 then 200 across a re-auth.

Two tests were rewritten after mutation testing showed them **vacuous** — the review neutered the
foreign-layer guard and the push layer loop and the suite stayed green. The SSRF test is now
*differential*: two byte-identical manifests differing only in the `urls` key, with the layer fully
routed and correctly hashed, so the guard is the only possible reason to fail. The push test now
asserts per-digest that each blob was uploaded, since a push that skipped every layer still ended
with a manifest PUT and still looked like success. Both mutations now fail, verified.

Every short fixture digest across the suite was normalized to a well-formed 64-hex form, since
`descriptor_from_jv` now rejects anything else.

### Added — roadmap §B increments 3–6: the registry client can now authenticate, resolve a manifest, and stream a layer

- **Inc-3 — transport seam.** `RegTransportOff` (SEND@0/SINK@8/CTX@16) with `_reg_send` /
  `_reg_download` dispatching to sandhi or to an injected transport, plus a
  `registry_last_error()` / `registry_last_status()` channel (the pointer-return idiom has no
  payload slot for a status code). `_reg_http_opts` overrides two sandhi defaults that are simply
  wrong for a registry: `follow_redirects = 0` (registries 307 to CDNs) and a 256 KiB response cap
  (multi-arch indexes exceed it). `_reg_headers` fails **closed** on a CR/LF-bearing value —
  a registry-supplied token reaches `Authorization`, so header smuggling is reachable by hostile
  input.
- **Inc-4 — bearer state machine.** `_reg_acquire_token` + `_reg_authenticated_request`'s four
  phases: cached-token attempt → unauthenticated probe → challenge + token → retry once, second
  401 terminal. The canned transport makes the oracle's wiremock cases portable to a suite that
  never opens a socket, with a request log so tests assert what was actually **sent**: a valid
  cached token costs exactly 1 request, a stale one costs 4.
- **Inc-5 — manifest fetch + platform resolve.** `registry_fetch_manifest` verifies
  `Docker-Content-Digest` against the received bytes *before* parsing, then classifies
  manifest-vs-index, reporting which via `registry_last_manifest_kind()`.
  **Divergence from the oracle:** with no `Content-Type`, the oracle parses an index *as* a
  manifest and produces an image with zero layers — a pull that fails silently; we fall back to
  the body shape. `registry_resolve_manifest` selects the `current_platform()` entry and re-fetches
  **pinned to that child digest**, refusing a nested index rather than recursing.
- **Inc-6 — streaming blob fetch.** `registry_fetch_blob_to_store` streams a blob to
  `blobs/sha256/<hex>.dl`, hashing as it lands, so resident memory is one read buffer rather than
  one layer — this is what sandhi 1.9.3 unblocked, and it replaces the buffered-with-a-ceiling
  design the original §B brief was forced into. The SHA-256 is verified **before** the `rename`
  that publishes the blob at its content-addressed path, so a corrupt blob is never visible to
  `has_blob` / GC roots / `verify_integrity`. Stronger than the oracle, which buffers the whole
  blob and defers the check to `store_blob`. The descriptor's `size` is treated as a **ceiling
  only, never as truth** — it is attacker-controlled; the digest is the authority.

  Auth on the streaming path is the awkward part, and worth recording: sandhi's download driver
  reports a non-2xx as status + err and **never surfaces that hop's response headers**, so
  `WWW-Authenticate` is unreachable there. The token has to be acquired on the buffered path (a
  `HEAD` elicits the challenge), and that probe fires **only when the token cache is cold** — in a
  real pull the manifest fetch already primed the same `registry|scope` key, so a layer costs one
  request. Probing unconditionally would double the request count of every layer in the image. A
  stale cached token arrives as a bare 401 from the stream, which re-runs the buffered machine and
  retries the stream **once**.

> **cycc landmine found in Inc-4, and it retracts v3.0.8's "appears fixed" note.** An `ImageRef`
> obtained from a **wrapper function** reads back as garbage in the 6-module `tests/registry.tcyr`
> unit under 6.4.77: `image_ref_new` writes `"reg.test"`, the wrapper's caller sees `0@!Z`. It did
> not crash — it silently built a junk token-cache key, so every request re-authenticated. It was
> caught only because a test rebuilt the key from a literal and compared. Construct refs inline, or
> as a direct call assigned to a local; never through a helper.

## [3.0.8] — 2026-07-24 — §B foundations · toolchain 6.4.77 unblocks the registry client

### Changed — cyrius pin 6.4.76 → 6.4.77, which finally lands the sandhi DNS + download fixes
The 6.4.77 snapshot vendors **sandhi 1.9.3**, carrying two fixes stiva filed while building §B.
Both were hard blockers for a real registry pull, and neither was reachable from stiva until a
toolchain release re-vendored `lib/sandhi.cyr`:

- **DNS could not follow CNAME chains** (sandhi 1.9.2). The resolver accepted an answer record
  only when its owner name equalled the question name, so every CNAME-led answer was discarded
  and the host did not resolve *at all* — `SANDHI_ERR_DISCOVERY`, indistinguishable from "no such
  host". Measured from stiva's own vendored bundle, before → after: `auth.docker.io`
  **FAIL → 172.64.144.78**, `public.ecr.aws` **FAIL → 75.2.101.78**, `mcr.microsoft.com`
  **FAIL → 150.171.69.10**; `registry-1.docker.io` / `ghcr.io` / `quay.io` unchanged. Note the
  shape of the Docker Hub break: the *registry* host resolved while its **auth realm** did not,
  so an authenticated pull would have died at the token step with a discovery error.
- **The streaming download API could not send request headers** (sandhi 1.9.3).
  `sandhi_http_download_sink_a` hardcoded `headers = 0`, so the one API that does *not* buffer the
  whole body could not send `Authorization: Bearer …` — useless for authenticated blob fetches.
  New `sandhi_http_download_headers` / `_sink_headers` variants fix it, purely additively.

  This **changes §B's plan**: the increment brief assumed layers would come down the *buffered*
  path behind a descriptor-derived cap and a 256 MiB refuse-loudly ceiling, precisely because
  streaming could not authenticate. Inc-6 (blob fetch) can now stream a layer straight to disk
  with bounded resident memory, which is both correct and what a runtime should do.

Verified from stiva rather than assumed: a probe built in stiva's own context resolves all five
registry hosts through the vendored bundle, and the 1.9.3 download entry points link.

> **Toolchain-bump sequence, learned the hard way — `cyrius deps` alone is NOT enough.**
> After bumping the pin, `cyrius deps` reported "10 deps resolved / 83 locked" with a clean
> `--verify`, and `lib/sandhi.cyr` was **still 1.9.1** with zero references to either fix. Only
> `cyrius lib sync` refreshes the vendored `[deps].stdlib` subset. This is the exact inverse of
> the 3.0.7 bump, where `lib sync` alone left the lockfile at 56 entries with `--verify` failing
> and `cyrius deps` was the authority. **Both are required: `cyrius lib sync` then `cyrius deps`.**
> The dangerous half is this one — it looks entirely clean while silently keeping stale
> dependency code.

### Added — roadmap §B increments 0–2: the registry client's foundations
The first three increments of the blocking registry client (`src/registry.cyr`, +364 lines; new
`tests/registry.tcyr`, **85 assertions**):

- **Inc-0 — index/platform JSON.** `platform_from_jv` / `platform_manifest_from_jv` /
  `oci_index_from_jv` / `_reg_body_is_index`. Fixed a latent bug found while writing them:
  `oci_manifest_from_jv` read `schemaVersion` unguarded, so a JSON **string** `"2"` silently
  became `0` — a manifest from a registry that serializes it as a string would have parsed as
  schema-version zero and been mis-dispatched.
- **Inc-1 — client construction + URL builders.** All `/v2/` path builders, auth scopes, `Accept`
  header sets, `Location`/digest-query helpers, and `api_bases` mirror-fallback parity with the
  oracle.
- **Inc-2 — token cache** with an injectable clock (the `_at` seam) so expiry is testable without
  sleeping. Carries one **mandatory divergence** from the oracle: the cache key joins scope parts
  with `'|'`, not NUL. Cyrius maps are cstr-keyed, so a NUL separator truncates the key at the
  first part — every scope would collide under one entry, silently serving a *pull* token for a
  *push* request and 401-ing every push.

`tests/registry.tcyr` deliberately mirrors `store.tcyr`'s exact 6-module include set, and §B adds
**zero new structs** (offset-accessor enums throughout) so it provably cannot perturb the cycc
struct-id assignment that the still-open 20/21 miscompile keys on.

### Note — the cycc struct-id 20/21 miscompile appears fixed at 6.4.77
> **RETRACTED after this release.** During §B Inc-4, in the *same* 6-module
> `tests/registry.tcyr` unit this probe certified, an `ImageRef` obtained from a wrapper
> function read back as garbage — silently, producing a wrong cache key rather than a crash.
> The bug is context-dependent per **function**, not merely per unit, and the probe below
> certified only the expression it ran. See CLAUDE.md for the full write-up. The workarounds
> stay.
The probe that segfaulted the 6-module `tests/store.tcyr` unit at 6.4.76 — a typed
`var im: Image = p; im.id` against raw-offset ground truth — is now **green in every unit
shape**: `store` (6-module), `registry` (6-module), `runpath` (26-module), `stiva` (25-module),
plus a clean `main.cyr` build.

**The raw-offset workarounds are deliberately left in place for this release.** 6.4.76 also
looked fixed by the probe that mattered then, the accessors were retired on that basis, and the
suite SIGSEGV'd in `image_store_save_archive` — a passing probe is necessary but not sufficient.
Retiring them touches hot paths and earns its own increment with the conversion actually
attempted end-to-end. New code is free to use `x.field` (§B and §C already do, green throughout).

Suite **1374 → 1459**. Remaining §B: Inc-3 transport seam, Inc-4 bearer state machine, Inc-5
manifest fetch/resolve, Inc-6 blob fetch (now streaming), Inc-7 the `image_store_pull` driver,
Inc-8 push, Inc-9 discovery, Inc-10 facade + CLI + docs.

## [3.0.7] — 2026-07-24 — §C: ContainerManager + Stiva facade · toolchain 6.4.76

Completes roadmap **§C** — the stateful container manager and the top-level facade, with the
live CLI routed through the manager — on top of a toolchain bump to cyrius 6.4.76.

### Added — roadmap §C: ContainerManager + Stiva facade
The stateful lifecycle manager and the top-level `Stiva` facade land in the DEFERRED blocks of
`src/container.cyr` and `src/stiva_core.cyr` (no new module). Single-node run-to-completion, so
the oracle's `Arc<RwLock<HashMap>>` collapses to a plain `vec<Container*>` + a cstr-keyed
`internals` map — `container_state_save`/`_load` are reused verbatim and `state.json` round-trips
byte-for-byte.

- **ContainerManager** — `container_manager_new` + create/start(one-shot)/stop/wait/try_wait/
  list/get/remove/rename/signal/pause/unpause/stats/logs/log_tail/get_rootfs/restart/update, plus
  lifecycle events over majra `pubsub_*` (stiva's first pub/sub consumer, topic
  `"container.lifecycle"`). `Image`/`ContainerExecResult` are read by **raw offset**, respecting
  the still-open cycc struct-id 20/21 miscompile.
- **Stiva facade** — `stiva_new`/`with_registry` wire the image store, manager, and (when
  configured) the audit log via the already-ported `stiva_audit_log_new`; the lifecycle verbs
  delegate to the manager, auditing stop/rm/signal exactly as the oracle does (run/ps/etc. are not
  audited). **Divergence:** facade `run` resolves the image from the LOCAL store — the oracle
  pulls from a registry first, but the blocking client is roadmap §B.
- **CLI routed through the manager** — the 12 live container verbs (run/ps/stop/rm/inspect/pause/
  unpause/stats/logs/wait/export) now go through the ContainerManager, retiring the per-verb
  `container_state_load`/`save` (and the now-dead `_cli_find_container`). `run` gains a real
  CREATED→RUNNING→STOPPED transition with lifecycle events and a UUID id (`ps`/`stop`/`rm` still
  resolve by name). `stop` is now idempotent on an already-stopped one-shot container instead of
  erroring. The 9 image/convert verbs are untouched; `prune` keeps its image-reference-aware GC.

Detached `run -d`, daemon `wait`/`exec`, and CRIU `checkpoint`/`restore` stay v3.1 (each gated on
kavach `sandbox_spawn` / `sandbox_exec` / CRIU). **67 new assertions** (mgmt 128→180, runpath
187→217); `tests/stiva.tcyr` + `tests/mgmt.tcyr` gained `imagelayout.cyr` (now 26-module units)
so the facade's image resolution links. Suite **1374**.

### Changed — cyrius pin 6.4.72 → 6.4.76
The bump fixes `cyrius audit`'s tests stage (it had reported bogus compile errors for all five
units) and — more importantly — **raises the compiler caps**: `fn_table` 8192 → 32768 (now ~23%
used) and the identifier buffer 262144 → 524288 (now ~46%). The 90%/92% pressure that would have
blocked the `accel` feature for §H/§I is gone. `audit`'s **fmt** stage still false-positives
(per-file `cyrius fmt --check` is clean and `fmt -w` rewrites nothing), so per-file `--check`
remains the gate.

### Note — the cycc struct-id 20/21 miscompile is NOT fully fixed by 6.4.76
An attempt to retire the raw-offset workarounds (they were reported fixed) was reverted. A probe
in the 26-module `runpath.tcyr` unit passed and the minimal repro printed `MATCH`, so the
accessors were converted to `x.field` — and the suite then **SIGSEGV'd** in
`image_store_save_archive` under the **6-module `store.tcyr` unit**, where a bare
`var im: Image = p; im.id` crashes (a scalar field compiled as a vector load, out-of-bounds). The
bug is **compilation-unit-shape-dependent**: a green probe in one unit proves nothing about
another. All conversions were reverted; the workarounds stay until a probe is green in *every*
unit shape that includes the struct — the 6-module store unit especially. `CLAUDE.md`'s warning
was rewritten to record this.

## [3.0.6] — 2026-07-23 — §G complete: output scanning · compose YAML · working benchmarks

Closes roadmap **§G** — the three items the 2026-07-22 upstream re-check graduated out of
v3.1. zstd decode landed in 3.0.5; `scan_output` and `convert compose` land here.

### Added — `scan_output` + `stiva logs --scan` (roadmap §G)
`src/runtime.cyr` grows `scan_output(result, policy)`: it marshals a `ContainerExecResult`
into a kavach `ExecResult`, applies the externalization gate, and maps the verdict —
PASS/WARN return a new result carrying the possibly-redacted strings, QUARANTINE/BLOCK
return `0` with `STIVA_ERR_SANDBOX`. That collapse matches the oracle
(`rust-old/src/runtime.rs:565-604`), whose `gate.apply(...)?` yields a single
`Err(StivaError::Sandbox)` either way; `scan_output_last_verdict()` /
`scan_output_last_findings()` expose what the collapse hides.

Wired as **`stiva logs <ID> --scan`**. Opt-in, not default: scanning changes what `logs`
prints, and that should not happen behind an operator's back. The oracle instead gated it
on a per-container persisted `scan_policy`, which this port does not yet round-trip through
`state.json`, so the flag stands in for it.

Two things the roadmap had wrong, both corrected by reading the code:

- It advised **calling the three scanners directly** rather than `gate_apply`, because
  `gate_apply` only redacts at WARN. Moot: BLOCK and QUARANTINE become errors regardless, so
  redaction at those levels never mattered. `gate_apply` is the closer analogue of the oracle.
- More importantly, **kavach's `sandbox_exec` already applies the gate itself**
  (`lib/kavach.cyr:9026`) and returns `0` on BLOCK/QUARANTINE. Container exec output has
  therefore always been scanned; adding `scan_output` to `exec_container` would scan twice
  and diverge from the oracle. Confirmed on the binary — a container echoing an AWS key
  never completes: `kavach: externalization blocked: blocked by gate`. `logs`, which reads
  bytes back off disk, is the one correct site. There is also no `ExternalizationGate`
  *type* in kavach's Cyrius surface, only free functions; the deferred-note claiming an
  "unported handle" was wrong and is removed.

15 new assertions in `tests/runpath.tcyr` (the oracle's four cases plus null guards, the
verdict accessor, and stderr-only secrets — `gate_apply` scans `stdout + "\n" + stderr`).

### Added — `convert --format compose` (roadmap §G)
`compose_yaml_to_toml` + `compose_last_error()` in `src/convert.cyr`, over
`bayan_yaml_parse_str`. All four oracle fixtures (`rust-old/src/convert.rs:312-375`)
reproduce byte-for-byte. The `convert` verb already defaulted to `-f compose`, so the
default path stops printing "not yet implemented" and converts.

**Ordering was the one systematic hazard.** `serde_json::Map` is a `BTreeMap` (rust-old
takes no `preserve_order` feature), so the oracle emits services, networks, volumes,
env-maps and depends_on-objects **sorted**; bayan objects are insertion-ordered pair
vectors. `_cv_sorted_idx`, keyed on a byte-wise `_cv_key_lt` matching Rust's `String` Ord,
is applied at all five sites — and deliberately *not* at the document-ordered ones (the
command/ports/volumes/depends_on **array** forms and the env **list** form, all `Vec` in the
oracle). Miss one and every multi-service file diverges; the test file pins both orders.

**The subset is documented, not implied.** bayan rejects flow mappings, block scalars,
anchors/aliases, tags, multi-document input, tab indentation, complex keys, and escaped
quotes inside quoted scalars — each with a verbatim message surfaced through
`compose_last_error()`, plus a byte offset. Merge keys needed a stiva-side guard: bayan has
no `<<` handling at all, so `<<: *base` dies incidentally on the alias check but a literal
`<<: x` would silently become a key named `<<`. Three divergences from the oracle are worth
naming: serde-saphyr *resolved* anchors and merge keys before conversion (the real-world
loss — flatten such files first); it *errored* on duplicate mapping keys where bayan keeps
the first; and it *accepted* escaped quotes and then emitted invalid TOML, so rejecting them
is stricter and more correct. Neither version escapes output.

Assertions cover the oracle cases, both orderings, every per-field arm, the `Ok("")` cases,
one test per rejected construct, and one per review finding (see below).

### Added — `tests/convert.tcyr`, a 5th test unit
`tests/stiva.tcyr` was the most identifier-pressured unit in the repo (94% of cycc's cap); a
unit including only `error.cyr` + `convert.cyr` sits at 86%. The six `dockerfile_to_toml`
tests moved there alongside the new compose ones — assertions unchanged, though the fixtures
are now built a line at a time via a `_cv_yl` helper, since whitespace-significant YAML
embedded in a single `"a\n  b\n"` literal is unreadable.

Suite: **1307 assertions** across five files — stiva 664 · runpath 202 · store 197 ·
mgmt 128 · convert 116.

### Fixed — the benchmark scripts never ran, so no history was ever recorded
`scripts/bench-history.sh` ran `cargo bench --bench benchmarks` and `scripts/bench.sh` ran
`rustc`/`cargo test`/`cargo build` — both pre-port leftovers. `cargo` is not in this
toolchain, so the commands produced nothing, the criterion-format parser (`time: [lo mid hi]`)
matched nothing, and `bench-history.csv` had been header-only since the port. This sat under
CLAUDE.md's "Never skip benchmarks. The CSV history is the proof."

Both are ported to `cyrius bench` / `cyrius tests` / `cyrius build`, with a parser for the
actual output shape (`name: 7.203us avg (min=… max=…) [N iters]`). `bench.sh` now records
assertion and failure counts, binary size, and Cyrius LoC in place of the Rust metrics. Also
fixed a display bug carried in the Python trend generator: `fmt_ns` only stepped up at 1e6 ns
and labelled that µs, so 7 µs rendered as `7203.0 ns` and 1 ms would have read `1000 µs` —
it is a proper ns→µs→ms→s ladder now.

First recorded run (`3808225`): `noop` 2.00 ns · `oci_config_build+serialize` 7.203 µs ·
`oci_manifest_to_jv+serialize` 15.825 µs; suite 22.4 s, clean build 5.7 s → 16,304,096 B,
14,061 LoC.

### Fixed — `stiva convert` silently truncated input at 1 MiB
`_cli_convert` reads into a fixed `alloc(1048576)` and did not check for saturation, so a
larger file was quietly cut short. On the Dockerfile path that yields a plausible-but-wrong
Stivafile; on the new compose path it surfaces as a YAML parse error pointing at a line the
user can see is fine. It now refuses with `input exceeds the 1 MiB convert limit`.

### Fixed — the compose converter, after an adversarial review
The first cut of `compose_yaml_to_toml` was reviewed by a 4-dimension finder pass with
3-lens adversarial verification; 16 findings survived, several of them serious. One reviewer
compiled the frozen Rust oracle and diffed its output against the binary's, which is what
caught the parity breaks. Everything below is fixed and has a regression test.

- **YAML 1.1 boolean tokens broke parity on ordinary files.** `serde-saphyr`, the oracle's
  parser, *resolves* `yes/no/on/off/y/n/True/TRUE/False/FALSE/Null/NULL`; bayan implements
  the YAML 1.2 core schema and leaves them as strings. So every `.as_str()` guard flipped:
  `restart: no` — docker-compose's own documented default, routinely written unquoted —
  emitted `restart = "no"` where the oracle emits **nothing**, and `ports: [yes, "80:80"]`
  kept an element the oracle's `filter_map(as_str)` drops. The tokens are now resolved at the
  guards (`_cv_yaml11_token` / `_cv_is_yaml_str`), and render as the canonical `true`/`false`/
  `null` in the env `to_string()` fallback. **The roadmap asserted this was parity** ("bayan
  treats `true`/`false` only; `yes`/`no`/`on`/`off` stay strings — serde-saphyr agrees, YAML
  1.2 core"). It was not; the claim came from reading bayan without running the oracle.
- **Duplicate mapping keys produced TOML no parser will load.** `serde-saphyr` rejects the
  document. bayan keeps both pairs and `obj_get` returns the first — which reads like a
  harmless superset until you notice the emitter *iterates* these maps, so both survive:
  two `[services.a]` tables, or `env = { A = "1", A = "2" }`, handed back with exit 0.
  `_cv_validate_doc` now walks the whole document once and rejects duplicates the way the
  oracle does. The same pass subsumes the merge-key guard, which previously ran only at the
  root and on service bodies — so `<<:` errored in one place and became a literal `<<` key
  one level down.
- **The key sort was quadratic on attacker-chosen input.** `_cv_sorted_idx` was an insertion
  sort: 1 MiB of reverse-ordered service keys took **236 s of pinned CPU** against 0.09 s for
  the identical bytes ascending — a ~2000x amplification on a path that exists to ingest
  third-party files, with no output until it finished. It is a bottom-up merge sort now.
  Measured after: 43k reversed keys and 43k ascending keys both convert in 0.17 s.
- **Output is now escaped — a deliberate divergence from the oracle.** rust-old emits every
  value and section name through a bare `format!`, so a hostile compose file could close the
  string it was written into and open a new table: what a human reads in the compose source
  and what stiva's own `parse_ansamblu` later loads were different documents. Demonstrated
  end-to-end by the review. Values now go through TOML basic-string escaping and keys are
  quoted unless they are bare-safe. Inheriting the oracle here would have meant inheriting a
  structure-forgery hole, so parity loses to correctness on this one and it is recorded as an
  accepted divergence.
- **Nested object values in `environment` rendered in the wrong order.** serde_json's `Map`
  is a `BTreeMap` at *every* depth, so `v.to_string()` sorts recursively;
  `bayan_json_v_build` walks insertion order. The fallback goes through a new
  `_cv_value_to_string` that applies the same sort recursively. (The earlier claim that
  `bayan_json_v_build` is "the byte-exact analogue of `Value::to_string()`" holds for scalars
  and arrays only.)
- **An ordering test could false-pass.** `assert(strstr(h, a) < strstr(h, b))` goes green
  when `a` is *absent*, because this `strstr` returns −1 and −1 beats any real index — so a
  regression that dropped the earlier key entirely would not have been caught. All seven
  ordering assertions now go through `_cv_before`, which requires both substrings present.

Two bayan-level gaps remain, documented rather than worked around: an integer above
`i64::MAX` is a `u64` in serde_json and round-trips exactly but wraps negative through
bayan's `JTAG_INT`; and non-decimal / underscored / leading-zero numeric spellings (`0x1f`,
`1_000`, `007`) stay strings, which additionally *un-drops* array elements the oracle
discards. Both belong in bayan's scalar resolver.

### Fixed — `stiva convert` input handling
- **A directory succeeded with empty output.** A directory opens and reads 0 bytes, so
  `file_read_all` returned 0 rather than an error and `convert <dir> -o out.toml` wrote an
  empty file and exited 0. The oracle's `fs::read_to_string` surfaces `EISDIR`. Note
  `is_dir` takes a `Str`, not a cstr — passing the bare cstr compiles and silently never
  matches, which is how the first attempt at this fix did nothing.
- **The 1 MiB guard rejected a complete 1048575-byte file.** Reading exactly `limit` bytes
  cannot distinguish "fit exactly" from "there was more", so the read now goes one byte past
  the limit and rejects only on `n > limit`.

### Fixed — `scripts/bench.sh` recorded a broken tree as a clean run
A test unit that fails to *compile* emits no `N passed` line, so the sum silently shrank
while the failure count stayed 0 — a broken tree would be logged as a clean run with fewer
tests, corrupting the one number the file exists to track. It now refuses to record unless
every `tests/*.tcyr` reported a result, and fails on a non-zero build exit instead of
timing it.

### Added — benchmarks for the new paths
`compose_yaml_to_toml` **28.944 µs** on a realistic 3-service document, and
`compose_400_reverse_sorted_keys` **1.383 ms** — the second exists specifically to catch a
regression back to a quadratic sort. Suite now **1307 assertions** (convert 116 · stiva 664 ·
runpath 202 · store 197 · mgmt 128).

### Changed — cyrius pin 6.4.71 → 6.4.72
Every build was warning `cyrius.cyml pins 6.4.71 but cycc is 6.4.72`. The delta is two
vendored stdlib files (`syscalls_x86_64_agnos.cyr`, `fs.cyr`); `cyrius.lock` stays at 83
entries = 83 `lib/*.cyr` and `cyrius deps --verify` is clean.

Worth recording: `cyrius lib sync` alone is **not** sufficient after a pin bump. It copied 69
files against a `lib/` of 83 and left the lockfile at 56 entries with `--verify` failing on
46 of them. `cyrius deps` is the authority that restores the invariant — the same
"`lib/` is reproducible from the manifest + lockfile" property the 3.0.5 hygiene pass fixed.

## [3.0.5] — 2026-07-23 — zstd layer decode · dependency hygiene · toolchain 6.4.71

### Added — zstd layer decode (roadmap §G)
`unpack_layer` (`src/storage.cyr`) now handles `tar+zstd` layers alongside `tar+gzip`,
dispatching on the 4-byte zstd magic (`28 B5 2F FD`) into a new `_stor_unpack_zstd_bytes`.
This completes the compressed-layer media types the OCI image-spec defines on the unpack
side, and lights up `media_oci_layer_zstd` (`src/registry.cyr:57`) for the pull path.

Two design points worth recording:
- **Sized from the frame header, not a grow-retry loop.** `zstd_decompress` returns `-1` for
  *both* a short output buffer and a corrupt frame, so the two cannot be distinguished by
  return code and the gzip path's `ERR_BUFFER_TOO_SMALL`-driven retry has no equivalent. The
  buffer is sized from `zstd_frame_content_size`, with bounded retry only when the frame
  omits the size.
- **The declared size is attacker-controlled**, so it is bounded by an **8 GiB absolute** and
  **1000:1 ratio** ceiling. Without this, a 100-byte blob declaring a 1 TiB content size is
  an allocation-amplification DoS. The gzip path needs no such guard — its grow-loop reacts
  to actual decoded output rather than a declared number.

12 new assertions in `tests/store.tcyr` (real round-trip, corrupt-frame rejection, and both
DoS ceilings). Suite now **1196 assertions**: stiva 684 · store 197 · runpath 187 · mgmt 128.

### Changed — cyrius pin 6.4.66 → 6.4.71 (closes a heap overflow in the vendored zstd decoder)
The zstd work surfaced a **heap buffer overflow in sankoch 2.5.5**, the version vendored by
the cyrius 6.4.66 snapshot stiva was pinned to. Every output write in its zstd decoder stored
to `_z_out + _z_outpos` with no per-write bound, checking `_z_outpos > _z_outcap` only *after*
a whole block had been written — with the block size taken from the attacker-controlled frame
header. A canary proved it: a 32-byte frame (valid magic + `0x41` filler) declaring FCS 16961
returned `-1` **and clobbered all 4096 canary bytes** past the buffer. Reachable from
`stiva load` and layer unpack on exactly the untrusted input a runtime must survive; it is
also what silently corrupted unrelated tar/symlink/docker-archive tests when a
malformed-input test was first added.

sankoch had **already fixed this in 2.5.6** (`8b843d6`, 2026-07-19) — bounds checks now
precede every write. stiva was pinned one toolchain release below the fix. Bumping to
**6.4.71** (sankoch 2.7.5) closes it and also clears the wrapper/manifest drift warning that
had been present all along. `[deps.kavach]` → **3.8.3** (samay/ai-hwaccel now optional
upstream). The same canary passes clean against 2.7.5.

Note for future stdlib issues: stiva consumes sankoch from `~/.cyrius/versions/<pin>/lib/`,
**not** from the sibling repo — a stdlib security fix reaches stiva only via a pin bump, so
check the snapshot's vendored version rather than the sibling's.

### Fixed — `stiva --version` did not track the `VERSION` file
`cyrius.cyml` reads `version = "${file:VERSION}"`, but nothing interpolates that into the
compiled binary: `src/main.cyr`'s `_stiva_version_str()` is a hand-written literal. Caught
while cutting this release — a tree already bumped to 3.0.5 still built a binary reporting
`stiva 3.0.4`, meaning every prior release's `--version` was only right by hand. Fixed, and
made unrepeatable: `scripts/version-bump.sh` now rewrites VERSION, the `src/main.cyr`
literal, **and** re-runs `cyrius distlib` to restamp the `dist/stiva.cyr` bundle header, then
prints the remaining manual steps (CHANGELOG, README/CLAUDE.md, the zugot recipe). It
validates semver and is idempotent.

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
manifest + lockfile. `[deps.kavach]` now pins **3.8.3** (which adds the `optional` gating
itself), and the stdlib subset was re-synced, clearing the `sankoch … shadow` warning.

### Added — `tests/store.tcyr` (4th test unit)
Activating the two bundles pushes `tests/stiva.tcyr` past cycc's identifier cap — a hard
`identifier buffer full (261893/262144)` error. The imagelayout + storage tests (39 functions,
185 assertions) now live in `tests/store.tcyr`, which includes only the 6 `src/` modules they
need instead of all 26. No assertions were lost in the split; with the zstd tests above the
suite now stands at **1196** across four files: stiva 684 · store 197 · runpath 187 · mgmt 128.
Note the peel alone did not move the cap
(identifiers dedupe; test bodies only reference symbols the modules already define) — the
buffer is dominated by the vendored `lib/` bundles, which is why the feature gate is the fix.

## [3.0.4] — 2026-07-18 — group A complete

### Added — group A tails
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

## [3.0.3] — 2026-07-18 — docker-archive read · tar hardening

### Added — tar hardening + docker-archive (group A follow-ups)
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

## [3.0.2] — 2026-07-18 — oci-archive save/load · perms-preserving tar

### Added — perms-tar + oci-archive transfer (group A, A3+A4)
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

## [3.0.1] — 2026-07-18 — OCI image-layout store · sync backlog · toolchain 6.4.66

### Added — OCI image-layout (group A, A1+A2)
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

## [2.1.0] — Rust era · completed, never tagged

> Finished on the Rust crate (now the frozen `rust-old/` oracle) but never cut as its
> own release — this work was carried into the Cyrius port and shipped as part of
> **3.0.0**. Kept verbatim for provenance; the file paths below are `rust-old/` paths.

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
