# Release readiness

Echo is local-only. This checklist never authorizes publishing artifacts.

## Daily-use macOS build

Run `scripts/install-dogfood.sh`, then `open /Applications/Echo.app`.
Quit Echo before running `scripts/install-dogfood.sh --update`.
Both modes build Release against the qualified canonical CEF generation and
verify it before packaging. The script ad-hoc signs locally, stages in
`/Applications`, and replaces only an app carrying its own installer marker and
`dev.echo.browser` bundle identifier. It refuses symlinks and unmanaged installs.
It never changes the system default browser or launches Echo automatically.
Dogfood updates use this script, not Velopack replacement; update checks still
contact the authenticated endpoint and report errors gracefully.

Profiles are independent: installed dogfood uses
`~/Library/Application Support/Echo Dogfood`; debug builds use
`~/Library/Application Support/Echo Dev`; ordinary Release builds use
`~/Library/Application Support/Echo`. Existing legacy development data is not
moved into dogfood. `BROWSER_DATA_DIR` explicitly overrides any channel for tests.
Do not delete these directories to update the app.

Release and Developer ID signed builds use the real OS keychain; only unsigned
debug builds append `--use-mock-keychain`. Unsigned/ad-hoc builds can trigger keychain authorization
again after rebuilding; stable Developer ID signing is required for public use.
Never approve a different app's keychain access on Echo's behalf.

## Packaging gates

- [x] Echo icon: bundled multi-resolution `Echo.icns`, own artwork.
- [x] Bundle identifier: `dev.echo.browser`; HTTP/HTTPS handlers registered.
- [x] Version: `ECHO_RELEASE_VERSION` (default workspace version), build:
  numeric `ECHO_BUILD_NUMBER` (installer defaults to Git commit count).
  About displays both; plist has short version and build separately.
- [x] About links to installed Chromium credits and dependency acknowledgements.
  The latter includes Echo LICENSE, CEF, Ionicons, Gemoji and resolved Rust
  dependency notices. No font files are bundled; platform system fonts are used.
- [x] Local diagnostics: profile `Diagnostics/events.log` records starts/panic
  backtraces (bounded rotation); `heartbeat.txt` records UI liveness every ten
  seconds. CEF warnings are at `Chromium/cef.log`. Help → Troubleshooting →
  Copy Diagnostics copies version, OS, counts, heartbeat and diagnostics location;
  no browsing URLs, passwords or logs are uploaded. Reveal Echo Data opens files.
  For native crashes use macOS `~/Library/Logs/DiagnosticReports/Echo*.ips`;
  for a hang take an Activity Monitor sample before force quitting. A stale
  heartbeat is evidence of a blocked UI, not proof of the cause.
- [x] Update URL defaults to `https://updates.echo-browser.app/`. A missing
  `manifest.json` produces a recoverable “Couldn’t check for updates” state;
  no unsigned artifact or absent feed is accepted.
- [ ] Native acceptance: pristine onboarding, default-browser consent/cancel,
  launch/quit/relaunch, session and website storage preservation. Record exact
  results below; automated default-browser fixtures must not change the owner's
  actual default handler.

## Unsigned release dry run (no deployment)

Install pinned Velopack CLI:
`dotnet tool install vpk --version 1.2.161 --tool-path ~/.local/bin`.
Put that directory on PATH. The existing local update signing key must match
`config/update-public-key.txt`; do not regenerate production keys for testing.

```
OUT="$(mktemp -d /tmp/echo-release-0.1.0.XXXXXX)"
scripts/release.sh 0.1.0 "$OUT"
```

The offline verification command is
`cargo run --locked -p echo_updater --bin echo-release -- verify MANIFEST CHANNEL PLATFORM`.
It re-reads the produced manifest and validates its signed artifacts with the
client's embedded key.

Without `APPLE_DEVELOPER_ID`, the script explicitly reports that Developer ID
signing and notarization are skipped and output is for local testing. Manifest
creation calls the client's actual parser and verifies every full artifact with
the embedded public key before writing `manifest.json`. Keep the output local.
Never run `scripts/publish-release.sh` as part of a dry run.

## Public 0.1 blockers

Obtain an Apple Developer ID Application identity, Developer ID Installer identity
and notarization credentials/profile. Set `APPLE_DEVELOPER_ID`,
`APPLE_INSTALLER_ID`, `APPLE_NOTARY_PROFILE`; verify notarization, stapling and
Gatekeeper on a separate Mac. Public update-host deployment is a separate,
explicitly authorized operation. Windows/Linux runtime acceptance and installer
signing remain unverified; see `PLATFORMS.md`.

## Acceptance record

2026-10-25, macOS arm64:

- Release dogfood installed to `/Applications/Echo.app`; deep ad-hoc signature
  verification passed. Bundle identifier/version/build: `dev.echo.browser`,
  `0.1.0`, `388`. All ten standard icon resolutions validated.
- Pristine onboarding rendered; Theme/Favorites/Import progress persisted on
  relaunch. The real system default-browser consent prompt appeared. No consent
  to switch away from the existing default was given.
- **Blocked native acceptance:** upstream CEF requests the existing
  `Chromium Safe Storage` keychain item. SecurityAgent requires the owner's login
  keychain password. No credentials were read or entered. Leaving authorization
  unanswered causes CEF teardown to reach its watchdog (exit 2); this must not be
  reported as a clean quit. Real-keychain clean quit/relaunch and encrypted-cookie
  persistence remain unverified until the owner authorizes access locally.
  Explicitly labeled diagnostic comparisons using `--use-mock-keychain` completed
  pristine onboarding, quit and relaunched with the selected tab, cookie and
  localStorage retained; both exits were zero. The restored tab was `/check`
  (which only reads storage), so relaunch did not reseed the values. About and
  real-host 404 error windows were also inspected in this UI-only configuration.
  These observations do **not** qualify the real keychain. The flag is absent
  from the installed Release build's configuration.
- The owner should open the installed app, authorize its native keychain request
  locally, then complete onboarding. Never send that password to an agent.
  The daily profile was left pristine. Quit and wait for the process to exit
  before applying script updates.
- Both foreign checks pass: `x86_64-unknown-linux-gnu` and
  `x86_64-pc-windows-msvc` (CEF check-only, no runtime claim). The native bundle
  staging dependency on CEF is now declared on those platforms.
- Full Rust suite: **1,740 passed, zero failed, one existing ignored**. Installer
  safety and CEF adapter script tests: **11 passed**. A loopback 404 regression
  confirms a missing feed cannot pin an installable manifest. The real update
  endpoint returned 404 during inspection.
- Unsigned dry run: `/tmp/echo-release-0.1.0.j5WmuG/manifest.json`, with versioned
  portable ZIP, PKG and full NUPKG in its `macos-aarch64/` directory. No deployment
  ran. Production-key signature, expiry, channel/platform and full artifact
  validation passed independently with:

  ```
  cargo run --locked -p echo_updater --bin echo-release -- verify \
    /tmp/echo-release-0.1.0.j5WmuG/manifest.json stable macos-aarch64
  ```

Native evidence/logs: `~/Library/Caches/EchoBuildHealth/t3-71384893/dogfood-evidence/`;
`onboarding-1.png`, `persist-before-quit.png`, `security-66121.png`,
`security-66230.png`, `quit.sample.txt`, `mock-comparison.log`,
`about-2.png`, `update-404-2.png`, `mock-persist-before-quit.log` and
`mock-persist-after-relaunch.log` record the
observations and the authorization blocker. The unsigned packaging log is
`~/Library/Caches/EchoBuildHealth/t3-71384893/logs/dogfood-release-dry-run.log`.

