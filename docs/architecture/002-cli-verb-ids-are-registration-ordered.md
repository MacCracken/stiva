# 002 — CLI verb ids are registration-ordered

`cmdit_verb(h, name, help)` returns an id derived from **registration order**, and
`src/main.cyr` dispatches through a table indexed by that id.

Inserting a verb anywhere but the **end** of the registration list therefore renumbers every
verb after it — silently, with no compile error and no runtime warning. The program still
runs; it just runs the wrong verb.

This is not hypothetical. `stiva cron` was first registered alphabetically, before
`completions`, and `stiva cron ls` dispatched into the `completions` handler. Nothing failed
loudly; `cron ls` simply did something else.

## Rules

- **Register new verbs last.** `cron` is registered last today for exactly this reason.
- For a verb that takes trailing positionals, use
  `cmdit_verb_trailing_after(h, cmdit_verb(h, "name", "help"), n)` — it consumes the id the
  inner call returns, so the ordering property still holds.
- **`scripts/cli-smoke.sh` is the only thing that catches this.** A `.tcyr` file cannot
  include `main.cyr` (it has its own entry point), so the unit suite has no visibility into
  verb dispatch at all. Every new verb needs a smoke assertion.
