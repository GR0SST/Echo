# Local page capture

Reference sessions use the installed Arc and the existing Echo Test Space.
`target/parity/site-controls/other-capture2/capture.png` shows selection;
`other-capture10/captured.png` shows its action menu;
`other-capture11/saved.png` shows the checked Saved to Library state. The page is
our local fixture, not an owner's site. No permissions or zoom were changed.

The observed selection dims the page, outlines the hovered element or dragged
area, shows a camera cursor, and places the 355 × 67 instruction pill at the page
bottom. The action panel is 273 px wide with 40 px rows. Echo implements this in
GPUI over the retained CEF page. Display P3 reference colors are converted to
sRGB before being supplied to GPUI. The rounded selection is subtracted from the
mask, and shadows are clipped outside it so captured content stays unshaded.

Implemented entry points: Shift-Command-2, Command Bar → Capture, and the URL
field's Capture… context-menu item, and the Site Control Center camera tile. Drag selection, main-frame element hover,
Copy, Save…, Retake, Escape/Command-W, and Save to Library are connected. Saved to
Library is an idempotent checked state (confirmed by clicking Arc’s checked row again); local image bytes and a stable model
record are written beside the canonical session in its profile's Captures folder.
A window-only model without a session directory cannot publish Library data.
No global/default profile directory is used as a fallback for those writes.

A request retains its page identity, profile, URL, document epoch, viewport and
zoom. Closure, navigation (including same-URL reload), profile reassignment or a
viewport/zoom change invalidates it. Captures never adopt a screenshot already in
flight before their document scope was established. PNG cropping uses the actual
screenshot-to-logical-viewport ratio, preserving Retina pixels. Clipboard and
Library saves use the same cropped bytes. No extra browser, navigation, remote
port or inspector window is created to capture a page.

Validation: Core tests cover profile persistence, legacy defaults, stable IDs and
invalid paths. GPUI tests drive the shortcut and drag, verify Retina crop pixels,
copy/save/idempotence, Retake/Escape, same-URL replacement and a pre-existing
screenshot race. An isolated CEF bundle verifies the real clipboard, matching
saved pixels, canonical session metadata, retake/dismissal and clean shutdown.
Evidence is `target/parity/other/capture-native4/` (selecting, preview, saved,
retake, copied PNG, profile and app log); the native runner is alongside it.

## Remaining reference and implementation gaps

After Arc displayed Saved to Library, a new guarded session's Media view still
showed Nothing here yet (`target/parity/site-controls/other-media2/media.png`).
We have no populated Media reference. Echo retains its existing empty Media
presenter; presenting/enumerating saved captures is not implemented or claimed as
parity. The local files and canonical inventory are present and persisted.

Destination search, Edit, New Easel and system-message/share handoffs are not
connected. The Send To… field retains text but does not route a destination.
Those secondary actions, full-page capture, private/Blank Library behavior,
frame/shadow-DOM picking, exact remaining glyph/font/motion details and native
Save… panel presentation need further work. Cloud/gallery publishing is excluded.

The Round 2 SCC entry qualification is recorded in
[the closeout review](PARTIAL_ROW_REVIEW.md#round-2--scc-region-capture-entry). A
fresh guarded Arc click enters selection; the Echo native fixture verifies the
formerly inert tile, Escape, subsequent Command Bar navigation and exit 0. The
source/echo entry pair is evidence for behavior, not completion of cosmetic or
remaining full-page/destination features.
