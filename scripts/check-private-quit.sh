#!/bin/zsh
# Native regression: a captured private page must not retain CEF at application quit.
# Build the current bundle first with `cargo run -p bundle`.
set -eu
cd "$(dirname "$0")/.."
if [[ ${1:-} != --locked ]]; then
  exec lockf -k /tmp/echo-screen.lock gtimeout -k 3 90 zsh "$0" --locked "${1:-target/parity/library-polish/quit}"
fi
D=${2:?evidence directory required}
mkdir -p "$D"
D=${D:A}
for scenario in captured-private closed-private-tab; do
  runtime="$D/$scenario"
  mkdir -p "$runtime"
  if [[ $scenario == captured-private ]]; then
    automation="wait 1; key cmd-shift-n; wait 1; window last; resize 1000 760; key escape; key cmd-t; wait 1; type data:text/html,privatequitcapture; key enter; wait 2; dump; windows; shot $runtime/capture.png; quit"
  else
    automation="wait 1; key cmd-shift-n; wait 1; window last; resize 1000 760; key escape; key cmd-t; wait 1; type data:text/html,firstprivatepage; key enter; wait 1; key cmd-t; wait 1; type data:text/html,secondprivatepage; key enter; wait 1; dump; key cmd-w; wait 1; dump; windows; shot $runtime/capture.png; quit"
  fi
  BROWSER_DRIVE_STRICT_EXIT=1 TMPDIR="$runtime/" scripts/drive.sh "$automation" 25 > "$runtime/drive.log" 2>&1
  rg -q '^\[automation\] selected_window private=true' "$runtime/drive.log"
  rg -q '^\[dump\] today.*data:text/html.*loading=false.*visible=true' "$runtime/drive.log"
  if [[ $scenario == closed-private-tab ]]; then
    [[ $(rg -c '^\[dump\] today.*data:text/html' "$runtime/drive.log") == 3 ]]
    [[ $(rg -c '^\[dump\] today.*secondprivatepage' "$runtime/drive.log") == 1 ]]
    [[ $(rg -c '^\[dump\] today.*firstprivatepage' "$runtime/drive.log") == 2 ]]
  fi
  rg -q '\[quit\] CEF Complete; owners=0; browsers=\[\]; cookie_flushes=0; failures=\[\]' "$runtime/drive.log"
  rg -q 'invoking preserved original terminate exactly once' "$runtime/drive.log"
  if rg -qi 'watchdog|Termination deferred|\[automation\] aborted:' "$runtime/drive.log"; then
    cat "$runtime/drive.log"
    exit 1
  fi
  print "$scenario: process exited successfully after CEF Complete"
done
