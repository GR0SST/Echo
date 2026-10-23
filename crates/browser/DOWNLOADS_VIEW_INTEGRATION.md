> Current integration: BrowserWindow mounts this presenter through
> `library_downloads.rs` and the canonical `LibraryDownloadsHost`. Window → View
> Downloads…, ⇧⌘J, Command Bar and the Library rail are wired. File actions,
> active rows, type subtitles and default download admission remain incomplete.
> `DownloadsControlFocused::Requested` is explicit user intent; `Observed` is
> passive handle reconciliation and must not supersede a native popup responder.
> Earlier "unwired" checkpoints below describe their historical scope.

# Library Downloads presenter integration

## Search reference update (2026-10-24)

Native observation in `/tmp/echo-download-destination-search-native-20261024.md`
establishes a separate search-miss state. Supply
`DownloadsList::SearchEmpty { query }` to show the exact copy
`No results found for “<query>”` with the download-circle icon in the list
column. Preserve the query verbatim, including spaces. `FilteredEmpty` remains
the Profile-filter state; it must not substitute for search misses.

The presenter still performs no matching. The host supplies the search
projection; native ASCII observations support literal case-insensitive
filename matching, without trimming or matching the source URL/type label.
Unicode behavior remains unverified. The search-miss regression exercises
Filter/Done query retention and bounds at both supported test sizes. Production
route integration and native Echo presentation remain pending.

This frozen slice owns `src/downloads_view.rs`, `src/downloads_view_tests.rs`,
this guide, and exactly one module declaration in `src/lib.rs`:
`mod downloads_view;`. It registers the prepared presenter and its tests for
compilation; no Library route, real inventory, engine or download policy is
connected. Library, window, core and profile UI files are unchanged. The parent
owns inventory adapters, policy/reference completion, Cargo/build/native checks,
parity status and release integration.

## Reference and evidence boundary

Read-only reference: `/tmp/arc-downloads-native-20261023/GUIDE.md`, its safe
`empty-profile.png` and `empty-profile-ax.txt`, plus toolbar-only crops of the
search/filter captures. The requested `safeempty-profile.png` was not present;
the guide explicitly identifies `empty-profile.png` as safe, and that image
was inspected. Private download filenames/captures are not copied into these
files. No native app was opened or focused during this slice.

Verified presentation: Library Downloads in the same browser window; a list
column after the existing navigation; Search Downloads… / Filter toolbar;
inline Filter Downloads… / Done / Profiles / wrapping existing-profile chips;
chronological completed rows with file/thumbnail art and two truncated text
lines; Profile-filtered empty presentation inside the narrow column.

The supplied target geometry is a 304 px column immediately after the host's
112 px navigation. The source guide qualifies its measurements as screenshot
pixels, not calibrated logical points. This presenter follows the explicit
304/112 target; no native visual-parity acceptance is claimed.

Empty state copy is exactly:

> Nothing here yet!
>
> Come back after you’ve found some
> cool stuff to save.

It uses Echo-owned arrow-down-circle vector geometry. Do not generalize this
state to an empty download database or search misses: only empty Profile-filter
results were observed. There is no Media copy or Learn more link here.

## Stable integration hooks

`mod downloads_view;` is now registered in `src/lib.rs`. The module itself
includes `downloads_view_tests.rs`; no separate test-module registration or
Library route activation was added.
`crate::text_input::bind_keys(cx)` and `crate::downloads_view::bind_keys(cx)`
must be initialized by the host's ordinary input setup. Registration is the only lib.rs change; no Cargo checks were run
by this agent.

Create `DownloadsView<Token>` with:

- `DownloadsProjection<Token>`: owned profiles and ordered completed groups,
  `DownloadsList::FilteredEmpty`, or `DownloadsList::SearchEmpty { query }`.
- `DownloadsColors`: existing column, preview, text, muted, control, selected and
  hover colors from the host. This slice supplies no application palette.
- `DownloadsPreview`: a repeatable parent render callback returning an
  `AnyElement`. It can return an empty element for the observed blank pane.
- A GPUI `Context<DownloadsView<Token>>`.

The view occupies the route area **after** the existing nav: local x=0 is the
304 px list, local x=304 starts the preview. For the existing Library renderer,
pass this entity as the Downloads `other_section`; do not add another 112 px
inset inside it. The parent retains the Library rail, route selection and CEF
visibility handling. This module creates no overlay, sheet, window, browser
tab or native page.

`Token: Clone + Eq + 'static` is owned and opaque to this view. A parent can use an
engine `DownloadToken` directly without reconstructing it from numeric IDs or
adding CEF dependencies. Tests use fictional `u64` tokens. Tokens are not
persisted and row positions are never action identities. Each row also captures
its stable `UserProfileId` separately from the current/active Profile.

`CompletedDownload` contains only token, Profile ID, display title and an
already-formatted subtitle. No full path, URL parsing, filesystem metadata,
network requests, phase inference or time conversion exists in this module.
The parent supplies newest-first grouping and the observed date-heading copy
(e.g. Today/Yesterday), type descriptions and optional source domain. Do not
project pending, active, paused, failed, missing-file or canceled states as
completed rows. Only parent-owned fictional inventory is used by this slice;
connecting real inventory requires the parent's separate adapter.

`set_projection(projection, window, cx)` replaces owned metadata and redraws;
it does not silently change the search input or apply a local filter. Pass the
host Window so a removed focused Profile transfers to Done before its handle
is dropped. A retained focused Profile keeps its handle and is scrolled into
view at its new index. Other presentation state survives host result updates.

`set_row_artwork` optionally installs a callback for parent-provided completed
row artwork. The view bounds it in a fixed 36 × 36 clipped slot. The default
is an Echo-owned fixed-vector generic file outline, without format-specific
branding. Do not initiate file/network work from the render callback.

## Intents and behavior ownership

Subscribe to `DownloadsIntent<Token>` from the entity:

- `SearchChanged(String)` supplies the actual TextInput query. Matching stays
  in the host; the native ASCII evidence and search-miss copy are recorded
  above. Debounce and Unicode matching remain unverified. Typing alone leaves the host's
  existing rows visible until the host supplies a new projection.
- `FilterVisibilityChanged(bool)` accompanies the local inline toolbar switch.
  Filter opens in the same list column. Done restores Search Downloads… and
  Filter without silently clearing query or selections. Reopen persistence is
  not claimed as native-verified behavior.
- `ToggleProfile(UserProfileId)` captures the clicked existing Profile ID.
  The host chooses/updates selected chips and rows using `set_projection`.
  The view does not invent multi-selection semantics, change Space assignments,
  or offer All, new/edit Profile, account or synchronization controls.
- `RowPressed { token, profile }` is emitted only after the host explicitly
  calls `set_row_selection_enabled(true, cx)`. The default is disabled because
  native selection/preview behavior is unresolved. Even when enabled, this
  event performs no open/reveal/delete or engine action and adds no selection
  decoration. Do not opt in as a claim of verified native behavior.

There are no progress indicators, pause/resume/retry controls, hover actions,
context menus, history deletion, permissions or file-location controls.
Unobserved native behaviors remain outside this presenter.

## Selectors and checks for the parent

Stable hooks: `downloads-view`, `downloads-column`, `downloads-preview`,
`downloads-search-toolbar`, `downloads-search`, `downloads-filter`,
`downloads-filter-inline`, `downloads-filter-done`, `downloads-profile-chips`,
`downloads-profile-N`, `downloads-list-body`, `downloads-list-scroll`,
`downloads-group-G`, `downloads-row-G-N`, `downloads-artwork-G-N`,
`downloads-title-G-N`, `downloads-subtitle-G-N`, `downloads-filtered-empty`,
`downloads-empty-icon`, `downloads-empty-title`, `downloads-empty-body`.
Position selectors are for inspection; intents use captured stable identities.

Nine GPUI tests use fictional parent-owned data and real input/layout paths:
column/nav/parent-preview boundaries; inline Filter/Done and Profile identity
after reordering; query input without local filtering; filtered-empty centering
within the list at 1320 × 800 and 640 × 480; long titles/subtitles/Profile names
with bounded artwork and wrapping chips; vertical scrolling to the final row
with toolbar fixed, default row actions disabled, and opt-in token/Profile
capture; full search-field focusing through the icon, left/right padding and
vertical gutters under a focusable host; inner-click caret placement; and real
painted row glyph/run coverage at final width across viewport changes. The
scroll container is restricted to its Y axis so horizontal deltas cannot
become vertical list scrolls through GPUI's axis fallback.

The agent performed only rustfmt parsing/check and whitespace validation.
No Cargo, build, Git or native actions were run. The parent should run the
registered module's standalone tests and the integrated suite, then inspect actual
bundle screenshots against the safe reference before accepting visual parity.
The existing Library slice remains frozen and unchanged.


## Registration/fresh-shaping audit checkpoint

The current registration pass leaves all original six test scenarios in place
and adds three regressions; these nine tests are authored, not claimed green.
No Library navigation capability flags changed. Downloads remains unreachable
through the real Library until the parent intentionally wires its host route.

The entire visible 36 px Search Downloads field now forwards left mouse-down
focus after TextInput's own caret/selection handling. It focuses only if needed
and prevents an enclosing focusable root from taking focus back. No synthetic
editor click, cursor move, selection reset or automatic search policy is added.
New inspection hook: `downloads-search-input`.

Row title/subtitle, group headers, inline filter heading and Profile-chip labels
reuse `SidebarTitle`. It shapes after final layout width is known and creates a
fresh TextRun covering the actual final bytes, avoiding GPUI's intrinsic
truncate/clamp probe state and the NoopTextSystem/native shortened-run failure.
The 56 px completed rows, 36 px art, 18/16 px text lines and 304 px list geometry
remain unchanged. Profile chips measure a fresh full 12 px line only to choose
a finite intrinsic chip width, capped at the column's 280 px inner width; their
final labels are shaped separately by SidebarTitle. New label bounds hook:
`downloads-profile-label-N`. No shared shaper source was modified.

The long-name test inspects actual painted Profile labels, including full
short-name glyph coverage and long-name ellipsis. The new row-shaping test
inspects actual paint records for short/long fictional filenames/subtitles,
not a separate shape estimate. Cached hidden debug selectors are not used as
proof of toolbar removal: real Filter → Profile → Done → Filter clicks verify
restoration and retained captured Profile identity instead.

Exact four-file manifest, original snapshots, hashes and applied diff are in
`/tmp/echo-downloads-registration-20261023`. The slice is frozen for parent
Cargo. This is presenter registration/validation preparation, not completion
of a working download feature. Source-close policy, real inventory ownership,
private lifetime/history semantics and unobserved native actions remain owned
by the parent's separate reference/integration work.


## P2 scroll and keyboard checkpoint

The three-file follow-up preserves the existing nine scenarios and fresh
SidebarTitle shaping. The wrapping chips now have an independent, Y-only
scroll viewport capped at 130 px; short chip groups retain their natural
height. The heading, Done and Profiles label stay outside that viewport.
The new regression requires at least 200 px of result body at 640 × 480 with
20 long Profile names. No control, copy, row or column geometry was added.

Register `downloads_view::bind_keys(cx)` alongside TextInput bindings before
using the presenter. The view implements `Focusable` with the search input
as its host-facing entry when closed (Done when open). Tab/Shift-Tab traverse Search/Filter when closed
and Done/existing Profile chips when open. Enter/Space activate only focused
controls, leaving ordinary search input intact. Opening transfers focus to
Done; closing returns it to Filter. Keyboard traversal scrolls the focused
chip into view, and Profile focus handles survive metadata reordering by
stable Profile ID. Mouse actions use the same intents and closure focus.
Focus is invisible: there is no new focus decoration or visible affordance.
These Echo access paths are not evidence of Arc native keyboard parity.

Two additional GPUI regressions (eleven scenarios total) exercise 20 long
Profiles at 640 × 480: actual wheel scrolling followed by clicking the final
chip with the header/result bounds retained; and real Tab/Shift-Tab,
Enter/Space opening, captured-ID toggling after reordering, keyboard scroll
reachability, Done, closure focus, and subsequent search typing. The original
nine tests and their paint assertions remain in place. These new tests are
authored, not claimed green. Only standalone rustfmt parsing/checks and
source inspection were performed; the parent owns further Cargo and native
validation. No Library route or engine adoption is included.


The parent's r2 run reached the final keyboard-regression assertion with all
activation/focus intents correct; the expected log incorrectly assumed one
SearchChanged for the whole simulated input. The test now expects every
character prefix in sequence between the exact opening/toggle/closure and
reopening/closure intents, and separately checks the retained final query.
No events are filtered out and no product debounce behavior is changed.
This test/guide-only correction passed standalone rustfmt checks; the parent's
full rerun remains pending. Host key-binding registration remains a documented
requirement for the unwired route.


## Key release and projection-focus checkpoint

GPUI 0.2.2 `elements/div.rs` installs focused on_click activation for unmodified
Enter/Space on KeyUpEvent. Its `window.rs::dispatch_keystroke`, used by
simulate_keystrokes, dispatches KeyDown only. The former explicit activation
key bindings and action handler have therefore been removed: mouse and keyboard
now share only the existing on_click path. Tab/Shift-Tab still use the scoped
DownloadsView actions, so `downloads_view::bind_keys(cx)` remains a host setup
requirement for this unwired presenter. No focus decoration, visible geometry,
copy or native Arc keyboard-parity claim is added.

`set_projection` now requires the host Window as documented above. Before
replacing Profile handles it transfers a removed chip's focus to Done; this
also covers an empty replacement list. If the focused ID survives reordering,
it retains focus and the chips scroll handle reveals that ID at its new index.
The presenter emits no extra intent for either projection reconciliation.

All eleven existing scenarios remain, including exact per-character query
cadence and final query retention. The keyboard scenario now sends real
KeyDown/KeyUp pairs for its controls and traversal and verifies that reversing
20 long chips at 640 × 480 reveals the bottom-focused ID at index zero before
any subsequent traversal. Two additional tests (thirteen total) check both
Enter and Space down/up boundaries on Filter, Profile and Done, with exactly
one intent per release and no toolbar crossover; and focused-chip removal,
including removing all remaining chips, followed by scoped keyboard operation
and restored closure focus. Complete event logs remain exact and unfiltered.

Standalone rustfmt parsing/checks and static inspection only were performed
for this checkpoint. Cargo/native validation belongs to the parent; these
regressions are not claimed green. Product and test source are frozen after
this checkpoint. No Library route, engine or shared shaper source changed.


## Held mouse-press identity checkpoint

GPUI stores pending mouse-down interaction state by element ID. Profile-chip
and completed-row element IDs now use generated monotonic keys instead of
projection positions. Keys are retained by exact UserProfileId for chips and
by the exact opaque `(Token, UserProfileId)` pair for rows. The token bound is
now `Clone + Eq + 'static`; tokens are compared directly, never hashed, parsed
or reconstructed. The existing engine DownloadToken meets this bound. Removed
items lose their mapping; later insertions receive fresh keys. Checked counter
increment prevents key wraparound/reuse. Registries keep only current projected
items. Positional debug selectors remain unchanged for inspection.

This preserves a held press only for its original item. If a projection update
moves or replaces the item under the pointer, releasing there cannot activate
the unpressed replacement. A moved original can subsequently be clicked with
its original token/Profile identity. No mouse or keyboard activation path,
visible geometry, query cadence, route or engine behavior was added.

All thirteen prior scenarios remain unchanged. Two additional GPUI tests
(fifteen total) send actual MouseDown, update the projection while held, then
MouseUp at the same point. Each covers reorder and replacement for chips or
enabled rows, asserts no replacement intent on release, and verifies subsequent
real clicks capture the correct stable identities. The row cases separately
exercise equal tokens with different Profiles and equal Profiles with different
tokens. No control events are discarded from expected logs.

The parent reported the prior r3 snapshot green (794 tests). This follow-up
received only standalone rustfmt parsing/check and static inspection; no
Cargo/native/Git actions were run and the new regressions are not claimed
green. Source is frozen for the parent's affected/full reruns.

## Parent regression checkpoint — October 23, 2026

All fifteen presenter tests passed in the combined 796-test workspace run.
Independent review round 4 found no residual concrete issues. The rebuilt
application's existing page, Command Bar and menu smoke checks passed; this
unwired presenter did not alter those routes. Real Downloads route screenshots,
inventory admission and native visual acceptance remain outstanding.
