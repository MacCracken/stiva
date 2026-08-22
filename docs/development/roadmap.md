# Roadmap

**Completed work is not kept here.** The CHANGELOG carries what shipped and why; git carries the
rest. This file is only what is still to do.

**Every item belongs to a release, and nothing is parked for being hard.** Where an item needs
something from another repo, that prerequisite is written down as its own work item with a named
symbol — not as a reason to wait.

**Every deferral marker in the source is listed here.** `cyrius lint` reports untracked deferrals
per file; the count is expected to reach zero, either because the work landed or because the comment
was stale and got deleted. If you add a `TODO` / `deferred` / `not yet` to the source, add it here
in the same change.

---

## Where stiva is — v3.0.18

A working single-node OCI runtime in Cyrius, ported from the frozen Rust oracle at `rust-old/`.

| | |
|---|---|
| CLI | **34 of 36 verbs live** (`cron` is new); only `checkpoint` and `restore` are unwired (v3.1.0 item 3) |
| Image | pull · push · build · import · export · save · load · tag · rmi · gc · prune, over a valid OCI image layout |
| Container | run · run -d · exec · diff · ps · stop · kill · restart · rename · pause · unpause · logs · logs -f · events · wait · top · cp · stats · inspect |
| Tests | 2189 across `tests/*.tcyr` · 87 CLI smoke assertions · 14 benchmarks |
| Deps | cyrius 6.5.33 · kavach 3.12.1 · cmdit 1.2.2 · majra 2.6.7 · nein 1.6.10 · bote 3.3.2 · agnodrm 1.5.1 · sigil 3.12.9 · sakshi 2.4.11 · libro 2.8.8 · samay 1.0.1 · ai-hwaccel 2.3.18 |

**Three facts that constrain everything below**, all verified by execution rather than by reading:

- **The cycc struct-id 20/21 ↔ SIMD-sentinel miscompile was last verified live at 6.4.78;
  it has not been re-verified at the 6.5.33 pin, so assume live.** A typed
  `var x: T = p; x.field` can silently read garbage — not crash — and it is per-function AND
  per-compilation-unit. The raw-offset accessors (`_img_id` / `_img_layers` /
  `_img_manifest_digest` / `_layer_digest`) are deliberate workarounds. Retiring them needs a probe
  green in *every* unit shape including the 6-module `store.tcyr`, and an assertion that fails on a
  silent wrong value rather than only on a segfault. Full write-up:
  [`docs/architecture/001-cycc-struct-id-miscompile.md`](../architecture/001-cycc-struct-id-miscompile.md).
- **Only x86_64 works.** See v3.2.0.
- **Containers have no secrets.** `secrets` still serializes as an empty array and
  `build_sandbox` has no kavach setter to thread one through — v3.1.0 item 1. (Rootless networking
  and `scan_policy` persistence, the two siblings found in the same sweep, shipped in v3.0.17.)
- **Three `RuntimeSpec` fields were write-only, in the `RUNTIME_NS_PID` shape.** `env` is FIXED
  (kavach 3.12.0's `config_env`; see the CHANGELOG). `namespaces` is v3.1.0 item 3. **`mounts` is
  not tracked anywhere below and needs to be** — `standard_mounts()` (`src/runtime.cyr:228`) builds
  a `/dev` entry that never reaches a mount. When adding a field to a spec struct, grep for a
  READER before assuming it is wired; assembling a value and delivering it are separate steps and
  this codebase has now lost three of them at the same boundary.

---

## v3.0.17 — finish the single-node runtime — COMPLETE

All of v3.0.17 has landed — see the CHANGELOG. `accel` is now `default = ["accel"]`, `stiva cron`
schedules containers, `stiva info` reports accelerators, fleet placement has an accelerator
dimension, and CI checks the vendored files against the pinned tags.

The release also moved the toolchain **6.4.78 → 6.5.33** and every dependency pin to its current
tag, which forced three source-level adjustments (the bayan `_str` → `_buf` rename, kavach's
`Backend` → `KavachBackend`, and a samay 1.0.1 shim in `src/error.cyr` to retire when samay
≥ 1.0.2 lands). It also surfaced a latent packaging bug in nein that broke `cyrius deps`
outright — fixed upstream in **nein 1.6.6**, pinned here. The CHANGELOG carries the full account;
the short version is that a dep's `.deps` sidecar may name stdlib leaves only, and reverting the
toolchain would not have helped.

### Open cleanliness item — `cyrius audit` exits 1 on 43 undocumented public fns

Every other audit stage is clean at 3.0.17 — `fmt` ok, `lint` ok, tests 2175/0, bench 1/0 — and
`rc = 1` comes solely from the docs stage counting **43 undocumented public fns** across `src/`.
It is not a CI gate (`.github/workflows/ci.yml` runs deps/build/test, not `cyrius audit`), but it
does mean the dev-loop cleanliness check cannot be read as a simple pass/fail until the count is
zero. Pre-existing and unchanged by the 3.0.17 release work.

### Open cleanliness item — 163 line-length warnings in `tests/`

`src/` is fmt-clean and lint-clean at zero warnings. The **test** files are not:
`registry.tcyr` 69 · `stiva.tcyr` 53 · `mgmt.tcyr` 15 · `runpath.tcyr` 13 · `store.tcyr` 11 ·
`stiva.bcyr` 2 — all of them `> 120 bytes`, almost entirely long assertion messages.

It is recorded here rather than swept because the fix is 163 hand-verified line splits under a
formatter rule that is easy to get wrong: **`cyrius fmt --check` requires a continuation line to
sit at the *same* indent as the statement it continues, not one level deeper.** (That rule is
also what made `src/build.cyr` and `tests/registry.tcyr` fail `fmt --check` for several releases
— fixed in this line.) Note the linter counts **bytes, not display columns**, so a line of
box-drawing characters can trip it while looking short.

---

## v3.1.0 — secrets, interactivity, and mobility

> ⛔ **REWRITTEN 2026-08-21 after an adversarial re-audit. The previous version of this section
> claimed eight items were blocked on upstream releases. ZERO of them were.** Seven were never
> blocked; the eighth names a primitive that must be written, not waited for.
>
> The cause was mechanical, not careless. Every claim below was originally written in one commit
> (`44b9f8e`, 2026-07-26) against **cyrius 6.4.78 / kavach 3.9.3**. On 2026-08-21 every pin moved.
> The roadmap edit that accompanied that bump was **one line** — the version inventory. The
> conclusions *derived from* those versions were never re-checked, and four of them had already
> gone stale. One ("`SandboxConfig` is 8 fields / 64 bytes") was **never true at any pin** — it was
> 9 fields / 72 bytes the day it was typed, and stiva was already calling `config_rootfs`.
>
> Every item now carries a `block` stanza (see **Recording a block** at the end of this file).
> No stanza means **not blocked, merely unstarted** — which is what most of these were.

The theme: a container you can hand a credential to, attach to, and move.

### 1. Secret injection and the externalization policy

**Not blocked. Never was.** Ordered work, all stiva-side:

1. **Externalization** — the working lever, `sandbox_exec_set_ext_policy`, shipped in **kavach
   3.9.3**, before this item was written. Persistence (`src/container.cyr:544-580`) and the
   `stiva logs` consumer (`src/main.cyr:652-671`) are built and tested; what is missing is a
   *producer* — nothing in stiva ever sets a non-zero `ContainerConfig.scan_policy`. Add a
   `--scan-policy` family to `_cli_run`, then gate `container_manager_exec` through
   `scan_output`.
   ⛔ Call `sandbox_exec_set_ext_policy`, **NOT** `config_externalization`. The latter exists
   (kavach 3.11.2) and is **decorative**: `SandboxConfig_externalization` has zero readers in
   the bundle, and kavach's own doc says so — *"Carrying the policy is all this does; nothing
   applies it for you."* Wiring it produces no behaviour change.
   ⚠ Foreground `stiva run` output is **already** scanned today under kavach's default policy
   (block=CRITICAL), which stiva never chose. Unverified by execution — see the probe below.
2. **Secrets** — `SecretRef`, `CredentialProxy`, `credential_proxy_env_vars` and
   `credential_inject_files` all exist in kavach. The env channel they needed landed in
   **3.12.0** (`config_env`) and stiva now uses it for image env. What remains is a
   `SandboxConfig.secrets` field, or routing the proxy's env vars through `config_env` —
   **decide which before building.**

### 2. Interactive `exec -it`

**No upstream prerequisite remains.**

- **Coroutines: RESOLVED, and the old entry was stale for four toolchain releases.** cyrius
  triaged stiva's filed issue, **refuted its own premise**, and shipped `async_wait_rw` /
  `async_relay_once` in 6.5.25 — eight releases below the current pin. They are sitting in this
  repo's own `lib/async.cyr`.
- **pty helper: must be written, in stiva.** `openpty` / `posix_openpt` / `TIOCSCTTY` /
  `forkpty` exist nowhere — that part of the original entry is correct. But "most sensibly in
  the cyrius stdlib" is a **placement preference stated as a constraint**: `SYS_IOCTL` is a
  per-arch constant already in `lib/`, `sys_setsid` is in `lib/syscalls_linux_common.cyr`, and
  `exec` already bypasses kavach wholesale (`src/runtime.cyr:1359-1363`). ~200 lines:
  `/dev/ptmx` → `TIOCSPTLCK` → `TIOCGPTN` → `TIOCSCTTY`, raw mode via TCGETS/TCSETS, window
  size via TIOCGWINSZ. Then `-i`/`-t` on `_cli_exec` and swap the child's fds.
  ⛔ Build the relay on `async_wait_fd`/`async_wait_rw`, **not** `async_relay_once` — the
  latter has zero consumers and zero tests anywhere in the cyrius corpus, and used exactly as
  its own doc prescribes (two instances, reversed directions) it **deadlocks**: a blocking
  `sys_read` on an idle-but-open fd stalls the single-threaded reactor. Both fds must be
  `O_NONBLOCK`; the first call returns raw `-EAGAIN`; after child exit the master yields
  `-EIO` forever rather than EOF, so retire the loop explicitly.

### 3. CRIU `checkpoint` / `restore`

**The prerequisite was mis-stated. There is a route today.**

The old entry said kavach must "honour the namespace list" — but there is no namespace list to
honour: neither `SandboxConfig` nor `SandboxPolicy` has one, and `config_require_namespaces` is
a boolean, not a selector. That diagnosis was correct about `NS_PID` having no producer and
**irrelevant**, because stiva does not have to go through kavach to get a namespace:
`clone(CLONE_NEWUSER|CLONE_NEWPID|CLONE_NEWNS|SIGCHLD)` is one syscall whose constants are
already in the tree. Reproduced unprivileged, container init running as pid 1, by two
independent reviewers.

⚠ **"Every stiva container runs in the HOST pid namespace" was false** — one measurement on the
detached path generalised to a universal. The foreground path selects the OCI backend whenever
runc is present (`src/runtime.cyr:836-837`) and kavach's spec emitter always includes a pid
namespace. Only `run -d` lacks one.

Ordered work: (1) fork+clone in stiva's own spawn path, or wire `mounts`/`namespaces` (both
still write-only); (2) assert the container's pid namespace differs from ours, the shape
`_rt_userns_differs` already uses; (3) port `checkpoint`/`pre_dump`/`restore`/`restore_lazy`.
`container_manager_restore` must set `start_ticks` from the new pid or
`container_fixup_after_restart` demotes the restored container to STOPPED on the next load.

⚠ Worth costing first: the runc path already has a pid namespace, and `runc checkpoint` /
`runc restore` already wrap CRIU. Driving those may retire most of step 3.

### 4. §J — device and accelerator passthrough

**Not blocked on kavach or cyrius.** Both halves of the old diagnosis are true as *symbol*
statements — `SandboxPolicy` has no device field, `cgroup_setup` writes no devices controller —
and both are irrelevant:

- `stiva run -d` never reaches `oci_generate_spec`, which is the **only** place `/dev` is masked
  with an empty tmpfs.
- **cgroup v2 has no devices controller by design** — device access is gated by an attached eBPF
  program, and an unattached cgroup is **ALLOW**. "No devices gate" is why a bind mount *works*,
  not why it fails. (The claimed missing `SYS_BPF` is therefore not on the critical path either.)
- Host-side `MS_BIND` is already in the tree at `src/storage.cyr:340`, and a chrooted child
  inherits mounts established before it enters.

Ordered work: `--device` / `--gpus` on `_cli_run` (register verbs **last** — ids are
registration-ordered), validation and persistence, then bind the node into the merged rootfs
before spawn. ai-hwaccel supplies the matcher and `_find_pci_addr`; it emits no device path,
major/minor, cgroup rule or driver mount, and should not be expected to.

### 5. Device allocation ledger

**Blocked only by item 4's false block.** Design is stiva-side and unblocked.

⛔ Do not key it on ai-hwaccel's `_find_pci_addr` device index — it is an **unsorted getdents
ordinal**, not a stable identity, and a ledger keyed on an unstable id is worse than none.
⛔ Do not copy `state.json`'s unlocked tmp+rename (`src/container.cyr:764-772`); use
`file_lock` (`lib/io.cyr:612`), or the ledger reproduces the double-allocation race it exists
to prevent. `src/network_pool.cyr` is stiva's only existing exclusive allocator and has no
persistence at all — there is no working precedent to copy.
⚠ samay's `node_capacity_reserve` decrements CPU, memory and disk but **not** `accel_profiles`,
has zero file I/O, and releases only from `task_scheduler_cancel_task` — so a COMPLETED task
never releases. Do not adopt it.

### 6. `parse_intent` — RETIRE, do not implement

**The premise was false and the dependency arrow points the other way.**

`src/intents.cyr:4` says "The agnoshi project does not exist yet." agnoshi has existed since
**2026-04**, is at **1.8.9**, and ships a 44-tag NL parser. That sentence was inherited verbatim
from the Rust oracle (`rust-old/src/intents.rs:3`), where it was **already false when the oracle
froze**, and survived a whole-language port unread.

The real situation: agnoshi exposes no library surface (no `[lib]`, no `dist/`), has zero
container intents (its 12 `Stiva*` intents existed only in its deleted Rust tree), and its own
historical design **shells out to the `stiva` CLI** — agnoshi depends on stiva, not the reverse.
`parse_intent` has no caller in `src/` at all.

**Recommendation: delete `parse_intent` and keep the `Intent` value type + serde**, which are
complete and green. Confirm with agnoshi's maintainer first, then correct `src/intents.cyr:4`,
`:58-59`, `:269` in the same change.

### 7. Kavach error detail

**Not blocked.** All three accessors this item asks for shipped **before it was written**:
`backend_result_is_diagnostic`, `spawn_exit_name`, and `sandbox_exec_last_scan_result`. stiva
calls none of them.

Work: read the diagnostic channel stiva already receives; pre-flight `backend_is_available`
before `sandbox_create` (its only return-0 path is BACKEND_UNAVAILABLE, and stiva already calls
that probe at `src/runtime.cyr:836`); map `spawn_exit_name` over `sandbox_spawn`'s six return-0
arms — the live `run -d` path the old entry omitted entirely.

### 8. True concurrent layer downloads

**Not blocked, and the stated prerequisite was false twice.** `buffer_unordered(4)` is
concurrency, not parallelism, and real OS threads (`thread_create`/`thread_join`) existed at
**cyrius 6.4.78 — the pin in force the day the claim was typed.** The allocator has been
thread-safe since 6.0.64. A 9-way digest-correct parallel pull was measured (15.0 s vs 22.5 s).

⚠ **The real prerequisite is one the old entry never mentioned, and it is a dependency defect.**
The first concurrent TLS handshake in a cold process fails deterministically and poisons the
process — bisected to ~35 unguarded lazy-init flags in sigil (`_p256_init`, `_onc_init`, and
`_sha_ni_init`, which publishes its pointer before filling it). A single main-thread handshake
before fan-out makes the identical binary go 9/9.

Work: a main-thread TLS warm-up covering **every handshake shape the pull touches** (including
`auth.docker.io`, which `registry_resolve_manifest` does not pre-warm), a mutex on the token
cache (`src/registry.cyr:2377`), digest dedup (two workers otherwise race the same `.dl`
scratch), then a fixed 4-worker pool with layer order taken from the descriptor list.
⛔ Do not move `var ld: Descriptor = vec_get(descs, i)` into a worker fn — that is the
struct-id 20/21 shape and it is unverified at 6.5.33.
File the sigil issue in the same sweep; until it lands the warm-up is load-bearing.

### 9. Wire `RuntimeSpec.mounts` — NEW, and it gates more than it looks

`mounts` is **write-only** (`src/runtime.cyr:539`), the third field found in the
`RUNTIME_NS_PID` shape after `env` (fixed in 3.0.18) and `namespaces`. `standard_mounts()`
builds `/proc` and `/dev` entries that never reach a mount.

⚠ This is why **AGNOS-native payloads still see no environment** after 3.0.18: cyrius's
`getenv` reads `/proc/self/environ`, and `run -d` mounts no `/proc`. glibc and busybox payloads
are fine. daimon and sutra are not. Tracked in the agnos roadmap; the alternative fix is
cyrius-side.


## v3.2.0 — non-x86: aarch64, then AGNOS

### aarch64 — the known blocker is GONE; the target is untested, not broken

⛔ **This entry previously said the aarch64 blocker "cannot be fixed in stiva: the cyrius stdlib
exposes neither a `sys_lchown` wrapper nor a `SYS_LCHOWN`/`SYS_FCHOWNAT` per-arch constant."
That was FALSE, and false at the pin in force when it was written.** All four symbols were in
stiva's own `lib/`: `sys_fchownat` (`syscalls_x86_64_linux.cyr:393`,
`syscalls_aarch64_linux.cyr:611`), `SYS_FCHOWNAT` (`:116`, `:109`), `AT_FDCWD` and
`AT_SYMLINK_NOFOLLOW` (`syscalls_linux_common.cyr:38`/`:41`, `:61`/`:68`) — and the stdlib's own
header documents this exact use: *"with AT_FDCWD + AT_SYMLINK_NOFOLLOW it gives lchown
semantics."* Fixed in **3.0.18** with a one-line replacement. Nobody re-checked between the
claim being typed and it being disproved; see the `block` stanza rule below.

The `syscall(94, …)` → `__NR_exit_group` hazard was real: an aarch64 build terminated silently on
the first tar entry of any unpack, `import` or `load` — verified under qemu (`stiva load` printed
nothing, exit 16). It is gone.

Remaining work is ordinary and unblocked: build for aarch64, run the suite under qemu-aarch64,
add an aarch64 CI job, and fix whatever that surfaces. **No upstream landing is required.**

⚠ Still worth doing upstream: the suggested cyrius lint on bare integer literals in `syscall()`.
The same drift produced three sibling bugs here (`prctl` 157 → `setsid`, `chdir` 80 → `fstat`,
and this one), and agnos tracks the identical class on its own roadmap as *"Raw `syscall(N,…)`
with LINUX numbers on agnos paths"*. A lint catches the next one; a fixed call site does not.

### AGNOS kernel target

Unblocked by the agnos 1.45.x ring-3 net/socket syscall surface (#45–#57, including server
`sock_listen` #56 / `sock_accept` #57) — sockets were the major hurdle to container usage.

- Build + run stiva against the agnos syscall ABI (FS-write / exec-from-disk / sockets).
- Map kavach isolation onto the agnos sandbox primitives — no Linux namespaces or seccomp there.
- Container networking over the agnos socket surface: bridge / NAT / port-map without slirp/pasta.
- Host the docker-service-sweep server-stage workloads (agora BBS, descent MUD, web server,
  ark+nous server-side) in AGNOS containers.
- Soak and weak-point sweep: connection floods, fuzzed input, resource exhaustion — including the
  kernel's 8-connection TCP and 8-listener UDP caps.

`epoll` is a portability trap on this line: its event struct is packed differently on x86_64 (data at
+4, stride 12) and aarch64 (+8, stride 16), and the AGNOS wrappers take a **different arity** (3-arg
vs Linux's 4-arg). `_exec_capture2` avoids it deliberately by sending stderr to a file rather than a
second pipe; anything new reaching for epoll must handle all three.

---

## v3.3.0 — orchestration surface

Carried from v2.1.0, and unblocked by the port: `ansamblu`, `fleet`, `health` and the `Stiva` facade
all exist now.

- **Kubernetes CRI shim** — a minimal CRI gRPC server wrapping stiva for k8s node integration.
- **Metrics export** — a Prometheus-compatible `/metrics` endpoint.
- **Ansamblu blue-green deploys** — deploy the new version alongside the old, swap traffic.
- **Service mesh integration** — sidecar injection for ansamblu services.
- **Fleet auto-scaling** — adjust node count from majra queue depth, over
  `select_migration_target`. Its prerequisite — the three inherited `fleet.cyr` defects — was
  cleared in v3.0.17.
- **`stiva plugin` system** — loadable plugins for storage drivers, network drivers, auth providers.

## v3.4.0 — Windows containers

A kavach backend for Windows process isolation. Separate because it is the only item needing a
non-Linux isolation model end to end, and because `lib/syscalls_windows.cyr` currently stubs
`sys_fork` / `sys_execve` / `sys_waitpid` to `-1` — so the capture primitives every backend relies on
do not exist there yet.

---

## Known limitations

Real, understood, and deliberately lived with. Each states its cost rather than a plan — but each is
here so it can be argued with, not so it can be forgotten.

- **Exec output is truncated at the first NUL byte.** `ContainerExecResult` holds cstrs, so binary
  output from `stiva exec` stops there. Fixing it means carrying explicit lengths through the result
  struct and every consumer. Text commands are unaffected.
- **The event log's rotation is racy across processes.** `_cm_file_size` + `rotate_logs` run outside
  `file_append_locked`'s flock, so two processes crossing the threshold together can discard a
  retained generation. Closing it needs `file_append_locked` to expose its lock; a second lock here
  would invert the acquisition order against the container-log path. The live append IS flocked, so
  no event is ever lost or interleaved — only an older rotated file.
- **`index.json` cannot express a digest-pinned reference.** It carries the reference in the
  `org.opencontainers.image.ref.name` annotation, which has no digest field, so after a store reload
  two digest-pinned pulls of one repository read as unpinned. Making the annotation digest-aware is
  an on-disk format change.
- **The build cache's fingerprint is metadata-only** — path, mode, uid, gid, size, nanosecond mtime.
  An edit preserving all of those will not invalidate it. Reading contents would cost as much as
  rebuilding the layer the cache exists to skip. `rm -rf {root}/cache` forces a rebuild.
- **`_stor_write_tar` assembles in memory** (`storage.cyr:1363`), so a rootfs larger than addressable
  RAM cannot be exported. rust-old streamed via `tar::Builder`; a streaming writer lands if it ever
  matters. A 16 GiB assembly ceiling and a null-alloc check make the failure an error rather than a
  crash.
- **`policy_strict` cannot run on the OCI backend unprivileged** — the generated spec always emits
  `linux.resources`, which runc refuses without the cgroup delegation an unprivileged user lacks. A
  kavach-side fix; raise it there if a consumer needs strict policy on OCI.

---

## Recording a block

Adopted 2026-08-21, after an audit found that **eight of eight "blocked on upstream" claims in
the v3.1.0 section were wrong.** Not one was caught by review; every one was caught by running
something. The rules below are mechanical on purpose — the failure was never carelessness, it
was writing a conclusion once and never re-testing it.

**A claim of a block is not prose. It is a stanza, and without one the item is not blocked — it
is unstarted, and must be filed that way.**

````
```block
what:       the capability that cannot be delivered, in one line
dep:        <repo> | none
verified:   <dep> <version>          # the pin the probe was last RUN against
probe:      scripts/probes/<name>.sh # exits 0 when the block has LIFTED
routes:
  through-api:  <what the dependency's public API offers, and why it is not enough>
  around-api:   <MANDATORY — may not be blank>
  raw-syscall:  <MANDATORY — may not be blank>
```
````

Seven rules, each mapping to a specific way this went wrong:

1. **`probe:` is mandatory and executable.** It exits 0 when the block has lifted, non-zero
   while it holds. Writing it forces you to state what "unblocked" looks like — the step none of
   the eight took. A probe going green is a build failure: the tree tells you the block is gone.
2. **`around-api` and `raw-syscall` may not be blank.** Both were the answer for five of the
   eight. This tree bypasses kavach for `exec` and nein for `apply`, both deliberately and both
   documented — a library boundary here is a choice, never a wall.
3. **`verified:` records the dep version the probe last ran against.** CI compares it to the tag
   in `cyrius.cyml` and fails on mismatch. Four claims went stale at one dep bump that edited
   exactly one roadmap line.
4. **Never type a number about a dependency.** Struct sizes and field counts come from a script
   reading `lib/`, or they are omitted. "8 fields / 64 bytes" was wrong the day it was written.
5. **Assert symbol absence in backticks, and expect to be checked.** CI greps `lib/` and the
   pinned snapshot for every backticked identifier in a sentence containing *no* / *neither* /
   *does not exist* / *exposes no*. A hit fails the build. This alone would have caught four,
   including `SYS_FCHOWNAT`, which the roadmap named while it sat in this repo's own `lib/`.
6. **"Verified by execution" must carry the command and its expected output.** One measurement
   on the detached path became "**every** stiva container runs in the HOST pid namespace." It
   was false for the runc path, and one re-run would have said so.
7. **These phrases are build errors** outside a stanza with a live probe: `cannot be fixed`,
   `under any wiring`, `no amount of`, `not on stiva effort`. At adoption there were three hits
   in this tree and **two were provably false** — `roadmap.md`'s aarch64 entry and
   `src/storage.cyr`'s `_stor_lchown`. Both are now fixed; the phrase is what survived longest.
   ⚠ **Implementer's note, learned by running the check on the day it was written:** a bare grep
   now matches five lines, and every one is a *quotation of the phrase being disavowed* — this
   rule's own documentation, and the two corrected comments that record what they used to claim.
   The gate must skip lines where the phrase is quoted or negated, or it fires hardest on exactly
   the writing that fixed the problem. Match the *assertion*, not the string.

⚠ **Inherited oracle prose is not evidence.** `src/intents.cyr:4` still says "The agnoshi project
does not exist yet" — copied verbatim from `rust-old/`, where it was **already false when the
oracle froze**, and carried unread through a whole-language port. When porting a comment, port
the behaviour and re-derive the claim.
