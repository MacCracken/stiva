export const meta = {
  name: 'stiva-gap-closure',
  description: 'Close the achievable deferrals (build TOML / image JSON index / storage gzip-tar) using the existing Cyrius stdlib',
  phases: [
    { title: 'Implement', detail: 'one agent per deferral: implement the deferred fns in src/X.cyr using bayan/sankoch' },
    { title: 'Verify', detail: 'adversarial parity check of the implemented fns vs the Rust oracle' },
  ],
}

const REPO = '/home/macro/Repos/stiva'

// Shared Cyrius idiom + memory-safety gotchas (learned the hard way this port).
const GOTCHAS = `## Cyrius idioms + the gotchas that caused real bugs this port
- i64-everywhere; \`fn name(a,b){...return X;}\`; enums with MODULE-prefixed members (last-def-wins globals).
- **Heap constructors MUST alloc**: \`var p=alloc(N*8); var pp:S=p; pp.a=…; return p;\`. NEVER \`var x=S{...};return x;\` (stack-local → dangling pointer → SEGFAULT).
- **Single-field struct = VALUE semantics**: for \`struct V{x;}\`, \`v.x=0\` does NOT write through the pointer and \`v.x\` reads the pointer. Use raw \`store64(p,val)\`/\`load64(p)\` at offset 0.
- **Map key type**: \`map_new()\` = cstr keys (string literals); \`map_new_str()\` = Str fat-pointer keys ONLY — mixing → OOB SEGFAULT. Match how the module reads it.
- **strstr returns a 0-based INDEX** (\`-1\` absent), NOT a C pointer: "contains" = \`strstr(h,n)>=0\`; value starts at \`h + strstr(h,k) + strlen(k)\`.
- No unwrap/panic. Fallible → integer sentinel (document it) or STIVA_ERR_* code + stiva_err_print(code, detail). Preserve EXACT Rust display strings.
- Study the existing exemplars: ${REPO}/src/error.cyr, ${REPO}/src/audit.cyr (JSON serialize+parse), ${REPO}/src/convert.cyr (parsing).`

// The three achievable deferrals — each names the stdlib lib that unblocks it.
const TASKS = [
  {
    module: 'build',
    goal: `Implement **parse_build_spec** (rust-old/src/build.rs): parse a Stivafile TOML string into the ported BuildSpec/ImageDef/BuildStep/BuildConfig structs (already defined at the top of src/build.cyr — reuse them). The Rust used \`toml::from_str\`; the Cyrius stdlib \`bayan\` provides a TOML reader — READ ${REPO}/lib/bayan.cyr for the API (toml_parse(src) → sections; toml_get(pairs,key), toml_get_array(pairs,key), toml_get_sections(sections,name), toml_section_name/pairs). Map the [image] table → ImageDef, each [[steps]] → BuildStep (type-tagged: run/copy/env/workdir/label/from_stage — see build_step_* constructors already in the file), [config] → BuildConfig. Return the BuildSpec pointer, or 0 (STIVA_ERR_* per rust-old's Err mapping) on a parse error / missing [image]. Move parse_build_spec OUT of the DEFERRED block. Leave build_cache_key + the tar.gz layer *builder* deferred (those need a tar writer).`,
    keep: 'the multi-stage tar.gz layer BUILDER + build_cache_key stay deferred (tar writer / not needed for parse)',
  },
  {
    module: 'image',
    goal: `Implement the **images.json index** (rust-old/src/image.rs): load_index / save_index / add_to_index / list / remove — they round-trip a list of Image records (id, reference, size_bytes, digest, created_at) through JSON at \`<root>/images.json\`. The Rust used serde_json; the Cyrius stdlib \`bayan\` provides a JSON codec — READ ${REPO}/lib/bayan.cyr for the API (json_parse(src), json_get(obj,key), json_get_int, json_v_arr_new/push/get/len, json_build/json_key/json_pair_new for serialize; audit.cyr is a hand-rolled-JSON precedent if you prefer). Store the index as a JSON array of image objects. Reuse the ported Image/ImageRef structs from image.cyr. Move these fns OUT of the DEFERRED block. Leave gc / verify_integrity / pull / push deferred (dir-walk + async).`,
    keep: 'gc / verify_integrity (blob dir-walk) and async pull/push/verify stay deferred',
  },
  {
    module: 'storage',
    goal: `Implement **unpack_layer** + **unpack_tar_gz** + **prepare_layers** (rust-old/src/storage.rs): decompress a gzip'd tar layer blob and extract it into a destination dir. gzip decompression: the Cyrius stdlib \`sankoch\` provides it — READ ${REPO}/lib/sankoch.cyr (gzip_decompress(src, src_len, dst, dst_cap) → decompressed length, or gzip_decompress_with_ratio_cap). tar extraction: there is NO tar lib — hand-roll a minimal USTAR reader (512-byte headers: name@0 (100b), mode@100, size@124 (12b octal), typeflag@156 ('0'/'\\0'=file, '5'=dir), then ceil(size/512) data blocks; two zero blocks = end). Create dirs (sys_mkdir) + write files (file_write_all) under dest, honoring the path. prepare_layers: for each Layer, dedup via a .unpacked marker, verify the blob exists in blobs/sha256/<hex> (else STIVA_ERR_LAYER_UNPACK), call unpack_layer, write the marker (ImageStore/Layer are ported in image.cyr — reference them). Move these OUT of the DEFERRED block. Leave unpack_tar_zstd deferred (no zstd in the stdlib).`,
    keep: 'unpack_tar_zstd stays deferred — zstd is genuinely absent from the stdlib (sankoch has gzip/xz/lz4/bzip2, not zstd)',
  },
]

const IMPL_SCHEMA = {
  type: 'object', additionalProperties: false,
  properties: {
    module: { type: 'string' },
    implemented: { type: 'array', items: { type: 'string' }, description: 'fns now implemented (moved out of DEFERRED)' },
    stdlib_added: { type: 'array', items: { type: 'string' }, description: 'stdlib modules newly referenced (e.g. bayan, sankoch)' },
    test_cyr: { type: 'string', description: 'Cyrius test_* fns exercising the new surface (no includes/main)' },
    test_fn_names: { type: 'array', items: { type: 'string' } },
    still_deferred: { type: 'array', items: { type: 'string' } },
    parses: { type: 'boolean' },
    summary: { type: 'string' },
  },
  required: ['module', 'implemented', 'test_cyr', 'test_fn_names', 'still_deferred', 'parses', 'summary'],
}
const VERIFY_SCHEMA = {
  type: 'object', additionalProperties: false,
  properties: {
    module: { type: 'string' },
    parity: { type: 'string', enum: ['faithful', 'gaps'] },
    gaps: { type: 'array', items: { type: 'string' } },
    verdict: { type: 'string' },
  },
  required: ['module', 'parity', 'gaps', 'verdict'],
}

log(`closing ${TASKS.length} achievable deferrals: ${TASKS.map(t => t.module).join(', ')}`)

const results = await pipeline(
  TASKS,
  (t) => agent(
    `You are closing a DEFERRED gap in one module of the stiva OCI runtime (Cyrius). Implement it faithfully vs the Rust oracle in ${REPO}/rust-old/, using the EXISTING Cyrius stdlib (the earlier port wrongly deferred these as "no codec" — the stdlib has them).\n\n${GOTCHAS}\n\n=== MODULE: ${t.module} (edit ${REPO}/src/${t.module}.cyr in place) ===\n${t.goal}\n\nKEEP DEFERRED: ${t.keep}.\n\nSteps: read ${REPO}/rust-old/src/${t.module}.rs (the oracle) + ${REPO}/src/${t.module}.cyr (current) + the named stdlib lib. Implement the fns in src/${t.module}.cyr (move them out of the DEFERRED block; shrink that block to only what's still deferred). Run \`cd ${REPO} && cyrius check src/${t.module}.cyr\` — must parse. Provide test_* fns (test_group/assert/assert_eq) exercising the new surface. Return ONLY the structured object; write no file other than src/${t.module}.cyr.`,
    { label: `impl:${t.module}`, phase: 'Implement', schema: IMPL_SCHEMA, effort: 'high' }
  ),
  (impl, t) => agent(
    `Adversarially verify the newly-implemented ${t.module} surface for parity vs the Rust oracle.\nOracle: ${REPO}/rust-old/src/${t.module}.rs\nImpl:   ${REPO}/src/${t.module}.cyr\nImplemented: ${JSON.stringify(impl ? impl.implemented || [] : [])}\nRead both. Check the implemented fns match the Rust behavior (same fields/branches/error cases). Apply the memory-safety gotchas: flag any \`var x=Struct{...};return x\` (dangling), single-field-struct \`.field\` write, or map_new/map_new_str key-type mismatch, or strstr-as-pointer. Report concrete gaps only.`,
    { label: `verify:${t.module}`, phase: 'Verify', schema: VERIFY_SCHEMA, effort: 'high' }
  ).then(v => ({ impl, verify: v, module: t.module }))
)

return { results: results.filter(Boolean) }
