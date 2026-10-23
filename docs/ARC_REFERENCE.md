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
