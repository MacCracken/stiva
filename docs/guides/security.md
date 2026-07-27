# Security Hardening Guide

Stiva integrates with kavach for sandbox isolation. This guide covers the security features available for hardening container workloads.

## Rootless Containers

Stiva supports rootless operation via Linux user namespaces. When `rootless` is enabled, the container runs with UID/GID remapping so the process inside the container appears to run as root but is actually an unprivileged user on the host.

### CLI

```bash
# Rootless is configured at the runtime level.
# The Stivafile can set a non-root user:
# [config]
# user = "nobody"
```

### Library

`rootless` is a field on the runtime config; `build_sandbox` threads it into the kavach
policy, and the rootless network path (slirp4netns/pasta) is selected from the same flag.

When rootless is enabled, stiva adds OCI runtime-spec v1.2.0 ID-mapped mount options (`X-mount.idmap=`) so bind-mounted volumes work correctly under user namespaces.

## Seccomp Filters

kavach applies seccomp-bpf filters to restrict the syscalls available inside the container. The default policy blocks dangerous syscalls while allowing normal operation.

Seccomp filtering is part of the `SandboxPolicy` passed to kavach when creating a sandbox. The policy is reflected in the security score.

## Landlock

Landlock LSM restricts filesystem access for the container process. kavach uses Landlock to ensure containers can only access their own rootfs and explicitly mounted volumes.

Landlock support depends on kernel version (5.13+). When unavailable, kavach falls back to other isolation mechanisms.

## Credential Management

> ⚠️ **Containers currently receive no secrets.** This is the single largest gap in the
> port, and it is roadmap **v3.1.0 item 1**. Concretely: `secrets` serializes as an empty
> array, `build_sandbox` has no kavach setter to thread a `SecretRef` through, and `stiva
> run` registers no `-s` flag — passing one is a usage error, not a silent no-op. If you
> need a credential inside a container today, you must supply it by a mechanism stiva does
> not manage (a bind-mounted file, an image layer), with the exposure that implies.

The design it will be built on is kavach's `CredentialProxy` / `SecretRef` system, which
keeps credentials out of container configs and image layers. The intended properties, none
of which are load-bearing yet:

- Secrets are **not stored** in the container config or `state.json`.
- Secrets are injected at runtime via environment variables or files.
- `stiva inspect` does not expose secret values.

## Output Scanning (ExternalizationGate)

kavach's externalization gate scans container output for leaked secrets, PII, and
sensitive data. It applies in two places, and the distinction matters:

**Automatically, on every container exec.** kavach's `sandbox_exec` runs the gate
itself, so a container whose output trips a `BLOCK` verdict never completes — the
exec fails with `externalization blocked` and no output is persisted. This has always
been on; it needs no flag and cannot be turned off from stiva.

**On demand, when reading back a log.** `stiva logs --scan` routes the stored log
body through the gate before printing it:

```bash
stiva logs <ID> --scan
```

A `PASS` prints normally. A `WARN` prints with secrets replaced by
`[REDACTED:<category>]`. A `QUARANTINE` or `BLOCK` prints nothing and exits non-zero
with `stiva: sandbox error: output scan: block`.

It is **opt-in** because scanning changes what `logs` prints, which should never
happen behind an operator's back. The per-container `scan_policy` the Rust original
gated this on **is** round-tripped through `state.json` now, so a container can carry
its own policy; `--scan` remains the explicit per-read override.

The library entry point is `scan_output(result, policy)` in `src/runtime.cyr`, with
`scan_output_last_verdict()` / `scan_output_last_findings()` for the verdict and
finding count of the most recent call.

The gate runs three scanner categories:

1. **Secrets** -- API keys, tokens, passwords
2. **Code** -- source code patterns that should not appear in output
3. **Data** -- PII patterns (emails, phone numbers, etc.)

Severity maps to verdict through the policy's thresholds: `CRITICAL` → `BLOCK`,
`HIGH` → `QUARANTINE`, anything above `INFO` → `WARN`.

## Security Scoring

Stiva exposes a security score (0--100) via kavach's `StrengthScore` system. The score reflects the isolation strength of the current runtime configuration.

### CLI

```bash
stiva info              # includes overall security score
stiva inspect <id>      # includes per-container score
```

### Library

Scoring goes through kavach's `score_backend(backend, policy)`; stiva calls it from
`info` (host-level) and `inspect` (per-container, over that container's actual policy).

Factors that increase the score:

- Seccomp filters enabled
- Landlock filesystem restrictions
- User namespace isolation (rootless)
- Reduced capability set
- Resource limits (memory, CPU, PID)

## CVE Mitigation Practices

1. **Minimal base images** -- use small base images (e.g., `alpine`) to reduce attack surface.
2. **Non-root user** -- set `user = "nobody"` in Stivafile `[config]`.
3. **Read-only rootfs** -- mount the container rootfs read-only where possible.
4. **Resource limits** -- set memory, CPU, and PID limits in `SandboxPolicy` to prevent resource exhaustion.
5. **Dependency auditing** -- run `cyrius audit` regularly on stiva itself.
6. **Image provenance** -- verify image digests after pull. Stiva stores and validates content digests for all layers.
7. **Network isolation** -- place containers on separate bridge networks to limit lateral movement.
8. **Secret rotation** -- planned, once secret injection lands (v3.1.0 item 1). Until then,
   treat any credential a container can read as baked in and rotate it out of band.

## Further Reading

- [Architecture](../architecture.md) -- dependency stack and kavach integration
- [Security audit log](../security-audit-log.md) -- audit trail
- [ADR-0001](../adr/0001-kavach-sandbox-abstraction.md) -- kavach sandbox design decision
