# Security Audit Log

Tracks CVEs and security advisories reviewed against stiva, with status and remediation details.

## Format

Each entry records:
- **CVE ID** and severity
- **Affected software** (the upstream project where the CVE was found)
- **Status**: `mitigated`, `not-applicable`, `in-progress`, `fixed`
- **Details**: what was checked, what was done

---

## Audited CVEs

### CVE-2024-21626 — Container escape via leaked host fd (Critical)

- **Affected**: runc < 1.1.12
- **Vector**: Leaked file descriptor to host `/sys/fs/cgroup` allows container escape via `WORKDIR` or `process.cwd` pointing to `/proc/self/fd/<N>`
- **Status**: mitigated
- **Audit date**: 2026-04-02
- **Remediation**:
  - Added fd cleanup (`libc::close` for fds 3..1024) in `pre_exec` hook in `exec_in_container()` (`src/runtime.rs`)
  - Added `stdin(Stdio::null())` to prevent stdin fd inheritance in `exec_in_container()`
  - kavach's `execute_with_timeout()` and `spawn_process()` inherit stdin — tracked as upstream fix needed in kavach
  - kavach's `build_command()` pre_exec hook does not include fd cleanup — tracked as upstream fix needed in kavach

### CVE-2024-24557 — Build cache poisoning (Medium)

- **Affected**: Docker/Moby
- **Vector**: Malicious image layer can poison build cache, causing subsequent builds to use attacker-controlled layers
- **Status**: fixed
- **Audit date**: 2026-04-02
- **Findings**:
  - All blob storage is content-addressable (`blobs/sha256/<hex>`) with mandatory digest verification in `store_blob()` — secure
  - Layer deduplication is digest-based, not tag-based — secure
  - Build layers are keyed by computed SHA-256, not step index — secure
  - Image index lookup used `.contains()` substring matching on full_ref — could match unintended images — **fixed** to exact match
  - `store_manifest_ref()` stored manifests at tag-keyed paths (`manifests/{registry}/{repo}/{tag}.json`) but was never read back — dead code posing a cache poisoning risk if future code added read-back — **removed**

### CVE-2024-3154 — Arbitrary command execution via crafted volume (High)

- **Affected**: cri-o
- **Vector**: Crafted `gitRepoVolume` spec allows arbitrary command execution on the host
- **Status**: not-applicable
- **Audit date**: 2026-04-02
- **Notes**: stiva does not support `gitRepoVolume` or any volume type that fetches remote content. Volume mounts are bind-mount only (`/host:/container[:ro]`). No action required.

---

## RustSec Advisories

### RUSTSEC-2025-0067 / RUSTSEC-2025-0068 — libyml / serde_yml unsound and unmaintained

- **Affected**: `serde_yml` (all versions), `libyml` (all versions)
- **Vector**: Unsound memory handling in YAML parsing
- **Status**: fixed
- **Audit date**: 2026-04-02
- **Remediation**: Replaced `serde_yaml` → `serde_yml` → `serde-saphyr` (safe, maintained fork with pure-Rust YAML parser). Only usage was `compose_yaml_to_toml()` in `src/convert.rs`.
