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

⛔ **`rootless` is a config field that `build_sandbox` never reads** (`src/runtime.cyr:798-897`).
It feeds only the mount and namespace lists on `RuntimeSpec`, and **both are still write-only**
(roadmap v3.1.0 items 9 and 3). stiva creates no user namespace of its own.

The rootless *network* helper is real, but it is selected independently — by `is_unprivileged()`,
from the real uid and capabilities, not from this flag.

When rootless is set, `volume_mounts` appends OCI runtime-spec v1.2.0 ID-mapped mount options
(`X-mount.idmap=`, `src/runtime.cyr:335`) to `RuntimeSpec.mounts` — but since that field has no
reader, **stiva performs no mounts at all**, so volume bind-mounting does not happen yet. The
idmap work is assembled ahead of the consumer that will use it.

## Seccomp Filters

kavach applies seccomp-bpf filters to restrict the syscalls available inside the container. The default policy blocks dangerous syscalls while allowing normal operation.

Seccomp filtering is part of the `SandboxPolicy` passed to kavach when creating a sandbox. The policy is reflected in the security score.

## Landlock

⛔ **Landlock is not currently enabled for stiva containers.**

kavach implements it (`security_apply_landlock`, `policy_landlock_add`), but `build_sandbox`
builds its sandbox from `policy_basic()` (`src/runtime.cyr:800`), which carries **no landlock
rules** — and kavach applies no ruleset at all when the rule count is zero. No stiva code path
calls `policy_landlock_add`; the only occurrence of the word in `src/` is a comment.

Filesystem confinement today comes from the **rootfs entry (chroot) and the mount namespace**,
not from Landlock. Do not rely on a Landlock boundary that is not being installed.

## Credential Management

> ⚠️ **Containers currently receive no secrets.** This is the single largest gap in the
> port, and it is roadmap **v3.1.0 item 1**. Concretely: `secrets` serializes as an empty
> array, **nothing in stiva threads one into `build_sandbox`**, and `stiva run` registers no
> `-s` flag — passing one is a usage error, not a silent no-op.
>
> ⚠ This is **not** blocked upstream. kavach ships `SecretRef`, `CredentialProxy`,
> `credential_proxy_env_vars` and `credential_inject_files`, and the env channel they needed
> landed in kavach 3.12.0 (`config_env`, which stiva already uses for image env). It is
> stiva-side work. If you
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

**Automatically, on the foreground `stiva run` path only.** kavach's `sandbox_exec`
gate-applies **its own default policy** — one stiva never chose (block=CRITICAL,
quarantine=HIGH) — so a foreground run whose output trips BLOCK/QUARANTINE fails with
`externalization blocked` and persists no output. It needs no flag and cannot be configured
from stiva.

⚠ It does **not** cover `stiva run -d` or `stiva exec`, which take different paths
(`sandbox_spawn` and stiva's own `nsenter` respectively) and are ungated.

**On demand, when reading back a log.** `stiva logs --scan` routes the stored log
body through the gate before printing it:

```bash
stiva logs <ID> --scan
```

A `PASS` prints normally. A `WARN` prints with secrets replaced by
`[REDACTED:<category>]`. A `QUARANTINE` or `BLOCK` prints nothing and exits non-zero
with `stiva: sandbox error: output scan: block`.

It is **opt-in** because scanning changes what `logs` prints, which should never
happen behind an operator's back. The per-container `scan_policy` the Rust original gated
this on is round-tripped through `state.json` (`src/container.cyr:600`, `:710`) and honoured
by `logs` (`src/main.cyr:654`) — but ⛔ **nothing sets it yet**: there is no `run` flag and no
Stivafile key, so it is always absent in practice (roadmap v3.1.0 item 1). **`--scan` is
therefore the only way to scan a log today.**

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
```

### Library

Scoring goes through kavach's `score_backend(backend, policy)`. stiva calls it from **`info`
only** (`security_score()`, `src/runtime.cyr:1104` → `src/main.cyr:372`), over `policy_basic()`
and the best available backend. ⚠ The per-container entry point `security_score_for` exists
(`src/runtime.cyr:1115`) but **has no caller** — `stiva inspect` reports no score.

Factors that increase the score:

- Seccomp filters enabled
- Landlock filesystem restrictions
- User namespace isolation (rootless)
- Reduced capability set
- Resource limits (memory, CPU, PID)

## CVE Mitigation Practices

1. **Minimal base images** -- use small base images (e.g., `alpine`) to reduce attack surface.
2. **Non-root user** -- ⛔ **not yet enforced.** `user = "nobody"` in Stivafile `[config]` is
   recorded in the image config and in `state.json`, but `RuntimeSpec.user`
   (`src/runtime.cyr:536`) has no reader — nothing drops privileges before exec (roadmap
   v3.1.0 item 10). **Until it lands, treat every container as running as root** and rely on
   the sandbox boundary, not the user field.
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
