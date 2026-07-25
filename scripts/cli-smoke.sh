#!/usr/bin/env bash
# stiva CLI smoke tests — the only coverage src/main.cyr can have.
#
# main.cyr ends in `var exit_code = main(); syscall(60, exit_code);`, so it
# cannot be included in a .tcyr unit: any test file that included it would run
# the CLI and exit before reaching its own assertions. Every other module is
# covered by `cyrius tests tests/`; the CLI handlers are covered here or not at
# all. That gap is not theoretical — the v3.0.7 §C work shipped `logs` and
# `get_rootfs` resolving the container NAME where they needed the ID, and only a
# binary smoke test caught it.
#
# Usage:  ./scripts/cli-smoke.sh [path/to/stiva]
# Exits non-zero on the first failure, so it is CI-usable.

set -u

STIVA="${1:-build/stiva}"
if [ ! -x "$STIVA" ]; then
    echo "error: $STIVA is not executable — build it first:" >&2
    echo "  cyrius build src/main.cyr build/stiva" >&2
    exit 2
fi
STIVA="$(cd "$(dirname "$STIVA")" && pwd)/$(basename "$STIVA")"

ROOT="$(mktemp -d)"
WORK="$(mktemp -d)"
export STIVA_ROOT="$ROOT"
trap 'rm -rf "$ROOT" "$WORK"' EXIT

PASS=0
FAIL=0

ok ()   { PASS=$((PASS + 1)); printf '  ok   %s\n' "$1"; }
bad ()  { FAIL=$((FAIL + 1)); printf '  FAIL %s\n' "$1"; printf '       %s\n' "$2"; }

# assert_contains <label> <needle> <haystack>
assert_contains () {
    case "$3" in
        *"$2"*) ok "$1" ;;
        *)      bad "$1" "expected to contain '$2', got: $(printf '%s' "$3" | head -3 | tr '\n' '|')" ;;
    esac
}

# assert_absent <label> <needle> <haystack>
assert_absent () {
    case "$3" in
        *"$2"*) bad "$1" "expected NOT to contain '$2'" ;;
        *)      ok "$1" ;;
    esac
}

# assert_exit <label> <expected-code> <actual-code>
assert_exit () {
    if [ "$2" = "$3" ]; then ok "$1"; else bad "$1" "expected exit $2, got $3"; fi
}

# Run stiva with stderr folded in; INFO log lines are dropped so assertions see
# only the command's own output.
run () { "$STIVA" "$@" 2>&1 | grep -vE '^\[[0-9]+\] \[(INFO|DEBUG|WARN)\]'; }

group () { printf '\n=== %s ===\n' "$1"; }

# ── fixture: a tiny image ────────────────────────────────
mkdir -p "$WORK/rootfs/bin"
echo 'hello-from-the-image' > "$WORK/rootfs/greeting.txt"
tar -cf "$WORK/rf.tar" -C "$WORK/rootfs" .

group "version + help"
out="$(run --version)"
assert_contains "--version reports a semver" "OCI container runtime" "$out"
out="$(run --help)"
assert_contains "--help lists commands" "pull" "$out"

group "import + images"
out="$(run import "$WORK/rf.tar" demo v1)"
assert_contains "import prints the image id" "sha256:" "$out"
IMG_ID="$(printf '%s' "$out" | tail -1)"
out="$(run images)"
assert_contains "images lists the imported ref" "local/demo:v1" "$out"

group "completions (cmdit-driven)"
out="$(run completions bash)"
assert_contains "bash script has the complete builtin" "complete -F" "$out"
assert_contains "bash script carries the verb list" "local verbs=" "$out"
# Driven by cmdit's verb table, so every registered verb must appear — this is
# what a hand-maintained list would silently drift from.
for v in pull push run ps stop rm images tag import export save load convert; do
    assert_contains "  completions mention '$v'" "$v" "$out"
done
if command -v bash >/dev/null 2>&1; then
    if printf '%s' "$out" | bash -n - 2>/dev/null; then
        ok "bash accepts the generated script (bash -n)"
    else
        bad "bash accepts the generated script (bash -n)" "syntax error"
    fi
fi
if command -v zsh >/dev/null 2>&1; then
    if run completions zsh | zsh -n - 2>/dev/null; then
        ok "zsh accepts the generated script (zsh -n)"
    else
        bad "zsh accepts the generated script (zsh -n)" "syntax error"
    fi
fi
out="$(run completions fish)"
assert_contains "fish script uses __fish_use_subcommand" "__fish_use_subcommand" "$out"
"$STIVA" completions elvish >/dev/null 2>&1; rc=$?
assert_exit "unsupported shell exits USAGE" 2 "$rc"
# Nothing may reach stdout for an unsupported shell — a half-written script
# redirected into a completions file would be worse than none.
out="$("$STIVA" completions elvish 2>/dev/null)"
if [ -z "$out" ]; then ok "unsupported shell writes nothing to stdout"
else bad "unsupported shell writes nothing to stdout" "got: $out"; fi

group "run + ps"
run run --name c1 local/demo:v1 /bin/true >/dev/null 2>&1
out="$(run ps --all)"
assert_contains "ps --all shows the container" "c1" "$out"
assert_contains "ps --all shows its image" "local/demo:v1" "$out"

group "rename"
out="$(run rename c1 c2)"
assert_contains "rename echoes the new name" "c2" "$out"
out="$(run ps --all)"
assert_contains "ps reflects the rename" "c2" "$out"
assert_absent  "the old name is gone" "c1" "$out"
"$STIVA" rename c2 >/dev/null 2>&1; rc=$?
assert_exit "rename with one arg exits USAGE" 2 "$rc"

group "kill / top on a stopped container"
# Both must REFUSE with a message rather than failing silently — a bare
# non-zero exit with no output is the failure mode this group exists to catch.
out="$(run kill c2)"; rc=$?
assert_contains "kill on a stopped container explains itself" "not running" "$out"
out="$(run top c2)"
assert_contains "top on a stopped container explains itself" "not running" "$out"
out="$(run kill c2 -s 99)"
assert_contains "kill rejects an out-of-range signal" "out of range" "$out"
out="$(run kill)"
assert_contains "kill with no container reports it" "missing required argument" "$out"

group "cp"
echo 'from-the-host' > "$WORK/in.txt"
out="$(run cp "$WORK/in.txt" c2:/incoming.txt)"
assert_contains "cp host->container echoes the destination" "incoming.txt" "$out"
out="$(run cp c2:/incoming.txt "$WORK/back.txt")"
assert_contains "cp container->host echoes the destination" "back.txt" "$out"
if [ -f "$WORK/back.txt" ] && [ "$(cat "$WORK/back.txt")" = "from-the-host" ]; then
    ok "cp round-trips the file contents"
else
    bad "cp round-trips the file contents" "got: $(cat "$WORK/back.txt" 2>/dev/null)"
fi
out="$(run cp c2:/a c2:/b)"
assert_contains "cp refuses two container sides" "only one side" "$out"
out="$(run cp /tmp/a /tmp/b)"
assert_contains "cp refuses zero container sides" "must be" "$out"

group "deferred verbs still say so"
out="$(run exec c2 /bin/true)"
assert_contains "exec reports not-yet-wired" "not yet wired" "$out"

printf '\n────────────────────────────\n'
printf '  %d passed, %d failed\n' "$PASS" "$FAIL"
[ "$FAIL" -eq 0 ] || exit 1
