# Local CEF 154 build

This recipe builds the exact CEF used by Echo with H.264/AAC enabled. Sources,
toolchain downloads, intermediate objects and distributions live outside Echo.
`cargo run -p bundle` automatically prefers
`~/Documents/Bot/cef-src/echo-extensions-dist` on macOS arm64 when present.
`scripts/with-cef.sh cargo test` uses the same automatic selection. Missing local
distributions fall back to the original prebuilt via Cargo; CI and other hosts
retain that behavior. An existing invalid local distribution fails with a clear
error rather than silently losing its features. `ECHO_CEF_DIST=/path` overrides
the selection, and an explicit `CEF_PATH` overrides automatic detection.
`scripts/drive.sh` uses the previously assembled bundle; run `cargo run -p bundle`
after changing or removing a distribution. Direct `cargo test` keeps Cargo's
portable prebuilt default unless wrapped or given an explicit `CEF_PATH`.

## Pins and machine

- Rust: `cef = 154.5.0` (`cef-dll-sys 154.5.0+154.0.34`).
- CEF: `154.0.34+g14c5a08+chromium-154.0.8037.98`, commit
  `14c5a089e8452874cb1a556dc3fde26ed2d87723`, branch `8037`.
- Chromium: `154.0.8037.98`, commit
  `b859317bf11f6be47f9b7799ec690a0a42a1fb33`.
- Official `automate-git.py`: commit
  `e537569ecda2e3f23ffdac559ecf7f80b8e59467`, SHA-256
  `94d3ddd1e0d35482132f45dc101a593c9371b6eb2d8ff4af6cfe2734d2e48c6a`.
- Build host: Apple M2 Max, 64 GB RAM, macOS 27.0.1 (`26A434`) arm64, Xcode 27.1
  (`27A9275`), Python 3.12.4. Initial available disk: 281 GiB.
- Local source root: `/Users/grosstgrosst/Documents/Bot/cef-src`.
- Existing tooling:
  `/Users/grosstgrosst/Documents/Bot/ChromiumFork/depot_tools`.
  Its revision is `b1c2e86792a7f2b6576c181b7327ddd53b1a42a8`.
  `DEPOT_TOOLS_UPDATE=0` and `--no-depot-tools-update` prevent self-update.
- Build tools: Ninja `1.12.1`; Chromium Clang `24.0.0git`
  (`9fca3cf47d011a0af295d4252d70f16f8693d6a2`), targeting
  `arm64-apple-darwin27.0.0`; Bun `1.4.3` for Echo's adapter/checker scripts.

Upstream describes [automated macOS arm64 builds](https://github.com/chromiumembedded/cef/blob/e537569ecda2e3f23ffdac559ecf7f80b8e59467/docs/automated_build_setup.md)
and the [automate tool](https://github.com/chromiumembedded/cef/blob/e537569ecda2e3f23ffdac559ecf7f80b8e59467/tools/automate/automate-git.py).
The script pins and verifies the tool instead of downloading a moving master.

## Build

Run from Echo. Xcode and its command-line tools must already be installed.
Use Bun for the distribution adapter; Python runs CEF's official tooling.

```sh
scripts/build-cef.sh all

# Equivalent resumable stages (run sequentially):
scripts/build-cef.sh sync
scripts/build-cef.sh build
scripts/build-cef.sh distrib
```

`ECHO_CEF_SOURCE` overrides the external source root; `ECHO_DEPOT_TOOLS` overrides
the existing tooling path. A shared `/tmp/echo-cef-build.lock` serializes all
stages across Echo worktrees. The entire operation runs under `nice -n 10`;
the sole Chromium compile command uses `ninja -j 8`. No Debug build is generated.
No custom Echo CEF/download patches are applied in the codecs phase.

Exact environment:

```sh
export DEPOT_TOOLS_UPDATE=0
export CEF_ARCHIVE_FORMAT=tar.bz2
export GN_OUT_CONFIGS=Release_GN_arm64
export GN_DEFINES='proprietary_codecs=true ffmpeg_branding=Chrome is_official_build=true symbol_level=0 use_thin_lto=false chrome_pgo_phase=0 use_remoteexec=false use_siso=false use_system_xcode=true'
```

`GN_OUT_CONFIGS` limits generation to Release arm64. `symbol_level=0` limits
disk consumption; disabling ThinLTO and PGO avoids their memory and profile
requirements. `is_official_build=true` still selects an optimized release.
These arguments are added to CEF's platform defaults; the generated
`chromium/src/out/Release_GN_arm64/args.gn` records the complete configuration.

Generated configuration for this build:

```gn
alternate_cdm_storage_id_key="968b476909da4373b08903c28e859454"
chrome_pgo_phase=0
clang_use_chrome_plugins=false
disable_fieldtrial_testing_config=true
enable_background_mode=false
enable_cdm_host_verification=true
enable_cdm_storage_id=true
enable_downgrade_processing=false
enable_precompiled_headers=false
enable_resource_allowlist_generation=false
enable_rlz=true
enable_widevine=true
ffmpeg_branding="Chrome"
forbid_non_component_debug_builds=false
is_component_build=false
is_debug=false
is_official_build=true
optimize_webui=true
proprietary_codecs=true
symbol_level=0
target_cpu="arm64"
use_remoteexec=false
use_siso=false
use_system_xcode=true
use_thin_lto=false
```

The core official-tool commands, expanded for this host:

```sh
src_root=/Users/grosstgrosst/Documents/Bot/cef-src
depot=/Users/grosstgrosst/Documents/Bot/ChromiumFork/depot_tools
export PATH="$depot:$PATH"
nice -n 10 python3 "$src_root/automate/automate-git.py" \
  --download-dir="$src_root" --depot-tools-dir="$depot" \
  --no-depot-tools-update --branch=8037 \
  --checkout=14c5a089e8452874cb1a556dc3fde26ed2d87723 \
  --no-chromium-history --arm64-build --no-debug-build --no-build --no-distrib
cd "$src_root/chromium/src/cef"
nice -n 10 python3 tools/gclient_hook.py
nice -n 10 ninja -C ../out/Release_GN_arm64 -j 8 cefclient
cd "$src_root/chromium/src/cef/tools"
nice -n 10 python3 make_distrib.py \
  --output-dir="$src_root/chromium/src/cef/binary_distrib" \
  --allow-partial --ninja-build --arm64-build --minimal --no-symbols --no-docs
```

Prefer the script: it also acquires the build lock, records logs/elapsed time,
checks source pins, and stages the distribution for cef-rs. It never edits
the repository's Chromium158 checkout or `depot_tools`.

The packager is invoked directly because `automate-git.py`'s
`--no-distrib-symbols` does not forward `--no-symbols` for its minimal package;
without the explicit flag it also generates a separate release-symbol archive.

## Select the local distribution

Official minimal distributions put the framework in `Release/`. The cef-rs
downloader normally flattens that directory and writes `archive.json`.
`scripts/cef-dist.ts stage` creates the equivalent layout with links to our
own distribution and metadata containing the local archive's actual SHA-1.
The archive and distribution must remain in place while these links are used.

```sh
export ECHO_CEF_DIST=/Users/grosstgrosst/Documents/Bot/cef-src/echo-dist
scripts/with-cef.sh cargo test
scripts/with-cef.sh cargo run -p bundle
```

The wrapper validates the exact CEF version, metadata and required paths, then
exports an absolute `CEF_PATH` for Cargo and its child bundle commands.
Selection errors stop the command. Without `ECHO_CEF_DIST`, the wrapper and
bundle utility prefer `~/Documents/Bot/cef-src/echo-extensions-dist` when present
on macOS arm64, then fall back to Cargo’s prebuilt configuration. Direct
`CEF_PATH=/absolute/path` remains supported by cef-rs.

Expected minimal artifact:

```text
/Users/grosstgrosst/Documents/Bot/cef-src/chromium/src/cef/binary_distrib/
  cef_binary_154.0.34+g14c5a08+chromium-154.0.8037.98_macosarm64_minimal.tar.bz2
```

Do not commit binaries, source checkouts or intermediate outputs to Echo.

## Qualification record

Source preparation started 2026-10-25 at approximately 05:08 UTC and completed
successfully at approximately 06:05 UTC. The build recipe started at 06:05:23
UTC and succeeded after **11,705 seconds (3 h 15 m 5 s)**, including translation,
standard patching, GN generation and all **59,116 Ninja steps**. Generation
excluded Debug. All 28 versioned API hashes matched; 115 standard CEF patches
applied, one was skipped, and none failed.

The corrected minimal-only package/staging run succeeded in **67 seconds**.
Its archive is about **131 MiB**, and the framework executable is about
**244 MiB**. Archive SHA-1:
`06f341d7b28cea525b0b36384190f4aa538bb8aa`.
The whole external CEF tree, including sources, tools, intermediates,
distributions, archives and diagnostic logs, occupies **41,641,444 KiB
(39.71 GiB)** after packaging. This is a direct `du -sk` measurement; changes
in free space from other concurrent work are not attributed to this build.

Qualification passed on 2026-10-25:

- `bun test scripts/cef-dist.test.ts`: four tests passed.
- Full `cargo test` with the custom distribution: **1,594 passed, zero failed,
  one existing ignored test**. The cef-dll-sys build output explicitly records
  `CEF_DIR=/Users/grosstgrosst/Documents/Bot/cef-src/echo-dist`.
- `scripts/with-cef.sh cargo run -p bundle`: succeeded with the same selection.
- Final locked native check: passed, exit code zero. Video advanced from
  `0.021333` to `4.020019` seconds, with **96 rendered frames**, 854×480 video,
  no media error, unmuted playback, and **68,663 decoded audio bytes**.
  Web Audio independently decoded **2,492,416 stereo samples at 48 kHz**.
- The native screenshot was visually inspected and shows the playing movie.
  The automation log reports an audible tab and orderly CEF teardown through
  `CEF Complete`, with no remaining browsers or shutdown failures.

The bundled framework's SHA-256 matched the selected distribution:
`ebed276f21b42992d541c42bf0b75b5f73e0c09d5855f1665dd2803c75d15976`.
Native evidence lives in
`/Users/grosstgrosst/Documents/Bot/cef-src/qualification/2026-10-25T09-34-59.173Z/`:
`result.json`, `native.log` and `playback.png`. Full Rust qualification logs are
`cef-src/cargo-test.log` and `cef-src/bundle.log`.
An extension bridge has not been implemented yet.

Each script stage records UTC start time, elapsed seconds, exit status, total
source-root disk usage (`du -sk`) and remaining filesystem capacity (`df -k`) in
`cef-src/logs/`. The initial manual bootstrap sync log is `cef-src/sync.log`.

The native gate must run under `lockf -k /tmp/echo-screen.lock` with a hard
timeout, using an isolated Echo profile. Download a public H.264/AAC MP4,
inspect its tracks with `ffprobe`, then verify actual advancing playback,
decoded video frames and audio decoding in the bundled Echo. A successful
`canPlayType()` result alone is insufficient. Inspect the screenshot yourself.

```sh
src_root=/Users/grosstgrosst/Documents/Bot/cef-src
mkdir -p "$src_root/fixtures"
curl -fL --retry 3 https://media.w3.org/2010/05/sintel/trailer.mp4 \
  -o "$src_root/fixtures/sintel-trailer.mp4"
ffprobe -v error -show_entries stream=codec_name,codec_type,profile,width,height,sample_rate \
  -of json "$src_root/fixtures/sintel-trailer.mp4"
# Run only after custom-distribution cargo test and bundling have passed.
ECHO_CEF_DIST="$src_root/echo-dist" bun scripts/check-cef-codecs.ts
```

The fixture has H.264 High, 854×480 video and AAC-LC, 48 kHz audio.
Its SHA-256 is
`b670602fa00934ca27c4351bb0efe7ea7a07fae57284e44226025eeed7c51254`.
The native checker acquires the shared screen lock, copies the bundle to its
own temporary directory and uses its own profile. It verifies the bundled
framework's SHA-256 against the selected custom distribution, navigates through
the native Command Bar, checks advancing video frames, decodes the MP4 audio
track through Web Audio and checks nonzero samples, then captures the native
window stack and quits. Playback is unmuted at 5% volume. The diagnostic-only
`--autoplay-policy=no-user-gesture-required` flag avoids conflating autoplay
policy with codec support; it is never added to production initialization.
A 90-second watchdog kills its own app if the complete locked check hangs.
Evidence is written below `cef-src/qualification/<UTC timestamp>/`. Inspect
`playback.png`, `native.log` and `result.json`; the script's success alone is
not a substitute for viewing the screenshot. It retains its private temporary
copy/profile for diagnosis and never touches a live user profile.

## Extension bridge gate

Only after the codecs distribution passes the qualification above, implement
the approved bounded source experiment from `docs/CEF154_EXTENSIONS.md`
(commit `a44d0ab`). It must open the real MV3 action popup for an Alloy page,
without a Chrome toolbar, and preserve isolation from a second profile.
Stop if this requires a broad browser-window rewrite. Catalog/enable/disable,
verified Web Store installation and the engine wrapper come after that gate;
GPUI extension UI is a separate task.

## Codec licensing

This build is intended for the owner's private, personal use. Public
distribution requires an explicit licensing decision before shipping. Enabling
the GN flags does not grant codec patent licenses: Google's
[CEF build guidance](https://support.google.com/webdesigner/answer/10043691?hl=en-GB)
states that the additional codecs require licensing agreements and fees to
codec patent holders. Preserve CEF/Chromium/FFmpeg licenses and third-party
notices; this recipe is not approval to redistribute the resulting binaries.

## Opt-in extension bridge

`patches/cef154/extensions-bridge.patch` is a separate, experimental source
patch on the same pinned CEF/Chromium revisions. It adds an additive C ABI;
stock `cef` Rust bindings and their API hashes stay unchanged. The build script
requires explicit opt-in and verifies either a clean application or the exact
already-applied patch. It refuses a codec-only build from a patched checkout.

```sh
ECHO_CEF_EXTENSIONS_BRIDGE=1 scripts/build-cef.sh build
ECHO_CEF_EXTENSIONS_BRIDGE=1 scripts/build-cef.sh distrib
ECHO_CEF_DIST="$HOME/Documents/Bot/cef-src/echo-extensions-dist" \
  scripts/with-cef.sh cargo test
ECHO_CEF_DIST="$HOME/Documents/Bot/cef-src/echo-extensions-dist" \
  scripts/with-cef.sh cargo run -p bundle
ECHO_CEF_EXTENSIONS_BRIDGE=1 target/bundle/Echo.app/Contents/MacOS/Echo
```

The experimental distribution goes to `cef-src/bridge-binary-distrib` and its
adapter to `cef-src/echo-extensions-dist`. The qualified codec-only archive and
`cef-src/echo-dist` remain separate. All distributions stay outside Git.

`bun scripts/check-cef-extensions.ts` runs the source-built `cefclient` native
harness under the shared screen lock and a 90-second hard timeout. It uses
fresh canonicalized private profile paths, two actual Alloy pages and no Chrome
toolbar. Its fixture calls `chrome.extension.getViews({type:"popup"})` and
`chrome.storage.local` inside a real `ExtensionViewHost`. It checks registry,
action and close isolation, then profile catalog callbacks and enable/disable
results. Screenshots use WindowServer capture because bitmap-caching an NSView
omits Chromium GPU layers.

The final popup gate passed on 2026-10-25 at 10:36 UTC. Evidence:
`cef-src/qualification/extensions-2026-10-25T10-36-09.454Z/` (result, native log,
and visually reviewed popup screenshot). The fixture appeared over the Alloy
page; popup count was 1, profile A saw its extension, profile B did not; invoking
from B returned NOT_FOUND and closing from B returned WRONG_OWNER. Both browsers
reported Alloy and the process exited cleanly. This establishes a narrow native
host, not full extension API or Arc UI parity.

The Rust engine interface is `Engine::extension_bridge(view)`. A bridge binds to
the actual live Alloy page, reports owned JSON events, exposes enable/disable
and Web Store installation permission continuations, and cancels installations
when the originating page navigates, closes or its watcher is dropped. Native
popup attachment is an explicitly unsafe embedding boundary: detach the borrowed
view synchronously on CLOSED; do not retain it or reenter CEF from callbacks.
Host the borrowed popup view in a client-owned child window above the page, as
Echo does for overlays. Attaching below CEF's managed page view has unstable
stacking; the final fixture uses a child NSPanel with no Chrome toolbar.
Drop bridges/popups before engine shutdown. Stock distributions return an
unavailable error without introducing a required native symbol.

Cancellation is ordered against a verified install's commit using an optional
shared atomic request state. Before commit it prevents the CRX installer from
installing assets. Once commit starts, cancellation returns UNSUPPORTED rather
than claiming rollback. Approved downloads retain the originating profile until
the underlying Chromium installer actually finishes. Ordinary Chromium installs
leave this guard unset. Only browser-initiated EXTENSION_INSTALLER downloads
bypass CEF's generic Save As callbacks; Chromium's ordinary download target and
CRX path still handle them.

The installation path subclasses Chromium's `WebstoreStandaloneInstaller`.
A small virtual prompt-callback hook preserves the ordinary metadata fetch,
policy, approval, download and signed-CRX verification flow; no verification or
provenance flag is relaxed. Permission messages and their detailed warnings are
Chromium-generated. Approval is a separate, owner-validated continuation;
there is no automatic approval in Echo. The disposable native test can explicitly
approve an installation when `ECHO_CEF_TEST_STORE_ID` is supplied. Supervised,
parental and withheld-permission prompts fail closed instead of being treated
as ordinary approval.

Limitations: native popup attachment is currently macOS only; other platforms
return UNSUPPORTED. Full Chrome window/currentWindow semantics, external popup
navigation, eyedropper, action/icon/badge live-update coverage, command dispatch,
uninstall and the GPUI extension surfaces are not implemented by this slice.
The catalog includes installed packaged/unpacked extensions and manifest data;
this is not yet the complete metadata contract proposed in CEF154_EXTENSIONS.md.

Reproduce the management/store qualifications (each invocation owns new private
profiles and takes the screen lock):

```sh
ECHO_CEF_TEST_POPUP_ONLY=1 bun scripts/check-cef-extensions.ts
ECHO_CEF_TEST_STORE_ID=ddkjiahejlhfcafbddmgiahcphecmpfh \
  bun scripts/check-cef-extensions.ts
ECHO_CEF_TEST_STORE_ID=ddkjiahejlhfcafbddmgiahcphecmpfh \
  ECHO_CEF_TEST_STORE_MODE=cancel-after-approval \
  bun scripts/check-cef-extensions.ts
```

The store fixture is [uBlock Origin Lite](https://chromewebstore.google.com/detail/ublock-origin-lite/ddkjiahejlhfcafbddmgiahcphecmpfh),
linked by its [official project](https://github.com/uBlockOrigin/uBOL-home/blob/main/README.md?plain=1).
The 2026-10-25 native installation fetched version `2026.1021.1354`, presented
Chromium's generated "Read and change all your data on all websites" warning,
and completed with `success=true`, result `0`. It was enabled in A and absent
from B. Evidence (final child-window harness):
`qualification/extensions-2026-10-25T10-36-32.131Z/`.
The explicit after-approval cancellation run completed with `success=false`,
result `2` (ABORTED), error `Installation cancelled before commit`; the extension
remained absent. Evidence: `qualification/extensions-2026-10-25T10-33-45.169Z/`.
Wrong-owner permission decisions were rejected in both runs. These tests
approve only a disposable native fixture; no GPUI automatic approval is added.

Known native diagnostics: the bare ExtensionViewHost currently produces CEF's
"Timeout of new browser info response" and the fallback-renderer task warning.
Popup scripting and the qualified install flows complete despite those logs.
CEF browser-wrapper association and complete extension API compatibility remain
follow-up work; this build does not claim general extension parity.

Final phase-two incremental framework rebuild: **64 seconds**, followed by a
**54-second fixture-only rebuild** for stable child-window hosting. Minimal
packaging took **66 seconds**. Source/build/distribution tree after packaging:
**42,144,948 KiB (40.19 GiB)**, including the preserved codec-only distribution;
the additional minimal archive is approximately **131 MiB**. Both phases use
the exact GN configuration above. The bridge archive SHA-1 is
`7e1dc0c5b56ff04fc94f0ba660c77773a3d33834`; its framework SHA-256 is
`ac425004c299a009d300cc190c12df09cd246dcf9d113e27a6d790ea17657541`.
The patched distribution passed full `cargo test`: **1,602 passed, zero failed,
one existing ignored**; all four Bun adapter tests passed. A clean pinned-source
mirror accepted the complete 18-file patch and reverse-check, and its patched
contents matched the working source exactly.

The rebuilt Echo bundle also passed the final native H.264/AAC check with the
bridge enabled at `qualification/2026-10-25T10-40-55.900Z/`: custom framework hash
matched, playback advanced from 0.032 to 3.998 seconds, 101 frames rendered with
zero drops, 68,287 audio bytes decoded, and stereo 48 kHz AAC decoded to
2,492,416 samples. The screenshot was visually reviewed; the process exited
cleanly. Thus the extension build retains the independently delivered codecs.

## Shutdown deadline correction

Current source builds always apply `patches/cef154/shutdown-durability.patch`.
The original phase-one codec qualification used no Echo source patch; its
preserved distribution is unchanged. Newly built distributions include the
shutdown correction regardless of whether the optional extension bridge is
selected. The downloaded prebuilt fallback remains unchanged.

The shared timeout policy retains the network service's ten-second graceful
wait and reserves the existing ten-second release teardown allowance around it.
The macOS graceful watchdog therefore allows twenty seconds, disarming on actual
completion. Atomic flushes/replacement and cookie callbacks are unchanged; no
priority scheduling experiment is included. On macOS a network timeout/nonzero
helper exit fails explicitly before the inherited TerminateAll fallback, so the
void CEF shutdown call cannot report success after an unfinished durable write.

`scripts/build-cef.sh build` runs the production-header fake-clock regression in
`scripts/check-cef-shutdown-budget.sh`. It tests work before/after the full network
wait for normal and slow-build allowances. A local negative-control copy using
the original equal deadline fails the assertions. The six-file patch also applies
and reverses on a fresh pinned-source mirror.

The final incremental rebuild took **74 seconds**, including the fake-clock
regression; final minimal packaging took **72 seconds**. Sources, native evidence
and distributions occupied **53,044,936 KiB (50.59 GiB)** after packaging. The
previous phase-two archive is retained outside Git at
`/Users/grosstgrosst/Documents/Bot/cef-src/releases/phase2-413c771.tar.bz2`.

The latest `echo-extensions-dist` contains the final qualified patch. Its framework
SHA-256 is `5bddc3b2ea8c251ee8325be37768a775f5e05619082b8db86efe19eaba1c6f6e`;
the minimal archive SHA-1 is `5d741a3d852ae77895b7e06458c6710730dec7e0`.
The complete native matrix passed **16/16 exits**, all sixteen graceful zero-status
network-helper exits and all twelve cookie persistence checks. Maximum observed
`cef_shutdown` duration was **370 ms**. Current-main workspace tests passed
**1,610 tests**, zero failed, one existing ignored. Investigation, before/after
numbers and exact evidence paths are in [CEF_SHUTDOWN.md](CEF_SHUTDOWN.md).
The original prebuilt fallback and preserved codec-only `echo-dist` do not contain
this correction; select/rebuild the patched distribution to use it.

## Automatic local selection qualification

Plain `cargo run -p bundle` selected `echo-extensions-dist` without additional
environment variables. The resulting framework matched SHA-256
`5bddc3b2ea8c251ee8325be37768a775f5e05619082b8db86efe19eaba1c6f6e`.
The locked native codec check rendered 100 frames and decoded AAC successfully;
its screenshot was inspected. Evidence:
`/Users/grosstgrosst/Documents/Bot/cef-src/qualification/2026-10-25T11-31-51.636Z/`.
`scripts/with-cef.sh cargo test` passed all 1,610 tests (one existing ignored).

## Video PiP presentation

The source recipe also applies `patches/cef154/video-pip.patch`. Echo opts into
video-only pointer presentation through an engine-owned command-line switch;
upstream keyboard controls remain available. See [native PiP reference and
qualification](VIDEO_PIP.md) for evidence and remaining interaction gaps.
