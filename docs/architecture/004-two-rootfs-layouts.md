# 004 — A container rootfs has two possible layouts, and callers must handle both

`run` / `create` prefer an **overlay** mount: `{croot}/lower*` + `{croot}/upper` +
`{croot}/work`, merged at `{croot}/rootfs`. Overlay mounts need privileges that an
unprivileged or containerised environment often does not have, so when the mount fails stiva
falls back to **flattening** — extracting every layer into `{croot}/rootfs` directly.

The fallback is controlled by `STIVA_ROOTFS_FALLBACK` (`copy`, the default, or `none`), and
a flattened rootfs is marked by the presence of **`{croot}/.rootfs-flattened`**.

An unrecognised value for that variable warns and uses `copy`; it does not silently guess,
because guessing "on" for `STIVA_ROOTFS_FALLBACK=off` would be the opposite of what an
operator asked for.

## Why every rootfs consumer has two code paths

The layouts differ in where "what the container changed" lives:

| | overlay | flattened |
|---|---|---|
| changed set | `{croot}/upper` | nowhere — the whole tree is one merged copy |
| deletions | overlayfs whiteouts: char device 0:0, plus `trusted.overlay.opaque` dirs | not representable |
| how to diff | walk `upper` | walk `{croot}/rootfs` against the layer dirs |

`container_manager_diff` is the clearest example: it reads the marker and takes one of two
scans. Anything else that reasons about "what changed in this container" has to do the same
— a consumer that only knows about `upper` reports **no changes at all** on a flattened
rootfs, which reads as "clean" rather than as "unsupported".

Note the two whiteout vocabularies do not overlap and both are live in this codebase:
**overlayfs** whiteouts (char 0:0 / opaque xattr) describe a mounted container's upper dir;
**OCI** whiteouts (`.wh.<name>` / `.wh..wh..opq`) are markers *inside layer tarballs*,
applied by `storage` during unpack. They are unrelated mechanisms with the same job.
