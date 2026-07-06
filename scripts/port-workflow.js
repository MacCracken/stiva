export const meta = {
  name: 'stiva-port',
  description: 'Port dep-free stiva modules from rust-old/ (Rust oracle) to Cyrius, then adversarially verify parity',
  phases: [
    { title: 'Port', detail: 'one agent per module: rust-old/src/X.rs → src/X.cyr (distinct file, parallel-safe), standalone cyrius check' },
    { title: 'Verify', detail: 'adversarial parity check of the ported src/X.cyr against the Rust oracle' },
  ],
}

const REPO = '/home/macro/Repos/stiva'
// Modules to port this run (dep-free, error-only coupling).
// An entry may be "foo" (rust-old/src/foo.rs → src/foo.cyr) or a subdir path
// like "network/bridge" (rust-old/src/network/bridge.rs → src/network_bridge.cyr,
// flattened kavach-style). `norm` derives the oracle path + flat output name.
//
// EDIT `BATCH` per run and re-invoke with {scriptPath}. NOTE: passing `args`
// via the Workflow tool does NOT reliably reach the script under scriptPath
// (observed undefined), so BATCH is the source of truth; args only overrides
// when actually present.
const BATCH = ['image', 'registry']
const MODULES = (Array.isArray(args) && args.length) ? args : BATCH
function norm(mod) {
  const cyr = mod.split('/').join('_')       // network/bridge → network_bridge
  return { rs: `${REPO}/rust-old/src/${mod}.rs`, cyr: `${REPO}/src/${cyr}.cyr`, name: cyr, mod }
}

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
- Strings are NUL-terminated cstrings. Helpers in string.cyr: strlen, streq(a,b)->1/0, memeq(a,b,n), memcpy, memset, atoi(cstr)->i64, println. Heap string type + ops in str.cyr: Str, str_from, str_len, str_eq, str_cat, str_sub, str_contains, str_starts_with, str_index_of, str_split; str_builder_new/_add_cstr/_add/_add_int/_putc/_build, str_data/str_cstr. Preserve EXACT display strings from Rust \`#[error("...")]\` / format! literals.
- **strstr GOTCHA**: string.cyr's \`strstr(hay, needle)\` returns a 0-based INDEX (>= 0 found, **-1 absent**), NOT a C pointer. So "contains" is \`strstr(h,n) >= 0\` (index 0 IS a match), and the substring starts at \`hay + strstr(hay,key) + strlen(key)\` — never \`at + strlen(key)\` treating the return as a pointer. Verify with \`cyrius test\`, not just \`cyrius check\` (syntax-only check can't catch this).
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
    `${PLAYBOOK}\n\n=== YOUR MODULE: "${norm(mod).name}" ===\n` +
    `Oracle (read this, the parity source): ${norm(mod).rs}\n` +
    `Write the port to EXACTLY this path:    ${norm(mod).cyr}\n` +
    `Use "${norm(mod).name}" as the module prefix base for enum members and _-prefixed private helpers.\n` +
    (MODULES.length > 1
      ? `SIBLINGS ported ALONGSIDE you this same batch (their src/*.cyr are included together at build time, so you MAY reference their public types/fns directly instead of deferring — read their oracle .rs to learn the surface): ${MODULES.filter(m => m !== mod).map(m => `${norm(m).name} (rust-old/src/${m}.rs)`).join(', ')}. Only DEFER a cross-module ref if the OTHER module is NOT in this sibling list and is not yet ported.\n`
      : ``) +
    `Port it now.`,
    { label: `port:${norm(mod).name}`, phase: 'Port', schema: PORT_SCHEMA, effort: 'high' }
  ),
  (port, mod) => agent(
    `Adversarially verify the Cyrius port for parity against the Rust oracle.\n` +
    `Oracle:  ${norm(mod).rs}\n` +
    `Port:    ${norm(mod).cyr}\n` +
    `The port claims this public surface: ${JSON.stringify(port ? port.public_surface || [] : [])}\n` +
    `and these deferrals: ${JSON.stringify(port ? port.deferred || [] : [])}\n\n` +
    `Read BOTH files. Check every Rust public fn/type is either ported faithfully (same branches, same display strings, same numeric constants, same edge-case handling) or listed in a DEFERRED block that is genuinely blocked on a not-yet-ported stiva module. IMPORTANT: this stdlib's strstr returns a 0-based INDEX (-1 absent), NOT a C pointer — flag any \`strstr(...) != 0\` / \`+ strlen\`-as-pointer misuse. Default to reporting a gap when unsure. Enum members must be module-prefixed. Report concrete gaps only.`,
    { label: `verify:${norm(mod).name}`, phase: 'Verify', schema: VERIFY_SCHEMA, effort: 'high' }
  ).then(v => ({ port, verify: v, name: norm(mod).name }))
)

return {
  modules: MODULES,
  results: results.filter(Boolean),
}
