# Extension Developer mode

This is the separate Developer mode follow-up to the common extension controls
in [EXTENSIONS_UI.md](EXTENSIONS_UI.md). The interface is GPUI; Chromium supplies
profile services and its original inspection frontend. The optional API v4 is
additive to the qualified v1–v3 bridge. Stock prebuilt CEF keeps the metadata
toggle and does not advertise unsupported developer operations.

## Native boundary

- Developer mode reads and writes the selected profile's real preference and
  respects managed preferences, extension policy and private-profile restrictions.
- Load unpacked uses Chromium's normal `UnpackedInstaller`. The production path
  does not require the unsafe test-loader flag or mark an installation as CDP.
- Update requests a foreground `ExtensionUpdater` check and reloads unpacked
  extensions through `ExtensionRegistrar`, reporting errors and a bounded timeout.
  Re-enabling a disabled extension uses Chromium's `ExtensionEnableFlow`, including
  its normal consent prompt when an update increases permissions.
- Details come from `ExtensionInfoGenerator`, including actual permission messages,
  site access and worker state. Worker start/stop notifications invalidate only
  the affected details. Old API clients never receive the new event.
- Worker inspection wakes the selected enabled extension's worker, checks its exact
  profile, script URL and normal DevTools policy, then attaches a profile-bound
  agent-host client. A separate engine pool embeds the original `worker_app.html`
  frontend in its own GPUI window. It shares the actual request-context lease,
  authenticated frontend transport and runtime with its profile; no Chrome browser
  window, browser-window substitute or remote debugging port is introduced.
- Pack uses Chromium's `PackExtensionJob`. Existing CRX files require a separate,
  explicit overwrite confirmation. File work remains owned through completion;
  closing or replacing the originating service prevents delivery to another UI.

Requests and file-picker replies carry profile and service generation. Native
jobs retain the required context and cancel on owner closure. Worker windows
retain an independent profile lease across source-page changes, close on profile
change or quit, and release the native inspector before removing its GPUI parent.
Cmd+W closes the inspector alone. Other inspector shortcuts remain with the
original frontend.

## Reference and qualification

Arc references are in `target/parity/extensions-ui/arc-reference/`:
`developer-load-unpacked.png` and `developer-pack.png`. The directory picker is
an attached native sheet with the reference message and Select button. The Pack
dialog is 512 px wide, with 36 px fields and Browse buttons. The existing Arc
window and its original Space selection were restored after inspection.

The macOS Developer flows passed against candidate generation
`20261026T015700Z-65318`, framework SHA-256
`c9d3ae396593a3f8333c1017c9f3b9c6b64d9394285c1504257461bec9092829`.
The frozen app executable SHA-256 is
`b84623f03a4d17f26734f2f32320bda04c63ffdebd29a6de74ddda87813aefa3`.
Evidence is outside Git at
`~/Documents/Bot/cef-src/qualification/developer-final-20261026T021828Z/developer/`.
`loaded-v1.png`, `updated-v2.png` and `worker-v2.png` show a live unpacked
extension updated from 1.0 to 2.0 and the updated worker's console. The normal
permission details, disable/enable, second-profile isolation, persisted native
Developer preference and explicit CRX overwrite all passed. `pack-hashes.json`
records an unchanged CRX before consent and a changed CRX after consent; generated
private keys stay outside Git. The Arc/Echo Pack pair is
`target/parity/extensions-ui/arc-reference/developer-pack.png` and the evidence
directory's `pack-form.png`.

After rebasing onto main `4cf9291` (including quit-policy commits `a70317e` and
`1bf393b`), full `cargo test` passed: 1,928 passed, zero failed, one ignored.
Linux GNU and Windows MSVC compile checks passed. Their native file dialogs,
inspector windows and extension operations remain unqualified. The complete
canonical gates passed on generation `20261026T015700Z-65318`, which is now the
machine default. Recovery, deletion and 16/16 clean shutdown exits used the
rebased app; engine and Developer evidence retained the same-framework frozen
app. See [the canonical record](CEF_BUILD.md#developerapi-v4-build-record).

The bounded native operator runner is `scripts/check-cef-developer.py`. It runs
inside the canonical runner's existing screen lock, never acquiring another
lock. It creates an owned two-profile session, exposes its exact app identifier
and expiring input lease in `ready.json`, and requires an `operator.json` receipt
with screenshots for every declared check. The fixture starts at version 1.0;
change its manifest to 2.0 while the app remains open, then press Update and
verify both the catalog version and original worker console message. Pack once,
then reuse the generated PEM and verify the overwrite warning before explicitly
continuing. Test the other profile and restore the original test Space.

Never address the app after its lease expires: native app resolution can relaunch
it without the owned profile environment. The runner requires completed CEF
teardown, rejects automation failures and validates the generated CRX3 header.
Its LaunchServices status is explicitly separate from the actual zero exits
required by the direct-exec canonical gates and shutdown matrix. Generated PEM
keys remain outside the repository and are never included in artifacts.

Keep the app and session in the established `target/parity` runtime area and
fixtures under `Library/Caches/EchoQualification`. A normally launched test copy
under Documents stalled before CEF initialization; the runtime-area copy reached
its input barrier normally without any privacy-permission change. Bind native
automation by the exact running app path, since LaunchServices can retain multiple
copies of a test bundle identifier. In a directory sheet, observe the Go To Folder
dialog before setting its PathTextField and confirming; its previous selection
must not be mistaken for the requested fixture.

The Developer runner delegates real Store-extension checks to
`scripts/check-cef-real-extensions.py`. That script captures the app's own window
stack through the existing native automation path. Direct accessibility captures
of the standalone action window produced a scaled root composite, so those
images are excluded from passing evidence. Native events switch Dark Reader off
and on and capture both HTTPS page states. The same native event path selects uBOL's zapper, captures its hover target and
removes an element; native captures then record the page and isolated profile
before normal quit. Inspect every artifact
before writing the real-extension operator receipt. An already completed
Developer receipt for the same exact app/framework can be retained while this
real-extension gate is repeated in a subsequent long session.
