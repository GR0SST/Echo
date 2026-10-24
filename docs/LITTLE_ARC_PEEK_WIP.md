# Little Arc / Peek implementation notes

## Little Echo

Standalone lookup windows use an independent Blank model and page store in the source website storage context. Cmd+Option+N is handled by the menu, window and native page shortcut paths. External HTTP/HTTPS app events also create lookups. They inherit the source Space theme and use the shared chrome/material APIs. Closing a lookup does not modify regular Pinned/Favorites/Today selections. Launching a Little window from Incognito/Blank Peek uses the retained normal workspace, never the ephemeral Peek profile; the regression covers both origins.

The resting toolbar shows the hostname. Editing and copying retain the full URL. Cmd+O promotes directly; the arrow opens a searchable Space picker with current Space first. Same-profile promotion now adopts the existing CEF browser and native host into a newly allocated canonical Today identity. DOM/form state and back/forward history survive; no replacement browser or navigation is issued. Adoption failure leaves the lookup open and rolls back the unpublished Today tab. Cross-profile adoption is rejected because it must not leak the source website context into another profile. The initial standalone search prompt, exact saved placement/opening motion, Extensions, profile switching and downloads remain unfinished. ARC_PARITY stays partial.

## Measurements and evidence

Installed Arc 1.170.0 at a 1000×760 frame: Little toolbar 46 px; resting address x=83/y=7/height=32; page flush beneath it. Space picker width=244, top/right=4, header/row height=46, scrolling rows about 350 px. The chevrons use fixed vector geometry.

Evidence is retained locally under `target/parity/little-arc-peek/`: `arc-little-current.png`, `arc-little-picker-current.png`, `echo-little.png`, `echo-little-picker.png`, native logs and test logs. Theme/profile/Space data differ between reference and isolated Echo profiles; inheritance is covered by a GPUI regression. No exact material or motion parity claim is made. Native checks cover Escape restoration, expansion into Today and Split View with Pinned originals retained; logs reach CEF Complete. Peek regressions cover routing ownership, both preferences, compact card controls, page shortcuts and source restoration.

## Peek implementation

A valid guarded Shift-click reference has now been captured: `arc-peek.png`, `arc-peek-closed.png`, `arc-peek-open-valid.mov`. With Arc sidebar=228 px and window=1000×760, the card is x=305/y=12/width=618/height=736 (80% of the width after the sidebar). Three 34 px round controls sit vertically to its right, beginning at y=20. The original page shrinks by about 9 px per side and is dimmed. Escape closes Peek. The opening has spring overshoot and settles in roughly 300–400 ms. `arc-peek-close.mov` shows Escape shrinking the card back toward the source link in about 180–200 ms. Native routing now handles user cross-host GET navigation from Pinned/Favorites and Shift-click (CEF NEW_WINDOW), gated by the existing Settings toggles. The child keeps separate page/event ownership and source website storage. Three controls close, promote to Today, or fill a Split View; Pinned/Favorite originals remain intact. Escape is intercepted only in Peek pages; A guarded Arc 1.170.0 reference leaves the card open on Cmd+Escape; Echo now matches that behavior. Closing restores the source selection and native focus. Opening uses a 340 ms damped spring, closing 180 ms; exact motion parity remains unverified. The native source page insets by 9 px while open. Card width is 80% after the sidebar, y=12, radius=8, controls=34 with 9.5 px gaps. Shared material APIs provide tint; no new palette is introduced.

Closing now requests a fresh page photograph, bounded to 100 ms, hides native content and scales the cached image through the 180 ms exit. Failed/unsupported capture retains the previous live fallback. Snapshot success, unchanged live viewport geometry during exit, and missing-response bounds have GPUI regressions. The native hover poll reconciles both edges when CEF consumes pointer transitions. Native pairs are `round2-reference-valid/{idle,sidebar-hover,peek-before,peek-cmdesc,peek-escape}.png` and `round2-hover-echo-{idle,sidebar-hover,peek,cmdesc}.png`. Motion pairs are `round2-resize/close.mov` and `round2-hover-echo-motion.mov`; automation confirms a fresh snapshot and CEF Complete. `round2-final-close-zoom.png` and `round2-arc-close-zoom.png` expose content scaling. Space theme/profile data still differ, so exact tint/shadow parity is not claimed.

Round 2 guarded evidence: `round2-reference-valid/` contains idle/sidebar/address hover and Cmd+Escape/plain Escape states; `round2-global-little/` proves Option+Command+N opens Little Arc while Finder is frontmost. Its prompt is 490×329 before becoming the page window. `round2-resize/resized.png` shows a 1200×800 parent and an updated 778×776 card at x325/y12.

Peek now observes owner bounds and resizes its child viewport while preserving the live document. A GPUI regression checks the resize request, card layout and absence of create/navigate/close calls. Native pair: `round2-resize/resized.png` / `round2-echo-resized.png`; zooms: `round2-resize-{arc,echo}-zoom.png`. Both place the 778×776 card at x325/y12 in a 1200×800 owner. Page zoom/profile/theme still differ.

Remaining: exact spring/close timing and shadow, nested Peek/Little links, Undo Close Peek, and a dedicated Peek Space picker. Native Arc Cmd+Escape was rechecked with a visibly open card and leaves it open; plain Escape dismisses it.

## Native inspection safety

All Arc sessions use the shared guard at `~/Documents/Bot/parity-evidence/tools/arc_guard.py` under the immutable screen lock and a hard timeout. Each starts with snapshot → Cmd+N → claim → Escape, then selects **Little Peek R5** and verifies its header crop against `guard/verified-space.png`. This fresh test Space is the only allowed target; never touch Grosst, Work, Яремко or Playable. Never delete a Space.

The Space creation animation must finish before closing the fresh window. After invoking an Arc native menu through Accessibility, AXCancel dismisses its tracking loop. Command Bar and Space picker may create separate native windows: claim and check their identities before input, and allow dismissal animation to finish before checking the parent. If ownership fails, abort instead of acting on another window. Always close owned lookup/popup windows before releasing the lock. Never invoke Arc’s application AppleScript API (`tell application "Arc"`): use `open -a` and bounded System Events. Old CG IDs recorded before the owner restarted Arc are invalid.

Native Echo tests use scripts/drive.sh with a separate profile in `target/parity/little-arc-peek/drive/`. Run the complete deterministic `RUST_TEST_THREADS=4 cargo test` before pushing. Push only validated pieces under the required fetch/rebase/test/push lock.

The owner’s current run rules override the Ringmind clause: never contact/call the owner; report blockers to the orchestrator. Each Arc session may create at most one window. Install cleanup before Cmd+N; if its window delta is not exactly one, close every window created by that script and abort. Two bounded System Events timeouts or no window after Cmd+N permit restarting Arc inside the screen lock. Never reuse the archived pre-restart CG IDs.

The native pointer bridge reads the source window’s live native pointer in top-left view coordinates. CEF page clicks can leave GPUI’s cached pointer stale; close motion must not use that cached location on macOS. Other platforms retain the toolkit fallback. `drive position` is bottom-left AppKit coordinates: on the 3840×2160 primary display use `position 100 1300` for a 1000×760 window at screen top-left (100,100). Echo recordings made with `-R100,100` while positioned at AppKit (100,100) are invalid and named accordingly.

Peek retains its render marker throughout asynchronous native close preparation. Clearing that marker early briefly rendered the Little toolbar while CEF retired the child; only its parent attachment is detached at close admission.

Closing an owner requests the Peek child’s own asynchronous close boundary rather than directly removing its native window; the child page host retires even when a caller retains the GPUI Entity.


## Live page adoption (Round 2)

The engine moves the same CEF browser/clip host between independent pools, allocates a new pool-local ViewId and redirects callbacks through their original router. Repeated moves and returning to the original pool are covered; queued old-attachment input is discarded. Pages explicitly remaps unclaimed metadata to the new canonical identity. The original request-context lease remains with the same document. Inspector-open documents reject adoption; no reload fallback is used. Transfer is limited to the same user profile.

Core exposes a guarded lookup-adoption transaction, restoring selection without archives/Undo when native adoption fails. GPUI regressions cover Peek and Little live promotion, renderer history, adoption rejection/retry and pending metadata. Split promotion uses the same live transfer.

Arc reference: `round2-transfer-reference-b/{edited,nested,history-back,undo-close,expanded}.png`. Echo: `round2-echo-live/{peek-history-back,peek-expanded,peek-forward,peek-back-after-expand,little-history-back,little-promoted,little-forward,little-back-after-promotion}.png`. Text survives promotion and both history directions. Server `/dest` counts rise 2→4 only for the initial Peek/Little loads; `/nested` rises 1→3 only for the initial nested navigations. No promotion/history request occurs. The isolated run reaches CEF Complete. Arc enables its localhost developer toolbar after expansion; that separate feature is outside this change.


## System-wide shortcut (Round 2)

The default command/option N uses the global-hotkey provider on the UI loop. Registration follows the retained normal workspace's `LittleWindowShortcut` preference; disabling releases the binding. Conflicts retry every five seconds without repeating identical diagnostics. One press opens one lookup; repeat/release/unrelated/disabled events have unit coverage. A GPUI regression checks live preference changes. Headless tests never register a real OS shortcut.

Arc reference `round2-global-little/{prompt,page}.png` was opened while Finder was frontmost. Echo proof `round2-echo-global-context-final/{prompt,page}.png` uses real System Events physical key code 45 while Finder is frontmost, with an additional Ctrl modifier reserved for automation because Arc owns the default during reference work. The lookup receives URL input and loads the page; the run reaches CEF Complete and creates zero Arc windows. Production ignores the test modifier unless `BROWSER_SCRIPT` is present. The provider supplies Windows/macOS/X11 adapters; Windows/X11 remain unverified natively and Wayland is unsupported. Custom key recording is not implemented.

The initial prompt still differs: Echo opens its full 1000×760 lookup with the ordinary Command Bar, versus Arc's 490×329 standalone prompt. Global entry is functional; no initial-prompt visual parity is claimed.
