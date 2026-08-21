# Stiva

> **Stiva** (Romanian: stivă — stack, pile) — OCI-compatible container runtime for AGNOS

[![License: GPL-3.0-or-later](https://img.shields.io/badge/License-GPL--3.0--or--later-blue.svg)](LICENSE)
[![Language: Cyrius](https://img.shields.io/badge/language-Cyrius-8a2be2.svg)](#language)

Stiva is a container runtime written in **[Cyrius](#language)**, the AGNOS systems
language. It is built on [kavach](https://github.com/MacCracken/kavach) (sandbox
isolation), [majra](https://github.com/MacCracken/majra) (scheduling / pub-sub),
[nein](https://github.com/MacCracken/nein) (nftables networking), and
[bote](https://github.com/MacCracken/bote) (MCP integration).

## Status — the Rust → Cyrius port is complete; 34 of 36 CLI verbs are live

Stiva was ported from Rust to Cyrius (the frozen Rust crate lives at `rust-old/` as the
parity oracle). It pulls, builds, imports, runs, execs into, inspects and removes real
containers end-to-end from the CLI, with the algorithm-dense subsystems at 85–100% parity
with the oracle.

What that means concretely:

- **Images** — a valid **OCI image layout** (`oci-layout` + `index.json` + `blobs/sha256/`);
  `pull`/`push` against a real registry (bearer auth, multi-arch index resolution, layers
  **streamed straight to disk** with every digest verified before the blob becomes visible,
  chunked upload); `build` from a `Stivafile` with a content-fingerprinted layer cache;
  `save`/`load` as `oci-archive` (and `docker-archive` read) for Docker/Podman/skopeo interop;
  gzip + zstd layer decode with **OCI whiteouts** honoured.
- **Containers** — a stateful `ContainerManager` behind a `Stiva` facade. Foreground and
  detached (`run -d`) runs inside the container's own rootfs, `exec` via `nsenter`,
  `logs` (snapshot, `-f`, and `--scan` through kavach's externalization gate), `diff`,
  `top`, `cp`, `stats` off cgroups v2, and a CREATED→RUNNING→STOPPED lifecycle that
  publishes over majra pub/sub **and** appends to a rotated `{root}/events.jsonl` that
  `stiva events` replays.
- **Beyond one container** — ansamblu orchestration (DAG ordering, rolling updates,
  scaling), fleet scheduling with **accelerator-aware placement**, `cron` scheduled
  containers, rootless networking (slirp4netns/pasta), 9 MCP tools with live dispatch,
  and `convert` from a Dockerfile or a docker-compose YAML.

**2175 tests** across `tests/*.tcyr`, plus **87** CLI smoke assertions against the built
binary. Toolchain pin **6.5.33**.

Two verbs are not wired: `checkpoint` and `restore`, both gated on CRIU integration and
scheduled for v3.1 — see [What's next](#whats-next).

## Quick Start

### Build

Stiva builds with the **Cyrius toolchain** (not cargo). See [CONTRIBUTING](CONTRIBUTING.md)
for the full setup.

```bash
cyrius deps                              # resolve AGNOS + stdlib deps into lib/
cyrius build src/main.cyr build/stiva    # build the stiva binary
cyrius tests tests/                      # run the test suite (2175 tests)
./scripts/cli-smoke.sh                   # CLI smoke assertions (87) against the binary
```

### CLI

```bash
stiva import rootfs.tar alpine latest                # import a rootfs tar as an image (tar [name] [tag])
stiva run alpine:latest /bin/sh -c 'echo hello'      # run a container end-to-end
stiva ps                                             # list containers
stiva inspect <id>                                   # inspect a container (JSON)
stiva stats <id>                                     # cgroups v2 CPU/mem/PID stats
stiva logs <id>                                      # container logs
stiva stop <id> ; stiva rm <id>                      # stop + remove
stiva prune                                          # remove stopped containers + unused images
```

See [docs/cli.md](docs/cli.md) for every command, its flags, and its status.

## Live commands (34 of 36)

`run` (foreground and `-d` detached) · `ps` · `stop` · `rm` · `inspect` · `images` ·
`rmi` · `tag` · `import` · `export` · `stats` · `pause` · `unpause` · `logs`
(`-n` / `-f` / `--scan`) · `wait` · `gc` · `prune` · `info` ·
`pull` / `push` (OCI distribution, streaming + digest-verified) · `build` (Stivafile) ·
`exec` (non-interactive, via `nsenter`) · `diff` · `kill` · `restart` · `rename` ·
`top` · `cp` · `cron` (`add`/`ls`/`rm`/`enable`/`disable`/`check`) ·
`completions` (bash/zsh/fish) · `convert` (Dockerfile **or** docker-compose) ·
`save` / `load` (oci-archive; `load` also reads docker-archive) ·
`events` (lifecycle stream over a persisted JSONL log).

The other two — `checkpoint` and `restore` — are registered (visible in `--help`) but
print a clear "not yet wired" message. Their module logic is ported; what they need is
CRIU integration, scheduled for v3.1.

## Capabilities

| Category | Live | Next (v3.1 → v3.3) |
|----------|------|--------------------|
| **Images** | `pull`/`push` (bearer auth, multi-arch resolution, streamed + digest-verified layers, chunked upload); `build` from a Stivafile with a content-fingerprinted layer cache; import, tag, list, rmi, gc, export; **OCI image-layout store** (oci-layout + index.json + blobs); **`save`/`load` as oci-archive** + **`docker-archive` read**; blob integrity verify | concurrent layer downloads (v3.1); `run` steps and multi-stage `from_stage` builds (v3.3) |
| **Containers** | **`ContainerManager` + `Stiva` facade**; run (foreground **and `-d` detached**, inside the container rootfs), non-interactive `exec` via `nsenter`, `diff`, ps, stop, kill, rm, inspect, stats, pause/unpause, logs (snapshot, `-f`, `--scan`), wait, top, cp, rename, restart, update — all routed through the manager, with **lifecycle events over majra pub/sub *and* a rotated `{root}/events.jsonl`** behind `stiva events`; state persistence | interactive `exec -it` (needs cyrius stackless coroutines); CRIU `checkpoint`/`restore` (v3.1) |
| **Networking** | bridge/NAT/DNS/IP-pool/port-map/policy logic (IPv4 + IPv6 dual-stack); **rootless networking** — slirp4netns/pasta spawn + port forwarding | §J live network attach on the run path (v3.1) |
| **Storage** | overlay FS, volume mounts, **gzip + zstd layer unpack** with **OCI whiteouts** applied, cgroups v2 (CPU/mem/PID/IO); **perms-preserving tar** (mode/uid/gid + dir/symlink, GNU longname, base-256; traversal/symlink/DoS-hardened) | — |
| **Orchestration** | TOML ansamblu parse, DAG ordering, health-check / restart-policy / rolling-update / scaling logic; fleet scheduling (spread / bin-pack / pinned) with **accelerator-aware placement**; **`stiva cron`** scheduled containers | live deploy/scale driving the Stiva facade (v3.3) |
| **Security** | rootless mapping, seccomp/Landlock policy, NO_NEW_PRIVS, fd cleanup, credential store, strength scoring; **`scan_output` secret/PII scanning** via `stiva logs --scan`, with per-container `scan_policy` persisted in `state.json` (container exec output is gated by kavach automatically) | **secret injection** into containers (v3.1 — `secrets` still serializes empty); a device-allocation ledger (v3.1) |
| **Integration** | 9 MCP tool definitions + **live dispatch** (ps/stop/inspect/pull/push/build/ansamblu) and MCP resources; **`stiva events`** over the persisted lifecycle log; **`convert`** from a Dockerfile **or a docker-compose YAML** (documented subset); **accelerator inventory** in `stiva info` | MCP `handle_run`; daimon agent registration; `parse_intent` (needs the agnoshi NL parser) |

## <a name="whats-next"></a>What's next

The single-node runtime is finished. Every remaining item belongs to a release, and where
one needs something from another repo, that prerequisite is a named work item — nothing is
parked for being hard. [docs/development/roadmap.md](docs/development/roadmap.md) is the
full picture; the shape of it:

- **v3.1 — secrets, interactivity, mobility.** Secret injection (containers currently get
  none: `secrets` serializes as an empty array and `build_sandbox` has no kavach setter to
  thread one through), interactive `exec -it` over cyrius stackless coroutines, CRIU
  `checkpoint`/`restore`, §J device + accelerator passthrough with an allocation ledger,
  richer kavach error detail, and concurrent layer downloads.
- **v3.2 — non-x86.** aarch64 first (one upstream landing away), then the AGNOS kernel target.
- **v3.3 — orchestration surface.** Live deploy/scale driving the facade; `run` and
  `from_stage` build steps.
- **v3.4 — Windows containers.**

Three known limitations are worth reading before you rely on stiva:
**only x86_64 works**; **containers have no secrets** (v3.1 item 1); and the **cycc
struct-id ↔ SIMD-sentinel miscompile was last verified live at 6.4.78 and is un-re-verified
at the current 6.5.33 pin**, which is why several hot
paths use raw-offset accessors instead of typed field access.

## Known limitations

See [the roadmap's Known limitations section](docs/development/roadmap.md#known-limitations)
for the maintained list.

## <a name="language"></a>Language

Stiva is written in **Cyrius**, the AGNOS systems language, and built with the `cyrius`
toolchain (toolchain pin **6.5.33**). It consumes its AGNOS dependencies as Cyrius
single-file `dist/*.cyr` bundles (kavach, majra, nein, bote, agnodrm, cmdit, samay,
ai-hwaccel, sakshi, libro), wired **by git tag** in [`cyrius.cyml`](cyrius.cyml). Stiva is
itself consumable as a single-file bundle, `dist/stiva.cyr` (built by `cyrius distlib`).

## Documentation

| Document | Description |
|----------|-------------|
| [ADRs](docs/adr/) | Architecture decision records (12 decisions) |
| [Architecture](docs/architecture.md) | System design, module map |
| [CLI Reference](docs/cli.md) | Every command, its flags, and its status |
| [Roadmap](docs/development/roadmap.md) | What is left, by release (v3.1 → v3.4), and known limitations |
| [Quick Start](docs/guides/quick-start.md) | Getting started guide |
| [Networking](docs/guides/networking.md) | Network configuration guide |
| [Security](docs/guides/security.md) | Security hardening guide |
| [Testing Guide](docs/development/testing.md) | Test organization + coverage |
| [Scripts](docs/development/scripts.md) | Benchmark and version scripts |
| [Spec Compliance](docs/spec-compliance.md) | OCI, MCP, CRIU spec conformance |
| [Changelog](CHANGELOG.md) | Release history |
| [Contributing](CONTRIBUTING.md) | Contribution guidelines |
| [Security Policy](SECURITY.md) | Security policy |

## Development

```bash
cyrius build src/main.cyr build/stiva          # build
cyrius tests tests/                            # 2175 tests
./scripts/cli-smoke.sh                         # 87 CLI smoke assertions
cyrius bench tests/stiva.bcyr                   # benchmarks
cyrius fmt src/main.cyr --check                # format check (per file)
cyrius lint src/main.cyr                       # lint (per file)
cyrius audit                                   # project sweep: fmt/lint/docs/tests/bench
```

## License

GPL-3.0-or-later — see [LICENSE](LICENSE).
