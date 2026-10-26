# Website access to Space theme data

## Live Arc reference

The guarded localhost fixture in the shared **Echo Test** Space reads these
12 installed-bundle properties from the top document:

```
--arc-palette-foregroundPrimary
--arc-palette-foregroundSecondary
--arc-palette-foregroundTertiary
--arc-palette-maxContrastColor
--arc-palette-minContrastColor
--arc-palette-backgroundExtra
--arc-palette-background
--arc-palette-focus
--arc-palette-hover
--arc-palette-cutoutColor
--arc-palette-title
--arc-palette-subtitle
```

Values use eight-digit sRGB hex, including alpha (`#RRGGBBAA`). In the observed
Space the background is `#0A0D4BFF` and focus is `#4D54FBCE`. The first page report
is empty before the browser applies the colors. A cross-origin child remains
empty. A separate Command-Return reference loads a genuinely hidden background
tab: that new document also remains empty while the visible source has colors.
No Arc preference, theme, profile or owner data was changed; both sessions
archived their own tabs and closed their one owned window.

Evidence under `target/parity/partial-closeout/`:

- `arc-f28-values.json`, `arc-f28-palette.png`, `arc-f28-reference.log`.
- `arc-f28-background-values.json`, `arc-f28-background.png`,
  `arc-f28-background.log` (records `visibility: hidden`).

## Echo integration

The Advanced setting controls the existing window-resolved Space colors through
`WebEngine::set_page_palette`. Only a real main HTTP(S) document receives the
browser-owned constructed stylesheet. Child frames and internal/extension pages
are excluded. The palette contains color values only, with the exact property
names and hex representation above. No profile identity or browsing data crosses
this interface; no CEF patch or Chromium rebuild is involved.

Visible retained panes receive their current window's palette. New hidden pages
wait until selected, matching the background reference. Navigation invalidates the
cached application. Appearance and Space changes update colors without reloading
the document. Opt-out clears all retained documents owned by that host, including
hidden pages; it removes only the browser's own sheet and keeps page-owned sheets.
No unchanged-frame reinjection loop is introduced.

Private and Little models read the retained normal install preference rather
than resetting it from their isolated defaults. A private model without a
retained normal source exposes nothing. Settings notifies isolated roots on a
Theme Data change, so they also clear their sheets.

Three GPUI regressions cover setting clicks, retained/hidden pages, selection,
no navigation/recreation, appearance changes, load completion, unchanged-frame
coalescing and the private-window opt-out source. The full workspace suite passed
(1,382 browser tests plus the other workspace suites), as did Linux GNU and Windows
MSVC checks: `f28-full.log`, `f28-targets.log`.

Native `f28-native-r2.log` / `f28-native-receipt.json` verify all 12 variables,
Settings opt-out/re-enable, Light appearance, an unchanged document UUID, retained
page-owned stylesheet, absent child-frame colors and strict exit 0. Images:
`pair-f28-palette.png`, `echo-f28-advanced.png`, `echo-f28-light.png`.
The first UI probe stopped at the Settings-navigation guard; the final run uses
the previously qualified positioned-window/header-hover setup before clicking
its measured Advanced control. No further input followed that rejected state.

The frozen acceptance bundle uses canonical generation `20261026T015700Z-65318`,
framework SHA-256 `c9d3ae396593a3f8333c1017c9f3b9c6b64d9394285c1504257461bec9092829`. No canonical generation or shared CEF source was modified here.

Remaining palette differences are **cosmetic**: values reuse Echo's current
window/theme color system; Arc's exact palette derivation is not claimed. This
functional change does not redo the separate theme-rendering work.
