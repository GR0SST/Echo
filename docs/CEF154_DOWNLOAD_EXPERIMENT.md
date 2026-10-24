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

`base` checks 538 distinct selected paths, including absent files that the custom
patches will create. After applying the diffs, `patched` checks their postimages.
`qualified-inputs` also checks eleven recorded generated/read prerequisites, for
549 selected paths. The modes verify seven repository pins. They do not attest
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
The checkpoint passed 18 IO tests and 149 State/deferred tests exactly once.

On macOS, run the native selection from a separate copy of the completed bundle:

```sh
owned=$(mktemp -d "${TMPDIR:-/tmp}/echo-cef-native.XXXXXX")
ditto "$out/ceftests.app" "$owned/ceftests.app"
"$owned/ceftests.app/Contents/MacOS/ceftests" \
  --use-mock-keychain \
  --disable-features=SkipExtraBfcacheNavigationRequest \
  --enable-features=ServiceWorkerInterceptDownloads \
  --use-test-http-server \
  --cache-path="$owned/cache" \
  --gtest_filter='EchoDownloadBridge*.*' --gtest_repeat=1 \
  --gtest_output="xml:$owned/native-tests.xml" \
  > "$owned/native-tests.log" 2>&1
```

Require a successful process exit and every recorded native case with no skips
or failures.
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

Additional native cases allow and cancel real beforeunload dialogs while
unregister is Pending, then verify one original-token outcome and the expected
document. Same- and cross-origin iframe GET and POST commands retain the parent's
exact DOM/nonce and URL while completing in the named child after Closing. POST
checks its exact body on the IO resource request and transfers owned observations
to UI.
CEF154's NavigationHandle-backed OnBeforeBrowse metadata does not contain
postdata; checking it there produced the initial fixture failure, not evidence
that transport dropped the body.

The first cross-origin run passed 29 of 31 cases: Chromium terminated both target
renderers with INVALID_INITIATOR_ORIGIN. CEF's generic request conversion used the
destination origin as initiator. Frame LoadRequest now uses the current document's
security origin instead. Chromium's validation remains unchanged. All 31 tests,
including their receipt, parent-document and POST assertions, were unchanged for
the passing run.

Two source-close cases keep a genuine renderer sync XHR held after a GET/POST
frame command has reserved its receipt. Unregister must remain Pending. The
source browser closes before the held response is canceled. Each original token
then produces exactly one Shutdown outcome, with no item association, target
browse/load or target resource request. This verifies source-browser destruction;
isolated frame loss and renderer-only endpoint disconnect remain separate gates.

The first source-close run aborted in Mojo's pending-responder DCHECK during
CEF frame detach. LoadRequest now reports forwarding over an independent one-way
result pipe, so CEF can retain its intentional-detach reason without a pending
frame RPC responder. Losing the result pipe abandons only its original weak
command handle. No Mojo assertion was removed; all 33 native tests were unchanged
for the passing run.

Live Service Worker tests use a loopback server, real registration/activation and
a fetch event with an empty destination. They hold either the worker-generated
response or a network fallback while admission closes, then require the original
token and distinct, fully verified 2 MiB outputs. The test command enables
ServiceWorkerInterceptDownloads, disabled by default in Chromium 154; it changes
no Echo default. Use the test HTTP server: the legacy CefServer's 1 MiB queued
write limit rejected the first fallback body. The same 35 tests passed with
CefTestServer, without reducing the payload or relaxing assertions.

The 167 selected C++ tests also cover exact-target nonce consumption, failed RPC
cleanup, shared OpenURL copies through the real controller, and tagged
BeginNavigation through deferred Init. A public-value assignment guard covers
the out-of-line OpenURLParams copy assignment required by opaque receipt types.
These results are a bounded macOS slice, not the complete upstream CEF suite.
Three additional changed call-site translation units (both navigation URL loader
tests and the security exploit browser test) passed syntax-only compilation with
their configured compiler flags; their full test suites were not executed.

The save-page manager now captures a receipt before asynchronous item-ID
allocation and revalidates its exact live source/context before creation.
Association precedes embedder hooks; rejection cancels without a synthetic item.
Seven manager cases failed before the fix while the legacy control passed; those
eight test bodies are preserved. A ninth test invokes actual WebContents::SavePage
and verifies synchronous refusal, no item/files, and SaveFileManager teardown.

SAVE_PACKAGE is append-only kind **5**, separate from generic manager downloads.
Its document is needed for serialization even after item association. A caller
must retain that document until the save item is terminal, or refuse this kind
at reserve. Admission now precedes Stop, directory preparation and the save picker.
The original carrier moves through file work, the picker callback and delayed
item-ID allocation; the package retains only a weak cancellation handle while a
callback owns the carrier. Source loss, frozen legacy absence, callback disposal
and refusal without stopping an admitted navigation have test-platform coverage.

Seven early-save-info tests failed before the correction. Six test bodies remain
unchanged. The seventh originally discarded a history-database reply while the
manager still owned the save callback; it now destroys that actual queue while
keeping the package alive and retaining the original release/outcome assertions.
Two upstream SavePackage caller translation units passed syntax-only compilation;
their tests were not run. Four additional native Alloy tests cover actual Save Page:

- ONLY_HTML saves the complete expected HTTP body from a no-store loopback page.
- MHTML captures the changed live DOM; its retained MIME output is checked for
  the updated marker/title. Its hash records observed bytes rather than a full
  predetermined MIME serialization specification.
- Closing-first refusal preserves the live document and leaves an owned output
  directory independently checked empty after native exit.
- A second-fetch HTTP 503 cancels without reporting Complete or leaving output.

The last case failed against unchanged SavePackage with IsComplete=true.
SaveFinished now cancels a failed single-item ONLY_HTML package before Rename or
Finish can report phantom completion. All four test bodies are unchanged from
that red run. These four cases and the previous 35 remain unchanged in the
current native suite.
Both successful saves retain their source document through actual CEF terminal.

Alloy exposes no public CEF SavePage call, so these tests use a separate helper
in a test-only header. It invokes actual WebContents::SavePage with a chosen path;
it is absent from the negotiated production API and adds no capability or CEF
API-hash change. macOS resolves it through the already loaded framework handle.
The retained failed attempts document an invalid-ID guard fixed in the helper
and the initial mistaken assumption that ONLY_HTML captures DOM: it fetches the
network document, whereas MHTML covers live DOM. Native picker UI, complete HTML,
user cancellation/source loss, other platforms and document-retirement policy
remain open.

Two further native tests cover interrupted controls after unregister and
`about:blank`. A real loopback HTTP server sends a valid 64 KiB chunk, waits for
observed native byte progress and drainage, then closes before the terminating
chunk. Chromium reports Interrupted. Resume sends a nonzero Range and the exact
If-Range ETag; the same native item completes with the full expected 1 MiB body,
without a new receipt or association.

Cancel initially failed: CEF's `DoCancel` accepted only `IN_PROGRESS`, ignored
the Interrupted item and retained its 64 KiB partial file. It now also accepts
`INTERRUPTED`. The unchanged test queues a check after Cancel on the same UI
runner and requires the genuine Canceled notification. It independently checks
that the output directory is empty after exit. All 41 native tests passed at
that checkpoint.
The three relevant task-runner source prerequisites are included in qualification
inputs. The first fixture's context-scheme handler could not serve Chromium's
no-renderer resume loader; it was replaced with actual socket HTTP, and its
failure teardown now detaches the temporary directory on FILE before release.
Failed runs and the original partial bytes are retained in the local archive.

This proves the tested macOS Alloy known-item path with its source browser still
alive at about:blank. Source destruction, other transport errors, multiple
profiles and other platforms require further acceptance work.

Registered Alloy main-frame `LoadURL` now reserves its admission before CEF's
`NavigationLock` can queue the command. Copies share one opaque transfer and
consume it once.
The transfer follows its captured WebContents primary root across renderer
replacement; renderer-originated relays retain their stricter exact-RFH binding.
Unregistered legacy CEF keeps its existing path. A newly requested command after
Closing is rejected before pending navigation changes.

The native regression calls `LoadURL` inside the real `OnBeforeBrowse` lock,
begins unregister, then drains the queue. It failed against the previous source
and passes unchanged with early capture: the original token reaches genuine
download association and the exact expected 1 MiB attachment. Existing 41 native
TEST bodies are preserved; their shared LoadURL fixture now distinguishes an
accepted pre-Closing call from an API call first made after Closing. Five new
C++ tests cover POST/copies/one-shot use, foreign/child rejection, actual primary
RFH replacement, frozen legacy absence and rejection without replacing a pending
request. Those 153 selected C++ tests and 42 native tests passed at that
checkpoint.

Reload, IgnoreCache, Back and Forward now capture before the same CEF queue.
The one-shot closure binds the original controller, entry ID and each planned
frame's request identity. Replay cannot select another history entry or obtain
new admission. A removed/changed entry or frame plan rejects before pending
navigation mutation. Scroll updates remain allowed: the identity guard compares
URL, method, POST ID and document/item sequence numbers rather than incidental
PageState bytes. The original POST receipt also survives the repost dialog.

A WebContents observer reports Shutdown if its source disappears while the
embedder still owns the queued callback. Lease release waits for that callback;
dropping an unexecuted live-source command reports Abandoned exactly once.
The four native queue regressions failed without early capture, then passed
with original tokens and expected target URLs after Closing. Rejection tests
invoke their APIs after Closing; accepted tests invoke before it. Existing 42 native
TEST bodies are preserved. Fourteen added C++ regressions cover this ownership,
removed entries, POST replacement, scroll updates and distinct subframe plans.
That checkpoint passed 167 selected C++ tests and 46 native tests on macOS Alloy.

The initial native fixture incorrectly assumed every accepted command replaces
the pending network request. A rapid identical NORMAL reload is suppressed by
Navigator: the queued receipt must produce factual Abandoned before the fixture
releases the original held response, which then completes normally. IgnoreCache
does replace it. Back/Forward must restore BFCache with status 0 and no new
OnBeforeBrowse. The corrected fixture requires those concrete branches; all 46
native TEST bodies remain unchanged from the early-capture red run. Five upstream
navigation/security/SavePackage test translation units passed fresh syntax-only
compilation; their full test suites were not executed.

Seven further native cases use real loopback HTTP responses. The first request
reaches the server before admission is closed; only then does the server send
the redirect. Cross-origin cases change the host from `127.0.0.1` to `localhost`
on the same owned server port.

| Ingress | Response | Observed result after Closing |
| --- | --- | --- |
| Navigation GET | 302 | Original token, final GET, exact 1 MiB file |
| Navigation POST | 303 | Original token, final GET with no POST body, exact file |
| Navigation POST | Cross-origin 307 | Original token, unchanged POST/body, exact file |
| StartDownload | Cross-origin 302 | Original token through IO authorization, exact file |
| Renderer download attribute | 302 | Original renderer receipt through IO, exact file |
| Navigation, denied target | 302 | PolicyDenied, zero target requests or associations |
| StartDownload, denied target | 302 | Same factual denial through the IO reply and drain |

The server checks actual wire method, body and Host. After checking the initial
source navigation, each transfer attempt has one reserve and two authorizations
of that same opaque token. Allowed cases verify both
original/final item URLs and association before the public callback. Five full
1 MiB outputs are independently compared with their generated bodies and SHA256
values in [qualification.json](../patches/cef154/qualification.json); stdout
identifies them with `ECHO_REDIRECT_FILE` markers (seeds 97, 101, 103, 107 and
113). Both denied-output directories are
independently empty after native exit. The IO denial waits for an actual
PolicyDenied outcome and carrier drain; elapsed silence is not success.

Every new case first captures a renderer-created `crypto.randomUUID()` from the
live source DOM. After completion or denial, the same UUID, Browser ID and URL
must remain, with exactly one source HTTP request. This checks document state
rather than assuming equal URLs establish identity. All 53 native and 167
selected C++ tests pass once; all previous 46 native test bodies remain unchanged.
Only the native test source changed, so earlier upstream syntax checks are
reused. Production behavior required no correction for these cases.

These tests retain the source browser through the transfer. Source destruction
during redirect, other redirect/failure combinations and other platforms still
need acceptance work; this does not enable a complete fence or Rust adoption.

Capabilities remain **17** (`DIRECT_RECEIPT | EXACT_ASSOCIATION`). Public `seal`
returns Unsupported with fence zero; full-fence capability is off. Remaining
coverage includes retargeted subframes, request rewriting, duplicate suppression,
frame/context loss and endpoint disconnects. Further Service Worker failure and
destruction paths, other early ingress/no-item exits, SavePackage/resume and
ordered public seal/drain/failure semantics remain open. A timeout, first
association or nonloading source is not an admission fence.

Production adoption additionally needs verified distribution and loaded-framework
identity, Rust integration, native host handoff/cleanup, no-clobber publication,
captured-profile persistence and quit/reload durability. See
[Downloads integration](../DOWNLOADS_INTEGRATION.md) for the existing Rust seams.
The current patch checkpoint does not establish complete Arc parity.
