# CLI Reference

Stiva provides a `stiva` binary. Every command below is **registered** (visible in
`--help`); **34 of 36 execute end-to-end today**. The **Status** column marks each:

- **Live** — works end-to-end today.
- **v3.1 (planned)** — registered but prints a clear "not yet wired" message. Only
  `checkpoint` and `restore` are in this state; their module logic is ported and what
  they need is CRIU integration (roadmap v3.1.0 item 3).

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
| `stiva build [-f FILE] [-c CONTEXT]` | **Live** | Build an OCI image from a `Stivafile`. Resolves the base locally first (then the registry), builds one gzip layer per `copy` step, and writes a full OCI config + manifest + `index.json` entry. Prints the image ID. `run` and `from_stage` steps are refused — see below |

### Containers

| Command | Status | Description |
|---------|--------|-------------|
| `stiva run [--name N] [--backend B] [-w DIR] <IMAGE> [CMD...]` | **Live** | Run a container (synchronous, one-shot) |
| `stiva ps [-a]` | **Live** | List **running** containers; `-a`/`--all` includes stopped ones |
| `stiva stop <ID>` | **Live** | Stop a container (SIGTERM → SIGKILL) |
| `stiva rm <ID>` | **Live** | Remove a stopped container |
| `stiva pause <ID>` | **Live** | Pause via cgroups v2 freezer |
| `stiva unpause <ID>` | **Live** | Unpause a paused container |
| `stiva inspect <ID>` | **Live** | Inspect a **container** (JSON output). Images are not inspectable — the lookup is container-only; use `stiva images` for the store index |
| `stiva stats <ID>` | **Live** | Show CPU/memory/PID stats from cgroups v2 |
| `stiva logs <ID> [-n LINES] [-f] [--scan]` | **Live** | Show last N lines, or `-f` to follow, which returns once the log has been **quiet for 2 s** (`_CLI_FOLLOW_QUIET_MS`) — **not** when the container stops, so a `run -d` container that pauses output for longer ends the follow (polls the log file; the lifecycle bus is process-local). `--scan` runs the output through kavach's externalization gate first; it cannot be combined with `-f` |
| `stiva export <ID> <OUTPUT.tar>` | **Live** | Export container rootfs as tar archive (two positionals; there is no `-o`) |
| `stiva wait <ID>` | **Live** | Wait for container to exit, return exit code |
| `stiva run -d <IMAGE> [CMD...]` | **Live** | Detached run over kavach 3.9.0 `sandbox_spawn`. Returns the container id immediately; state survives into other stiva processes, and `stop` signals the recorded pid when the in-memory handle is gone. **See the rootfs caveat below.** |
| `stiva exec [-e K=V]... [-w DIR] <CONTAINER> <CMD> [ARGS...]` | **Live** | Run a command inside a running container via `nsenter`. stdout and stderr stay separate, and the command's exit code becomes stiva's. Non-interactive only — `-i`/`-t` are not offered |
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
| `stiva checkpoint <ID> [--leave-running]` | v3.1 (planned) | CRIU checkpoint a running container |
| `stiva restore <ID> <DIR>` | v3.1 (planned) | Restore container from CRIU checkpoint |
| `stiva cron <SUBCOMMAND> ...` | **Live** | Schedule containers. See [`stiva cron`](#stiva-cron) |
| `stiva events [--since SEC] [--until SEC] [-n N] [-f]` | **Live** | Replay container lifecycle events from `{root}/events.jsonl` (one JSON object per line, printed verbatim so `\| jq` works). Without `-f` it dumps and exits; `-f` follows until `-n`, `--until`, or an interrupt. See [Lifecycle events](#lifecycle-events) |
| `stiva diff <ID>` | **Live** | Show filesystem changes in a container vs its image, one `<status> <path>` line per change (`A` added, `C` changed, `D` deleted). Handles both rootfs layouts — over an overlay the changed set is `{croot}/upper`, with overlayfs whiteouts (char 0:0) and opaque dirs read as deletions; a flattened rootfs (flagged by `{croot}/.rootfs-flattened`) is compared against the layer dirs. "no changes" goes to **stderr**, so `stiva diff <ID> \| wc -l` counts changes |
| `stiva completions <SHELL>` | **Live** | Generate shell completions (bash, zsh, fish) on **stdout**, from cmdit's own verb table — so the script cannot drift from the CLI |
| `stiva save <IMAGE> <OUTPUT.tar>` | **Live** | Save an image as an `oci-archive` tarball (skopeo/podman-compatible) |
| `stiva load <INPUT.tar>` | **Live** | Load images from an `oci-archive` tarball into the store (`docker-archive` → v3.0.3) |

## `stiva run` Flags

| Flag | Description |
|------|-------------|
| `--name <NAME>` | Container name |
| `--backend <NAME>` | Sandbox backend override (`process`, `oci`, `noop`, …) |
| `-w, --workdir <DIR>` | ⛔ **Accepted and silently ignored.** Registered, stored on the spec, and never read — `build_sandbox` does not call `config_workdir` and the OCI spec hardcodes `"cwd":"/"`. Roadmap v3.1.0 item 10 |
| `-d, --detach` | Run detached over kavach `sandbox_spawn`; prints the container id and returns |

> These four are the **complete** set `src/main.cyr` registers for `run`. Port mapping
> (`-p`), env (`-e`) and secret injection (`-s`) are **not** wired — passing them is a
> usage error. Secret injection is roadmap v3.1.0 item 1 (containers currently get no
> secrets at all: `secrets` serializes as an empty array and `build_sandbox` has no kavach
> setter to thread one through); port mapping arrives with §J live network attach, also v3.1.

## `stiva exec` Flags

| Flag | Default | Description |
|------|---------|-------------|
| `-e, --env <K=V>` | — | Environment variable, **repeatable**. A bare `NAME` is a usage error |
| `-w, --workdir <DIR>` | the container's cwd | Working directory inside the container |

> **Rootless containers work, with a caveat.** When the container owns its own user namespace,
> `exec` enters the **mount** and **user** namespaces (plus the container's root) but not the
> network, UTS or IPC ones — joining those needs `CAP_SYS_ADMIN` in the owning user namespace,
> which is exactly what an unprivileged exec cannot have. A command that inspects the container's
> network from inside `exec` therefore sees the host's. Privileged containers get the full set.
>
> **`exec -it` is not offered.** There is no pty helper in the dependency tree at all, so the
> interactive half has no substrate — it is not merely waiting on the async work.

## `stiva build` Flags

| Flag | Default | Description |
|------|---------|-------------|
| `-f, --file <PATH>` | `Stivafile` | Path to build spec |
| `-c, --context <DIR>` | `.` | Build context directory |

## <a name="stiva-cron"></a>`stiva cron`

Scheduled containers, over samay's cron engine. Entries live in `{root}/cron.json`.

| Subcommand | Description |
|------------|-------------|
| `cron add --schedule <EXPR> <NAME> <IMAGE> [CMD...]` | Register a job. `EXPR` is a 5-field cron expression or an `@shortcut` (`@daily`, `@hourly`, …); an invalid one is rejected at add time |
| `cron ls` | List jobs — `NAME`, `SCHEDULE`, `IMAGE`, `ENABLED`, tab-separated |
| `cron rm <NAME>` | Remove a job |
| `cron enable <NAME>` / `cron disable <NAME>` | Toggle a job without deleting it |
| `cron check` | Fire whatever is due **right now**, then exit |

`stiva` is a one-shot dispatcher with no daemon, so **nothing fires unless something calls
`cron check`**. Drive it from a systemd timer (or any external timer) at whatever
granularity your shortest schedule needs:

```bash
stiva cron add --schedule "0 3 * * *" nightly-backup backup:latest /usr/bin/backup.sh
stiva cron check   # run this on a timer
```

Two semantics worth knowing:

- **Missed schedules are skipped, not caught up** (`CRON_SKIP`). A machine that was off for
  six hours starts an hourly job **once** on the next `check`, not sixty times. samay logs
  the drop either way.
- **`check` persists the advanced anchors before starting anything.** If a container start
  crashes the process, the alternative would be re-firing the same job on every subsequent
  tick forever.

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
| `copy` | `source`, `destination` | Copy from the build context. Produces one gzip layer. `source` must be relative to the context and free of `..` |
| `env` | `key`, `value` | Set environment variable |
| `workdir` | `path` | Set working directory (a `[config].workdir` overrides it) |
| `label` | `key`, `value` | Add metadata label |
| `run` | `command: [String]` | Parsed, but **refused by `build`** — see below |
| `from_stage` | `stage`, `source`, `destination` | Parsed, but **refused by `build`** — see below |

> **`run` and `from_stage` are refused rather than faked.** Both parse, so an existing `Stivafile`
> still validates, but `stiva build` fails on them with a reason instead of producing an image.
>
> The Rust oracle's `run` step does not execute anything: it writes a marker file
> `.stiva/run/<idx>.cmd` containing the argv, and its own doc comment calls that a placeholder.
> Emitting that layer would produce an image asserting a command ran when none did — a lie the user
> cannot see in the output. Its `from_stage` copies from `<context>/<stage_name>` if that directory
> happens to exist and skips silently otherwise, and the `stages` table it implies is parsed and then
> never read. Executing `run` steps for real needs an exec-into-an-intermediate-rootfs design, not
> just an exec primitive.

## Examples

```bash
# Run a container (foreground, one-shot)
stiva run --name web nginx:latest

# Pick the sandbox backend explicitly and set a working directory
stiva run --backend oci myapp:latest /bin/echo hi

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
| `STIVA_ROOTFS_FALLBACK` | `copy` (default) or `none`. How `run`/`create` build the container rootfs when the overlay mount is unavailable — see below |
| `STIVA_EVENTS` | `on` (default) or `off`. Whether lifecycle events are persisted to `{root}/events.jsonl` — see [Lifecycle events](#lifecycle-events) |
| `STIVA_EVENTS_MAX_BYTES` | `8388608` (8 MiB). Rotate the event log once a write would push it past this size |
| `STIVA_EVENTS_MAX_FILES` | `3`. How many rotated generations to keep |

### `STIVA_ROOTFS_FALLBACK`

Mounting an overlay needs `CAP_SYS_ADMIN` and an overlayfs-capable kernel. Without them — the
normal case for an unprivileged user — stiva falls back to a plain directory at
`{root}/containers/<id>/rootfs` and, by default (`copy`), **flattens the image layers into it**
bottom-to-top so the container has the filesystem its image describes. Modes and symlinks are
preserved; the copy is reclaimed by `stiva rm` with the rest of the container directory.

Set `none` to skip that copy and leave the directory empty. It saves one copy of the image per
container, and it is the right choice only if nothing will actually run against the rootfs — a
`create` you never start, or a host that mounts the overlay itself. A container started against
an empty rootfs cannot find its own entrypoint.

Log level is currently fixed at `INFO` (`sakshi_set_level`); there is no log-filter env
var. `RUST_LOG` was a Rust-era leftover and is not read.

## Lifecycle events

Every container state change publishes an event to the in-process majra hub **and** appends it to
`{root}/events.jsonl`. `stiva events` reads that file.

The file is what makes the verb work at all. The hub is a `pubsub_new()` owned by one
`ContainerManager` and is never persisted, and every CLI invocation is a separate process building
its own manager — so a subscriber can never observe a publish from anywhere else. The file is
observable across processes; the hub is not.

One JSON object per line, printed **verbatim** so `stiva events | jq` works:

```bash
stiva events | jq -r '"\(.ts) \(.event) \(.container_id)"'
```

```json
{"ts":1785021407505,"event":"created","container_id":"3f38…","image":"local/demo:v1"}
{"ts":1785021407517,"event":"started","container_id":"3f38…","detach":"false"}
{"ts":1785021407680,"event":"stopped","container_id":"3f38…","exit_code":0}
{"ts":1785021407691,"event":"removed","container_id":"3f38…","state":"removed"}
```

`ts` is epoch **milliseconds**; `--since` / `--until` take Unix **seconds** (what `date +%s`
gives) and are inclusive at both ends. Hints and warnings go to stderr, so redirecting stdout gives
a clean stream.

### What terminates `stiva events`

Unlike `logs -f` there is no single container whose state ends the stream, so the plain form is
bounded by default:

| Form | Terminates when |
|---|---|
| `stiva events` | Immediately after dumping the matching events |
| `stiva events -f -n 5` | 5 events have been printed |
| `stiva events -f --until $(date +%s)` | The wall clock passes that second |
| `stiva events -f` | The operator interrupts it (`Ctrl+C`), like `docker events` |

An inverted window (`--since` after `--until`) is a **usage error**, not an empty stream — zero
events looks exactly like "the runtime did nothing". A line whose `ts` will not parse is always
shown rather than filtered out; an event stream is an audit surface, and hiding a malformed record
hides the evidence. A log that ends in a torn record skips it and says so on stderr.

### Rotation

The log rotates like a container log — `events.jsonl` → `.1` → … → `.N`, oldest dropped — at
8 MiB × 3 generations by default (`STIVA_EVENTS_MAX_BYTES` / `STIVA_EVENTS_MAX_FILES`). Rotated
generations are **not** read back by `stiva events`; it reads the live file only, so `--since`
cannot reach past the last rotation. A follower tracks the log's inode, so a rotation under it
resumes at the new file rather than stalling.

Persistence costs about **7.8 µs** per event — roughly 31 µs over a container's whole lifecycle,
against the ~1.3 ms one `flatten_layers` call costs on the same create. Set `STIVA_EVENTS=off` to
skip it entirely; `stiva events` then reports that no events are recorded.

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | Error (message printed to stderr) |
| N | Container exit code (for `stiva exec` and `stiva wait`) |

## Container filesystem

As of **v3.0.14** (kavach 3.9.1) `stiva run` executes the command **inside the container's
rootfs**. A binary present only in the image runs; a missing one reports a real failure instead of
exiting 0.

Backend notes: with `runc`/`crun` installed the OCI backend is selected and the container runs
rootless (user namespace + uid/gid mappings, `/proc` and a `/dev` tmpfs mounted, argv exec'd
directly rather than through `/bin/sh`). Without one, the process backend enters the rootfs via
`unshare(CLONE_NEWUSER|CLONE_NEWNS)` + `chroot`.

**Decompression-bomb ceiling:** a layer blob is refused when its uncompressed size exceeds
**min(1000× the compressed length, 8 GiB)** (`_STOR_MAX_RATIO` / `_STOR_MAX_OUT`,
`src/storage.cyr:751-752`). Real base images are far below both bounds.
*(A previous revision of this page claimed layers over "roughly 1 MB" fail to unpack and that
this "rules out most real base images". That was false.)*
