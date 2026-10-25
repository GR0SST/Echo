#!/usr/bin/env bash
# Real Velopack replacement/restart using two Rust fixture builds and static HTTP.
set -euo pipefail
cd "$(dirname "$0")/.."
[[ "$(uname -s)" == Darwin ]] || { echo "macOS acceptance test"; exit 2; }
export CARGO_BUILD_JOBS=3 DOTNET_PROCESSOR_COUNT=3
VPK="${VPK:-vpk}"
ROOT="$(pwd)"
WORK="$(mktemp -d /tmp/echo-update-e2e.XXXXXX)"
SERVER_PID=""
trap '[[ -z "$SERVER_PID" ]] || kill "$SERVER_PID" 2>/dev/null || true; echo "Update evidence: $WORK"' EXIT
mkdir -p "$WORK/keys" "$WORK/releases" "$WORK/input"
cargo run --locked -p echo_updater --bin echo-release -- keygen "$WORK/keys/private" "$WORK/keys/public"
export ECHO_UPDATE_PUBLIC_KEY="$(cat "$WORK/keys/public")"
export ECHO_E2E_RESULT="$WORK/installed/restarted-version"
PLATFORM="macos-$(uname -m)"; PLATFORM="${PLATFORM/arm64/aarch64}"
# A separate HOME gives this disposable Echo package its own Velopack cache.
# The actual launching process uses /usr/bin/env so the parent shell is unchanged.
for VERSION in 0.1.0 0.2.0; do
  ECHO_PROBE_VERSION="$VERSION" cargo build --locked -p echo_updater --example update-probe
  cp target/debug/examples/update-probe "$WORK/input/Echo"
  # Use vpk-generated bundle and a unique bundle ID; no CEF/browser session needed.
  "$VPK" pack --skip-updates --packId Echo --packTitle EchoUpdaterFixture --packVersion "$VERSION" --packDir "$WORK/input" --mainExe Echo --bundleId dev.echo.updater.fixture --icon crates/bundle/assets/Echo.icns --channel stable --delta BestSize --outputDir "$WORK/releases" >"$WORK/pack-$VERSION.log" 2>&1
  if [[ "$VERSION" == 0.1.0 ]]; then
    OLD_ZIP="$(find "$WORK/releases" -maxdepth 1 -name '*Portable.zip' -print -quit)"
    [[ -n "$OLD_ZIP" ]] || OLD_ZIP="$(find "$WORK/releases" -maxdepth 1 -name '*.zip' -print -quit)"
    mkdir -p "$WORK/installed"
    ditto -x -k "$OLD_ZIP" "$WORK/installed"
  fi
done
cargo run --locked -p echo_updater --bin echo-release -- sign "$WORK/keys/private" "$WORK/releases" stable "$PLATFORM" "$WORK/host/manifest.json"
# Require actual deltas; both downloads and reconstructed full package are signed.
find "$WORK/releases" -name '*-delta.nupkg' | rg -q .
# Pick an ephemeral port and keep one static server for the entire installation.
cat > "$WORK/server.ts" <<'TS'
import { join } from "node:path";
const server = Bun.serve({ hostname: "127.0.0.1", port: 0, async fetch(request) {
  const url = new URL(request.url);
  console.log(`${request.method} ${url.pathname}${url.search}`);
  const name = decodeURIComponent(url.pathname);
  if (name.split("/").includes("..")) return new Response(null, { status: 400 });
  const file = Bun.file(join(Bun.argv[2], name));
  return await file.exists() ? new Response(file) : new Response(null, { status: 404 });
} });
await Bun.write(Bun.argv[3], String(server.port));
TS
bun "$WORK/server.ts" "$WORK/host" "$WORK/port" >"$WORK/http.log" 2>&1 &
SERVER_PID=$!
for _ in {1..50}; do [[ -f "$WORK/port" ]] && break; sleep 0.1; done
export ECHO_UPDATE_URL="http://127.0.0.1:$(cat "$WORK/port")/"
APP="$(find "$WORK/installed" -maxdepth 1 -name '*.app' -print -quit)"
# Preserve the previous full package in the isolated cache so the client uses delta.
mkdir -p "$WORK/home/Library/Caches/velopack/Echo/packages"
cp "$WORK/releases/Echo-0.1.0-stable-full.nupkg" "$WORK/home/Library/Caches/velopack/Echo/packages/"
env HOME="$WORK/home" "$APP/Contents/MacOS/Echo" >"$WORK/client.log" 2>&1
for _ in {1..60}; do [[ -f "$ECHO_E2E_RESULT" ]] && break; sleep 1; done
[[ "$(cat "$ECHO_E2E_RESULT")" == 0.2.0 ]]
[[ ! -e "$WORK/home/Library/Caches/velopack/Echo/packages/.betaId" ]]
rg -- '-delta.nupkg' "$WORK/http.log"
! rg -i '\?(.*(id|user)|.*version)|cookie:' "$WORK/http.log"
echo "PASS: authenticated old build upgraded by delta and restarted as 0.2.0."
