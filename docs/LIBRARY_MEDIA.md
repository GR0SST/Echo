# Local Library media backend

The browser presenter is **not wired to discovery yet**. Opening Media currently
retains its existing empty/search-miss presentation. No user-folder scan,
watcher, permission request, or thumbnail worker starts at browser launch or on
an existing Library route. The Media parity row remains partial.

## Implemented boundary

`browser_core::media` holds metadata, portable image/movie classification,
name search, identity-based selection and inventory reconciliation. PDF and SVG
are excluded; the accepted extension set is a conservative implementation set,
not a verified list of Arc's supported formats.

`echo_media` implements:

- Known Folder / XDG resolution via `FolderPlatform`, without directory reads.
- A scan/watch capability constructed from explicitly approved system-folder
  selections. The future host must acquire that selection from the native OS
  prompt **only after the user opens Media**. Cancel/deny must not create a
  capability. Merely resolving Documents/Desktop/Downloads grants no access.
- Recursive discovery capped at 20,000 inspected entries and eight levels,
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

There is no network service or upload. Native permission UI, persisted grants,
platform security-scoped bookmarks, the populated GPUI layout/sections/selection
and preview, Open/Reveal/Copy/drag-out/Hide actions and Trash/Undo UI are **not
implemented** by this backend commit. Do not call this populated Library parity.

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
They cover classification/search, replacement-safe selection, bounds, symlink
and package exclusion, watcher create/rename/remove, profile-local Captures,
thumbnail output/eviction/coalescing, and collision-safe Trash/Undo.

Explicit native checks (dedicated fixture paths only):

```
cargo run -p echo_media --example inspect -- /fixture/image.png /fixture/video.mov
cargo run -p echo_media --example trash_undo -- /fixture/echo-media-fixture-trash.png
```

The macOS checks produced PNG/JPEG 160×107 and MOV 160×90 thumbnails, and restored
exact fixture bytes after Trash/Undo. Windows/Linux native checks remain pending.
