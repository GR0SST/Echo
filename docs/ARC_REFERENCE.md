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
