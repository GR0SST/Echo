#!/usr/bin/env bash
# Type-check target-specific Rust, including the real CEF bindings. No app is launched.
set -u -o pipefail

repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root" || exit 1
if [[ ${1:-} == --help ]]; then
    cat <<'HELP'
Usage: scripts/check-targets.sh [target-triple ...]
Defaults: x86_64-unknown-linux-gnu x86_64-pc-windows-msvc
Uses a rustup-managed toolchain with the requested targets installed.
CEF's check-only feature skips its native distribution/wrapper, not Rust code.
On a foreign host, Windows uses cargo-xwin when available. Linux can use Zig
and a real target SDK selected by ECHO_LINUX_SYSROOT. See docs/PLATFORMS.md.
HELP
    exit 0
fi
cargo_bin=${CARGO:-cargo}
rustc_bin=${RUSTC:-rustc}
host=$("$rustc_bin" -vV | sed -n 's/^host: //p')
if [[ -z "$host" ]]; then
    echo 'Cannot identify the active Rust host toolchain.' >&2
    exit 1
fi
targets=("$@")
if (( ${#targets[@]} == 0 )); then
    targets=(x86_64-unknown-linux-gnu x86_64-pc-windows-msvc)
fi

check_target() (
    target=$1
    std_dir=$("$rustc_bin" --print target-libdir --target "$target") || exit 1
    # Rust prints Windows paths; Git Bash globbing needs its POSIX spelling.
    if [[ "$host" == *windows* ]]; then
        if command -v cygpath >/dev/null; then
            std_dir=$(cygpath -u "$std_dir") || exit 1
        else
            std_dir=${std_dir//\\//}
        fi
    fi
    if ! compgen -G "$std_dir/libstd-*.rlib" >/dev/null; then
        echo "Missing Rust standard library for $target in $std_dir." >&2
        echo "Run rustup target add $target using the toolchain that supplies this rustc." >&2
        exit 1
    fi
    # Keep each target's native CEF cache separate from the existing macOS bundle.
    export CEF_PATH="${ECHO_CEF_ROOT:-$repo_root/third_party/cef}/$target"
    if [[ "$target" == x86_64-unknown-linux-gnu && "$host" != "$target" && -n ${ECHO_LINUX_SYSROOT:-} ]]; then
        sdk=$(cd -- "$ECHO_LINUX_SYSROOT" && pwd) || exit 1
        command -v zig >/dev/null || { echo 'Zig is required with ECHO_LINUX_SYSROOT.' >&2; exit 1; }
        export CC_x86_64_unknown_linux_gnu="${CC_x86_64_unknown_linux_gnu:-$repo_root/scripts/zig-linux-cc.sh}"
        export CXX_x86_64_unknown_linux_gnu="${CXX_x86_64_unknown_linux_gnu:-$repo_root/scripts/zig-linux-cc.sh --cxx}"
        export AR_x86_64_unknown_linux_gnu="${AR_x86_64_unknown_linux_gnu:-zig ar}"
        export PKG_CONFIG_ALLOW_CROSS=1 PKG_CONFIG_SYSROOT_DIR="$sdk" PKG_CONFIG_PATH=
        export PKG_CONFIG_LIBDIR="$sdk/usr/lib/x86_64-linux-gnu/pkgconfig:$sdk/usr/lib/pkgconfig:$sdk/usr/share/pkgconfig"
    fi
    runner=("$cargo_bin" check)
    if [[ "$target" == x86_64-pc-windows-msvc && "$host" != *windows* ]] && command -v cargo-xwin >/dev/null; then
        runner=(cargo-xwin check)
    fi
    printf '\nChecking %s (host %s, CEF check-only)\n' "$target" "$host"
    "${runner[@]}" --locked --workspace --target "$target" --features engine/check-only
)

result=0
for target in "${targets[@]}"; do
    if check_target "$target"; then
        printf 'PASS %s\n' "$target"
    else
        printf 'FAIL %s\n' "$target" >&2
        result=1
    fi
done
exit "$result"
