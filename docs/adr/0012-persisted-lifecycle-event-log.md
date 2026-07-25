# 0012 — Persisted lifecycle event log at {root}/events.jsonl

**Status**: Accepted
**Date**: 2026-07-25

## Context

[ADR-0006](0006-majra-pubsub-lifecycle-events.md) chose majra pub/sub for lifecycle events and
recorded the cost honestly in its own consequences: *"No event persistence — missed events are
lost. Subscribers must be active."*

Roadmap §E then called for a `stiva events` verb. That is where the cost came due. The hub is a
`pubsub_new()` owned by one `ContainerManager` (`src/container.cyr`), created per instance and
never persisted — and stiva's CLI is a **one-shot dispatcher** ([ADR-0003](0003-daemon-vs-oneshot-model.md)),
so every invocation is a separate process that builds its own manager. A `stiva events` written
against the bus would construct a manager, subscribe to *its own* empty publisher, and poll forever
printing nothing. Not a degraded stream — a guaranteed empty one, and strictly worse than the "not
yet wired" message it replaced.

`stiva logs -f` (v3.0.12) works for exactly one reason: a container's log is a **file**, and a file
is observable across processes. Events needed the same property.

Two things had to be decided beyond "write them down": where the log lives, and — since there is no
single container whose exit ends the stream, the way there is for `logs -f` — what makes
`stiva events` terminate.

## Decision

`container_manager_publish_event` publishes to the majra hub as before **and additionally appends
the payload to `{root}/events.jsonl`**, one JSON object per line, via `file_append_locked`
(`O_APPEND` under an `flock`). Rotation reuses the pair the container logs already use —
`container_should_rotate` + `rotate_logs` — giving `events.jsonl` → `.1` → … → `.N` with the oldest
dropped, at 8 MiB × 3 generations by default.

The append is **best-effort by contract**: every failure path is silent-and-continue, because the
caller is a lifecycle transition whose success must not depend on a log write.

**The log is per-root, not per-container.** `stiva events` is a whole-runtime stream, so
per-container files would have to be discovered and merged in timestamp order; a container that
failed before its directory existed would have nowhere to write; and decisively,
`container_manager_remove` deletes `{root}/containers/{id}`, which would destroy the `removed`
event as it was being written. A log that loses the record of a removal at the moment of the
removal is not an event log.

**Termination is bounded by default.** Without `-f`, `stiva events` dumps the matching events and
exits. With `-f` it stops at the first of `--count N`, wall clock past `--until T`, or an
interrupt; with neither bound it follows until interrupted, as `docker events` does.

Events carry a `ts` field in epoch **milliseconds**, sourced from `clock_epoch_ns` —
deliberately not `clock_now_ms`, which `lib/chrono.cyr` documents as monotonic (ns since boot).

## Consequences

- **Positive** — `stiva events` is possible at all, and a subscriber no longer has to be running at
  the moment of the event. This retires the second negative consequence recorded in ADR-0006.
- **Positive** — the log is a cross-process audit trail that outlives the container it describes,
  and outlives the process that wrote it. `flock` + `O_APPEND` makes concurrent stiva invocations
  interleave whole lines.
- **Positive** — one JSON object per line, printed verbatim, so `stiva events | jq` works without a
  parser. No `[HH:MM:SS] ` prefix; hints and warnings go to stderr.
- **Negative** — lifecycle transitions now do disk I/O. Measured at **7.8 µs** per append
  (`tests/stiva.bcyr`), or ~31 µs across a container's whole lifecycle, against the ~1.3 ms a
  single `flatten_layers` costs on the same create. `STIVA_EVENTS=off` opts out.
- **Negative** — an unattended root now grows a file. Bounded by rotation, and the bound is
  tunable (`STIVA_EVENTS_MAX_BYTES` / `STIVA_EVENTS_MAX_FILES`), with a non-positive value from the
  environment **refused** rather than honored — `atoi` returns 0 for junk and 0 means "never
  rotate", so a typo would otherwise silently buy an unbounded log.
- **Negative** — a follower must handle its file being renamed underneath it. It tracks `st_ino`,
  not just size: lifecycle events are near enough fixed-width that a rotated log routinely grows
  back to *exactly* the offset the reader held, at which point a size-only test sees nothing to do
  and the follower goes quiet permanently. This was found by a smoke run across a forced rotation,
  not by reading the code.
- **Neutral** — rotated generations are not read back; `stiva events` reads the live file only, so
  `--since` cannot reach past the last rotation. Merging generations is deferrable until someone
  needs it.
- **Neutral** — ADR-0006's *other* negative stands unchanged: this is still local, not network,
  pub/sub. Daimon integration still wants an HTTP bridge; it can now be built over a file that
  survives restarts.

## Alternatives considered

- **A per-container `events.jsonl` under `{root}/containers/{id}/`** — rejected on the removal
  argument above, and because it would force `stiva events` to walk directories and merge streams
  in timestamp order for what is conceptually one stream.
- **Reuse `state.json`** ([ADR-0004](0004-persistent-container-state.md)) — rejected: it is a
  rewritten snapshot of current state, not an append-only history, and every write would rewrite
  the whole file. Wrong data structure for an event log.
- **A daemon owning the bus, with `stiva events` as a client** — the honest fix, and out of scope:
  it is the daemon half of ADR-0003, which the one-shot line has deliberately deferred. A file
  gets the observability now and does not conflict with a daemon later.
- **Persist only on an explicit opt-in** — rejected as a default. An event log nobody enabled is an
  event log that is empty exactly when it is needed, and the measured cost does not justify making
  the useful case the unusual one. The opt-out (`STIVA_EVENTS=off`) covers the operator who
  genuinely does not want the writes.
- **`--tail N` instead of `--count N`** — rejected for a bounded stream: "last N" and "stop after N"
  disagree the moment `-f` is involved, and one flag with two meanings across modes is worse than a
  terminator that means the same thing in both.
