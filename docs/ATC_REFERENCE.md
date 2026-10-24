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
settings frame top, with 18 px corners, #161c2f surface, 24 px horizontal padding,
14 px bold title and 13 px semibold fields. The header is 84 px high; its
122×28 New Route button and close control sit above a hairline. Rules use two
24 px rows with a 4 px gap and 20 px between rules. URL rows have an 80 px
label, 111 px matcher, flexible text field and a minus-circle delete control.
The destination starts at x=112 and is 355 px wide. The 90 px footer has a
270×24 Default picker. Selector surfaces are #282d3f; text fields #16171a.

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

The GPUI settings sheet and Command Bar entry are the next validated piece.
