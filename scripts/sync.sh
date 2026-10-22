#!/usr/bin/env bash
source "$(dirname "$0")/env.sh"
# Keep the current browser branch; download DEPS and execute Chromium hooks.
gclient sync --no-history
