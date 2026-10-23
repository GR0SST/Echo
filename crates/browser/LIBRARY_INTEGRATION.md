# Library → Spaces integration

New files only: `src/library.rs`, `src/library_tests.rs`. The parent owns module
wiring, browser navigation, Cargo checks, bundle/native verification, parity
status, commits and pushing. This slice is not an accepted parity claim.

Add `mod library;` to `src/lib.rs`. The module includes its dedicated test file;
no second test-module declaration is needed. Run `cargo test -p browser library`
and then the full workspace suite. No Cargo or native tests were run by this
implementation agent, as instructed.

## Host rendering contract

On Home, show Library in the existing browser window, initially using the
host's Media route/empty-state presentation. Do not start Space creation.
Media inventory is out of scope here. The observed empty Media view says
“Nothing here yet!” and includes Learn more; do not invent its URL or route.

Capture the currently selected real `SpaceId` when entering Library. Build
`LibraryProjection::from_browser(&browser, origin)` on every render, preserving
that origin through Library navigation. It refuses Blank/private models and
missing origins. It snapshots real ordered Space IDs, their assigned Profile
IDs/names, Space names, icons and hues; it does not clone tabs or own a Browser.
If the origin disappears, the host must leave Library or explicitly establish
a new valid origin rather than indexing a stale Space.

Call `render_library` at the full browser-content origin, including the left
navigation area (not inside the ordinary sidebar). Hide CEF's page views with
the existing native-page visibility path so they cannot cover this GPUI view.
Do not create an overlay, sheet, modal, window, synthetic tab or CEF page.

Supply `LibraryColors` from existing window/theme values. `card_surface` takes
a hue in degrees, matching `Space.hue`; adapt the existing theme mapper. The
module does not invent a separate palette.

`render_sidebar(&LibrarySpace, LibraryEvents) -> AnyElement` must render the
existing Favorites/Pinned/Today components **for that card's Space and assigned
Profile**, including their existing folders, splits, tab menus, and drag hooks.
Use the captured IDs, not active Space/Profile globals. The return value must
fit the remaining card width and own vertical overflow when needed; do not use
a fixed ordinary-sidebar width or duplicate the card's header/profile/footer.
The module owns the Space name/icon header, assigned-profile pill, theme edit
control, footer handle appearance and overflow button. It deliberately owns no
tab selection/loading/closing or engine logic.

Provide `other_section` for the host's non-Spaces Library route. Unsupported
routes render no fake inventory. Only enable navigation flags when those host
routes exist; Easels remains visible and disabled, with no implementation.

## Intent dispatch

`LibraryEvents = Rc<dyn Fn(LibraryIntent, &mut Window, &mut App)>` can capture a
weak BrowserWindow entity and call `update` to dispatch. Notify the host after
state changes; no background polling or subscriptions are required here.

- `Navigate(section)`: switch only the Library route. Opening Library and
  navigating to Spaces must never create a Space.
- `SelectSpace(id)`: validate the real ID, leave Library and use the existing
  Space selection/page visibility path.
- `BeginSpaceCreation { origin }`: call the parent's unified
  `beginSpaceCreation(origin)` path. The temporary sidebar page belongs after
  that real origin; Cancel returns to that real Space, **not Library**. Creation
  never happens in this module.
- `OpenProfile { space, profile }`: open the existing Profile control anchored
  to the clicked card. Validate both IDs against current state before applying
  anything. Do not silently redirect to the active Space or use a stale Profile
  assignment after an asynchronous menu delay.
- `EditTheme(id)` / `Overflow(id)`: anchor the existing controls to the clicked
  card and validate the target. Enable the corresponding `LibraryActions`
  field only when its real action is wired. Defaults are disabled.

The bottom handle is visual only. Space reordering from this handle has no
implementation or completion claim in this slice. Reuse the parent's existing
Space drag owner when adding that behavior; do not create a second drag model.
All control artwork uses existing Echo fixed vector helpers; user emoji remain
content. No Arc branded assets are included.

## Stable selectors and acceptance

`library`, `library-spaces-scroll`, `library-new-space`,
`library-nav-{media,downloads,easels,spaces,boosts,archive}`;
for each projection index N: `library-space-N`, `library-space-select-N`,
`library-space-profile-N`, `library-space-theme-N`,
`library-space-handle-N`, `library-space-overflow-N`; additional bounds hooks:
`library-space-name-N`, `library-space-profile-name-N`, `library-space-body-N`.
Selectors identify rendered positions; intents always carry persistent IDs.

Tests render actual GPUI elements and click controls, check five-card bounds
against the supplied Arc 1.170 measurements (1920 × 1050: cards x=140+264N,
y=48, width=240, bottom inset=48; plus center=1484,525), verify injection of host
sidebar content, captured inactive-card Profile and origin identities, disabled
actions, and absence of creation on render. These tests do not establish native
input, tab/menu/drag reuse, theme/popover visuals or integrated navigation.
The parent should additionally cover Home → Spaces → plus → Cancel against the
unified sidebar-page owner and inspect the bundled app's screenshots. Never
delete any test Space during cleanup.


## Review P2 correction and freeze

The public module types and render signature are unchanged. Space names use a
single-line shrinking/truncated label beside a nonshrinking icon. Profile pills
have a 180 px maximum including padding and a truncated label; the adjacent
28 × 28 theme control and 8 px gap fit the card's 216 px inner width. Header,
profile row and footer do not shrink vertically, and vector controls and the
handle do not shrink. The card clips overflow, while the callback retains
ownership of vertical sidebar scrolling. The horizontal strip has an explicit
content width so the final creation button remains reachable in small windows.

Two additional GPUI tests use an 800 × 480 viewport. They check long Space and
Profile names against card bounds and theme-button dimensions, send real scroll
wheel events to reach the last creation button and the last injected sidebar
row, verify fixed header/footer bounds during vertical scroll, and click nested
sidebar rows without stop-propagation to detect accidental card-selection
bubbling. These tests are authored, not executed: this module is not wired into
the parent's current Cargo checks yet. Rustfmt parsing/check and whitespace
checks are the only verification performed by this agent. The slice is frozen
for host integration after this correction.


### Standalone r3 scroll failure correction

The reported absolute-header assertion occurs immediately after a vertical body
wheel event, before the test's explicit horizontal gesture. Inspection of the
installed GPUI 0.2.2 scroll listener showed its default fallback maps a vertical
delta onto an X-only scroll container. The outer Library strip now sets
`restrict_scroll_to_axis = Some(true)` so body scrolling cannot move cards
horizontally. No public API changed; `card_index = 0usize` is preserved.

The regression keeps strict absolute card/header/footer/plus bounds checks
around only the vertical gesture, retains nested row clicks and final-plus
reachability, and separately verifies that explicit horizontal scrolling moves
chrome together with its card using relative offsets. This correction has only
rustfmt and whitespace verification here; the parent owns the next standalone
and integrated test runs. The slice is frozen again for host integration.

### Native visual gate: centered icon navigation

The previous text-only navigation rows at y=48 failed the native visual gate.
They are superseded by the directly inspected safe reference
`/tmp/arc-downloads-native-20261023/empty-profile.png` and a safe rail-only crop.
Only this module, its tests and this guide changed; the Downloads presenter and
all host files remain frozen. Public types, render signature, navigation
intents and captured Space/Profile IDs are unchanged.

The rail is 112 px wide. Six 96 × 80 tiles start at x=8 with 20 px between them,
forming one 580 px group centered on the window: y=775 at height 2130 and y=110
at height 800. Each tile has fixed 24 px unbranded vector section artwork at
local x=36/y=16 and a centered 12 px semibold label at y=48. The selected tile
has a filled rounded background. Existing route capability flags still govern
which tiles emit navigation intents; unimplemented routes do not gain actions.

At heights below 580 px, the group is clamped to the top of a vertical scroll
viewport without shrinking the observed tiles. This is a bounded fallback,
not a native-verified small-window layout. Its scroll is restricted to Y and
is independent of the Spaces-card strip.

New selectors: `library-nav-scroll`, `library-nav-group`,
`library-nav-selected-fill`, plus `-icon` and `-label` appended to each existing
section selector. Existing navigation selectors remain the tile click targets.
Two additional GPUI tests assert the supplied capture geometry at both 2130
and 800 px heights, the selected Downloads tile's physical fill bounds, clicks
through icon/label children, and short-height access to the last route while
Space cards remain stationary. Existing card and intent tests remain intact.

Bottom Back is still a presentation/behavior gap; no Back action was inferred
from the unlabelled native control. Gift was not implemented: its action is
unverified and account-related UI is outside scope. This correction is not a
full rail/native visual-parity acceptance claim. Only rustfmt parsing/check
and whitespace checks were performed here; Cargo/build/native/Git operations
remain the parent's responsibility. The three Library files are frozen again.
