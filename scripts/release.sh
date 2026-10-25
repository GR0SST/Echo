#!/usr/bin/env bash
# Produce authenticated static files; this script never contacts an update host.
set -euo pipefail
cd "$(dirname "$0")/.."
export CARGO_PROFILE_RELEASE_DEBUG_ASSERTIONS=false
export CARGO_BUILD_JOBS=3 DOTNET_PROCESSOR_COUNT=3 DOTNET_CLI_TELEMETRY_OPTOUT=1
VERSION="${1:?Usage: scripts/release.sh VERSION OUTPUT_DIR [CHANNEL]}"
OUTPUT_ROOT="${2:?Output directory required}"
CHANNEL="${3:-stable}"
KEY="${ECHO_SIGNING_KEY:-$HOME/.config/echo/update-signing.key}"
VPK="${VPK:-vpk}"
[[ -f "$KEY" ]] || { echo "No signing key: run scripts/update-keygen.sh first." >&2; exit 1; }
command -v "$VPK" >/dev/null || { echo "Install pinned CLI: dotnet tool install vpk --version 1.2.161 --tool-path ~/.local/bin" >&2; exit 1; }
# Homebrew's dotnet wrapper finds its SDK; generated tool apphosts also need
# DOTNET_ROOT when the SDK is outside /usr/local/share/dotnet.
if [[ "$(uname -s)" == Darwin && -z "${DOTNET_ROOT:-}" ]]; then
  DOTNET_ROOT="$(dotnet --list-sdks | sed -n '1s/.*\[\(.*\)\/sdk\]/\1/p')"
  [[ -z "$DOTNET_ROOT" ]] || export DOTNET_ROOT
fi
"$VPK" --help >/dev/null

export ECHO_RELEASE_VERSION="$VERSION" ECHO_UPDATE_CHANNEL="$CHANNEL"
export ECHO_BUILD_NUMBER="${ECHO_BUILD_NUMBER:-$(git rev-list --count HEAD)}"
ARCH="$(uname -m)"; ARCH="${ARCH/arm64/aarch64}"
SIGN=(); PACK_FLAGS=(); MAIN_EXE=Echo
case "$(uname -s)" in
  Darwin)
    PLATFORM="macos-$ARCH"
    ECHO_CEF_DIST="${ECHO_CEF_DIST:-$HOME/Documents/Bot/cef-src/echo-extensions-dist}"
    PACK=target/bundle/Echo.app
    PACK_FLAGS+=(--bundleId dev.echo.browser --icon crates/bundle/assets/Echo.icns)
    SIGN=()
    if [[ -n "${APPLE_DEVELOPER_ID:-}" ]]; then
      SIGN+=(--signAppIdentity "$APPLE_DEVELOPER_ID")
      [[ -z "${APPLE_INSTALLER_ID:-}" ]] || SIGN+=(--signInstallIdentity "$APPLE_INSTALLER_ID")
      [[ -z "${APPLE_NOTARY_PROFILE:-}" ]] || SIGN+=(--notaryProfile "$APPLE_NOTARY_PROFILE")
      SIGN+=(--signEntitlements config/release-entitlements.plist)
      [[ -n "${APPLE_INSTALLER_ID:-}" ]] || echo "Installer signing skipped: APPLE_INSTALLER_ID is not configured."
      [[ -n "${APPLE_NOTARY_PROFILE:-}" ]] || echo "Notarization skipped: APPLE_NOTARY_PROFILE is not configured."
    else
      echo "Apple Developer ID signing and notarization skipped: APPLE_DEVELOPER_ID is not configured. Output is for local testing."
    fi
    ;;
  Linux)
    PLATFORM="linux-$ARCH"; PACK=target/bundle/Echo
    PACK_FLAGS+=(--icon crates/browser/assets/echo-icon.png)
    ;;
  MINGW*|MSYS*|CYGWIN*)
    PLATFORM="windows-$ARCH"; PACK=target/bundle/Echo; MAIN_EXE=Echo.exe
    ;;
  *) echo "Unsupported release host: $(uname -s)" >&2; exit 2 ;;
esac
# Freeze the qualified native distribution; never use the CDN fallback in releases.
ECHO_CEF_DIST="$(bun scripts/check-release-cef.ts "${ECHO_CEF_DIST:?Set ECHO_CEF_DIST to a qualified native CEF distribution}")"
export ECHO_CEF_DIST CEF_PATH="$ECHO_CEF_DIST"
cargo run --locked -p bundle -- --release
if [[ "$(uname -s)" == Darwin ]]; then
  cmp -s "$ECHO_CEF_DIST/Chromium Embedded Framework.framework/Chromium Embedded Framework" "$PACK/Contents/Frameworks/Chromium Embedded Framework.framework/Chromium Embedded Framework" || { echo 'Bundled CEF differs from the qualified distribution.' >&2; exit 1; }
fi
OUT="${ECHO_RELEASE_CACHE:-target/update-packages}/$CHANNEL/$PLATFORM"
mkdir -p "$OUT" "$OUTPUT_ROOT"
# Keep prior releases in OUT: vpk derives per-file deltas from the previous full package.
"$VPK" pack --skip-updates --packId Echo --packTitle Echo --packVersion "$VERSION" --packDir "$PACK" --mainExe "$MAIN_EXE" --channel "$CHANNEL" --delta BestSize --outputDir "$OUT" "${SIGN[@]}" "${PACK_FLAGS[@]}"
cargo run --locked -p echo_updater --bin echo-release -- sign "$KEY" "$OUT" "$CHANNEL" "$PLATFORM" "$OUTPUT_ROOT/manifest.json"
echo "Static release ready in $OUTPUT_ROOT. Upload artifacts first, manifest last. No deployment performed."
