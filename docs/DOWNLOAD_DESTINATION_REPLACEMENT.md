# Deliberate download destination replacement

Arc's native conflict sheet offers Cancel and Replace. Ordinary quiet duplicate
names keep both files through a free suffix. Echo reuses the native save chooser
and its overwrite confirmation; it does not add another conflict picker or an
invented Keep Both button.

An accepted chooser destination can reserve an existing regular file. The
reservation retains its file identity, length and modification time. CEF still
receives an absent payload path in a fresh private staging directory on the
same filesystem, never the existing final file. Destination acceptance, byte
counts and cancellation cannot publish that payload. The unchanged opaque
registration, provenance and exact staged path must receive a genuine Complete
through the existing service adapter before publication can run.

Only this chooser reservation can replace its exact chosen target. Publication
revalidates the original file immediately before atomic replacement. A changed
file identity or changed length/modification time fails closed and keeps the
staged bytes. A symlink is rejected. A destination absent when accepted retains
no-clobber semantics even if another file appears later. Quiet downloads always
use the original no-clobber candidate list; no replacement grant is minted.
Published-path identity proof and canonical inventory admission are unchanged.

This extends the existing path-based primitive, with the same exclusive staging
and non-hostile filesystem assumptions. The target identity check and rename
are not a single filesystem compare-and-swap. Crash recovery, staging cleanup
and native Windows/Linux filesystem acceptance remain separately qualified.

## Verification

The Arc Cancel/Replace reference is
`target/parity/download-replacement/arc-replace-confirmation.png`. The reference
uses a synthetic existing HTML file, confirms that Cancel leaves its bytes
unchanged, then removes only that scratch file. Its owned window is closed.

Real-filesystem tests cover genuine-completion ordering, idempotent publication,
changed/replaced chosen targets, late collisions without replacement authority,
last-moment identity checks and symlink rejection, alongside all existing
no-clobber, provenance, permissions and ambiguity tests.

Accepted native evidence is under
`target/parity/download-replacement/echo-native-v5/`: `accepted.json` records
exact replacement bytes and one canonical result; `quiet-accepted.json` records
three distinct preserved payloads and their exact final paths/names after two
quiet duplicates. Both runs reach CEF Complete and exit zero.
`echo-replace-confirmation.png` captures the actual native conflict;
`quiet-library.png` captures the completed inventory. The inspected 2× pair is
`../arc-replace-crop.png` and `../echo-replace-crop.png` relative to that directory.
System alert height follows the filename/folder text rather than a custom layout.
The full workspace and all 29 focused publication tests pass.
