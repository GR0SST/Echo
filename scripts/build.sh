#!/usr/bin/env bash
source "$(dirname "$0")/env.sh"
bash "$FORK_ROOT/scripts/configure.sh"
cd "$FORK_ROOT/src"
autoninja -C out/Default chrome "$@"
