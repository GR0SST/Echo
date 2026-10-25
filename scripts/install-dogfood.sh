#!/usr/bin/env bash
# Local daily-use Release install. Never launches or publishes the app.
set -euo pipefail
cd "$(dirname "$0")/.."
[[ "$(uname -s)" == Darwin ]] || { echo 'Dogfood installer currently requires macOS.' >&2; exit 2; }
# Serialize builds/replacements without taking the shared screen.
if [[ "${ECHO_DOGFOOD_LOCKED:-}" != 1 ]]; then
  exec lockf -k /tmp/echo-dogfood-install.lock env ECHO_DOGFOOD_LOCKED=1 bash "$0" "$@"
fi
MODE="${1:-}"; [[ -z "$MODE" || "$MODE" == --update ]] || { echo 'Usage: scripts/install-dogfood.sh [--update]' >&2; exit 2; }
APP=/Applications/Echo.app
MARKER=Contents/Resources/echo-dogfood-install.txt
OWNER='echo-dogfood-installer-v1:dev.echo.browser'
# Refuse before compiling; a symlink or an unmanaged installation is never ours.
check_destination() {
  [[ ! -L "$APP" ]] || { echo 'Refusing symlink /Applications/Echo.app' >&2; exit 1; }
  if [[ -e "$APP" ]]; then
    [[ -f "$APP/$MARKER" && "$(cat "$APP/$MARKER")" == "$OWNER" && "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$APP/Contents/Info.plist")" == dev.echo.browser ]] || { echo 'Refusing to replace unmanaged /Applications/Echo.app. Move it aside yourself first.' >&2; exit 1; }
    if pgrep -f '^/Applications/Echo.app/Contents/MacOS/Echo( |$)' >/dev/null; then
      echo 'Quit the installed Echo before updating.' >&2; exit 1
    fi
  elif [[ "$MODE" == --update ]]; then
    echo 'No managed dogfood installation to update. Run without --update first.' >&2; exit 1
  fi
}
check_destination
export CARGO_PROFILE_RELEASE_DEBUG_ASSERTIONS=false
export CARGO_BUILD_JOBS=3
export ECHO_RELEASE_VERSION="${ECHO_RELEASE_VERSION:-0.1.0}"
export ECHO_UPDATE_CHANNEL="${ECHO_UPDATE_CHANNEL:-stable}"
export ECHO_BUILD_NUMBER="${ECHO_BUILD_NUMBER:-$(git rev-list --count HEAD)}"
ECHO_CEF_DIST="$(bun scripts/check-release-cef.ts "${ECHO_CEF_DIST:-$HOME/Documents/Bot/cef-src/echo-extensions-dist}")"
export ECHO_CEF_DIST CEF_PATH="$ECHO_CEF_DIST"
cargo run --locked -p bundle -- --release
cmp -s "$ECHO_CEF_DIST/Chromium Embedded Framework.framework/Chromium Embedded Framework" "target/bundle/Echo.app/Contents/Frameworks/Chromium Embedded Framework.framework/Chromium Embedded Framework" || { echo 'Bundled CEF differs from the qualified distribution.' >&2; exit 1; }
STAGE="$(mktemp -d /Applications/.echo-dogfood.XXXXXX)"
cleanup() {
  if [[ -e "$STAGE/previous.app" && ! -e "$APP" ]]; then
    echo "Previous install preserved at $STAGE/previous.app; restore it manually." >&2
  else
    rm -rf "$STAGE"
  fi
}
trap cleanup EXIT
/usr/bin/ditto target/bundle/Echo.app "$STAGE/Echo.app"
printf '%s\n' "$OWNER" > "$STAGE/Echo.app/$MARKER"
# Locally runnable only; public distribution requires Developer ID + notarization.
/usr/bin/codesign --force --deep --sign - "$STAGE/Echo.app"
/usr/bin/codesign --verify --deep --strict "$STAGE/Echo.app"
check_destination
if [[ -e "$APP" ]]; then mv "$APP" "$STAGE/previous.app"; fi
if ! mv "$STAGE/Echo.app" "$APP"; then
  [[ ! -e "$STAGE/previous.app" ]] || mv "$STAGE/previous.app" "$APP"
  exit 1
fi
echo 'Installed /Applications/Echo.app (local ad-hoc signing; no notarization).'
echo 'Open it with: open /Applications/Echo.app'
echo 'Daily profile: ~/Library/Application Support/Echo Dogfood; debug profile: ~/Library/Application Support/Echo Dev'
