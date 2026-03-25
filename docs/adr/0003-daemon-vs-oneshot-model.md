# ADR-0003: Dual execution model — one-shot and daemon

## Status
Accepted

## Context
Container runtimes need both short-lived (build steps, CI jobs) and long-lived (web servers, databases) execution. The initial implementation only supported one-shot (run-to-completion).

## Decision
Support both models via `ContainerConfig.detach`:
- **One-shot** (default): `start()` blocks until command completes, returns exit code.
- **Daemon** (`detach: true`): `start()` spawns via kavach `Sandbox::spawn()` and returns immediately. Caller manages lifecycle via `wait()`, `stop()`, `try_wait()`.

## Consequences
- **Positive**: Single API for both models. No separate daemon runtime.
- **Positive**: Daemon containers get proper SIGTERM→SIGKILL lifecycle on `stop()`.
- **Positive**: `try_wait()` enables non-blocking health checks.
- **Negative**: `DaemonHandle` holds a kavach `SpawnedProcess` + `Sandbox` — dropping it leaks the process (documented behavior).
- **Negative**: Daemon stdout/stderr is piped but not streamed — only available after `wait()` or `kill()`.
