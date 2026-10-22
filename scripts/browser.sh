#!/usr/bin/env bash
source "$(dirname "$0")/env.sh"
BROWSER_BIN="$FORK_ROOT/src/out/Default/Chromium.app/Contents/MacOS/Chromium"
if [[ ! -x "$BROWSER_BIN" ]]; then
  echo 'Browser has not been built yet. Run: bun run sync && bun run build' >&2
  exit 1
fi
exec "$BROWSER_BIN" --user-data-dir="$FORK_ROOT/.browser-profile" "$@"
