# Quick Start

This guide covers installing stiva, pulling images, running containers, basic networking, and cleanup.

## Installation

Build from source (requires Rust 1.89+):

```bash
cargo install --path .
```

The `stiva` binary provides 28 subcommands. Run `stiva --help` for the full list.

## Pulling an Image

### CLI

```bash
stiva pull alpine:3.19
stiva pull nginx:latest
stiva images   # list local images
```

### Library

```rust
use stiva::Stiva;

let mut s = Stiva::new()?;
s.pull("alpine:3.19").await?;
let images = s.list_images()?;
```

## Running a Container

### CLI

One-shot (foreground):

```bash
stiva run alpine:3.19 echo "hello from stiva"
```

Daemon (background):

```bash
stiva run -d --name web -p 8080:80 nginx:latest
stiva ps          # list running containers
stiva logs web    # view output
```

With environment variables and secrets:

```bash
stiva run -d -e APP_ENV=prod -s DB_PASSWORD=hunter2 myapp:latest
```

Secrets are injected through kavach and never stored in the container config.

### Library

```rust
use stiva::{Stiva, ContainerConfig};

let mut s = Stiva::new()?;

let config = ContainerConfig {
    image: "nginx:latest".into(),
    detach: true,
    ports: vec!["8080:80".into()],
    ..Default::default()
};

let id = s.run(config)?;
```

## Basic Networking

Stiva creates a default bridge network (`stiva0`, subnet `172.17.0.0/16`) on startup. Containers receive an IP automatically.

Port mapping forwards host ports to container ports:

```bash
stiva run -d -p 8080:80 -p 8443:443 nginx:latest
```

See the [networking guide](networking.md) for custom networks, IPv6, and DNS.

## Inspecting and Managing

```bash
stiva inspect <id>    # detailed JSON output (includes security score)
stiva top <id>        # processes inside the container
stiva stats <id>      # CPU, memory, PID stats from cgroups v2
stiva exec <id> sh    # run a shell inside the container
```

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
