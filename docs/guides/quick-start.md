# Quick Start

This guide covers installing stiva, getting images, running containers, basic networking, and cleanup.

## Installation

Build from source with the cyrius toolchain (pin 6.4.78):

```bash
cyrius deps
cyrius build src/main.cyr build/stiva
```

The `stiva` binary registers 36 subcommands, 34 of which run end-to-end. Run
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
stiva run alpine:3.19 echo "hello from stiva"
stiva ps          # list containers
stiva logs <id>   # view output
stiva logs -f <id>   # follow until the container stops
```

Detached:

```bash
stiva run -d alpine:3.19 sleep 3600   # prints the container id and returns
```

The complete set of flags `run` accepts is `--name`, `--backend`, `-w/--workdir` and
`-d/--detach`.

> **Not yet wired.** Environment variables (`-e`), port mapping (`-p`) and secret
> injection (`-s`) are **not** accepted by `run` — passing them is a usage error, not a
> silent no-op. Containers currently receive no secrets at all: `secrets` serializes as an
> empty array and `build_sandbox` has no kavach setter to thread one through. That is
> roadmap v3.1.0 item 1; port mapping arrives with §J live network attach, also v3.1.
> To set environment variables today, use `stiva exec -e K=V` against a running container,
> or bake them into the image with a `Stivafile` `env` step.

## Working Inside a Container

```bash
stiva exec <id> ls /app            # run a command inside (non-interactive, via nsenter)
stiva exec -e K=V -w /app <id> ./run.sh
stiva top <id>                     # processes inside the container
stiva diff <id>                    # filesystem changes vs the image
stiva cp ./local.txt <id>:/app/    # copy in; reverse the operands to copy out
```

`exec` keeps stdout and stderr separate and makes the command's exit code stiva's.

> **`exec -it` is not offered.** There is no pty helper anywhere in the dependency tree, so
> the interactive half has no substrate. It is roadmap v3.1.0 item 2, over cyrius stackless
> coroutines.

## Inspecting and Managing

```bash
stiva inspect <id>    # detailed JSON output (includes security score)
stiva stats <id>      # CPU, memory, PID stats from cgroups v2
stiva events -f       # follow the lifecycle event stream
stiva info            # host info, security score, accelerator inventory
```

## Basic Networking

Stiva creates a default bridge network (`stiva0`, subnet `172.17.0.0/16`); containers
receive an IP automatically. Rootless containers get slirp4netns or pasta instead.

See the [networking guide](networking.md) for custom networks, IPv6, and DNS.

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
`copy` step, and writes a full OCI config + manifest + `index.json` entry. The layer cache
key covers the **content** of the source tree, not just its path — changing a file in the
context invalidates the layer.

> **`run` and `from_stage` steps are refused**, loudly, rather than silently skipped —
> executing build steps needs a sandbox per step, and multi-stage needs stage-scoped
> layer sets. Both are roadmap v3.3.

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
stiva rmi alpine:3.19 # remove an image
stiva gc              # garbage-collect unreferenced blobs
stiva prune           # remove all stopped containers and unused images
```

## Next Steps

- [Networking guide](networking.md) -- custom networks, IPv6, DNS
- [Security guide](security.md) -- rootless, seccomp, Landlock, output scanning
- [CLI reference](../cli.md) -- full command list
