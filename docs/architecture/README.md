# Architecture notes

Non-obvious constraints, quirks, and invariants that a reader cannot derive from the code alone. Numbered chronologically — never renumber.

Not decisions (those live in [`../adr/`](../adr/)) and not guides (those live in [`../guides/`](../guides/)). An item here describes *how the world is*, not *what we chose* or *how to do something*.

## Items

| # | Item | In one line |
|---|------|-------------|
| 001 | [Typed struct field access can silently read garbage (cycc)](001-cycc-struct-id-miscompile.md) | The struct-id ↔ SIMD-sentinel miscompile was last verified live at 6.4.78 and is un-re-verified at the current 6.5.33 pin (assume live); it is per-function *and* per-compilation-unit, and it fails silently more often than it crashes. Explains every raw-offset accessor in the codebase |
| 002 | [CLI verb ids are registration-ordered](002-cli-verb-ids-are-registration-ordered.md) | Inserting a verb anywhere but the end of `main.cyr` silently renumbers every verb after it. `cli-smoke.sh` is the only thing that catches it |
| 003 | [A `path` dep override silently wins over its `tag`](003-path-overrides-silently-win-over-tags.md) | …and `cyrius.lock` records nothing that would reveal the substitution. Why every `path` line is commented out and CI compares bundles against tags |
| 004 | [A container rootfs has two possible layouts](004-two-rootfs-layouts.md) | Overlay vs flattened (`{croot}/.rootfs-flattened`). Anything reasoning about "what changed" needs both paths, or it reports a flattened container as clean |

_Add a numbered entry (`00N-kebab-case-title.md`) the next time the code has a non-obvious invariant a reader can't derive. Do not write entries for decisions — those are ADRs._
