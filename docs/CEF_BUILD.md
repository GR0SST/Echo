# Local CEF 154 build

This recipe builds the exact CEF used by Echo with H.264/AAC enabled. Sources,
toolchain downloads, intermediate objects and distributions live outside Echo.
The default Cargo configuration continues downloading the original prebuilt.

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
Selection errors stop the command. Without `ECHO_CEF_DIST`, the wrapper simply
executes the command and Cargo retains its existing prebuilt fallback. Direct
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
