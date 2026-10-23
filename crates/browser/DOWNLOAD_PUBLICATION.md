# Owned staging and no-clobber publication

`downloads_publication` is an engine-independent filesystem primitive. It is
registered for compilation and tests, but is not connected to destination policy,
AppDownloads, canonical persistence or the Library UI.

CEF's public destination callback does not guarantee that an existing final
target survives. The integration must therefore give CEF an absent payload path
inside a fresh, exclusively created staging directory on the destination
filesystem. An existing final file is never used as a reservation.

## Ownership and ordering

1. Outside application/service/model/engine borrows, reserve staging for the exact
   opaque transfer identity and an injected destination directory and candidate
   list. The primitive does not select Downloads, invent suffixes or infer names.
2. Retain its process-local registration capability. Revalidate the absent stage
   path immediately before the existing native destination effect.
3. Feed only genuine owned engine observations through the sole event adapter.
   Only an exact-key, exact-registration Updated Complete with the unchanged
   native stage path can detach a Publication. Bytes, destination selection,
   percent complete and cancellation cannot substitute for that event.
4. Release all borrows before executing the detached Publication. Target creation
   uses `tempfile::TempPath::persist_noclobber`; an existing target is preserved.
   Only a real AlreadyExists result advances to the next injected candidate.
   Permission, unsupported-filesystem and cross-filesystem errors do not trigger
   a copy, overwrite or guessed fallback.
5. A successful identity-checked operation yields a private PublishedPath proof.
   The future canonical writer must require and revalidate that proof outside
   borrows immediately before recording its final path. The current router does
   not yet do this; wiring staging alone would incorrectly persist the stage
   path. It must remain disabled until the writer integration is complete.

## Failure and retry

The effect sets Attempting before the publication syscall. An uncertain outcome
stays blocked: retrying cannot choose another name and create a second final
link. Published replay validates and returns the same path/identity. A failed
canonical write must retain the publication/proof and retry only that write after
revalidation, rather than publishing again.

Dropping a reservation, effect, failed TempPath or proof closes handles but
deletes no staged data. A successful no-clobber operation may retain a source
hardlink; cleanup and acknowledgment are not one atomic transaction. Recovery
and eventual cleanup need an explicit policy before activation.

## Verification and remaining boundaries

Real-filesystem tests cover existing and late collisions, concurrent publishers,
symlinks, source/directory replacement between effects, ambiguous outcomes,
identity mismatch, idempotence, error/drop retention and native path spelling.
APFS can reject non-UTF-8 filenames: the macOS test requires exact byte retention
and staged-data preservation on errno 92 rather than lossy conversion.

The primitive assumes exclusive control of staging and no hostile replacement
of destination ancestors during an effect. Revalidation detects changes between
effects; it does not make path-based CEF or filesystem calls adversarial-race-free.
Crash recovery, durability, Windows/Linux filesystem behavior, quarantine/xattr
preservation, and service/writer/cancellation/shutdown wiring remain activation
requirements. This module alone does not establish production Downloads parity.

The registered macOS workspace passed 976 tests on October 24, 2026: 709 browser,
165 core and 102 engine. The 24 applicable publication tests include a real
`com.apple.quarantine` attribute and file-permission preservation check. This
proves the tested filesystem operation preserves those attributes, not that a
CEF download has been staged or published through the service.

An isolated bundle smoke check retained one document UUID/load and the input
`NOCLOBBEROK` through Library Media and Command Bar dismissal, then exited 0 after
CEF Complete. The screenshot was inspected. Binary SHA-256:
`ade3d0524ed1d03af3a76434f528933f7e8b6310868ba8db424b4c279238db65`.
Evidence: `/tmp/echo-download-publication-workspace-r2.log` and
`/tmp/echo-publication-native-r2.{log,json,png}`, with the captured fixture reports
in `/tmp/echo-publication-native-r2-reports.json`.
