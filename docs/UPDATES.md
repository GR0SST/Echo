# Echo updates

Echo owns this channel. The server is static files only: no application server,
accounts, telemetry, staged rollout IDs, cookies, or user/device identifiers.
The server/CDN necessarily sees ordinary connection metadata (IP address).
Disable request/access logs and Cloudflare analytics where supported by the host;
never add an identifier or current-version query parameter.

## Choice

Use **Velopack 1.2.161**, with its Rust client and native platform installers.
Its [per-file Zstandard delta updates](https://docs.velopack.io/packaging/deltas)
apply on macOS, Windows and Linux, important for the approximately 250 MB CEF
payload. Preserve previous full packages to generate deltas. A changed CEF can
still produce a large patch; an unchanged framework should cost very little.
The client falls back to a signed full package if a delta cannot be reconstructed. A reconstructed package
that fails Echo's additional content proof is rejected; it is never installed.

[Sparkle](https://sparkle-project.github.io/documentation/) has mature macOS
integration and Ed25519 signing, but would need a separate Windows integration
([WinSparkle](https://github.com/vslavik/winsparkle)) and another Linux updater.
Velopack keeps one Rust integration and a single packaging/feed format.
We do not use Velopack Flow, GitHub releases or its built-in HTTP source: the
latter includes staged-user query parameters. `SignedSource` ignores that local
SDK argument and makes only fixed anonymous file requests. Echo also selects the
release/delta chain locally instead of calling SDK `check_for_updates`, which
would create a persistent `.betaId` even with a custom source. No rollout ID is
created. Delta bases must pass the signed package-content proof first.

## Static layout and configuration

One build-time constant, `DEFAULT_URL`, defaults to
`https://updates.echo-browser.app/`. `ECHO_UPDATE_URL` overrides it at build time
and, if present in the launched process, at runtime. A fallback deployment can
build with `ECHO_UPDATE_URL=https://echo-browser.app/updates/` without code changes.
The client fetches `manifest.json` relative to this base. That single signed catalog
contains platform entries keyed by Rust's OS/architecture (`macos-aarch64`,
`macos-x86_64`, `windows-x86_64`, `linux-x86_64`). It selects locally, then requests
signed packages under `<platform>/<filename>`. No platform/version/user query
parameters are sent. `ECHO_UPDATE_CHANNEL` selects a build's channel (default
`stable`); installed channel, catalog and build must agree. Stable rejects
prereleases and all channels reject downgrades/lateral moves.

`release.sh VERSION OUTPUT_ROOT [CHANNEL]` writes a plain publishable directory:

```
OUTPUT_ROOT/
  manifest.json
  macos-aarch64/
    Echo-<version>-stable-full.nupkg
    Echo-<version>-stable-delta.nupkg
    Echo-stable-Setup-<version>.pkg
    Echo-stable-Portable-<version>.zip
```

Use a dedicated persistent OUTPUT_ROOT for each channel. The exporter merges
platform entries into the authenticated root catalog and refuses to change bytes
at an existing artifact path. Preserve other platforms' artifacts/catalog when
building on separate hosts. Native Velopack output and its mutable feed/installer
names stay in a separate build cache (`target/update-packages/<channel>/<platform>`,
override `ECHO_RELEASE_CACHE`). Only signed artifacts are copied to OUTPUT_ROOT;
installers/portable archives get versioned names. No symlinks, raw Velopack feeds,
keys, or build metadata are exported. Upload through the atomic publisher below.

## Authentication

`config/update-public-key.txt` embeds the release Ed25519 public key. A build-only
`ECHO_UPDATE_PUBLIC_KEY` override exists for isolated acceptance fixtures; there
is deliberately no runtime public-key override.

The JSON envelope contains `payload` (base64 of the exact manifest JSON bytes)
and `signature` (base64 Ed25519 signature of those bytes). Authenticate before
parsing the payload. Schema 1 binds the channel and complete platform map. Each platform entry binds
platform, expiry, the complete Velopack feed, and every artifact signature. Reject unsafe/duplicate filenames,
unsigned packages, wrong channel/platform, malformed versions, and expired feeds.
Manifests last one year; re-sign before expiry even if no version is released.

Artifact signatures cover this domain-separated UTF-8 statement:

```
echo-artifact-v1\n<lowercase SHA-256 of artifact bytes>\n<byte length>\n
```

Hashing streams bounded buffers instead of loading CEF into memory. The client
verifies size/hash/signature before returning a download to Velopack, verifies the
reconstructed/cached full package before showing Ready, and checks it again before
installation. Velopack reconstructs ZIP compression bytes differently, so full
packages also carry a manifest-authenticated canonical content digest: sorted
UTF-8 entry names, Unix modes, uncompressed lengths and SHA-256 file hashes,
with length-prefixed fields under `echo-package-v1`. This covers executable and
symlink payloads as well as package metadata. Extra/duplicate/unsafe paths are
rejected. The final content proof is checked even when Velopack skips a cached
package or reconstructs it from deltas; raw full/delta downloads still require
their artifact signatures. Velopack's checksum alone is not the trust boundary. HTTPS is
mandatory; only debug builds allow loopback HTTP for acceptance tests. Redirects,
URL credentials, query strings and fragments are rejected.

## Key custody (do this before distributing a build)

A release signing key was generated **once** on this machine into
`~/.config/echo/update-signing.key` (32 raw bytes, mode 0600, parent mode 0700).
Only its public key is committed. The private key is not in release output or git.
`ECHO_SIGNING_KEY=/absolute/outside/repository/path` selects another secure copy.
Do not generate a different production key after shipping: existing clients trust
the original public key. Key rotation requires an explicit signed migration.

For a new, unshipped channel, `scripts/update-keygen.sh` creates the private file
exclusively and refuses to overwrite it. It writes the public key to config.
Back up the existing private file to an encrypted offline volume/password-manager
attachment, verify a restore by signing and locally verifying a fixture, and keep
a second recovery copy under the owner's control. Never put the key on the VPS,
in a shell argument, chat, CI logs, release output, or repository. A dedicated
release machine or a CI secret mounted as a 0600 file can hold the signing copy.
Loss of the only key prevents authentic updates to installed clients.

## Release

Prerequisites: Bun, Rust, .NET 10 runtime/SDK, `vpk` 1.2.161:

```sh
dotnet tool install vpk --version 1.2.161 --tool-path ~/.local/bin
export PATH="$HOME/.local/bin:$PATH"
# Homebrew .NET may need this for generated tool launchers:
export DOTNET_ROOT="$(brew --prefix dotnet)/libexec"
export ECHO_UPDATE_URL=https://updates.echo-browser.app/
export APPLE_DEVELOPER_ID='Developer ID Application: OWNER (TEAMID)'
export APPLE_INSTALLER_ID='Developer ID Installer: OWNER (TEAMID)'
export APPLE_NOTARY_PROFILE=echo-notary
scripts/release.sh 0.2.0 /absolute/path/to/echo-releases stable
```

Store notarytool credentials separately using `xcrun notarytool store-credentials
 echo-notary`; the script passes only the profile name. Missing Developer ID
causes an explicit test-only signing/notarization skip. Without notarization,
output is not ready for public Gatekeeper acceptance. Velopack handles nested
codesigning, package signing, notarization and stapling using the provided IDs.
CEF needs the supplied JIT/hardened-runtime entitlements.

The script requires the qualified canonical CEF distribution and verifies the
framework against its `build.json` and `qualification.json` receipt before building.
It does not build or mutate CEF. It builds optimized browser/helper executables,
assembles the bundle, packages with `--delta BestSize`, and signs static output.
First release has no delta; subsequent releases preserve the Velopack cache and
OUTPUT_ROOT. Use a new semantic version for each build.
The native bundle adapter stages Windows/Linux browser/helper executables and
CEF resources; Velopack produces their native installer/AppImage and delta files.
Run `release.sh` on each target OS (Git Bash on Windows), with `ECHO_CEF_DIST`
pointing to its qualified native distribution. Native qualification receipts use
`framework_sha256` for the platform's engine image (`libcef.dll`/`libcef.so`).
Only macOS installer execution was tested in this task; Windows/Linux installer
and native CEF qualification still need those hosts before public distribution.

## Client lifecycle and Arc reference

First background check occurs 30 seconds after launch, then every six hours.
Only one check/download may run. “Check for Updates…” in Echo's app menu opens a
GPUI status window; Ready presents “Restart and Update” above the Space dock.
No update is applied implicitly at startup or during ordinary quit. Downloads
(including paused/queued transfers and retiring owners) block restart; the user
can retry once they finish. Update fetching itself must also have finished.

The restart action requests the existing quit coordinator. Session/profile save,
CEF/cookie shutdown, final download drain, persistence and owner release all
precede installer arming. Any recovery/shutdown failure leaves installation
unarmed. Then Velopack waits for the parent process to exit before swapping the
bundle and relaunching. Installation failure leaves Velopack's diagnostic logs;
Echo does not bypass its normal recovery startup.

Installed Arc 1.170.0 was inspected without a debugger on 2026-10-25: its app menu
reads “Check for Updates…”; the 270 × 350 status window shows product/version,
“Your browser is up to date”, and a 170 × 48 Close button. Echo uses its own icon.
Arc's available-update sidebar banner was absent in the current installation.
“Restart and Update” was confirmed in bundle strings; the banner's exact colors,
spacing and animation remain **unqualified** pending a live reference. Do not
claim full update visual parity. The auto-update setting is also still missing.

## Verification

```sh
CARGO_BUILD_JOBS=3 RUST_TEST_THREADS=4 cargo test
VPK=/absolute/path/to/vpk scripts/test-updates-macos.sh
cargo run -p bundle
TMPDIR="$(mktemp -d /tmp/echo-updater-drive.XXXXXX)/" \
  scripts/drive.sh 'wait 3; shot /tmp/echo-updater.png; quit'
```

Unit tests cover signed manifest parsing, semantic versions, channels/platforms,
expiry, malformed paths, tampered artifacts/manifests and anonymous HTTP requests.
GPUI tests exercise the real app-menu action, dialog close, banner hit testing
and refusal to quit without a verified package. The macOS acceptance script builds
old/new Rust fixture executables (no browser profile or CEF), packages them with
real Velopack, signs an ephemeral channel, serves local static HTTP, installs a
delta over the old build and proves the new executable restarts. It retains logs
under its printed temporary evidence directory. This tests the actual native
installer, not just a mocked fetch; browser shutdown is separately covered by the
existing quit/recovery suite and the final barrier integration.

## Publishing (no VPS operations in this implementation task)

The static hosting infrastructure, server configuration, TLS/cache notes,
provisioning, deploy-user setup and rollback documentation live in the private
repository [GR0SST/echo-update-host](https://github.com/GR0SST/echo-update-host).
Production base URL: `https://updates.echo-browser.app/`.
Manifest URL: `https://updates.echo-browser.app/manifest.json`.
The isolated smoke host path is `https://updates.echo-browser.app/test/manifest.json`;
never use its test key as the production trust root.

1. The host agent/owner follows that private runbook to configure DNS, Cloudflare
   Full (strict) TLS, static hosting and the `echo-updates-deploy` SSH alias.
   There is no application server or signing key on the host.
2. Build with the production key and configured Apple identities:
   `scripts/release.sh 0.2.0 /absolute/path/to/echo-releases stable`.
3. Publish the plain output directory:
   `scripts/publish-release.sh /absolute/path/to/echo-releases`.
   The existing publisher stages all files before switching the root manifest
   generation atomically. `ECHO_UPDATE_KEEP` defaults to 5 (minimum 2); the host
   retains at least seven days. Only root `manifest.json` and the optional root
   detached-signature names are mutable; every artifact path must stay immutable.
4. Fetch `https://updates.echo-browser.app/manifest.json`, verify its envelope
   with the embedded public key, check each platform's files/signatures and
   Cache-Control, then test an older disposable installation against HTTPS.
   This implementation's acceptance test used only loopback HTTP; it did not
   connect to the VPS or production update host.
5. For isolated smoke fixtures, build with `ECHO_UPDATE_CHANNEL=test` and
   `ECHO_UPDATE_URL=https://updates.echo-browser.app/test/`, then set
   `ECHO_UPDATE_CHANNEL=test` for the publisher. The test key is separate.
   If fallback hosting is selected, use the same static tree and build/runtime
   base override `ECHO_UPDATE_URL=https://echo-browser.app/updates/`.
