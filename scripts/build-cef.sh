#!/usr/bin/env bash
# Pinned CEF 154 codec build. All sources/artifacts stay outside the repository.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MODE="${1:-all}"
case "$MODE" in all|sync|build|distrib) ;; *) echo "Usage: $0 [all|sync|build|distrib]" >&2; exit 2;; esac
if [[ "${2:-}" != --locked ]]; then
  # Serialize sync, generation, compilation and packaging across worktrees.
  exec lockf -k -t 0 /tmp/echo-cef-build.lock nice -n 10 bash "$0" "$MODE" --locked
fi
[[ "$(uname -s)/$(uname -m)" == Darwin/arm64 ]] || { echo "This recipe requires macOS arm64" >&2; exit 1; }
DOWNLOAD="${ECHO_CEF_SOURCE:-$HOME/Documents/Bot/cef-src}"
mkdir -p "$DOWNLOAD"
DOWNLOAD="$(cd "$DOWNLOAD" && pwd -P)"
case "$DOWNLOAD/" in "$ROOT/"*) echo "CEF sources must be outside the Echo checkout" >&2; exit 1;; esac
COMMON="$(git -C "$ROOT" rev-parse --path-format=absolute --git-common-dir)"
TOOLS="${ECHO_DEPOT_TOOLS:-$(dirname "$COMMON")/depot_tools}"
[[ -x "$TOOLS/gclient" ]] || { echo "Set ECHO_DEPOT_TOOLS to the existing depot_tools directory" >&2; exit 1; }
export PATH="$TOOLS:$PATH" DEPOT_TOOLS_UPDATE=0 PYTHONUNBUFFERED=1 CEF_ARCHIVE_FORMAT=tar.bz2
export GN_OUT_CONFIGS=Release_GN_arm64
# Keep official release optimization, but avoid PGO/LTO and debug-symbol disk costs.
export GN_DEFINES='proprietary_codecs=true ffmpeg_branding=Chrome is_official_build=true symbol_level=0 use_thin_lto=false chrome_pgo_phase=0 use_remoteexec=false use_siso=false use_system_xcode=true'
CEF_COMMIT=14c5a089e8452874cb1a556dc3fde26ed2d87723
CHROMIUM_COMMIT=b859317bf11f6be47f9b7799ec690a0a42a1fb33
AUTOMATE_COMMIT=e537569ecda2e3f23ffdac559ecf7f80b8e59467
AUTOMATE_SHA256=94d3ddd1e0d35482132f45dc101a593c9371b6eb2d8ff4af6cfe2734d2e48c6a
mkdir -p "$DOWNLOAD/automate" "$DOWNLOAD/logs"
exec > >(tee "$DOWNLOAD/logs/$MODE-$(date -u +%Y%m%dT%H%M%SZ).log") 2>&1
STARTED=$SECONDS
finish() {
  result=$?
  printf '\nExit=%s elapsed_seconds=%s\n' "$result" "$((SECONDS - STARTED))"
  du -sk "$DOWNLOAD"
  df -k "$DOWNLOAD"
}
trap finish EXIT
echo "Started $(date -u); GN_DEFINES=$GN_DEFINES"
xcodebuild -version
df -k "$DOWNLOAD"
AUTOMATE="$DOWNLOAD/automate/automate-git.py"
if [[ ! -f "$AUTOMATE" ]]; then
  curl -fL --retry 3 "https://raw.githubusercontent.com/chromiumembedded/cef/$AUTOMATE_COMMIT/tools/automate/automate-git.py" -o "$AUTOMATE"
fi
echo "$AUTOMATE_SHA256  $AUTOMATE" | shasum -a 256 -c -
ARGS=("--download-dir=$DOWNLOAD" "--depot-tools-dir=$TOOLS" --no-depot-tools-update
  --branch=8037 "--checkout=$CEF_COMMIT" --no-chromium-history --arm64-build --no-debug-build)
if [[ "$MODE" == all || "$MODE" == sync ]]; then
  python3 "$AUTOMATE" "${ARGS[@]}" --no-build --no-distrib
fi
SRC="$DOWNLOAD/chromium/src"
if [[ "$MODE" == all || "$MODE" == build || "$MODE" == distrib ]]; then
  [[ "$(git -C "$SRC" rev-parse HEAD)" == "$CHROMIUM_COMMIT" ]]
  [[ "$(git -C "$SRC/cef" rev-parse HEAD)" == "$CEF_COMMIT" ]]
fi
if [[ "$MODE" == all || "$MODE" == build ]]; then
  (cd "$SRC/cef" && python3 tools/gclient_hook.py)
  # Explicit ninja, rather than autoninja's automatic machine-wide parallelism.
  # macOS make_distrib.py packages the framework from cefclient.app.
  ninja -C "$SRC/out/Release_GN_arm64" -j 8 cefclient
fi
if [[ "$MODE" == all || "$MODE" == distrib ]]; then
  # automate-git's --no-distrib-symbols does not pass --no-symbols to a
  # minimal distribution. Call the official packager with that flag explicitly.
  (cd "$SRC/cef/tools" && python3 make_distrib.py \
    "--output-dir=$SRC/cef/binary_distrib" --allow-partial --ninja-build \
    --arm64-build --minimal --no-symbols --no-docs)
  DIST="$SRC/cef/binary_distrib/cef_binary_154.0.34+g14c5a08+chromium-154.0.8037.98_macosarm64_minimal"
  bun "$ROOT/scripts/cef-dist.ts" stage "$DIST" "$DOWNLOAD/echo-dist"
  echo "Use: ECHO_CEF_DIST='$DOWNLOAD/echo-dist' scripts/with-cef.sh cargo test"
fi
