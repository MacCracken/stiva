#!/usr/bin/env bash
# stiva CLI smoke tests — the only coverage src/main.cyr can have.
#
# main.cyr ends in `var exit_code = main(); syscall(60, exit_code);`, so it
# cannot be included in a .tcyr unit: any test file that included it would run
# the CLI and exit before reaching its own assertions. Every other module is
# covered by `cyrius test tests/`; the CLI handlers are covered here or not at
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
SKIP=0

ok ()   { PASS=$((PASS + 1)); printf '  ok   %s\n' "$1"; }
bad ()  { FAIL=$((FAIL + 1)); printf '  FAIL %s\n' "$1"; printf '       %s\n' "$2"; }
# A skip is counted and printed, because a quietly-skipped test reads like a pass.
skip () { SKIP=$((SKIP + 1)); printf '  skip %s\n' "$1"; }

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
    # `sleeper` doubles as the short-run payload — invoked with an argument it
    # exits immediately, so it serves both the run and detach groups.
    #
    # A SECOND binary is fine now. It was not before: two static binaries push
    # the layer past 1 MiB, and every layer over that boundary was written as
    # corrupt gzip until the batch-encoder defect was fixed (see the 3.0.14
    # CHANGELOG). Two payloads here therefore also regression-test that fix on
    # a real image.
    printf '#include <unistd.h>\nint main(int argc,char**argv){if(argc<2)sleep(30);return 0;}\n' > "$WORK/s.c"
    printf '#include <unistd.h>\nint main(void){write(1,"to-stdout\\n",10);write(2,"to-stderr\\n",10);return 3;}\n' > "$WORK/e.c"
    # `argvp` prints each argument it received between brackets, so a test reads
    # what the payload observed rather than what the spec carried.
    printf '#include <stdio.h>\nint main(int c,char**v){for(int i=1;i<c;i++)printf("[%%s]",v[i]);putchar(10);return 0;}\n' > "$WORK/a.c"
    # `secprobe` prints what the kernel says of the payload, through prctl because
    # the process backend's rootfs has no /proc. Given an argument it tries a mount,
    # which exits 1 if it merely fails: 159 can only be a filter's kill.
    printf '#include <stdio.h>\n#include <sys/mount.h>\n#include <sys/prctl.h>\nint main(int c,char**v){if(c>1)return mount("t","/","tmpfs",0,0)?1:0;printf("seccomp=%%d nnp=%%d\\n",prctl(PR_GET_SECCOMP,0,0,0,0),prctl(PR_GET_NO_NEW_PRIVS,0,0,0,0));return 0;}\n' > "$WORK/p.c"
    # `report` prints what a payload was handed: its working directory, two
    # variables and its arguments, so a test reads what reached it, not a spec.
    printf '#include <stdio.h>\n#include <stdlib.h>\n#include <unistd.h>\nstatic const char*g(const char*k){const char*v=getenv(k);return v?v:"-";}\nint main(int c,char**v){char d[256];if(!getcwd(d,256))d[0]=0;printf("cwd=%%s FOO=%%s BAR=%%s",d,g("FOO"),g("BAR"));for(int i=1;i<c;i++)printf(" [%%s]",v[i]);puts("");return 0;}\n' > "$WORK/r.c"
    if cc -static -o "$WORK/rootfs/bin/sleeper" "$WORK/s.c" 2>/dev/null \
       && cc -static -o "$WORK/rootfs/bin/errw" "$WORK/e.c" 2>/dev/null \
       && cc -static -o "$WORK/rootfs/bin/argvp" "$WORK/a.c" 2>/dev/null \
       && cc -static -o "$WORK/rootfs/bin/secprobe" "$WORK/p.c" 2>/dev/null \
       && cc -static -o "$WORK/rootfs/bin/report" "$WORK/r.c" 2>/dev/null; then
        HAVE_PAYLOAD=1
    fi
fi
tar -cf "$WORK/rf.tar" -C "$WORK/rootfs" .

# ── what this host can run ───────────────────────
# Asked of the HOST, and never of stiva. A gate of "did `stiva run` work" turns a
# broken `run` into a green skip, so these are what stiva depends on, tested with
# no stiva in the way: past them a failure is stiva's. `unshare -Urm` is the test
# because both the process backend and rootless runc need a mount namespace inside
# a user namespace, and Ubuntu's AppArmor restriction lets the unshare through and
# denies the mount, so a bare `unshare -U` would read as fine.
HAVE_USERNS=0
if unshare -Urm true 2>/dev/null; then HAVE_USERNS=1; fi
HAVE_OCI=0
for rt in /usr/local/bin/runc /usr/bin/runc /usr/local/bin/crun /usr/bin/crun; do
    if [ -x "$rt" ]; then HAVE_OCI=1; fi
done
CAN_RUN=0
WHY_NOT='no static compiler for an in-image payload'
if [ "$HAVE_PAYLOAD" -eq 1 ]; then
    if [ "$HAVE_USERNS" -eq 1 ]; then CAN_RUN=1; else WHY_NOT='this host refuses user namespaces'; fi
fi

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
else
    skip "bash completions (no bash here)"
fi
if command -v zsh >/dev/null 2>&1; then
    if run completions zsh | zsh -n - 2>/dev/null; then
        ok "zsh accepts the generated script (zsh -n)"
    else
        bad "zsh accepts the generated script (zsh -n)" "syntax error"
    fi
else
    skip "zsh completions (no zsh here)"
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
if [ "$CAN_RUN" -eq 0 ]; then
    skip "run -d ($WHY_NOT)"
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

group "build"
# A base to derive from, then a Stivafile that copies a file into it. The
# decisive check is that the built image RUNS the copied payload — an image
# whose layer never made it into the rootfs still lists fine in `images`.
mkdir -p "$WORK/bctx/payload"
printf 'from-the-build-layer\n' > "$WORK/bctx/payload/marker.txt"
cat > "$WORK/Stivafile" <<'STIVAFILE'
[image]
base = "local/smokebase:v1"
name = "smokebuilt"
tag = "v1"

[[steps]]
type = "copy"
source = "payload"
destination = "/payload"

[[steps]]
type = "env"
key = "SMOKE"
value = "yes"

[config]
workdir = "/payload"
STIVAFILE
mkdir -p "$WORK/bbase/etc"
printf 'smoke\n' > "$WORK/bbase/etc/os-release"
tar -cf "$WORK/bbase.tar" -C "$WORK/bbase" . 2>/dev/null
run import "$WORK/bbase.tar" smokebase v1 >/dev/null 2>&1

out="$(run build -f "$WORK/Stivafile" -c "$WORK/bctx")"
assert_contains "build prints the image id" "sha256:" "$out"
out="$(run images)"
assert_contains "and the image is listed" "local/smokebuilt:v1" "$out"

# The copied file must be present in a container created from the built image.
run run --name builtc local/smokebuilt:v1 /bin/true >/dev/null 2>&1
BCID="$(run ps -a | awk -F'\t' '$4=="builtc" || $2=="builtc" {print $1}' | head -1)"
if [ -z "$BCID" ]; then BCID="$(run ps -a | tail -1 | awk '{print $1}')"; fi
if [ -f "$ROOT/containers/$BCID/rootfs/payload/marker.txt" ]; then
    ok "the built layer's file is present in the container rootfs"
else
    bad "the built layer's file is present in the container rootfs" "no marker.txt under $BCID"
fi

# THE edit-rebuild-run loop. Two separate defects made this silently reuse the
# old bytes: a cache key that did not cover the source content, and an output
# ref carrying a digest, which made the digest-aware index dedup append a second
# entry under the same tag instead of moving it (so `run` kept resolving the
# original). Both are invisible in unit tests of a single build.
BUILT_ID_1="$(run build -f "$WORK/Stivafile" -c "$WORK/bctx" | tail -1)"
printf 'edited-in-place\n' > "$WORK/bctx/payload/marker.txt"
BUILT_ID_2="$(run build -f "$WORK/Stivafile" -c "$WORK/bctx" | tail -1)"
if [ -n "$BUILT_ID_1" ] && [ "$BUILT_ID_1" != "$BUILT_ID_2" ]; then
    ok "editing the context produces a different image"
else
    bad "editing the context produces a different image" "both builds gave $BUILT_ID_1"
fi
count="$(run images | grep -c 'local/smokebuilt:v1' || true)"
if [ "$count" = "1" ]; then
    ok "and the tag MOVES rather than duplicating"
else
    bad "and the tag MOVES rather than duplicating" "$count index entries for local/smokebuilt:v1"
fi
run run --name builtc2 local/smokebuilt:v1 /bin/true >/dev/null 2>&1
BCID2="$(run ps -a | tail -1 | awk '{print $1}')"
if grep -q 'edited-in-place' "$ROOT/containers/$BCID2/rootfs/payload/marker.txt" 2>/dev/null; then
    ok "and a container from the rebuilt tag sees the EDITED file"
else
    bad "and a container from the rebuilt tag sees the EDITED file" "stale or missing marker.txt"
fi

# A missing spec file must fail, not build an empty image.
out="$(run build -f "$WORK/nope.toml" -c "$WORK/bctx")"
assert_absent "a missing Stivafile does not print an image id" "sha256:" "$out"

# RUN steps are refused rather than silently emitting a marker-file layer.
cat > "$WORK/Stivafile.run" <<'STIVAFILE'
[image]
base = "local/smokebase:v1"
name = "smokerun"
tag = "v1"

[[steps]]
type = "run"
command = ["echo", "hi"]
STIVAFILE
out="$(run build -f "$WORK/Stivafile.run" -c "$WORK/bctx")"
assert_contains "a run step is refused with a reason" "not executed" "$out"
out="$(run images)"
assert_absent "and no image is indexed for it" "local/smokerun:v1" "$out"

# A repeated label or port once reached the config blob as a duplicate JSON key, which
# bayan 1.11.1 refuses whole: the build still printed an id, and only a read of the
# image failed. So the decisive check is deriving from it, which reads that blob.
cat > "$WORK/Stivafile.dup" <<'STIVAFILE'
[image]
base = "local/smokebase:v1"
name = "smokedup"
tag = "v1"

[[steps]]
type = "copy"
source = "payload"
destination = "/payload"

[[steps]]
type = "label"
key = "maintainer"
value = "first"

[[steps]]
type = "label"
key = "maintainer"
value = "second"

[config]
expose = [8080, 8080]
STIVAFILE
cat > "$WORK/Stivafile.child" <<'STIVAFILE'
[image]
base = "local/smokedup:v1"
name = "smokechild"
tag = "v1"

[[steps]]
type = "copy"
source = "payload"
destination = "/child"
STIVAFILE
out="$(run build -f "$WORK/Stivafile.dup" -c "$WORK/bctx")"
assert_contains "a repeated label and port still build" "sha256:" "$out"
out="$(run build -f "$WORK/Stivafile.child" -c "$WORK/bctx")"
assert_contains "and the image they built can be derived from" "sha256:" "$out"

group "arguments"
# Until kavach 3.14.3 stiva could only hand over one string, which was split on
# spaces and tabs: an argument could hold neither, and an empty one vanished, so
# `sh -c 'echo one; echo two'` ran `echo`. Each case reads what the payload received.
if [ "$CAN_RUN" -eq 1 ]; then
    TAB="$(printf '\t')"
    out="$(run run local/demo:v1 -- /bin/argvp "a  b" "" "c${TAB}d")"
    assert_contains "run keeps each argument whole" "[a  b][][c${TAB}d]" "$out"
    run run -d --name argd local/demo:v1 -- /bin/argvp "a  b" "" "c${TAB}d" >/dev/null 2>&1
    sleep 1
    out="$(run logs argd)"
    assert_contains "run -d keeps each argument whole" "[a  b][][c${TAB}d]" "$out"
    out="$(run run local/demo:v1 "/bin/argvp one two")"
    assert_contains "one quoted string is still a command line" "[one][two]" "$out"
else
    skip "arguments ($WHY_NOT)"
fi

group "confinement"
# What a payload reads from the kernel, on each path `run` can take. Through kavach
# 3.14.3 the OCI backend, the default where runc is installed, read seccomp=0 nnp=0
# and its mount succeeded, under every test here: they asserted the policy, and the
# spec dropped it. `exec` is left out: it has no filter yet (roadmap v3.2.2).
if [ "$CAN_RUN" -eq 1 ]; then
    out="$(run run local/demo:v1 /bin/secprobe)"
    assert_contains "the default path has both" "seccomp=2 nnp=1" "$out"
    out="$(run run --backend process local/demo:v1 /bin/secprobe)"
    assert_contains "process backend: a filter and no_new_privs" "seccomp=2 nnp=1" "$out"
    "$STIVA" run --backend process local/demo:v1 /bin/secprobe mount >/dev/null 2>&1
    assert_exit "process backend: a mount is killed" 159 "$?"
    run run -d --name confd local/demo:v1 /bin/secprobe >/dev/null 2>&1
    sleep 1
    out="$(run logs confd)"
    assert_contains "run -d: a filter and no_new_privs" "seccomp=2 nnp=1" "$out"
    if [ "$HAVE_OCI" -eq 1 ]; then
        out="$(run run --backend oci local/demo:v1 /bin/secprobe)"
        assert_contains "OCI backend: a filter and no_new_privs" "seccomp=2 nnp=1" "$out"
        "$STIVA" run --backend oci local/demo:v1 /bin/secprobe mount >/dev/null 2>&1
        assert_exit "OCI backend: a mount is killed, as the container's own exit" 159 "$?"
    else
        skip "confinement on the OCI backend (no runc or crun here)"
    fi
else
    skip "confinement ($WHY_NOT)"
fi

group "references"
# One rule for what a user may type. `tag` used to read `local/x` as a Hub user, so
# what it had just printed did not resolve, and `run` matched a stored name exactly.
out="$(run tag local/demo:v1 local/alias:v9)"
assert_contains "tag prints what it made" "local/alias:v9" "$out"
assert_contains "and images lists it as printed" "local/alias:v9" "$(run images)"
run tag demo:v1 bare:v8 >/dev/null 2>&1
assert_exit "tag takes a short source" 0 "$?"
out="$(run inspect alias:v9)"
assert_contains "inspect resolves a short name to an image" '"reference": "local/alias:v9"' "$out"
out="$(run inspect local/alias:v9)"
assert_contains "and the full one" '"image_config"' "$out"
out="$(run inspect no-such-thing)"
assert_contains "and still says so when there is neither" "not found" "$out"
"$STIVA" save alias:v9 "$WORK/alias.tar" >/dev/null 2>&1
assert_exit "save takes a short name" 0 "$?"
# rmi of a name removes that name. It went by id, so this used to delete the image
# under every other name as well, and sweep its blobs.
out="$(run rmi bare:v8)"
assert_contains "rmi of one name says so" "Untagged:" "$out"
assert_absent "and it is gone" "bare:v8" "$(run images)"
assert_contains "and the image keeps its other names" "local/alias:v9" "$(run images)"
assert_contains "all of them" "local/demo:v1" "$(run images)"
# Its last name takes the image, and that is reported as the deletion it is.
run tag demo:v1 local/last:v1 >/dev/null 2>&1
LAST_ROOT="$WORK/lastroot"
mkdir -p "$LAST_ROOT"
STIVA_ROOT="$LAST_ROOT" "$STIVA" import "$WORK/rf.tar" solo v1 >/dev/null 2>&1
out="$(STIVA_ROOT="$LAST_ROOT" "$STIVA" rmi solo:v1 2>&1 | grep -v '^\[[0-9]*\] \[')"
assert_contains "rmi of an image's last name reports the deletion" "Deleted:" "$out"
if [ "$CAN_RUN" -eq 1 ]; then
    out="$(run run alias:v9 /bin/argvp ok)"
    assert_contains "run takes a short name" "[ok]" "$out"
else
    skip "run by a short name ($WHY_NOT)"
fi

group "image config"
# What a real image carries, and neither `import` nor a Stivafile can write all of:
# a config rewritten into a saved archive, shaped as Docker writes one, nulls and all,
# and loaded back. Each case reads what the payload was handed, on each path `run` can
# take: the roadmap's rule, since `env` once shipped carried by a spec and read by nobody.
"$STIVA" save local/demo:v1 "$WORK/plain.tar" >/dev/null 2>&1
shape_image () {
    python3 - "$WORK/plain.tar" "$1" "$2" "$3" <<'PY'
import hashlib, io, json, sys, tarfile
src, dst, ref, conf = sys.argv[1:5]
with tarfile.open(src) as t:
    files = {m.name: t.extractfile(m).read() for m in t.getmembers() if m.isfile()}
def blob(digest):
    return files["blobs/sha256/" + digest.split(":")[1]]
def put(data):
    d = hashlib.sha256(data).hexdigest()
    files["blobs/sha256/" + d] = data
    return "sha256:" + d, len(data)
index = json.loads(files["index.json"])
desc = index["manifests"][0]
man = json.loads(blob(desc["digest"]))
cfg = json.loads(blob(man["config"]["digest"]))
cfg["config"] = json.loads(conf)
man["config"]["digest"], man["config"]["size"] = put(json.dumps(cfg).encode())
desc["digest"], desc["size"] = put(json.dumps(man).encode())
desc["annotations"] = {"org.opencontainers.image.ref.name": ref}
files["index.json"] = json.dumps(index).encode()
with tarfile.open(dst, "w") as t:
    for name, data in files.items():
        info = tarfile.TarInfo(name)
        info.size = len(data)
        t.addfile(info, io.BytesIO(data))
PY
    "$STIVA" load "$1" >/dev/null 2>&1
}
SHAPED='{"Entrypoint":null,"Cmd":["/bin/report","from-cmd"],"Env":["FOO=from-image","PATH=/opt/none:/bin"],"WorkingDir":"/made/by/image","User":"","Labels":null,"ExposedPorts":null}'
if [ "$CAN_RUN" -eq 1 ]; then
    shape_image "$WORK/shaped.tar" local/shaped:v1 "$SHAPED"
    assert_exit "a Docker-shaped config loads" 0 "$?"
    assert_contains "inspect shows what it says" "from-cmd" "$(run inspect shaped:v1)"
    for path in "--backend process" "default"; do
        flags=""
        [ "$path" = "default" ] || flags="$path"
        # shellcheck disable=SC2086
        out="$(run run $flags local/shaped:v1)"
        assert_contains "$path: Cmd runs with no command given" "[from-cmd]" "$out"
        assert_contains "$path: the image's Env reaches it" "FOO=from-image" "$out"
        assert_contains "$path: in the image's WorkingDir, made for it" "cwd=/made/by/image" "$out"
        # shellcheck disable=SC2086
        out="$(run run $flags local/shaped:v1 report bare)"
        assert_contains "$path: a bare name found on the image's PATH" "[bare]" "$out"
        assert_absent "$path: arguments replace Cmd" "from-cmd" "$out"
    done
    if [ "$HAVE_OCI" -eq 1 ]; then
        out="$(run run --backend oci local/shaped:v1)"
        assert_contains "OCI backend: all of it" "cwd=/made/by/image FOO=from-image BAR=- [from-cmd]" "$out"
    else
        skip "image config on the OCI backend (no runc or crun here)"
    fi
    run run -d --name cfgd local/shaped:v1 >/dev/null 2>&1
    sleep 1
    assert_contains "run -d: all of it" "cwd=/made/by/image FOO=from-image BAR=- [from-cmd]" "$(run logs cfgd)"

    # With no PATH in the image, `run -d img sleep 1` used to exit 127.
    run run -d --name barep local/demo:v1 report nopath >/dev/null 2>&1
    sleep 1
    assert_contains "a default PATH finds a bare name on run -d" "[nopath]" "$(run logs barep)"

    shape_image "$WORK/entry.tar" local/entry:v1 '{"Entrypoint":["/bin/report","fixed"],"Cmd":["default"]}'
    assert_contains "Entrypoint then Cmd" "[fixed] [default]" "$(run run local/entry:v1)"
    assert_contains "arguments replace Cmd, not Entrypoint" "[fixed] [mine]" "$(run run local/entry:v1 mine)"

    # Not applied yet, so it must say so rather than quietly run as root.
    shape_image "$WORK/user.tar" local/nobody:v1 '{"User":"nobody","Cmd":["/bin/report"]}'
    out="$("$STIVA" run local/nobody:v1 2>&1)"
    assert_contains "an image's User is not silently dropped" "runs as root" "$out"

    # A derived image keeps its base, and a step wins over it.
    mkdir -p "$WORK/dctx"
    cat > "$WORK/Derived" <<'STIVAFILE'
[image]
base = "shaped:v1"
name = "derived"
tag = "v1"

[[steps]]
type = "env"
key = "FOO"
value = "from-step"

[[steps]]
type = "env"
key = "BAR"
value = "from-step"
STIVAFILE
    "$STIVA" build -f "$WORK/Derived" -c "$WORK/dctx" >/dev/null 2>&1
    assert_exit "build derives from a short base name" 0 "$?"
    out="$(run run local/derived:v1)"
    assert_contains "a derived image keeps Cmd and WorkingDir" "cwd=/made/by/image" "$out"
    assert_contains "and takes its own steps over the base's Env" "FOO=from-step BAR=from-step [from-cmd]" "$out"
else
    skip "image config ($WHY_NOT)"
fi

group "output gate"
# A foreground container's output is its owner's. The gate used to quarantine at HIGH,
# and the text /bin/sh is HIGH, so a payload printing one failed with its output withheld
# and its exit status lost. It warns now, and still redacts. The key comes from the
# image's Env and not an argument, because the log line naming the command would hold
# an argument and the test could not say "nowhere".
KEY=AKIAIOSFODNN7EXAMPLE
if [ "$CAN_RUN" -eq 1 ]; then
    shape_image "$WORK/key.tar" local/haskey:v1 "{\"Env\":[\"FOO=$KEY\"],\"Cmd\":[\"/bin/report\",\"after\"]}"
    for path in "--backend process" "default"; do
        flags=""
        [ "$path" = "default" ] || flags="$path"
        # shellcheck disable=SC2086
        out="$("$STIVA" run $flags local/demo:v1 /bin/argvp /bin/show 2>&1)"
        assert_contains "$path: a shell path is printed" "[/bin/show]" "$out"
        assert_contains "$path: and warned about" "output scan: 1 finding(s), worst high" "$out"
        # shellcheck disable=SC2086
        "$STIVA" run $flags local/demo:v1 /bin/errw >/dev/null 2>&1
        assert_exit "$path: a payload's exit status survives" 3 "$?"
        # shellcheck disable=SC2086
        "$STIVA" run $flags local/demo:v1 /bin/argvp /bin/show >/dev/null 2>&1
        assert_exit "$path: flagged output is not a failure" 0 "$?"
        # shellcheck disable=SC2086
        out="$("$STIVA" run $flags local/haskey:v1 2>&1)"
        assert_absent "$path: a credential is on neither stream" "$KEY" "$out"
        assert_contains "$path: it is redacted" "FOO=[REDACTED:" "$out"
        assert_contains "$path: the rest of the output stands" "[after]" "$out"
        assert_contains "$path: and it is named" "cloud_credential" "$out"
        # shellcheck disable=SC2086
        out="$("$STIVA" run $flags local/demo:v1 /bin/argvp clean 2>&1)"
        assert_absent "$path: clean output says nothing" "output scan" "$out"
    done
    run run --name keylog local/haskey:v1 >/dev/null 2>&1
    assert_absent "and the stored log holds no credential" "$KEY" "$(run logs keylog)"
else
    skip "output gate ($WHY_NOT)"
fi

# A config that cannot be trusted refuses, rather than running without what it says.
if [ "$HAVE_PAYLOAD" -eq 1 ]; then
    shape_image "$WORK/bad.tar" local/badcfg:v1 '{"Env":"not a list"}'
    "$STIVA" run local/badcfg:v1 /bin/report >/dev/null 2>&1
    assert_exit "a malformed image config refuses to run" 1 "$?"
    # Refused before a layer is unpacked, so it leaves nothing for `rm` to find.
    shape_image "$WORK/blank.tar" local/blankcmd:v1 '{"Cmd":["/bin/report two"]}'
    before="$(find "$ROOT/containers" -maxdepth 1 | wc -l)"
    "$STIVA" run local/blankcmd:v1 >/dev/null 2>&1
    assert_exit "an image's command with a blank refuses to run" 1 "$?"
    assert_exit "and leaves no container directory" "$before" "$(find "$ROOT/containers" -maxdepth 1 | wc -l)"
else
    skip "a malformed image config ($WHY_NOT)"
fi

group "exec"
# Usage/validation cases run everywhere. The live-exec cases need nsenter AND a
# detached container, so they are gated — but the gate is NOT silent: a skip is
# reported, because a quietly-skipped test reads identically to a passing one.
out="$(run exec)"
assert_contains "exec with no args prints usage" "usage: exec" "$out"
out="$(run exec c2)"
assert_contains "exec with no command prints usage" "usage: exec" "$out"
# NOTE: `$STIVA` directly, not the `run` helper — `run` pipes through grep, so
# `$?` would be grep's status, and it folds 2>&1, which would defeat every
# stream-separation assertion below.
"$STIVA" exec -e JUSTNAME c2 /bin/true >/dev/null 2>&1
assert_exit "a bare --env NAME is a usage error" 2 "$?"
out="$(run exec definitely-no-such-container /bin/true 2>&1)"
assert_contains "exec on an unknown container says so" "not found" "$out"

if [ "$CAN_RUN" -eq 0 ]; then
    skip "exec into a live container ($WHY_NOT)"
elif ! command -v nsenter >/dev/null 2>&1; then
    skip "exec into a live container (no nsenter here)"
else
    run run -d --name execsmoke local/demo:v1 /bin/sleeper >/dev/null 2>&1
    sleep 1
    # The decisive case: run a binary that exists in the IMAGE. If exec were
    # execing on the host this would still pass for /bin/sh, which is why the
    # unit tests assert the primitive and this asserts the plumbing.
    # The image holds only the static payloads built above, so exec runs one of
    # those rather than a shell — which is also the stronger test: /bin/sleeper
    # exists ONLY in the image, so an exec that leaked to the host would fail.
    out="$(run exec execsmoke /bin/errw 2>/dev/null)"
    assert_contains "exec runs an IMAGE-only binary inside the container" "to-stdout" "$out"
    # Streams must be separate — the whole point of the dual-capture primitive.
    # Again `$STIVA` directly: the `run` helper merges them.
    outonly="$("$STIVA" exec execsmoke /bin/errw 2>/dev/null)"
    erronly="$("$STIVA" exec execsmoke /bin/errw 2>&1 >/dev/null | grep -v '^\[[0-9]*\] \[')"
    assert_contains "stdout carries the stdout line" "to-stdout" "$outonly"
    assert_absent "stdout does not carry stderr" "to-stderr" "$outonly"
    assert_contains "stderr carries the stderr line" "to-stderr" "$erronly"
    # The container's exit code becomes stiva's, the way docker exec behaves.
    "$STIVA" exec execsmoke /bin/errw >/dev/null 2>&1
    assert_exit "the command's exit code is stiva's exit code" 3 "$?"
    # What a write to a cgroup does, it does to everything in it, and this one holds this script.
    out="$(run pause execsmoke 2>&1)"
    assert_contains "pause refuses a cgroup it shares" "shares its cgroup" "$out"
    run stop execsmoke >/dev/null 2>&1
    # A stopped container must refuse, not hang or exec on the host.
    out="$(run exec execsmoke /bin/true 2>&1)"
    assert_contains "exec on a stopped container refuses" "not running" "$out"

    # `wait` used to answer at once for a running container, and say 0.
    run run -d --name waitsmoke local/demo:v1 /bin/sleeper >/dev/null 2>&1
    ( sleep 1; "$STIVA" stop waitsmoke >/dev/null 2>&1 ) &
    began=$SECONDS
    "$STIVA" wait waitsmoke >/dev/null 2>&1
    if [ $((SECONDS - began)) -ge 1 ]; then ok "wait waits for a detached container"
    else bad "wait waits for a detached container" "returned in $((SECONDS - began)) s"; fi
    wait
fi

# A second root, for records a command should never have written itself.
ODD="$(mktemp -d)"
trap 'rm -rf "$ROOT" "$WORK" "$ODD"' EXIT
printf '[{"id":"odd1","name":"na\\u001b[2Jme","image_id":"","image_ref":"im\\u001b]0;x","state":"Stopped"}]' \
    > "$ODD/state.json"
out="$(STIVA_ROOT="$ODD" "$STIVA" ps -a 2>/dev/null)"
assert_contains "ps replaces control bytes in a name" "na?[2Jme" "$out"
assert_contains "and in an image reference" "im?]0;x" "$out"
assert_absent "so nothing reaches the terminal" "$(printf '\033')" "$out"

# One stray byte used to mean the next add or check wrote the whole schedule away.
printf '{"jobs":[{"name":5}]}' > "$ODD/cron.json"
STIVA_ROOT="$ODD" "$STIVA" cron ls >/dev/null 2>&1
assert_exit "cron ls refuses a schedule it cannot read" 1 "$?"
STIVA_ROOT="$ODD" "$STIVA" cron add --schedule '* * * * *' j img >/dev/null 2>&1
assert_exit "cron add refuses to save over it" 1 "$?"
STIVA_ROOT="$ODD" "$STIVA" cron check >/dev/null 2>&1
assert_exit "cron check refuses to run from it" 1 "$?"
assert_contains "and the file is as it was" '"name":5' "$(cat "$ODD/cron.json")"

# A failure used to exit 1 and say nothing.
out="$(STIVA_ROOT="$ODD" "$STIVA" export odd1 "$WORK/odd.tar" 2>&1 | grep -v '^\[[0-9]*\] \[')"
assert_contains "export says why it failed" "cannot read" "$out"

printf '\n────────────────────────────\n'
printf '  %d passed, %d failed, %d skipped\n' "$PASS" "$FAIL" "$SKIP"
[ "$FAIL" -eq 0 ] || exit 1
