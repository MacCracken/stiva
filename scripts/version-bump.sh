#!/usr/bin/env bash
set -euo pipefail

# Bump the project version. cyrius.cyml reads `version = "${file:VERSION}"`, so
# the single source of truth is the VERSION file.
NEW_VERSION="${1:?Usage: $0 <new-version>}"

echo "$NEW_VERSION" > VERSION

echo "Bumped to $NEW_VERSION (VERSION file; cyrius.cyml reads it via \${file:VERSION})"
echo "Remember to update the recipe version in zugot to match."
