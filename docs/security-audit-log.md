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
- **Remediation** (as of the Cyrius port):
  - `_stor_cloexec_fds` marks every descriptor above stderr close-on-exec in the child of `_exec_capture2`, in the rootless helper's child, and before `spawn_container` and `exec_container` call into kavach
  - stdin is null for spawned sandbox processes
  - kavach's own sweep stops at 1024, which is why stiva does this first (see the roadmap's upstream section)

### CVE-2024-24557 — Build cache poisoning (Medium)

- **Affected**: Docker/Moby
- **Vector**: Malicious image layer can poison build cache, causing subsequent builds to use attacker-controlled layers
- **Status**: fixed
- **Audit date**: 2026-04-02
- **Findings**:
  - All blob storage is content-addressable (`blobs/sha256/<hex>`), and `image_store_store_blob` verifies the digest on write — secure
  - Layer deduplication is digest-based, not tag-based — secure
  - Build layers are stored by computed SHA-256. The build cache is keyed by a metadata fingerprint, not by content: see SECURITY.md
  - Image index lookup used `.contains()` substring matching on full_ref — could match unintended images — **fixed** to exact match
  - `store_manifest_ref()` stored manifests at tag-keyed paths (`manifests/{registry}/{repo}/{tag}.json`) but was never read back — dead code posing a cache poisoning risk if future code added read-back — **removed**

### CVE-2024-3154 — Arbitrary command execution via crafted volume (High)

- **Affected**: cri-o
- **Vector**: Crafted `gitRepoVolume` spec allows arbitrary command execution on the host
- **Status**: not-applicable
- **Audit date**: 2026-04-02
- **Notes**: stiva does not support `gitRepoVolume` or any volume that fetches remote content. It has no volumes at all yet (roadmap v3.2.2). No action required.

---

## RustSec Advisories

### RUSTSEC-2025-0067 / RUSTSEC-2025-0068 — libyml / serde_yml unsound and unmaintained

- **Affected**: `serde_yml` (all versions), `libyml` (all versions)
- **Vector**: Unsound memory handling in YAML parsing
- **Status**: not-applicable since the port
- **Audit date**: 2026-04-02
- **Remediation**: At the time, the Rust crate replaced `serde_yaml` → `serde_yml` → `serde-saphyr`. The Cyrius port has no serde: `compose_yaml_to_toml` uses bayan's YAML parser.

---

## Audits of the Cyrius port

### 2026-11-06 — P(-1) audit of 3.1.1, fixed in 3.1.2

Seven reviews by attack surface: image and tar, registry, container lifecycle, network, control
plane, documentation and performance. Every finding was reproduced before it was fixed. The fixes have
regression tests, apart from two the CHANGELOG names, and 47 of those tests were checked by putting
the bug back and watching them fail.

**Fixed:**

| Surface | Finding | Severity |
|---|---|---|
| Archive | A crafted `load` overwrote host files through a symlink under a read-only directory, and chmod'd them through a directory entry | High |
| Archive | A docker-archive member could be a symlink to a host file, whose bytes went into a world-readable blob | High |
| `cp` | Copying in or out followed links the container planted, reading and writing host files, and a planted FIFO hung it | High |
| `rm` | A container named `../../x` deleted outside the store, and `rm <name>` never removed the rootfs | High |
| Registry | Credentials went to any host, a token realm could point anywhere, and redirects carried them | High |
| Registry | A silent server hung `pull` for ever, and size-0 layers had no bound | Medium |
| Lifecycle | An fd above 1024 reached the payload, which a directory fd turns into a chroot escape | High |
| Lifecycle | `pause` froze the operator's session, and `update` limited it | Medium |
| Lifecycle | `state.json` lost updates between processes, a corrupt one wiped every record, and it was world-readable with `env` inside | Medium |
| Rootless | pasta's defaults exposed host loopback and every bound port in both directions | High |
| Rootless | A stale pid in `rootless-net.pid` was signalled unchecked | Medium |
| Network | The nftables ruleset went to a fixed `/tmp` name, which a symlink redirected | Medium |
| MCP | `stiva_build` read any host directory, and `stiva_pull` claimed to be read-only | High |
| Cron | One bad byte erased the schedule on the next save, and a clock step or a second timer fired a job twice | Medium |
| Convert | A Dockerfile value could break out of its TOML string, and compose restrictions vanished silently | Medium |
| Audit log | `$USER` was believed, and an invalid UTF-8 name hid the entry from the reader | Medium |
| CLI | Control bytes in an image name or a process title reached the terminal | Low |

**Found and not fixed here**, because they need another repo or are feature work: the default
foreground path has no seccomp or NO_NEW_PRIVS, Landlock is not applied, and containers run as root.
All three are in SECURITY.md and on the roadmap.
