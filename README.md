# Stiva

> **Stiva** (Romanian: stivă — stack, pile) — OCI-compatible container runtime for AGNOS

[![License: GPL-3.0-or-later](https://img.shields.io/badge/License-GPL--3.0--or--later-blue.svg)](LICENSE)
[![Language: Cyrius](https://img.shields.io/badge/language-Cyrius-8a2be2.svg)](#language)

Stiva is a container runtime written in **[Cyrius](#language)**, the AGNOS systems
language. It is built on [kavach](https://github.com/MacCracken/kavach) (sandbox
isolation), [majra](https://github.com/MacCracken/majra) (scheduling / pub-sub),
[nein](https://github.com/MacCracken/nein) (nftables networking), and
[bote](https://github.com/MacCracken/bote) (MCP integration).

## Status — the Rust → Cyrius port is complete; 33 of 36 CLI verbs work

Stiva was ported from Rust to Cyrius (the frozen Rust crate lives at `rust-old/` as the
parity oracle). It pulls, builds, imports, runs, execs into, inspects and removes real
containers end-to-end from the CLI, with the algorithm-dense subsystems at 85–100% parity
with the oracle. ⛔ It does not honour an image's own config yet, and a container keeps almost
every capability: read [What's next](#whats-next) before relying on it.

What that means concretely:

- **Images** — a valid **OCI image layout** (`oci-layout` + `index.json` + `blobs/sha256/`);
  `pull`/`push` against a real registry (bearer auth, multi-arch index resolution, layers
  **streamed straight to disk** with every digest verified before the blob becomes visible,
  chunked upload); `build` from a `Stivafile` with a metadata-fingerprinted layer cache (path, mode, uid, gid,
  size, ns-mtime — it never reads file contents);
  `save`/`load` as `oci-archive` (and `docker-archive` read) for Docker/Podman/skopeo interop;
  gzip + zstd layer decode with **OCI whiteouts** honoured.
- **Containers** — a stateful `ContainerManager` behind a `Stiva` facade. Foreground and
  detached (`run -d`) runs inside the container's own rootfs, `exec` via `nsenter`,
  `logs` (snapshot, `-f`, and `--scan` through kavach's externalization gate), `diff`,
  `top`, `cp`, `stats` off cgroups v2, and a CREATED→RUNNING→STOPPED lifecycle that
  publishes over majra pub/sub **and** appends to a rotated `{root}/events.jsonl` that
  `stiva events` replays.
- **Beyond one container** — `cron` scheduled containers, rootless networking
  (slirp4netns/pasta) and `convert` from a Dockerfile or a docker-compose YAML, plus library logic
  that the CLI does not reach yet: ansamblu orchestration, fleet scheduling with
  **accelerator-aware placement**, and 9 MCP tool definitions.

**2649 tests** across `tests/*.tcyr`, plus **108** CLI smoke assertions against the built
binary. Toolchain pin **6.7.18**.

Two verbs are not wired, `checkpoint` and `restore`, which need CRIU, and `restart` cannot succeed
yet. See [What's next](#whats-next).

## Quick Start

### Build

Stiva builds with the **Cyrius toolchain** (not cargo). See [CONTRIBUTING](CONTRIBUTING.md)
for the full setup.

```bash
cyrius deps                              # resolve AGNOS + stdlib deps into lib/
cyrius build src/main.cyr build/stiva    # build the stiva binary
cyrius test tests/                       # run the test suite (2649 tests)
./scripts/cli-smoke.sh                   # CLI smoke assertions (108) against the binary
```

### CLI

```bash
stiva import rootfs.tar alpine latest                # import a rootfs tar as an image (tar [name] [tag])
                                                     # -> names it local/alpine:latest
stiva run local/alpine:latest /bin/sh -c 'echo hello'     # run a container end-to-end
stiva ps -a                                          # list containers
stiva inspect <id>                                   # inspect a container (JSON)
stiva stats <id>                                     # cgroups v2 CPU/mem/PID stats
stiva logs <id>                                      # container logs
stiva stop <id> ; stiva rm <id>                      # stop + remove
stiva prune                                          # remove stopped containers + unused images
```

See [docs/cli.md](docs/cli.md) for every command, its flags, and its status.

## Live commands (33 of 36)

`run` (foreground and `-d` detached) · `ps` · `stop` · `rm` · `inspect` · `images` ·
`rmi` · `tag` · `import` · `export` · `stats` · `pause` · `unpause` · `logs`
(`-n` / `-f` / `--scan`) · `wait` · `gc` · `prune` · `info` ·
`pull` / `push` (OCI distribution, streaming + digest-verified) · `build` (Stivafile) ·
`exec` (non-interactive, via `nsenter`) · `diff` · `kill` · `rename` ·
`top` · `cp` · `cron` (`add`/`ls`/`rm`/`enable`/`disable`/`check`) ·
`completions` (bash/zsh/fish) · `convert` (Dockerfile **or** docker-compose) ·
`save` / `load` (oci-archive; `load` also reads docker-archive) ·
`events` (lifecycle stream over a persisted JSONL log).

`checkpoint` and `restore` are registered (visible in `--help`) but print a clear "not yet wired"
message: they need CRIU, which is roadmap v3.4.1. `restart` cannot succeed yet, because the runtime
spec lives only in the process that created the container (v3.2.1). `pause` and `update` refuse a
container that shares its cgroup, which is nearly all of them until v3.2.1.

## Capabilities

| Category | Live | Next |
|----------|------|--------------------|
| **Images** | `pull`/`push` (bearer auth, multi-arch resolution, streamed + digest-verified layers, chunked upload); `build` from a Stivafile with a metadata-fingerprinted layer cache; import, tag, list, rmi, gc, export; **OCI image-layout store** (oci-layout + index.json + blobs); **`save`/`load` as oci-archive** + **`docker-archive` read**; blob integrity verify | honouring the image's own config (v3.2.0); private registries and concurrent layer downloads (v3.3.0); `run` steps and multi-stage builds (v3.3.2) |
| **Containers** | **`ContainerManager` + `Stiva` facade**; run (foreground **and `-d` detached**, inside the container rootfs), non-interactive `exec` via `nsenter`, `diff`, ps, stop, kill, rm, inspect, stats, pause/unpause, logs (snapshot, `-f`, `--scan`), wait, top, cp, rename — all routed through the manager (`container_manager_update` is library-only; there is no `stiva update` verb), with **lifecycle events over majra pub/sub *and* a rotated `{root}/events.jsonl`** behind `stiva events`; state persistence | manageable foreground containers, `restart`, per-container cgroups (v3.2.1); `-e`/`-v`/`-p`/`--user` and the rest of `run`'s flags (v3.2.2); `exec -it` (v3.4.0); CRIU `checkpoint`/`restore` (v3.4.1) |
| **Networking** | bridge/NAT/DNS/IP-pool/port-map/policy logic (IPv4 + IPv6 dual-stack); **rootless networking** — slirp4netns/pasta spawn + port forwarding | attaching any of it to a container (v3.3.1): nothing constructs a `NetworkManager` |
| **Storage** | overlay FS, **gzip + zstd layer unpack** with **OCI whiteouts** applied, cgroups v2 (CPU/mem/PID/IO); **perms-preserving tar** (mode/uid/gid + dir/symlink, GNU longname, base-256; traversal/symlink/DoS-hardened) | volume mounts — `RuntimeSpec.mounts` is assembled but never applied, and `run` has no `-v` flag (v3.2.2) |
| **Orchestration** | TOML ansamblu parse, DAG ordering, health-check / restart-policy / rolling-update / scaling logic; fleet scheduling (spread / bin-pack / pinned) with **accelerator-aware placement**; **`stiva cron`** scheduled containers | a verb that runs an ansamblu, and health and restart policies applied (v3.3.2) |
| **Security** | seccomp and NO_NEW_PRIVS on `run`, on both backends, and on `run -d` (⛔ no filter on `exec`, no Landlock anywhere, and an OCI container keeps every capability but CAP_SYS_ADMIN: see [SECURITY.md](SECURITY.md)), fd cleanup, strength scoring; **`scan_output` secret/PII scanning** via `stiva logs --scan` (the per-container `scan_policy` field round-trips through `state.json`, but nothing sets it yet — `--scan` is the only way to turn scanning on; a `--scan-policy` producer is v3.2.2) | seccomp on the foreground path (kavach); **secret injection** and a non-root `user` (v3.2.2); a device-allocation ledger (v3.4.2) |
| **Integration** | 9 MCP tool definitions with dispatch for 7 (`stiva_run` is unwired and `stiva_ansamblu` does nothing), and no server yet, so no client can reach them; **`stiva events`** over the persisted lifecycle log; **`convert`** from a Dockerfile **or a docker-compose YAML** (documented subset); **accelerator inventory** in `stiva info` | an MCP server (v3.3.3); daimon agent registration (v3.3.2); `parse_intent` is slated for **removal** (v3.3.3) — agnoshi calls the stiva CLI, not the reverse |

## <a name="whats-next"></a>What's next

Every remaining item belongs to a release, and where one needs something from another repo, that
prerequisite is a named work item. [docs/development/roadmap.md](docs/development/roadmap.md) is the
full picture; the shape of it:

- **v3.2 — one container, done properly.** Honour the image's Entrypoint, Cmd, Env, WorkingDir and
  User. Make foreground containers manageable, add per-container cgroups, and make `restart` work.
  Then mounts, a real `user`, secrets, and the rest of `run`'s flags.
- **v3.3 — beyond one container.** Private registries and parallel pulls, networking attached to
  containers, ansamblu and health driven for real, and an MCP server.
- **v3.4 — interactive, mobile, hardware.** `exec -it`, CRIU, device passthrough, encrypted storage.
- **v3.5 — other platforms.** aarch64 in CI, the AGNOS kernel target, Windows containers.
- **v3.6 — ecosystem.** CRI, metrics, deploy strategies, plugins.

Five limitations are worth reading before you rely on stiva:
**on the default path a container keeps every capability but CAP_SYS_ADMIN, and `/proc` is
unmasked** (a kavach change; see [SECURITY.md](SECURITY.md)); **an image's Entrypoint and Env are ignored** (v3.2.0); **every container
runs as root and gets no secrets or mounts** (v3.2.2); **only x86_64 is built and tested in CI**; and
the **cycc struct-id ↔ SIMD-sentinel miscompile was last verified live at 6.4.78 and is
un-re-verified at the current 6.7.18 pin**, which is why several hot paths use raw-offset accessors
instead of typed field access.

## Known limitations

See [the roadmap's Known limitations section](docs/development/roadmap.md#known-limitations)
for the maintained list.

## <a name="language"></a>Language

Stiva is written in **Cyrius**, the AGNOS systems language, and built with the `cyrius`
toolchain (toolchain pin **6.7.18**). It consumes its AGNOS dependencies as Cyrius
single-file `dist/*.cyr` bundles (sigil, kavach, majra, nein, bote-core, cmdit, samay,
ai-hwaccel, sakshi, libro), wired **by git tag** in [`cyrius.cyml`](cyrius.cyml). Stiva is
itself consumable as a single-file bundle, `dist/stiva.cyr` (built by `cyrius distlib`).

## Documentation

| Document | Description |
|----------|-------------|
| [ADRs](docs/adr/) | Architecture decision records (12 decisions) |
| [Architecture](docs/architecture.md) | System design, module map |
| [CLI Reference](docs/cli.md) | Every command, its flags, and its status |
| [Roadmap](docs/development/roadmap.md) | What is left, by release (v3.2 → v3.6), and known limitations |
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
cyrius test tests/                             # 2649 tests
./scripts/cli-smoke.sh                         # 108 CLI smoke assertions
cyrius bench tests/stiva.bcyr                   # benchmarks
cyrius fmt src/main.cyr --check                # format check (per file)
cyrius lint src/main.cyr                       # lint (per file)
cyrius audit                                   # project sweep: fmt/lint/docs/tests/bench
```

## License

GPL-3.0-or-later — see [LICENSE](LICENSE).
