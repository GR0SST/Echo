# Air Traffic Control — Arc 1.170.0, 2026-10-24

Reference: installed `/Applications/Arc.app`, Settings → Links → Air Traffic
Control and the “Air Traffic Control” Command Bar action. Three guarded,
bounded sessions used one new window each and the agent's Echo ATC Oct24 Space.
All new preferences/sheet windows were guarded as children created during the
session; Escape dismissed menus/sheets before closing the owned main window.
Owner windows, Spaces and routing configuration were preserved. The blank New
Route was cancelled without typing. The stored route IDs, source conditions,
destinations and default were verified unchanged afterward (JSON key ordering
may change when Arc persists its state).

`target/parity/atc/arc-atc.png`, `arc-new-route.png`, `arc-match-menu.png`,
`arc-destination-menu.png`, `arc-default-menu.png` and corresponding AX dumps
record the controls. Arc's route sheet is 510×500, positioned 88 px below the
settings frame top, with 18 px corners, #141c31 sRGB surface, 24 px horizontal padding,
14 px semibold title and 13 px medium fields. The header is 84 px high; its
122×28 New Route button and close control sit above a hairline. Rules use two
24 px rows with a 4 px gap and 24 px between rules. URL rows have an 80 px
label, 111 px matcher, flexible text field and a minus-circle delete control.
Destination buttons size to their menu labels, capped at 355 px; the Open in
row is centered as a group. The 90 px footer centers its intrinsic button in a
270×24 slot. Selector surfaces are #272d41 sRGB; text fields #16171a.

New Route adds an inline blank row: Contains, `arc.net` placeholder, Most Recent
Space destination. No separate Save or Cancel button is present; Escape closes
the sheet and discards the blank route. Existing rows are editable in place.
Matchers are exactly “Is equal to” and “Contains”. Both destination menus show
Spaces as `Space (Profile)`, followed by a separator, Most Recent Space and
Little Arc. The installed editor exposes no app-source, enable/disable,
standalone profile target or reorder controls; Echo must not invent those UI
controls. Requested source-application, domain, profile, ordering and enable
state are supported in the core routing model.

The bundle's LinkRoutingEditor reflection lists `rules`, `defaultDestination`,
`focusedRuleID`, `addRule`, `updateDestination`, `updateTextMatchingStyle`,
`updateTextMatchingPattern`, `delete`, `submit`, `matches` and `contains`.
`StorableLinkRouting.json` confirms the ordered rule array and independent
default destination. No Arc assets or account/cloud services are used.

## Model and delivery

The install's canonical normal session stores routing once, shared across local
website profiles and windows. Ordered enabled rules use the first valid target;
deleted destinations fall through. Empty predicates and non-HTTP(S) inputs do
not match. A domain predicate observes hostname boundaries, not substring
suffixes. Old sessions retain their Little-window-links preference; choosing a
new default keeps that toggle consistent.

The OS open-URL callback captures the source bundle identifier synchronously
from its Apple Event sender PID on macOS. Other platforms may supply an
identifier through the shared routing entry point; absent identifiers never
match app-source rules. Links route into normal Space/profile storage or a
separate Little window. Private and Blank windows never become the external
routing source. Little promotion resolves the current URL's route, while an
explicit picker destination takes precedence. Crossing website profiles opens
the URL in destination storage and does not transfer source cookies.

Model tests cover order/disable, domains, empty/unsafe inputs, source application,
profile destination and session round trips. GPUI tests exercise external
routing, app-source/profile selection and Little promotion. Native model evidence:
`target/parity/atc/model-space.png`, `model-little.png`, `native-model.log`.
The fixture receives real PID-targeted GURL Apple Events, without changing the
system default browser or delivering URLs to another Echo process.

## GPUI sheet

The Links action and Command Bar open the same singleton settings window, select
Links and present a modal GPUI sheet. The New Route row is local while blank;
Escape/close discards it. Nonempty edits, matcher/destination changes, deletion
and the default persist through the canonical install model. Menus support
clicks, arrow keys, Enter and Escape; another Escape closes the sheet and returns
focus. The backdrop blocks the settings toolbar. Echo uses “Little Echo” and
`echo.local` for the reference's branded “Little Arc” and `arc.net` placeholder.
No separate app-source/profile/enable/reorder controls are invented.

A later bounded recording used the shared “Echo Test” Space, reusing its recorded
header crop, and archived only its own about:blank tab before closing all owned
windows. No replacement Space was needed. `arc-sheet-entry.mov` and
`sheet-entry-motion.json` record about 24 px of descent over roughly 200 ms;
Echo uses a cosine ease and early surface fade, with reduced motion respected.
Nine GPUI interaction regressions cover the model delivery cases plus sheet
entry, blank draft cancellation, modal isolation, matcher/destination keyboard
selection, default/toggle consistency, deletion and cross-profile promotion.
GPUI's debug-bounds cache retains removed selectors; negative visibility checks
read the panel state after exercising the real controls instead.


Native editor evidence: `echo-atc.png`, `echo-new-route.png`,
`echo-match-menu.png`, `echo-destination-menu.png`, `echo-default-menu.png`,
`echo-cancelled-draft.png`, `echo-saved-route.png`, `ui-external-route.png` and
`native-ui.log`. The native sequence creates a route through the real controls,
reopens to verify blank cancellation, and delivers a real GURL event to its
private Echo PID; the URL arrives in Work. It completes CEF shutdown normally.
The Command Bar-to-Settings handoff suppresses old page focus restoration;
existing patterns initialize with a caret at the end and do not write on view.

Colors are sampled after converting the captures' Display P3 ICC profile to
sRGB. For example, the native menu highlight's P3 (52,120,246) is sRGB #007aff.
`arc-system-menu-highlight.png` reuses the installed Arc native menu reference
from the sidebar worktree's `arc-r2-share-submenu.png`; no new Arc state changes
are needed for that standard NSMenu color. The optional ATC-specific highlight
session stopped at the unchanged header guard after toolbar mode moved the
header; a reversible shortcut reset was attempted and restored. No replacement
Space or alternate named test Space was created during these shared-Space runs.

Remaining visual gaps: native menu vibrancy, exact long-menu placement beyond
the settings frame, and the unmeasured sheet exit transition. The underlying
Links illustration/toggle styling remains the existing partial settings work.
Light appearance and Windows/Linux native presentation have not been checked.
Non-macOS callers currently provide no source application identifier, so those
rules fall through unless an identifier is supplied to the shared entry point.

The Command Bar uses a portable horizontal airplane contour for the entry and
selected input prefix, referenced by `arc-command.png`; the white tile’s airplane
is ICC-converted sRGB #6e74ff. `echo-command.png` records the native entry.

Validation: `RUST_TEST_THREADS=4 cargo test` passes 929 browser, 190 core and
120 engine tests. `pairs.png` compares 2× intrinsic sheet, New Route and matcher
crops; `arc-sheet-zoom.png` and `echo-sheet-zoom.png` preserve individual zooms.
All evidence is under `target/parity/atc/` in this worktree. Parent settings
windows and test Space labels differ, so pairs compare the sheet itself.
