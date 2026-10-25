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


### Floating resize key-window transition correction

A later diagnostic probe established actual child targeting: the drag reached
the floating handle and installed a resize gesture, but the parent root's resign
notification immediately abandoned it before the first move. The failure was
therefore confirmed in Echo, beyond the earlier harness limitation.

The corrected owner tracks activation of its root and exact floating child,
allows their paired key transitions to settle for one GPUI frame, and preserves
only that same gesture/hierarchy when either window remains active. Moving to
an unrelated window or leaving the application still cancels in regression tests.
It does not use GPUI's `App::active_window` as focus evidence: that macOS API
returns mainWindow rather than keyWindow.

The immutable hierarchy r1 bundle passed native floating resize from 240 to 396,
including actual child-window size updates and docking at the retained 396 width.
Screenshots were inspected. The fixture retained UUID
`66374c7b-45bb-411c-aebd-e5bb792519d6`, one load, and continued input afterward.
The process exited normally after CEF Complete. Full tests passed 767 cases;
external native deactivation and exact Arc floating-mode semantics remain open.


### Theme material layering: compiled reference

A bounded read-only audit of installed Arc 1.170.0's arm64 executable found
`WindowTheme` separating window appearance, background and color palettes.
Its background configuration independently stores base/overlay colors,
translucency style, noise and intensity. No intensity-to-opacity formula was
established.

`WindowBackgroundTranslucencyViewAppKit` creates an NSVisualEffectView with
FollowsWindowActiveState and emphasis enabled. Material selection is separate;
compiled paths include Light, Dark and WindowBackground materials. A second
effect frame takes a configurable alpha. This does not establish a universal
material/alpha for Sidebar, Library or menus. Popover configuration separately
stores material, state, blending mode, emphasis and wash-layer alpha.

The executable hash was
`25af5f4eaaae5aca25021f2737e90cc4d41fdf195c4a5c4012ea31a1bba4b86a`.
Reproduction anchors (arm64 unslid addresses): effect factory
`0x1015405c8–0x101540608`, update routine `0x10153fcc0`, configurable alpha
`0x1015401ec–0x1015401f0`, popover update `0x100a2a1f0–0x100a2ace8`.
These compiled facts support independently configurable effect layers, not a
single global opacity multiplier or an adopted reference palette.

### Normal download continuation after source closure, 2026-10-23

Native Arc 1.170.0 checks used an owned normal window and the existing test Space
`Echo Enter Reference 20261023`. A local HTTP fixture streamed two independent
32 MiB attachments. Closing the first source tab with Cmd+W and the second
source window with Cmd+Shift+W did not cancel either transfer. Server byte counts
continued increasing after each closure; both final files had the expected
33,554,432 bytes and SHA256
`46a3eca34e6d0d965224144558ccf51a14916a0075a09089f08da4868a020644`.
Library Downloads showed each fixture after its source had closed.

This establishes normal continuation while Arc remains running with other
windows. It does not identify Arc's internal host lifetime or establish final
private-window, Little Arc, Blank or application-quit policy. An unrelated
private window was left untouched. The temporary owned windows were closed;
no Space was deleted or reassigned. Detailed fixture/ownership evidence is in
`/tmp/echo-download-lifetime-native-20261023.md`. Echo's existing explicit
cancel-before-native-close capability must therefore not be adopted as normal
browser parity; normal downloads need an independently verified continuing
lifetime before that route is enabled.

### Library entry routes: installed menu and Echo acceptance, 2026-10-23

Installed Arc 1.170.0 MainMenu.nib places View Library (⇧⌘L) in Window after
Zoom/separator and before the other Library destinations/Bring All to Front.
Its metadata includes `arcTypeTravelID=openLibrary`,
`arcKeyBindingID=viewLibrary` and `toggleLibrary:`; the selector name alone is
not evidence of toggle/back semantics. Official sources also list Command-T →
Open Library:
https://resources.arc.net/hc/en-us/articles/19230634389911-Library-A-home-for-your-downloads-archived-tabs-easels-and-more
and https://start.arc.net/command-bar-actions. These source URLs were not
independently browser-opened during this change.

Echo's rebuilt bundle was checked with real native input from a loaded local
CEF fixture: ⇧⌘L, Window → View Library, Command-T → Open Library, and ⇧⌘L
from Find all reached the existing in-window Library. Returning via its Spaces
card restored input to the same document. The fixture recorded one load and
one UUID (`e75b1295-6825-4df9-adc5-ccab235f436c`) while retaining all four
post-return typing markers. Reopening Library while Spaces was selected kept
that section. Screenshots were inspected; no Space was deleted or reassigned.
The combined 803-test workspace run passed, and independent route review found
no material issue. This verifies these entry routes and document retention; it
does not establish full Library parity or private/Blank native availability.

### Download destination/search and Library retention, 2026-10-24

A separate owned Arc 1.170.0 window in the existing test Space downloaded a tiny
local attachment whose generated name already existed. It saved without a
prompt to Downloads as `echo-source-tab-reference-20261023 (1).bin`; the original
32 MiB fixture retained its expected hash. This establishes one normal-profile,
single-extension collision, not custom-folder or concurrent-name behavior.

Downloads search matched fixture filenames case-insensitively by literal ASCII
substring. It did not match their source host/port or displayed Archive type;
leading/trailing spaces and separated words were not normalized in the tested
cases. Empty search results use `No results found for “<query>”`, distinct from
the previously observed empty profile-filter copy. Filter → Done retained the
query. Unicode matching rules and other empty states remain unverified.

With an existing source page selected, both Downloads and Media retained that
visible page to the right of their list. Empty right content was observed in
windows with no selected page. The current runtime Window menu label is
`Open Library…` (the installed nib's older `View Library` is not the live label).
Invoking that menu action or ⇧⌘L while Media was open retained Media. These
observations do not establish toggle/back or file-preview behavior.

The owned window was closed, no Space was deleted/reassigned and no private
window was touched. Detailed fixture hashes and safe observations are recorded
in `/tmp/echo-download-destination-search-native-20261024.md`.

## Split View controls and divider — 2026-10-24

Inspected the installed Arc in a dedicated `Echo Split Reference` test Space,
with a fixed window frame `{100,100,1000,760}`. Only pages opened for this
inspection were split. Reference captures are retained in `target/parity/split-view/` (originally `/tmp/parity/split-view/`).
The installed shortcut is customized to Shift+Command+E (the View menu's
AX command character is `E`, modifiers `1`); Control+Shift+= did not invoke it.

- Two side-by-side pane frames occupy x=228…604 and x=614…990 in the window:
  the inter-pane gap is 10 px. Pane headers are 32 px high.
- Hovering the divider reveals a centered 2×24 px vertical thumb. In the
  stacked layout the thumb is 64×2 px. Hover must settle before pressing;
  clicking before the thumb appears can focus a page instead of resizing.
- Dragging the horizontal divider to the left clamps the first pane at
  220 px (x=228…448). The neighboring pane consumes the remaining extent.
- The focused pane has a thin colored outline; the other pane does not.
- The pane control menu has Move Left, Move Right, separator, Add Right Split,
  Add Left Split, separator, Separate Tab from Split, Convert to Vertical
  Split View, separator, Share Split View…. Move Right remains visible but
  disabled for the last pane. Mute, Copy and Close Pane are absent here;
  the header close button and sidebar pane submenu provide those actions.
- Pane-menu icons are small vector diagrams: a pane and directional arrow,
  split and plus, separation arrow, target orientation and share arrow.

Echo now uses this menu order, disables unavailable moves/additions, and
reserves a stable 1 px border outside native page surfaces for focus. Its
split dividers resize adjacent weights and preserve selection/page identities.
Exact corner contours, material/colors, drop previews and transitions are
still visual acceptance work; these observations do not establish full parity.

Additional captures: `arc-vertical-menu.png` confirms stacked menu order
Move Up, Move Down, Add Top Split, Add Bottom Split (with the same separator,
Separate/Convert/Share grouping). The initial fourth-pane capture appeared to expand the window. A later clean
CoreGraphics measurement disproved that interpretation: the frame stays fixed
and the minimum-size pane layout overflows it (see the correction below).
Echo's earlier 1160×760 / 1000×930 auto-resize behavior was incorrect and has
been removed. Those earlier Echo images are retained as before-change evidence.
The split Command Bar uses the ordinary window-wide panel; its old clipped
`echo-four-empty.png` is also retained as before-change evidence.
`arc-drop-preview.png` shows an outlined theme-filled half-pane and clipping
of the existing page; `arc-drop-result.png` shows the committed split.

The add and Cmd+W recordings (`arc-add.mov`, `arc-remove.mov`) were sampled
locally at 60 fps. No intermediate pane-layout positions were observed:
the first divider changes from y≈330 to y≈225 between the 1.017 and 1.033 s
samples during addition; the divider disappears at 1.300 s during removal.
This records the sampled behavior, not a universal animation duration under
all system loads. Both recordings retain the survivor's page.

Native Echo verification used the freshly compiled executable with the
unchanged CEF framework/helper from the base bundle, in an isolated profile.
`echo-empty.png`, `echo-two.png`, `echo-divider.png`, `echo-resized.png`,
`echo-pane-menu.png` and `echo-after-cmd-w.png` cover draft search, pane
geometry, divider hover/drag, menu targeting and closing only the focused
page. Both loaded pages survive resizing; Cmd+W archives one and retains the
other live page. Arc/Echo comparison crops are `pair-*-crop.png`; they were
viewed directly. Header materials/controls, Command Bar width/placement and
menu positioning still visibly differ and are not claimed as full parity.


## Split draft Command Bar width — 2026-10-24

A fresh owned Arc window was tracked by CoreGraphics window number under the
screen lock, and its test Space header was checked against a reference crop
before input. Captures are in `target/parity/split-view/guard-session/`.
`window.png` shows the settled ordinary Command Bar at x=118 with width 764 px
in a 1000 px window. `arc-owned-three-empty.png` and `arc-owned-four-empty.png`
confirm that split drafts retain the wide panel across pane boundaries. Earlier
captures taken while the popup was settling have a different horizontal origin.
Echo now uses the ordinary panel width and top anchor for drafts in both axes;
GPUI regressions cover empty horizontal/vertical drafts and four-pane input.


## Edge-drop Split View preview — 2026-10-24

The Arc reference `arc-drop-preview.png` clips the current document without
reflow while its target half shows the window's own theme field, with a thin
light outline and 6 px corners. Echo now uses a passive, click-through child
overlay with the same root-gradient origin (`theme_region`). The preview covers
the prospective divider gap as well as the target half, leaving a 10 px gap
between the retained document and the outlined destination. Native verification
is `echo-drop-preview-new.png`, `echo-drop-result-new.png` and
`drop-preview-native.log`; `pair-drop-preview.png` was inspected directly.
The copied Pinned/Favorite source remains intact, while ordinary tabs reuse their
identity and engine. Four GPUI tests cover committing, cancellation and protected
originals. The retained half's rounded clipping, the floating drag card and exact
activation thresholds/animation remain different; this is partial parity.


## Joined split sidebar item — 2026-10-24

Owned-window Arc captures `guard-session/arc-row-selected-idle.png` and
`arc-row-selected-hover.png` show a 36 px joined row with equal 28 px inner
pane targets. Both selected segments have the same treatment regardless of
which pane owns page focus. Hover reveals a close button in each segment and
shortens its title without resizing the segment. An inactive group retains
hairline separators. The ordinary whole-row close target is absent from an
unnamed split item; Archive Item in its context menu still archives the group.
Echo now follows this structure, with a regression closing an inactive pane
without switching focus or replacing survivor pages. Dark selected fill was
measured against the captured Space; exact light-theme and Favorite-group
presentation remain partial.


Pane header placement was checked in `guard-session/arc-pane-top-hover.png`
and `arc-pane-controls-hover.png`. The current Arc exposes a Split View Options
vector button beside Close; hovering the inactive header reveals Reload and
Options. No additional top-center ellipsis appeared in this inspected flow.
Echo's outer header now measures 32 px including its contour (previously 33),
and its address group centers over the complete pane instead of the unequal
left/right controls. Inactive Reload/Options reveal on header hover. Header
material and narrow active-pane address truncation still need comparison.


## Four-pane frame correction — 2026-10-24

A clean guarded session tracked newly opened Arc CGWindowNumber **3050**.
The initial window frame and the frames after opening a unique source URL,
adding panes 2, 3 and 4 through View → Add Split View, and converting four panes
to vertical were all exactly `{X:100,Y:100,Width:1000,Height:760}` according to
`CGWindowListCopyWindowInfo`. The test Space header was compared with its known
crop before input; all fresh test tabs and the owned window were closed.
Evidence: `guard-clean-minimum.log` and
`guard-session/arc-clean-four-{horizontal,vertical}.png` under
`target/parity/split-view/`.

Arc retains 220 px pane frames and 10 px gaps. Horizontal overflow extends
right; vertical overflow extends above the window, with the last pane ending
at y=750. Echo now matches those layout constraints without resizing its root.
The earlier auto-expansion claim and commit ba71a47 were based on an incorrect
interpretation of cropped captures and are superseded by this measurement.

### Empty sidebar affordances — 2026-10-24

The default Dark reference (`arc_default_sidebar_Dark.png`) shows a magnifier
and “Search or Enter URL…” in the empty address field, followed by the Space
header and New Tab without a separator. Retained native captures in
`parity/sidebar-states/` distinguish the section rule: `arc-pinned-final.png`
has Pinned items and no Today items, with no separator; `arc-real-pinned.png`
has both sections and a separator; `arc-pinned-hover.png` has Today without
Pinned, with the separator and Clear action. The separator follows Today
content, rather than the presence of Pinned items.

Echo's three default Spaces were present in the dark fixture; their dock
marks used dark ink and became nearly invisible. Both stationary marks and
the traveling active dot now use theme-dependent monochrome ink. Native
Light/Dark captures show the three marks, empty address affordance, and
pinned-only/Today transitions at 1000×760. Evidence is retained under
`target/parity/space-themes/round3/`; Arc images there are reused captures,
not fresh observations. Two guarded Arc attempts failed responsiveness or
created no window, so no shared test Space/profile content was changed.

The empty Favorites card is retained. The owner-supplied October 23 reference
above establishes its geometry and dismissal. Independently, the installed
Arc binary contains “Drag to add Favorites” and “Favorites keep your most
used sites and apps close”, labeled as the Top Apps tray empty-state title
and subtitle beside `ARC_AppTray.bundle`. The shared profile's existing
Favorites were not removed to reproduce that state. Exact theme contrast
of a fresh live Arc empty tray remains unverified.

### Max settings and user-owned AI foundation — 2026-10-24

The installed bundle reports **1.170.0** (`CFBundleShortVersionString`), rather
than the requested 1.169.0. These captures document the installed reference;
version-specific 1.169 parity is unverified. A guarded read-only session opened
one fresh Arc browser window in the shared Echo Test Space, verified its header crop, and claimed the newly opened settings
window separately before any popup input. Max is 684×650, with an 88 px toolbar,
126 px banner, 64 px horizontal inset, 84 px Enable Max/future-features group,
and 80 px feature rows with 94×62 previews and 36×16 switches. The scroll order
is Tidy Tabs, Instant Links, 5-Second Previews, Tidy Tab Titles, Tidy Downloads,
ChatGPT in the Command Bar. Ask on Page and Browse for Me were not switches in
this installed Max pane. Both top and scrolled screenshots were inspected;
no Arc toggles, profiles, themes, tabs or Spaces were changed, and both owned
windows were closed. Reference/implementation pairs are under
`target/parity/arc-max/`: `arc_max.png` / `echo_max.png` and
`arc_max_bottom.png` / `echo_max_features.png`.

Echo keeps the measured layout and feature preferences, with original banner
and preview artwork instead of Arc assets. Its required user-owned provider
controls are below the reference feature group: OpenAI, Anthropic, compatible
base URL, masked key entry, model selection, and AI-only ChatGPT connection.
The key field cannot Copy/Cut its secret and clears itself when saved. Session
JSON stores only credential references/configuration. Provider and OS storage
contracts, official OAuth sources and validation limits are documented in
`crates/ai/README.md`. Actual account consent/inference and Windows/Linux
credential stores have not been verified; the macOS isolated keychain round-trip
and offline transport/identity tests passed. Feature request hooks are separate
follow-up changes; switches alone are not complete Max feature parity.


### Tidy Tabs reference limits — 2026-10-25

In guarded Echo Test windows, Tidy appears beside Clear above six Today items;
hover text is “Organize Today Tabs”. Two local-page requests and a public-page
request (Rust Book/Cargo/serde, Pasta/Bread, Paris/Rome) produced no grouping or
undo entry, including a 29-second wait. The seven fixture tabs were individually
archived after each session, preserving the two existing tabs and all Spaces.
No existing successful Tidy capture was found in the shared parity evidence.
The owner authorized a Today-folder fallback and explicitly partial result
visuals. `target/parity/arc-max/arc_tidy_public_hover.png` and
`arc_tidy_public_result.png` record the reference; successful Arc group geometry,
header context menu and feedback remain unverified.

Echo conditionally applies strict JSON grouping only to the captured Today
items/profile, rejecting unknown/duplicate IDs, changed titles/URLs/order,
feature disablement and provider changes. Grouping retains split panes and
native page identities. New tabs and later page titles survive undo/redo; moved
or closed tabs are never resurrected. Original folder geometry is provisional.

Native Echo validation used one synthetic credential entered through Max settings
and a loopback compatible provider. Exactly one authorized streaming request
contained the nine fixture tabs; two folders/five grouped members persisted,
all nine pages remained, collapse and ⌘Z/⇧⌘Z worked, and the retained Rome page
kept its position. The generated OS credential was removed afterward. Inspected
captures: `echo_tidy_hover.png`, `echo_tidy_groups.png`, `echo_tidy_folded.png`,
`echo_tidy_undo.png` and `echo_tidy_redo.png`, beside the Arc public-page captures
in `target/parity/arc-max/`. The reference produced no successful groups, so
`arc_tidy_public_result.png` / `echo_tidy_groups.png` is behavioral evidence,
not a claim of matching group-result visuals. Fixture Favorites/sidebar widths
also differ.


### Tidy Tab Titles — 2026-10-25

Guarded Arc Pin Tab menu inspection changed a newly opened Wikipedia “Rust
(programming language)” Today tab to the pinned label “Rust Language”. Unpin
kept that label. The generated tab was unpinned and individually archived,
without changing Max preferences or other tabs. The initial physical ⌘D attempt
did not pin in the owner's Arc setup and is not shortcut evidence. Current
captures are `arc_tab_title_before.png`, `arc_tab_title_pinned.png`,
`arc_tab_title_result.png` and `arc_tab_title_unpinned.png` under
`target/parity/arc-max/`. The shared Space's other fixture tabs changed between
sessions; no visual claim relies on their count or placement.

Echo queues only Today → Pinned transitions, separates AI names from explicit
user names and live page titles, retains the label after unpinning, and restores
the latest page title with Reset Name. Requests are shared-claimed per workspace
and tab, cancelled on changed identity/provider, and discarded during inline
editing. UI regressions cover pin/unpin/reset with retained engine pages, late
manual edits and switching providers in Settings. Core tests cover persistence,
changed URLs, unpinning and disabled features. Provider context strips URL
credentials, queries and fragments; non-web URLs submit only their scheme,
never local paths or inline document bodies. Successful actual model output
quality, Split/Favorites title behavior and Arc cloud feedback are unverified.

Native Echo validation used an isolated session and synthetic key entered in
Max settings, then one authorized loopback streaming response. Pin displayed
“Rust Language”; unpin retained it; Reset Name restored the live Wikipedia
title without navigation/reloading. CEF quit cleanly and the generated keychain
entry was removed. Inspected 1000×760 pairs:
`arc_tab_title_result.png` / `echo_tab_title_result.png`, and
`arc_tab_title_unpinned.png` / `echo_tab_title_unpinned.png`; reset evidence is
`echo_tab_title_reset.png`. All are under `target/parity/arc-max/`. The Echo
fixture has fewer Favorites/other tabs and a different sidebar width, so these
verify title behavior rather than whole-window pixel parity. Actual provider
model quality remains unverified; the local fixture deliberately returns the
same label observed in Arc.

### Max completed-download renaming, 2026-10-25

Arc Max settings remained read-only, including the disabled Tidy Downloads
switch. Source for the two messages is the installed Arc 1.170.0 binary's
ToastController string group: local extraction
`target/parity/arc-max/arc-strings.txt:47511` (“Download name reverted!”) and
`:47513` (“Download renamed!”), with adjacent Tidy Downloads comments.
The plain/accessory layouts are the independently verified `d7ed3a3` reference
in `docs/TOAST_REFERENCE.md`. No exact Arc download-toast visual is claimed.

Echo queues inference only after a new canonical completion when enabled;
enabling Max does not scan existing downloads. Only basename, MIME type and
sanitized source URL reach the selected direct provider. The generated stem
cannot choose a directory or change the original extension. Retained file and
parent identities, conditional inventory edits, no-overwrite moves and local
Undo preserve completed-record ownership. Edited files/replaced paths and stale
provider/settings/record responses are rejected. Quit waits only for an already
started local file move and its inventory commit, never for provider inference.

An isolated native CEF attachment fixture made one compatible-provider SSE
request, renamed the file, and used the toast accessory to restore its original
name and all 2,800 bytes; the saved inventory retained its profile and record ID.
The generated test credential was removed and CEF completed its quit barrier.
This validates the canonical completion hook through the explicit download
acceptance fixture. It does not enable the separately gated ordinary download
routing/continuation pipeline. Windows/Linux native file moves remain unverified.

Evidence at 1000×760: `target/parity/arc-max/echo_download_renamed.png`
and `echo_download_reverted.png` include the passive child windows and were
visually inspected. The retained generic Arc templates are
`verified_toast_accessory_pair.png` and `arc_verified_plain_toast_template.png`
in the same directory; they are explicitly not a download-specific Arc pair.
`download-native-provider-observation.json`, `download-native-inventory.json`,
`download-native-file-proof.txt` and `echo-download-run-native.log` record the
request, restored record, bytes and clean shutdown. Seven added regressions
cover core name/metadata validation, filesystem collision/replacement/symlink
handling, and GPUI completion/Undo/stale-response behavior. The full workspace
suite passed (989 browser, 212 core, 13 AI, 135 engine, 6 importer; the native
credential-store integration test remains opt-in).


### ChatGPT in the Command Bar, 2026-10-25

Installed Arc 1.170.0 was inspected in a guarded disposable window in the shared
Echo Test Space, with Max settings unchanged. Typing ChatGPT shows the
“Ask ChatGPT” / Tab hint; Tab adds a 24 px-high rounded green ChatGPT capsule
and “Send a Message…” placeholder. Scope and selected row sample `(117,169,156)`
in the captured Display P3 PNG (sRGB conversion: `(100,171,156)`). A typed question becomes a single search row. Empty
scope retains ChatGPT website/search/history suggestions. Enter opens a website
tab and prefills the question in ChatGPT's composer; the user still owns Send.
It is not an in-popup API chat view. The test tab was closed and the window
cleaned up without modifying any Max preferences or other tabs.

The observed handoff is `https://chat.openai.com/?q=What%20is%20the%20Rust%20programming%20language?`.
A read-only, in-memory query of only the fixture's recent ChatGPT history URLs
confirmed that address, its chatgpt.com redirect and the final prompt route;
`arc_chatgpt_fixture_history.json` retains only those fixture URLs. The browser
uses the initial observed URL and percent-encodes query delimiters and Unicode
into the single q parameter. The site performs its own redirect/composer setup;
Echo never injects JavaScript or presses the website's Send button. In the
isolated logged-out Echo session the same route automatically generated an
answer, whereas the captured logged-in Arc session only prefilled its composer.
This authentication-dependent website behavior remains a parity limitation. This route
uses the user's website session, independently of the selected API provider.
The other Max inference features continue to use the chosen direct provider.

Reference evidence: `target/parity/arc-max/arc_chatgpt_commandbar.png`,
`arc_chatgpt_scope.png`, `arc_chatgpt_query.png`, `arc_chatgpt_submitted.png` and
`arc_chatgpt_submitted_url.txt`. These were visually inspected. Header favicon
and website history suggestions vary with each profile; no OpenAI/Arc asset
was copied into the repository.

Echo native evidence uses the matching filenames with `echo_` prefixes for
Command Bar, empty scope, question scope and destination. All frames are
1000×760 and were visually compared. The isolated profile has fewer history
rows/Favorites and a different sidebar width. The scoped fill was calibrated
through the screenshot ICC profile: Arc and Echo final Display P3 samples both
read `(117,169,156)` for green and `(67,72,208)` for the unscoped selected row.
Panel/font metrics and the input's generic search icon versus Arc's ChatGPT
favicon remain visual limitations. The native CEF run created one website tab
and exited with status zero after its complete shutdown barrier. Core URL
encoding plus three GPUI regressions cover Tab/Backspace, stale disabled scope
and keeping ChatGPT text in a rename form. The full suite passed: 995 browser,
213 core, 13 AI, 135 engine and 6 importer tests; the OS credential integration
test remains opt-in.

## Archive interval scope question (2026-10-25)

The installed Arc applies a changed archive interval to the selected profile
before presenting the scope question. Its buttons are “Only this Profile”
(Escape) and “All Profiles” (Return); Escape retains the selected profile's
change. The question is a centered 450 × 228 px card with 18 px corners,
38 px left text inset, a 28 px right/bottom button inset, 164/124 × 36 px
buttons and an 8 px gap. Echo uses its own product name in the heading.

Reference and private native acceptance evidence are under
`target/parity/sweep/`: `arc-archive-scope-question.png`,
`echo-archive-scope-question.png`, `pair-archive-scope.png` and its zoomed crop.
`native-archive-only.json` records 24/12-hour profile intervals after Escape;
`native-archive-all.json` records seven days in both profiles after Return.
The Arc test used only the unused “Echo Profile Reference 20261023” profile;
All Profiles was never confirmed in live Arc. The initial restoration alert was covered before final readback. A complete
CG inventory later confirmed those windows were gone. A fresh guarded session
observed the unused profile at 12 hours, preserved that current value, and
verified its return to 12 hours after the follow-up capture.
No live Space was created/deleted and no tabs were opened in Echo Test.

Three GPUI regressions cover pointer and keyboard scope choices, unchanged
selection, persistence, blocked background clicks and rejection of a prompt
superseded by a newer selected-profile interval. GPUI 0.2.2 retains removed
selector bounds in its debug-frame map, so dismissal assertions inspect modal
state alongside the canonical profile data, rather than stale selector bounds.


Follow-up `arc-archive-opening.mov` measures about a 133 ms fade and a 300 ms
32 px downward settle with an ease-in-out shape. `arc-archive-motion-sheet.png`
and extracted frames retain the observation. Idle/hover colors were converted
from their embedded ICC profile to sRGB: Only this Profile `#302f63` →
`#454473`; All Profiles `#3239fb` → `#2c33e2`. Follow-up captures are
`arc-archive-hover-only.png` and `arc-archive-hover-all.png`.


`arc-archive-opening-closing.mov` and `echo-archive-opening-closing.mov`
also check the 200 ms, 32 px upward exit. A closing card blocks background
input until its exit ends; generation checks prevent an older exit timer from
removing a newer dialog. Reduce Motion skips entry and exit travel. Paired
idle/Only-hover/All-hover crops and their zooms were inspected. Native samples
are retained in `archive-motion-samples.json`; the separate profile-list layout
and general settings popup behavior are outside this question's scope. The
fresh Arc session verified the unused profile's original 12-hour value after
restoration and closed its preferences and test window without opening tabs.

## Camera and microphone source state — 2026-10-25

The owned Echo Test window was inspected with a loopback-only getUserMedia
fixture. It never records or uploads media, and every stream automatically stops
after 30 seconds. Combined camera/microphone capture displayed an 8 px circle
before the favicon: a 1 px white rim and sRGB `#ff736d` fill, adding 14 px to the
label/favicons' horizontal origin. Reference screenshots and enlarged row crops
are in `target/parity/capture-indicator/`. Early single-source captures did not
show this dot, so they do not establish its full activation policy or latency.
A later session's guard rejected foreign front Arc window 21869 ahead of the
recorded test window 21328; no input was sent to the foreign window. Test-tab,
site-permission restoration and owned-window cleanup remain blocked by that
guard. Streams have auto-stopped. No Space was created or deleted.

CEF `DisplayHandler::on_media_access_change` now supplies separate live video
and audio flags. These are document metadata, not audible playback or remembered
permission decisions. State survives backgrounding, Peek promotion and live view
transfers; renderer termination resets it, and destroyed browser IDs reject late
callbacks. The sidebar row remains partial: no unverified single-source condition
or indicator animation has been invented. Five regressions cover transitions,
deduplication, late callbacks, view transfer and Peek promotion. The full run
passes 1022 browser, 213 core and 139 engine tests (plus the other workspace
suites).

Native CEF verification used virtual capture devices in a fresh private profile,
with loopback-only permission entries. Microphone-only, camera-only, combined and
each stop transition produced the exact expected flags; CEF reached Complete
and exited 0. `target/parity/capture-indicator/native-states.json` contains the
asserted sequence. The inspected pair and enlarged crop are
`target/parity/media-pip/sweep/pair-capture-state.png` and
`pair-capture-row-zoom.png`; they explicitly show the remaining indicator gap,
not visual parity. Archive scope, hover and Space-menu pairs are copied to the
same evidence directory. A final guarded retry again rejected 21869/21328 and
performed no Arc input.

### Max public-link reference, 2026-10-25 (Arc 1.170.0)

The owner authorized temporary changes only for 5-Second Previews, Instant Links
and Ask on Page, on public pages in the shared Echo Test Space. The seven Max
switches originally read: future features 1, Tidy Tabs 1, Instant Links 0,
5-Second Previews 0, Tidy Tab Titles 1, Tidy Downloads 0, ChatGPT 1. The combined
inspection changed Instant Links and Previews from 0 to 1 and restored both to 0
in its `finally` block. Preview-only sessions changed Previews 0 → 1 → 0.
`target/parity/arc-max/arc-enabled-toggles-{before,during,after}.json` and
`arc-preview-toggles-{before,after}.json` record these values. A separate live
recovery check (`arc-preview-recovery-{before,after}.json`) confirmed all seven
original values without changing any switch. Tidy Downloads remained off.
No Ask on Page switch or Ask control appeared in this installed version's Max
pane / Command-F UI. No undocumented preference was written to force it on.

On the public Wikipedia Rust article, holding Shift over Type safety opens a
300 px-wide blue translucent panel 6 px left of the link's bounding box and
8 px below it. The completed reference is 270 px high with a 12 px corner
radius, 15 px padding, 13 px system text, 19 px line spacing, and approximately
5 px between the title/fact rows. It has a bold heading and four icon-led facts
with bold labels. The Share control is a 32 px circle inset 8 px from the right
and bottom. Its menu has exactly Copy as Text, Copy as Image and Share, with
24 px rows in an approximately 174 × 84 px surface. Opening this menu retains
the preview after Shift is released; Escape first dismisses the menu and then
the retained preview. `arc_preview_complete.png` and `arc_preview_share.png`
were visually inspected. The later `arc_preview_loading.png` is a cached,
already-completed answer, so it is not evidence for the initial skeleton.
Earlier uncached inspection showed a 64 px panel with two inset skeleton bars.
The bundle description (`arc-strings.txt:51906`) distinguishes automatic
previews on certain websites from Shift-hover previews on any link; the
automatic-site list has not been established.

With Instant Links temporarily enabled, Shift-Return on the public query
"Rust programming language official documentation" closed the Command Bar,
showed a Today row titled "Searching the web for you", then opened ten public
results, selecting Rust Documentation. The query, loading and result captures
are `arc_instant_{query,loading,result}.png`. The nine remaining fixture tabs
were subsequently archived by their captured IDs/creation times; no preexisting
Today tabs were included. The returned URLs are documented in
`instant-owned-tabs-archived.json`. Browse for Me produced no dedicated action
in the inspected desktop Command Bar. These observations do not establish an
Ask or Browse for Me UI and must not be replaced with invented controls.

Echo's Shift-hover portion is implemented separately from the still-unverified
automatic-site list. The private native fixture saved a synthetic key through
Settings → Max, read the public Type safety destination without browser cookies,
and made exactly one authorized loopback SSE request. The key was removed after
native termination (strict exit status 0). `preview-native-provider-observation.json`
and `preview-native-credential-cleaned.txt` contain the non-secret proof.
Core/GPUI tests cover stale documents, disabled observers, native modifier proof,
Shift release without a key callback, both keyboard and click menu actions, and
minimization. A native Share click now preserves its panel across child-window
activation; Escape dismisses its menu and then its retained panel.

Visually inspected pairs at 1000 × 760 are `arc_preview_complete.png` /
`echo_preview_complete.png` and `arc_preview_share.png` /
`echo_preview_share.png`, all under `target/parity/arc-max`. Both captures have
Display P3 profiles. Background row medians at y=290/400/500 are respectively
(10,13,75)/(13,16,77)/(15,17,78) in Arc and
(10,13,76)/(13,15,77)/(15,17,78) in Echo. The lower highlight still differs;
`preview-color-comparison.json` records the remaining sampled differences.
Exact font wrapping, some fact glyphs, motion and blur remain partial. These
are panel comparisons: the fixture has different Favorites and the two browsers
have different cached revisions of Wikipedia. Native image-copy output,
system-share payload fidelity and non-macOS native controls are unverified.
The local bundle check required an ignored `third_party/cef/CREDITS.html` link
to the actual versioned SDK credits because the existing bundler expected that
unversioned path; the upstream notice itself was preserved.
## Precise page history swipes, sweep round 2 — 2026-10-25

The installed Arc binary exposes WebContentOverscrollDelegate,
useChromiumOverscroll and overscroll-behavior-x containment strings, retained in
`target/parity/sweep-r2/arc-overscroll-strings.txt`. No synthetic gesture was sent
to Arc. Echo's isolated native view shim decodes precise finger phases; the
shared engine checks direction, distance/flick and the starting document's
scroll policy through a read-only Runtime evaluation. Editors, frames, custom
elements, media, horizontal scrollers, CSS containment and any non-passive wheel
handlers retain input. Failed, stale or timed-out policy queries deny navigation;
native events are buffered only until that decision. Hiding, navigating or terminating the source cancels an in-flight gesture.
The passive GPUI arrow reuses the Space return spring; its exact Arc appearance
is unverified, and non-macOS native input bridges remain unfinished.

`native-swipe-assertions.json` verifies C → C for a short gesture, C → B → C for
back/forward, and unchanged horizontal/contained/blocking-handler URLs. The actual CEF view was
hit by precise scroll events; the screenshot `echo-page-swipe-arrow.png` was
inspected, including `echo-page-swipe-arrow-zoom.png`. Native exit was 0 with
CEF Complete. Four regressions cover policy decoding, gesture
threshold/direction/velocity and feedback focus, teardown and background rejection.

## Little window inactivity timer, sweep round 2 — 2026-10-25

The retained installed-Arc Links reference
`~/Documents/Bot/parity-evidence/parity/settings/arc_little_popup.png` shows
1 hour, 6 hours (default), and 12 hours. The existing Echo picker now governs
Little window expiry rather than merely persisting a value. Each lookup reads
its canonical source profile's live preference; its independent timer stops on
promotion or window destruction. Foreground root/child popup activity protects
the lookup. Monotonic and wall ages cover clock corrections and system sleep.
A 30-second poll uses the existing close/engine-retirement coordinator and does
not archive or replace the normal window's tabs. Little Library entries remain
unfinished. Exact hours-long Arc expiry timing has not been measured.

Two GPUI regressions cover all interval boundaries, foreground protection, live
interval changes, automatic timer dispatch, and normal-tab preservation.

`target/parity/sweep-r2/native-little-assertions.json` records the native check:
a disposable OneHour profile, a 7,200-second idle lookup, two roots becoming one
within a 35-second poll wait, unchanged normal URL, exit 0 and CEF Complete.
`echo-little-before-expiry.png` and `echo-little-after-expiry.png` were inspected.
`pair-little-interval-retained.png` reuses the checked Settings row captures;
the timer does not change that control. Native Settings clicks were stopped by
the overlay/pixel guard; live preference changes are covered by the GPUI test.

## Capture indicator static fallback, sweep round 2 — 2026-10-25

Fresh guarded normal 22461 reused Echo Test; its R2 fixture streams and tabs were
stopped/archived and the window closed. Window 21328 was left for batch cleanup.
The camera fixture selects the available OBS/virtual device before getUserMedia.
`target/parity/capture-indicator/arc-r2-camera.mov` and the inspected contact
sheet `target/parity/sweep-r2/arc-camera-contact.png` show a live single camera
without a dot during the sampled interval. This does not establish timing.
Under the authorized static fallback, Today/Pinned rows use the previously
captured 8 px `#ff736d` dot, 1 px white rim, and 14 px icon/title advance. Capture
of either source shows it; stopping both restores the original layout. Motion,
activation timing and Favorites/Split placement remain partial. The enlarged
pair `target/parity/sweep-r2/pair-capture-indicator-zoom.png` was inspected.
Native virtual-device transitions and clean CEF shutdown were rechecked; the
GPUI regression verifies background source states without activation and the
restored title position after stopping.



### Instant Links, installed Arc 1.170 (2026-10-25)

The owner's temporary-toggle exception was used only for public pages in the
shared Echo Test Space. Instant Links was **0 → 1 → 0**; 5-Second Previews
remained 0 and Tidy Downloads remained 0. The seven original checkbox values
were restored exactly in the same locked capture session. Evidence is under
`target/parity/arc-max/instant-existing-toggles-{before,after}.json`. All eleven
owned source/result tabs were archived; four late-loading result titles required
identity-checked cleanup. `instant-existing-cleanup-verified.json` records that
none of those remaining identities survives in Echo Test. No Spaces were deleted.

Pairs at 1000×760: `arc_instant_existing_query.png` / `echo_instant_query.png`,
`arc_instant_existing_loading.png` / `echo_instant_loading.png`,
`arc_instant_existing_result.png` / `echo_instant_result.png` in that directory.
Shift-Return closes the Command Bar. The original page stays selected and live
while a non-selected Today row says “Searching the web for you” (bundle string
38078/230095). Arc returned ten tabs at the top of Today and selected the first,
with the original source below the entire result block. Result titles may change
after navigation. Echo's transient row is never a persisted or archived tab.

Echo uses the user's exact selected model and provider; only the query is sent.
OpenAI and compatible Responses endpoints use the documented hosted web_search
tool and `web_search_call.action.sources`; Anthropic uses basic
`web_search_20250305` results. There is no guessed-URL/model-prose fallback or
Echo server. Compatible servers and models without hosted search fail normally.
Sources: https://developers.openai.com/api/docs/guides/tools-web-search and
https://platform.claude.com/docs/en/agents-and-tools/tool-use/web-search-tool .
The native fixture is a local authenticated Responses server with ten supplied
public URLs, not a paid/live provider validation. The synthetic key is saved
through Echo's Max settings and deleted from Keychain after the capture.
Arc's result ranking, proprietary AI glyph and exact loading motion remain
partial. Fixtures differ in Favorites/existing tabs and remote page revisions.

`arc_instant_existing_query.png` shows a second 46 px row for the unchanged
query, captioned “Ask ChatGPT”, with a 24 px arrow tile on the right and no
leading icon backplate. Echo now offers that row for ordinary New Tab searches
when the existing ChatGPT Max flag is enabled. It preserves keyword/Tab scope
and uses the already verified `https://chat.openai.com/?q=...` website handoff;
it makes no provider request. URLs and rename/split forms retain their existing
behavior. The observed New Tab flow is the scope of this follow-up; Command-L
ordinary-query behavior has not been established. Keyboard, click and disabling
the flag have GPUI regressions.

The private native fixture disables remote search suggestions to match the
reference's two-row state. Compare `arc_instant_existing_query.png` with
`echo_instant_query.png` under `target/parity/arc-max/`.
`arc_ask_chatgpt_selected.png` / `echo_ask_selected_colors.png` record the
keyboard-selected Ask row, with `arc_search_hover_with_ask_selected.png` /
`echo_search_hover_colors.png` covering hover while Ask stays selected. The
Instant Links loading label was raised to 14 px medium after the original
13 px label measured two pixels shorter than Arc's text in the paired captures.
Loading/result capture names remain those in the Instant Links section. The
original search glyph substitutes for proprietary artwork; font/material
fidelity and exact ranking remain partial. The authenticated loopback fixture
returns ten public sources; its body is checked to contain only the explicit
query and standard search controls, without browser context. Its synthetic
credential is saved via Max settings and deleted after clean native shutdown.

A later guarded read-only inspection confirmed the primary search selection is
blue with Instant Links off as well (`selection-max-values.json`: Instant 0,
Previews 0, Downloads 0, ChatGPT 1). Search selection converts from Display P3
to sRGB `#4248d8`; Ask selection is `#64ab9c`, including its arrow on the white
tile. Hover is a separate translucent fill while keyboard selection persists.
Only these result classes use the measured colors; page/tab result materials
retain their existing policy. The public IANA reference fixture was archived
and the later keyboard/hover session opened no tabs. No Max setting changed.
`arc_search_selection_readonly.png` / `echo_search_selected_colors.png` are
empty-Space comparisons; the `_wiki` Echo capture also pairs with the earlier
public-page query reference. Icon/font/blur differences remain partial.

Native color verification used a disconnected provider with no credentials.
After the private public Wiki tab was closed, the same 1000×760 window showed
an empty Space. The harness now verifies its post-resize position at (100,100)
before global pointer input; its initial early-position attempt had left the
window at (1320,680). `command-result-color-comparison.json` records exact
sRGB selection samples (search 66/72/216, Ask 100/171/156); hover differs by at
most two levels in that sample. A real mouse click opened ChatGPT and the
public query appeared in its logged-out website UI (`echo_chatgpt_mouse_handoff.png`).
The website auto-submits in this logged-out state, as in the prior native
handoff check; Echo only navigates to the observed handoff URL. Shutdown
completed normally. One earlier launch trapped in a CEF image on a libdispatch
background thread before automation; a fresh private-profile retry completed
normally. The exact startup cause remains unresolved, with a stripped summary
in `max-colors-startup-crash.json`.

Ask on Page still exposes no Ask control in the installed desktop Command-F
flow, including the available permitted Max flags. There is no separate Ask
toggle in this build. Browse for Me captions exist in the bundle (52626–52635)
and an `about:blank?type=ask-arc` identifier, but no desktop invocation/result
layout was found. Those flows remain reference-blocked; no invented controls.
