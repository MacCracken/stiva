# Quick Start

This guide covers installing stiva, importing images, running containers, basic networking, and cleanup.

## Installation

Build from source with the cyrius toolchain (pin 6.4.66):

```bash
cyrius build src/main.cyr build/stiva
```

The `stiva` binary provides 19 subcommands. Run `stiva --help` for the full list.

## Importing an Image

Registry pull over HTTP (`stiva pull`) is a v3.0.x (planned) feature. In v3.0.0, load images
from a local OCI archive with `import`:

### CLI

```bash
stiva import alpine.tar alpine 3.19   # import <tarfile> [name] [tag]
stiva images                          # list local images
```

> **v3.0.x (planned)** — `stiva pull alpine:3.19` (registry pull/push over HTTP) is
> registered but not yet wired; it is buildable now over the ported sync core.

## Running a Container

### CLI

One-shot (foreground):

```bash
stiva run alpine:3.19 echo "hello from stiva"
stiva ps          # list containers
stiva logs <id>   # view output
```

With environment variables and secrets:

```bash
stiva run -e APP_ENV=prod -s DB_PASSWORD=hunter2 myapp:latest
```

Secrets are injected through kavach and never stored in the container config.

> **Milestones** — streaming logs (`logs -f`) and the `Stiva` library facade are
> **v3.0.x (planned)** — blocking glue over the sync core, buildable now but not yet
> wired. Detached/background runs (`run -d`) stay **v3.1 (blocked)**, gated on kavach
> `sandbox_spawn`. In v3.0.0 `run` executes in the foreground.

## Basic Networking

Stiva creates a default bridge network (`stiva0`, subnet `172.17.0.0/16`) on startup. Containers receive an IP automatically.

Port mapping forwards host ports to container ports:

```bash
stiva run -p 8080:80 -p 8443:443 nginx:latest
```

See the [networking guide](networking.md) for custom networks, IPv6, and DNS.

## Inspecting and Managing

```bash
stiva inspect <id>    # detailed JSON output (includes security score)
stiva stats <id>      # CPU, memory, PID stats from cgroups v2
```

> **v3.0.x (planned)** — `stiva top <id>` (processes inside the container) and
> non-interactive `stiva exec <id> <cmd>` (via nsenter) are buildable now over the
> sync core, not yet wired. Interactive `exec -it` (a live shell) stays **v3.1
> (blocked)**, gated on cyrius stackless coroutines.

## Building Images

Create a `Stivafile` (TOML format):

```toml
[image]
base = "alpine:3.19"
name = "myapp"
tag = "v1.0"

[[steps]]
type = "run"
command = ["apk", "add", "--no-cache", "curl"]

[[steps]]
type = "copy"
source = "./app"
destination = "/app"

[config]
entrypoint = ["/app/start.sh"]
```

```bash
stiva build
stiva build -f Stivafile -c ./project
```

> **v3.0.x (planned)** — the Stivafile parser is implemented in v3.0.0; running a layer
> build (`stiva build`) is buildable now over the sync core but not yet wired.

## Cleanup

```bash
stiva stop <id>       # graceful stop (SIGTERM then SIGKILL)
stiva rm <id>         # remove a stopped container
stiva rmi alpine:3.19 # remove an image
stiva prune           # remove all stopped containers and unused images
```

## Next Steps

- [Networking guide](networking.md) -- custom networks, IPv6, DNS
- [Security guide](security.md) -- rootless, seccomp, Landlock, secrets
- [CLI reference](../cli.md) -- full command list
