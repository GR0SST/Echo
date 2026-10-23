# Arc reference and parity decisions

## Required standard

Match Arc's behavior and visual presentation 1:1. Menus, dropdowns, pane
headers, sidebar grouping, spacing, focus and cancellation are part of the
feature. Do not replace an Arc flow with a convenient custom picker and call
it complete. Preserve the existing prohibition on Arc branding and assets.

Before changing a flow, inspect the installed Arc app in a separate test
window, record the trigger and resulting states, and compare our real app.
Keep the relevant row in `ARC_PARITY.md` partial while known visual or
behavioral differences remain. Add regression tests for user interactions.

## Split View — 2026-10-23

Reference: installed Arc **1.169.0**, inspected through its native UI in a
separate File → Blank Window with disposable example.com/example.org pages.
The user's existing tabs were not used for the experiment.

Observed directly:

- View → Add Split View immediately displays a second, empty pane.
- The original page remains visible beside the empty pane.
- The empty pane shows Search or Enter URL… in its header and opens the
  ordinary Command Bar inside that pane, centered within its page area.
- Entering a URL fills the new pane. There is no intermediate custom
  selection dialog before the empty pane appears.
- Escape cancels the unfinished pane and returns to the original page.
- Cmd+W closes the focused pane. The other pane remains as a standalone tab
  when only one page is left.
- A populated split is one sidebar row containing separate segments for
  its pages, rather than a concatenated group title and favicon pair.
- Each pane is a single rounded outer surface. Its top bar joins the web
  content along a straight edge; the web page is not a separately rounded card
  under a rectangular header. Confirmed against the user-provided comparison
  screenshots on 2026-10-23.
- Each page has a compact top bar: navigation controls, a centered address,
  Split View controls and a close control. There is no additional shared
  title strip above the pane headers.

Additional reference behavior explicitly reported by the user:

- Creating a split from Favorites or Pinned must preserve the original
  item and use an independent copy in the split, rather than move the
  original out of its section.

Implementation checks:

- UI and native-page Cmd+W routing closes only the active pane.
- New-pane search, Escape and Cmd+W cancellation, sidebar segment focus,
  protected-item copying and preservation of existing views have GPUI tests.
- Session saves exclude unfinished panes and any temporary source copies.
- Cancelling an added pane restores the previous axis, weights and focus.
- Real-app automation checks empty and populated panes, native rendering,
  closing, and preservation of Favorites/Pinned.

Remaining pixel and menu-state differences are tracked as partial in
`ARC_PARITY.md`; this record does not claim that every menu or the complete
browser now matches Arc visually.

### Expand Current Split — 2026-10-23

Inspected View → Expand Current Split in a separate Blank Window in the
installed Arc. The active pane grows while the other pane remains visible
as an approximately 220 logical px strip. The reference display uses 3840 ×
2160 logical pixels at 1x. Repeating the command retains the expanded layout;
it does not separate the pages or toggle back to equal widths. The menu
continues to say Expand Current Split. The pane menu itself does not expose
this action.

Our View menu and Command Bar now resize the existing split in either axis.
Other panes receive 220 px each, including their headers; windows too small
for this use equal shares. Existing page views, group identity, pane order
and focus survive. Session saves retain the new weights. Regression tests
cover menu and Command Bar activation, both axes, small four-pane windows,
and a standalone-page no-op. A native CEF run at 1200 × 800 confirmed widths
of 220/724 px and vertical page heights of 188/524 px below 32 px headers.
The screenshots were inspected with the window on screen. Expansion
animation remains a parity follow-up; this implementation changes the
weights immediately.

## View Source — 2026-10-23

Inspected View → Developer → View Source on example.com in a separate Arc
Blank Window. The result is a new Today tab with a `view-source:` address;
the original Example Domain tab remains in the sidebar. Chromium supplies
the source viewer with line numbers, syntax coloring, linked resources and
a Line wrap checkbox. View Source becomes disabled on the source tab.
The disposable window was closed after inspection.

Our Developer submenu, Command Bar and Cmd+Alt+U use a new Today tab with
the same engine-owned source viewer. The command preserves the exact source
URL and targets the focused Split View pane without changing its original
group. Internal pages and source tabs do not offer the action. Regression
tests cover menu, keyboard, Command Bar, native page shortcut routing,
preservation of loaded original pages, and returning to a split after closing
its source tab.

Native CEF verification confirmed source rendering, exact query preservation,
repeat-shortcut suppression and Command Bar activation in an on-screen window.
CEF's address callback reports the underlying document URL for a source view;
the engine now reads the visible navigation entry's source URL so the address,
session and command availability retain source mode.

## Space header and Pinned reveal — 2026-10-23

Reference: the user's Arc sidebar screenshot and explicit interaction brief.
Pinned folders and pages sit immediately below the Space name/icon. There is
no additional Pinned caption between the name and those items. Clicking the
name retracts the items into the header or reveals them below it.

Our Space header now owns that interaction, including mid-animation reversal,
per-Space persisted collapse state, and tab/folder drops at the Pinned root.
The transition clips the list under the header and moves subsequent sidebar
content with its height. macOS Reduce Motion bypasses the slide. Existing
right-click Space actions and the optional hidden-header preference remain.

The supplied close-ups also define the header's control states: the Space
icon sits to the left of the title at rest; hovering replaces it with a
downward chevron and reveals an ellipsis on the right. A collapsed header
keeps a rightward chevron visible. The ellipsis opens Space actions without
toggling Pinned. The 36 px row has a 12 px corner radius, 12 px horizontal
insets, an 18 px leading icon slot and an 8 px title gap. Chevron and
ellipsis use fixed vector geometry rather than font glyphs. The hover
background and controls remain visible while the Space menu is open.

## Sidebar motion and pointer tolerance — 2026-10-23

Reference: the user's Arc observation and sidebar screenshot. Native window
buttons move with the panel instead of remaining at the window origin until
it disappears. Both docked toggles and floating panels drive native button
positions from the panel's current horizontal offset.

A revealed sidebar remains open beyond its visible right edge. We use a
100 logical px dismissal buffer (a tuning value from the user's distance
estimate, not an independently measured Arc constant). Native pointer reads
keep this working over CEF without making the transparent overlay wider or
blocking page input. Open menus and the Command Bar keep it alive.

Online discussion confirms deliberate hover timing, but does not document
Arc's dismissal distance or animation implementation:
https://www.reddit.com/r/ArcBrowser/comments/17ewxo7/
The supplied reference takes precedence over inferred online behavior.

## Sidebar icons and Split View row width — 2026-10-23

Reference: the user's full Arc sidebar screenshot and the confirmed Split
View clipping screenshot. Existing interface controls use a shared 18 px
vector box and inherit control color. Reference-specific frame controls use
a 1.5 px stroke; the semibold disclosure uses 1.8 px. Catalog-backed controls
retain their source geometry at that size. Default folders are monochrome outlines, with the
disclosure replacing the icon on hover. Selected user emoji and website
favicons remain unchanged. Reference-specific controls are independently
drawn; general-purpose controls use the open Ionicons catalog below. Arc
branding and proprietary bundled artwork are not included.

The sidebar's top row exposes sidebar toggle, back, forward and reload/stop.
Back/forward reflect engine navigation state; reload changes to stop during
loading. Split pane headers use the same navigation icon set.

Unnamed Split View rows give each segment an equal share of the full row.
The group close button is overlaid on hover, so no empty trailing gutter is
reserved. Only the last segment's text inset changes for that button; its
outer bounds remain fixed. Long page names truncate within their segment.

The installed Arc icon bundle contains the open-source Ionicons font. We
export its 1108 non-brand glyphs to embedded filled contours, preserving the
source shapes and MIT notice (`crates/browser/assets/ionicons`). Existing
general-purpose controls now use those contours. Disclosure, ellipsis and
reference-specific frame/key controls retain their fixed vector geometry.
The graphical Space icon picker remains separate unfinished parity work.

## Inline ordinary tab names — 2026-10-23

Reference: installed Arc, inspected in a disposable Blank Window. Double-clicking
an ordinary sidebar tab replaces its title with a selected inline text field.
Enter commits, Escape cancels, and clicking the page commits. Clearing a custom
name restores the website title; the empty field uses that title as its
placeholder. The row's close control is hidden while editing.

Echo now uses this placement for single-page Today and Pinned tabs, including
Rename from the context menu and Command Bar. Editing an inactive item keeps
the current page selected. Hidden Pinned items are revealed before editing.
Regression coverage includes page-title updates, cancellation, focus returning
to CEF, switching tabs, opening the Command Bar, hiding the sidebar, closing
and restoring the edited tab, and session persistence. Favorites and Split
groups retain their existing editor pending direct reference inspection;
folder and Space name placement are separate unfinished work.

Native CEF verification used the isolated drive profile: double-click selected
the inline name, Enter saved it, Escape preserved that saved name, and the
Command Bar's Rename action saved a new name on a page click. A modified HTML
input retained `Native-state-kept` throughout, confirming the page was not
reloaded. Both editor and saved-state screenshots were inspected; the final
window report was key, main and on-screen. All 328 workspace tests passed.

## Link status — 2026-10-23

Reference: the installed Arc in a disposable Blank Window on example.com.
Focusing its Learn more link with Tab displays `iana.org/help/example-domains`
in a small rounded pill at the lower left inside the page. The pill uses the
Space's tint, omits the HTTPS scheme, and does not reserve page layout space.
The test window was closed after inspection. This observation used keyboard
link focus; pointer hover is checked separately in Echo.

Arc's [April 20, 2023 release notes](https://start.arc.net/release-notes)
document a short preview followed by the complete URL after 1.5 seconds.
Echo retains the exact CEF target in transient page state; it does not apply
the tracking-parameter cleanup used when copying links. Hiding a page or
starting navigation clears that state, and callbacks from hidden pages are
ignored. A passive GPUI child window presents the status above CEF without
moving or reloading the page. Native pointer passthrough is isolated in the
platform interface. Exact colors, transitions and long-URL layout remain
partial pending closer reference comparison.

Native Echo verification used an isolated data-URL page with a long link and
an input under the status position. The inspected screenshots show the short
preview and wider delayed state. The target's query and fragment remain in
the event state. The window report confirms the overlay ignores mouse events
and is not key, while the browser remains key/main. Clicking the covered input
and typing `Through-overlay` succeeds, and the status disappears. All 332
workspace tests passed. The expanded pill still truncates targets wider than
the page; complete long-URL presentation remains unfinished.

### Long-link follow-up — 2026-10-23

A local HTTP fixture exposed a link with 100 path segments plus a query and
fragment. In a separate installed Arc Blank Window, opening and cancelling
the link's context menu triggered its status. Both the screenshot and the
accessibility text confirm a single bounded line: the host and leading path
segments, `…/`, the final path segment (including a trailing slash), then a
query prefix ending in `…` when space runs out. Even the expanded Arc status
does not display arbitrarily long targets in full or wrap them into lines.
The disposable window was closed after inspection.

Echo now measures with the rendered label's font and applies the same path
and query shortening order. A fitting target retains its query and fragment;
shortening changes only the displayed label. Grapheme boundaries keep emoji
and combining sequences intact. Tests cover unique final filenames, trailing
slashes, encoded path separators, narrow widths and non-HTTP schemes. Exact
host/subdomain formatting and the previously recorded theme/transition
differences remain partial.

Native verification of the follow-up passed with `require-focus` enabled.
The inspected preview and expanded screenshots show middle-path and tail
ellipses rather than a hard clip. The exact CEF target still contains the
encoded query and final fragment. The final window report confirms the
browser is key/main/on-screen and the passive status window ignores mouse
events. The run clicked the page before hovering its link. All 336 workspace
tests passed. The automation driver now routes synthetic moves to the native
view under the point rather than unconditionally sending them to GPUI.
`require-focus` aborts interrupted runs, including between typed characters.

## Text Undo and Redo — 2026-10-23

Reference: the installed Arc Command Bar in a separate Blank Window. Three
individual key presses produced three characters; Cmd+Z removed only the last
character and Cmd+Shift+Z restored it. Pasting over the selected query, then
undoing, restored the entire previous query in one step. The disposable
window was closed without navigating or changing the user's existing tabs.
The system pointer and keyboard layout were left unchanged.

Echo text fields now keep independent text/selection histories, with one step
per insertion event. A new edit after undo drops the abandoned redo branch.
Programmatic initialization or retargeting starts a fresh history. IME
candidates are provisional; committing or unmarking records one edit, while
cancelling back to the original text preserves redo. IME selection offsets
are interpreted relative to the inserted string, including after Unicode
prefixes. These composition paths have GPUI coverage; their exact Arc IME
grouping has not been inspected directly.

Edit menu Undo/Redo also route to CEF's focused frame when no GPUI editor owns
the action. A focused name editor with an empty undo stack must not undo the
underlying page. Native page keyboard shortcuts stay with Chromium. Sidebar
and Peek operation histories remain separate unfinished parity work.

Native verification exercised both shortcuts and the real Edit menu in an
on-screen, focused Echo window. The Command Bar followed `abc → ab → abc`,
and undoing a whole-text replacement restored `abc` selected; redo restored
`replacement`. On an autofocus HTML input, native Edit → Undo restored
`Original`, and Edit → Redo restored `NativeEdit`. All four screenshots were
inspected. No cursor warp or clipboard operation was used in this Echo run.
All 344 workspace tests passed.

This check also exposed an automation error: modified key events had been
addressed to the main window while its Command Bar child owned focus. The
driver now targets the key window only when it belongs to this browser, and
can invoke enabled native menu items directly for end-to-end routing checks.

### Sidebar Undo: pinning and archiving

Inspected the installed Arc through native accessibility menus in a separate
normal New Window, using a disposable Example Domain tab. Blank Window disables
Pin Tab and closes when its last Today tab is archived, so it cannot establish
normal sidebar behavior. In the normal window, Tabs → Pin Tab moved the test
item above New Tab. Edit → Undo returned it below New Tab at its previous
position; Edit → Redo pinned it again. Undoing Archive Tab restored the same
page at its previous Today position and selected it. Redo removed it again.
The Edit menu labels remained plain Undo and Redo during these checks. The
restored page and sidebar were also inspected in a native screenshot. The
disposable window was closed after redoing the archive; existing tabs were
not archived or edited. No system cursor movement or clipboard was used.

Echo's transient sidebar journal now records pin/unpin, including Split View
items, folder placement and the core model's Favorite-to-Today operation.
Undo restores identities, positions and pin metadata without restoring a
Browser snapshot: later navigation, page titles, new tabs, split geometry,
folder labels and unrelated selection changes remain intact. Invalidated
entries cannot resurrect archived tabs or break a changed split. A new pin
operation clears redo; initial profile seeding and restored sessions have no
undo history. The journal retains at most 100 entries.

CEF reports native DOM edit focus, including contenteditable and subframes,
through renderer-to-browser IPC. Editable page shortcuts remain in Chromium;
non-editable pages route Undo/Redo to sidebar history. Edit menu actions use
the focused web editor when one is active. GPUI inputs retain their own undo
stacks and cannot consume sidebar entries. Archive/close, other sidebar actions,
Peek and action-specific command suggestions are still unfinished.

Native verification passed for pin Undo/Redo through both keyboard shortcuts
and the real Edit menu. With the test tab pinned, Edit → Undo restored an
input's `Original` text without unpinning the tab. After Tab moved focus to a
button, ⌘Z moved the same page to Today; ⇧⌘Z and Edit → Redo pinned it again.
The edited field value remained intact across placement changes. Three
screenshots and the model dumps were inspected.

A local HTTP fixture repeated this with an autofocus input inside an iframe:
⌘Z and Edit → Undo restored `FrameOriginal` while the tab remained pinned;
after moving focus to the frame's button, ⌘Z unpinned the tab. All three
result screenshots were inspected. This run did not use the pointer or
clipboard; the local fixture shut down normally afterward. The preceding
data-URL `srcdoc` attempts stayed blank with loading=true and did not accept
composed text, so they are not counted as edit-focus verification. That load
failure remains to investigate separately. All 356 workspace tests passed
(259 browser, 95 core, 2 engine).

### Clear Today and archive Undo — 2026-10-23

A disposable Arc Blank Window with three test tabs established the batch
policy: Tabs → Clear Today removed the two background Today tabs and retained
the selected page. A single Edit → Undo restored both background tabs in their
previous order without changing the selected page. The restored sidebar and
page were inspected in a screenshot. This corrects Echo's earlier behavior,
which incorrectly cleared the active Today item too. The disposable Arc window
was closed after the check; no existing user tabs were cleared.

Echo now records manual Today archives in the same transient journal as pin
operations. Single tabs, whole sidebar Split View items, Clear Today and the
relative archive commands restore their identities, physical order, split
weights/name/pane focus, and original Space selection. Clear Today retains
all panes of an active split. Background auto-archiving creates no undo entry.
Undo reads payloads from the actual archive; deleting or separately restoring
an entry invalidates its transaction instead of resurrecting or duplicating
pages. Redo validates the whole batch before archiving any item and retains
subsequent navigation and other live page changes. Time is passed into Undo
and Redo, so restored pages get a fresh activity timestamp. Closed engine views
are recreated when restored pages become visible.

Undo from another Space settles the sidebar back to the source Space. An empty
GPUI editor now consumes text Undo/Redo rather than bubbling it to a sidebar
action; the History search field has dedicated regression coverage. Redo
remains inert while an IME composition is active.

Native verification used a separate seeded profile with a two-pane split, a
single Today tab and a foreground page. ⌘W → ⌘Z restored the single tab in its
original position; ⇧⌘Z and native Edit → Undo repeated the archive/restore.
Native Tabs → Clear Today preserved the foreground page while archiving the
split and single tab as two entries. One native Edit → Undo restored both;
selecting the split showed its two rendered CEF pages. Edit → Redo removed
both items again, and a second Undo rendered the complete split again. All
five screenshots were inspected; window inventory confirmed key=true and
onScreen=true. No system cursor movement or clipboard operation was used.
All 369 workspace tests passed (265 browser, 102 core, 2 engine).

Individual split-pane closure and Pinned/Favorite page unloading do not yet
have undo entries. Other sidebar operations, Peek and action-specific command
suggestions also remain unfinished; the overall Undo/Redo parity row stays
partial.

### Individual pane-close Undo — 2026-10-23

The installed Arc was checked with disposable two-pane example.org/example.com
splits in Blank Windows and a separate normal New Window. View → Close this
Split Pane removed the selected pane and collapsed the remaining page into a
standalone item. Edit → Undo Archive Tabs reconstructed the original split.
The address display and blue selection outline stayed on the surviving pane;
a subsequent Close this Split Pane removed that survivor, confirming the
selection. Accessibility still reported the restored web content as focused,
so its focused-element line alone is not evidence of Arc's selected pane.
The restored split was inspected in a screenshot.

Redo is asymmetric: after Undo has reconstructed the split, Edit → Redo
archives the entire group, rather than removing only the original closed pane.
This was reproduced twice in Blank Windows (closing the temporary window)
and in the normal window (selecting its next unrelated Today item). A second
normal-window check first pinned the test split; Redo removed that entire
Pinned group too, rather than leaving it unloaded in Pinned. The test group's
absence from the sidebar was checked. No pre-existing user items were
archived, and the extra normal window was closed afterward.

Echo's pane journal restores the closed page identity, anchor, original pane
position and weight, while keeping the survivor selected and retaining its
live page. Surviving panes' later geometry ratios and group customizations are
retained. Collapsed pairs regain their group metadata; Pinned folder placement,
pin URLs and the core model's shared Favorite selections are covered. Redo
archives the current complete group as observed in Arc, and subsequent Undo
restores that group to its saved section and folder. Regrouping/moving pages,
explicit archive restoration/deletion and a subsequently full Favorites dock
invalidate stale entries without duplicating or resurrecting pages.

Native Echo verification caught a responder issue not represented by the old
fake engine: creating a restored CEF view took focus even when the intended
active survivor had not changed. Page reconciliation now reasserts the
intended native responder after all view creation. A GPUI regression test
models CEF's focus-on-create callbacks and verifies that the survivor remains
selected. The View menu label now matches Close this Split Pane.

The final native checks passed on two separate fixture profiles. In Today,
⌘W closed the right pane; text on the surviving left page was changed to
`SurvivorEdited`, then keyboard Undo restored the right pane while preserving
that text, the left selection and its button's native focus. Native Edit →
Redo archived the complete group; Undo restored it again. Closing the original
anchor through the View menu and undoing it preserved the right survivor's
selection. Keyboard Redo again archived the complete group.

In the second profile, the split started in a Pinned folder. Native View/ Edit
commands restored the collapsed split inside that folder, archived the entire
Pinned group on Redo, and returned the full group to the same folder on the
next Undo. All seven final screenshots were inspected; both runs reported
key=true and onScreen=true. No system pointer movement or clipboard operation
was used. All 382 workspace tests passed (269 browser, 111 core, 2 engine).
Favorite-group restoration has model coverage; these native runs cover Today
and Pinned folder groups. Pinned/Favorite unloading, other sidebar Undo actions,
Peek and dynamic action labels remain unfinished.

### Cancelled split drafts and Redo — 2026-10-23

In a disposable Arc Blank Window, a Today tab was archived and restored with
Undo. View → Add Split View opened an unfinished pane; Escape cancelled it.
Native Edit → Redo still rearchived the original restored tab, leaving its
previous neighbor open. Cancellation therefore must not consume or replace
the preceding redo action. The disposable window was closed afterward.

Three new GPUI tests reproduced Echo's failure before the fix: cancelling a
normal draft destroyed pin Redo, cancelling a copied Pinned group destroyed
archive Redo, and cancelling with Cmd+W destroyed pane-close Redo. Draft
rollback had used recorded pane/tab closure before truncating the archive,
which left invalid history entries and cleared the redo stack.

The model now has an explicit draft-discard operation. It removes the
uncommitted pane and, when requested, the entire protected-item copy without
recording closure or adding to the archive. The original Pinned/Favorite item
is retained. Replacing a draft with an existing suggested tab uses the same
unrecorded cleanup. Scope/anchor validation prevents accidental disposal of
protected panes or unrelated items, and cleanup preserves the active Space
even when invoked from another Space. The browser still restores the original
group geometry and selection after cancellation.

Native Echo checks used a separate profile with a Pinned two-pane group, a
Today tab to archive/restore, and a fallback page. After opening and cancelling
a normal draft, Shift+Cmd+Z rearchived the intended Today tab. The same passed
after cancelling a draft copied from the Pinned group; both original pinned
pages remained, with no Today copies. Cmd+W cancellation also preserved the
prior Redo. The root-window draft preview and three post-Redo screenshots
were inspected; inventory reported key=true and onScreen=true. The root-only
draft preview does not capture the child Command Bar and is not evidence of
complete overlay visual parity. No mouse movement or clipboard was used.
All 387 tests passed (272 browser, 113 core, 2 engine).

A further Arc check committed a new third page in a split after restoring an
archived second tab. Redo still removed the original second tab, leaving the
new third page and the first standalone tab. Echo currently invalidates a
whole-item archive redo when its target has been regrouped. This is a known
remaining mismatch, distinct from pane-close Redo's whole-group behavior, and
is tracked in the partial Undo/Redo row. The temporary Arc window was closed.


### Shared normal windows and live page ownership — 2026-10-23

The earlier installed-Arc check recorded shared Today, Pinned, Favorites and
Spaces, empty per-Space selections in a new window, a focused Command Bar,
and a single live document when the same tab is selected in another window.
This implementation keeps the normal profile and CEF page pool together while
each window owns its selection, layout and native parent. Pages are keyed by
TabId, so two tabs with the same URL remain distinct documents. The active
window transfers an existing native view instead of creating another browser.
Renderer callbacks are delivered once, in FIFO order, to the current host.
Background metadata updates do not replace the session's selected window.

A bundled Echo run used a fresh profile and a local form fixture. It loaded
/A then /B, typed `pool-before`, opened a new window with Cmd+N, selected the
existing tab through the Command Bar and appended `-new`. Returning to the
original window retained the same document token and input. The second window
then reclaimed the page and was closed with Shift+Cmd+W; the original window
accepted `-closed` in the same input. All five /B dumps carried document token
`180587e2-216e-4a50-86f5-158af88c469e`. Fixture requests were exactly
`/A`, `/B`, `/A`, with the final /A reached through Back. No /B request was
repeated during transfer or window closure. The empty-window Command Bar,
second-window page, restored original page, post-close input and Back result
were visually inspected. The guarded script completed without focus aborts
or panics; an earlier run interrupted at an intentional activation was not
counted as a complete verification.

A second fresh-profile native run invoked File > New Window from Work. The
new window inherited Work and had an open, empty Command Bar; Research and
Personal also had no selected page. The named New Window action opened another
normal window. After loading a disposable private-only page, Cmd+N from its
native page opened a normal window with the retained regular sidebar and no
private tab. Both final Command Bar screenshots were inspected. No system
mouse movement or clipboard was used.

All 426 workspace tests passed (301 browser, 123 core, 2 engine), including
shared-page event order, activation with a stale prior URL, inline-editor
focus, split-pane transfer, closure of the hosting window, transfer rejection,
same-URL distinct tabs, menu/shortcut/Command Bar routing, private-to-normal
isolation and normal-profile retention after all windows close. Inactive
windows do not yet mirror a page hosted in another window; their exact Arc
representation still needs a simultaneous-window reference capture. Windows
and X11 native checks remain pending. New Window therefore remains partial.


### A shared tab's inactive-window presentation — 2026-10-23

Two disposable normal Arc windows displayed the same local fixture on two
fully visible displays (3840×2160 and 1920×1080). The fixture included an input,
a half-second counter, a document UUID and copies of those values fixed at
the right edge. It was created and selected using native keyboard/accessibility
actions. AppleScript was authorized for window inspection and screen capture;
System Events window positioning lacked Assistive Access, so the two existing
displays were captured together instead. No permission or mouse position changed.

Window A initially showed `reference-A`. After the same tab was selected in B,
A retained a dimmed, frozen image with `reference-A`, counter 68 and its original
viewport layout. B remained live, with `reference-B` and counter 269. Both carried
document UUID `c2cf31bb-1b53-442e-bd38-49fe4e76d11c`. Source A's address field
became the Search or Enter URL placeholder while its selected sidebar item
remained. This is a per-window frozen image, not a continuously updated mirror.

Cmd+` returned to A without navigation: it showed `reference-B`, counter 315
and the same UUID at A's own viewport size. Inactive B retained `reference-B`
and counter 301 at its original size. The simultaneous before/after and return
captures were inspected. A roughly 30% dark overlay describes flat inactive
page pixels; this estimate does not establish exact display-color calibration.
The local tab was archived and both disposable windows were closed afterward.

Echo's CEF layer now exposes asynchronous viewport capture of the existing
windowed browser through the local DevTools observer transport. It does not
create a browser, navigate, change device metrics or open DevTools UI. Results
are correlated with caller generations; busy requests, submission failure,
detach, timeout and closing a view are covered by seven engine tests. A native
probe returned four page-only PNGs of the existing Home document before and
after native host transfer. The per-window freeze/restore UI and marker-input
validation remain separate integration work; this primitive does not complete
New Window parity.

### Developer Tools opening and toggling — 2026-10-23

In a disposable normal Arc window on the same local fixture, Option+Cmd+I
opened the original DevTools frontend embedded to the right of the page,
with Elements selected. The sidebar remained unchanged and a vertical splitter
separated page and tools. This is the current profile's remembered docking
state, not proof of the factory default. A screenshot and accessibility tree
were inspected. Repeating Option+Cmd+I closed the frontend and retained the
fixture document UUID. Option+Cmd+J from closed opened Console in the same
embedded layout; repeating it closed DevTools again. The temporary fixture tab
was archived and its window closed. Other dock positions, Split View placement,
Inspect Elements, Network Inspector and dimension persistence need native checks.


### Echo frozen-window integration — 2026-10-23

A bundled Echo run used a fresh profile, local input/counter/UUID fixture and
focus guards. Two 1200×800 roots were placed side by side without moving the
pointer. Both reported onScreen=true. After transfer, source A showed the
frozen, dimmed `source-A` at counter 3 and the address placeholder. Active B
showed `source-A-live-B` at counter 5. Both displayed UUID
`4ed46dbe-de4b-49f8-a2c8-919b38618fa6`. Returning the native document to A
preserved the value; appending `-returned` worked without recreating the page.
Source, active and returned screenshots were inspected. This run exercised
native host-transfer diagnostics; production New Window routing is checked
separately. Captures are bounded by a 250 ms handoff fallback; a failed capture
without a valid previous frame may leave no inactive image. Windows/X11,
minimization, hidden surfaces and exact display-color calibration remain
unverified, so New Window stays partial.

The workspace suite passed 457 tests: 327 browser, 123 core and 7 engine.
Coverage includes capture ordering, per-host frozen images, cancellation,
source closure, stale results, geometry, deferred navigation and UI focus.

A second guarded native run exercised the production Cmd+N flow. The new
window's focused Command Bar selected the existing fixture tab from its tab
suggestion (Down, Enter). With both roots on screen, source A froze `cmdn-A`
at counter 3; B showed `cmdn-A-cmdn-B` at counter 4, sharing UUID
`96ef0781-142a-4426-97aa-8b4f60271714`. Closing B with Shift+Cmd+W and activating
A retained that UUID and accepted `-host-closed` at counter 7. All three images
were inspected. No extra fixture tab or reload was created. The diagnostic
window selector now sorts native root window numbers rather than depending
on App::windows hash-map iteration order.

### Echo DevTools transport and asset probe — 2026-10-23

A native bundle probe loaded CEF 154's original inspector.html as an ordinary
Alloy child in the same GPUI window. The original Elements/Console toolbar and
Styles panel rendered; screenshots were inspected. No ShowDevTools window or
remote-debugging port was used. The frontend has no host adapter yet, so these
panels remain disconnected and DevTools parity stays missing.

A live localhost fixture retained its input and document across closing and
reopening that frontend. Two independent Runtime.evaluate replies returned the
same diagnostic UUID `75793a24-3325-4b4d-8f81-1aa20b81d5bb`. A concurrent page
snapshot also completed and its image was inspected. This establishes bundled
asset availability and local transport correlation, not frontend readiness or
private storage isolation. The workspace suite passed 466 tests (327 browser,
123 core, 16 engine), including sessionless child-target errors and shared CDP
request IDs. Renderer host injection and authenticated readiness come next.

### Empty Favorites reference supplied by the user — 2026-10-23

Two user screenshots establish the normal empty state and its collapsed state.
The first has a rounded dashed card below the address field, approximately
234×95 px in a 247 px-wide sidebar. A circle with a star sits above the bold
“Drag to add Favorites” title. Its subtitle wraps as “Favorites keep your most”
and “used sites and apps close”; a small vector close control is at top right.
The Space header follows below it. The second image has no card, and the
Space header and Pinned/Today sections move upward without a reserved gap.
No Favorites or profile content was changed for these supplied references.
The visibility and geometry of a dismissed empty dock during dragging remain
unobserved. Image noise prevents treating sampled colors as exact calibration.

Bundled Echo's 95 px card, close control and collapsed state were inspected
against these screenshots. Clicking close removed the full card plus its 8 px
sidebar gap; restarting the same profile kept it dismissed. In a guarded native
drag run, moving the live Today fixture onto the empty card made it the first
Favorite. Its `favorite-live` input and document UUID
`4455791d-96f0-4b38-8a9e-67134e4986c8` survived, with one history entry and no
reload. The resulting window and page PNGs were inspected. The full workspace
passed 472 tests: 332 browser, 124 core and 16 engine. Exact drag-after-dismiss
Arc states and theme-dependent contrast still need reference checks.

The parent automation now exposes fixed inspector open/status/evaluate/close
commands. A later native run verified close restores the inspected page's
responder: `after-inspector` was typed into the retained fixture after closing,
and a page PNG showed that input with the original fixture UUID. Diagnostic
identifiers use getRandomValues rather than the secure-context-only randomUUID.

### Developer command targeting follow-up — 2026-10-23

In a temporary normal Arc window with a localhost fixture, Inspect Elements
opened Elements; repeating its shortcut kept DevTools open. Network Inspector
from the Command Bar opened Network, and repeating the same action closed the
frontend. Opening JavaScript Console while Elements was visible switched to
Console rather than closing DevTools. These were checked through the native
accessibility tree. Exact selected-element overlay and repeated inspect-mode
state still need visual confirmation. User input interrupted reference cleanup;
the uniquely named temporary Space is tracked for later removal.


## Installed reference version and hidden sidebar shortcut (2026-10-23)

The installed Arc `Info.plist` now reports version **1.170.0, build 89123**.
The parity inventory was assembled from 1.169.0; the previous observations keep
their original version scope. No automatic behavioral equivalence is assumed.

Static inspection of the current `Base.lproj/MainMenu.nib` distinguishes:

- Hidden View action **Reveal Current Tab**, Control+Space: `focusSidebar:`.
  The nib permits the shortcut while hidden.
- Tabs action **Reveal Tab in Sidebar**, no shortcut: `revealCurrentTabInSidebar:`.

The different selectors do not prove equivalent keyboard focus or reveal
behavior. Echo already implements the second action (expanding ancestors and
scrolling to the active item/split anchor), but Control+Space must not be wired
as an alias until native checks establish the first action's behavior, including
hidden sidebar, page input, Favorites, Split View and empty-window contexts.
This was a bundle-only audit; no native behavior was observed for the hidden
action.

## Profile history and Blank Window (2026-10-23)

Arc's [official Profiles documentation](https://resources.arc.net/hc/en-us/articles/19227964556183-Profiles-Separate-Work-Personal-Browsing)
scopes browsing history and site storage to a Profile. Spaces assigned to the
same Profile share history; different Profiles have separate histories. Consult
documented contracts first, reserving native checks for undocumented behavior,
visual presentation and input/focus details.

Native observations below use installed Arc 1.170.0 (89123), disposable localhost
fixtures and individually tracked test windows:

- File → Blank Window opens an empty Today list with the Command Bar focused.
  Its sidebar has the profile header and New Tab, with no Favorites, Pinned,
  Space header or Space switcher. Pin Tab and New Folder are disabled.
- Two Blank windows have independent Today lists. Their tabs do not appear in
  the regular window's tab inventory.
- Regular and Blank fixture documents share the selected normal Profile's
  cookie/localStorage marker. Creating Blank from Incognito also uses the
  normal Profile: the private source had no marker, while the Blank saw the
  normal marker. This does not establish a multi-profile selection rule.
- New Window invoked from Blank creates a regular window with the retained
  regular Space and normal sidebar, rather than another Blank model.
- Archiving a nonfinal Blank Today tab keeps that window open. Restore Last
  Closed Tab restores locally into that Blank with a fresh document UUID.
  Archiving its final Today tab closes the Blank window. Another Blank's
  restore stack and the regular window's restore command stayed empty.
  Global View Archive contents were not inspected.
- Regular History showed visits from both Blank windows and the Blank created
  from Incognito, but not the private source visit, consistent with the
  documented Profile scope.

The owned storage marker was cleared and the test windows closed. A previously
created temporary reference Space remains tracked for cleanup. Profile chooser
behavior, global archive presentation, relaunch persistence, and the exact
creation shortcut's native input path remain unverified. The bundle establishes
Control+Command+N; a tool key attempt alone did not establish shortcut behavior.

## Auto Archive interval evidence boundary (2026-10-23)

The [official Auto Archive article](https://resources.arc.net/hc/en-us/articles/19228855311127-Auto-Archive-Clean-as-you-go)
documents a 12-hour inactivity default, per-Profile timing, and says Auto Archive
cannot be disabled. Its text was available through the official-domain search
index; direct article fetching failed, and browser opening was not verified.
Installed Arc 1.170.0 contains compiled threshold descriptions for Never,
1 hour, 6 hours, 12 hours, 24 hours, 7 days, 30 days and an internal-only
20-second option. These strings do not establish release-menu availability.
The existing Echo searchable picker and its current-value-first ordering have
not been matched to the native settings control. Interval parity therefore
remains partial pending release-menu inspection and Profile propagation checks.
Incognito's internal disabled timer and Little Arc's separate interval must not
be changed solely on this public-menu evidence.

## Echo Blank Window acceptance (2026-10-23)

The immutable bundled build passed guarded native input checks through
Control+Command+N, File → Blank Window and the Command Bar action. Empty Blank
opened with focused input; stack screenshots showed its profile header and
New Tab without Favorites, Pinned or Space controls. Two simultaneous Blanks
kept distinct Today lists and document UUIDs; closing one left the other intact.

Regular and Blank documents saw the same owned cookie/localStorage marker.
The Incognito source saw neither; Blank created from it saw normal storage.
New Window from Blank returned to the retained regular Space/sidebar. Regular
History included Blank visits and excluded the private source. Closing a
nonfinal Blank tab and restoring it produced a fresh document UUID; closing
the last tab removed that root while the original normal document survived.
The resulting PNGs were inspected. The first automation attempt used the
unsupported `return` key token and was discarded; acceptance used `enter`.

The workspace passed 526 Rust tests (366 browser, 132 core, 28 engine), plus
8 installed-CEF Bun contract tests with 82 assertions. Regression coverage
includes independent overlapping TabIds, profile-specific history, retention
of the normal session writer after its windows close, persistent-sidebar
restrictions, split/draft behavior and final-removal cleanup. Named profile
selection, global Archive, relaunch behavior and exact remaining visual details
are not claimed complete.

## Profile settings and release interval menu (2026-10-23)

Native Arc 1.170.0 Settings → Profiles shows a left profile list with per-profile
Space counts, plus Add and Remove controls. The selected Default profile's
Remove control is disabled with “The Default Profile cannot be deleted.”
The right side includes search engine, search suggestions, Archive tabs after,
new document app, download location, and data/settings links. No setting value
was changed.

The expanded Archive tabs after menu contains exactly these choices in order:
**12 hours (default), 24 hours, 7 days, 30 days**. The existing 7-day selection
was checked in the third position. Never, 1 hour and 6 hours were absent.
Both the accessibility menu and its screenshot were inspected. This supersedes
the earlier bundle-only uncertainty for public choices; Profile propagation
and Little Arc intervals remain unverified.

Add opens a creation sheet with an initially focused name field, placeholder
“Personal”, Cancel and disabled Next. A nonempty test name enables Next. The
second step asks which Spaces should use the Profile, displays Space cards
with all toggles initially off, and has Back and disabled Create Profile.
It warns that assigned Spaces start fresh and authentication/passwords/extensions
must be set up again. The first step says Profiles are not synced between
computers. Screenshots of both steps were inspected; Back retained the test
name. Creation was cancelled before applying anything, and Settings was closed.
No profile was created, reassigned or deleted. Validation beyond nonempty text,
successful creation, assignment consequences and switching/deletion dialogs
remain unverified.

Bundled Echo's existing Command Bar interval picker now offers only those four
public choices in fixed order. A guarded native run selected 7 days, reopened
the picker with Current on the third row, and confirmed that entering Never
offered no action and left the saved 7-day duration unchanged. Stack screenshots
were inspected. The full workspace still passed 526 tests; legacy None/custom
values are preserved on cancellation or unmatched input. This verifies the
choice correction, not parity of the still-missing native settings control.

Reference cleanup constraint: the user explicitly requested retaining the
temporary `Echo Favorites Reference 20261023` Space for manual deletion.
Do not delete that Space during automated cleanup.

### Successful disposable profile creation follow-up

File → New Profile from the temporary reference Space uses the same two-step
sheet as Settings Add, but preselects only the originating Space on the second
step. The parent created `Echo Profile Reference 20261023` with only
`Echo Favorites Reference 20261023` selected. The resulting Space had empty
Favorites and retained its existing name. No other Space was reassigned.

Spaces → Change Profile opens a sheet naming the current Space, warning about
different browser data/logins/Favorites/extensions, and showing a profile
dropdown with Cancel and Change Profile. The dropdown lists existing profiles
and New Profile; the newly created profile was selected. Its layout was
inspected, but the switch was cancelled. User activity interrupted further
reference checks. The parent closed only its disposable window
`9F759E29-D319-48F3-9DA1-86A2370BC5E1`, leaving the test Space and profile
intact. Loaded-tab reassignment, successful switching, deletion and timing
propagation remain unverified.

### Loaded-tab profile switching follow-up

With a fresh owned normal window and the retained test Space, a localhost
fixture in `Echo Profile Reference 20261023` stored an owned cookie/localStorage
marker. It then navigated to a marker-reading URL with no write query.
Spaces → Change Profile selected the renamed Default profile, Grosst.
Clicking Change Profile opened a second confirmation, rather than applying
immediately: “Switch to your Grosst Profile?”, explanatory text about different
browser data/Favorites/extensions, “This action cannot be undone.”, and
Cancel / Switch it up. Only the final button completed the switch.

The Today item retained its URL and reloaded. Fixture reports changed document
UUID from `ec416636-c747-4046-8ff4-7c4697a02ec7` to
`22e04417-082e-4834-8c95-f2bfefa6e752`; the target document saw neither owned
cookie nor localStorage marker. The resulting page screenshot was inspected.
The parent archived only its test tab and closed owned window
`AC6CF7C9-840C-4803-8AF5-3A06595A16DC`. The Space remains intact, assigned back
to Default; the test profile remains intact and is now unassigned. Other
Spaces/profiles were not changed. The user granted a short focus slot; slow
native observations extended it, and focus was released after cleanup.

### Profile creation visual correction and local storage acceptance

A subsequent direct comparison found that Echo's initial creation form did
not match Arc: its shortened explanatory copy and minimum-height layout packed
the controls at the top and left excess empty space below the buttons. This
presentation is not accepted as visual parity.

Echo intentionally omits the synchronization notice and its icon at the
owner's explicit request. Do not reintroduce them during reference matching.

Arc 1.170.0 File → New Profile opens an accessibility sheet attached to the
originating window. Native accessibility measurements are 450 × 426 for the
name step and 450 × 506 for Space assignment, superseding the earlier rounded
425/495 targets. The name step describes separate history, saved passwords,
extensions and other browsing data; its input is about 48 px high. The second
step explains logging back into websites and switching back to the previous
Profile. Five 64 × 95 Space cards fit across the sheet. Screenshots of both
steps were inspected. Creation was cancelled, and only owned window
`F6B20C7C-62E7-4EA5-89BA-0C24A8901B52` was closed. No Profile was created or
reassigned; the user-retained reference Space was untouched.

An isolated Echo workspace containing Default and two named profiles passed
a full process restart against one localhost origin. Each profile retained its
own session cookie, persistent cookie, localStorage and IndexedDB marker.
The reading URLs had no write query, and the reloaded documents had fresh
UUIDs. Screenshots for all three profiles were inspected. A production
DevTools Elements panel also displayed the named profile's live document.
Both automation Quit and native-keyboard Cmd+Q reached logged CEF Complete
with zero owners, browsers and cookie flushes before process exit. This checks
the tested bundle's storage/shutdown foundation, not the later input-ordering
fixes, private cleanup, final dialog presentation, or other-platform behavior.

A later isolated bundle verified the private-session lifetime with the same
localhost fixture. Two concurrent Incognito windows shared session cookies,
persistent cookies, localStorage and IndexedDB. After both windows closed,
a new Incognito window read null for all four stores. The screenshots and
fixture reports were inspected. The native script must retain the newly opened
Command Bar child's focus: explicitly activating its root first redirects
typing away from that input. This check does not establish OS-initiated quit
acceptance; the AppKit termination path is still under correction.

The same bundle's Profile creation screenshot confirms that the requested
synchronization notice and icon are absent. Switching a test Space from a new
empty profile back to Storage B required the second confirmation, retained the
Today URL, created a fresh document, and restored Storage B's four independent
storage markers. The full restart checker passed again. Dropdown selection
was exercised with Enter and Home/arrow navigation; native Space-key activation
needs a separate check. Switching-sheet visual parity remains unaccepted.

Native malformed-session acceptance used a separate directory containing an
invalid version-2 session. Echo displayed a Retry-only recovery window, left
the original bytes unchanged and created no Chromium storage files. Ordinary
Cmd+Q reached CEF Complete with zero owners/browsers/flushes and exited. This
checks preservation rather than replacing an invalid workspace with demo data.

A Blank opened from Storage B read that named profile's four storage markers.
After closing the last normal window, the surviving Blank opened another local
fixture page and quit through the complete CEF barrier. The retained normal
session writer saved the new visit only in Storage B's history; its normal
Today inventory excluded both Blank tabs. The Blank screenshot and saved
session were inspected. Blank relaunch/profile-chooser parity remains separate.

The corrected native pre-quit path passed both a direct AppKit terminate call
and a standard AppleScript quit addressed to the isolated Echo bundle. The
AppleEvent was suspended before Cocoa handling, then resumed through its
preserved original handler only after CEF Complete with zero pending owners,
browsers and cookie flushes. AppKit legitimately returned noErr before one
later terminate continuation; the original reply was preserved. Both sender
and application exited 0 without forced cleanup. Default and both named
profiles again passed the full restart storage checker. Actual OS logout,
restart, joined-event scenarios and other-platform termination remain untested.

Additional Change Profile reference: in an owned empty New Window, the first
sheet measured 450 × 338 px. Its body says “Each Profile has different browser
data, so you might get logged out of some websites, see different Favorites
up top, or need to re-install extensions.” The row labels the originating
Space's Profile and uses a compact selected-profile pill. With no selected page
in that window, Change Profile applied immediately rather than displaying the
loaded-page confirmation. This unexpectedly changed Grosst's assignment during
inspection; the original Grosst Profile was immediately restored and its
Favorites verified. Owned window `B1627C9F-22A4-45CE-B681-E1B3A565D2BE` was
closed. No Space was created or deleted. Do not generalize the confirmation
condition beyond this empty-window observation without further inspection.

The first chooser with the longer test Space name `Echo Space Reference
20261023` measured 450 × 357 px. Its heading wrapped onto two lines rather
than truncating, shifting the body, row and footer down. The short-name
450 × 338 measurement is therefore not a fixed height for all names. This
chooser was cancelled without reassignment; the owned local reference tab
and window `BC4530E7-BB86-4E4B-B224-F047B911CB55` were closed afterward.

The corresponding Echo bundle renders a 357 px chooser with the full 12 px
Space row label and compact selected-profile pill visible together; its footer
remains inside the sheet. The screenshot was inspected. Exact heading word
wrapping still differs from Arc, so typography and full visual parity remain
partial. GPUI's NoopTextSystem uses fixed-width test metrics and cannot prove
the native font's full-label fit by itself.

### Sidebar Space creation and Library → Spaces

The user explicitly identified four creation entry points: the sidebar + menu,
Spaces menu, swiping to the creation page at the right edge, and the far-left
Home/Library → Spaces view. They also requested reproducing the Library view.

Direct Arc 1.170.0 checks confirmed that both Spaces → New Space and sidebar
+ → New Space insert a temporary Create Space page immediately after the
originating real Space. This is a sidebar page, not a Command Bar or central
dialog. The name input receives focus. The sidebar shows a Create a Space
illustration/title, “Separate your tabs for life, work, projects, and more.”,
name/icon row, Profile selector with information control, and Choose a Theme.
Create Space and Cancel are anchored above the bottom dock; Create is disabled
for an empty name. The main content card becomes an empty themed surface.
Choose a Theme opens an anchored popover with color canvas, swatches, theme
mode and intensity/noise controls. No theme was applied to a real Space.

Home opens Library in the same browser window. Its left navigation includes
Media, Downloads, Easels, Spaces, Boosts and Archived Tabs. Spaces displays
horizontally arranged tall cards containing each Space's sidebar, profile and
theme controls, drag handle and overflow menu. A circular + follows the final
card. Clicking it returns to the same sidebar creation page. Cancel returns to
the originating real Space rather than Library. Screenshots of Library's Media
empty state, Spaces overview and the creation page/popover were inspected.
Easels remain outside Echo's existing agreed scope.

Cmd+Option+Right from the final real Space wraps to the first real Space;
it did not enter creation. The user-reported right-edge swipe entry still
needs a true trackpad-phase check; a generic horizontal mouse-wheel event did
not reproduce it. Profile dropdown contents and final successful creation
placement remain unverified. All drafts were cancelled. Owned reference window
`B66C4814-259D-4059-A337-EF32913EF1C0` was closed; no Space or Profile was
created, deleted or reassigned, including the user-retained reference Space.

With the owner's later permission to create test Spaces, an owned Arc window
created `Echo Space Reference 20261023`, ID
`15EE919F-EB0C-4F24-BDD4-718BE92F521C`, from Grosst. The committed Space replaced
the virtual creation page immediately after Grosst, before the preexisting
Favorites Reference Space. It became selected with no selected tab and no
Command Bar. It retained the originating Grosst Profile and therefore showed
that Profile's existing Favorites. The new Space remains for the owner to
delete manually. Owned window `DCDCDCE3-66D3-4261-B99A-A6107F35ACD4` was closed;
no Space was deleted. Creation-draft Profile-menu contents remain unverified.

In a later owned Arc window `FB13DAA7-44D9-4EBC-9BCD-7A513708E33E`, a named
creation draft received Return while its name field was focused. The draft and
name remained; Return did not create a Space. Escape from the focused name
field moved focus to the window while keeping the draft. A second Escape with
the window focused cancelled the draft. The owned window was then closed.
No real Space was created, deleted or reassigned during this keyboard check.
Return's exact focus result was inconsistent between accessibility captures.
Later inspection found a real `Echo Enter Reference 20261023` Space in owned
window `A67B9EAD-A154-4233-AA82-4458797CBEBA`; the intervening input delivery
was not reliably bound to that window. The earlier observation therefore does
not establish a universal Return-submission rule. Recheck keyboard behavior
with safely bound native input before claiming parity. No Space was deleted.

### Library Downloads: initial list and profile filtering

A delegated native check in owned window
`701EB055-383A-4BFA-ADBD-77B507E02755` observed Cmd+Shift+J opening Library's
Downloads section in the same window. The Window menu includes View Downloads.
The left navigation is approximately 112 px wide; the adjacent download-list
column extends to approximately x=416 in the 3840 × 2130 capture. With no item
selected, a large themed preview surface remains blank to its right.

The list has Search Downloads… and Filter controls and date-grouped rows with
filename and muted type/source text. Filter replaces that toolbar inline with
Filter Downloads…, Done, a Profiles heading and wrapping profile chips. Toggling
a chip updates results immediately; toggling it off restores the unfiltered
list. Multi-selection and filter persistence were not tested.

The filtered-empty screenshot was also inspected by the parent. Its download
arrow, “Nothing here yet!” heading and “Come back after you’ve found some / cool
stuff to save.” subtitle are centered within the narrow list column, not the
large preview surface. This establishes filtered-empty presentation only.
Search semantics, context actions, active/missing downloads, file opening and
download-location settings remain unverified. No download was started or file
opened. The owned window was closed and Space inventory was unchanged.

The delegated check accidentally clicked the protected YouTube window after a
focus change. Its final accessibility state showed playback running and the
window remained open. A stale-index capture from that window was rejected as
Downloads evidence. Future native actions must verify the exact owned window
against a fresh accessibility state immediately before interaction.


### Echo sidebar creation and Library acceptance, 2026-10-23

The immutable `/tmp/echo-space-library-qa-r2.app` passed both edge gestures in
an isolated retained session: swiping left from the final real Space opened
the right-edge Create Space page; swiping right from the first real Space
opened Library. The menu creation route displayed the responsive sidebar form,
fixed dock, retained draft marker and window controls. Empty content contained
no invented placeholder or Command Bar. The Library rail rendered its six
centered icon tiles. Screenshots were inspected directly.

The r2 check exposed a production rendering defect: Library card titles
clipped despite full saved values and passing test-platform geometry checks.
The r3 bundle fixed this by shaping fresh complete font runs against final
label bounds. Native screenshots confirmed GitHub, Gmail, Home, GPUI and
Arc is shutting down in full, both idle and hovered, in Library and the normal
sidebar. A server-supplied title containing CJK, combining accents and a
family emoji rendered correctly and ended with an ellipsis. Sending non-BMP
characters through the automation typing command corrupted its URL; the
Unicode acceptance therefore used an ASCII URL and server-owned title.
The strengthened regression inspects
actual painted text and glyph indices, not only container bounds.

Native acceptance remains partial overall.
Theme, icon and draft Profile pickers remain unimplemented. The test process
completed orderly shutdown with zero CEF owners, browsers and cookie flushes;
no live Space was deleted.


### Sidebar resize foundation and Echo acceptance

Official Arc release notes document edge dragging, double-click reset and
all-left collapse. Static inspection of installed Arc 1.170.0's x86_64 pan path
found rounded beginning-width plus translation, an open range of 164–500 raw
units, changed-state collapse below 82, and a mode-zero end branch at or below
82. The reset path computes 228. These are raw implementation values; the
conversion to screenshot interior/outer width and semantic mode-tag identity
were not established. Snap velocity, eligibility, timing and haptics need native
comparison. No additional live Arc drag was attempted after unsafe window binding
was observed.

Echo's isolated macOS resize r2 bundle passed docked edge dragging across CEF,
minimum/maximum width, double-click reset, reset → collapse → reopen, and held
mouse-down → Cmd+S cancellation. A local fixture kept one document UUID and one
load while native page bounds changed; continued typing preserved its input.
Screenshots were inspected. Full workspace tests passed 751 cases. Window width
is local and unpersisted; new-window inheritance and exact Arc feel remain open.

The floating panel appeared, but its resize probe used root-window automation
and an inner-edge coordinate that missed the child handle. It is unverified,
not a demonstrated product failure. The test process exited normally after CEF
Complete with zero owners, browsers and cookie flushes. No Space was deleted.


### Media list/preview presentation correction

The owner's supplied image showed a narrow Media search/list column and a
separate large empty preview on its right. Echo's full-width centered placeholder
was incorrect. The new presenter keeps the existing shared Library rail, a
provisional 284-unit list width and independent preview. The source raster's
106/284 pixel measurements are not established logical-point dimensions.

The immutable combined Media/material/drag r1 bundle was inspected directly.
Clicking the search icon accepted a query; switching to Spaces and back retained
it. The empty group stayed centered in the list rather than the preview. Theme
colors still use the existing host palette. File enumeration, result matching,
file actions and the Learn more target remain unimplemented. Material foundation
registration introduces no palette or visible opacity change by itself.
Full workspace tests passed 765 cases. No Space was deleted.
