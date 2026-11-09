# Window dragging from the top of webpages

Settings → Advanced → "Allow window dragging from the top of webpages"
(`Toggle::DragWebpages`, default off).

## Arc reference (Arc 1.169, macOS, measured 2026-11-09)

Measured with real HID mouse events on a disposable window and a fixture page
that logs `mousedown`/`mouseup`/`click`/`dblclick` and draws 10 px bands.
The option was switched through Arc's own Settings → Advanced toggle.

- **Off:** the page never moves the window. Only the strip around the page
  (window y < 10 without the Toolbar) drags it. Clicks reach the page.
- **On:** a press on page content within the top **70 pt of the window**
  moves the window.
  - Sidebar shown or hidden: page starts at window y 10, so the band is the
    top 60 pt of the page (page y 59 drags, 60 does not).
  - Toolbar shown: page starts at window y 42; the band is the top 28 pt of
    the page (y 27 drags, 28 does not). The edge stays at window y 70.
  - The band spans the full page width.
- The press still reaches the page (`mousedown`). On the first movement
  (≤ 2 pt) the page receives `mouseup` (so also `click`) at the moved point,
  and the system window move takes over: the window follows the remaining
  movement (50 pt drag → 48 pt move).
- A click without movement and a double click reach the page normally;
  double click does not zoom the window.
- A press on a link does not move the window: the page's own link drag wins.
- The toggle applies immediately to existing tabs, both ways.
- Little Arc and Peek were not measured (synthetic shortcuts did not open
  Little Arc reliably). Echo enables the band only in normal browser windows.

## Echo implementation

- Policy (shared Rust, all platforms): `crates/browser/src/page_window_drag.rs`.
  `regions()` intersects page surfaces with the top 70 pt; `Press` decides,
  per raw left-button event, whether to pass it, start the window move or
  swallow it. Regions are empty when the option is off, the page is
  fullscreen, the window is Little/Peek, or the pointer is on a link (CEF
  status text). Regions are recomputed on every layout, so the toggle and
  layout changes (Toolbar, Split, sidebar) apply live.
- Host interface: `native::set_page_window_drag_regions(view, regions)`.
  - macOS: `crates/browser/src/native/page_window_drag.rs`, a local
    `NSEvent` monitor; on the first movement it sends the page a balancing
    `mouseUp` and calls `performWindowDragWithEvent:`.
  - Windows/Linux: no-op stub for now (pages are native child windows whose
    input does not pass through this host).
- Off parity fix: CEF's transparent `RenderWidgetHostViewCocoa` defaulted
  `mouseDownCanMoveWindow` to YES, so the page under the titlebar (window
  y < ~32) moved the window even with the option off. The engine now returns
  NO for page views (`crates/engine/src/mac.rs`).

## Verification

- Unit/GPUI tests: `page_window_drag::tests`,
  `tests::dragging_the_window_from_the_top_of_webpages_follows_the_live_setting`.
- Native (drive.sh bundle + real HID mouse events), same fixture as Arc:
  off: page y 2–30 no move, strip above page moves, clicks/double clicks
  reach the page; on: page y 2–59 moves (48/50 pt), y 60/70 no move, click
  and double click reach the page without zoom, link press does not move the
  window; toggling on in Settings applies to the open tab without restart.

## Known gaps

- Link detection uses the asynchronous CEF status text; a press immediately
  after entering a link can still start a window move.
- Windows/Linux implementation; Little Arc / Peek reference behavior.
