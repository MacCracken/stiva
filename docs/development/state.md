# stiva — Current State

> Refreshed every release. CLAUDE.md is preferences/process/procedures
> (durable); this file is **state** (volatile).

## Version

**0.1.0** — ported from Rust (2026-07-06) via `cyrius port`. 18622 lines of Rust preserved at `rust-old/` for parity reference.

## Toolchain

- **Cyrius pin**: `6.4.10` (in `cyrius.cyml [package].cyrius`)

## Source

- Rust reference: 18622 lines at `rust-old/` (frozen, do not edit).
- Cyrius port: scaffold only — `src/main.cyr` stub.

## Tests

_Replace with parity test status once tests land._

## Dependencies

Direct (declared in `cyrius.cyml`):

- stdlib — string, fmt, alloc, vec, str, syscalls, io, args, assert

## Consumers

_None yet._

## Next

See [`roadmap.md`](roadmap.md). The first milestone is typically Rust→Cyrius surface parity for the 18622-line subset.
