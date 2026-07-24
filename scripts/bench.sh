#!/usr/bin/env bash
set -euo pipefail

# Toolchain-timing runner that appends results to a history log.
#
# This is the *coarse* companion to ./scripts/bench-history.sh: that script
# records per-benchmark nanoseconds from `cyrius bench`, this one records
# suite-level wall-clock (test time, build time) plus size metrics, so the
# cost of the toolchain itself is tracked alongside the code's hot paths.
#
# Usage:
#   ./scripts/bench.sh              # Run and append to history
#   ./scripts/bench.sh --history    # Show history
#   ./scripts/bench.sh --clean      # Remove history file
#
# The history file is stored at benches/history.log with timestamped entries.

HISTORY_DIR="$(cd "$(dirname "$0")/.." && pwd)/benches"
HISTORY_FILE="${HISTORY_DIR}/history.log"

mkdir -p "$HISTORY_DIR"

case "${1:-}" in
    --history)
        if [[ -f "$HISTORY_FILE" ]]; then
            cat "$HISTORY_FILE"
        else
            echo "No benchmark history yet. Run: $0"
        fi
        exit 0
        ;;
    --clean)
        rm -f "$HISTORY_FILE"
        echo "Benchmark history cleared."
        exit 0
        ;;
esac

# Gather metadata.
VERSION=$(cat VERSION 2>/dev/null || echo "unknown")
GIT_SHA=$(git rev-parse --short HEAD 2>/dev/null || echo "unknown")
GIT_BRANCH=$(git branch --show-current 2>/dev/null || echo "unknown")
TIMESTAMP=$(date -u +"%Y-%m-%dT%H:%M:%SZ")
CYRIUS_VERSION=$(cyrius --version 2>/dev/null | head -1 || echo "unknown")

echo "=== Stiva Benchmark Run ==="
echo "Version:  ${VERSION}"
echo "Commit:   ${GIT_SHA} (${GIT_BRANCH})"
echo "Date:     ${TIMESTAMP}"
echo "Cyrius:   ${CYRIUS_VERSION}"
echo ""

# Run the full test suite (timing + assertion count).
echo "--- Test suite timing ---"
TEST_START=$(date +%s%N)
TEST_OUTPUT=$(cyrius tests tests/ 2>&1 || true)
TEST_END=$(date +%s%N)
TEST_MS=$(( (TEST_END - TEST_START) / 1000000 ))
echo "$TEST_OUTPUT" | grep -E '^[0-9]+ passed, [0-9]+ failed' || true
echo "Test suite: ${TEST_MS}ms"
echo ""

# Sum the per-file "<N> passed" totals; the trailing "4 passed, 0 failed" line
# (a file count, no "(N total)" suffix) is excluded by requiring that suffix.
TEST_COUNT=$(echo "$TEST_OUTPUT" \
    | grep -oE '^[0-9]+ passed, [0-9]+ failed \([0-9]+ total\)' \
    | grep -oE '^[0-9]+' \
    | awk '{s+=$1} END {print s+0}')
FAIL_COUNT=$(echo "$TEST_OUTPUT" \
    | grep -oE '^[0-9]+ passed, [0-9]+ failed \([0-9]+ total\)' \
    | grep -oE ', [0-9]+ failed' | grep -oE '[0-9]+' \
    | awk '{s+=$1} END {print s+0}')

# A unit that fails to COMPILE reports no "N passed" line at all, so the sum
# above silently shrinks and the failure count stays 0 — a broken tree would
# be recorded as a clean run with fewer tests, corrupting the one number this
# file exists to track. Refuse to record instead.
if [ "${TEST_COUNT:-0}" -eq 0 ]; then
    echo "ERROR: no test results parsed — the suite did not compile or did not run." >&2
    echo "$TEST_OUTPUT" | tail -20 >&2
    exit 1
fi
EXPECTED_UNITS=$(ls tests/*.tcyr 2>/dev/null | wc -l)
REPORTED_UNITS=$(echo "$TEST_OUTPUT" | grep -cE '^[0-9]+ passed, [0-9]+ failed \([0-9]+ total\)')
if [ "$REPORTED_UNITS" -ne "$EXPECTED_UNITS" ]; then
    echo "ERROR: ${REPORTED_UNITS}/${EXPECTED_UNITS} test units reported results — one failed to compile." >&2
    echo "$TEST_OUTPUT" | grep -iE 'error|identifier buffer full|fn_table' | head -20 >&2
    exit 1
fi

# Build timing (clean build — remove the artifact first).
echo "--- Build timing ---"
rm -f build/stiva
BUILD_START=$(date +%s%N)
if ! cyrius build src/main.cyr build/stiva >/dev/null 2>&1; then
    echo "ERROR: build failed — refusing to record a timing for a broken build." >&2
    exit 1
fi
BUILD_END=$(date +%s%N)
BUILD_MS=$(( (BUILD_END - BUILD_START) / 1000000 ))
BIN_BYTES=$(stat -c%s build/stiva 2>/dev/null || echo 0)
echo "Build: ${BUILD_MS}ms (${BIN_BYTES} bytes)"
echo ""

# Count lines of Cyrius source.
LOC=$(find src/ -name '*.cyr' -exec cat {} + | wc -l)

# Append to history.
{
    echo "---"
    echo "timestamp: ${TIMESTAMP}"
    echo "version: ${VERSION}"
    echo "commit: ${GIT_SHA}"
    echo "branch: ${GIT_BRANCH}"
    echo "cyrius: ${CYRIUS_VERSION}"
    echo "tests: ${TEST_COUNT:-0}"
    echo "failures: ${FAIL_COUNT:-0}"
    echo "test_ms: ${TEST_MS}"
    echo "build_ms: ${BUILD_MS}"
    echo "bin_bytes: ${BIN_BYTES}"
    echo "loc: ${LOC}"
    echo ""
} >> "$HISTORY_FILE"

echo "=== Summary ==="
echo "Tests:     ${TEST_COUNT:-0} passed, ${FAIL_COUNT:-0} failed"
echo "Test time: ${TEST_MS}ms"
echo "Build:     ${BUILD_MS}ms (${BIN_BYTES} bytes)"
echo "LoC:       ${LOC}"
echo ""
echo "Results appended to ${HISTORY_FILE}"
