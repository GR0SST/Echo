#!/usr/bin/env bash
# Runs the bundled app with an automation script (see crates/browser/src/automation.rs)
# and prints its log. Usage: scripts/drive.sh "wait 5; shot /tmp/a.png; dump; quit" [timeout]
set -uo pipefail
cd "$(dirname "$0")/.."
SCRIPT="${1:?automation script required}"
LIMIT="${2:-60}"
# Run a private copy so rebuilding or a running app does not interfere.
COPY="${TMPDIR:-/tmp}/browser-drive.app"
rm -rf "$COPY" && cp -R "${BROWSER_DRIVE_BUNDLE_PATH:-target/bundle/Echo.app}" "$COPY"
if [[ "${BROWSER_DRIVE_LOCAL_CRASH_DUMPS:-0}" == 1 ]]; then
  # No upload endpoint; let CEF's watchdog write its own local failure report.
  cat > "$COPY/Contents/Resources/crash_reporter.cfg" <<'CFG'
[Config]
ProductName=EchoShutdownAcceptance
BrowserCrashForwardingEnabled=false
ServerURL=
CFG
fi
APP="$COPY/Contents/MacOS/Echo"
LOG="${BROWSER_DRIVE_LOG_PATH:-$(mktemp -t browser-drive)}"
# Snapshots of our own window cannot see layers hosted by Chromium's GPU
# process without Screen Recording permission; in-process GPU keeps them local.
# A separate profile, so this works while the app is open.
case "${BROWSER_DRIVE_GPU_MODE:-in-process}" in
  in-process) ENGINE_FLAGS=(--in-process-gpu) ;;
  separate) ENGINE_FLAGS=() ;;
  *) echo "[drive] GPU mode must be in-process or separate" >&2; exit 2 ;;
esac
if [[ -n "${BROWSER_DRIVE_CEF_LOG_PATH:-}" ]]; then
  ENGINE_FLAGS+=(--enable-logging "--log-file=$BROWSER_DRIVE_CEF_LOG_PATH" --v=1)
fi
if [[ -n "${BROWSER_DRIVE_TRACE_PATH:-}" ]]; then
  ENGINE_FLAGS+=(--enable-tracing=shutdown,toplevel --trace-startup-format=proto "--trace-startup-file=$BROWSER_DRIVE_TRACE_PATH")
fi
BROWSER_DATA_DIR="${TMPDIR:-/tmp}/browser-drive-profile" \
BROWSER_SCRIPT="$SCRIPT" "$APP" "${ENGINE_FLAGS[@]}" > "$LOG" 2>&1 &
PID=$!
EXITED=0
for _ in $(seq 1 "$LIMIT"); do
  if ! kill -0 "$PID" 2>/dev/null; then
    EXITED=1
    break
  fi
  sleep 1
done
if [[ "$EXITED" == 0 ]]; then
  kill "$PID" 2>/dev/null
  sleep 0.2
  # Only our still-running shell job may be force-stopped. Never leave the
  # test window alive after the caller releases the shared screen lock.
  if [[ "$(jobs -pr)" == "$PID" ]]; then kill -KILL "$PID" 2>/dev/null; fi
fi
APP_STATUS=0
wait "$PID" || APP_STATUS=$?
cat "$LOG"
if rg -q '^\[automation\] aborted:' "$LOG"; then
  exit 1
fi

# Existing diagnostic callers may inspect a deliberately interrupted app. Native
# exit regressions opt in so a timeout/signal cannot masquerade as a clean quit.
if [[ "${BROWSER_DRIVE_STRICT_EXIT:-0}" == 1 ]]; then
  if [[ "$EXITED" != 1 ]]; then
    echo "[drive] timed out after ${LIMIT}s" >&2
    exit 124
  fi
  echo "[drive] application exited with status ${APP_STATUS}"
  exit "$APP_STATUS"
fi
