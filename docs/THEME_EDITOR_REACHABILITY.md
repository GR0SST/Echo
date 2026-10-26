# Theme editor reachability

The guarded Arc 1.170.0 reference uses the shared Echo Test Space, a
1000 × 760 browser at screen (100, 100), and the 1920 × 1080 display.
Opening Spaces → Edit Theme with the pointer at screen (950, 830) moves the
panel upward: its padded native surface is (328, 548, 379, 530), ending at
1078 instead of extending below the screen. The previously measured content
is 359 × 512, with 9 px padding; the remaining 2 px clearance motivates the
content's 11 px screen inset. This is a measured placement inference, not a
new layout. The screen's native visible area starts at y30.

Library → Spaces → the Echo Test card's pencil opens the same editor beside
that card, rather than beside the browser sidebar. Its top is clamped below
the menu bar. The inspected card is the second card. No theme, profile,
Space assignment or owner tab was changed; both owned reference windows and
editor popovers were closed. The first Library tree request timed out and
was replaced with a bounded, exact-window accessibility walk.

References under `target/parity/partial-closeout/`:

- `arc-f70-edge-r2.log`, `arc-f70-edge-receipt.json`, `arc-f70-edge.png`.
- `arc-f70-library-r2.log`, `arc-f70-library-ax.txt`.
- `arc-f70-pencil-r2.log`, `arc-f70-pencil-receipt.json`, `arc-f70-pencil.png`.

Echo now clamps the full editor content to the available display area, with
macOS's menu bar/Dock excluded by the native work-area query. Windows/Linux still use GPUI's display bounds; their reserved-work-area
adapters remain a functional gap. Only the
transparent child extends beyond the browser; the host and its retained
page stay in place. Library pencils and footer-menu actions read the painted
bounds of their captured Space card. Editing a different card does not select
it or apply its pigment to the active Space.

Three GPUI regressions exercise both vertical edges, the reachable palette and
appearance controls, dismissal, and both Library entry points. The existing
header-anchor test now expects screen clamping when its test host starts at
the display's top edge; the ordinary three-position pointer reference remains
unchanged. Exact material, shadow, pigment rendering and small placement
residuals are cosmetic and remain partial.

A separate focus regression prevents both retired-popup restoration and sidebar
focus deferral from focusing the Library root over a live theme editor. Removing
the guard fails on the resulting root focus, not on an implementation flag;
`f70-focus-red.log` preserves that check.

Native acceptance: `f70-qualified.log`, `f70-qualified-receipt.json`, and
`f70-{edge,pencil}-qualified-owner.json`. The documented native NSEvent driver
moves the physical pointer before opening each editor. Both entries preserve the
1000 × 760 host; a palette click persists to Echo Test, the document loads once,
and native termination completes without a dirty marker. The disposable profile
disables Warn Before Quitting; no live preference is changed. Paired captures
are `pair-f70-{edge,pencil}.png` and the inspected `-2x.png` crops. These establish
placement and reachability, not cosmetic equivalence.

Preliminary global-CG and coordinate-only prototypes are retained as failed,
unqualified probes; they did not persist a palette edit. One native-script
prototype also kept the old physical pointer and therefore opened the panel at
a different vertical anchor. They are not accepted interaction evidence. The
qualified driver explicitly moves the pointer and checks the saved model.
The Arc pencil capture succeeded before a subsequent root-only capture was
rejected by its guard; its finally block closed both owned windows.

Validation: `f70-final-full.log` (including 1,397 browser tests),
`f70-final-targets.log` (Linux GNU and Windows MSVC).
