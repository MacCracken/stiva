# 001 — Typed struct field access can silently read garbage (cycc)

**Status: last verified live at cyrius 6.4.78, by execution rather than by reading.
NOT re-verified at 6.5.33** — the pin moved at 3.0.17 and no probe was run against it, so
treat the status as *unknown at the current pin, and assume live*.

The full suite (2175 tests) and the 87 CLI smoke assertions are green at 6.5.33 with the
workarounds below still in place. **That is not evidence the bug is gone** — see "the
expensive lesson" below, where a green probe and a green suite coexisted with live silent
corruption. Assume live until someone runs the retirement checklist at the end of this
document.

cycc assigns each struct a numeric id. Ids **20 and 21** collide with the SIMD `f64v2` /
`f64v4` sentinels, so a scalar field access on such a struct can compile to a **vector
load**. The consequences:

- The read returns **garbage** — usually silently. Sometimes it **segfaults**, because a
  scalar field compiled as a vector load reads past the end of the allocation.
- It is **per-function**: the same struct, the same field, read in two different functions
  in the same file, can give different answers.
- It is **per-compilation-unit**: it moves with the unit's struct-id assignment, so it is
  present in some `.tcyr` include sets and absent in others.

## Why this is written down rather than fixed

It has been "fixed" upstream twice and regressed both times as stiva's struct count grew.
6.4.14 fixed it; it came back. 6.4.76 partially fixed it, the workarounds were retired on
the strength of a green probe, and the suite then SIGSEGV'd in `image_store_save_archive`
under the 6-module `store.tcyr` unit. All of it was reverted.

## The expensive lesson (2026-07-24)

At 6.4.77 a probe asserting typed `Image` / `Layer` field access against raw-offset ground
truth passed in **all four unit shapes**, so the bug was recorded as "appears fixed". Hours
later, in the *same* 6-module `tests/registry.tcyr` unit that probe had certified, an
`ImageRef` obtained from a **wrapper function** read back as garbage: `image_ref_new` wrote
`"reg.test"` correctly and the wrapper's caller saw `0@!Z`. Both the direct-return and the
return-via-local wrapper forms failed; constructing the ref inline in the caller worked.

The symptom was **not a crash**. It was a cache key silently built from junk, which merely
made every registry request re-authenticate. It was caught only because a test built the
same cache key independently and compared.

Two rules follow:

1. **A probe certifies the exact expression it runs, in the exact function it runs in —
   nothing more.** "Green in every unit shape" is not a property this bug respects.
2. **Prefer assertions that reconstruct a value from a second source** over assertions that
   read it back the way the code under test does. The latter reads the same garbage and
   passes.

## What is in the code because of this

Raw-offset accessors — `_img_id`, `_img_layers`, `_img_manifest_digest`, `_layer_digest`,
and the `load64(res + N)` reads in `scan_output`, `container_manager_start`,
`_fleet_select_and_reserve` and the accel branch of `node_matches_constraints`. They are
deliberate. Do not "clean them up".

New code touching these structs is free to use `x.field` — §B, §C and later work do, and are
green — but any *new* miscompile shows up as a wrong value long before it shows up as a
crash, so bisect on values, not on signals.

**Retiring the workarounds** needs: a probe green in *every* unit shape including the
6-module `store.tcyr`, the actual conversion attempted (not just the probe), a full-suite
run, and an assertion that would fail on a silent wrong value rather than only on a segfault.
