# Quick Start

This guide covers installing stiva, getting images, running containers, basic networking, and cleanup.

## Installation

Build from source with the cyrius toolchain (pin 6.7.18):

```bash
cyrius deps
cyrius build src/main.cyr build/stiva
```

The `stiva` binary registers 36 subcommands, 33 of which run end-to-end. Run
`stiva --help` for the full list, or see the [CLI reference](../cli.md).

## Getting an Image

Three ways in, all live:

```bash
stiva pull alpine:3.19                # from a registry
stiva load alpine.tar                 # from an oci-archive (or a `docker save` tarball)
stiva import rootfs.tar alpine 3.19   # a rootfs tar as a single-layer image
stiva images                          # list what you have
```

`pull` resolves multi-arch indexes to this platform, streams each layer straight to disk,
and verifies every digest **before** the blob becomes visible in the store.

## Running a Container

Foreground (one-shot):

```bash
stiva run docker.io/library/alpine:3.19 /bin/echo "hello from stiva"
stiva ps -a          # list containers, including finished ones
stiva logs <id>      # view output
stiva logs -f <id>   # follow, until the log has been quiet for 2 s
```

Detached:

```bash
stiva run -d docker.io/library/alpine:3.19 /bin/sleep 3600   # prints the container id and returns
```

A container runs what its image says, so `stiva run alpine:3.19` needs no command, and an argument
replaces the image's `Cmd`, not its `Entrypoint`. A name resolves in one way everywhere: `alpine:3.19`
is `docker.io/library/alpine:3.19` when you have it, and otherwise `local/alpine:3.19`.

⚠ An image's `User` is not applied yet, so every container runs as root (roadmap v3.2.2). A foreground
container's output is scanned: what the scan flags is printed with a warning on stderr, and a
credential it knows is replaced with `[REDACTED:…]` (see `docs/guides/security.md`).

The complete set of flags `run` accepts is `--name`, `--backend`, `-w/--workdir` and
`-d/--detach`, and they come before the image.

> **Not yet wired.** Environment variables (`-e`), port mapping (`-p`), volumes (`-v`) and
> secrets are **not** accepted by `run`, and passing them is a usage error, not a silent
> no-op. All are roadmap v3.2.2. To set an environment variable today, use `stiva exec -e K=V`
> against a running container. A `Stivafile` `env` step is recorded in the image, but nothing reads
> it when a container starts.

## Working Inside a Container

```bash
stiva exec <id> ls /app            # run a command inside (non-interactive, via nsenter)
stiva exec -e K=V -w /app <id> ./run.sh
stiva top <id>                     # processes inside the container
stiva diff <id>                    # filesystem changes vs the image
stiva cp ./local.txt <id>:/app/    # copy in; reverse the operands to copy out
```

`exec` keeps stdout and stderr separate and makes the command's exit code stiva's.

> **`exec -it` is not offered.** It needs a pty helper, which is roadmap v3.4.0.

## Inspecting and Managing

```bash
stiva inspect <id>    # detailed JSON output (no security score — that is `stiva info`)
stiva stats <id>      # CPU, memory, PID stats from its cgroup
stiva events -f       # follow the lifecycle event stream
stiva info            # host info, security score, accelerator inventory
```

## Basic Networking

⛔ A container gets no bridge and no address yet: the network stack is a library with no caller
(roadmap v3.3.1). What does work is slirp4netns or pasta for a rootless `run -d`, when either is
installed.

See the [networking guide](networking.md).

## Building Images

Create a `Stivafile` (TOML format):

```toml
[image]
base = "alpine:3.19"
name = "myapp"
tag = "v1.0"

[[steps]]
type = "copy"
source = "./app"
destination = "/app"

[[steps]]
type = "env"
key = "APP_ENV"
value = "prod"

[[steps]]
type = "workdir"
path = "/app"

[config]
entrypoint = ["/app/start.sh"]
```

```bash
stiva build
stiva build -f Stivafile -c ./project
```

`build` resolves the base locally first (then the registry), produces one gzip layer per
`copy` step, and writes a full OCI config + manifest + `index.json` entry. The layer cache key
covers each source file's **metadata** (path, mode, owner, size, modification time), so an edit
that preserves all of those does not invalidate the layer. `rm -rf {root}/cache` forces a rebuild.

> **`run` and `from_stage` steps are refused**, loudly, rather than silently skipped —
> executing build steps needs a sandbox per step, and multi-stage needs stage-scoped
> layer sets. Both are roadmap v3.3.2.

## Scheduled Containers

```bash
stiva cron add --schedule "0 3 * * *" nightly backup:latest /usr/bin/backup.sh
stiva cron ls
stiva cron check    # fires whatever is due; drive this from a systemd timer
```

Stiva has no daemon, so `cron check` is what makes jobs fire. See
[the CLI reference](../cli.md#stiva-cron).

## Cleanup

```bash
stiva stop <id>       # graceful stop (SIGTERM then SIGKILL)
stiva rm <id>         # remove a stopped container
stiva rmi docker.io/library/alpine:3.19 # remove an image
stiva gc              # garbage-collect unreferenced blobs
stiva prune           # remove all stopped containers and unused images
```

## Next Steps

- [Networking guide](networking.md) -- rootless networking, and the library underneath
- [Security guide](security.md) -- what is enforced, rootless, output scanning
- [CLI reference](../cli.md) -- full command list
