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

### Library (v3.0.x — planned, blocking Stiva facade, not yet wired in the Cyrius port)

```rust
use stiva::runtime::RuntimeConfig;

let config = RuntimeConfig {
    rootless: true,
    // ...
};
```

When rootless is enabled, stiva adds OCI runtime-spec v1.2.0 ID-mapped mount options (`X-mount.idmap=`) so bind-mounted volumes work correctly under user namespaces.

## Seccomp Filters

kavach applies seccomp-bpf filters to restrict the syscalls available inside the container. The default policy blocks dangerous syscalls while allowing normal operation.

Seccomp filtering is part of the `SandboxPolicy` passed to kavach when creating a sandbox. The policy is reflected in the security score.

## Landlock

Landlock LSM restricts filesystem access for the container process. kavach uses Landlock to ensure containers can only access their own rootfs and explicitly mounted volumes.

Landlock support depends on kernel version (5.13+). When unavailable, kavach falls back to other isolation mechanisms.

## Credential Management

Secrets are injected into containers through kavach's `CredentialProxy` and `SecretRef` system. This keeps credentials out of container configs and image layers.

### CLI

```bash
stiva run -s DB_PASSWORD=secret123 -s API_KEY=abc myapp:latest
```

(Detached secret injection with `run -d` lands in v3.1.)

### Library (v3.0.x — planned, blocking Stiva facade, not yet wired in the Cyrius port)

```rust
use kavach::SecretRef;

let secrets = vec![
    SecretRef { name: "DB_PASSWORD".into(), /* ... */ },
    SecretRef { name: "API_KEY".into(), /* ... */ },
];
```

Key properties:

- Secrets are **not stored** in the container config or state.json.
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
happen behind an operator's back. The Rust original instead gated this on a
per-container persisted `scan_policy`; that field is not yet round-tripped through
`state.json` in the Cyrius port, so the flag stands in for it.

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

### Library (v3.0.x — planned, blocking Stiva facade, not yet wired in the Cyrius port)

```rust
let score = stiva.security_score();
// score.value is 0..100

// Score a specific backend + policy combination
let score = stiva.score_backend(backend, &policy)?;
```

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
8. **Secret rotation** -- use `CredentialProxy` for runtime injection; rotate secrets without rebuilding images.

## Further Reading

- [Architecture](../architecture.md) -- dependency stack and kavach integration
- [Security audit log](../security-audit-log.md) -- audit trail
- [ADR-0001](../adr/0001-kavach-sandbox-abstraction.md) -- kavach sandbox design decision
