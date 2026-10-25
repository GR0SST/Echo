#!/usr/bin/env bash
# Pinned CEF 154 codec build. All sources/artifacts stay outside the repository.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MODE="${1:-release}"
case "$MODE" in all|sync|build|distrib|release) ;; *) echo "Usage: $0 [release|sync|build|distrib|all]" >&2; exit 2;; esac
if [[ "${2:-}" != --locked ]]; then
  # Serialize sync, generation, compilation and packaging across worktrees.
  # Keep the wrapper in memory: another worktree may update this file during
  # a long compile, and Bash otherwise resumes reading at stale file offsets.
  exec lockf -k -t 0 /tmp/echo-cef-build.lock nice -n 10 bash -c "$(cat "$0")" "$0" "$MODE" --locked
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
STARTED_UTC="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
BUILD_ID="$(date -u +%Y%m%dT%H%M%SZ)-$$"
STACK_JSON="$DOWNLOAD/logs/stack-$BUILD_ID.json"
BUILD_RECORD="$DOWNLOAD/build-state.json"
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
  if [[ -f "$DOWNLOAD/chromium/src/cef/include/echo_cef_extensions_v1.h" ||
        -f "$DOWNLOAD/chromium/src/cef/include/echo_cef_download_v1.h" ||
        -f "$DOWNLOAD/chromium/src/content/public/common/shutdown_timeouts.h" ]] ||
      rg -q 'echo_keyboard_controls_' "$DOWNLOAD/chromium/src/chrome/browser/ui/views/overlay/video_overlay_window_views.h" 2>/dev/null; then
    echo "Refusing to sync over custom sources; use release for an incremental build or prepare a separate fresh checkout" >&2
    exit 1
  fi
  python3 "$AUTOMATE" "${ARGS[@]}" --no-build --no-distrib
fi
SRC="$DOWNLOAD/chromium/src"
if [[ "$MODE" != sync ]]; then
  [[ "$(git -C "$SRC" rev-parse HEAD)" == "$CHROMIUM_COMMIT" ]]
  [[ "$(git -C "$SRC/cef" rev-parse HEAD)" == "$CEF_COMMIT" ]]
  # Includes every top-level patch, in series.json order. Rehearsal rejects
  # partial stacks without touching source, and retains all already-applied work.
  bun "$ROOT/scripts/cef-stack.ts" apply "$SRC" | tee "$STACK_JSON"
fi
if [[ "$MODE" == all || "$MODE" == build || "$MODE" == release ]]; then
  # Regenerate the download bridge's additive native loader before GN/build.
  python3 "$SRC/cef/tools/translator.py" "--root-dir=$SRC/cef"
  (cd "$SRC/cef" && python3 tools/version_manager.py -u --fast-check)
  python3 "$ROOT/scripts/configure-cef.py" "$SRC"
  # Explicit ninja, rather than autoninja's automatic machine-wide parallelism.
  # macOS make_distrib.py packages the framework from cefclient.app.
  ninja -C "$SRC/out/Release_GN_arm64" -j 8 cefclient
  "$ROOT/scripts/check-cef-shutdown-budget.sh" "$SRC"
  bun "$ROOT/scripts/cef-stack.ts" check "$SRC" > "$STACK_JSON"
  bun "$ROOT/scripts/cef-build-record.ts" write "$BUILD_RECORD" "$SRC" "$TOOLS" "$STACK_JSON" "$GN_DEFINES" "$STARTED_UTC" "$((SECONDS - STARTED))"
fi
if [[ "$MODE" == all || "$MODE" == distrib || "$MODE" == release ]]; then
  bun "$ROOT/scripts/cef-build-record.ts" verify "$BUILD_RECORD" "$SRC" "$STACK_JSON" "$GN_DEFINES"
  # Package to a new immutable generation. Never overwrite a framework that
  # another Cargo build or running browser may have resolved already.
  DISTRIB_ROOT="$DOWNLOAD/canonical-binary-distrib/$BUILD_ID"
  STAGED="$DOWNLOAD/canonical-adapters/$BUILD_ID"
  mkdir -p "$DISTRIB_ROOT" "$(dirname "$STAGED")"
  (cd "$SRC/cef/tools" && python3 make_distrib.py \
    "--output-dir=$DISTRIB_ROOT" --allow-partial --ninja-build \
    --arm64-build --minimal --no-symbols --no-docs)
  DIST="$DISTRIB_ROOT/cef_binary_154.0.34+g14c5a08+chromium-154.0.8037.98_macosarm64_minimal"
  bun "$ROOT/scripts/cef-dist.ts" stage "$DIST" "$STAGED"
  bun "$ROOT/scripts/cef-build-record.ts" stage "$BUILD_RECORD" "$STAGED"
  printf '%s\n' "$STAGED" > "$DOWNLOAD/candidate-path"
  echo "CEF candidate: $STAGED"
  echo "Qualify and publish: bun scripts/check-cef-canonical.ts"
fi
