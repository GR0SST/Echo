# Local Library media backend

The Media route discovers files only after user approval. Opening Media requests
native directory selection for Documents, Desktop and Downloads, one sheet at a
time. Cancel/deny is persisted per profile, so reopening does not repeat prompts.
Advanced Settings → Library Media access allows revoking or explicitly granting
each folder. Unknown or denied folders are never scanned or watched. Profile-owned
Captures do not require access to external folders.

The populated presentation uses existing Library surfaces and metrics, with a
virtualized two-column grid grouped by source, literal-name search, keyboard
selection and the shared rounded preview. Open, Show in Finder, Copy, macOS
native file dragging and Move to Trash + Undo target captured file identities.
Search input keeps its own text Undo; file selection routes ⌘Z to Trash Undo.
Revocation stops the discovery session and clears its rows and thumbnail cache,
including while Library is hidden. Delayed native replies cannot undo revocation
or grant access to a different profile. No scan starts at browser launch.

**populated layout unverified — no Arc reference available**. The row remains
partial. Videos currently show a poster with external playback. Windows/Linux
system drag and native movie thumbnails remain unqualified. PDF/SVG and Hide
from Arc remain outside the implemented subset; deleting is explicitly labeled
Move to Trash.

## Implemented boundary

`browser_core::media` holds metadata, portable image/movie classification,
name search, identity-based selection and inventory reconciliation. PDF and SVG
are excluded; the accepted extension set is a conservative implementation set,
not a verified list of Arc's supported formats.

`echo_media` implements:

- Known Folder / XDG resolution via `FolderPlatform`, without directory reads.
- A scan/watch capability constructed from explicitly approved system-folder
  selections. The host acquires that selection from the native OS
  prompt **only after the user opens Media or explicitly enables a Settings grant**. Cancel/deny must not create a
  capability. Merely resolving Documents/Desktop/Downloads grants no access.
- Recursive discovery capped at 20,000 inspected entries per root and eight levels,
  skipping hidden paths, symlinks and application/document packages. Errors for
  one root do not suppress the other roots. Capture discovery reads only the
  named files in the captured profile's model, independent of external access.
- One background discovery worker, coalesced native filesystem events, one
  replaceable snapshot and a 30-second metadata fallback for watcher failure or
  recreated roots. Dropping the consent session cancels further work.
- Two thumbnail workers and a 32-slot request queue; no unbounded pending jobs.
  The LRU cache retains at most 96 results / 16 MiB of decoded pixels, including
  negative caching. Raster decoding limits source size, allocation/dimensions
  and output edge (512 px maximum). macOS Quick Look supplies unsupported image
  and movie thumbnails with a three-second request deadline. Windows/Linux use
  the portable raster decoder; movie/HEIC native thumbnail backends remain open.
- Stamp validation before/after reads and before file operations. Stamps include
  canonical path, size, modification time and Unix/Windows file identity when
  available, so a refreshed row cannot redirect an operation to a replacement.
- System Trash and an opaque Undo receipt. macOS captures the exact resulting
  Trash URL through NSFileManager, without Finder AppleEvents or enumerating
  unrelated Trash entries. Undo refuses an occupied original path; an exclusive
  same-volume hard link preserves attributes and prevents a racing overwrite.
  Windows/Linux use the OS Trash backend/receipt and collision-checked restore.

There is no network service or upload. Directory grants are stored in local
profile settings. The current macOS bundle is not sandboxed; native OS grants and
selected absolute paths persist without security-scoped bookmarks. A sandboxed
shipping bundle would require the platform access shim to retain bookmarks.

## Reference blocker and local evidence

Arc 1.170 in Echo Test remained empty with a dedicated fixture subfolder and
with eight uniquely prefixed image/movie fixtures temporarily placed directly
in Downloads. A further attempt attached screenshot/download metadata and
imported the fixtures into Spotlight; `mdls` confirmed `public.image`, but Arc
still returned no matching rows. Every fixture was removed in the same script's
finally block after prefix + SHA-256 verification. No other file was opened,
moved or deleted. Learn more opened an unavailable Arc Help Center article.

Evidence is **local only** under `target/parity/media-populated/`: empty/search
captures, fixture cleanup records, scoped fixture metadata and native check logs.
No populated Arc/Echo pair exists. These captures contain no visible owner file
items; any future populated captures must retain only fixture crops. Do not
commit or publish screenshots containing the owner's real downloads.

## Validation

Tests use TempDir fixtures and a fake Trash, never the user's folders or Trash.
They cover route-only consent, persistence/revocation and stale replies, preview
geometry, keyboard/file actions, fake-Trash Undo collisions, classification/search,
replacement-safe selection, bounds, symlink
and package exclusion, watcher create/rename/remove, profile-local Captures,
thumbnail output/eviction/coalescing, and collision-safe Trash/Undo.

Explicit native checks (dedicated fixture paths only):

```
cargo run -p echo_media --example inspect -- /fixture/image.png /fixture/video.mov
cargo run -p echo_media --example trash_undo -- /fixture/echo-media-fixture-trash.png
```

The macOS checks produced PNG/JPEG 160×107 and MOV 160×90 thumbnails, and restored
exact fixture bytes after Trash/Undo. Windows/Linux native checks remain pending.
