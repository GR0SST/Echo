#!/usr/bin/env bash
# Prefer this machine's source build; keep Cargo's CDN fallback elsewhere.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
# bindgen may load a non-Apple libclang (e.g. Homebrew's), which has no default SDK.
if [[ "$(uname -s)" == Darwin && -z "${SDKROOT:-}" ]]; then
  SDKROOT="$(xcrun --show-sdk-path)"
  export SDKROOT BINDGEN_EXTRA_CLANG_ARGS="${BINDGEN_EXTRA_CLANG_ARGS:-} -isysroot $SDKROOT"
fi
if [[ -z "${ECHO_CEF_DIST:-}" && -z "${CEF_PATH:-}" && "$(uname -sm)" == "Darwin arm64" &&
      -d "$HOME/Documents/Bot/cef-src/echo-extensions-dist" ]]; then
  ECHO_CEF_DIST="$HOME/Documents/Bot/cef-src/echo-extensions-dist"
fi
if [[ -n "${ECHO_CEF_DIST:-}" ]]; then
  CEF_PATH="$(bun "$ROOT/scripts/cef-dist.ts" check "$ECHO_CEF_DIST")"
  export CEF_PATH
  # Fail closed: never let cef-rs silently download over a bad custom selection.
  [[ -n "$CEF_PATH" ]] || exit 1
fi
[[ $# -gt 0 ]] || { echo "Usage: ECHO_CEF_DIST=/path/to/echo-dist $0 cargo <args>" >&2; exit 2; }
exec "$@"
