# Nested Peek and Little navigation

Reference: installed Arc 1.170.0, local synthetic pages only. Guarded captures
are under `target/parity/nested-navigation/`; no Arc assets are copied.

## Observed behavior

`arc-v4/peek-child.png` → `peek-grand.png`: Shift-click from a normal tab
opens Peek; an ordinary link navigates its existing document. A
`target="_blank"` link opens a normal foreground tab (`normal-blank.png`).
Selecting the source sidebar row restores the same Peek at its last URL
(`return-source.png`), rather than replacing the source page.

`little/arc-little-plain-child.png` → `arc-little-target-blank.png`:
ordinary and `target="_blank"` links navigate the same Little window. The
before/after native inventories contain no additional window for that popup.

## Implementation

Little popup navigation uses its existing tab and CEF page, retaining Back /
Forward history. Peek routes a popup to its normal owner using the captured
website profile. Foreground navigation parks the live child; source selection
restores that same child, page and history without reload. Background routing
keeps the lookup visible and preserves the owner's selection. Deferred routing
revalidates the opener, document epoch, attachment and selected source.

Retained children are keyed by source Space/tab/profile. Closing the source,
retiring its profile or closing the owner retires their native pages. Restoration clears pending source-page focus and its hover overlay so a
covered source cannot order a sibling window over Peek. Native
hide/show checks child ownership; macOS, Windows and X11 shims contain only
window visibility calls. This adds no browser interface or Chromium UI. Internal Site Settings sheets
retain their existing unavailable/stale-action behavior rather than inheriting
website-source retirement; its five regressions also pass.

Seven GPUI regressions cover Little single-document navigation, foreground Peek
return without create/navigate/close calls, background selection/profile
preservation, source retirement while parked/visible, owner teardown, and rejection of covered-source hover overlays.

## Qualification limits

Nested Shift-click inside an existing lookup remains unqualified: the guarded
reference did not demonstrate a positive distinct result. Little background
popup focus and background-Peek focus have test coverage only where stated;
additional native reference cases remain. Normal popup Today insertion order follows the existing route and remains
a broader link-disposition qualification. Source lookup retention is local to
the current normal window, not a persisted session checkpoint. Moving a source
to another Space, opening from a profile with no matching destination Space,
private-window edge cases, exact animation/material fidelity, and Windows/X11
native runtime acceptance are not claimed by this change.

## Native acceptance

- Peek: `echo-native-v5/accepted.json` records the same native window before and
  after parking (`72734`, owner `72719`), Root/Grand/Blank/Back-to-Child URL
  checks, CEF Complete and exit zero. `return-source-screen.png` is an external
  physical-screen capture, independently of the app's composite screenshots.
  Its `restored-native-windows.json` confirms Peek is above the source. Initial
  probes caught a genuine native-order failure; a source hover overlay and
  pending source focus could cover the restored child. Those probes are not
  accepted evidence. Reinserting the owned native child and suppressing covered
  source hover/focus resolved it.
- Arc/Echo Peek pair: `arc-v4/return-source.png` /
  `echo-native-v5/return-source-screen.png`; zooms
  `arc-peek-return-crop.png` / `echo-peek-return-crop.png` were inspected.
  Normal-tab escape is `arc-v4/normal-blank.png` /
  `echo-native-v5/normal-blank.png`; retained Back is
  `echo-native-v5/retained-back.png`.
- Little: `echo-little-v2/accepted.json` checks exactly one tab per dump across
  Root → Child → target-blank → Back-to-Child, CEF Complete and exit zero.
  Pair: `little/arc-little-target-blank.png` /
  `echo-little-v2/target-blank.png`; inspected zooms are
  `arc-little-blank-crop.png` / `echo-little-blank-crop.png`.
- Only synthetic local pages and disposable Echo profiles were used. The native
  framework hash matches canonical generation `20261025T192350Z-5008`.
  Preliminary runs intermittently hit the existing CEF shutdown watchdog;
  the accepted runs above each exited cleanly. No engine shutdown policy was
  changed here. Profile/theme/sidebar settings differ, so these are behavior
  pairs, not a claim that existing unrelated chrome now has exact pixel parity.
- Final Arc fixture cleanup was blocked by the guard: changing inventory and
  then an ambiguous new-window delta prevented safe input. Exactly claimed
  windows were closed; unknown/restored windows were left untouched. An
  `Echo Nested root` fixture tab may remain in the shared Echo Test Space.
  No Space was created, deleted or reassigned.

Final workspace validation passes: 1,318 browser, 276 core and 193 engine tests,
plus every other enabled suite; one existing AI integration test is ignored.
Log: `~/Library/Caches/EchoBuildHealth/t3-71384893/logs/nested-full-restack.log`.
