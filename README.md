# Stiva

> **Stiva** (Romanian: stivă — stack, pile) — OCI-compatible container runtime for AGNOS

[![License: GPL-3.0-or-later](https://img.shields.io/badge/License-GPL--3.0--or--later-blue.svg)](LICENSE)
[![Language: Cyrius](https://img.shields.io/badge/language-Cyrius-8a2be2.svg)](#language)

Stiva is a container runtime written in **[Cyrius](#language)**, the AGNOS systems
language. It is built on [kavach](https://github.com/MacCracken/kavach) (sandbox
isolation), [majra](https://github.com/MacCracken/majra) (scheduling / pub-sub),
[nein](https://github.com/MacCracken/nein) (nftables networking), and
[bote](https://github.com/MacCracken/bote) (MCP integration).

## Status — v3.0.0: synchronous single-node runtime

Stiva was ported from Rust to Cyrius (the frozen Rust crate lives at `rust-old/` as
the parity oracle). **v3.0.0 is a working single-node OCI runtime**: it imports, runs,
and manages real containers end-to-end via a CLI, with the algorithm-dense subsystems
at 85–100% parity with the Rust oracle. The port stops cleanly at the sync/async
boundary — the remaining async container-orchestration surface is the **v3.1** milestone
(see [below](#whats-v31)). **1033 tests** across `tests/*.tcyr`.

## Quick Start

### Build

Stiva builds with the **Cyrius toolchain** (not cargo). See [CONTRIBUTING](CONTRIBUTING.md)
for the full setup.

```bash
cyrius deps                              # resolve AGNOS + stdlib deps into lib/
cyrius build src/main.cyr build/stiva    # build the stiva binary
cyrius tests tests/                      # run the test suite (1033 tests)
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

See [docs/cli.md](docs/cli.md) for every command, including which are live in v3.0.0
and which are deferred to v3.1.

## Live commands (v3.0.0)

`run` · `ps` · `stop` · `rm` · `inspect` · `images` · `rmi` · `tag` · `import` ·
`export` · `stats` · `pause` · `unpause` · `logs` · `wait` · `gc` · `prune` · `info` ·
`convert` (Dockerfile → Stivafile).

Every other verb is registered (visible in `--help`) but prints a clear
"deferred to v3.1" message — its module logic is ported; only the async execution
path remains.

## Capabilities

| Category | v3.0.0 (live) | v3.1 (async milestone) |
|----------|---------------|------------------------|
| **Images** | import, tag, list, rmi, gc, export; content-addressable store; blob integrity verify; Stivafile parse + build-cache key | registry pull/push over HTTP, full multi-stage build layers |
| **Containers** | run (synchronous), ps, stop, rm, inspect, stats, pause/unpause, logs, wait; state persistence | detached `run -d`, `exec` (nsenter), `restart`, streaming `logs -f`, CRIU checkpoint/restore, top/cp wiring |
| **Networking** | bridge/NAT/DNS/IP-pool/port-map/policy logic (IPv4 + IPv6 dual-stack) | live network attach on the async run path |
| **Storage** | overlay FS, volume mounts, gzip layer unpack, cgroups v2 (CPU/mem/PID/IO), USTAR tar writer | zstd layers |
| **Orchestration** | TOML ansamblu parse, DAG ordering, health-check / restart-policy / rolling-update / scaling logic | live deploy/scale driving the async facade |
| **Security** | rootless mapping, seccomp/Landlock policy, NO_NEW_PRIVS, fd cleanup, credential store, strength scoring | — |
| **Integration** | 9 MCP tool definitions + 2 sync tool handlers (build/ansamblu), lifecycle-event model | live MCP dispatch over the async runtime, daimon agent |

## <a name="whats-v31"></a>What's v3.1

The one remaining architectural band is ~async: mapping the Rust tokio-shaped async onto
Cyrius `lib/async.cyr` (cooperative futures). Landing it unblocks the async `Stiva` facade
+ `ContainerManager`, the registry HTTP client, detached/`exec`/CRIU flows, streaming logs,
and live MCP dispatch. The stdlib pieces (JSON/TOML via `bayan`, HTTP/TLS via
`sandhi`/`tls_native`, gzip/xz/lz4 via `sankoch`, async via `lib/async.cyr`) already
exist — the narrow genuine gaps are **zstd** (sankoch) and a **YAML** parser (bayan, for
`convert compose`). `intents.parse_intent` awaits the (nonexistent) agnoshi NL parser.

See [docs/development/roadmap.md](docs/development/roadmap.md) for the full parity snapshot.

## <a name="language"></a>Language

Stiva is written in **Cyrius**, the AGNOS systems language, and built with the `cyrius`
toolchain (toolchain pin **6.4.66**). It consumes its AGNOS dependencies as Cyrius
single-file `dist/*.cyr` bundles (kavach, majra, nein, bote, agnodrm) wired in
[`cyrius.cyml`](cyrius.cyml). Stiva is itself consumable as a single-file bundle,
`dist/stiva.cyr` (built by `cyrius distlib`).

## Documentation

| Document | Description |
|----------|-------------|
| [ADRs](docs/adr/) | Architecture decision records (11 decisions) |
| [Architecture](docs/architecture.md) | System design, module map |
| [CLI Reference](docs/cli.md) | Every command; live-vs-v3.1 status |
| [Roadmap](docs/development/roadmap.md) | Port status, parity snapshot, v3.1 milestone |
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
cyrius tests tests/                            # 1033 tests
cyrius bench tests/stiva.bcyr                   # benchmarks
cyrius fmt src/main.cyr --check                # format check (per file)
cyrius lint src/main.cyr                       # lint (per file)
cyrius audit                                   # project sweep: fmt/lint/docs/tests/bench
```

## License

GPL-3.0-or-later — see [LICENSE](LICENSE).
