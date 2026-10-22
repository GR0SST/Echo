#!/usr/bin/env bash
source "$(dirname "$0")/env.sh"
CLONE_PID="${1:?clone process ID required}"
trap 'printf "failed\n" > "$FORK_ROOT/.checkout-status"' ERR
printf 'downloading\n' > "$FORK_ROOT/.checkout-status"
while kill -0 "$CLONE_PID" 2>/dev/null; do
  sleep 10
done
git -C src rev-parse --verify HEAD
git -C src remote add upstream https://chromium.googlesource.com/chromium/src.git
git -C src switch -c browser/main
git -C src config core.untrackedCache true
git -C src log -1 --format='%H %s' > "$FORK_ROOT/.checkout-revision"
printf 'ready\n' > "$FORK_ROOT/.checkout-status"
printf 'Checkout ready. Next: bun run sync, then bun run build.\n'
