# CLI Reference

Stiva provides a `stiva` binary. Every command below is **registered** (visible in
`--help`); **29 of 35 execute end-to-end today**, and the rest print a clear "not yet
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
| `stiva pull <IMAGE>` | **Live** | Pull from a registry. Resolves multi-arch indexes to this platform, streams each layer straight to disk, and verifies every digest before the blob becomes visible. Prints the image ID |
| `stiva push <IMAGE> [TARGET]` | **Live** | Push a local image to a registry. Uploads the config, then every layer, then the manifest — a manifest is only valid once everything it references is present. `TARGET` re-tags on the way out; omitted, it pushes to the image's own reference |
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
| `stiva logs <ID> [-n LINES] [--scan]` | **Live** / v3.0.x (planned) | Show last N lines (snapshot live; `-f` poll-loop is v3.0.x). `--scan` runs the output through kavach's externalization gate first |
| `stiva export <ID> <OUTPUT.tar>` | **Live** | Export container rootfs as tar archive (two positionals; there is no `-o`) |
| `stiva wait <ID>` | **Live** | Wait for container to exit, return exit code |
| `stiva run <IMAGE> -d ...` | v3.1 (blocked) | Detached `run -d` (needs kavach sandbox_spawn) |
| `stiva exec <ID> <CMD...>` | v3.0.x (planned) / v3.1 (blocked) | Execute command in a running container (nsenter; non-interactive is v3.0.x, `-it` needs cyrius stackless coroutines) |
| `stiva top <ID>` | **Live** | List processes inside a running container (`/proc` walk over the descendants of the container PID) |
| `stiva cp <SRC> <DST>` | **Live** | Copy files host↔container. Exactly one side is `<container>:<path>`; both or neither is refused rather than guessed |
| `stiva kill <ID> [-s SIGNAL]` | **Live** | Send a signal (number, 1–64, default 15 = SIGTERM). Requires a running container |
| `stiva restart <ID>` | **Live** | Restart a container (stop then start; needs the runtime spec from the creating process) |
| `stiva rename <ID> <NAME>` | **Live** | Rename a container |

### Operations

| Command | Status | Description |
|---------|--------|-------------|
| `stiva prune` | **Live** | Remove stopped containers and unused images |
| `stiva gc` | **Live** | Garbage-collect unreferenced image blobs |
| `stiva info` | **Live** | Show system information and security score |
| `stiva convert <FILE> -f dockerfile [-o OUT]` | **Live** | Convert a Dockerfile to a `Stivafile` |
| `stiva convert <FILE> -f compose ...` | **Live** | Convert docker-compose YAML to a compose TOML (documented YAML subset — see below) |
| `stiva checkpoint <ID> [--leave-running]` | v3.0.x (planned) | CRIU checkpoint a running container |
| `stiva restore <ID> <DIR>` | v3.0.x (planned) | Restore container from CRIU checkpoint |
| `stiva events` | v3.0.x (planned) | Stream container lifecycle events |
| `stiva diff <ID>` | v3.0.x (planned) | Show filesystem changes in a container vs its image |
| `stiva completions <SHELL>` | **Live** | Generate shell completions (bash, zsh, fish) on **stdout**, from cmdit's own verb table — so the script cannot drift from the CLI |
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
| `-f, --format <FORMAT>` | `compose` | Input format: `dockerfile` or `compose` — both live |
| `-o, --output <PATH>` | stdout | Output file path |

Both formats are live as of v3.0.6. Input is capped at 1 MiB; a larger file is
rejected rather than silently truncated.

### The compose YAML subset

`--format compose` is built on `bayan`'s YAML parser, which implements a subset of
YAML 1.2. Everything docker-compose files normally use works: nested block mappings,
block sequences, flow sequences (`[a, b]`), quoted and unquoted scalars, comments,
CRLF, a BOM, and a single leading `---`.

These are **rejected with an explicit error**, never silently mis-parsed:

| Construct | Example |
|---|---|
| Flow mappings | `a: {image: x}` |
| Block scalars | `command: \|` |
| Anchors / aliases | `&base` / `*base` |
| Merge keys | `<<: *base` |
| Tags | `!!str x` |
| Multi-document | a second `---` |
| Tab indentation | `\ta:` |
| Complex keys | `? key` |
| Escaped quotes in a quoted scalar | `"say \"hi\""` |

Anchors and merge keys are the notable loss against the Rust original, which used
`serde-saphyr` and *resolved* them before conversion. A compose file built around
`<<: *defaults` must be flattened first. Escaped quotes are one of two cases where
this port is **stricter and more correct** than the original, which accepted them and
then emitted invalid TOML.

Duplicate mapping keys are also rejected (`invalid YAML: duplicate mapping key: <k>`),
matching the original. bayan itself accepts them, keeping both pairs — which would have
produced two `[services.a]` tables, i.e. TOML no parser can load.

### Two divergences from the original worth knowing

**Output is escaped; the original's was not.** Values go out as TOML basic strings with
`"`, `\` and control characters escaped, and section names are quoted unless they are
bare-safe (`[A-Za-z0-9_-]+`). The Rust original emitted everything through a bare
`format!`, which let a hostile compose file close the string it was written into and
open a fabricated table — so the file a human reviewed and the Stivafile stiva later
loaded could differ. Since `convert` exists to ingest third-party files, that is a
structure-forgery hole rather than a formatting quirk, and this port does not inherit it.

**YAML 1.1 boolean tokens are resolved.** `yes`, `no`, `on`, `off`, `y`, `n`, `True`,
`TRUE`, `False`, `FALSE`, `Null` and `NULL` are treated as booleans/null, matching
`serde-saphyr`. This is not cosmetic: `restart: no` is compose's documented default, and
the original emits **no** `restart` line for it because the value is not a string. A
converter that left these as strings would silently add fields the original dropped, and
keep array elements it discards. Quote them (`restart: "no"`) to mean the literal text.

Two known gaps, both in bayan's scalar resolver rather than in the conversion: an integer
above `i64::MAX` wraps negative instead of round-tripping, and non-decimal or underscored
numeric spellings (`0x1f`, `1_000`, `007`) stay strings.

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
