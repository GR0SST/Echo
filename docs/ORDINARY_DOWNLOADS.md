# Ordinary download integration

## Normal default destination

Normal-profile page creation now selects the existing explicit download-owner
creator. The source captures its canonical profile writer before CEF creation;
callbacks do not infer ownership from the currently selected Space. Private and
Blank contexts retain the deny path. The existing normal continuation capability,
single AppDownloads consumer, exclusive quit handoff, staged no-clobber publisher
and proof-bound canonical writer remain the transfer path. Ordinary admission
is excluded from the explicit disposable probe/acceptance scripts.

The profile's default Downloads directory or selected absolute download folder
supplies the staging folder. The frozen before-download suggested name supplies
the filename; invalid/path-traversing names are rejected. Native Arc repeated-file
inspection established `name (1).ext` collision naming. Publication attempts the
original and then suffixed candidates, preserving every existing target.

A real public file was downloaded through the page link in an isolated Echo:
`https://proof.ovh.net/files/1Mb.dat`. Its final 1,048,576 bytes match an independent
fetch (SHA-256 `30e14955ebf1352266dc2ff8067e68104607e750abb9d3b36582b8af909fcb58`).
The final file carries CEF's native `com.apple.quarantine` attribute identifying
Echo. A canonical completed record is present both in the live Library and the
saved session after strict native exit zero / CEF Complete. No stage path was
persisted as a completed file.

Evidence is under `target/parity/ordinary-downloads/`:

- `arc-default-reference/`: default no-prompt behavior, dock indicator and completion.
- `arc-collision-reference/`: a second completed file with the `(1)` suffix.
- `arc-public-reference/`: the same public resource, completion and Library row.
- `echo-default-public/`: native actor, canonical record, saved session, final
  file hash and strict native exit zero.
- `arc-active-reference/`: active Library row with received/total bytes and
  estimated remaining time. Source fixture tabs and owned roots are closed.

The public data-file subtitle now uses the observed generic `Download` label.
Typed records retain their type/source subtitle.

## Native chooser and explicit Save As

The profile's **Ask me where to save files every time** preference now opens a
plain native save sheet. Save Link As… and Save Image As… are enabled for admitted
normal-profile documents and always request that chooser. The response's frozen
CEF filename supplies its initial name; the native chooser has no page-export
Format accessory. Its collapsed/expanded state is the platform dialog's state.

Explicit transfers use a dedicated hidden native registration in the captured
request context. The original tab is not navigated, and concurrent transfers from
that tab cannot consume the selected destination. Preparation and delayed dispatch
validate the document epoch and lifetime; native creation, chooser presentation,
reservation and download effects happen outside application/page/engine borrows.
The same single consumer performs staged publication and canonical recording.
A canceled/rejected destination cannot be reopened by a late chooser reply.
Source closure and quit release initiators through the normal native lifecycle.
An outstanding native chooser observes quit and cancels its own panel outside
application borrows, allowing AppKit termination to finish.

Strict native exit-zero checks cover:

- `echo-save-as-confirmed/` and `echo-save-final/`: Save Link As of the public 1 MiB file, selected
  destination, matching independent SHA-256, canonical Library record and session.
- `echo-download-ask/`: the same public file through an ordinary click with the
  saved Ask preference enabled; selected destination and matching bytes.
- `echo-save-image/`: Save Image As of an owned PNG fixture; all 317 bytes match,
  with canonical completion and no service errors.
- `echo-save-quit-pending/`: quit with the native chooser open closes it,
  cancels the unassigned transfer, leaves canonical inventory empty and exits zero.

`pairs/save-{arc,echo}.png` retain the full native states. Arc had a saved expanded
state and a 680 px compact minimum after collapsing it; the fresh Echo chooser
starts at the platform's 370 px compact size. The shared native controls use the
same 230 px name field and system type. These state-dependent sheet dimensions
are not a claim of identical presentation preferences.

GPUI coverage checks document-bound Save As input without source navigation and
rejects a same-URL document replacement before dispatch. Service coverage checks
captured-profile cancellation, deduplication, genuine terminal authority and
rejection of a chooser reply after cancellation.

## Active progress and file actions

Normal-profile transfers now appear above completed records in Library Downloads.
The copied CEF byte count, total, speed and percentage supply progress and ETA;
none of these metrics authorize publication or completion. macOS byte formatting
uses the native locale (the reference machine displays `8,4 MB`). The active row
is 288 × 56 px, starts at window-relative y=83 and uses a 24 px blue progress
circle. The sidebar Library dock uses the distinct 20 px pale circle with a blue
arrow/ring, rather than reusing the larger blue/white Library artwork.

The active overflow menu matches the inspected order: Open, disabled Copy and
Show in Finder, Cancel Download, and Hide from Echo. Cancel queues the original
opaque backend token and captured profile through the sole consumer. Only its
genuine terminal callback makes the row `Cancelled`; it remains in the list with
muted artwork and never creates a completed record. Open while active waits for
the proof-bound canonical record before handing its final path to the system.
A terminal update dismisses an open active menu so an old selection cannot be
retargeted to a different command. UI keys are separate from engine tokens;
all delayed intents recheck presenter generation, workspace, profile and visible
membership. Hide is presentation state, not cancellation or file deletion.

Strict native evidence:

- `arc-active-cancel-verified-reference/`: the complete native menu and canceled row.
- `arc-download-toolbar-verified/`: active progress with toolbar/sidebar shown and
  sidebar hidden. No separate toolbar download control was observed. Original
  chrome visibility is restored and owned fixture tabs/windows are closed.
- `echo-active-cancel-final/`: genuine InProgress → Canceled, visible muted
  `Cancelled` row, no final file or canonical record, empty service errors,
  CEF Complete and native exit zero. Staged ingress advances the presentation
  revision even when the direct router does not receive that observation.
- `echo-file-actions/`: a genuine canonical Save Image As record reopened in a
  clean profile opens the exact published PNG in Preview and selects that exact
  file in Finder. Only the newly caused document windows are closed; the native
  browser exits zero after CEF Complete.
- `pairs/active-{arc,echo}.png`, `pairs/cancelled-{arc,echo}.png`,
  `pairs/dock-{arc,echo}.png`, and `pairs/menu-{arc,echo}.png`: inspected zoom crops.
  Menu selected-row position is matched; per-Space backdrop colors remain the
  existing material API's responsibility.

GPUI/service regressions cover duplicate-name identity through row reorder,
canceled-row actions, profile rejection, detached-work progress, non-revivable
cancellation, queued Open consuming canonical completion once, and the 20 px
sidebar badge opening Downloads without navigating the retained page.

## Remaining qualification

Fine title truncation and the native menu edge/shadow remain visually qualified.
No generic completion toast text was observed or added. The exact file-flight
completion animation is still partial. The native save sheet inherits per-app
expanded/compact preferences; its overall state-dependent dimensions differ in
the recorded pair. Explicit replacement of an existing chosen file is not
implemented: the staged publisher preserves existing targets. Canceled-row history
is ephemeral. Private/Blank admission, other platforms, filesystem failures,
crash recovery and staging cleanup remain qualified boundaries.
