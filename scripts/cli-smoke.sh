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
# Since v3.0.14 containers run INSIDE their rootfs, so a payload must exist in
# the IMAGE — a host path like /bin/sleep is no longer reachable. Build static
# binaries into the fixture; skip the run-dependent groups if there is no
# compiler rather than pretending to test them.
mkdir -p "$WORK/rootfs/bin"
echo 'hello-from-the-image' > "$WORK/rootfs/greeting.txt"
HAVE_PAYLOAD=0
if command -v cc >/dev/null 2>&1; then
    # ONE binary only: two static binaries push the layer past ~1 MB, which
    # currently fails to unpack (a pre-existing defect, tracked separately).
    # `sleeper` doubles as the short-run payload — invoked with an argument it
    # exits immediately, so it serves both groups.
    printf '#include <unistd.h>\nint main(int argc,char**argv){if(argc<2)sleep(30);return 0;}\n' > "$WORK/s.c"
    if cc -static -o "$WORK/rootfs/bin/sleeper" "$WORK/s.c" 2>/dev/null; then
        HAVE_PAYLOAD=1
    fi
fi
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
if [ "$HAVE_PAYLOAD" -eq 1 ]; then
    run run --name c1 local/demo:v1 "/bin/sleeper now" >/dev/null 2>&1
else
    run run --name c1 local/demo:v1 /nonexistent >/dev/null 2>&1
fi
out="$(run ps --all)"
assert_contains "ps --all shows the container" "c1" "$out"
assert_contains "ps --all shows its image" "local/demo:v1" "$out"

group "rename"
out="$(run rename c1 c2)"
assert_contains "rename echoes the new name" "c2" "$out"
# Assert on the NAME column only. Matching the whole `ps` table for "c1" is
# flaky: the row carries a 32-hex container ID, and "c1" is a valid hex bigram
# — measured ~11% false failures per run.
names="$(run ps --all | awk -F'\t' 'NR>1 {print $4}')"
assert_contains "ps reflects the rename" "c2" "$names"
assert_absent  "the old name is gone" "c1" "$names"
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

group "cp guards"
# A ':' is legal in a Linux filename, so an unambiguous host path must never be
# read as <container>:<path>.
out="$(run cp /tmp/nonexistent-a:b/f.txt "$WORK/out.txt")"
assert_absent "an absolute host path with ':' is not split" "container not found" "$out"
out="$(run cp c2:../../../../../../../tmp/ESCAPED "$WORK/e.txt")"
assert_contains "cp refuses a container path with .." "escapes the container root" "$out"
out="$(run cp "$WORK/in0.txt" "c2:")"
assert_contains "cp refuses an empty container path" "empty path" "$out"

group "cp"
echo 'from-the-host' > "$WORK/in.txt"
echo x > "$WORK/in0.txt"
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

group "logs --follow"
# -f must TERMINATE once the container is no longer running, the way
# `docker logs -f` does; a follow that hangs on a stopped container would wedge
# any script that uses it. The timeout is the assertion.
timeout 15 "$STIVA" logs c2 --follow >/dev/null 2>&1; rc=$?
assert_exit "logs --follow terminates on a stopped container" 0 "$rc"
out="$(run logs c2 --follow --scan)"
assert_contains "--follow refuses --scan" "cannot be combined" "$out"
out="$(run logs no-such-container --follow)"
assert_contains "--follow on an unknown container reports it" "not found" "$out"

group "events"
# By now the script has run a container and renamed it, so the log under
# $STIVA_ROOT holds at least a created + started pair.
out="$(run events)"
assert_contains "events replays the lifecycle log" '"event":"created"' "$out"
assert_contains "events records the start" '"event":"started"' "$out"
# One JSON object per line, verbatim — `stiva events | jq` is the point of the
# verb, so no "[HH:MM:SS] " prefix may creep in front of the payload.
first="$(run events | head -1)"
case "$first" in
    '{'*'}') ok "each line is a bare JSON object" ;;
    *)       bad "each line is a bare JSON object" "got: $first" ;;
esac
assert_contains "events stamps a wall-clock ts" '"ts":1' "$first"

n="$("$STIVA" events -n 1 2>/dev/null | wc -l)"
assert_exit "--count bounds the output" 1 "$n"
n="$("$STIVA" events --since "$(( $(date +%s) + 3600 ))" 2>/dev/null | wc -l)"
assert_exit "--since in the future matches nothing" 0 "$n"
n="$("$STIVA" events --until "$(( $(date +%s) - 3600 ))" 2>/dev/null | wc -l)"
assert_exit "--until in the past matches nothing" 0 "$n"
out="$(run events --since 200 --until 100)"
assert_contains "an inverted window is refused, not silently empty" "window is empty" "$out"
"$STIVA" events --since 200 --until 100 >/dev/null 2>&1; rc=$?
assert_exit "  and exits USAGE" 2 "$rc"
out="$(run events -n -1)"
assert_contains "a negative bound is refused" "must not be negative" "$out"

# The plain form must always terminate, and each -f terminator must actually
# terminate: an `events` that hangs is exactly the failure this verb was
# blocked on. The timeouts are the assertions.
timeout 15 "$STIVA" events >/dev/null 2>&1; rc=$?
assert_exit "events without -f terminates" 0 "$rc"
timeout 15 "$STIVA" events -f -n 1 >/dev/null 2>&1; rc=$?
assert_exit "--follow terminates on --count" 0 "$rc"
timeout 15 "$STIVA" events -f --until "$(( $(date +%s) - 3600 ))" >/dev/null 2>&1; rc=$?
assert_exit "--follow terminates on a past --until" 0 "$rc"

# A follower in one process must see events published by another — the whole
# reason this is a file and not the in-process pubsub bus.
FOLLOW_ROOT="$(mktemp -d)"
( STIVA_ROOT="$FOLLOW_ROOT" timeout 20 "$STIVA" events -f -n 2 >"$WORK/follow.out" 2>/dev/null ) &
follow_pid=$!
sleep 1
STIVA_ROOT="$FOLLOW_ROOT" "$STIVA" import "$WORK/rf.tar" followed v1 >/dev/null 2>&1
STIVA_ROOT="$FOLLOW_ROOT" "$STIVA" run local/followed:v1 /bin/true >/dev/null 2>&1
wait "$follow_pid"; rc=$?
assert_exit "a cross-process follow terminates on its count" 0 "$rc"
assert_contains "and saw the other process's events" '"event":"created"' "$(cat "$WORK/follow.out")"
rm -rf "$FOLLOW_ROOT"

# An empty root reports itself on stderr and leaves stdout clean, so a
# `stiva events > file` on a fresh host produces an empty file, not a message.
EMPTY_ROOT="$(mktemp -d)"
out="$(STIVA_ROOT="$EMPTY_ROOT" "$STIVA" events 2>&1 >/dev/null)"
assert_contains "an empty root explains itself on stderr" "no lifecycle events" "$out"
out="$(STIVA_ROOT="$EMPTY_ROOT" "$STIVA" events 2>/dev/null)"
if [ -z "$out" ]; then ok "and writes nothing to stdout"
else bad "and writes nothing to stdout" "got: $out"; fi
rm -rf "$EMPTY_ROOT"

# STIVA_EVENTS=off must actually stop the writes.
OFF_ROOT="$(mktemp -d)"
STIVA_ROOT="$OFF_ROOT" "$STIVA" import "$WORK/rf.tar" quiet v1 >/dev/null 2>&1
STIVA_ROOT="$OFF_ROOT" STIVA_EVENTS=off "$STIVA" run local/quiet:v1 /bin/true >/dev/null 2>&1
if [ -f "$OFF_ROOT/events.jsonl" ]; then
    bad "STIVA_EVENTS=off writes no event log" "events.jsonl exists"
else
    ok "STIVA_EVENTS=off writes no event log"
fi
rm -rf "$OFF_ROOT"

group "run -d (detached)"
if [ "$HAVE_PAYLOAD" -eq 0 ]; then
    printf '  skip (no static compiler for an in-image payload)\n'
else
# The whole point of detach: the call returns while the process keeps running,
# and the state survives into a DIFFERENT stiva process. /bin/sleep is a host
# binary on purpose — see the rootfs caveat in docs/cli.md.
out="$(run run -d --name detached1 local/demo:v1 /bin/sleeper)"
DID="$(printf '%s' "$out" | tail -1)"
case "$DID" in
    [0-9a-f]*) ok "run -d returns a container id immediately" ;;
    *)         bad "run -d returns a container id immediately" "got: $out" ;;
esac
sleep 1
names="$(run ps | awk -F'\t' 'NR>1 {print $4}')"
assert_contains "a NEW process still sees it Running" "detached1" "$names"
DPID="$(python3 -c "
import json,sys
d=json.load(open('$ROOT/state.json'))
cs = d if isinstance(d, list) else d.get('containers', [])
print(next((c['pid'] for c in cs if c.get('name')=='detached1'), 0))" 2>/dev/null || echo 0)"
if [ "${DPID:-0}" -gt 0 ] && kill -0 "$DPID" 2>/dev/null; then
    ok "the detached process is genuinely alive"
else
    bad "the detached process is genuinely alive" "pid=$DPID"
fi
# stop from a different process has no in-memory handle — it must fall back to
# signalling the recorded pid, or the record says Stopped while the process runs.
run stop detached1 >/dev/null 2>&1
sleep 1
if [ "${DPID:-0}" -gt 0 ] && kill -0 "$DPID" 2>/dev/null; then
    bad "cross-process stop actually kills the process" "pid $DPID still alive"
else
    ok "cross-process stop actually kills the process"
fi
names="$(run ps | awk -F'\t' 'NR>1 {print $4}')"
assert_absent "and ps no longer reports it Running" "detached1" "$names"
fi

group "deferred verbs still say so"
out="$(run exec c2 /bin/true)"
assert_contains "exec reports not-yet-wired" "not yet wired" "$out"

printf '\n────────────────────────────\n'
printf '  %d passed, %d failed\n' "$PASS" "$FAIL"
[ "$FAIL" -eq 0 ] || exit 1
