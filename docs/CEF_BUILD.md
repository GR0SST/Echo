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

## Canonical distribution

The current recipe applies every top-level patch in
[`patches/cef154/series.json`](../patches/cef154/series.json), in this order:

1. `chromium.patch` — Chromium download-receipt checkpoint.
2. `cef.patch` — its CEF bridge and loader.
3. `extensions-bridge.patch` — Alloy actions, profile services and install prompts.
4. `shutdown-durability.patch` — bounded background shutdown, helper reaping and storage recovery.
5. `video-pip.patch` — the final video-only pointer presentation and keyboard fallback.
6. `browsing-data-bridge.patch` — profile-scoped browsing-data deletion callbacks.

The extension patch is rebased over the receipt checkpoint. Both headers and
exports are retained in their shared CEF build lists, and both integrations remain
in the download delegate. `scripts/cef-stack.ts` rehearses the complete ordered
stack on a shadow of its selected files before writing anything. It rejects
partial/edited stacks and unlisted patches. An already-applied stack must reverse
and reapply exactly. It does not reset source or modify `depot_tools`.

```sh
# Fresh external checkout only: official automate-git prepares the pinned base.
scripts/build-cef.sh sync
# Existing prepared checkout: one locked Release arm64 build and package.
scripts/build-cef.sh release
# Full Rust suite and native gates; publish only the qualified generation.
bun scripts/check-cef-canonical.ts
# Only after every gate passes, remove superseded external distributions.
bun scripts/clean-cef-dists.ts
```

`release` is the default mode. Every sync/build/package operation holds immutable
`/tmp/echo-cef-build.lock`, inherits `nice -n 10`, and uses Ninja `-j 8`. Only
`Release_GN_arm64` is generated. `sync` refuses to overwrite custom sources;
use `release` for incremental work. Generation uses CEF's official `gn_args.py`
helpers after translator/version-manager updates. It does not rerun CEF's base
patcher over the custom stack: the receipt changes overlap `content_2015.patch`.
The latter was verified as already applied in a shadow with the custom stack
reversed. API hashes remained unchanged.

Packages are immutable generations under `cef-src/canonical-binary-distrib/`;
flattened cef-rs adapters are under `cef-src/canonical-adapters/`.
`cef-src/echo-extensions-dist` points to the current adapter, the same path used
by automatic bundling. Publication changes one symlink. The first migration
preserves the original directory adapter under `.previous-<id>` until checks
pass. `build.json` records source/tool pins, ordered patch hashes, selected-source
hash, GN definitions, build duration, archive digest and framework SHA-256.
Separate `echo-dist`, `echo-pip-dist` and `echo-pip-final-dist` adapters are obsolete.

`release` stages a candidate and writes `cef-src/candidate-path`; it does not
publish it. The canonical acceptance runner tests that exact candidate: complete
`cargo test`, an explicit candidate bundle, H.264/AAC playback, an Alloy
MV3 popup with second-profile isolation, eight shutdown write/restart scenarios
(16 clean exits with HTTP cookie persistence), and the native PiP harness.
It also forces a network timeout to prove normal parent exit, helper termination,
and next-launch cookie/localStorage/IndexedDB recovery; a corrupt SQLite fixture
must be rejected without modifying it. A separate deletion smoke test seeds and
removes cookies, localStorage, IndexedDB and CacheStorage through patch #6.
Every native result must carry the candidate framework hash. Screen holders use
`/tmp/echo-screen.lock`, hard timeouts and disposable Echo Test profiles.

The metadata bridge adds two mandatory automated gates: unprivileged popup
redaction (including URL-filter result counts) and the GPUI tab/window fixture
in [CEF_EXTENSION_TABS.md](CEF_EXTENSION_TABS.md). Before publication, also check
Store-installed Dark Reader and uBlock Origin Lite on an actual HTTPS page and
verify that the other test profile has neither extension. Record the viewed
screenshots/logs in the candidate's `real-extension-qualification.json`:

```json
{
  "passed": true,
  "framework_sha256": "the candidate's exact SHA-256",
  "executable_sha256": "the model fixture app's exact SHA-256",
  "checks": {"dark_reader_https": true, "ubol_https": true, "profile_isolation": true},
  "artifacts": ["/absolute/path/to/verified/screenshot.png", "/absolute/path/to/native.log"]
}
```

This is a record of completed native checks, not an approval override. The runner
requires the exact framework and model-fixture executable hashes, all three
checks, and existing artifacts.
`ECHO_CEF_REAL_EXTENSION_QUALIFICATION` can select an external receipt.
Build and freeze the app before collecting that receipt. To use that same app
throughout the automated gates without rebuilding it, set
`ECHO_CEF_QUALIFICATION_BUNDLE=/absolute/path/to/frozen/Echo.app` when running
`bun scripts/check-cef-canonical.ts`. The runner still runs the complete Rust
suite, checks the framework, and requires the real-extension executable hash to
match its GPUI model gate. Individual codec/model checks accept
`ECHO_CEF_TEST_BUNDLE` for the same purpose. Keep the
artifacts outside Git, with the rest of the native evidence.

Only after these checks pass does the runner write a native qualification receipt
and publish the adapter with one atomic symlink switch under the build lock. It
then runs plain `cargo run -p bundle` without CEF overrides and verifies the default
framework hash before marking the entire qualification passed. During development,
`ECHO_CEF_BROWSING_DATA_BUNDLE` can select the data agent's probe-enabled bundle;
the smoke harness replaces its entire framework with the candidate and verifies
both executable and framework hashes in its evidence.

Cleanup takes the same CEF build lock and requires the complete qualification
receipt and matching framework. It preserves the canonical generation, sources,
objects, evidence and prebuilt fallback. `cleanup.json` records removed paths and
logical disk size. Compiling the download receipt checkpoint does not enable
production download admission.

### Published metadata bridge generation

Generation `20261025T192350Z-5008` adds the GPUI-authoritative extension tab/window
API to the same ordered six-patch stack. CEF/Chromium pins and GN definitions
below are unchanged. Incremental compile/link took **190 seconds**; compilation
and minimal packaging together took **343 seconds**. The framework SHA-256 is
`55a60b71fd32579a392ee53f337a908cecb7b0f012971d6ad3317b4a32ec82aa`;
minimal archive SHA-1 is `6eb300d569d61ddaf261ff74fcaed521b808f7de`.

Qualification used one frozen Echo executable
`50a987a377c608a56260d6408b9d08e586711f68d4f48bd5d70cd7103d580250`.
Full workspace tests passed **1,763 tests, zero failed, one existing ignored**,
including doctests. Native gates passed all 18 query filters, permission
redaction, 20 GPUI mutation/message/event checks, Alloy popup/profile isolation,
and real Web Store uBlock Origin Lite and Dark Reader operation on HTTPS.
The management/pinning/shortcuts UI still has a separate Arc parity gate.

The uninterrupted shutdown matrix passed **16/16 exits**, eight write/restart
scenarios with expected HTTP cookies and private isolation, no native failures,
and sixteen graceful network-helper exits. Full teardown median/max were
**227/2,323 ms**. The matrix held the screen lock for its entire duration; its
Arc CGWindowList preflight found no elevated panels and did not restart Arc.
The final PiP run passed plain 224×126 idle/hover, Space pause, Tab reveal,
mouse Back to Tab, manual re-entry at retained bounds, Close and clean shutdown.
Forced-timeout recovery exited normally, reaped helper 93716, checked retained
cookie/localStorage/IndexedDB data and rejected corrupt SQLite. Browsing-data
removal cleared cookies, localStorage, IndexedDB and CacheStorage (`failed_bits=0`).

A codec trial never navigated away from `about:blank`, so it did not exercise
H.264/AAC decoding. The harness now waits before activation, positions its owned
window and requires focus before typing. An A/B check used the identical frozen
executable and W3C MP4 with the preceding `9210…` framework and this `55a6…`
framework. Both passed; the new framework rendered **101 frames, zero drops**,
decoded **68,663 audio bytes**, and produced **2,492,416 stereo 48 kHz samples**.
Both playback screenshots were visually inspected. One PiP trial missed Close;
an unchanged fresh retry passed. The final harness refreshes pointer motion after
its screenshot and uses the project's 300 ms native-button hold; its single
Close click and unchanged visibility/three-second dismissal assertions passed.
The failed trials remain in the evidence.

Evidence beneath `/Users/grosstgrosst/Documents/Bot/cef-src/qualification/`:

- `final-long-lock-matrix/run-3g7m3id2/`: all sixteen exits and bundle hashes.
- `final-post-long-lock/`: codec A/B reports and `collect-final.ts`, which validates
  the individual same-executable receipts and all six patch hashes before publication.
- `final-post-retry-1/`: unchanged PiP retry, recovery and browsing-data reports.
- `final-pip-input-barrier/pip-kgodxm1x/`: final PiP proof; image pair `idle.png`
  and `hover.png`, with keyboard controls in `tab-reveal.png` (visually inspected).
- `canonical-2026-10-25T20-05-34.434Z/`: popup, redaction and model API receipts.
- `canonical-2026-10-25T20-21-15.846Z/rust-tests.log`: full Rust suite.

Immediately before cleanup, the source/build/evidence tree measured
**103,852,788 KiB (99.04 GiB)**; the disk had **321 GiB free**. Native evidence
contains APFS-cloned app/profile copies, so logical tree size includes those.
The canonical adapter retains its `build.json`, `qualification.json` and cleanup
receipt outside Git. No binaries are committed.
Plain `cargo run -p bundle` without CEF overrides selected this framework and
matched its hash. Qualified cleanup removed **3,000,736 KiB (2.86 GiB)** of
superseded distributions; only this generation remains in the canonical adapter
and package roots. Source, objects, evidence and the prebuilt fallback remain.
After rebasing this integration onto main, full `cargo test` passed **1,798 tests,
zero failed, one existing ignored**. The rebuilt bundle passed all 20 native
model checks again at `qualification/extension-model-2026-10-25T20-51-48.035Z/`,
using executable `6e778e3fa00f3e634253e3c851fc07069256e8411deaf1673473a54c7a562fce`
and the unchanged canonical framework.

### Previous six-patch generation

Generation `20261025T160409Z-9401` was staged on 2026-10-25. Its incremental
compile/link and budget regression took **92 seconds**, and complete build and
packaging took **183 seconds**, under the same pins/GN definitions below.
Framework SHA-256:
`9210e6a73e91c143fb62b68261990425226796b962d5c814492290ddb4129324`.
Minimal archive SHA-1: `3024db11cd195a9c44397a2fea63c40d7e60539f`.
Before qualification/cleanup, `du -sk cef-src` measured **77,841,664 KiB
(74.24 GiB)**, including sources, objects, all generations and retained evidence.
The verifier recognizes both origin-prefixed and plain bucket `indexeddb.leveldb`
filenames. A real closed IndexedDB clone in the bucket layout failed on the
preceding candidate and passed this framework with 19 entries, at
`cef-src/qualification/bucket-positive-0vr6e_7p/`. Full canonical qualification
also repeats this layout check after its real forced-timeout/restart test.

### Native qualification and publication

The final framework passed full `cargo test`: **1,695 passed, zero failed, one
existing ignored**, including doctests. All 23 Bun build/stack/cleanup tests passed.
The native matrix passed **16/16 clean exits**, all twelve HTTP cookie persistence
checks, private isolation and sixteen graceful network-helper exits. CEF shutdown
median/max were **152/363 ms**; full background teardown median/max were
**204.5/2,275 ms**. The existing immediate-hide behavior remains in place; its
CoreGraphics request-to-window-gone qualification is documented in
[CEF_SHUTDOWN.md](CEF_SHUTDOWN.md#quit-visibility-and-latency).

The same framework passed H.264/AAC decoding (100 rendered frames, zero drops,
68,287 decoded audio bytes, 2,492,416 stereo 48 kHz samples), Alloy MV3 popup
isolation, native PiP input/return/manual re-entry/close, and browsing-data removal
of seeded cookies, localStorage, IndexedDB and CacheStorage (`failed_bits=0`).
Forced timeout exited normally and reaped helper 73124; restart verified one
SQLite database and eleven LevelDB stores, preserved all three sentinels, and
cleared its marker only after writing a receipt. The plain bucket IndexedDB clone
passed with 19 entries, and a corrupt SQLite copy was rejected unchanged.

Evidence root:
`cef-src/qualification/canonical-2026-10-25T16-07-28.461Z/`.
The first PiP attempt stopped at the unchanged visibility guard because an Arc
layer-500 auxiliary panel covered it; Echo still exited normally. After a short
wait, the first retry passed without restarting Arc. The original failed report
is retained as `result.json`; `result-final.json` and the adapter's
`qualification.json` combine the actual same-framework reports, after verifying
that the executable hash matched the frozen matrix bundle and Rust inputs had
not changed. `complete-canonical.ts` in the evidence directory records that
continuation and the locked publication/default-bundle commands. Plain
`cargo run -p bundle` without CEF overrides selected the published generation
and its framework hash matched. PiP image pair:
`pip-mqy6mc2a/idle.png` and `pip-mqy6mc2a/hover.png`; keyboard reveal is
`pip-mqy6mc2a/tab-reveal.png`. These images were visually inspected.

Qualified cleanup removed **5,634,920 KiB (5.37 GiB)** across 23 superseded
adapter/package paths, including the separate PiP distributions and all older
canonical generations. Only `20261025T160409Z-9401` remains in both canonical
adapter/package roots. `cleanup.json` lists every deletion. After qualification
and cleanup, the complete source/build/evidence tree measured **81,632,732 KiB
(77.85 GiB)**; native evidence includes frozen app/profile copies, so that total
also reflects the acceptance runs. No source, object, evidence or prebuilt
fallback was deleted.

After rebasing onto main's subsequent UI-only changes, full `cargo test` passed
**1,706 tests, zero failed, one existing ignored**, with doctests green. The
rebased bundle passed a fresh native codec check in a self-contained Echo Test
profile at `cef-src/qualification/2026-10-25T16-49-02.821Z/`; its bundled framework
still matched the canonical SHA-256. The playback screenshot was inspected.

### Earlier recovery preflight

Generation `20261025T154030Z-56106` was staged on 2026-10-25, using the exact
pins and GN definitions below. The final incremental compile/link and budget
regression took **77 seconds**; the complete configuration/build/package
invocation took **147 seconds**. Its framework SHA-256 is
`d5e18c3fbe322f8c64262a7a902b06e7be1846408b580680c8257f441d46c57e`; minimal archive SHA-1 is
`d9d54b1ffaa279a8e7c98137d58035ae851b9a26`. Full actual `args.gn`, tool/source commits and
all six patch digests are recorded in the adapter's `build.json`.

The forced-timeout preflight passed against this framework at
`cef-src/qualification/recovery-final-preflight/recovery-6fkqy4wo/`:
normal parent exit, network helper 70759 reaped, one SQLite database and eleven
LevelDB stores verified on restart, HTTP cookie/localStorage/IndexedDB sentinels
retained, and a corrupt SQLite copy rejected without modification. Full canonical
qualification is separate; staging alone does not change the default symlink.

### Combined build record

The earlier five-patch candidate was built on 2026-10-25. Its framework SHA-256 is
`dbe532c0f9188392cf8d8f262e614ed134a4552fdcaba06eb4033f9b869a27da`,
and its minimal archive SHA-1 is `e0605e5a6cf5627f714ed0efac9fc5835ced1a81`.
Its historical generation is `20261025T140112Z-43046`; the preferred adapter
path remains `~/Documents/Bot/cef-src/echo-extensions-dist`.

The combined incremental compile/link completed all 4,726 Ninja steps in
3,674 seconds (61 min 14 s). Editing the running shell wrapper caused a parser
failure after compilation; no source or compiled object was reset. A restart
reported no Ninja work and completed configuration, provenance checks,
packaging and staging in 128 seconds. This candidate was not published after
the later PiP run exposed a network-helper timeout crash. The two invocation durations total
3,802 seconds (63 min 22 s), excluding their final disk scans. Logs are
`cef-src/canonical-release-driver-2.log` and
`cef-src/canonical-release-driver-3.log`.
The launcher now snapshots its shell text in memory before entering the lock,
so later edits cannot invalidate Bash's file offsets during a long build.

Before obsolete-package cleanup, `du -sk cef-src` measured 59,242,032 KiB
(56.50 GiB), including sources, intermediates, all distributions and logs.
Other agents were building Rust concurrently; changes in filesystem free
space are not attributed to this CEF build.

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
scripts/build-cef.sh sync     # only for a fresh checkout
scripts/build-cef.sh release  # incremental build + canonical package

# Resumable stages, when needed (run sequentially):
scripts/build-cef.sh build
scripts/build-cef.sh distrib
```

`ECHO_CEF_SOURCE` overrides the external source root; `ECHO_DEPOT_TOOLS` overrides
the existing tooling path. A shared `/tmp/echo-cef-build.lock` serializes all
stages across Echo worktrees. The entire operation runs under `nice -n 10`;
the sole Chromium compile command uses `ninja -j 8`. No Debug build is generated.
The original codecs-only phase applied no custom Echo patches. The current
recipe applies the complete ordered series above.

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

The original codecs-only official-tool commands, expanded for this host
(historical; use the canonical script above for the complete stack):

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
export ECHO_CEF_DIST=/Users/grosstgrosst/Documents/Bot/cef-src/echo-extensions-dist
scripts/with-cef.sh cargo test
scripts/with-cef.sh cargo run -p bundle
```

The wrapper validates the exact CEF version, metadata and required paths, then
exports an absolute `CEF_PATH` for Cargo and its child bundle commands.
Selection errors stop the command. Without `ECHO_CEF_DIST`, the wrapper and
bundle utility prefer `~/Documents/Bot/cef-src/echo-extensions-dist` when present
on macOS arm64, then fall back to Cargo’s prebuilt configuration. Direct
`CEF_PATH=/absolute/path` remains supported by cef-rs.

Current minimal artifact (the generation name is recorded in `build.json`):

```text
/Users/grosstgrosst/Documents/Bot/cef-src/canonical-binary-distrib/<generation>/
  cef_binary_154.0.34+g14c5a08+chromium-154.0.8037.98_macosarm64_minimal.tar.bz2
```

Do not commit binaries, source checkouts or intermediate outputs to Echo.

## Original codecs-only qualification record

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
That phase preceded the extension bridge described below.

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
ECHO_CEF_DIST="$src_root/echo-extensions-dist" bun scripts/check-cef-codecs.ts
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

## Extension bridge

`patches/cef154/extensions-bridge.patch` is a separate, experimental source
patch on the same pinned CEF/Chromium revisions. It adds an additive C ABI;
stock `cef` Rust bindings and their API hashes stay unchanged. The canonical
build always includes it and verifies the full ordered stack. Operations remain
runtime-gated by `--echo-enable-extensions-bridge`; the native harness enables it
explicitly. GPUI extension UI is still a separate implementation task.

```sh
scripts/build-cef.sh release
ECHO_CEF_DIST="$HOME/Documents/Bot/cef-src/echo-extensions-dist" \
  scripts/with-cef.sh cargo test
ECHO_CEF_DIST="$HOME/Documents/Bot/cef-src/echo-extensions-dist" \
  scripts/with-cef.sh cargo run -p bundle
ECHO_CEF_EXTENSIONS_BRIDGE=1 target/bundle/Echo.app/Contents/MacOS/Echo
```

The original phase-two experiment used `bridge-binary-distrib` and a separate
codec-only `echo-dist`. The canonical recipe above supersedes those packages;
all source patches now share `echo-extensions-dist`. All binaries stay outside Git.

`bun scripts/check-cef-extensions.ts` freezes the source-built `cefclient` native
harness with the complete selected distribution framework, verifies its hash,
and runs under the shared screen lock and a 90-second hard timeout. It uses
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
The original phase-one codec qualification used no Echo source patch; the
canonical distribution supersedes that package. Newly built distributions
include the shutdown correction regardless of whether the extension bridge is
enabled at runtime. The downloaded prebuilt fallback remains unchanged.

The current policy allows the network service up to **60 seconds** in the
background, followed by at most **two seconds** to terminate/reap a late helper.
The outer release watchdog reserves the original ten-second teardown allowance
as well (72 seconds total). Echo hides its window immediately on Quit; normal
quit latency and the window disappearance are measured separately. Cookie flush
callbacks and atomic replacement remain unchanged.

A timeout no longer triggers `CHECK`/SIGTRAP. The parent atomically records
`echo-dirty-shutdown.json` in its cache root, terminates and reaps its owned helper,
updates the marker, logs the outcome, and returns normally. Before the next CEF
startup, Echo launches its own executable in isolated headless verifier mode.
The same bundled framework checks cookie SQLite integrity and persisted storage
SQLite/LevelDB integrity, using Chromium's IndexedDB comparator. SQLite opens
read-only; LevelDB performs ordinary journal recovery with paranoid checks and
checksum iteration under exclusive locks. The verifier has a 60-second parent
limit and never repairs, razes or deletes a failed store. Only a successful
verification writes `echo-shutdown-recovery.json` and clears the marker. Failure
retains the profile and marker and prevents the engine from opening them.

The previous ten-second fail-closed CHECK described by phase three below is
historical: a later PiP qualification found a real network shutdown taking about
80 seconds and exiting 133. This revision replaces that user-visible crash with
the bounded background policy and explicit recovery above.

`scripts/build-cef.sh build` runs the production-header fake-clock regression in
`scripts/check-cef-shutdown-budget.sh`. It tests work before/after the full network
wait for normal and slow-build allowances. A local negative-control copy using
the original equal deadline fails the assertions. The ordered-stack rehearsal applies and reverses the current patch on selected
source mirrors before compilation.

The historical phase-three incremental rebuild took **74 seconds**, including the fake-clock
regression; final minimal packaging took **72 seconds**. Sources, native evidence
and distributions occupied **53,044,936 KiB (50.59 GiB)** after packaging. The
previous phase-two archive was retained during that qualification outside Git at
`/Users/grosstgrosst/Documents/Bot/cef-src/releases/phase2-413c771.tar.bz2`.

That phase's `echo-extensions-dist` contained the final qualified shutdown patch. Its framework
SHA-256 is `5bddc3b2ea8c251ee8325be37768a775f5e05619082b8db86efe19eaba1c6f6e`;
the minimal archive SHA-1 is `5d741a3d852ae77895b7e06458c6710730dec7e0`.
The complete native matrix passed **16/16 exits**, all sixteen graceful zero-status
network-helper exits and all twelve cookie persistence checks. Maximum observed
`cef_shutdown` duration was **370 ms**. Current-main workspace tests passed
**1,610 tests**, zero failed, one existing ignored. Investigation, before/after
numbers and exact evidence paths are in [CEF_SHUTDOWN.md](CEF_SHUTDOWN.md).
The original prebuilt fallback and the old codec-only package do not contain
this correction; the canonical distribution includes it together with PiP and
the extension bridge.

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
