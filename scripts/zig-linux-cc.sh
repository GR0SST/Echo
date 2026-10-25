#!/usr/bin/env bash
# cc-rs adds Rust's target spelling; Zig expects x86_64-linux-gnu instead.
set -euo pipefail
driver=cc
if [[ ${1:-} == --cxx ]]; then
    driver=c++
    shift
fi
args=(-target x86_64-linux-gnu)
for arg in "$@"; do
    case "$arg" in
        --target=x86_64-unknown-linux-gnu) args+=(--target=x86_64-linux-gnu) ;;
        *) args+=("$arg") ;;
    esac
done
exec zig "$driver" "${args[@]}"
