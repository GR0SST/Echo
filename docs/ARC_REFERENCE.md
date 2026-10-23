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
