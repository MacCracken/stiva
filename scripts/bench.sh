#!/usr/bin/env bash
set -euo pipefail

# Benchmark runner that appends results to a history log.
#
# Usage:
#   ./scripts/bench.sh              # Run benchmarks and append to history
#   ./scripts/bench.sh --history    # Show benchmark history
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
RUSTC_VERSION=$(rustc --version 2>/dev/null || echo "unknown")

echo "=== Stiva Benchmark Run ==="
echo "Version:  ${VERSION}"
echo "Commit:   ${GIT_SHA} (${GIT_BRANCH})"
echo "Date:     ${TIMESTAMP}"
echo "Rustc:    ${RUSTC_VERSION}"
echo ""

# Run cargo test (as benchmark proxy — compile time + test time).
echo "--- Test suite timing ---"
TEST_START=$(date +%s%N)
cargo test --all-features --quiet 2>&1 | tail -5
TEST_END=$(date +%s%N)
TEST_MS=$(( (TEST_END - TEST_START) / 1000000 ))
echo "Test suite: ${TEST_MS}ms"
echo ""

# Run cargo build --release for compile time benchmark.
echo "--- Release build timing ---"
cargo clean --release -q 2>/dev/null || true
BUILD_START=$(date +%s%N)
cargo build --release --quiet 2>&1
BUILD_END=$(date +%s%N)
BUILD_MS=$(( (BUILD_END - BUILD_START) / 1000000 ))
echo "Release build: ${BUILD_MS}ms"
echo ""

# Count tests.
TEST_COUNT=$(cargo test --all-features 2>&1 | grep "^test result" | head -1 | grep -oP '\d+ passed' | grep -oP '\d+')

# Count lines of code.
LOC=$(find src/ -name '*.rs' -exec cat {} + | wc -l)

# Append to history.
{
    echo "---"
    echo "timestamp: ${TIMESTAMP}"
    echo "version: ${VERSION}"
    echo "commit: ${GIT_SHA}"
    echo "branch: ${GIT_BRANCH}"
    echo "rustc: ${RUSTC_VERSION}"
    echo "tests: ${TEST_COUNT:-0}"
    echo "test_ms: ${TEST_MS}"
    echo "build_ms: ${BUILD_MS}"
    echo "loc: ${LOC}"
    echo ""
} >> "$HISTORY_FILE"

echo "=== Summary ==="
echo "Tests:     ${TEST_COUNT:-0}"
echo "Test time: ${TEST_MS}ms"
echo "Build:     ${BUILD_MS}ms"
echo "LoC:       ${LOC}"
echo ""
echo "Results appended to ${HISTORY_FILE}"
