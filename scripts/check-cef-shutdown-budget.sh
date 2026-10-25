#!/usr/bin/env bash
# Exercise the production timeout policy with a constexpr fake clock.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SRC="${1:-${ECHO_CEF_SOURCE:-$HOME/Documents/Bot/cef-src}/chromium/src}"
xcrun --sdk macosx clang++ -std=c++23 -DNDEBUG -fsyntax-only \
  -I"$SRC" -I"$SRC/out/Release_GN_arm64/gen" \
  -I"$SRC/base/allocator/partition_allocator/src" \
  -I"$SRC/out/Release_GN_arm64/gen/base/allocator/partition_allocator/src" \
  -I"$SRC/third_party/abseil-cpp" \
  "$ROOT/patches/cef154/tests/shutdown-budget-test.cc"
echo "CEF shutdown budget fake-clock checks passed"
