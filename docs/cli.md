# CLI Reference

Stiva provides a `stiva` binary with 34 subcommands.

## Global Options

| Option | Default | Description |
|--------|---------|-------------|
| `--root <PATH>` | `/var/lib/agnos/containers` | Container data directory |
| `--images <PATH>` | `/var/lib/agnos/images` | Image storage directory |

## Commands

### Images

| Command | Description |
|---------|-------------|
| `stiva pull <IMAGE>` | Pull an image from a registry |
| `stiva push <IMAGE> [TARGET]` | Push a local image to a registry |
| `stiva build [-f FILE] [-c CONTEXT]` | Build image from `Stivafile` (default: `./Stivafile`) |
| `stiva images` | List local images |
| `stiva rmi <IMAGE>` | Remove a local image (by ID or tag) |
| `stiva tag <SOURCE> <TARGET>` | Tag a local image with a new reference |
| `stiva import <FILE> --name NAME [--tag TAG]` | Import tar archive as a local image |

### Containers

| Command | Description |
|---------|-------------|
| `stiva run <IMAGE> [-d] [-p PORT] [-e ENV] [-s SECRET] [CMD...]` | Run a container |
| `stiva ps` | List containers |
| `stiva stop <ID>` | Stop a container (SIGTERM → SIGKILL) |
| `stiva rm <ID>` | Remove a stopped container |
| `stiva restart <ID>` | Restart a stopped container |
| `stiva exec <ID> <CMD...>` | Execute command in a running container |
| `stiva kill <ID> [-s SIGNAL]` | Send signal (default: 15/SIGTERM) |
| `stiva pause <ID>` | Pause via cgroups v2 freezer |
| `stiva unpause <ID>` | Unpause a paused container |
| `stiva inspect <ID>` | Inspect container or image (JSON output) |
| `stiva top <ID>` | List processes inside a running container |
| `stiva stats <ID>` | Show CPU/memory/PID stats from cgroups v2 |
| `stiva logs <ID> [-n LINES]` | Show last N lines of container logs |
| `stiva export <ID> -o FILE` | Export container rootfs as tar archive |
| `stiva cp <SRC> <DST>` | Copy files between host and container |
| `stiva wait <ID>` | Wait for container to exit, return exit code |

### Operations

| Command | Description |
|---------|-------------|
| `stiva prune` | Remove stopped containers and unused images |
| `stiva checkpoint <ID> [--leave-running]` | CRIU checkpoint a running container |
| `stiva restore <ID> <DIR>` | Restore container from CRIU checkpoint |
| `stiva convert <FILE> [-f FORMAT] [-o OUT]` | Convert YAML to TOML (compose or dockerfile) |
| `stiva rename <ID> <NAME>` | Rename a container |
| `stiva gc` | Garbage-collect unreferenced image blobs and layers |
| `stiva events` | Stream container lifecycle events in real time |
| `stiva diff <ID>` | Show filesystem changes in a container vs its image |
| `stiva completions <SHELL>` | Generate shell completions (bash, zsh, fish) |
| `stiva info` | Show system information and security score |

## `stiva run` Flags

| Flag | Description |
|------|-------------|
| `-d, --detach` | Run as daemon (return immediately) |
| `-p, --port <HOST:CONTAINER>` | Port mapping (repeatable) |
| `-e, --env <KEY=VALUE>` | Environment variable (repeatable) |
| `-s, --secret <KEY=VALUE>` | Secret injection via kavach (repeatable, not stored in config) |
| `--name <NAME>` | Container name |

## `stiva build` Flags

| Flag | Default | Description |
|------|---------|-------------|
| `-f, --file <PATH>` | `Stivafile` | Path to build spec |
| `-c, --context <DIR>` | `.` | Build context directory |

## `stiva convert` Flags

| Flag | Default | Description |
|------|---------|-------------|
| `-f, --format <FORMAT>` | `compose` | Input format: `compose` or `dockerfile` |
| `-o, --output <PATH>` | stdout | Output file path |

## `Stivafile` Format

`Stivafile` is stiva's build spec — a TOML file (like Dockerfile, but typed and validated):

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

[[steps]]
type = "env"
key = "PORT"
value = "8080"

[[steps]]
type = "workdir"
path = "/app"

[config]
entrypoint = ["/app/start.sh"]
expose = [8080]
user = "nobody"
```

### Step Types

| Type | Fields | Description |
|------|--------|-------------|
| `run` | `command: [String]` | Execute a command |
| `copy` | `source`, `destination` | Copy from build context |
| `env` | `key`, `value` | Set environment variable |
| `workdir` | `path` | Set working directory |
| `label` | `key`, `value` | Add metadata label |
| `from_stage` | `stage`, `source`, `destination` | Copy from named build stage (multi-stage) |

## Examples

```bash
# Pull and run a daemon
stiva pull nginx:latest
stiva run -d -p 8080:80 nginx:latest

# Run with secrets (injected via kavach, not stored in config)
stiva run -d -s DB_PASSWORD=secret123 -e DB_HOST=localhost myapp:latest

# Check status
stiva ps
stiva top <id>
stiva stats <id>

# Execute inside running container
stiva exec <id> ls /etc/nginx

# Stop, restart, remove
stiva stop <id>
stiva restart <id>
stiva rm <id>
stiva prune

# Build from Stivafile
stiva build
stiva build -f Stivafile -c ./project

# Push to registry
stiva push myapp:latest registry.example.com/myapp:latest

# Export/import
stiva export <id> -o rootfs.tar
stiva import rootfs.tar --name imported --tag v1

# Copy files in/out
stiva cp ./config.toml <id>:/etc/app/
stiva cp <id>:/var/log/app.log ./app.log

# Convert from Docker formats
stiva convert docker-compose.yml -f compose
stiva convert docker-compose.yml -f compose -o ansamblu.toml
stiva convert Dockerfile -f dockerfile -o Stivafile

# System info
stiva info
```

## Environment Variables

| Variable | Description |
|----------|-------------|
| `RUST_LOG` | Tracing filter (e.g., `stiva=debug`, `warn`) |

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | Error (message printed to stderr) |
| N | Container exit code (for `stiva exec` and `stiva wait`) |
