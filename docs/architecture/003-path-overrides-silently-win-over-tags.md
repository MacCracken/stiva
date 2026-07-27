# 003 — A `path` dep override silently wins over its `tag`, and the lock cannot tell you

In `cyrius.cyml`, a dep block may carry both:

```toml
[deps.sakshi]
# path = "../sakshi"
tag = "2.4.6"
```

If the `path` line is uncommented, **`path` wins outright**. `tag` is not consulted, no
warning is printed, and — this is the part that bites — **`cyrius.lock` records no dep name,
version or revision under path resolution**. It writes only bare `<sha256>  lib/<file>`
lines. There is nothing in the committed artifacts that says which sakshi you built against.

## How this actually failed

Every local build and test ran against whatever the sibling checkouts happened to be.
`~/Repos/sakshi` sat **3 commits past its 2.4.6 tag**, so the local suite was green against a
sakshi that exists at no tag. CI, which has no sibling checkouts, resolved the tag and got
different bytes — and failed, with no diff to look at, because the two lock files are not
even the same *format*.

That last point deserves its own line: **the lock's format depends on the resolution mode.**
Tag resolution writes `commit <sha> <name> <url> <tag>` header lines and orders the hashes
differently; path resolution writes only file hashes. A `git diff --exit-code cyrius.lock`
guard can therefore **never pass** — the first CI guard tried exactly that and had to be
reverted.

## The standing arrangement

- Every `path = "../<dep>"` line in `cyrius.cyml` is **commented out**, deliberately, with a
  pointer to the explanation block in that file.
- `cyrius deps` resolves from tags on every machine, so a clean checkout with no siblings
  builds the same bytes CI does.
- `.github/workflows/ci.yml` compares the resolved dep bundles against `git show
  HEAD:cyrius.lock` — scoped to the ~10 dep-owned `lib/<basename>.cyr` files, **not** all of
  `lib/`, which also holds stdlib files vendored from the toolchain snapshot.

To develop a dep alongside stiva: uncomment its `path` line, work, then **tag and release the
dep and bump the `tag` here** before committing `cyrius.lock`. Never commit a lock produced
through a path override.
