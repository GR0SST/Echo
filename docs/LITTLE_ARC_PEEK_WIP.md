# Little Arc / Peek implementation notes

## Little Echo

Standalone lookup windows use an independent Blank model and page store in the source website storage context. Cmd+Option+N is handled by the menu, window and native page shortcut paths. External HTTP/HTTPS app events also create lookups. They inherit the source Space theme and use the shared chrome/material APIs. Closing a lookup does not modify regular Pinned/Favorites/Today selections.

The resting toolbar shows the hostname. Editing and copying retain the full URL. Cmd+O promotes directly; the arrow opens a searchable Space picker with current Space first. Promotion currently creates a regular page from the URL, so the live document and navigation history are not transferred. Global shortcut registration, the initial standalone search prompt, exact saved placement/opening motion, Extensions, profile switching and downloads remain unfinished. ARC_PARITY stays partial.

## Measurements and evidence

Installed Arc 1.170.0 at a 1000×760 frame: Little toolbar 46 px; resting address x=83/y=7/height=32; page flush beneath it. Space picker width=244, top/right=4, header/row height=46, scrolling rows about 350 px. The chevrons use fixed vector geometry.

Evidence is retained locally under `target/parity/little-arc-peek/`: `arc-little-current.png`, `arc-little-picker-current.png`, `echo-little.png`, `echo-little-picker.png`, native logs and test logs. Theme/profile/Space data differ between reference and isolated Echo profiles; inheritance is covered by a GPUI regression. No exact material or motion parity claim is made.

## Peek work in progress

A valid guarded Shift-click reference has now been captured: `arc-peek.png`, `arc-peek-closed.png`, `arc-peek-open-valid.mov`. With Arc sidebar=228 px and window=1000×760, the card is x=305/y=12/width=618/height=736 (80% of the width after the sidebar). Three 34 px round controls sit vertically to its right, beginning at y=20. The original page shrinks by about 9 px per side and is dimmed. Escape closes Peek. The opening is a spring with overshoot; timing analysis and implementation are pending. Pinned/Favorite routing, expansion and Split View require further verification.

## Native inspection safety

All Arc sessions use the shared guard at `~/Documents/Bot/parity-evidence/tools/arc_guard.py` under the immutable screen lock and a hard timeout. Each starts with snapshot → Cmd+N → claim → Escape, then selects **Little Peek R5** and verifies its header crop against `guard/verified-space.png`. This fresh test Space is the only allowed target; never touch Grosst, Work, Яремко or Playable. Never delete a Space.

The Space creation animation must finish before closing the fresh window. After invoking an Arc native menu through Accessibility, AXCancel dismisses its tracking loop. Command Bar and Space picker may create separate native windows: claim and check their identities before input, and allow dismissal animation to finish before checking the parent. If ownership fails, abort instead of acting on another window. Always close owned lookup/popup windows before releasing the lock.

Native Echo tests use scripts/drive.sh with a separate profile in `target/parity/little-arc-peek/drive/`. Run the complete deterministic `RUST_TEST_THREADS=4 cargo test` before pushing. Push only validated pieces under the required fetch/rebase/test/push lock.
