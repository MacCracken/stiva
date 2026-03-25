# CLI Reference

Stiva provides a `stiva` binary with 26 subcommands.

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
| `stiva build [-f FILE] [-c CONTEXT]` | Build image from Stivafile |
| `stiva images` | List local images |
| `stiva rmi <IMAGE>` | Remove a local image |
| `stiva tag <SOURCE> <TARGET>` | Tag a local image |
| `stiva import <FILE> --name NAME [--tag TAG]` | Import tar as image |

### Containers

| Command | Description |
|---------|-------------|
| `stiva run <IMAGE> [-d] [-p PORT] [-e ENV] [CMD...]` | Run a container |
| `stiva ps` | List containers |
| `stiva stop <ID>` | Stop a container |
| `stiva rm <ID>` | Remove a container |
| `stiva restart <ID>` | Restart a stopped container |
| `stiva exec <ID> <CMD...>` | Execute command in running container |
| `stiva kill <ID> [-s SIGNAL]` | Send signal (default: SIGTERM) |
| `stiva pause <ID>` | Pause via cgroups freezer |
| `stiva unpause <ID>` | Unpause a paused container |
| `stiva inspect <ID>` | Inspect container or image (JSON) |
| `stiva top <ID>` | List processes in container |
| `stiva stats <ID>` | Show CPU/memory/PID stats |
| `stiva logs <ID> [-n LINES]` | Show container logs (last N lines) |
| `stiva export <ID> -o FILE` | Export rootfs as tar |
| `stiva cp <SRC> <DST>` | Copy files (container:path or host path) |
| `stiva wait <ID>` | Wait for container to exit |

### Operations

| Command | Description |
|---------|-------------|
| `stiva prune` | Remove stopped containers + unused images |
| `stiva checkpoint <ID> [--leave-running]` | CRIU checkpoint |
| `stiva restore <ID> <DIR>` | Restore from checkpoint |
| `stiva info` | Show system information |
| `stiva convert <FILE> [-f FORMAT] [-o OUT]` | Convert YAML to TOML (compose or dockerfile) |

## Examples

```bash
# Pull and run a daemon
stiva pull nginx:latest
stiva run -d -p 8080:80 nginx:latest

# Check status
stiva ps
stiva top <container-id>
stiva stats <container-id>

# Execute inside
stiva exec <container-id> ls /etc/nginx

# Stop and clean up
stiva stop <container-id>
stiva rm <container-id>
stiva prune

# Build and push
stiva build -f Stivafile -c .
stiva push myapp:latest registry.example.com/myapp:latest

# Export/import
stiva export <container-id> -o rootfs.tar
stiva import rootfs.tar --name imported --tag v1

# Copy files
stiva cp ./local-file.txt <container-id>:/app/
stiva cp <container-id>:/var/log/app.log ./app.log
```

## Environment

| Variable | Description |
|----------|-------------|
| `RUST_LOG` | Tracing filter (e.g., `stiva=debug`, `warn`) |
