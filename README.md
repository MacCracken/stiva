# Stiva

> **Stiva** (Romanian: stivă — stack, pile) — OCI-compatible container runtime for AGNOS

[![License: GPL-3.0-or-later](https://img.shields.io/badge/License-GPL--3.0--or--later-blue.svg)](LICENSE)
[![Language: Cyrius](https://img.shields.io/badge/language-Cyrius-8a2be2.svg)](#language)

Stiva is a container runtime written in **[Cyrius](#language)**, the AGNOS systems
language. It is built on [kavach](https://github.com/MacCracken/kavach) (sandbox
isolation), [majra](https://github.com/MacCracken/majra) (scheduling / pub-sub),
[nein](https://github.com/MacCracken/nein) (nftables networking), and
[bote](https://github.com/MacCracken/bote) (MCP integration).

## Status — v3.0.5: single-node OCI runtime (group A complete)

Stiva was ported from Rust to Cyrius (the frozen Rust crate lives at `rust-old/` as
the parity oracle). **v3.0.0 was a working single-node OCI runtime**; **v3.0.1–v3.0.4
completed group A — the OCI image-layout + transfer surface**. The local store is now a
valid **OCI image layout** (`oci-layout` + `index.json` + `blobs/sha256/`, the ad-hoc
`images.json` retired), with a perms-preserving tar codec and `save`/`load` as
`oci-archive` (plus `docker-archive` read) — Docker/Podman/skopeo-interop. **v3.0.5 adds
zstd layer decode**, closing the compressed-layer media types the OCI image-spec defines,
alongside a dependency-hygiene pass and a toolchain bump to cyrius 6.4.71. It imports,
runs, and manages real containers end-to-end via a CLI, with the algorithm-dense
subsystems at 85–100% parity with the Rust oracle. The runtime is single-node
run-to-completion; the remaining surface is synchronous/blocking work over the ported
sync core — the **v3.0.x line** (buildable now, just not wired yet, e.g. the registry
`pull`/`push` client), with a small externally-blocked residue in **v3.1**
(see [below](#whats-next)). **1196 tests** across `tests/*.tcyr`.

## Quick Start

### Build

Stiva builds with the **Cyrius toolchain** (not cargo). See [CONTRIBUTING](CONTRIBUTING.md)
for the full setup.

```bash
cyrius deps                              # resolve AGNOS + stdlib deps into lib/
cyrius build src/main.cyr build/stiva    # build the stiva binary
cyrius tests tests/                      # run the test suite (1184 tests)
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

See [docs/cli.md](docs/cli.md) for every command, including which are live, which
are planned for the v3.0.x line, and which are the blocked v3.1 residue.

## Live commands (21 of 35)

`run` · `ps` · `stop` · `rm` · `inspect` · `images` · `rmi` · `tag` · `import` ·
`export` · `stats` · `pause` · `unpause` · `logs` · `wait` · `gc` · `prune` · `info` ·
`convert` (Dockerfile → Stivafile) · `save` / `load` (oci-archive; `load` also reads
docker-archive).

Every other verb (14) is registered (visible in `--help`) but prints a clear
"not yet wired" message — its module logic is ported. Most such verbs are planned
for the v3.0.x line (blocking glue over the sync core: `pull`/`push`, `build`, `exec`,
`top`/`cp`/`kill`, `checkpoint`/`restore`, `restart`/`rename`, `events`/`diff`,
`completions`); only a small residue is blocked on external landings (v3.1).

## Capabilities

| Category | Live (v3.0.5) | v3.0.x (planned) | v3.1 (blocked) |
|----------|---------------|------------------|----------------|
| **Images** | import, tag, list, rmi, gc, export; **OCI image-layout store** (oci-layout + index.json + blobs); **`save`/`load` as oci-archive** + **`docker-archive` read**; per-image platform passthrough; blob integrity verify; Stivafile parse + build-cache key | registry pull/push over HTTP (blocking client), full multi-stage build layers | — |
| **Containers** | run (foreground), ps, stop, rm, inspect, stats, pause/unpause, logs (snapshot), wait; state persistence | non-interactive `exec` (nsenter), `restart`, `rename`, streaming `logs -f`, CRIU checkpoint/restore, top/cp wiring; ContainerManager + Stiva facade | detached `run -d` (needs kavach sandbox_spawn), interactive `exec -it` (needs cyrius coroutines) |
| **Networking** | bridge/NAT/DNS/IP-pool/port-map/policy logic (IPv4 + IPv6 dual-stack) | live network attach on the run path | — |
| **Storage** | overlay FS, volume mounts, **gzip + zstd layer unpack**, cgroups v2 (CPU/mem/PID/IO); **perms-preserving tar** (mode/uid/gid + dir/symlink, GNU longname, base-256; traversal/symlink/DoS-hardened) | — | — |
| **Orchestration** | TOML ansamblu parse, DAG ordering, health-check / restart-policy / rolling-update / scaling logic | live deploy/scale driving the Stiva facade | — |
| **Security** | rootless mapping, seccomp/Landlock policy, NO_NEW_PRIVS, fd cleanup, credential store, strength scoring | `scan_output` secret/PII scan over exec+logs output (kavach's ExternalizationGate surface is available) | — |
| **Integration** | 9 MCP tool definitions + 2 sync tool handlers (build/ansamblu), lifecycle-event model | live MCP dispatch (ps/stop/inspect/pull/push/exec), daimon agent | MCP handle_run (needs `run -d`) |

## <a name="whats-next"></a>What's next: v3.0.x and v3.1

**Group A (OCI image-layout + transfer) is complete** (v3.0.1–v3.0.4). The remaining
surface is not an async rewrite — the runtime is single-node run-to-completion and the
async substrate already exists, so it is blocking glue over the ported sync core (the
**v3.0.x line**), buildable now, just not wired yet. Next up, roughly in order:

- **Group B — blocking registry client**: `acquire_token`/`authenticated_request` →
  `fetch_manifest`/`fetch_blob` → live `stiva pull`, then `push_blob`/`push_manifest`
  → `stiva push`, writing into the group-A layout (over `sandhi`/`tls_native` + bayan).
- **`ContainerManager` + `Stiva` facade**, non-interactive `exec`/CRIU flows, streaming
  `logs -f`, full multi-stage `build`, and MCP dispatch.

The stdlib pieces (JSON/TOML via `bayan`, HTTP/TLS via `sandhi`/`tls_native`,
gzip/zstd/xz/lz4 via `sankoch`) already exist.

Only a small residue is genuinely **v3.1**, each gated on an external landing: detached
`run -d` (kavach `sandbox_spawn`), interactive `exec -it` and true multiplexed streaming
(cyrius stackless coroutines), and MCP `handle_run` (needs `run -d`).
`intents.parse_intent` awaits the (nonexistent) agnoshi NL parser.

Three items previously listed here have since landed upstream and moved back onto the
**v3.0.x** line: **zstd** layer decode (shipped in v3.0.5), `convert compose` (bayan gained
a YAML subset), and `scan_output` (kavach's ExternalizationGate surface is available).

See [docs/development/roadmap.md](docs/development/roadmap.md) for the full parity snapshot.

## <a name="language"></a>Language

Stiva is written in **Cyrius**, the AGNOS systems language, and built with the `cyrius`
toolchain (toolchain pin **6.4.71**). It consumes its AGNOS dependencies as Cyrius
single-file `dist/*.cyr` bundles (kavach, majra, nein, bote, agnodrm) wired in
[`cyrius.cyml`](cyrius.cyml). Stiva is itself consumable as a single-file bundle,
`dist/stiva.cyr` (built by `cyrius distlib`).

## Documentation

| Document | Description |
|----------|-------------|
| [ADRs](docs/adr/) | Architecture decision records (11 decisions) |
| [Architecture](docs/architecture.md) | System design, module map |
| [CLI Reference](docs/cli.md) | Every command; live / v3.0.x (planned) / v3.1 (blocked) status |
| [Roadmap](docs/development/roadmap.md) | Port status, parity snapshot, v3.0.x + v3.1 split |
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
cyrius tests tests/                            # 1184 tests
cyrius bench tests/stiva.bcyr                   # benchmarks
cyrius fmt src/main.cyr --check                # format check (per file)
cyrius lint src/main.cyr                       # lint (per file)
cyrius audit                                   # project sweep: fmt/lint/docs/tests/bench
```

## License

GPL-3.0-or-later — see [LICENSE](LICENSE).
