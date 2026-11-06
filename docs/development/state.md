# stiva — Current State

> Refreshed every release. CLAUDE.md is preferences/process/procedures
> (durable); this file is **state** (volatile).

## Version

**3.2.0** — the Rust → Cyrius port is **COMPLETE**. 18622 lines of Rust preserved at
`rust-old/` as the frozen parity oracle (do not edit).

## Toolchain

- **Cyrius pin**: `6.7.18` (in `cyrius.cyml [package].cyrius`)

## Source

- Rust oracle: 18622 lines at `rust-old/` (frozen).
- Cyrius: **24,180 lines** across `src/*.cyr` — **27 domain modules** listed in
  `cyrius.cyml [lib].modules`, plus `src/lib.cyr` (aggregation header) and `src/main.cyr`
  (entry + CLI). `cyrius distlib` folds the 27 into `dist/stiva.cyr`.
- CLI: **36 registered verbs, 33 live**. `checkpoint` and `restore` print "not wired yet" —
  they need CRIU (roadmap v3.4.1). `restart` cannot succeed yet (v3.2.1).

## Tests

**2892** assertions across 6 `tests/*.tcyr` files — stiva 667 · runpath 902 · registry 444 ·
store 387 · mgmt 354 · convert 138 — plus **164** CLI smoke assertions
(`./scripts/cli-smoke.sh`, which covers `src/main.cyr`; the `.tcyr` files cannot include it)
**21** fuzz targets (`tests/stiva.fcyr`) and **16** benchmarks.

⚠ Counting note: `cyrius test tests/` prints a trailing `6 passed, 0 failed`, which is the
**file** tally, not six extra assertions. Adding it to the per-file sum overstates the count
by 6 — a mistake that reached four documents before it was caught.

## Dependencies

`[deps].stdlib` declares a **40-entry** union (stdlib is opt-in; nothing is pulled that is not
named). Git-pinned AGNOS bundles, in declaration order:

| Dep | Pin | Dep | Pin |
|---|---|---|---|
| sigil | 3.13.15 | kavach | 3.14.5 |
| sakshi | 2.5.10 | samay | 1.5.1 |
| libro | 2.11.3 | ai-hwaccel | 2.5.6 |
| majra | 2.13.0 | cmdit | 1.5.0 |
| bote-core | 3.8.15 | nein | 1.9.1 |

`sigil` is declared **first** and deliberately — it claims the name before libro's transitive
thin sub-bundle selection, which would otherwise collide with the full bundle and break the
build outright. `bote-core` is named for its module for the same reason: nein declares that name
itself, at an older tag. Each tag is that dep's newest, and each is built on a `cyrius` pin ≤ 6.7.18.
`sigil` and `sakshi` compile from the toolchain snapshot's folds, which at 6.7.18 are the same bytes
as the tags above.

## Consumers

daimon (container management), sutra (fleet deployment).

⚠ Both are **cyrius-built**, and a cyrius-built payload currently reads **no environment**
inside a stiva container: `getenv` reads `/proc/self/environ` and nothing mounts `/proc` on the
`run -d` path. Tracked on the agnos roadmap and as roadmap v3.2.2.

## Next

See [`roadmap.md`](roadmap.md). v3.2 is making one container correct: manageable lifecycle, then
mounts, user and secrets. Its **Recording a block** convention says what it
takes to call anything blocked in this tree.

⛔ The largest open security gap: every container runs as root, an image's `User` included (roadmap
v3.2.2). `run -d` and the process backend also hold every capability, in a user namespace. See
SECURITY.md.
