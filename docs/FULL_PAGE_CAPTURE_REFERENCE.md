# Full Page capture — functional qualification

Arc is the only reference. The guarded session used the existing Echo Test
Space and a disposable localhost page, keeping all four baseline owner windows.
`target/parity/partial-closeout/arc-fullpage-clipboard-r4.png` and its JSON
receipt record Arc's 762 × 1840 full-document PNG from a 1000 × 760 window.
`arc-fullpage-toast-r4.png` records the exact confirmation, Save and Share.
Both source fixture tabs were archived; a read-only check filtered solely to
the two fixture URLs confirmed neither remains attached to a sidebar container.
No source profile, setting or Space was changed.

The later accessory/Portrait session lost foreground during toast inspection;
the guard rejected further input and closed its owned window/tab. Its Save
and Portrait output are **reference unqualified**. The previously captured
Portrait popover does not establish output composition, so it is not guessed.

Installed `/Applications/Arc.app/Contents/MacOS/Arc` strings in
`arc-strings-r4.txt` support `Screenshot Copied to Clipboard` (line 47578),
the adjacent comment explicitly defining Save as saving that screenshot to a
file (47580), and the `yyyy-MM-dd-HHmmss` filename / `Save Capture Image`
dialog title (34123–34126). Existing verified plain/accessory toast styling is
reused, adding the measured 48 × 28 Save and 32 × 28 Share controls.

Echo measures the selected document through the existing profile/URL-scoped
renderer interface and requests a bounded full-document CEF screenshot.
It makes no navigation, scroll, viewport-resize or device-emulation calls.
Document images are ineligible as viewport handoff photographs. Same-URL
replacement, profile, tab, zoom and frame changes invalidate an unfinished
capture; a request cannot adopt an earlier screenshot. Invalid/oversized
measurements are rejected, including dimensions above 8192 × 32768 or 32
million pixels. The operation times out without copying another document.

Save uses the captured PNG and the observed timestamp filename format through
the system file picker. Share passes the captured image to native image
services, independently of later clipboard contents. Both actions retain their
original payload/profile; stale generations cannot redirect them. The system
Save dialog title and native service-menu presentation remain unqualified/
cosmetic; Windows/Linux native image sharing remains partial. No Arc artwork
or cloud service is used.

GPUI tests click the SCC tile, complete a full-sized screenshot, verify exact
clipboard bytes, image Share and file Save, and reject a same-URL replacement.
Dimension tests reject malformed/nonfinite/extreme renderer results. Native
and full-test logs and Arc/Echo artifacts are under
`target/parity/partial-closeout/` (`fullpage-*`, `echo-fullpage-*`).
`Capture in Progress…` conditions and Portrait output/composition remain partial.

Reviewed pairs: `pair-fullpage-r4.png`, `pair-fullpage-top-r4-2x.png` and
`pair-fullpage-toast-r4-3x.png` in that directory. Echo captures all 1840 px
while retaining scroll position 400; its current content width is 750 px versus
Arc's 762 px. That chrome-width difference and toast color/border/raster are
cosmetic and remain deferred. Native acceptance: `fullpage-native-r4-qualified.log`
(strict exit 0). `fullpage-full-r4-rebuilt.log` is the full-workspace gate after
rebuilding a pruned bundle test executable.

Native accessory qualification completed with the frozen private bundle
`fullpage-qualified-r4.app`: image Share presented with the retained PNG, the
standalone system Save dialog canceled, and the Command Bar accepted input
after cancellation. `fullpage-accessory-acceptance-r4.json` records those
assertions and strict native exit 0 from the disposable fixture. No CEF build
or canonical publication was performed by this change.
