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

The initial inspected public-row pair exposes an existing presentation difference:
Arc labels this generic data file `Download`, while Echo currently labels it
`File from proof.ovh.net`. This and the active surface belong to the next slice.

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

## Remaining integration

Active Library rows/cancellation, dock progress/animation and final presentation
comparison remain the next slice. No generic completion toast text was observed
or added. Explicit replacement of an existing chosen file is not implemented:
the staged publisher still preserves existing targets. Other lifetimes/platforms,
filesystem failure, crash recovery and staging cleanup remain qualified boundaries.
