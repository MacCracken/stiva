# Roadmap

**Completed work is not kept here.** The CHANGELOG carries what shipped and why; git carries the
rest. This file is only what is still to do.

**Every item belongs to a release, and nothing is parked for being hard.** Where an item needs
something from another repo, that prerequisite is written down as its own work item with a named
symbol, not as a reason to wait.

**Every deferral marker in the source is listed here.** If you add a `TODO` / `deferred` /
`not yet` to the source, add it here in the same change.

| Release | Theme |
|---|---|
| [Upstream](#upstream-work-that-stiva-needs) | Asks of kavach, sandhi, sakshi and sankoch |
| [v3.2.0](#v320--containers-honour-their-image) | A container runs what its image says |
| [v3.2.1](#v321--containers-you-can-manage) | Lifecycle: ps, stop, restart, wait, per-container cgroups |
| [v3.2.2](#v322--configuring-a-container) | Mounts, user, secrets, the `run` flags |
| [v3.3.0](#v330--registries-and-the-store) | Private registries, parallel pulls, store integrity |
| [v3.3.1](#v331--networking) | A container gets a network |
| [v3.3.2](#v332--orchestration) | ansamblu, health, fleet, daimon, multi-stage builds |
| [v3.3.3](#v333--the-control-plane) | The MCP server, `parse_intent` |
| [v3.4.0–v3.4.3](#v34--interactive-mobile-and-hardware) | `exec -it`, CRIU, devices, encrypted storage |
| [v3.5.0–v3.5.2](#v35--other-platforms) | aarch64, AGNOS, Windows |
| [v3.6.0](#v360--ecosystem) | CRI, metrics, deploy strategies, plugins |

---

## Where stiva is — v3.1.2

A working single-node OCI runtime in Cyrius, ported from the frozen Rust oracle at `rust-old/`.

| | |
|---|---|
| CLI | **33 of 36 verbs live**: `checkpoint` and `restore` are unwired (v3.4.1), and `restart` cannot succeed (v3.2.1) |
| Image | pull · push · build · import · export · save · load · tag · rmi · gc · prune, over a valid OCI image layout |
| Container | run · run -d · exec · diff · ps · stop · kill · rename · logs · logs -f · events · wait · top · cp · inspect |
| Deps and counts | [`state.md`](state.md) lists both, and CI checks the dep list against the manifest. Not restated here. |

**What constrains everything below:**

- **The cycc struct-id 20/21 ↔ SIMD-sentinel miscompile was last verified live at 6.4.78 and has
  not been re-verified at any later pin, so assume live.** A typed `var x: T = p; x.field` can
  silently read garbage, per function and per compilation unit. The raw-offset accessors are
  deliberate. Retiring them needs an assertion that fails on a silent wrong value rather than only
  on a segfault. Write-up:
  [`docs/architecture/001-cycc-struct-id-miscompile.md`](../architecture/001-cycc-struct-id-miscompile.md).
- **Only x86_64 is built and tested in CI.** aarch64 builds and its suite is run by hand under
  qemu; see v3.5.0.
- **The default foreground path has no seccomp and no NO_NEW_PRIVS.** See the first
  [upstream item](#kavach). Only the process backend and `run -d` have both, and `exec` has
  NO_NEW_PRIVS alone. This is the largest security gap in stiva.
- **Three `RuntimeSpec` fields are still write-only.** `mounts` and `user` are v3.2.2 and
  `namespaces` is v3.4.1. `env` and `workdir` were the other two, and each needed a kavach change
  first because the setter stiva wanted was decorative: a green call to a setter nothing reads is
  indistinguishable from a fix. **When you add a field to a spec struct, grep for a reader on both
  sides of the boundary.** `env` shipped broken through 2175 green tests because the test asserted
  the spec carried the value, and nothing asserted a container could read it.

---

## Upstream work that stiva needs

Each is a change to another repo, then a pin bump here. None is a reason to wait: the items that
depend on them say what to do in the meantime.

### kavach

- **Seccomp and NO_NEW_PRIVS on the OCI backend.** `oci_generate_spec` emits neither, and a
  foreground `stiva run` takes the OCI backend whenever runc is present. Emit
  `process.noNewPrivileges` and a `linux.seccomp` profile. Until then SECURITY.md says so plainly.
- **`SPAWN_MAX_FD`.** Its fd sweep stops at 1024. stiva marks every descriptor close-on-exec before
  it reaches kavach (`_stor_cloexec_fds`), so this is defence in depth and not a live hole.
- **Landlock is never applied** to a sandbox, on any path. Either apply it or remove it from what
  kavach advertises.

### sandhi

- **`_sandhi_dl_drain_headers_a` has no deadline.** stiva bounds the socket with `_reg_set_bounds`,
  so a silent server fails after one I/O timeout, but a server that trickles one header byte per
  interval keeps it alive indefinitely.

### sakshi

- **An opt-out for TSC calibration.** It costs about 10 ms in every process that logs, so in every
  `stiva` command. Measure again at the next pin, because it dominates the fast verbs.

### sankoch

- **A public output cap on one-shot `gzip_decompress`.** It keeps a 16 MiB ceiling, and stiva lifts
  it through the private `_deflate_dec_set_out_max`. A public setter retires that reach.

---

## v3.2.0 — containers honour their image

A pulled image should run. Today `stiva run` needs an explicit command, and what the image's
config says is parsed and dropped.

- **Entrypoint, Cmd, Env, WorkingDir, User.** Read them from the image config and apply them as
  defaults. `cc.config` carries none of them. `env` already reaches the payload through
  `config_env`, so image env needs only the merge. ⛔ Test what the payload observes, not what the
  spec carries.
- **A default `PATH`.** With no image env the environment is empty, so `run -d img sleep 1` cannot
  find `sleep`. Docker's default is the precedent.
- **A derived image keeps its base.** `build_image` writes a config that drops the base's
  environment, entrypoint, user and architecture (`_bld_` config assembly, see `build_image`), so
  building on an image loses what it says.
- **Short references resolve.** `stiva tag` prints `local/bb:v2`, which does not resolve as written,
  and `run repo@sha256:…` fails after a pull by digest. One rule for what a user may type, used by
  `run`, `rmi`, `tag`, `save`, `push` and `inspect`.

## v3.2.1 — containers you can manage

- **Foreground containers are visible.** While a foreground `stiva run` executes, its record says
  `Created` with no pid, so `ps` omits it, and `stop` from another terminal reports success and
  leaves the payload running.
- **Streaming output.** Foreground output sits in a 1 MiB buffer until the payload exits. Stream it,
  and report truncation in `exec`'s exit status instead of a silent cut.
- **`restart` and `start` from a new process.** Both need the runtime spec, which lives only in the
  process that created the container. Persist it.
- **Exit codes across processes.** `wait` now follows a detached container's pid, but its exit code
  is only known to whatever reaps it, so `wait` says none was recorded. Needs a small supervisor, or
  a pipe held by the container's parent.
- **Per-container cgroups.** stiva creates none, so a payload shares the cgroup of whatever started
  it. `pause` and `update` refuse rather than freeze or limit the operator's session
  (`_rt_cgroup_is_exclusive`), which today means they refuse nearly every container, and `stats`
  reports the shared cgroup's counters.
- **Unique names.** Two containers can hold one name, and every by-name verb takes the first.
- **Cron runs detached.** `cron check` runs each job in the foreground, so a long job delays the
  next, and nothing reaps or records them.
- **Timestamps.** `inspect`'s `created_at` is uptime in milliseconds, not an epoch.
- **Events are complete.** A foreground run emits `created` and `started` only, with `started` after
  the payload exits, and a `run -d` container that exits by itself emits nothing. `logs -f` should
  also stop on container state, not on quiet.
- **Foreground logs are framed**, so `logs -n 2` shows the trailer rather than the last two lines.
- **Kavach error detail** (was v3.1.0 item 7). All three accessors already exist:
  `backend_result_is_diagnostic`, `spawn_exit_name` and `sandbox_exec_last_scan_result`, and stiva
  calls none of them. Read the diagnostic channel stiva already receives, pre-flight
  `backend_is_available` before `sandbox_create`, and map `spawn_exit_name` over `sandbox_spawn`'s
  return-0 arms, which is the live `run -d` path.
- **`--image-path`** is accepted and ignored. Wire it or remove it.

## v3.2.2 — configuring a container

### The `run` flags

`-e`, `-v`, `-p`, `--user`, `--restart` and `--rm` are unknown flags today, and nothing on the CLI
sets a limit, `timeout_ms`, `rootless`, `seccomp_profile` or `log_max_*`, though the config carries
them all. Register verbs last, because ids are registration-ordered, and give each flag an
end-to-end test against what the payload observes.

### Wire `RuntimeSpec.mounts`

`mounts` is write-only, the same shape as `env`. `standard_mounts()` builds `/proc` and `/dev`
entries that never reach a mount.

⚠ This is why **AGNOS-native payloads still see no environment**: cyrius's `getenv` reads
`/proc/self/environ`, and `run -d` mounts no `/proc`. glibc and busybox payloads are fine. daimon
and sutra are not. Tracked on the agnos roadmap, and the alternative fix is cyrius-side; see v3.5.1.

### Wire `RuntimeSpec.user`

`RuntimeSpec.user` is assembled and never read, so a Stivafile's `user = "nobody"` is recorded and
ignored. **Treat every container as running as root today.**

⛔ This is not a one-liner and it is not obviously kavach's: dropping privileges means
`setgid`/`setgroups`/`setuid` in the child **after** the rootfs entry and **before** execve, in that
order, with the uid resolved against the container's `/etc/passwd` rather than the host's. Establish
where it belongs before writing it, and verify any kavach setter reaches a syscall before trusting
it: `config_workdir` was decorative until kavach 3.12.2.

### Secrets and the externalization policy

**Not blocked, and never was.** In order:

1. **Externalization.** The lever is `sandbox_exec_set_ext_policy`. Persistence and the `stiva logs`
   consumer are built and tested, and what is missing is a producer: nothing sets a non-zero
   `ContainerConfig.scan_policy`. Add a `--scan-policy` family to `_cli_run`, then gate
   `container_manager_exec` through `scan_output`.
   ⛔ Call `sandbox_exec_set_ext_policy`, **not** `config_externalization`, which is decorative:
   `SandboxConfig_externalization` has no readers in the bundle.
   ⚠ Foreground `stiva run` output is already scanned under kavach's default policy
   (block=CRITICAL), which stiva never chose. Verify that by execution before relying on it.
2. **Secrets.** `SecretRef`, `CredentialProxy`, `credential_proxy_env_vars` and
   `credential_inject_files` all exist in kavach, and `config_env` is the env channel. What remains
   is a `SandboxConfig.secrets` field or routing the proxy's env vars through `config_env`. **Decide
   which before building.** `secrets` serializes as an empty array until then.

### Seccomp on `exec`

`exec` enters the container by its own route and sets NO_NEW_PRIVS but no seccomp filter. Apply the
container's profile.

---

## v3.3.0 — registries and the store

- **Private registries.** A `login` verb, credential use from the CLI (`pull` and `push` do not
  read the credential store today), and plain HTTP or insecure TLS for a local registry.
  `RegistryConfig.host` is what binds a credential to its host, and a config without one sends none.
- **Tag listing follows `Link`.** `tags/list` stops at the first page.
- **Parallel layer downloads** (was v3.1.0 item 8). Real OS threads and a thread-safe allocator
  exist, and a 9-way digest-correct pull was measured at 15.0 s against 22.5 s. ⚠ The prerequisite
  was a dependency defect: the first concurrent TLS handshake in a cold process failed
  deterministically, from ~35 unguarded lazy-init flags in sigil. sigil 3.13.6 builds those tables
  under `_sigil_once_*`. That is from reading the code, and **the cold 9-way pull has not been re-run
  at this pin**. Re-run it first, and keep a main-thread TLS warm-up covering every handshake shape
  the pull touches (including `auth.docker.io`) unless it goes 9/9 without one. Then a mutex on the
  token cache and a fixed 4-worker pool with layer order taken from the descriptor list.
  ⛔ Do not move `var ld: Descriptor = vec_get(descs, i)` into a worker fn: that is the struct-id
  20/21 shape.
- **Blob integrity on read.** Digests are checked on download and by an explicit verify, not when a
  blob is read back. `save` passed a flipped byte straight into an archive.
- **`layers/` is never collected.** `gc` sweeps blobs. Unpacked layer directories stay until the
  store is deleted.
- **Archives over 2 GiB.** `export`, `save` and the tar writer hold the whole archive in one
  allocation, and `alloc` returns 0 past `ALLOC_MAX`. Stream them. (Unpacking already fails cleanly.)
- **xattrs and file capabilities** in layers. Dropped today, so a `cap_net_raw` binary loses it.
- **zstd layers on write.** stiva reads them and writes gzip. A build-side choice with no reason to
  make it yet.

## v3.3.1 — networking

Most of this exists as a library with no caller. `network_manager_new` is never called, so no
container gets a bridge, a veth or an address.

- **Construct a `NetworkManager`** and attach containers to it.
- **Move the veth into the netns.** `move_to_netns` and `configure_container_iface` have no caller.
- **DNAT per container**, and a host-port conflict check.
- **`-p`** (v3.2.2) reaches `connect_container`.
- **Persist the IP pools**, which live only in memory.
- **The DNS registry** has no caller either. When it gets one, `unregister` needs an owner check, so
  a container that has gone cannot remove a name another has since taken.
- **Verify the rootless helpers against real binaries.** pasta and slirp4netns are not installed in
  CI, so stiva's tests see the argv pasta receives, not what it does with it.

## v3.3.2 — orchestration

- **A verb that runs an ansamblu.** There is none. Parsing, DAG order and rolling-update plans exist,
  and nothing drives them.
- **`parse_ansamblu`** reads dotted service names and inline `env`.
- **Health** is unreachable from the CLI. rust-old's `run_probe` and `start_probe_loop` are not
  ported, and the question to settle first is what process runs the loop, since a CLI exits.
- **Restart policies** are parsed and stored, and nothing applies them.
- **Fleet.** Spread and bin-pack re-sort their candidates, which throws away the `preferred_nodes`
  order that `_fleet_candidates` puts first.
- **Daimon registration.** `register_container`, `deregister_container` and `report_status` are not
  ported. sandhi is the HTTP client, so this is unstarted, not blocked.
- **`run` steps and multi-stage builds.** `build_image` refuses both. A `run` step needs a sandbox
  per step. bayan's section model is flat, so `[[stages]]` parse to an empty vec.
- **`convert`** drops `CMD` (as the oracle did), and an `ADD`, `ARG` or `SHELL` silently. Report what
  it drops, as compose conversion now does.

## v3.3.3 — the control plane

- **An MCP server.** There is no `mcp` verb, and `mcp_handle_tool` has no caller, so the nine tool
  definitions cannot be reached by a client. `stiva_run` needs its handler over the `stiva_run`
  facade. `stiva_build` confines `context_dir` to the server's working directory.
- **Retire `parse_intent`** (was v3.1.0 item 6). It only ever fails and has no caller. agnoshi has
  existed since 2026-04, exposes no library surface, and shells out to the `stiva` CLI, so the
  dependency arrow points the other way. Keep the `Intent` value type and its serde. Confirm with
  agnoshi's maintainer first.

---

## v3.4 — interactive, mobile and hardware

### v3.4.0 — interactive `exec -it`

**No upstream prerequisite remains.** `async_wait_rw` and `async_relay_once` are in `lib/async.cyr`.

The pty helper must be written, in stiva. `openpty` / `posix_openpt` / `TIOCSCTTY` / `forkpty` exist
nowhere, but `SYS_IOCTL` is a per-arch constant in `lib/`, `sys_setsid` is in
`lib/syscalls_linux_common.cyr`, and `exec` already bypasses kavach. About 200 lines:
`/dev/ptmx` → `TIOCSPTLCK` → `TIOCGPTN` → `TIOCSCTTY`, raw mode via TCGETS/TCSETS, window size via
TIOCGWINSZ. Then `-i`/`-t` on `_cli_exec` and swap the child's fds.

⛔ Build the relay on `async_wait_fd`/`async_wait_rw`, **not** `async_relay_once`. The latter has no
consumers or tests anywhere, and used as its own doc prescribes (two instances, reversed directions)
it **deadlocks**: a blocking `sys_read` on an idle-but-open fd stalls the single-threaded reactor.
Both fds must be `O_NONBLOCK`, the first call returns raw `-EAGAIN`, and after child exit the master
yields `-EIO` forever rather than EOF, so retire the loop explicitly.

### v3.4.1 — CRIU `checkpoint` / `restore`

**There is a route today.** stiva does not have to go through kavach to get a namespace:
`clone(CLONE_NEWUSER|CLONE_NEWPID|CLONE_NEWNS|SIGCHLD)` is one syscall whose constants are in the
tree. Reproduced unprivileged, container init running as pid 1, by two independent reviewers.

⚠ Only `run -d` lacks a pid namespace. The foreground path selects the OCI backend whenever runc is
present, and kavach's spec emitter always includes one.

In order: (1) fork+clone in stiva's own spawn path, or wire `namespaces` (write-only); (2) assert the
container's pid namespace differs from ours, the shape `_rt_userns_differs` uses; (3) port
`checkpoint` / `pre_dump` / `restore` / `restore_lazy`. `container_manager_restore` must set
`start_ticks` from the new pid, or `container_fixup_after_restart` demotes the restored container to
STOPPED on the next load.

⚠ Worth costing first: `runc checkpoint` and `runc restore` already wrap CRIU, which may retire most
of step 3.

### v3.4.2 — device and accelerator passthrough

**Not blocked on kavach or cyrius.** `SandboxPolicy` has no device field and `cgroup_setup` writes no
devices controller, and neither matters:

- `stiva run -d` never reaches `oci_generate_spec`, the only place `/dev` is masked.
- **cgroup v2 has no devices controller by design.** Device access is gated by an attached eBPF
  program, and an unattached cgroup is ALLOW.
- Host-side `MS_BIND` is in the tree, and a chrooted child inherits mounts established before it
  enters.

Work: `--device` / `--gpus` on `_cli_run`, validation and persistence, then bind the node into the
merged rootfs before spawn. ai-hwaccel supplies the matcher and `_find_pci_addr`. It emits no device
path, major/minor, cgroup rule or driver mount, and should not be expected to.

**The allocation ledger** comes with it:

- ⛔ Do not key it on `_find_pci_addr`'s device index, which is an unsorted getdents ordinal.
- Use `file_lock`, as `_cm_persist` does now.
- ⚠ samay's `node_capacity_reserve` decrements CPU, memory and disk but not `accel_profiles`, has no
  file I/O, and releases only on cancel, so a completed task never releases. Do not adopt it.

### v3.4.3 — encrypted storage

The `encrypted` feature. `src/encrypted.cyr`'s closing block maps each entry point onto sigil's
`luks_*` and `dmverity_*`, which stiva already compiles, so it needs no new dep.

---

## v3.5 — other platforms

### v3.5.0 — aarch64

The target builds, and `cyrius test --aarch64 tests/` passes by hand under qemu-aarch64 (first run at
3.1.2, where it found every symlink guard disabled by x86 open-flag literals). Add a CI job so it stays
that way. The container paths themselves have not run on real aarch64 hardware. **No upstream landing
is required.**

⚠ Worth doing upstream: a cyrius lint on bare integer literals in `syscall()`. The same drift
produced `prctl` 157 → `setsid`, `chdir` 80 → `fstat`, and `lchown` 94 → `exit_group`. stiva's own
stat reads and open flags had the same shape until 3.1.2: bare offsets and bare `O_*` numbers are
the same bug, and a lint could catch those too.

### v3.5.1 — AGNOS kernel target

Unblocked by the agnos 1.45.x ring-3 net/socket syscall surface (#45–#57, including server
`sock_listen` #56 / `sock_accept` #57).

- Build and run stiva against the agnos syscall ABI (FS-write / exec-from-disk / sockets).
- Map kavach isolation onto the agnos sandbox primitives: no Linux namespaces or seccomp there.
- Container networking over the agnos socket surface: bridge / NAT / port-map without slirp or pasta.
- Host the docker-service-sweep server-stage workloads (agora BBS, descent MUD, web server, ark+nous
  server-side) in AGNOS containers.
- Soak and weak-point sweep: connection floods, fuzzed input, resource exhaustion, including the
  kernel's 8-connection TCP and 8-listener UDP caps.

⚠ **An AGNOS-native payload cannot read its environment in a stiva container today**, and that comes
first. cyrius's `getenv` reads `/proc/self/environ` on the Linux target, `run -d` mounts no `/proc`,
and the workloads above are all cyrius-built. Filed on the agnos roadmap. The two candidate fixes are
**not** equivalent: cyrius preferring the stack `envp` fixes every consumer, and stiva wiring `mounts`
fixes only stiva. A design call is owed before either is built.

`epoll` is a portability trap here: its event struct is packed differently on x86_64 (data at +4,
stride 12) and aarch64 (+8, stride 16), and the AGNOS wrappers take a different arity (3-arg vs
Linux's 4-arg). `_exec_capture2` avoids it by sending stderr to a file rather than a second pipe.

### v3.5.2 — Windows containers

A kavach backend for Windows process isolation. Separate because it is the only item needing a
non-Linux isolation model end to end, and because `lib/syscalls_windows.cyr` stubs `sys_fork` /
`sys_execve` / `sys_waitpid` to `-1`, so the capture primitives every backend relies on do not exist
there.

---

## v3.6.0 — ecosystem

- **Kubernetes CRI shim**: a minimal CRI gRPC server wrapping stiva for k8s node integration.
- **Metrics export**: a Prometheus-compatible `/metrics` endpoint.
- **Ansamblu blue-green deploys**: deploy the new version alongside the old, swap traffic.
- **Service mesh integration**: sidecar injection for ansamblu services.
- **Fleet auto-scaling**: adjust node count from majra queue depth, over `select_migration_target`.
- **`stiva plugin` system**: loadable storage drivers, network drivers and auth providers.

---

## Known limitations

Real, understood, and deliberately lived with. Each states its cost rather than a plan, and each is
here so it can be argued with, not so it can be forgotten.

- **Exec output is truncated at the first NUL byte.** `ContainerExecResult` holds cstrs, so binary
  output from `stiva exec` stops there. Fixing it means carrying explicit lengths through the result
  struct and every consumer. Text commands are unaffected.
- **The event log's rotation is racy across processes.** `_cm_file_size` + `rotate_logs` run outside
  `file_append_locked`'s flock, so two processes crossing the threshold together can discard a
  retained generation. Closing it needs `file_append_locked` to expose its lock; a second lock here
  would invert the acquisition order against the container-log path. The live append is flocked, so
  no event is ever lost or interleaved, only an older rotated file.
- **`index.json` cannot express a digest-pinned reference.** It carries the reference in the
  `org.opencontainers.image.ref.name` annotation, which has no digest field, so after a store reload
  two digest-pinned pulls of one repository read as unpinned. Making the annotation digest-aware is
  an on-disk format change.
- **The build cache's fingerprint is metadata-only**: path, mode, uid, gid, size, nanosecond mtime.
  An edit preserving all of those will not invalidate it, and reading contents would cost as much
  as rebuilding the layer the cache exists to skip. `rm -rf {root}/cache` forces a rebuild. A tree
  deeper than 64 levels is never cached at all.
- **`logs` and the audit reader show the newest 4 MiB and 1 MiB.** A longer history stays on disk.
- **`diff` on a flattened rootfs reads every file of equal size** on both sides, so it costs about
  two reads of the image.
- **A policy carrying any resource limit fails on the OCI backend unprivileged.** runc refuses
  `linux.resources` without the cgroup delegation an unprivileged user lacks. kavach populates
  `resources` only when the policy carries a memory or pid limit, and `policy_strict` sets both,
  which is why it trips. The fix is on the kavach side: emit `resources` only when delegation is
  available, or degrade with a warning.

---

## Recording a block

Adopted 2026-08-21, after an audit found that **eight of eight "blocked on upstream" claims in the
v3.1.0 section were wrong.** Not one was caught by review; every one was caught by running
something. The rules below are mechanical on purpose: the failure was never carelessness, it was
writing a conclusion once and never re-testing it.

**A claim of a block is not prose. It is a stanza, and without one the item is not blocked, it is
unstarted, and must be filed that way.**

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

1. **`probe:` is mandatory and executable.** It exits 0 when the block has lifted, non-zero while it
   holds. Writing it forces you to state what "unblocked" looks like, the step none of the eight
   took. A probe going green is a build failure: the tree tells you the block is gone.
2. **`around-api` and `raw-syscall` may not be blank.** Both were the answer for five of the eight.
   This tree bypasses kavach for `exec` and nein for `apply`, both deliberately and both documented.
   A library boundary here is a choice, never a wall.
3. **`verified:` records the dep version the probe last ran against.** CI compares it to the tag in
   `cyrius.cyml` and fails on mismatch. Four claims went stale at one dep bump that edited exactly
   one roadmap line.
4. **Never type a number about a dependency.** Struct sizes and field counts come from a script
   reading `lib/`, or they are omitted. "8 fields / 64 bytes" was wrong the day it was written.
5. **Assert symbol absence in backticks, and expect to be checked.** CI greps `lib/` and the pinned
   snapshot for every backticked identifier in a sentence containing *no* / *neither* / *does not
   exist* / *exposes no*. A hit fails the build. This alone would have caught four, including
   `SYS_FCHOWNAT`, which the roadmap named while it sat in this repo's own `lib/`.
6. **"Verified by execution" must carry the command and its expected output.** One measurement on
   the detached path became "every stiva container runs in the HOST pid namespace." It was false for
   the runc path, and one re-run would have said so.
7. **These phrases are build errors** outside a stanza with a live probe: `cannot be fixed`, `under
   any wiring`, `no amount of`, `not on stiva effort`. At adoption there were three hits in this tree
   and two were provably false. The gate must skip lines where the phrase is quoted or negated, or it
   fires hardest on exactly the writing that fixed the problem. Match the assertion, not the string.

⚠ **Inherited oracle prose is not evidence.** A comment in `src/intents.cyr` said "The agnoshi project
does not exist yet", copied verbatim from `rust-old/`, where it was already false when the oracle
froze, and carried unread through a whole-language port. When porting a comment, port the behaviour
and re-derive the claim.
