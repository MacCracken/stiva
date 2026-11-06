# Security Policy

## Scope

Stiva is an OCI-compatible container runtime that manages container lifecycle, images and
orchestration. It executes processes in sandboxes through kavach. This file says what is enforced
today, and where something is not, it says so.

The security-relevant surface areas are:

- **Container isolation** — process isolation via kavach sandboxes. A sandbox escape would be
  critical. What a container actually gets depends on the path; see below.
- **File descriptor hygiene** — every descriptor above stderr is marked close-on-exec before a
  payload starts, to prevent CVE-2024-21626-class escapes via `/proc/self/fd/N`.
- **Layer and archive unpacking** — tar archives (gzip + zstd) from a registry or a `load`.
  Malicious entries (symlink attacks, path traversal, hard links out of the tree) could escape the
  rootfs, and a decompression bomb could exhaust memory before a single entry is read. Output is
  bounded at 1000:1 against the compressed length and at the allocator's ceiling, and entries are
  walked by file descriptor, never following a link.
- **`cp`, `export` and `build`** — all three read or write paths a container or a build context can
  change while they run, so they walk by file descriptor with `O_NOFOLLOW`.
- **Registry authentication** — bearer tokens cached in memory. Credentials are bound to the host
  they were configured for, and a token realm must be on that origin. `pull` and `push` do not read
  a credential file today.
- **OCI manifest parsing** — JSON from a registry, covered by the fuzz harness. Manifest digests are
  verified against the `Docker-Content-Digest` header.
- **Build cache** — keyed by a metadata fingerprint, not by content. See below.

## Supported Versions

| Version | Supported |
|---------|-----------|
| 3.1.x   | Yes       |
| 3.0.x   | Yes       |
| 1.x     | Security fixes only |
| < 1.0   | No        |

## Reporting a Vulnerability

If you discover a security vulnerability, please report it privately:

1. **Do not** open a public GitHub issue.
2. Email: security@agnos.dev (or use GitHub Security Advisories).
3. Include: description, reproduction steps, impact assessment.
4. We will acknowledge within 48 hours and aim to patch within 7 days.

## What a container actually gets

| Path | Seccomp | NO_NEW_PRIVS | PID namespace | Rootfs entry |
|------|---------|--------------|---------------|--------------|
| `stiva run`, OCI backend (the default where runc is present) | ⛔ **no** | ⛔ **no** | yes | runc |
| `stiva run`, process backend | `basic` profile | yes | no | chroot + user and mount namespaces |
| `stiva run -d` | `basic` profile | yes | ⛔ no | chroot + user and mount namespaces |
| `stiva exec` | ⛔ **no** | yes | the container's | nsenter |

⛔ **The default foreground path has no seccomp filter and no NO_NEW_PRIVS.** kavach's
`oci_generate_spec` emits neither. Until that changes, use `--backend process` or `run -d` where you
need them. See the roadmap's upstream section.

⛔ **Other things that are not there yet:**

- **Landlock is not applied** on any path. stiva builds from `policy_basic()`, which carries no
  Landlock rules, and nothing calls `policy_landlock_add`. Filesystem confinement comes from the
  rootfs entry and the mount namespace.
- **Every container runs as root.** `user` is recorded and never applied (roadmap v3.2.2).
- **Containers get no secrets**, and **no bind mounts**: `RuntimeSpec.mounts` is write-only (roadmap
  v3.2.2).
- **Containers have no network of their own** apart from rootless slirp4netns or pasta on `run -d`.
  The bridge, NAT and DNS code has no caller (roadmap v3.3.1), and `build_sandbox` turns networking
  off.
- **stiva creates no per-container cgroup** (roadmap v3.2.1), so a payload's limits and stats are
  those of whatever started it. `pause` and `update` refuse rather than act on a shared cgroup.

## Security Practices

- Descriptors above stderr are marked close-on-exec before every payload exec, and stdin is null for
  spawned sandbox processes.
- Tar extraction refuses traversal, refuses to write through a symlink, checks every header
  checksum, creates files with `O_EXCL|O_NOFOLLOW`, and applies directory modes last. Hard links
  must stay inside the tree. An archive given to `load` is extracted with links refused outright.
- Manifest digests are verified against `Docker-Content-Digest` on pull, and blobs are verified by
  SHA-256 and size as they download. ⚠ A blob is **not** re-verified when read back: a byte flipped
  on disk is caught by `run`, but `save` passes it on (roadmap v3.3.0).
- Redirects never carry credentials to another origin, and a redirect that ends in a 401 is refused.
- Every network read and connect in the registry client has a deadline.
- Image lookups use exact match, not substring, to prevent confusion attacks.
- The build cache is keyed by a **metadata** fingerprint (path, mode, uid, gid, size, ns-mtime) over
  the copy source, not by content. An edit preserving all of those will not invalidate it, and
  `rm -rf {root}/cache` forces a rebuild.
- `state.json`, `cron.json` and the credentials file are written atomically at mode 0600, because
  they hold environment variables, command lines and passwords. `layers/` and `containers/` are 0700.
- Container names, image names, tags, DNS names and port protocols are validated when they arrive,
  and a value an image or a payload chose has its control bytes replaced before `ps`, `images` or
  `top` print it.
- The audit log's `user` is the effective uid from the kernel, not `$USER`, and every line stays
  valid JSON whatever a name holds.
- The nftables ruleset is held in a memfd, not in a file a local user could reach.

## Audited CVEs

See [docs/security-audit-log.md](docs/security-audit-log.md) for the full audit trail.

| CVE | Severity | Status |
|-----|----------|--------|
| CVE-2024-21626 | Critical | Mitigated (close-on-exec sweep, stdin null) |
| CVE-2024-24557 | Medium | Fixed (tag-keyed cache removed, exact match) |
| CVE-2024-3154 | High | Not applicable (no gitRepoVolume) |
| RUSTSEC-2025-0067/68 | N/A | Not applicable since the port (no serde) |
