#!/usr/bin/env bash
# Build the drag-to-Applications DMG from a release's signed Portable zip.
# The app inside is the Velopack-packaged bundle, so it can update itself.
# Usage: scripts/make-dmg.sh VERSION RELEASE_ROOT [OUTPUT.dmg]
set -euo pipefail
cd "$(dirname "$0")/.."
[[ "$(uname -s)" == Darwin ]] || { echo "macOS only" >&2; exit 2; }
VERSION="${1:?Usage: scripts/make-dmg.sh VERSION RELEASE_ROOT [OUTPUT.dmg]}"
ROOT="${2:?Release root required}"
CHANNEL="${ECHO_UPDATE_CHANNEL:-stable}"
ARCH="$(uname -m)"; ARCH="${ARCH/arm64/aarch64}"
PLATFORM="macos-$ARCH"
ZIP="$ROOT/$PLATFORM/Echo-$CHANNEL-Portable-$VERSION.zip"
# Outside RELEASE_ROOT: publish-release.sh uploads that whole tree.
OUT="${3:-target/dmg/Echo-$VERSION-$ARCH.dmg}"
[[ -f "$ZIP" ]] || { echo "Missing $ZIP: run scripts/release.sh first." >&2; exit 1; }
[[ ! -e "$OUT" ]] || { echo "$OUT already exists" >&2; exit 1; }
BG=crates/bundle/assets/dmg-background.png
BG2=crates/bundle/assets/dmg-background@2x.png
VOLUME="Echo"

WORK="$(mktemp -d "${TMPDIR:-/tmp}/echo-dmg.XXXXXX")"
trap '[[ -z "${DEVICE:-}" ]] || hdiutil detach "$DEVICE" -quiet -force || true; rm -rf "$WORK"' EXIT
mkdir -p "$WORK/src/.background" "$(dirname "$OUT")"
ditto -x -k "$ZIP" "$WORK/src"
[[ -d "$WORK/src/Echo.app" ]] || { echo "Portable zip has no Echo.app" >&2; exit 1; }
ln -s /Applications "$WORK/src/Applications"
# Combine both resolutions so Finder picks the Retina one.
tiffutil -cathidpicheck "$BG" "$BG2" -out "$WORK/src/.background/background.tiff" >/dev/null

# Writable image for layout, then compress.
SIZE_MB=$(( $(du -sm "$WORK/src" | cut -f1) + 64 ))
hdiutil create -quiet -srcfolder "$WORK/src" -volname "$VOLUME" -fs HFS+ \
  -format UDRW -size "${SIZE_MB}m" "$WORK/rw.dmg"
# Another mounted "Echo" volume would get a suffixed name and break the layout.
[[ ! -d "/Volumes/$VOLUME" ]] || { echo "/Volumes/$VOLUME is already mounted" >&2; exit 1; }
DEVICE="$(hdiutil attach -readwrite -noverify -noautoopen "$WORK/rw.dmg" | awk '/Apple_HFS/ {print $1}')"
sleep 2
osascript <<OSA
tell application "Finder"
  tell disk "$VOLUME"
    open
    set current view of container window to icon view
    set toolbar visible of container window to false
    set statusbar visible of container window to false
    # 960 wide: Finder keeps windows at least ~920 pt wide on recent macOS.
    set the bounds of container window to {200, 120, 1160, 660}
    set viewOptions to the icon view options of container window
    set arrangement of viewOptions to not arranged
    set icon size of viewOptions to 128
    set text size of viewOptions to 13
    set background picture of viewOptions to file ".background:background.tiff"
    set position of item "Echo.app" of container window to {300, 270}
    set position of item "Applications" of container window to {660, 270}
    set extension hidden of item "Echo.app" to true
    close
    open
    update without registering applications
    delay 2
    close
  end tell
end tell
OSA
cp crates/bundle/assets/Echo.icns "/Volumes/$VOLUME/.VolumeIcon.icns"
SetFile -a C "/Volumes/$VOLUME" 2>/dev/null || true
chmod -Rf go-w "/Volumes/$VOLUME" || true
sync
hdiutil detach "$DEVICE" -quiet
DEVICE=""
hdiutil convert -quiet "$WORK/rw.dmg" -format ULFO -o "$OUT"

if [[ -n "${APPLE_DEVELOPER_ID:-}" ]]; then
  codesign --force --timestamp --sign "$APPLE_DEVELOPER_ID" "$OUT"
  if [[ -n "${APPLE_NOTARY_PROFILE:-}" ]]; then
    xcrun notarytool submit "$OUT" --keychain-profile "$APPLE_NOTARY_PROFILE" --wait
    xcrun stapler staple "$OUT"
    spctl -a -t open --context context:primary-signature -v "$OUT"
  fi
else
  echo "DMG unsigned: APPLE_DEVELOPER_ID is not configured."
fi
echo "DMG ready: $OUT"
