# CLI Reference

Stiva provides a `stiva` binary. Every command below is **registered** (visible in
`--help`), but in **v3.0.0** only the synchronous subset executes end-to-end; the rest
print a clear "not yet wired" message (their module logic is ported — only the
wiring over the sync core remains). The **Status** column marks each:

- **Live** — works end-to-end in v3.0.0.
- **v3.0.x (planned)** — buildable now (blocking, over the ported sync core), just not wired yet.
- **v3.1 (blocked)** — gated on an external landing (kavach spawn/gate, cyrius coroutines, bayan YAML, sankoch zstd).

## Global Options

| Option | Default | Description |
|--------|---------|-------------|
| `--root <PATH>` | `/var/lib/agnos/containers` | Container data directory |
| `--images <PATH>` | `/var/lib/agnos/images` | Image storage directory |

## Commands

### Images

| Command | Status | Description |
|---------|--------|-------------|
| `stiva import <FILE> [NAME] [TAG]` | **Live** | Import tar archive as a local image (name→`imported`, tag→`latest` if omitted) |
| `stiva images` | **Live** | List local images |
| `stiva tag <SOURCE> <TARGET>` | **Live** | Tag a local image with a new reference |
| `stiva rmi <IMAGE>` | **Live** | Remove a local image (by ID or tag) |
| `stiva pull <IMAGE>` | v3.0.x (planned) | Pull an image from a registry (over the blocking registry client) |
| `stiva push <IMAGE> [TARGET]` | v3.0.x (planned) | Push a local image to a registry |
| `stiva build [-f FILE] [-c CONTEXT]` | v3.0.x (planned) | Build from `Stivafile` (parse is done; the layer build is blocking) |

### Containers

| Command | Status | Description |
|---------|--------|-------------|
| `stiva run <IMAGE> [-p PORT] [-e ENV] [-s SECRET] [CMD...]` | **Live** | Run a container (synchronous, one-shot) |
| `stiva ps` | **Live** | List containers |
| `stiva stop <ID>` | **Live** | Stop a container (SIGTERM → SIGKILL) |
| `stiva rm <ID>` | **Live** | Remove a stopped container |
| `stiva pause <ID>` | **Live** | Pause via cgroups v2 freezer |
| `stiva unpause <ID>` | **Live** | Unpause a paused container |
| `stiva inspect <ID>` | **Live** | Inspect container or image (JSON output) |
| `stiva stats <ID>` | **Live** | Show CPU/memory/PID stats from cgroups v2 |
| `stiva logs <ID> [-n LINES]` | **Live** / v3.0.x (planned) | Show last N lines (snapshot live; `-f` poll-loop is v3.0.x) |
| `stiva export <ID> -o FILE` | **Live** | Export container rootfs as tar archive |
| `stiva wait <ID>` | **Live** | Wait for container to exit, return exit code |
| `stiva run <IMAGE> -d ...` | v3.1 (blocked) | Detached `run -d` (needs kavach sandbox_spawn) |
| `stiva exec <ID> <CMD...>` | v3.0.x (planned) / v3.1 (blocked) | Execute command in a running container (nsenter; non-interactive is v3.0.x, `-it` needs cyrius stackless coroutines) |
| `stiva top <ID>` | v3.0.x (planned) | List processes inside a running container (`/proc` walk is ported) |
| `stiva cp <SRC> <DST>` | v3.0.x (planned) | Copy files host↔container (the copy logic is ported) |
| `stiva kill <ID> [-s SIGNAL]` | v3.0.x (planned) | Send a signal (needs a live PID) |
| `stiva restart <ID>` | v3.0.x (planned) | Restart a container |
| `stiva rename <ID> <NAME>` | v3.0.x (planned) | Rename a container |

### Operations

| Command | Status | Description |
|---------|--------|-------------|
| `stiva prune` | **Live** | Remove stopped containers and unused images |
| `stiva gc` | **Live** | Garbage-collect unreferenced image blobs |
| `stiva info` | **Live** | Show system information and security score |
| `stiva convert <FILE> -f dockerfile [-o OUT]` | **Live** | Convert a Dockerfile to a `Stivafile` |
| `stiva convert <FILE> -f compose ...` | v3.1 (blocked) | Convert compose YAML (needs a bayan YAML parser) |
| `stiva checkpoint <ID> [--leave-running]` | v3.0.x (planned) | CRIU checkpoint a running container |
| `stiva restore <ID> <DIR>` | v3.0.x (planned) | Restore container from CRIU checkpoint |
| `stiva events` | v3.0.x (planned) | Stream container lifecycle events |
| `stiva diff <ID>` | v3.0.x (planned) | Show filesystem changes in a container vs its image |
| `stiva completions <SHELL>` | v3.0.x (planned) | Generate shell completions (bash, zsh, fish) |

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
| `-f, --format <FORMAT>` | `compose` | Input format: `dockerfile` (live) or `compose` (v3.1 (blocked) — needs a bayan YAML parser) |
| `-o, --output <PATH>` | stdout | Output file path |

> In v3.0.0, `--format dockerfile` is live (`dockerfile_to_toml`); `--format compose`
> is v3.1 (blocked), pending a bayan YAML parser.

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
stiva import rootfs.tar imported v1

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
