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
| `stiva run`, OCI backend (the default where runc is present) | `basic` profile | yes | yes | runc |
| `stiva run`, process backend | `basic` profile | yes | no | chroot + user and mount namespaces |
| `stiva run -d` | `basic` profile | yes | ⛔ no | chroot + user and mount namespaces |
| `stiva exec` | ⛔ **no** | yes | the container's | nsenter |

`scripts/cli-smoke.sh`'s "confinement" group reads these from inside the payload, except the `exec`
row. The first row has held since kavach 3.14.4; through 3.14.3 a payload on the default path read
`Seccomp: 0` and `NoNewPrivs: 0`, and its `mount` succeeded.

The `basic` profile is kavach's exec filter, a deny list and not an allowlist. A payload calling
`mount`, `unshare`, `ptrace`, `init_module`, `bpf` and the like is killed, and the container exits 159.

**What else each path holds, measured from a payload under kavach 3.14.5:**

| Path | Capabilities | `/proc` |
|------|--------------|---------|
| `stiva run`, OCI backend | 12, `CapEff: 00000000a00405fb`: kavach's default, Docker's set less CAP_NET_RAW and CAP_MKNOD | masked, and `/proc/sys` read-only |
| `stiva run`, process backend | ⛔ all of them, in a user namespace | none mounted |
| `stiva run -d` | ⛔ all of them, in a user namespace | none mounted |
| `stiva exec` | ⛔ all of them, in a user namespace | none mounted |

The default foreground path is the bounded one. **`run -d` and the process backend are not**: their
capabilities are held in a user namespace that owns nothing on the host, and they share the host's
PID namespace. Prefer the OCI backend where that matters, and run stiva rootless where you can.

**Output scanning warns; it does not stop a foreground run.** A secret kavach recognises is replaced
with `[REDACTED:…]` in the output and the stored log, and anything else flagged is printed with a
warning on stderr. ⚠ That includes a card number or an IBAN, which the gate flags and cannot redact, and
it misses some credentials outright, such as Slack tokens and `password=…`. Do not treat it as a
guarantee that a secret cannot leave a container. `run -d` and `exec` are not scanned at all. A
container's own `scan_policy` is honoured, so a caller can make a run refuse. `docs/guides/security.md`
has what each kind of finding does.

⚠ **On the OCI path the filter is only as complete as runc's libseccomp**, which skips a syscall name
it does not know, silently. Under libseccomp 2.6.0 that is `open_tree_attr`, which reads EPERM for want
of CAP_SYS_ADMIN instead. An older libseccomp may skip more.

⛔ **Other things that are not there yet:**

- **`exec` has no seccomp filter**: a process entering a container that has one runs without it
  (roadmap v3.2.2).
- **Landlock is not applied** on any path. stiva builds from `policy_basic()`, which carries no
  Landlock rules, and nothing calls `policy_landlock_add`. Filesystem confinement comes from the
  rootfs entry and the mount namespace.
- **Every container runs as root, and an image's `User` is not applied.** `user` is recorded and never
  applied (roadmap v3.2.2), so an image built to run as `nobody` runs as root here, with a warning in
  the log. This is the largest gap left.
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
