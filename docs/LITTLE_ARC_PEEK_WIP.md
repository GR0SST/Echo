# Little Arc / Peek implementation notes

## Little Echo

Standalone lookup windows use an independent Blank model and page store in the source website storage context. Cmd+Option+N is handled by the menu, window and native page shortcut paths. External HTTP/HTTPS app events also create lookups. They inherit the source Space theme and use the shared chrome/material APIs. Closing a lookup does not modify regular Pinned/Favorites/Today selections.

The resting toolbar shows the hostname. Editing and copying retain the full URL. Cmd+O promotes directly; the arrow opens a searchable Space picker with current Space first. Promotion currently creates a regular page from the URL, so the live document and navigation history are not transferred. Global shortcut registration, the initial standalone search prompt, exact saved placement/opening motion, Extensions, profile switching and downloads remain unfinished. ARC_PARITY stays partial.

## Measurements and evidence

Installed Arc 1.170.0 at a 1000×760 frame: Little toolbar 46 px; resting address x=83/y=7/height=32; page flush beneath it. Space picker width=244, top/right=4, header/row height=46, scrolling rows about 350 px. The chevrons use fixed vector geometry.

Evidence is retained locally under `target/parity/little-arc-peek/`: `arc-little-current.png`, `arc-little-picker-current.png`, `echo-little.png`, `echo-little-picker.png`, native logs and test logs. Theme/profile/Space data differ between reference and isolated Echo profiles; inheritance is covered by a GPUI regression. No exact material or motion parity claim is made. Native checks cover Escape restoration, expansion into Today and Split View with Pinned originals retained; logs reach CEF Complete. Peek regressions cover routing ownership, both preferences, compact card controls, page shortcuts and source restoration.

## Peek implementation

A valid guarded Shift-click reference has now been captured: `arc-peek.png`, `arc-peek-closed.png`, `arc-peek-open-valid.mov`. With Arc sidebar=228 px and window=1000×760, the card is x=305/y=12/width=618/height=736 (80% of the width after the sidebar). Three 34 px round controls sit vertically to its right, beginning at y=20. The original page shrinks by about 9 px per side and is dimmed. Escape closes Peek. The opening has spring overshoot and settles in roughly 300–400 ms. `arc-peek-close.mov` shows Escape shrinking the card back toward the source link in about 180–200 ms. Native routing now handles user cross-host GET navigation from Pinned/Favorites and Shift-click (CEF NEW_WINDOW), gated by the existing Settings toggles. The child keeps separate page/event ownership and source website storage. Three controls close, promote to Today, or fill a Split View; Pinned/Favorite originals remain intact. Escape is intercepted only in Peek pages; Cmd+Escape is routed as a browser action. Closing restores the source selection and native focus. Opening uses a 340 ms damped spring, closing 180 ms; exact motion parity remains unverified. The native source page insets by 9 px while open. Card width is 80% after the sidebar, y=12, radius=8, controls=34 with 9.5 px gaps. Shared material APIs provide tint; no new palette is introduced.

Remaining: exact spring/close timing and shadow (the native page currently reflows during the shrinking close, instead of compositor-scaling its contents), resize synchronization, nested Peek/Little links, Undo Close Peek, live page/history transfer, and a dedicated Peek Space picker. Native Arc Cmd+Escape did not dismiss the card in the latest bounded session; treat this shortcut’s reference behavior as unresolved.

## Native inspection safety

All Arc sessions use the shared guard at `~/Documents/Bot/parity-evidence/tools/arc_guard.py` under the immutable screen lock and a hard timeout. Each starts with snapshot → Cmd+N → claim → Escape, then selects **Little Peek R5** and verifies its header crop against `guard/verified-space.png`. This fresh test Space is the only allowed target; never touch Grosst, Work, Яремко or Playable. Never delete a Space.

The Space creation animation must finish before closing the fresh window. After invoking an Arc native menu through Accessibility, AXCancel dismisses its tracking loop. Command Bar and Space picker may create separate native windows: claim and check their identities before input, and allow dismissal animation to finish before checking the parent. If ownership fails, abort instead of acting on another window. Always close owned lookup/popup windows before releasing the lock. Never invoke Arc’s application AppleScript API (`tell application "Arc"`): use `open -a` and bounded System Events. Old CG IDs recorded before the owner restarted Arc are invalid.

Native Echo tests use scripts/drive.sh with a separate profile in `target/parity/little-arc-peek/drive/`. Run the complete deterministic `RUST_TEST_THREADS=4 cargo test` before pushing. Push only validated pieces under the required fetch/rebase/test/push lock.

The owner’s current run rules override the Ringmind clause: never contact/call the owner; report blockers to the orchestrator. Each Arc session may create at most one window. Install cleanup before Cmd+N; if its window delta is not exactly one, close every window created by that script and abort. Two bounded System Events timeouts or no window after Cmd+N permit restarting Arc inside the screen lock. Never reuse the archived pre-restart CG IDs.
