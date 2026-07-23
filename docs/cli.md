# CLI Reference

Stiva provides a `stiva` binary. Every command below is **registered** (visible in
`--help`); **21 of 35 execute end-to-end today**, and the rest print a clear "not yet
wired" message (their module logic is ported — only the wiring over the sync core
remains). The **Status** column marks each:

- **Live** — works end-to-end today.
- **v3.0.x (planned)** — buildable now (blocking, over the ported sync core), just not wired yet.
- **v3.1 (blocked)** — gated on an external landing (kavach `sandbox_spawn`, cyrius stackless coroutines).

## Global Options

| Option | Default | Description |
|--------|---------|-------------|
| `--root <PATH>` | `/var/lib/stiva` | Root directory for container **and image** data (the image store — `blobs/sha256/`, `index.json`, `oci-layout` — lives under it). Resolution order: `--root` > `$STIVA_ROOT` > the default |

## Commands

### Images

| Command | Status | Description |
|---------|--------|-------------|
| `stiva import <FILE> [NAME] [TAG]` | **Live** | Import a rootfs tar as a local OCI image — config + manifest blobs + `index.json` entry (name→`imported`, tag→`latest` if omitted) |
| `stiva images` | **Live** | List local images |
| `stiva tag <SOURCE> <TARGET>` | **Live** | Tag a local image with a new reference |
| `stiva rmi <IMAGE>` | **Live** | Remove a local image (by ID or tag) |
| `stiva pull <IMAGE>` | v3.0.x (planned) | Pull an image from a registry (over the blocking registry client) |
| `stiva push <IMAGE> [TARGET]` | v3.0.x (planned) | Push a local image to a registry |
| `stiva build [-f FILE] [-c CONTEXT]` | v3.0.x (planned) | Build from `Stivafile` (parse is done; the layer build is blocking) |

### Containers

| Command | Status | Description |
|---------|--------|-------------|
| `stiva run [--name N] [--backend B] [-w DIR] <IMAGE> [CMD...]` | **Live** | Run a container (synchronous, one-shot) |
| `stiva ps` | **Live** | List containers |
| `stiva stop <ID>` | **Live** | Stop a container (SIGTERM → SIGKILL) |
| `stiva rm <ID>` | **Live** | Remove a stopped container |
| `stiva pause <ID>` | **Live** | Pause via cgroups v2 freezer |
| `stiva unpause <ID>` | **Live** | Unpause a paused container |
| `stiva inspect <ID>` | **Live** | Inspect container or image (JSON output) |
| `stiva stats <ID>` | **Live** | Show CPU/memory/PID stats from cgroups v2 |
| `stiva logs <ID> [-n LINES]` | **Live** / v3.0.x (planned) | Show last N lines (snapshot live; `-f` poll-loop is v3.0.x) |
| `stiva export <ID> <OUTPUT.tar>` | **Live** | Export container rootfs as tar archive (two positionals; there is no `-o`) |
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
| `stiva convert <FILE> -f compose ...` | v3.0.x (planned) | Convert compose YAML (bayan ships a YAML subset; the walk is not wired yet) |
| `stiva checkpoint <ID> [--leave-running]` | v3.0.x (planned) | CRIU checkpoint a running container |
| `stiva restore <ID> <DIR>` | v3.0.x (planned) | Restore container from CRIU checkpoint |
| `stiva events` | v3.0.x (planned) | Stream container lifecycle events |
| `stiva diff <ID>` | v3.0.x (planned) | Show filesystem changes in a container vs its image |
| `stiva completions <SHELL>` | v3.0.x (planned) | Generate shell completions (bash, zsh, fish) |
| `stiva save <IMAGE> <OUTPUT.tar>` | **Live** | Save an image as an `oci-archive` tarball (skopeo/podman-compatible) |
| `stiva load <INPUT.tar>` | **Live** | Load images from an `oci-archive` tarball into the store (`docker-archive` → v3.0.3) |

## `stiva run` Flags

| Flag | Description |
|------|-------------|
| `--name <NAME>` | Container name |
| `--backend <NAME>` | Sandbox backend override (`process`, `oci`, `noop`, …) |
| `-w, --workdir <DIR>` | Working directory inside the container |
| `-d, --detach` | Registered, but refused — detached run is v3.1 (needs kavach `sandbox_spawn`) |

> These four are the **complete** set `src/main.cyr` registers for `run`. Port mapping
> (`-p`), env (`-e`) and secret injection (`-s`) are **not** wired yet — passing them is a
> usage error. They arrive with the `ContainerManager` work on the v3.0.x line.

## `stiva build` Flags

| Flag | Default | Description |
|------|---------|-------------|
| `-f, --file <PATH>` | `Stivafile` | Path to build spec |
| `-c, --context <DIR>` | `.` | Build context directory |

## `stiva convert` Flags

| Flag | Default | Description |
|------|---------|-------------|
| `-f, --format <FORMAT>` | `compose` | Input format: `dockerfile` (live) or `compose` (v3.0.x (planned)) |
| `-o, --output <PATH>` | stdout | Output file path |

> `--format dockerfile` is live (`dockerfile_to_toml`); `--format compose` is
> **v3.0.x (planned)** — no longer externally blocked. `bayan` 1.2.0 ships
> `bayan_yaml_parse` over the same tagged value graph the JSON parser uses; what
> remains is stiva-side mapping. Note it is a documented *subset*: anchors/aliases,
> merge keys (`<<:`), block scalars and multi-document input are rejected.

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
# Run a container (foreground, one-shot)
stiva run --name web nginx:latest

# Pick the sandbox backend explicitly and set a working directory
stiva run --backend oci -w /srv myapp:latest /bin/sh -c 'echo hi'

# Check status
stiva ps
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
stiva export <id> rootfs.tar
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
| `STIVA_ROOT` | Root directory for container + image data (overridden by `--root`) |

Log level is currently fixed at `INFO` (`sakshi_set_level`); there is no log-filter env
var. `RUST_LOG` was a Rust-era leftover and is not read.

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | Error (message printed to stderr) |
| N | Container exit code (for `stiva exec` and `stiva wait`) |
