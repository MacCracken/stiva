# Development Scripts

Stiva ships utility scripts in `scripts/` for version management and performance tracking.

## `scripts/version-bump.sh`

Writes the project version to the `VERSION` file, which is the single source of
truth — `cyrius.cyml` reads it via `version = "${file:VERSION}"`, so nothing else
needs editing.

```bash
./scripts/version-bump.sh 3.0.1
```

**What it does:**
1. Writes the new version to `VERSION` (picked up by `cyrius.cyml` automatically).

**When to use:** Before tagging a release. Also update the recipe version in
[zugot](https://github.com/MacCracken/zugot) to match.

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
- **Test suite time** — `cyrius tests tests/` wall-clock duration (ms)
- **Build time** — `cyrius build src/main.cyr build/stiva` wall-clock duration (ms)
- **Test count** — number of passing tests
- **Lines of code** — total `.cyr` lines in `src/`

**History file:** `benches/history.log`

Each entry records:
```yaml
---
timestamp: 2026-07-26T06:00:00Z
version: 3.0.17
commit: abc1234
branch: main
cyrius: cyrius 6.5.33
tests: 2175
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

Runs the `cyrius bench tests/stiva.bcyr` benchmarks, appends results to a CSV history, and generates a `benchmarks.md` trend report (matching hisab's pattern).

```bash
# Run benchmarks and generate report
./scripts/bench-history.sh

# Custom CSV file
./scripts/bench-history.sh results.csv
```

**What it measures:**
- All benchmark groups (imageref, volume, port, blob, ippool, fleet, build)
- Median time per benchmark, normalized to nanoseconds

**Output files:**
- `bench-history.csv` — timestamped CSV with all benchmark results
- `benchmarks.md` — 3-point trend table (baseline → mid → current)

**Makefile integration:**
```bash
make bench           # Runs cyrius bench tests/stiva.bcyr
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
