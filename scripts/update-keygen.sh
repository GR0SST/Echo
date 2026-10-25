#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
export CARGO_BUILD_JOBS=3
KEY="${ECHO_SIGNING_KEY:-$HOME/.config/echo/update-signing.key}"
mkdir -p -m 700 "$(dirname "$KEY")"
cargo run -p echo_updater --bin echo-release -- keygen "$KEY" config/update-public-key.txt
