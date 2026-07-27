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

## Where stiva is — v3.0.16

A working single-node OCI runtime in Cyrius, ported from the frozen Rust oracle at `rust-old/`.

| | |
|---|---|
| CLI | **32 of 35 verbs live**; `checkpoint`, `restore`, `diff` are not wired |
| Image | pull · push · build · import · export · save · load · tag · rmi · gc · prune, over a valid OCI image layout |
| Container | run · run -d · exec · ps · stop · kill · restart · rename · pause · unpause · logs · logs -f · events · wait · top · cp · stats · inspect |
| Tests | 2075 across `tests/*.tcyr` · 87 CLI smoke assertions · 14 benchmarks |
| Deps | cyrius 6.4.78 · kavach 3.9.3 · cmdit 1.2.2 · majra 2.5.1 · nein 1.6.4 · bote 3.1.4 · agnodrm 1.5.0 |

**Three facts that constrain everything below**, all verified by execution rather than by reading:

- **The cycc struct-id 20/21 ↔ SIMD-sentinel miscompile is still live at 6.4.78.** A typed
  `var x: T = p; x.field` can silently read garbage — not crash — and it is per-function AND
  per-compilation-unit. The raw-offset accessors (`_img_id` / `_img_layers` /
  `_img_manifest_digest` / `_layer_digest`) are deliberate workarounds. Retiring them needs a probe
  green in *every* unit shape including the 6-module `store.tcyr`, and an assertion that fails on a
  silent wrong value rather than only on a segfault.
- **Only x86_64 works.** See v3.2.0.
- **Containers have no secrets and, unprivileged, no network.** See v3.0.17 item 3 and v3.1.0
  item 1. Both were `TODO(v3.1)` comments in the source and appeared in no roadmap until now.

---

## v3.0.17 — finish the single-node runtime

No external landing required for any of this.

### 1. `stiva diff` — the last wireable verb

Over an overlay the changed set IS `{croot}/upper`; a flattened rootfs has no upper and must be
compared against the layer dirs. The two cannot be told apart after the fact — `internals` is
process-local and never persisted, and `setup_overlay` creates `upper`/`work`/`merged` *even when
the mount fails* — so `create` drops a `{croot}/.rootfs-flattened` marker for `diff` to read.

**There is no oracle.** `ContainerManager::new` restores containers from `state.json` but leaves
`internals` empty (`rust-old/src/container.rs:260`) and nothing rehydrates it, so `get_rootfs`
always returns `ContainerNotFound` and the Rust `diff` verb has never executed in the CLI. This is a
rewrite; the Docker A/C/D output convention is the reference.

### 2. Rootless container networking — containers currently have NO network unprivileged

`network_manager.cyr:119-122` and `:204-207` log `"bridge creation deferred (requires root)"` /
`"veth creation deferred (requires root)"` and carry on, so an unprivileged `stiva run` produces a
container with no connectivity and only a warning. That is stiva's default mode.

`network_rootless.cyr` already ports the detection half — `RootlessNetworkBackend`,
`parse_port_mappings`, `is_unprivileged`, `stiva_which`, `available_backends`, `select_backend` —
and only the slirp4netns/pasta **spawn** was left out, for want of a subprocess primitive.

**That primitive now exists**: `_exec_capture2` (`src/runtime.cyr`) forks, execs, captures both
streams and reports a real wait status. The remaining work is to spawn the selected backend against
the container's network namespace and wire `parse_port_mappings` through to it.

### 3. Persist `scan_policy`, so `logs --scan` stops being opt-in

`container_config_to_jv` writes `scan_policy` as `json_v_null()` and `_from_jv` reads it back as 0
(`container.cyr:548, 658`), so the field never survives a save/load. The oracle gates output
scanning on that persisted value (`rust-old/src/container.rs:1210-1222`); because the port drops it,
`stiva logs --scan` is an explicit flag instead of a per-container default.

Purely stiva-side: give `ContainerConfig.scan_policy` a real serde round-trip and have `logs`
consult the record, keeping the flag as an override. (The *sandbox-side* half — actually threading
the policy into kavach — is v3.1.0 item 1, because it needs a kavach setter.)

### 4. Three inherited `fleet.cyr` defects

All present in `rust-old/src/fleet.rs` too, so correcting them is a deliberate divergence and wants
an ADR. Pure functions, no I/O, fully CI-testable.

- `plan_rollback` (`src/fleet.cyr:561-587`) calls `select_migration_target` once per running
  container with **no reservation between calls**, so all N containers on a failed node plan onto
  the same target.
- `DeploymentConstraints.preferred_nodes` (`src/fleet.cyr:127`) is never read.
- `FLEET_NODE_DRAINING` / `FLEET_NODE_CORDONED` (`src/fleet.cyr:59-60`) exist but every filter tests
  only `== FLEET_NODE_READY`, so there is no drain semantic.

### 5. Delete the stale deferral comments

`cyrius lint` reports 36 untracked deferrals across 12 files. Some describe work that has since
landed, so the comment is now false and actively misleading. Delete these; do not re-track them.

- `imagelayout.cyr:854` — claims a docker-archive is "reported as unsupported (the docker→OCI read
  path is a v3.0.2 follow-up)". The read path landed in **v3.0.3** and the function handles it.
- `container.cyr:478, 947, 1555` — describe the `ContainerManager` as deferred. It landed in §C.
- `health.cyr:15` — calls `exec_in_container` deferred. It landed in §D.
- `mcp.cyr:13` — says the handlers "land with the Stiva runtime driver". They landed in §E.
- `runtime.cyr:9, 1771, 1781` — the DEFERRED block header; only CRIU is still deferred, and the
  block lists `export_rootfs` / `import_rootfs` / `spawn_container`, all of which shipped. Trim it
  to CRIU and point at v3.1.0.
- `fleet.cyr:600`, `storage.cyr:1752`, `image.cyr:277` — already say nothing is deferred; reword so
  the linter agrees.
- `intents.cyr:60, 65, 76, 266` — real, but the tracking reference belongs here: see v3.1.0 item 5.
- `audit.cyr:174, 457` — false positives on ordinary prose ("worst-case", "take low byte only");
  reword.
- `main.cyr:202, 205, 1586, 1589` — the `_cli_deferred` handler for the three unwired verbs. It goes
  away when `diff` (item 1) and CRIU (v3.1.0 item 3) land.
- `storage.cyr:1363` — a real limitation, not a deferral: see "Known limitations".
- `network_manager.cyr:51, 101, 121, 206` — real: item 2 above.
- `container.cyr:547, 548, 657, 658, 1814` and `runtime.cyr:796, 856` — real: item 3 above and
  v3.1.0 item 1.

### 6. Decide the `accel` feature gate — blocks items 7 and 8

**A product decision, not an engineering one, and it needs an answer before §H or §I start.**

Cyrius `[features]` gate **dependency activation only, not source**; there is no `#[cfg(feature)]`
and no conditional `include`. Proven by execution: with `default = []` a build calling into
`ai-hwaccel` fails with "refusing to emit binary with 2 reachable undefined function(s)", and
`cyrius test` / `cyrius tests` have **no `--features` flag**. So any §I code in `src/` forces
`default = ["accel"]` permanently, making 276 KB of samay + ai-hwaccel non-optional for every
consumer of `dist/stiva.cyr` — daimon and sutra included.

The identifier-cap worry is **not** a reason to hesitate: measured at pin 6.4.78, the worst unit with
`accel` on is 276,898 / 524,288 (52.8%), and a full `cyrius tests --features accel` run passes.

Options: turn it on and accept the size; keep it off and drop §H/§I; or split the accelerator surface
into a separate consumer-side module.

### 7. §H — scheduled / cron containers

stiva has no time-triggered container start, and neither does `rust-old`. The hard parts are already
written in `samay` — named here so nobody re-derives them:

- `cron_expr_parse` / `cron_expr_matches` / `cron_expr_next_after` — a full standard 5-field parser
  with `@shortcuts`, ranges, steps, month/DOW name lookup, and the Vixie `crontab(5)` DOM-vs-DOW
  star rule.
- `cron_scheduler_new` / `_add(name, expr, template, enabled, missed_policy)` / `_check_due` /
  `_check_due_at` / `_remove_entry` / `_list_entries`, with `struct CronEntry` / `struct
  CronScheduler`, a missed-schedule policy, and catch-up counting with a cap
  (`_cron_count_due`, `_cron_log_catchup_cap`) — the hard part, already solved.
- `cron_scheduler_to_json_str` / `from_json_str` for persistence.

`cron_scheduler_check_due_at` takes an injected clock, so catch-up/skip/cap semantics are
deterministically testable with no wall clock.

Two pieces samay does not give you:

- **A container side table.** `CronTaskTemplate` carries name/description/agent_id/priority/
  resource_requirements — no image, command, env, mounts or restart policy — and its JSON codec
  cannot carry them either. stiva owns an entry-name → `ContainerConfig` table and its persistence,
  and must strip the `" (cron)"` suffix the task namer appends. The typed cron *expression string*
  is not persisted either (only the seven bitmasks), so stiva stores its own.
- **A poll site.** `src/main.cyr` is a one-shot dispatcher ending in `syscall(SYS_EXIT, …)`. v1 is
  `stiva cron add/ls/rm/check` driven by an external systemd timer — `check` fires what is due and
  exits. The in-process loop is a later increment.

Prerequisite: item 6.

### 8. §I — accelerator inventory and placement

`lib/ai-hwaccel.cyr` is a read-only hardware inventory and workload-planning library across 19
accelerator types and 17 backends. It is **not** a device-plumbing library — see v3.1.0 item 4.

- **Node inventory in `stiva info` / `inspect`.** `registry_detect_no_exec()` is subprocess-free
  (pure sysfs/syscall reads) so it cannot hang on a missing `nvidia-smi`; pair with
  `registry_to_summary_json`. Two calls.
- **An accelerator dimension in `fleet` placement — graft onto stiva's own structs, do NOT adopt
  samay's `NodeCapacity`.** samay's is a superset only on the continuous dimensions: no
  `max_containers` (stiva's entire fit test), no node status, no label constraints, and only
  best-fit-by-utilization against stiva's three strategies. Adopting it regresses three shipped
  features. Its ledger also leaks — `node_capacity_release` is reached only from `cancel_task`, so a
  task reaching COMPLETED/FAILED never releases its reservation.
- **Persist accel profiles** — `profile_to_json` / `profile_from_json` round-trip losslessly.
- *(optional)* NUMA / fabric affinity, so ansamblu can express "these two containers must land on
  NVSwitch-connected devices". `detect_interconnects` shells out and is masked off under
  `registry_detect_no_exec`.

Prerequisite: item 6.

### 9. CI guard for the tag/path substitution

A local `path = "../<dep>"` override **silently wins** over the `tag` pin — this is how kavach 3.8.1
once arrived unannounced — and `cyrius.lock` records no dep name or version, so it cannot detect the
substitution.

A `git diff --exit-code cyrius.lock` guard was attempted and **reverted**: `cyrius.lock`'s *format*
depends on the resolution mode. Resolving from git tags (CI) writes `commit <sha> <name> <url> <tag>`
lines and a different hash ordering; resolving through path overrides (local) writes only file
hashes. The comparison is between two structurally different files and fails unconditionally.

The workable shape compares the sorted **set** of `<sha256>  lib/<file>` lines, ignoring the `commit`
lines and their order — that tests the real invariant, "the files the tags produce are the files the
lock records". Validate it against a real CI run before shipping it again.

---

## v3.1.0 — secrets, interactivity, and mobility

The theme: a container you can hand a credential to, attach to, and move. Every item names its
prerequisite, and every prerequisite is work in a repo we own.

### 1. Secret injection and the externalization policy — currently absent

`CLAUDE.md` lists kavach's **CredentialProxy / SecretRef** and **ExternalizationGate** among the
features stiva must keep wired. Neither is. `container_config_to_jv` writes `secrets` as an empty
array and `scan_policy` as null (`container.cyr:547-548`), `_from_jv` zeroes both (`:657-658`), and
`build_sandbox` carries `TODO(v3.1): scan_policy + secret injection — no SandboxConfig setter in
dist` (`runtime.cyr:856`).

Ordered work:
1. **kavach** — `SandboxConfig` setters for a secret list and an externalization policy. Today
   `SandboxConfig` is 8 fields / 64 bytes with neither, so stiva cannot express this under any
   wiring.
2. **stiva** — round-trip `secrets` / `scan_policy` through `state.json` (the persistence half is
   v3.0.17 item 3), thread them into `build_sandbox`, and add `stiva run -s NAME=ref`.
3. **stiva** — with the policy threaded, `logs --scan` becomes the per-container default the oracle
   intended, and `exec` output is gated the same way.

### 2. Interactive `exec -it`

Non-interactive `exec` shipped in 3.0.15. The `-t` half needs two things, and **neither is a waiting
game**:

- **A pty helper, which exists nowhere.** `openpty` / `posix_openpt` / `/dev/ptmx` / `TIOCSCTTY` /
  `forkpty` / `ptsname` return nothing across all of `lib/` and `src/`. It has to be written, most
  sensibly in the cyrius stdlib alongside the other syscall wrappers.
- **Stackless coroutines in cyrius**, for the two-directions-in-one-task relay. Filed as
  `cyrius/docs/development/issues/2026-07-25-stiva-stackless-coroutines-interactive-exec.md`;
  `roadmap-future.md:116` parked the item as "no live consumer", and that file is stiva going on
  record as the consumer. Awaiting triage — accept-and-pin, or a decline stiva designs around
  permanently.

### 3. CRIU `checkpoint` / `restore`

**Sequenced, not skipped.** The port is straightforward — the oracle shells out to the `criu` binary
and `criu_available()` is already ported. What makes it pointless *today* is one layer down:

**Every stiva container runs in the HOST pid namespace.** `NS_PID` is defined in kavach and set by
no caller; `build_sandbox` never forwards the spec's namespace list, so `RUNTIME_NS_PID`
(`src/runtime.cyr:519`) is decorative. Verified by comparing `/proc/<pid>/ns/pid` against our own on
a live container: identical. `criu restore` would therefore have to re-claim the exact host PIDs via
`/proc/sys/kernel/ns_last_pid` — racy by construction, and an outright failure when the pid is taken.

Ordered work:
1. **kavach** — honour the namespace list in `sandbox_spawn` / `build_sandbox` so `NS_PID` actually
   creates a PID namespace. The real prerequisite, and it is kavach-side work.
2. **stiva** — pass the spec's namespaces through and assert the container's pid namespace differs
   from ours (the `_rt_userns_differs` shape `exec` already uses).
3. **stiva** — port `checkpoint` / `pre_dump` / `restore` / `restore_lazy`, wire both verbs.
   `container_manager_restore` must also set `start_ticks` from the new pid, or
   `container_fixup_after_restart` demotes the just-restored container to STOPPED on the next load.

CI cannot test step 3 (criu needs root and a kernel config the runner lacks); steps 1–2 are
unit-testable and step 3 ships behind `criu_available()` with manual verification recorded.

### 4. §J — device and accelerator passthrough

`--device` / `--gpus`, `/dev` node injection, cgroup device rules, driver-library mounts.

The hole is one layer upstream: `SandboxPolicy` has **no device field** and `cgroup_setup` writes
**no devices controller** — verified — so stiva cannot express passthrough through the kavach API
under any wiring. kavach's own samay bridge silently drops `accel_req` / `accel_min_chips` for
exactly this reason.

Ordered work:
1. **kavach** — a device-allow list on `SandboxPolicy`, a cgroup v2 devices gate in `cgroup_setup`,
   `linux.devices` emission in `oci_spec.cyr`. kavach's roadmap already wants adjacent work
   (USB/device selective passthrough, GPU passthrough / VFIO / virtio-gpu).
2. **stiva** — emit OCI `linux.devices[]` + `linux.resources.devices[]` beside the existing
   CPU/mem/PID/IO cgroup writers, using ai-hwaccel's `REQ_*` / `profile_*` vocabulary to describe and
   validate the request.

**ai-hwaccel does not supply the plumbing** and should not be expected to: it never emits a device
node path, computes a major/minor, produces a cgroup rule, or lists driver mounts. It implements the
*guest* half of the nvidia-container-toolkit contract (reading `NVIDIA_VISIBLE_DEVICES`,
`/.dockerenv`, `/proc/1/cgroup` to discover what another runtime already set up); stiva writes the
*host* half. The one usable seed is `_find_pci_addr`, returning a BDF sysfs path.

### 5. Device allocation ledger

"Container A holds GPU 0, don't hand it to B." Neither library provides it — samay's
`node_capacity_reserve` decrements CPU and memory but **not** `accel_profiles`, so even its
accelerator handling is match-only, never exclusive. ai-hwaccel supplies the matcher; the ledger is
unwritten in both.

Unblocked design work, not a dependency: a natural extension of `state.json` plus the majra lifecycle
events. Needs item 4 to be useful, so it lands with it.

### 6. `parse_intent` — the agnoshi NL parser

`intents.cyr:60, 65, 76, 266`. The `Intent` value type, its constructors and the externally-tagged
JSON serde all shipped; only the natural-language → `Intent` parser is missing, and it returns
`STIVA_ERR_RUNTIME` with "agnoshi intent parsing not yet implemented". Gated on the **agnoshi**
project, which is where the grammar lives — not on stiva effort.

### 7. Kavach error detail

`sandbox_create` / `sandbox_exec` return 0 on failure with no error code (`runtime.cyr:796`), so
stiva's messages carry no cause — `sandbox_transition` returns a code and gets `kavach_err_name`,
these do not. A kavach API change: return a code, or expose a `kavach_last_error()` the way stiva's
own `registry_last_error()` works.

### 8. True concurrent layer downloads

`image_store_pull` fetches layers sequentially; the oracle used `buffer_unordered(4)`. Needs a
multi-threaded async runtime — the current one is single-threaded run-to-completion. Fine for a
single node, which is why it is here rather than in 3.0.x.

---

## v3.2.0 — non-x86: aarch64, then AGNOS

### aarch64 — currently broken, one upstream landing away

`_stor_lchown` (`src/storage.cyr`) issues `syscall(94, path, uid, gid)`. 94 is `lchown` on x86_64 and
`__NR_exit_group` on aarch64, so an aarch64 build **terminates silently** on the first tar entry of
any layer unpack, `import`, or `load` — verified under qemu: `stiva load` prints nothing and exits 16
(the path pointer's low byte).

It cannot be fixed in stiva: the cyrius stdlib exposes neither a `sys_lchown` wrapper nor a
`SYS_LCHOWN` / `SYS_FCHOWNAT` per-arch constant. Filed as
`cyrius/docs/development/issues/2026-07-26-no-lchown-wrapper-forces-a-hardcoded-x86-64-syscall-number.md`,
which also suggests a lint on bare integer literals in `syscall()` — the same drift produced two
sibling bugs (`prctl` 157 → `setsid`, `chdir` 80 → `fstat`) that *were* fixable because the stdlib
did expose the right thing.

Once the wrapper lands: swap the call, run the suite under qemu-aarch64, add an aarch64 CI job.

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
- **Fleet auto-scaling** — adjust node count from majra queue depth. Needs the v3.0.17 fleet defects
  fixed first, since it drives `select_migration_target`.
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
