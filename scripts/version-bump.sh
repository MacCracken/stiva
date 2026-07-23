#!/usr/bin/env bash
set -euo pipefail

# Bump the project version.
#
# `cyrius.cyml` reads `version = "${file:VERSION}"`, so VERSION is the single
# source of truth for the MANIFEST — but nothing interpolates it into the
# compiled binary. `src/main.cyr`'s `_stiva_version_str()` holds a hand-written
# literal, and `dist/stiva.cyr`'s bundle header is stamped at distlib time.
# Both drift silently: 3.0.5 was first built from an already-bumped tree and
# `stiva --version` still said 3.0.4. This script now updates all three.
#
# Git is the user's: this script touches files only, never runs git.

NEW_VERSION="${1:?Usage: $0 <new-version>}"

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

echo "$NEW_VERSION" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+([-+][0-9A-Za-z.]+)?$' || {
    echo "error: '$NEW_VERSION' is not valid semver" >&2
    exit 1
}

OLD_VERSION="$(tr -d '[:space:]' < VERSION 2>/dev/null || echo '?')"

# 1. VERSION — the manifest's source of truth.
echo "$NEW_VERSION" > VERSION
echo "  VERSION: $OLD_VERSION -> $NEW_VERSION"

# 2. src/main.cyr — the literal behind `stiva --version` / `--help`.
#    Matches the `X.Y.Z — ` prefix inside _stiva_version_str's return string,
#    leaving the descriptive tail alone.
if grep -qE '^\s*return "[0-9]+\.[0-9]+\.[0-9]+ — ' src/main.cyr; then
    sed -i -E "s/^(\s*return \")[0-9]+\.[0-9]+\.[0-9]+( — )/\1${NEW_VERSION}\2/" src/main.cyr
    echo "  src/main.cyr: _stiva_version_str() -> $(grep -oE '"[0-9]+\.[0-9]+\.[0-9]+ — [^"]*"' src/main.cyr | head -1)"
else
    echo "  WARNING: could not find the version literal in src/main.cyr — check _stiva_version_str() by hand" >&2
fi

# 3. dist/stiva.cyr — the bundle header carries the version, so a bump must
#    regenerate it. Skipped if the toolchain is not on PATH.
if command -v cyrius >/dev/null 2>&1; then
    cyrius distlib >/dev/null 2>&1 && echo "  dist/stiva.cyr: regenerated (cyrius distlib)" \
        || echo "  WARNING: cyrius distlib failed — regenerate dist/stiva.cyr by hand" >&2
else
    echo "  NOTE: cyrius not on PATH — run 'cyrius distlib' to restamp dist/stiva.cyr"
fi

cat <<EOF

Bumped stiva to $NEW_VERSION.

Still to do by hand:
  - CHANGELOG.md      — add a '## [$NEW_VERSION] — <date>' section
  - README.md / CLAUDE.md — current-version statements (leave history alone)
  - zugot recipe      — marketplace/stiva.cyml 'version = "$NEW_VERSION"'
  - rebuild + verify  — cyrius build src/main.cyr build/stiva && cyrius tests tests/
                        then confirm: ./build/stiva --version
EOF
