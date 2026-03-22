# Security Policy

## Scope

Stiva is an OCI-compatible container runtime that manages container lifecycle,
networking, and orchestration. It executes processes in sandboxed environments
via kavach and manages network interfaces, firewall rules, and filesystem mounts.

The primary security-relevant surface areas are:

- **Container isolation** — process isolation via kavach sandboxes (seccomp,
  Landlock, namespaces, cgroups). A sandbox escape would be critical.
- **Overlay filesystem** — layer unpacking from tar archives. Malicious tar
  entries (symlink attacks, path traversal) could escape the rootfs.
- **Network isolation** — veth pairs, bridge networks, and nftables rules.
  Misconfigured rules could leak traffic between containers.
- **Registry authentication** — bearer tokens cached in memory. Credential
  leakage via logs or error messages.
- **OCI manifest parsing** — JSON deserialization of untrusted manifests from
  registries. Malformed input could trigger unexpected behavior.
- **Port mapping** — DNAT rules expose container ports to the host network.
  Misconfigured mappings could expose unintended services.
- **Compose files** — TOML parsing of user-provided compose files. While not
  Turing-complete, malformed input could cause resource exhaustion.

## Supported Versions

| Version | Supported |
|---------|-----------|
| 0.22.x  | Yes       |
| < 0.22  | No        |

## Reporting a Vulnerability

If you discover a security vulnerability, please report it privately:

1. **Do not** open a public GitHub issue.
2. Email: security@agnos.dev (or use GitHub Security Advisories).
3. Include: description, reproduction steps, impact assessment.
4. We will acknowledge within 48 hours and aim to patch within 7 days.

## Security Practices

- All container processes run through kavach sandbox with seccomp + Landlock.
- Tar archive unpacking uses `set_overwrite(true)` with `set_preserve_permissions(true)`.
- Registry tokens are cached in memory only (not persisted to disk).
- Blob digests are SHA-256 verified on every write.
- Network bridge creation requires root — stiva degrades gracefully without it.
- Port spec parsing validates input before constructing nftables rules.
