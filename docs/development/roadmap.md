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

## Where stiva is — v3.0.17

A working single-node OCI runtime in Cyrius, ported from the frozen Rust oracle at `rust-old/`.

| | |
|---|---|
| CLI | **34 of 36 verbs live** (`cron` is new); only `checkpoint` and `restore` are unwired (v3.1.0 item 3) |
| Image | pull · push · build · import · export · save · load · tag · rmi · gc · prune, over a valid OCI image layout |
| Container | run · run -d · exec · diff · ps · stop · kill · restart · rename · pause · unpause · logs · logs -f · events · wait · top · cp · stats · inspect |
| Tests | 2189 across `tests/*.tcyr` · 87 CLI smoke assertions · 14 benchmarks |
| Deps | cyrius 6.5.33 · kavach 3.12.0 · cmdit 1.2.2 · majra 2.6.7 · nein 1.6.10 · bote 3.3.2 · agnodrm 1.5.1 · sigil 3.12.9 · sakshi 2.4.11 · libro 2.8.8 · samay 1.0.1 · ai-hwaccel 2.3.18 |

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
