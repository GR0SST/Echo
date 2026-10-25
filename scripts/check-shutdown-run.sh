#!/bin/zsh
# Called only under the screen lock by check-shutdown.py. Bound the entire holder.
set -eu
cd "${0:A:h:h}"
export TMPDIR="$1/"
export ECHO_SHUTDOWN_TRACE=1
export BROWSER_DRIVE_STRICT_EXIT=1
export BROWSER_DRIVE_GPU_MODE="$3"
export BROWSER_DRIVE_CEF_LOG_PATH="$1/$2-cef.log"
if [[ "${ECHO_SHUTDOWN_PERFETTO:-0}" == 1 ]]; then
  export BROWSER_DRIVE_TRACE_PATH="$1/$2.pftrace"
fi
export BROWSER_DRIVE_LOG_PATH="$1/$2-engine.log"
exec gtimeout -k 3 75 scripts/drive.sh "$4" 65
