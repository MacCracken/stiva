# Development Scripts

Stiva ships utility scripts in `scripts/` for version management and performance tracking.

## `scripts/version-bump.sh`

Bumps the crate version across `VERSION` and `Cargo.toml`, then updates `Cargo.lock`.

```bash
./scripts/version-bump.sh 0.23.3
```

**What it does:**
1. Writes the new version to `VERSION`
2. Updates the `version = "..."` line in `Cargo.toml`
3. Runs `cargo check --quiet` to regenerate `Cargo.lock`

**When to use:** Before tagging a release. Run this, verify with `cargo check`, then commit.

---

## `scripts/bench.sh`

Benchmark runner that measures test suite timing and release build timing, appending results to a persistent history log.

```bash
# Run benchmarks and append to history
./scripts/bench.sh

# View benchmark history
./scripts/bench.sh --history

# Clear history
./scripts/bench.sh --clean
```

**What it measures:**
- **Test suite time** — `cargo test --all-features` wall-clock duration (ms)
- **Release build time** — `cargo build --release` wall-clock duration (ms)
- **Test count** — number of passing tests
- **Lines of code** — total `.rs` lines in `src/`

**History file:** `benches/history.log`

Each entry records:
```yaml
---
timestamp: 2026-03-22T06:00:00Z
version: 0.22.3
commit: abc1234
branch: main
rustc: rustc 1.89.0
tests: 290
test_ms: 150
build_ms: 12000
loc: 4500
```

**When to use:**
- Before and after significant changes to track performance impact
- Before releases to establish baseline
- In CI via `make bench-history`

**Makefile integration:**
```bash
make bench-history   # Runs scripts/bench.sh
```

---

## `scripts/bench-history.sh`

Runs criterion benchmarks, appends results to a CSV history, and generates a `benchmarks.md` trend report (matching hisab's pattern).

```bash
# Run benchmarks and generate report
./scripts/bench-history.sh

# Custom CSV file
./scripts/bench-history.sh results.csv
```

**What it measures:**
- All criterion benchmark groups (imageref, volume, port, blob, ippool, fleet, build)
- Median time per benchmark, normalized to nanoseconds

**Output files:**
- `bench-history.csv` — timestamped CSV with all benchmark results
- `benchmarks.md` — 3-point trend table (baseline → mid → current)

**Makefile integration:**
```bash
make bench           # Runs criterion benchmarks
make bench-history   # Runs scripts/bench-history.sh
```

---

## Adding New Scripts

Scripts should:
1. Live in `scripts/`
2. Be executable (`chmod +x`)
3. Start with `#!/usr/bin/env bash` and `set -euo pipefail`
4. Be documented in this file
5. Have a corresponding Makefile target if used frequently
