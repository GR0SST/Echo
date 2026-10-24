# CEF154 download receipt experiment

This source checkpoint investigates continuing downloads after their originating
tab or window closes. It adds request identity and lifetime facts that stock CEF
does not expose. Echo's normal Cargo build still uses its existing prebuilt CEF;
these patches do not enable production download admission.

The [patch directory](../patches/cef154/) contains the Chromium and CEF diffs,
their source manifest, a selected-test record and native test XML. It contains no
framework binary or Arc assets.

## Exact source baseline

| Source | Pin |
|---|---|
| Chromium | `154.0.8037.98`, `b859317bf11f6be47f9b7799ec690a0a42a1fb33` |
| CEF | `154.0.34`, `14c5a089e8452874cb1a556dc3fde26ed2d87723` |
| Standard CEF layer | 116 pinned patch files, applied before these custom diffs |

Use a separate, prepared CEF154 checkout with its matching dependencies and
standard CEF patches. The Chromium158 checkout used by this repository's
`bun run build` is a different source tree. Upstream's
[pinned building instructions](https://raw.githubusercontent.com/chromiumembedded/cef/14c5a089e8452874cb1a556dc3fde26ed2d87723/docs/branches_and_building.md)
describe preparing and configuring CEF. This guide starts after that preparation.

The read-only checker requires the pinned repository HEADs and checks selected
source bytes, standard patch inputs and the custom patch files. It does not fetch,
apply patches, build, launch a browser or register a distribution.

```sh
# Run in the Echo repository. Supply the separate CEF154 chromium/src directory.
shasum -a 256 -c patches/cef154/SHA256SUMS
bun scripts/cef154-patches.ts base /path/to/cef154/chromium/src
```

`base` checks 534 distinct selected paths, including absent files that the custom
patches will create. After applying the diffs, `patched` checks their postimages.
`qualified-inputs` also checks six recorded generated/read prerequisites, for
540 selected paths. The modes verify seven repository pins. They do not attest
the entire checkout, SDK, all generated outputs, a compiled binary or a loaded
framework. Keep the source tree stable while checking it.

## Apply and build

With builds stopped and the `base` check successful, apply the two diffs from
their respective repository roots. Stop if any command fails.

```sh
echo_repo=/path/to/Echo
cef_src=/path/to/cef154/chromium/src

patch -d "$cef_src" -p1 --fuzz=0 --batch \
  -i "$echo_repo/patches/cef154/chromium.patch"
patch -d "$cef_src/cef" -p1 --fuzz=0 --batch \
  -i "$echo_repo/patches/cef154/cef.patch"

bun "$echo_repo/scripts/cef154-patches.ts" patched "$cef_src"
python3 "$cef_src/cef/tools/translator.py" --root-dir "$cef_src/cef"
```

Translator regeneration is required: macOS test code resolves the additive API
through CEF's loaded framework handle using the patched generated loader.
Regenerating headers alone does not update that loader implementation.

Configure GN with the pinned CEF workflow, then build these targets from the
Chromium source directory:

```sh
cd "$cef_src"
out=out/Release_GN_arm64
gn gen "$out"
gn check "$out" //components/download/internal/common:download_receipt_tests
gn check "$out" //cef:libcef_static_unittests
autoninja -C "$out" -j8 \
  download_receipt_tests libcef_static_unittests cef_framework ceftests
```

The qualified build used macOS arm64, Xcode 27.1 and SDK 27.0. Selected settings
were `is_debug=false`, `is_component_build=false`, `symbol_level=1`,
`target_cpu="arm64"` and `disable_fieldtrial_testing_config=true`; these are not
a complete GN configuration. The test build had DCHECKs enabled. Windows and
Linux builds and native behavior have not been qualified for this checkpoint.

## Run the selected tests

Run each selection once, with no automatic retries:

```sh
"$out/download_receipt_tests" \
  --gtest_filter='ResourceDownloaderRedirectTest.*:DownloadResponseHandlerTest.*' \
  --gtest_repeat=1 --test-launcher-jobs=1 --test-launcher-retry-limit=0
"$out/libcef_static_unittests" \
  --gtest_filter='DownloadBridgeState.*:EchoDownloadState.*:DeferredNavigationReceiptTest.*' \
  --gtest_repeat=1 --test-launcher-jobs=1 --test-launcher-retry-limit=0
```

The exact executed test names and counts are recorded in
[qualification.json](../patches/cef154/qualification.json). Review the listed
suite names against that record before using a selection from another revision.
The checkpoint passed 18 IO tests and 113 State/deferred tests exactly once.

On macOS, run the native selection from a separate copy of the completed bundle:

```sh
owned=$(mktemp -d "${TMPDIR:-/tmp}/echo-cef-native.XXXXXX")
ditto "$out/ceftests.app" "$owned/ceftests.app"
"$owned/ceftests.app/Contents/MacOS/ceftests" \
  --use-mock-keychain \
  --disable-features=SkipExtraBfcacheNavigationRequest \
  --cache-path="$owned/cache" \
  --gtest_filter='EchoDownloadBridge*.*' --gtest_repeat=1 \
  --gtest_output="xml:$owned/native-tests.xml" \
  > "$owned/native-tests.log" 2>&1
```

Require a successful process exit and all 25 cases with no skips or failures.
The BFCache flag selects the legacy request-replacement path covered by these
tests; it is not an Echo application default. Retain the owned log/XML and check
that its helper processes have exited. A failed copy is evidence of that attempt;
use a new copy after changing the source.

The log prints `ECHO_QUIESCENCE_FILE`, `ECHO_SIBLING_FILE_0` and
`ECHO_SIBLING_FILE_1`. Check the actual bytes, not only CEF's Complete state:
each file must contain exactly 2 MiB of bytes `(i * 37 + seed) % 251`, with seeds
11, 11 and 19 respectively. The recorded run independently compared those bytes
and their SHA-256 values. The two sibling files must differ. Preserve the files
with the run's evidence before removing its temporary directories.

## What passed, and what remains

The pre-close GET/POST cases first reproduced zero reservations and premature
unregister on the prior runtime. They now retain their original token through
genuine delayed renderer work and Closing. Post-close commands remain rejected
before dispatch. A same-document case first exposed a renderer crash in trusted
null-origin fragment navigation. The unchanged test now verifies the original
DOM, fragment URL, original-token outcome and final unregister.

The 131 selected C++ tests also cover exact-target nonce consumption, failed RPC
cleanup, shared OpenURL copies through the real controller, and tagged
BeginNavigation through deferred Init. A public-value assignment guard covers
the out-of-line OpenURLParams copy assignment required by opaque receipt types.
These results are a bounded macOS slice, not the complete upstream CEF suite.
Three additional changed call-site translation units (both navigation URL loader
tests and the security exploit browser test) passed syntax-only compilation with
their configured compiler flags; their full test suites were not executed.

Capabilities remain **17** (`DIRECT_RECEIPT | EXACT_ASSOCIATION`). Public `seal`
returns Unsupported with fence zero; full-fence capability is off. Complete
beforeunload, subframe, request rewriting, duplicate suppression, frame/context
loss and endpoint-disconnect coverage. Actual Service Worker paths, other early
ingress/no-item exits, SavePackage/resume and ordered public seal/drain/failure
semantics remain open. A timeout, first association or nonloading source is not
an admission fence.

Production adoption additionally needs verified distribution and loaded-framework
identity, Rust integration, native host handoff/cleanup, no-clobber publication,
captured-profile persistence and quit/reload durability. See
[Downloads integration](../DOWNLOADS_INTEGRATION.md) for the existing Rust seams.
The current patch checkpoint does not establish complete Arc parity.
