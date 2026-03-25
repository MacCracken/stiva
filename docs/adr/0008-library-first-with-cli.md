# ADR-0008: Library-first design with thin CLI

## Status
Accepted

## Context
Container runtimes can be designed CLI-first (Docker) or library-first (containerd's Go API). Stiva serves both daimon (programmatic) and operators (CLI).

## Decision
Stiva is a Rust library crate with a thin CLI binary. All functionality lives in `lib.rs` and modules. The CLI (`main.rs`) is a ~450-line clap wrapper that delegates every command to `Stiva` struct methods. No business logic in `main.rs`.

## Consequences
- **Positive**: Daimon, sutra, and other AGNOS crates use stiva as a library dependency — no subprocess spawning.
- **Positive**: CLI and library always have feature parity.
- **Positive**: Testing is done at the library level — CLI is a thin pass-through.
- **Negative**: Binary adds `clap` and `tracing-subscriber` dependencies.
