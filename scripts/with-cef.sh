#!/usr/bin/env bash
# Opt in per command. Without ECHO_CEF_DIST, Cargo keeps its default CDN build.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
if [[ -n "${ECHO_CEF_DIST:-}" ]]; then
  CEF_PATH="$(bun "$ROOT/scripts/cef-dist.ts" check "$ECHO_CEF_DIST")"
  export CEF_PATH
  # Fail closed: never let cef-rs silently download over a bad custom selection.
  [[ -n "$CEF_PATH" ]] || exit 1
fi
[[ $# -gt 0 ]] || { echo "Usage: ECHO_CEF_DIST=/path/to/echo-dist $0 cargo <args>" >&2; exit 2; }
exec "$@"
