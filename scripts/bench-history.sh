#!/usr/bin/env bash
set -euo pipefail

# Run the Cyrius benchmarks, append results to CSV history, and generate
# benchmarks.md with a 3-point trend table (baseline → previous → current).
#
# Usage:
#   ./scripts/bench-history.sh              # defaults to bench-history.csv
#   ./scripts/bench-history.sh results.csv  # custom output file
#
# Reads `cyrius bench` output, whose per-benchmark line looks like:
#     oci_config_build+serialize: 7.233us avg (min=7.233us max=7.233us) [100000 iters]
# (The pre-port version of this script parsed criterion's `time: [lo mid hi]`
# format and ran `cargo bench`, so it silently recorded nothing.)

HISTORY_FILE="${1:-bench-history.csv}"
BENCHMARKS_MD="benchmarks.md"
BENCH_FILE="tests/stiva.bcyr"
TIMESTAMP=$(date -u +"%Y-%m-%dT%H:%M:%SZ")
COMMIT=$(git rev-parse --short HEAD 2>/dev/null || echo "unknown")
BRANCH=$(git branch --show-current 2>/dev/null || echo "unknown")

# Create header if file doesn't exist
if [ ! -f "$HISTORY_FILE" ]; then
    echo "timestamp,commit,branch,benchmark,estimate_ns" > "$HISTORY_FILE"
fi

echo "╔══════════════════════════════════════════╗"
echo "║         stiva benchmark suite            ║"
echo "╠══════════════════════════════════════════╣"
printf "║  commit: %-31s ║\n" "$COMMIT"
printf "║  branch: %-31s ║\n" "$BRANCH"
printf "║  date:   %-31s ║\n" "$TIMESTAMP"
echo "╚══════════════════════════════════════════╝"
echo ""

# Run benchmarks and capture output, stripping ANSI escape codes.
BENCH_OUTPUT=$(cyrius bench "$BENCH_FILE" 2>&1 | sed 's/\x1b\[[0-9;]*m//g')

# Show the benchmark lines (compiler warnings are noise here).
echo "$BENCH_OUTPUT" | grep -E '^[[:space:]]+[^[:space:]]+: [0-9.]+(ns|us|µs|ms|s) avg' || true
echo ""

# Collect results for CSV.
COUNT=0
while IFS= read -r line; do
    # Match: "  <name>: <value><unit> avg (min=... max=...) [N iters]"
    if [[ "$line" =~ ^[[:space:]]+([^[:space:]:]+):[[:space:]]+([0-9.]+)(ns|us|µs|ms|s)[[:space:]]+avg ]]; then
        BENCH_NAME="${BASH_REMATCH[1]}"
        VALUE="${BASH_REMATCH[2]}"
        UNIT="${BASH_REMATCH[3]}"

        # Normalize to nanoseconds.
        case "$UNIT" in
            ns)     NS=$(awk -v v="$VALUE" 'BEGIN{printf "%.4f", v}') ;;
            µs|us)  NS=$(awk -v v="$VALUE" 'BEGIN{printf "%.4f", v * 1000}') ;;
            ms)     NS=$(awk -v v="$VALUE" 'BEGIN{printf "%.4f", v * 1000000}') ;;
            s)      NS=$(awk -v v="$VALUE" 'BEGIN{printf "%.4f", v * 1000000000}') ;;
            *)      NS="$VALUE" ;;
        esac

        echo "${TIMESTAMP},${COMMIT},${BRANCH},${BENCH_NAME},${NS}" >> "$HISTORY_FILE"
        COUNT=$((COUNT + 1))
    fi
done <<< "$BENCH_OUTPUT"

if [ "$COUNT" -eq 0 ]; then
    echo "ERROR: no benchmark results parsed from '$BENCH_FILE'." >&2
    echo "       Full output follows:" >&2
    echo "$BENCH_OUTPUT" >&2
    exit 1
fi

# Generate benchmarks.md with 3-point trend using python
python3 - "$HISTORY_FILE" "$BENCHMARKS_MD" <<'PYEOF'
import csv, sys
from collections import OrderedDict

history_file = sys.argv[1]
md_file = sys.argv[2]

rows = list(csv.DictReader(open(history_file)))
if not rows:
    sys.exit(0)

# Get unique timestamps (runs) in order
timestamps = list(OrderedDict.fromkeys(r["timestamp"] for r in rows))

# Pick up to 3: first, middle-ish, last
if len(timestamps) >= 3:
    pick = [timestamps[0], timestamps[len(timestamps)//2], timestamps[-1]]
elif len(timestamps) == 2:
    pick = [timestamps[0], timestamps[-1]]
else:
    pick = [timestamps[0]]

# Deduplicate while preserving order
seen = set()
pick = [t for t in pick if not (t in seen or seen.add(t))]

# Build data: {benchmark: {timestamp: ns}}
data = {}
commits = {}
for r in rows:
    ts = r["timestamp"]
    if ts in pick:
        data.setdefault(r["benchmark"], {})[ts] = float(r["estimate_ns"])
        commits[ts] = r["commit"]

# Labels for columns
labels = []
for i, ts in enumerate(pick):
    if i == 0 and len(pick) > 1:
        labels.append(f"Baseline (`{commits[ts]}`)")
    elif i == len(pick) - 1:
        labels.append(f"Current (`{commits[ts]}`)")
    else:
        labels.append(f"Mid (`{commits[ts]}`)")

def fmt_ns(ns):
    # Unit ladder. The pre-port version only stepped up at 1e6 ns and called it
    # µs, so a 7 µs result rendered as "7203.0 ns" and a 1 ms one as "1000 µs".
    if ns >= 1_000_000_000:
        return f"{ns/1_000_000_000:.3f} s"
    elif ns >= 1_000_000:
        return f"{ns/1_000_000:.3f} ms"
    elif ns >= 1_000:
        return f"{ns/1_000:.3f} µs"
    elif ns >= 100:
        return f"{ns:.1f} ns"
    else:
        return f"{ns:.2f} ns"

def delta(old, new):
    if old == 0:
        return ""
    pct = ((new - old) / old) * 100
    if pct < -3:
        return f" **{pct:+.0f}%**"
    elif pct > 3:
        return f" {pct:+.0f}%"
    return ""

with open(md_file, "w") as f:
    f.write("# Benchmarks\n\n")
    ts_last = pick[-1]
    f.write(f"Latest: **{ts_last}** — commit `{commits[ts_last]}`\n\n")
    if len(pick) >= 3:
        f.write(f"Tracking: `{commits[pick[0]]}` (baseline) → `{commits[pick[1]]}` (mid) → `{commits[pick[-1]]}` (current)\n\n")

    # Group by prefix
    groups = OrderedDict()
    for bench in data:
        group = bench.split("/")[0] if "/" in bench else "stiva"
        groups.setdefault(group, []).append(bench)

    for group, benches in groups.items():
        f.write(f"## {group}\n\n")
        # Header
        cols = " | ".join(labels)
        f.write(f"| Benchmark | {cols} |\n")
        f.write(f"|-----------|{'|'.join(['------'] * len(labels))}|\n")

        for bench in benches:
            name = bench.split("/")[1] if "/" in bench else bench
            vals = data[bench]
            cells = []
            for ts in pick:
                ns = vals.get(ts)
                if ns is None:
                    cells.append("—")
                else:
                    cell = fmt_ns(ns)
                    # Add delta vs baseline if not the first column
                    if ts != pick[0] and pick[0] in vals:
                        cell += delta(vals[pick[0]], ns)
                    cells.append(cell)
            f.write(f"| `{name}` | {' | '.join(cells)} |\n")
        f.write("\n")

    f.write("---\n\n")
    f.write("Generated by `./scripts/bench-history.sh` (`cyrius bench tests/stiva.bcyr`). History in `bench-history.csv`.\n")

print(f"  Generated {md_file} with {len(pick)}-point trend across {len(data)} benchmarks")
PYEOF

echo "════════════════════════════════════════════"
echo "  ${COUNT} benchmarks recorded"
echo "  CSV:      ${HISTORY_FILE}"
echo "  Markdown: ${BENCHMARKS_MD}"
echo "════════════════════════════════════════════"
