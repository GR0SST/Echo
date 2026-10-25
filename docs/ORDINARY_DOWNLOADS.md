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

## Remaining integration

The saved Ask where to save preference currently defers requests; the asynchronous
chooser, explicit Save Link/Image As path, dock progress/animation, active Library
rows and cancellation are the next integration slice. No generic completion toast
text was observed or added. Other lifetimes/platforms and filesystem failure,
crash recovery and staging cleanup remain separately qualified boundaries.
