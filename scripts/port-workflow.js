export const meta = {
  name: 'stiva-port',
  description: 'Port dep-free stiva modules from rust-old/ (Rust oracle) to Cyrius, then adversarially verify parity',
  phases: [
    { title: 'Port', detail: 'one agent per module: rust-old/src/X.rs → src/X.cyr (distinct file, parallel-safe), standalone cyrius check' },
    { title: 'Verify', detail: 'adversarial parity check of the ported src/X.cyr against the Rust oracle' },
  ],
}

const REPO = '/home/macro/Repos/stiva'
// Modules to port this run (dep-free, error-only coupling by default).
const MODULES = (Array.isArray(args) && args.length) ? args : ['intents', 'audit', 'convert']

const PLAYBOOK = `You are porting one module of the "stiva" OCI container runtime from Rust to the Cyrius systems language, for FULL PARITY. The Rust source in ${REPO}/rust-old/ is the frozen PARITY ORACLE — the bar is "matches what the Rust did".

## Ground truth you MUST read first (in this order)
1. ${REPO}/src/error.cyr  — the canonical exemplar: enum with PREFIX_* members, name fn, print fn, #[must_use], header comment style.
2. ${REPO}/src/oci.cyr    — exemplar for: leaf-part porting, sentinel returns, string/int handling, and the "DEFERRED (land with src/<other>.cyr)" block pattern for cross-module-coupled code.
3. /home/macro/Repos/cyrius/docs/guides/cyrius-guide.md — the language reference (skim the sections you need: Enums, Structs, Functions, Strings, Slices, Result/?, Match, Vec/hashmap).
4. A real ported peer for idioms at scale: /home/macro/Repos/kavach/src/*.cyr (e.g. audit.cyr, policy.cyr, oci_spec.cyr).

## Cyrius idioms (the ones that bite)
- i64-everywhere. Type annotations optional and non-enforcing. No pointer type — addresses are i64.
- Functions: \`fn name(a, b) { ... return X; }\`. EVERY function returns a value (\`return 0;\` if nothing). Forward calls are fine. Up to 6 register params.
- Enums: \`enum E { E_PREFIX_A = 0; E_PREFIX_B = 1; }\`. Cyrius hoists members to GLOBAL constants under "last-def-wins", so members MUST be module-prefixed (e.g. STIVA_ERR_*, OCI_STATUS_*) to avoid silent cross-module collisions. Access bare (\`E_PREFIX_A\`) or qualified (\`E.E_PREFIX_A\`).
- NO unwrap/panic in library code. Fallible functions either return an integer sentinel (e.g. \`return 0 - 1;\` for "invalid") — document it — or a Result via \`include "lib/result.cyr"\` (\`Ok(v)\`/\`Err(code)\`, postfix \`?\`). Rust \`Result<T, StivaError>\` maps to: return the value on success, or the negated/dedicated error path on failure. Rust error variants that carry a \`String\` map to an integer error CODE from src/error.cyr (STIVA_ERR_*); the context string is carried by the caller via stiva_err_print(code, detail).
- Strings are NUL-terminated cstrings. Helpers in string.cyr: strlen, streq(a,b)->1/0, memeq(a,b,n), memcpy, memset, atoi(cstr)->i64, println. Heap string type + ops in str.cyr: Str, str_from, str_len, str_eq, str_cat, str_sub, str_contains, str_starts_with, str_index_of, str_split. Preserve EXACT display strings from Rust \`#[error("...")]\` / format! literals.
- Structs: \`struct S { a; b; }\`; init \`S { 1, 2 }\` or \`S { a: 1, b: 2 }\`; access \`s.a\`; \`s.a = v\`. Pointer-to-struct dot-access needs a \`: TypeName\` annotation on the local.
- Collections: vec.cyr (vec_new, vec_push, vec_get, vec_len, vec_set, vec_pop); hashmap.cyr (hashmap_new, hashmap_put/get/has). alloc.cyr: call \`alloc_init()\` once before any heap use.
- Control flow: if/elif/else, while, counted for (all 3 clauses required), switch (int-literal cases, no fallthrough), match (enum-exhaustive; use \`_ =>\` catch-all).
- I/O: \`sys_write(fd, buf, len)\`; io.cyr file_open/file_read/file_write/file_read_all; fs.cyr for dir ops. Diagnostics to stderr use the module's err_print helper, never raw magic numbers.
- Attributes: #[must_use] on pure functions; #[inline] hints allowed. write! over format! (avoid temp allocs) — Cyrius has str_builder in str.cyr.

## Cross-module coupling rule
This module may reference types from other stiva modules not yet ported (e.g. container's Container/ContainerState/ContainerConfig, image's types). Port everything that is self-contained or depends only on already-ported modules (error.cyr is ported). For any function/type that needs a NOT-YET-PORTED module's types, DO NOT invent them — put them in a clearly commented \`# ── DEFERRED (land with src/<module>.cyr) ──\` block at the bottom listing the Rust signatures, exactly like src/oci.cyr does. Report those in the "deferred" field.

## Your deliverable for module "<MOD>"
1. Read ${REPO}/rust-old/src/<MOD>.rs (the oracle) end to end.
2. Write ${REPO}/src/<MOD>.cyr — a faithful port. Header comment: "# stiva — <one-line purpose>\\n# Ported from rust-old/src/<MOD>.rs." Port EVERY public type and function (or DEFER with a note). Module-prefix all enum members.
3. Run \`cd ${REPO} && cyrius check src/<MOD>.cyr\` — it must PARSE (standalone syntax check; undefined-symbol notes for stdlib fns are expected and fine, a PARSE error is not). Fix any parse errors.
4. Determine which stdlib modules your .cyr references (string, str, vec, hashmap, io, fs, fmt, chrono, result, tagged, slice, etc.) beyond the always-on base.
5. Provide the test functions (Cyrius \`fn test_*()\` bodies using test_group/assert/assert_eq) that mirror the Rust \`#[cfg(test)]\` module — these will be integrated into tests/stiva.tcyr by the integrator; do NOT edit tests/stiva.tcyr yourself (shared file).

Return ONLY the structured object. Do not write any file other than ${REPO}/src/<MOD>.cyr.`

const PORT_SCHEMA = {
  type: 'object',
  additionalProperties: false,
  properties: {
    module: { type: 'string' },
    wrote_file: { type: 'boolean', description: 'true if src/<module>.cyr was written' },
    parses: { type: 'boolean', description: 'true if `cyrius check` parsed the file (ignoring undefined-symbol notes)' },
    stdlib_needed: { type: 'array', items: { type: 'string' }, description: 'stdlib modules this .cyr references (union to add to cyrius.cyml)' },
    test_cyr: { type: 'string', description: 'Cyrius source of the test_* functions mirroring the Rust #[cfg(test)] module (no main, no includes)' },
    test_fn_names: { type: 'array', items: { type: 'string' }, description: 'names of the test_* fns to call from tests/stiva.tcyr main()' },
    deferred: { type: 'array', items: { type: 'string' }, description: 'Rust items deferred due to not-yet-ported cross-module coupling (name + why)' },
    public_surface: { type: 'array', items: { type: 'string' }, description: 'ported public fns/types (for the parity check)' },
    summary: { type: 'string' },
  },
  required: ['module', 'wrote_file', 'parses', 'stdlib_needed', 'test_cyr', 'test_fn_names', 'deferred', 'summary'],
}

const VERIFY_SCHEMA = {
  type: 'object',
  additionalProperties: false,
  properties: {
    module: { type: 'string' },
    parity: { type: 'string', enum: ['full', 'partial-with-documented-deferrals', 'gaps'] },
    gaps: { type: 'array', items: { type: 'string' }, description: 'behavioral parity gaps vs the Rust oracle (missing fn, wrong display string, wrong branch, off-by-one) — empty if none' },
    deferrals_ok: { type: 'boolean', description: 'true if every deferred item is genuinely blocked on a not-yet-ported module (not laziness)' },
    verdict: { type: 'string' },
  },
  required: ['module', 'parity', 'gaps', 'deferrals_ok', 'verdict'],
}

log(`porting ${MODULES.length} module(s): ${MODULES.join(', ')}`)

const results = await pipeline(
  MODULES,
  (mod) => agent(
    `${PLAYBOOK}\n\n=== YOUR MODULE: "${mod}" ===\nPort ${REPO}/rust-old/src/${mod}.rs to ${REPO}/src/${mod}.cyr now.`,
    { label: `port:${mod}`, phase: 'Port', schema: PORT_SCHEMA, effort: 'high' }
  ),
  (port, mod) => agent(
    `Adversarially verify the Cyrius port for parity against the Rust oracle.\n` +
    `Oracle:  ${REPO}/rust-old/src/${mod}.rs\n` +
    `Port:    ${REPO}/src/${mod}.cyr\n` +
    `The port claims this public surface: ${JSON.stringify(port ? port.public_surface || [] : [])}\n` +
    `and these deferrals: ${JSON.stringify(port ? port.deferred || [] : [])}\n\n` +
    `Read BOTH files. Check every Rust public fn/type is either ported faithfully (same branches, same display strings, same numeric constants, same edge-case handling) or listed in a DEFERRED block that is genuinely blocked on a not-yet-ported stiva module. Default to reporting a gap when unsure. Enum members must be module-prefixed. Report concrete gaps only.`,
    { label: `verify:${mod}`, phase: 'Verify', schema: VERIFY_SCHEMA, effort: 'high' }
  ).then(v => ({ port, verify: v }))
)

return {
  modules: MODULES,
  results: results.filter(Boolean),
}
