#!/bin/zsh
# One bounded screen session for all sixteen exits; no per-drive nested locks.
set -eu
cd "${0:A:h:h}"
lockf -k /tmp/echo-screen.lock gtimeout -k 5 700 zsh -c '
  set -eu
  python3 -c '\''import datetime, os, time
deadline = os.environ.get("ECHO_CEF_SCREEN_DEADLINE")
if deadline and time.time() + 700 >= datetime.datetime.fromisoformat(deadline.replace("Z", "+00:00")).timestamp():
    raise SystemExit("Insufficient time before the screen-work cutoff")'\''
  touch /tmp/echo-screen-long
  trap "rm -f /tmp/echo-screen-long" EXIT
  python3 scripts/check-shutdown-preflight.py
  python3 scripts/check-shutdown.py --screen-locked "$@"
' -- "$@"
