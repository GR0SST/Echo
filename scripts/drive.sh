#!/usr/bin/env bash
# Runs the bundled app with an automation script (see crates/browser/src/automation.rs)
# and prints its log. Usage: scripts/drive.sh "wait 5; shot /tmp/a.png; dump; quit" [timeout]
set -uo pipefail
cd "$(dirname "$0")/.."
SCRIPT="${1:?automation script required}"
LIMIT="${2:-60}"
# Run a private copy so rebuilding or a running app does not interfere.
COPY="${TMPDIR:-/tmp}/browser-drive.app"
rm -rf "$COPY" && cp -R target/bundle/browser.app "$COPY"
APP="$COPY/Contents/MacOS/browser"
LOG=$(mktemp -t browser-drive)
# Snapshots of our own window cannot see layers hosted by Chromium's GPU
# process without Screen Recording permission; in-process GPU keeps them local.
# A separate profile, so this works while the app is open.
BROWSER_DATA_DIR="${TMPDIR:-/tmp}/browser-drive-profile" \
BROWSER_SCRIPT="$SCRIPT" "$APP" --in-process-gpu > "$LOG" 2>&1 &
PID=$!
for _ in $(seq 1 "$LIMIT"); do
  kill -0 "$PID" 2>/dev/null || break
  sleep 1
done
kill "$PID" 2>/dev/null
cat "$LOG"
