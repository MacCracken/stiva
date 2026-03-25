# ADR-0004: Persistent container state via state.json

## Status
Accepted

## Context
Container records are held in-memory (`HashMap` behind `RwLock`). If the stiva process restarts, all container metadata is lost — containers become orphans.

## Decision
Persist container state to `{root}/state.json` after every state-changing operation (create, start, stop, remove, pause, unpause). On startup, load and restore. Running/Paused containers are transitioned to Stopped (the process is gone after restart).

## Consequences
- **Positive**: Container records survive daemon restart. `stiva ps` shows containers from previous sessions.
- **Positive**: Atomic writes (tmp + rename) prevent corruption on crash.
- **Negative**: Extra I/O on every state change (~1ms per write for typical state size).
- **Negative**: Daemon containers cannot be automatically restarted after process restart — only their metadata is restored. The `restart` command handles manual re-start.
