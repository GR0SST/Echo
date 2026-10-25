# Sidebar drag scrolling — Arc 1.170

Reference window: `{100,100,1000,760}`, 228 px sidebar, shared **Echo Test** Space. Evidence: `target/parity/command-sidebar-r2/`.

## Measured behavior

Arc's common Pinned/Today scroll viewport is window-relative `{0,181,228,529}`. Favorites, the Space header and the 50 px bottom dock remain stationary. Pinned rows, the Today divider, New Tab and Today rows move together. Echo previously capped Pinned at 45% and scrolled the sections separately; an additional 8 px gap before its dock also shortened the lower drag hit region.

Holding a tab at window-relative `{150,699}` starts sustained scrolling after approximately 200 ms. Arc's tracked row moves 39 px in 0.661 s, 66 px in 1.060 s, 84 px in 1.426 s and 81 px in 1.369 s: approximately 60 px/s. See `target/parity/reaudit2/sessions/command-sidebar-r2-drag-measure/drag-timeline.json`. Additional edge probes are retained in sibling `command-sidebar-r2-*edges*` and `*-edge-probe` session directories. The complete speed curve is not yet qualified.

Echo now uses one viewport, preserves the existing row positions with the reference's 4 px upper padding, and scrolls while the pointer stays still. It applies the measured delay and speed at the reference point. Pointer exit, drop, Escape and invalid ownership stop scrolling; scroll offsets clamp at either end. Revealing a tab scrolls its actual row within the common viewport. Both GPUI and renderer-origin Escape cancel the drag without a subsequent mouse-up reordering the tab.

## Remaining parity gaps

The dragging row stays **partial**. Arc's floating row ghost crosses the page boundary, includes a close glyph and uses different geometry/material; Echo's existing row ghost is clipped by the native page. Arc also shows an upper edge arrow and scroll thumb. The extreme-edge speed curve, folder drags and the dismissed-empty-Favorites drag state need further positive comparison. No claim is made that the entire drag surface matches.

## Safety and validation

Arc drag sessions claim only their root window and the two directly caused edge panels (80 × 760 and 30 × 760). Their `finally` path sends Escape and releases the held mouse button even if cancellation raises. CGWindowList verifies that the panels and every large Arc layer above layer 0 are gone before the screen lock is released. All fixture titles use random tokens and are archived by exact title, newest first. No Spaces are created or deleted.

Regression coverage exercises shared scrolling with a fixed header, stationary-pointer scrolling with multiple Spaces, renderer-origin Escape and subsequent mouse-up, viewport boundaries, clamping and pointer exit. Existing reveal tests now assert against the actual common viewport. Renderer identities and navigation remain unchanged.
