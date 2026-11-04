#!/usr/bin/env bash
# Build the same Echo as the project's own: pinned CEF source, the complete patch
# stack, then the app bundle. Run from a clone on macOS arm64. Sources and build
# output stay in ECHO_CEF_SOURCE (default ~/Documents/Bot/cef-src).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SOURCE="${ECHO_CEF_SOURCE:-$HOME/Documents/Bot/cef-src}"
cd "$ROOT"
scripts/build-cef.sh release
DIST="$(cat "$SOURCE/candidate-path")"
ECHO_CEF_DIST="$DIST" scripts/with-cef.sh cargo run -p bundle
echo "Built target/bundle/Echo.app with CEF $DIST"
echo "Unqualified until: bun scripts/check-cef-canonical.ts (see docs/CEF_BUILD.md)"
