#!/usr/bin/env bash
source "$(dirname "$0")/env.sh"
cd src
mkdir -p out/Default
if [[ ! -f out/Default/args.gn ]]; then
  cp "$FORK_ROOT/config/args.gn" out/Default/args.gn
fi
gn gen out/Default
