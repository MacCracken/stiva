# stiva — Current State

> Refreshed every release. CLAUDE.md is preferences/process/procedures
> (durable); this file is **state** (volatile).

## Version

**3.0.19** — the Rust → Cyrius port is **COMPLETE**. 18622 lines of Rust preserved at
`rust-old/` as the frozen parity oracle (do not edit).

## Toolchain

- **Cyrius pin**: `6.5.33` (in `cyrius.cyml [package].cyrius`)

## Source

- Rust oracle: 18622 lines at `rust-old/` (frozen).
- Cyrius: **22,667 lines** across `src/*.cyr` — **27 domain modules** listed in
  `cyrius.cyml [lib].modules`, plus `src/lib.cyr` (aggregation header) and `src/main.cyr`
  (entry + CLI). `cyrius distlib` folds the 27 into `dist/stiva.cyr`.
- CLI: **36 registered verbs, 34 live**. `checkpoint` and `restore` print "not wired yet" —
  they need CRIU (roadmap v3.1.0 item 3).

## Tests

**2186** assertions across 6 `tests/*.tcyr` files — stiva 667 · registry 421 · runpath 358 ·
mgmt 325 · store 299 · convert 116 — plus **87** CLI smoke assertions
(`./scripts/cli-smoke.sh`, which covers `src/main.cyr`; the `.tcyr` files cannot include it)
and **14** benchmarks.

⚠ Counting note: `cyrius tests tests/` prints a trailing `6 passed, 0 failed`, which is the
**file** tally, not six extra assertions. Adding it to the per-file sum overstates the count
by 6 — a mistake that reached four documents before it was caught.

## Dependencies

`[deps].stdlib` declares a **40-entry** union (stdlib is opt-in; nothing is pulled that is not
named). Git-pinned AGNOS bundles, in declaration order:

| Dep | Pin | Dep | Pin |
|---|---|---|---|
| sigil | 3.12.9 | kavach | 3.12.2 |
| sakshi | 2.4.11 | samay | 1.0.1 (optional) |
| libro | 2.8.8 | ai-hwaccel | 2.3.18 (optional, on by default) |
| majra | 2.6.7 | agnodrm | 1.5.1 |
| bote | 3.3.2 | cmdit | 1.2.2 |
| nein | 1.6.10 | | |

`sigil` is declared **first** and deliberately — it claims the name before libro's transitive
thin sub-bundle selection, which would otherwise collide with the full bundle and break the
build outright.

## Consumers

daimon (container management), sutra (fleet deployment).

⚠ Both are **cyrius-built**, and a cyrius-built payload currently reads **no environment**
inside a stiva container: `getenv` reads `/proc/self/environ` and nothing mounts `/proc` on the
`run -d` path. Tracked on the agnos roadmap and as roadmap v3.1.0 item 9.

## Next

See [`roadmap.md`](roadmap.md). The single-node runtime is finished; v3.1.0 is secrets,
interactivity and mobility.

⚠ An adversarial audit at 3.0.18 found **zero of the eight** v3.1.0 items genuinely blocked on
an upstream release — seven were never blocked. Read the rewritten v3.1.0 section, and the
**Recording a block** convention at the end of `roadmap.md`, before believing any "blocked"
label in this tree.
