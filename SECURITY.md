# Security Policy

## Scope

Stiva is an OCI-compatible container runtime that manages container lifecycle,
networking, and orchestration. It executes processes in sandboxed environments
via kavach and manages network interfaces, firewall rules, and filesystem mounts.

The primary security-relevant surface areas are:

- **Container isolation** — process isolation via kavach sandboxes (seccomp,
  Landlock, namespaces, cgroups, NO_NEW_PRIVS). A sandbox escape would be critical.
- **File descriptor hygiene** — inherited fds are closed (3..1024) in pre_exec
  to prevent CVE-2024-21626-class escapes via `/proc/self/fd/N`.
- **Overlay filesystem** — layer unpacking from tar archives (gzip + zstd).
  Malicious tar entries (symlink attacks, path traversal) could escape the rootfs.
- **Network isolation** — veth pairs, bridge networks, nftables rules, network
  policies. Misconfigured rules could leak traffic between containers.
- **Registry authentication** — bearer tokens cached in memory, credentials
  optionally persisted to `~/.stiva/credentials.json`. Credential leakage via
  logs or error messages.
- **OCI manifest parsing** — JSON deserialization of untrusted manifests from
  registries. Manifest digest verification (Docker-Content-Digest header) defends
  against MITM.
- **Port mapping** — DNAT rules expose container ports to the host network.
  Misconfigured mappings could expose unintended services.
- **Build cache** — content-addressable cache keyed by digest. Cache poisoning
  is prevented by SHA-256 verification on all blob reads.

## Supported Versions

| Version | Supported |
|---------|-----------|
| 2.0.x   | Yes       |
| 1.x     | Security fixes only |
| < 1.0   | No        |

## Reporting a Vulnerability

If you discover a security vulnerability, please report it privately:

1. **Do not** open a public GitHub issue.
2. Email: security@agnos.dev (or use GitHub Security Advisories).
3. Include: description, reproduction steps, impact assessment.
4. We will acknowledge within 48 hours and aim to patch within 7 days.

## Security Practices

- All container processes run through kavach sandbox with seccomp + Landlock + NO_NEW_PRIVS.
- File descriptors 3..1024 closed in pre_exec before entering container namespaces.
- stdin set to null for all spawned sandbox processes.
- Tar archive unpacking uses `set_overwrite(true)` with `set_preserve_permissions(true)`.
- Manifest digests verified against Docker-Content-Digest header on pull.
- Blob digests are SHA-256 verified on every write.
- Image lookups use exact match (not substring) to prevent confusion attacks.
- Build cache keyed by content digest — cache poisoning impossible without hash collision.
- Network bridge creation requires root — stiva degrades gracefully without it.
- Port spec parsing validates input before constructing nftables rules.
- ID-mapped mounts (`X-mount.idmap=`) used for rootless bind mounts.

## Audited CVEs

See [docs/security-audit-log.md](docs/security-audit-log.md) for the full audit trail.

| CVE | Severity | Status |
|-----|----------|--------|
| CVE-2024-21626 | Critical | Mitigated (fd cleanup + stdin null) |
| CVE-2024-24557 | Medium | Fixed (tag-keyed cache removed, exact match) |
| CVE-2024-3154 | High | Not applicable (no gitRepoVolume) |
| RUSTSEC-2025-0067/68 | N/A | Fixed (serde_yaml → serde-saphyr) |
