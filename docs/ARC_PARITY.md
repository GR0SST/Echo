# Parity with Arc

Comparison with Arc for macOS version 1.169.0 (`/Applications/Arc.app`).
The inventory baseline remains 1.169.0. On 2026-10-23 the installed app
updated to 1.170.0 (build 89123); later reference checks record that version
explicitly. This update does not automatically revalidate earlier observations.
Our code: `crates/core`, `crates/browser`, `crates/engine` as of this
document's date.

**Where the data comes from.** The initial inventory was assembled without launching Arc, from
the bundle's files: the main menu in `Base.lproj/MainMenu.nib` (items,
shortcuts, Command Bar synonyms), the AppleScript dictionary `Arc.sdef`,
interface strings and translator comments in the binary
(`Contents/MacOS/Arc`, grouped by source module: `PreferencesFeature`,
`ContextMenus`, `TypeTravel`, `CommandBar`, `SpaceStyleEditor`, `Toasts`,
`LittleBrowser`, `PictureInPicture` and others), and the `ARC_*.bundle`
resource bundles. **(?)** marks an item that was inferred indirectly, e.g.
a shortcut that isn't in the nib and is assigned in code, or a string whose
exact place in the UI is unknown.

Command Bar behavior is also cross-checked against [Arc's public action
reference](https://start.arc.net/command-bar-actions) and [release notes](https://start.arc.net/release-notes)
(repeated Command Bar shortcuts dismiss it; confirming the current URL reloads it).

Easels are left out of the comparison by the owner's decision. Notes (Arc's
documents) are mentioned once.

Echo is a **local-only browser** by the owner's decision. There is no
Echo/Arc account, browser-account sign-in, unified browser login, cross-device
sync or cloud backend of ours. Anything dependent on The Browser Company's
servers or an Arc account is permanently out of scope: Arc Members and account
recovery; syncing Spaces, tabs, Favorites or settings; arc.net cloud sharing of
Spaces/Folders/Easels/Splits; publishing Easels/Boosts to the gallery; "Share
Arc" referrals; Arc's auto-update feed and telemetry.
Do not add UI for these features, including placeholder buttons, sync notices
or synchronization icons. Existing inert controls described below are inventory
observations, not permission to retain or implement excluded UI.
Local profiles, local sessions, browsing data, website logins/passwords, local
import from other browsers and the system share sheet remain in scope.

Arc Max parity is in scope by the owner's decision (2026-10-24): Tidy Tabs,
tab/download renaming, 5-Second Previews, Ask on Page, ChatGPT in the Command
Bar, Browse for Me and Instant Links. AI requests go directly to the user's
own provider, with no Echo server in between. Settings must offer "Sign in
with ChatGPT" (OAuth to the user's OpenAI/ChatGPT account, as in the Codex CLI)
and manual API key entry for OpenAI and other providers. Keys and OAuth
credentials stay locally in the OS keychain. This AI-only ChatGPT sign-in is
the only allowed browser-provided sign-in; it creates no Echo account and
enables no sync. Website logins remain in scope as above.

Statuses: **done** (implemented), **partial** (local parity remains incomplete),
**missing** (in-scope work not implemented), **out of scope** (permanently
excluded; never build, not a parity gap). Mixed rows retain the local part's
status and identify excluded cloud parts separately.

---

## 1. Main menu and keyboard shortcuts

Shortcuts come from the nib. Menu items have an `arcKeyBindingID`, so users
can rebind them (see "Settings → Shortcuts"). Items marked "hidden" exist in
the menu but aren't shown; usually they are alternative shortcuts.

### Arc (app menu)

| Feature | Arc shortcut | Status | What's missing |
|---|---|---|---|
| About Arc | — | missing | About window with licenses (Arc's `Credits.html`) |
| Preferences… | ⌘, | partial | Separate GPUI Settings window from the app menu, Command Bar and page shortcut; General, Profiles, Links, Shortcuts, Icon and Advanced. Stored controls and remaining integrations are detailed below |
| Share Arc (referral link) | — | out of scope | Permanently excluded: Arc service/account dependency; no UI, including placeholders |
| Set as Default Browser | — | partial | Application-menu action and explicit first-run prompt use isolated macOS LaunchServices/NSWorkspace consent requests; duplicate requests serialized. Windows/Linux stubs; native OS consent and weekly prompts remain unvalidated. See docs/ONBOARDING_IMPORT_WIP.md |
| Import from Another Browser… | — | partial | GPUI import flow in the app menu, matching the installed Arc menu location. Source picker geometry and fixture import checked natively; later-screen visual parity and unsupported formats remain. See "Onboarding and import" |
| Check for Updates… | — | out of scope | Permanently excluded: Arc service/account dependency; no UI, including placeholders |
| Privacy Policy | — | missing | — |
| Hide Arc / Hide Others / Show All | ⌘H / ⌥⌘H / — | partial | Native menu and shortcuts wired through the platform interface on macOS, omitted elsewhere. Final native behavior check awaits an unlocked desktop |
| Quit | ⌘Q | done | No "Warn before quitting" confirmation. Native captured-private-page and closed-private-tab cases assert actual zero-status process exit after the complete CEF barrier with `scripts/check-private-quit.sh`; the driver strict mode rejects timeouts and signals |

### File

| Feature | Arc shortcut | Status | What's missing |
|---|---|---|---|
| New Tab… (opens the Command Bar) | ⌘T | done | — |
| New Window | ⌘N | partial | File menu, shortcut and Command Bar open normal windows sharing the profile and one live CEF page per TabId. The source Space is inherited with no selected page in any Space and the Command Bar focused. Selection is independent; transferring and closing a hosting window retain document input and navigation history. Normal state survives Dock reopen and stays separate from private windows. Verified in bundled Echo on macOS. Inactive windows retain their own frozen, dimmed source-viewport image while another window owns the same live CEF page; returning transfers the document without reloading. Capture completion preserves Command Bar and inline-editor focus. Windows/X11 hosting and remaining window states need native verification |
| Blank Window | ⌃⌘N | partial | File menu, Command Bar and shortcut open an independent Today model with focused input; no Favorites/Pinned/Space controls. Normal-profile storage/history are shared, including creation from Incognito; local restore and final-tab closure verified in bundled Echo. Profile chooser, global Archive behavior, exact visual details and relaunch persistence remain unverified |
| New Incognito Window | ⇧⌘N | partial | File menu, shortcut and Command Bar open independent private tab lists. CEF cookies and localStorage are shared only among live private windows, isolated from regular browsing and discarded after the last private window closes. Private visits and sessions are not saved; closed tabs can be restored in memory. Verified against installed Arc and bundled Echo. Private menu/action availability and remaining visual states still need parity work |
| New Little Arc Window | ⌥⌘N | partial | Standalone Little Echo window and in-app shortcut; see "Little Arc" for remaining parity gaps |
| Restore Last Closed Tab | ⇧⌘T | done | Brings the last archived tab back to its Space |
| Open Command Bar (on the current URL) | ⌘L | done | — |
| New Boost / Edit Boost / Boost Gallery | hidden | partial | Local New Boost and Edit Boost open the floating editor. Boost Gallery and cloud publishing are out of scope. See "Boosts" |
| New Profile | — | partial | Local name/Space-assignment sheets and menu/Command Bar routes work; exact visual states and Settings entry remain incomplete |
| New Note | ⌃⇧N | missing | Notes (Arc Notes) |
| New Note (in Split) | ⌃⌥N (hidden) | missing | — |
| Close Window | ⇧⌘W | done | File menu and shortcut close the owning window from page or popup input focus, retain tabs without archiving and remove child panels. Checked against Arc and in bundled Echo, including Dock restoration. The engine retains its native clip hosts until teardown so closing a live CEF page is safe |
| Archive Tab | ⌘W | done | Today tabs go to the archive; a pinned tab closes, stays in Pinned and returns to its pinned URL |
| Share… | — | missing | System share menu |
| Capture… | ⇧⌘2 | missing | Screenshot of a page area |
| Capture Full Page / Capture in Portrait Mode | — | missing | — |
| Save Page As | ⇧⌘S | missing | — |
| Print | ⌘P | partial | File → Print, native-page shortcut and Command Bar → Print Page use the focused document. macOS dialog/Cancel checked; Windows native validation and Linux print handler remain pending. See [printing contract](PRINTING.md). |

Normal-window behavior was checked in the installed Arc with disposable local
pages: New Window starts with no selected page and an open Command Bar, but
shares Today, Pinned, Favorites and Spaces. Selecting the same tab in another
window retains DOM input state; navigation updates the page in both windows.
Echo must share that live page identity, in addition to the sidebar model.

### Edit

| Feature | Arc shortcut | Status | What's missing |
|---|---|---|---|
| Undo / Redo (including sidebar actions: "Undo moving a space", "Close Peek" etc.) | ⌘Z / ⇧⌘Z | partial | GPUI fields restore text, caret and selection, including paste/cut and committed IME input. Sidebar history supports pin/unpin, tab and whole-group archives, Clear Today and relative archive batches. Pane-close Undo reconstructs the split while retaining the survivor's selection and live page; Redo archives the complete reconstructed group, matching Arc even for Pinned groups. Folder placement, pin URLs and group metadata survive repeated Undo. Native creation reasserts the intended pane's focus. Editable frames and GPUI fields own text history even when empty. Redo across committed regrouping, Pinned/Favorite unloading, other sidebar actions and remaining action-specific menu/command labels remain unfinished. Close Peek Undo/Redo retains the live renderer and history, with editable fields owning text history |
| Cut / Copy / Paste / Select All | ⌘X/⌘C/⌘V/⌘A | partial | Work in GPUI TextInput. Bundled macOS native Edit menu Copy/Paste and Select All/Cut/Paste were verified against a live CEF input with unchanged document identity. Native menu selectors target the page responder. Remaining platform and non-input editing contexts need verification |
| Copy URL (clean link without trackers) | ⇧⌘C | done | Removes known advertising query parameters, preserves signed URLs, copies to the clipboard and shows a toast |
| Copy URL as Markdown | ⌥⇧⌘C | done | Escaped page title and clean link; also available in the Command Bar |
| Copy URL as Quote (link to the selected text) | ⌃⇧⌘C | missing | — |
| Paste and Match Style | ⇧⌘V | partial | Edit menu and shared shortcut; GPUI plain-text selection/undo, exact native page/iframe and separate DevTools routing. Platform/menu-state acceptance remains; see [validation](PASTE_AND_MATCH_STYLE.md) |
| Find… / Find and Replace / Find Next / Find Previous / Jump to Selection | ⌘F / ⌥⌘F / ⌘G / ⇧⌘G / ⌘J | partial | Find in Page via CEF, live matches/count, next/previous (Enter/Shift+Enter, ⌘G/⇧⌘G), Escape to close. Find and Replace and Jump to Selection remain missing |
| Spelling, Substitutions, Transformations, Speech, Format (Bold/Italic/Underline) | ⌘: ⌘; ⌘B ⌘I ⌘U | missing | Standard macOS submenus |

### View

| Feature | Arc shortcut | Status | What's missing |
|---|---|---|---|
| Appearance: websites use Automatic / Light / Dark | — | missing | — |
| Show/Hide Sidebar | ⌘S | done | — |
| Show/Hide Toolbar (top bar mode) | ⇧⌘D (hidden) | partial | Persistent mode moves the address/navigation into a 32 px page header plus its 1 px divider and shifts sidebar Favorites up 46 px. URL editing, Copy URL, site controls, navigation and split creation are connected. Split View retains its pane headers without a second toolbar; exact theme/motion and remaining address-menu actions need parity checks |
| Expand/Collapse Pinned Section | hidden | done | Click the Space name or use the Command Bar; pins retract/expand beneath the name, with no separate Pinned label. Animation reverses on another click; state persists per Space |
| Reveal Current Tab | ⌃Space (hidden) | missing | — |
| Stop Loading | ⌘. | done | CEF stop_load; also available in the Command Bar |
| Refresh the Page / Force Refresh | ⌘R / ⇧⌘R | done | Reload and reload_ignore_cache through CEF |
| Clear Cookies and Refresh / Clear Cache and Refresh | — | missing | — |
| Add Split View | ⌃⇧= | partial | Picker, direction and four-pane limit implemented. Cancelling drafts preserves prior Undo/Redo and creates no archive entry; protected-item copies are discarded completely. Fresh native visual review remains |
| Close this Split Pane | ⌃⇧- | done | Reference menu label, Command Bar, pane header and engine shortcut. Undo restores the group while preserving the surviving page's selection and loaded view; Redo archives the entire reconstructed item as Arc does. Native Today and Pinned-folder checks passed |
| Separate Page from Split View / Expand Current Split | — | partial | Both actions are implemented. Expand Current Split is available from View and the Command Bar, preserves the group and loaded pages, and leaves other panes as 220 px strips. Expansion animation fidelity remains |
| Zoom to Actual Size / Zoom In / Zoom Out | ⌘0 / ⌘+ (⌘=) / ⌘- | done | Chromium zoom steps from 25% to 500%, reset to 100%; also in the Command Bar |
| Cast | — | missing | — |
| Developer: View Source | ⌥⌘U | done | Opens a new Today tab using Chromium's source viewer, retaining the original page or Split View. View → Developer and Command Bar actions; disabled on internal/source pages. Native-page shortcut routing and CEF rendering verified |
| Developer: Developer Tools, Inspect Elements, JavaScript Console, Network Inspector | ⌥⌘I, ⌥⌘C, ⌥⌘J, — | partial | Menus, page/frontend shortcuts (including F12) and Command Bar route to the focused live pane. The original CEF frontend is embedded in a GPUI-owned right dock through an authenticated local protocol bridge. Bundled macOS checks cover live DOM/styles, Console evaluation, Network requests, element picking, repeated toggles, document/input retention and closing the inspected Split pane after frontend focus. Other dock positions, resizing and remembered dimensions, complete host services and remaining platform/private native checks are unfinished |
| Developer: Allow JavaScript from Apple Events | — | missing | — |
| Toggle Developer Mode (per site) | ⌃D | missing | — |
| Enter/Exit Full Screen | ⌃⌘F | partial | View menu, page shortcuts and overlays target the main window; Enter/Exit commands work in GPUI tests. macOS bundle entry/exit verified with guarded input, native fullscreen state and inspected screenshots; native toolbar buttons stay under AppKit control. Other platforms and exact Arc transition presentation remain unverified |
| Exit (leave a context: Peek, full screen) | ⌘Esc | partial | Peek closes with Escape. Guarded Arc 1.170.0 leaves Peek open on Cmd+Escape, which Echo now matches; full-screen exit remains unfinished |

### Spaces

| Feature | Arc shortcut | Status | What's missing |
|---|---|---|---|
| New Space… | — | partial | Sidebar creation page from the dock + menu, Spaces menu, Command Bar, Library Spaces and right-edge pager. Drafts stay window-local; atomic creation inserts after the origin. Icon, theme and profile pickers, and exact swipe/visual parity remain incomplete |
| Edit Theme… | — | partial | Opens the Space Style Editor; see "Space themes" |
| Rename Space | — | done | Command Bar action opens a focused name editor; Escape cancels |
| Change Profile | — | partial | Captured-Space routing and storage replacement work; warning conditions and exact sheet presentation still need matching |
| Next Space / Previous Space | ⌥⌘→ / ⌥⌘← | done | — |
| Go to Space N ("Jumps over to your %@ Space") | ⌃1…⌃9 (?) | done | Default shortcuts aren't in the nib |
| Manage Spaces… (Space Manager in Library) | — | partial | Spaces menu and Command Bar open Library → Spaces through the existing navigation lifecycle, preserving loaded documents and the window-local creation draft. Full manager presentation and repeated-invocation behavior still need reference validation. See [entry contract](SPACE_MANAGER.md) |

### Tabs

| Feature | Arc shortcut | Status | What's missing |
|---|---|---|---|
| New Tab… | ⌘T | done | — |
| New ChatGPT Message… | ⌥⌘G (hidden) | missing | AI-only user-provider integration and shortcut routing |
| Join Meeting (link to the next meeting) | ⌃⌘J (hidden) | missing | — |
| Peek Email… (code from an email) | ⇧⌘E (hidden) | missing | — |
| Pin/Unpin Tab | ⌘D | done | — |
| Open Little Arc or Peek in… (pick a Space) | ⌘O | partial | Little Echo and Peek promote into the originating Space. Little’s arrow opens a searchable Space list. Same-profile promotion adopts the existing CEF browser, retaining DOM, form state and navigation history. Failed native adoption leaves the lookup open without publishing a tab. Cross-profile promotion still needs a reference-verified storage transition |
| New Folder… | ⇧⌘N per nib (?) | partial | GPUI name editor from Spaces menu and Command Bar; the nib's shortcut conflicts with Incognito, so no binding is assigned yet |
| Duplicate / Rename Current Item / Collapse All Folders / Expand All Folders | hidden | done | Tab duplication/rename and current-Space folder collapse/expand are available from menus and the Command Bar |
| Next Tab / Previous Tab | ⌥⌘↓ / ⌥⌘↑ | done | — |
| Tab Switcher forward/back ("Recent Items" overlay, MRU, like ⌘Tab) | ⌃Tab / ⌃⇧Tab | partial | Our ⌃Tab steps through tabs in sidebar order. No overlay, no most-recently-used order |
| Cycle Tab Forward/Backward | ⌃⌘→ / ⌃⌘← (hidden) | done | Same grouped sidebar order as Next/Previous Tab; intercepted inside CEF pages |
| Go to Sidebar Item 1–8 / last item (with or without Favorites) | ⌘1…⌘9 (?) | done | Favorites (optional), Pinned, Today; Split Views count as one item. Include/Exclude Favorites in Ordering is persistent and available in the Command Bar and Tabs menu |
| Switch to Split View 1–4 / next / previous | (?) | done | Command Bar actions |
| Reveal Tab in Sidebar | — | done | Command Bar and Tabs menu show the sidebar, unfold Pinned and scroll to the active item; Split View pane focus and loaded pages are preserved |
| Clear Today | ⇧⌘K | done | Archives background Today items while retaining the selected page or entire active Split View, matching the installed Arc. One Undo restores the batch and order without reloading the retained page; Redo rearchives it |
| Reset all tabs in this Space | — | done | Command Bar → Reset Space returns all Pinned tabs to their original URLs and unloads their pages; Today and other Spaces are preserved |
| Reset Tab to Original URL (pinned) | hidden | done | Command Bar → Reset Tab to Original URL navigates modified pinned tabs back to their original page |

### Archive (that's the menu's name)

| Feature | Arc shortcut | Status | What's missing |
|---|---|---|---|
| Go Back / Go Forward | ⌘[ / ⌘] (and ⌘← / ⌘→ hidden) | partial | ⌘[/⌘] and ⌘←/⌘→ work. No history menu on long-pressing Back |
| View History | ⌘Y | partial | GPUI History tab in Today with search, opening/deleting entries and confirmed clearing. HTTP(S) visits/titles persist; native restored-page search/Enter passed. Fresh visual check awaits an unlocked desktop |
| View Archive | — | partial | Command Bar → View Archive opens search, Space filters, restore/delete and clear controls. Library page remains missing |
| Clear Archive (with the "This action is permanent." confirmation) | — | done | Clear Archive… requires typing the confirmation phrase; Escape or Cancel preserves the archive |

### Extensions / Window / Help

| Feature | Arc shortcut | Status | What's missing |
|---|---|---|---|
| Add Extension… / Manage Extensions… | — | missing | No Chrome extensions |
| Minimize Window / Zoom / Bring All to Front | ⌘M | partial | Window menu and Command Bar target the main window. Bundled macOS checks verified minimization from page, Command Bar, Find and the Minimize Window command, plus restored document/input and History/DevTools responder retention. Popup teardown cancels deferred activation that would undo minimization. Native Zoom/Bring All ordering and remaining platforms still need verification |
| View Library | ⇧⌘L | partial | Dock Home, the far-left sidebar page, Window → Open Library…, ⇧⌘L (including CEF focus), and Command Bar → Open Library open the existing Library route. Reopening retains its selected section. Spaces is interactive. Media and Downloads translate the selected page into the right preview without changing its viewport width, clipping it at the window edge; origin-only native moves do not send CEF resize notifications. Entry/return use the measured fast ease-out slide (approximately 0.28/0.35 s), with Reduce Motion respected; macOS checks preserve document identity and input through Command Bar/Find dismissal, and Reload works there. The six Library rail glyphs use reference-measured vector artwork, accent-filled selected panels, optically aligned bold labels and existing theme-owned tile surfaces. Downloads now reads canonical completed inventory; remaining behavioral/visual parity is incomplete |
| View Downloads | ⇧⌘J | partial | Window → View Downloads…, Command Bar, Library rail and the shortcut open canonical completed records beside the retained page. Completed-file menu actions are connected; default download admission, active rows and remaining presentation parity are incomplete |
| View Media / View Boosts | — | partial | Library has Media and Boosts routes; View Boosts is available in the Command Bar |
| Help: Getting Started, Essential Keyboard Shortcuts, Release Notes, Contact the Team, Help Center, Restore Data, Export Arc Notes | — | missing | Relevant for us: "Essential Keyboard Shortcuts" (cheat sheet window) and "Restore Data" |
| Troubleshooting: Record Trace, Task Manager, Reveal Arc Data, Copy Arc Info | — | missing | — |

---

## 2. Sidebar

Folder storage follows Arc's [Folders guide](https://resources.arc.net/hc/en-us/articles/19228419623447-Folders-Stash-Similar-Tabs-Together):
folder pages are Pinned and do not auto-archive.
Folder duplication follows the right-click action documented in the
[April 13, 2023 release notes](https://start.arc.net/release-notes).

| Feature | Shortcut/gesture | Status | What's missing |
|---|---|---|---|
| Vertical sidebar with tabs | — | partial | CEF site favicons are cached for unloaded/restored tabs. Unknown or failed icons use a neutral 14 px page tile (3 px radius), without title initials or Space accents. Tab labels, address text and New Tab use 13 px type with 8 px sidebar gutters. Fresh window-relative Arc captures confirm the 46 px address top and 90 px Favorites top; the earlier audit included a 5 px window-origin offset. Plain Space headings start at x=16 without an invented dot/icon slot or hover fill; custom icons retain their disclosure controls. The 50 px bottom dock puts Space marks 25 px above the window bottom. Selected rows use translucent light fills in Light/Dark themes, without a card shadow. Remaining differences include selected-row fill height (Arc 36 px, Echo 38 px), Favorites tile height (Arc 47 px, Echo 46 px), and other interaction states. |
| Resize sidebar | Drag edge; double-click to reset | partial | Window-local live width, 164–500 bounds, 228 reset and all-left collapse; native docked/floating checks preserve the CEF document and input across width changes. Exact hit area, snapping/haptics, persistence, new-window inheritance and remaining platform parity are unverified |
| Window buttons (traffic lights) inside the sidebar | — | done | Native buttons aligned with the toolbar row; idle as neutral dots with tinted rims and show their colors only while hovered |
| Reopen after closing the last window | Dock | done | Restores the saved session in a new window; keeps CEF initialized across window lifetimes |
| Address field at the top of the sidebar (click opens the Command Bar with the URL) | — | done | Shows the magnifier and “Search or Enter URL…” when no tab is selected; otherwise only the site host. Back/forward/reload in the toolbar row; Copy URL and site controls buttons appear on sidebar hover; the site controls button opens the Site Control Center panel (see section 16) |
| Hiding the sidebar and sliding it in when hovering the left edge | ⌘S | done | Docked and floating panels move native window buttons on the same horizontal track. Reversible docked slide, 100 logical px pointer buffer past the floating panel, without extending its input surface. Reduce Motion skips travel |
| "Show as Toolbar" mode (context menu: Show as Toolbar / Hide Toolbar) | ⇧⌘D | partial | The address context menu and shortcut persistently switch between sidebar address and the 32 px top toolbar plus its 1 px divider. CEF viewport begins below the toolbar; the URL opens the existing editor. Share/Capture/Shared Quotes and theme/motion polish remain |
| **Favorites** (Top Apps): icon grid above Spaces, shared by all Spaces of a profile | — | partial | Shared persistent dock (up to 12) with cached site favicons, click/keyboard activation and title/URL tooltips. Close unloads and resets the original URL; Favorite/Unfavorite and moves preserve the live page. Drag items into a nonempty dock, insert at an icon, reorder existing icons or move Favorites into folders/Pinned/Today. Split Views stay atomic, current page/pane stays focused during sorting, and saved order drives numbered shortcuts. The empty dock shows Arc’s dismissible onboarding card and accepts the first drop without reloading; dismissal persists per profile and removes its gap. Idle/dismissed geometry is checked against supplied Arc screenshots and bundled Echo; installed ARC_AppTray empty-state strings independently confirm the card. Drag visuals after dismissal, theme contrast, unread badges, calendar captions and profile isolation remain unfinished |
| Hover previews for Favorites, Pinned and folders (Gmail, Google/Outlook Calendar, Linear, Notion, Figma, Confluence) | hover | missing | Configured in "Previews Settings…" |
| **Pinned** section of a Space | ⌘D | partial | Closing keeps the tab pinned and resets it to its pinned URL. Missing: "Edit Pinned Page" (custom URL). Reset to Original URL and Replace Pinned URL with Current are available in the Command Bar; section collapses from the Space name without a separate Pinned header |
| **Today** section (unpinned) and the "+ New Tab" button | — | done | — |
| "Clear" button above Today (archives unpinned tabs) | ⇧⌘K | done | "↓ Clear" appears at the right of the Today divider on sidebar hover; the divider and its height disappear when Today is empty, including pinned-only Spaces. Preserves Pinned and other Spaces |
| "Tidy Tabs": AI grouping of Today by topic | — | partial | Direct user-provider grouping from the Tidy control above six Today items; collapsible persistent Today folders, ungrouped tabs, whole splits, undo/redo and stale-response isolation. Arc returned no result with local and public topic fixtures; Today folder visuals use the owner-authorized fallback and remain unverified against successful Arc grouping. Header context menu/feedback fidelity remains |
| Folders: nested, collapse/expand, turn a folder into a Space, folder from selected tabs | ⇧⌘N (?) | partial | Persistent ordered nested Pinned tree with GPUI creation, rename, icons, collapse/expand and folder/tab context menus. Move pages, complete Split Views or folder subtrees across Spaces without recreating pages; tabs and complete folder subtrees can also be dragged onto folder headers or Space icons. Hover a Space icon during a drag to reveal its folders. Invalid self/descendant or over-depth folder drops are rejected. Closed folders stay closed and reveal the active item. Reveal Tab unfolds ancestors. Turn into Space promotes the children into a new Space with the folder's name/icon, preserving page and group identities. Native Arc comparison confirms 16 px nesting, persistent open/closed folder icons on hover and no inline plus/ellipsis controls. Folder sibling insertion, previews and multi-selection remain missing |
| Live Folders (GitHub Pull Requests: Created by Me, Review Requests…) | — | missing | — |
| Dragging tabs (between sections, into folders, into Favorites, into another Space) | drag | partial | Drag Today, Pinned or Favorite items onto folders, Pinned/Space headers, Today headers / New Tab, or the Favorites dock, including its empty onboarding card. Drop on a row's upper/lower half to insert before/after it and adopt its section/folder; an insertion line shows the edge. Space icon drops preserve Today/Pinned (Favorites become Pinned); hovering 500ms switches Spaces so the drag can continue to a folder or row. Leaving early cancels the hover. Complete Split Views stay atomic and keep their source pane even when the destination Space had another pane selected; loaded CEF views are reused. Closed folders stay closed. The Space name also accepts nested folders at the Pinned root. Favorites can be reordered or inserted at an icon; a full dock allows sorting but rejects new items. Drops outside targets cancel without activating an inactive tab. The empty Favorites card accepts the first drop; its dismissal persists. Exact drag-after-dismiss reference states and edge auto-scroll remain unfinished |
| Haptic feedback when reordering tabs | — | missing | Option in Advanced |
| Multi-selecting tabs (⌘/⇧-click) | — | partial | Window-local command-toggle and shift-range selection of visible Today/Pinned rows; page activation is preserved. Favorites/folders and remaining dismissal states need comparison |
| Renaming a tab by double-click ("Double-click to rename") | double-click | partial | Today/Pinned single-page tabs use an inline sidebar field from double-click, Rename… and the Command Bar action. Enter or leaving the field saves; Escape cancels; an empty name restores the latest page title. Loaded pages and inactive-tab selection survive. Favorites and Split View group editor placement still differ; custom icons use a graphical popover |
| Custom tab icon ("Customize Tab") | — | partial | Graphical emoji/symbol popover for tabs and Split View groups: emoji search, category scrolling, eight-column cells, the 56-symbol catalog, removal, pointer states and keyboard navigation. Named portable symbols and emoji persist through pinning, Favorites, moves, duplication, archive and sessions. Uploaded images, popup positioning outside the browser window and exact native popup animation remain missing |
| Close button on hover (× for Today, − for Pinned) | hover | done | — |
| Audio indicator, Mute/Unmute in the context menu | — | partial | CEF PCM callbacks report real per-page audio activity, including background pages. Playing/muted controls sit before the tab title and toggle without activating the tab; silent unmuted context-menu actions are disabled. Timestamped replay and output-route recovery are covered by the section 17 A/V check; the background-audio strip adds per-tab gain and source-targeted transport without activating the tab. Rejected Document PiP requests preserve the selected audio tab. Exact Favorites placement, tooltip and theme fidelity remain unverified |
| Camera/microphone indicator on a tab | — | missing | — |
| Split View item in the sidebar (tab group) | — | partial | One item for 2–4 pages; shared title, Favorites, pinning, archive and last focused pane. Unnamed groups have equal pane segments and independent hover close targets. Favorite-group treatment and remaining theme/hover fidelity need comparison |
| Space header (name), Show/Hide Space Header | — | done | Visibility persists per Space; Command Bar and Spaces menu. The name and Space icon form the collapse/expand control for Pinned tabs and folders; hover replaces the icon with a vector chevron and reveals Space actions at the right. Collapsed state keeps the rightward chevron visible; the actions button does not toggle pins |
| Sidebar interaction states and Space dock | — | partial | Space dock uses measured 8 px active and 4 px inactive dots inside unchanged 22 px hit targets (inactive was 7 px). Close/reset hover targets match Arc’s 24 × 24 px square, centered vertically with 5 px corners; Today close glyph uses a 22 px vector canvas; context menu corners are 12 px. Native comparison at `target/parity/sidebar-states/` confirms 208 px Today and 200 px Pinned menus, a darker translucent hue-tinted native menu material with opaque fallback when installation is unavailable, and matching close target size. Folder nesting advances 16 px, and hover keeps the open/closed icon without invented inline actions. Ordinary rows compress 1 px at each side while pressed and restore on release, retaining nested indentation. Drag states, playback indicator and reorder animation remain under comparison; no theme palette/material changes |
| Browser chrome iconography | — | partial | 1108 MIT-licensed non-brand Ionicons from the installed Arc reference are embedded as portable contours. Existing navigation, folder, search, sound, history and action controls use this catalog; reference-specific sidebar/split/key controls use fixed vector geometry. Font look-alikes and default folder/mute emoji are removed. Sidebar navigation and toggle controls are visible beside native window buttons. User-selected emoji and website favicons remain content. Graphical Space/tab symbol pickers and the compact folder catalog now use these contours |
| Spaces dock at the bottom (Space icons) | click | done | Custom emoji and named vector symbols persist per Space and render in the dock. Default marks are monochrome: 8 px for the active Space, 4 px for inactive Spaces. Active and inactive fills follow Light/Dark theme ink rather than disappearing on dark chrome |
| "+" button in the dock: New Tab, New Space, New Folder, New Split (right/bottom), New Note, New Boost | — | partial | Dock + opens a translucent menu: New Space, New Folder, New Boost, New Split (⌃⇧=), New Tab (⌘T), keyboard navigable. New Boost opens the floating page editor; New Easel is left out |
| "Springy" click animation on sidebar items | — | partial | Ordinary tab rows ease 1 px inward per edge over 60 ms, then expand from the current inset on release; nested indentation and drag-card grab points are retained. Native Arc/Echo recordings confirm 212 → 210 → 212 px at the same sidebar width. Other sidebar item types and reorder settling remain unfinished |
| Incognito window sidebar ("Incognito Tabs") | — | partial | Dark sidebar with the installed reference's "Incognito" header, empty tab list and focused Command Bar on opening; no regular Favorites, Pinned items or Space switcher. Tab selection and closure work. Private New-menu/context-menu behavior and complete visual-state matching remain |
| Banners at the bottom of the sidebar: update, "We cleaned up!" after auto-archiving, speed-up (several ad blockers) | — | partial | Auto-archive toast is wired; the education banner still needs a live Arc layout reference. No nib/layout was found for the SwiftUI education banner; existing education eligibility is false and was not modified. Round 3 leaves the local education banner partial without a reference. Local blocker banners remain missing; Arc update-feed and account banners are out of scope |
| Media controls in the sidebar | — | missing | See "Media and PiP" |

## 3. Spaces

| Feature | Shortcut/gesture | Status | What's missing |
|---|---|---|---|
| Several Spaces, each with its own Pinned/Today and active tab | — | done | — |
| Two-finger swipe on the sidebar: Spaces follow the fingers and rubber-band at the edges | gesture | partial | Native scroll gain and elapsed-time spring follow recorded Arc samples; paused short drags return faster, reversing flicks take precedence, and begin/end deltas are retained. One active default dock dot follows pager position. Paused drags must pass halfway; the recorded 19.5px flick commits. Exact flick cutoff, edge resistance, custom-icon transitions and virtual-page motion still need validation. See [motion measurements](SPACE_SWIPE.md) |
| Switching Spaces from the keyboard (animated) | ⌥⌘→/← | done | Ctrl+digits and dock clicks use the same directional pager. Recorded Arc keyboard spring: 24.7/s, approximately 307ms to within 1px, no measurable overshoot. Elapsed-time evaluation preserves timing under delayed frames |
| Creating a Space ("Create a Space" dialog: name, icon, theme, profile) | — | partial | Named Spaces can be created from the dock or Command Bar with an automatic accent; Turn into Space converts a folder and inherits its name/icon. Space icons can be edited afterward. Theme/profile controls in the creation dialog remain missing |
| Renaming a Space | — | done | Name editor via the Command Bar, Spaces menu or header/dock context menu; persisted with the same Space identity. Context actions can target an inactive Space without changing the current page |
| Space icon (emoji or symbol, "Change Space Icon…", "Remove icon") | — | partial | Graphical 272 × 272 px popover anchored below the header icon, opened from the Spaces menu, Command Bar action or clicked Space context menu. Emoji search and category scrolling, eight-column cells, a separate 56-symbol Icon tab, immediate selection and Remove icon. Pointer selection and removal keep the picker open; Escape and outside clicks dismiss. Popup positioning outside the browser window and exact native popup animation remain unfinished |
| Deleting a Space (with Undo) | — | partial | Captured confirmation and identity-scoped archive/Undo transaction; unrelated Spaces and later navigation survive Undo/Redo. Last-Space replacement and destructive native execution remain unverified; no live Space was deleted |
| Reordering Spaces ("Move Space") | drag | done | Drag dock icons onto another Space to move them to that position. Stable Space/page/group identities and active pane are preserved; the pager stays on the selected Space without animating through unrelated Spaces. Dropping outside the dock cancels; order persists in the session |
| Turning a Space into a folder ("Export “%@” into Folder") | — | missing | — |
| Live Folders for a Space | — | missing | — |
| Space Manager (Library → Spaces) | — | partial | Interactive Space cards reuse tab/folder/split controls, drag ordering, captured profile switching and inline rename; creation returns to the sidebar draft. Theme, overflow and full visual parity remain incomplete |
| Assigning a profile to a Space ("Set Profile", "Edit Space profile") | — | partial | Set Profile opens the captured Space's sheet; creation-page profile selection and remaining visual details are missing |

## 4. Space themes and colors (Space Style Editor)

| Feature | Status | What's missing |
|---|---|---|
| The Space color tints the sidebar and window; during a swipe the color blends smoothly | partial | Shared 1–3-color background, procedural texture and intensity blend continuously at the pager position. Native bounded backdrop for chosen pigments, with aligned passive region previews. The unpicked Light/Dark field and empty-panel tint are measured from desktop captures (controlled red/green windows behind it leave the field unchanged); legacy single hues no longer tint unpicked chrome. Original fine grain preserves the shared viewport origin. Exact Arc shader, inactive-window response and other-palette calibration remain |
| Theme editor ("Edit Theme Color…", "Tap to pick a color for this space") | partial | GPUI child-window popover from Space context and Spaces menus, 4.25 px dot-grid color field, circular color handles, selected appearance icon states, palettes and texture/intensity controls. Color handles keep the chosen pigment in each appearance. The wave flattens at zero; its white thumb appears only after a color is picked. Exact picker mechanics, anchor behavior and visual parity remain |
| Palette with several colors ("Add color" / "Remove color"), gradient of 2–3 colors (?) | partial | Persistent 1–3 RGB colors, Add/Remove, selected handle and gradient preview; exact Arc spatial blending remains |
| Preset color sets in pages: Brand, Drab, Greyscale, Pastel | partial | Four nine-color pages measured from installed Arc catalogs; mixed sRGB, linear sRGB and Display P3 components convert to stored sRGB (out-of-gamut values clip). Reference swatches agree within two 8-bit channel values; exact page transitions and wide-gamut output remain |
| Noise texture ("Cycle noise texture") | partial | Original deterministic Grain, Sand, Tweed and Denim textures, cycle action and independent strength. The five-step cycle includes an untextured step. Textures use the measured 100 px Metal-material period, black-biased mean/contrast and texture-specific correlations; the dial starts at noon and spans a full turn. The procedural patterns are original; exact reference pixels remain |
| "Intensity" slider | partial | Persistent independent intensity, live preview and wavy slider; precise Arc response curve remains |
| Mode: Automatic / Light / Dark Appearance | partial | Arc existing-Space matrix verified: A=Light, B=Dark makes A Dark; changing A back to Light makes B Light. A second fresh window retains Dark after the first closes. Echo stores one shared browser preference; colors/intensity/texture remain per Space. Native backdrop, editor, empty panel and retained pages follow the shared choice; Automatic clears website CSS override and follows system appearance. Restart lifetime and exact shader/material calibration remain unverified |
| Undo "Update Theme" | partial | Dismissal commits one color/intensity/texture edit; browser appearance is independent of Space theme undo. Edit menu labels update and unrelated tabs/Space names survive. Arc grouping timing during a long-lived editor needs further comparison |
| Websites can read the theme ("Allow websites to get your theme data", CSS variables `--arc-palette-*`) | missing | Option in Advanced |

## 5. Command Bar (⌘T / ⌘L)

| Feature | Shortcut | Status | What's missing |
|---|---|---|---|
| Popup over the page | ⌘T | partial | 764 px centered panel, 46 px rows and reference spacing. Native within-window material samples the live CEF page below the separate GPUI controls, with measured dark tint, pigment-tinted translucent selection, double edge and an exterior-only shadow. The dark 128 px stripe reference matches the blur profile within 1–2 RGB levels after sRGB conversion; Light mode also changes the panel and text, with a small remaining tint difference. Dismissal and resize retain the document; platform fallbacks and unpicked/other-palette calibration remain unverified |
| Entering a URL or searching ("Search or Enter URL…") | — | done | Uses the selected local profile’s search engine from Settings → Profiles |
| Editing the current URL | ⌘L | done | Enter on an unchanged URL reloads the page and preserves its title |
| Navigation with ↑/↓, ⌃N/⌃P, Enter, Esc; clicking outside closes it | — | done | Repeating ⌘T or ⌘L dismisses its mode; switching between them replaces the input in the same overlay |
| Switching to an open tab | — | partial | Lists matching tabs across all Spaces, identifies the destination Space and switches there. Split View results are deduplicated by group, match pane titles/URLs and focus the matching page |
| Search engine suggestions (Chromium: Search Suggestion, Navigate Suggest) | — | partial | Debounced, bounded, cookie-free completions from Google, DuckDuckGo and Bing; the other three Settings engines currently route search without completions; requests and late replies are guarded by input identity, generation, profile and preference. Disabled in private windows and for addresses. The information disclosure uses Arc's observed wording; its Search Settings route/privacy-policy link and complete Navigate Suggest ranking remain unfinished |
| History and bookmarks in suggestions (Chromium: History / Bookmark / Search History) | — | partial | Local browsing history matches title/URL in ⌘T and ⌘L, with keyboard/click selection; open tabs match original page titles and suppress duplicate history rows. Bookmark/folder sources remain missing |
| Calculator (Search Calculator Answer) | — | partial | Bounded local arithmetic, precedence/parentheses/percentages and copy-on-confirm. Answer-row presentation and confirmation behavior still require verification against an Arc profile that displays calculator answers |
| Popular domains (Popular Domains, ARC_TopDomains) | — | partial | Local common-domain catalog and selected-suffix inline completion; typing replaces the suffix, Backspace removes it, Right accepts it. Full catalog and reference ranking remain incomplete |
| Site Search: Tab or Space after a site's name searches that site; has a management menu | Tab / Space | partial | Observed Wikipedia Tab/Space flow, orange scope pill/highlight, scoped query and empty-field Backspace to leave the scope. Other providers, scope hint and management menu remain missing |
| Removing a suggestion ("Delete Suggestion") | — | partial | × removes a local history suggestion and its saved visit without opening it; the query and keyboard selection remain usable. Removing remote engine suggestions remains unimplemented |
| **Actions (Type Travel)**: every menu item is available by name and synonyms (`arcTypeTravelTriggers` in the nib) | — | partial | Implemented actions with names and synonyms: sidebar, pin/unpin, close/archive, clear Today, restore, reload/force reload, stop, zoom, Find in Page, back/forward, next/previous tab, window controls, Space/tab management and archive. Remaining menu actions follow their features |
| Space actions: "Focus on <space>", "Switch to …" | — | done | Matches Space names and synonyms; Enter or click switches with the pager animation |
| Moving a tab: "Move to Pinned in <space>", "Move to Today in <space>", "Move to Folder" | — | done | Search destinations by Space and full folder path; direct Pin to <folder> actions identify the Space when names overlap, and Pin to New Folder opens a name editor around the current item. Move to Folder creates a folder if none exist. Identity, custom name, selected Split View pane and loaded pages are preserved; destination is focused. Follows Arc's [organization actions](https://start.arc.net/command-bar-actions) |
| Favorites: "Favorite Tab", "Unfavorite Tab", "Pin to Top Apps" | — | done | Command Bar and Tabs menu; Unfavorite returns the tab to Today in the current Space |
| Split: Add Top/Bottom/Left/Right Split, Convert to Vertical/Horizontal | — | done | Command Bar actions and pane context menus; existing pages or URLs can fill up to four panes |
| Folders: Create New Folder, Collapse/Expand All Folders | — | done | Name editor, current-Space collapse/expand, named Expand Folder / Rename Folder, Pin to <folder> and Pin to New Folder actions |
| Tabs: Duplicate Current Tab, Rename Current Tab…, Reset Tab, Replace Pin with Current Page, Mute/Unmute All Tabs | — | partial | Rename Current Tab, Reset Tab Name and Mute/Unmute All are implemented; Reset Tab and Replace Pin with Current Page work for modified pins. Duplicate works for ordinary tabs, Pinned, Favorites and entire Split Views, creating independent page identities |
| Other: Toggle Sidebar, Manage Passwords, Search Settings, Air Traffic Control, Edit Keyboard Shortcuts, Check for Updates, Undo/Redo <action> | — | partial | Toggle Sidebar and Air Traffic Control are available; remaining local actions need their features. Check for Updates via Arc's feed is out of scope |
| Running an extension by name ("Extension …") | — | missing | — |
| Join next video call / Zoom | — | missing | — |
| Suggestions for an empty field (Air Traffic Control, Tips for Organizing, Help Center) | — | partial | Five local tab suggestions with contextual Space/Switch to Tab hints. Feature/help links, Max/ChatGPT suggestions and Arc's complete recency/ranking policy remain missing. Arc-account promotion rows remain excluded |
| ChatGPT in the Command Bar, "Browse for Me" (Arc Max) | — | missing | AI-only user-provider integration and Command Bar flow |
| Instant Links (Arc Max) | — | missing | Direct navigation using the user's AI provider |

## 6. Context menus

Strings come from the `ContextMenus/SidebarMenu+*` module. Short item names
(under 15 bytes: Copy Link, Rename…, Duplicate etc.) are stored separately
in code, so some were reconstructed from translator comments (?).

| Menu / item | Status | What's missing |
|---|---|---|
| **Tab**: Copy Link, Copy link as Markdown, Share, Duplicate, Rename (?), Edit Pinned Page → Replace Pinned URL with Current, Add/Open in Split View, Move to… (Spaces, profile), New Folder with…, Mute/Unmute, Archive Tab / Close Tab / Remove Pin / Remove Favorite, Archive Other Tabs / Tabs Below / Tabs Above, Customize Page… (Boost) | partial | Captured Arc 2026-10-24: Today ends with Archive Tab and always-visible Archive Tabs Below (disabled when empty); Pinned ends with Remove Pin; Favorites omit Customize Page and use Remove Favorite. Inactive items offer Open in Split View; the active item offers Add Split View. Edit Pinned Page offers Replace Pinned URL with Current and Edit…. Removed invented top-level Pin/Unpin/Add to Favorites and relative archive entries. Remove Pin archives the item; the sidebar minus button still resets/unloads it. The Edit Pinned Page submenu is 232 × 58 px with a 5 px parent overlap; root placement follows the measured (+1, −6) px click offset. Menu frames no longer add layout padding, and native backdrop installation schedules a deferred fill refresh without requiring pointer movement. Share, unread badges, playback-dependent Mute enablement, Move to submenu, custom pinned-URL editing and complete native appearance remain unfinished |
| **Multi-selection**: Archive Items, Copy links (as Markdown), New Folder with N items, Move to… | partial | Window-local ⌘/⇧ selection without navigation; live Arc Today menu wording/order, ordered copying and existing Markdown command, batch archive/Undo, folder creation, duplication, mute, same-profile Space moves and plain-tab Split View creation. Cloud Share Items omitted. Favorites/folder selection, cross-profile moves, inline folder creation and Option alternates remain. [Reference](CONTEXT_MENUS_REFERENCE.md) |
| **Folder**: Share, Duplicate, Archive Folder / Delete, New Nested Folder…, Turn into Space, Copy All Links (as Markdown) | partial | Top-level menu now follows captured Arc sections: Share Folder…, New Nested Folder…, Turn "name" into Space, Change Icon…, Rename…, Duplicate, Move to, Delete for an empty folder / Archive Folder otherwise. Removed extra collapse/expand and copy-link rows. Rename, the 272 × 144 px folder icon picker with 24 symbols, nested creation, duplicate, conversion and empty deletion work. Move to still opens the existing destination picker; its native submenu, Share and nonempty-folder archiving remain unfinished Share Folder via arc.net is out of scope; no placeholder UI. |
| **Live Folder**: Pull Requests, Repositories, Single Sign-On, Delete Live Folder | missing | — |
| **Split View in the sidebar**: Share Split View…, Separate All Tabs, Add Split View, Convert to Vertical/Horizontal, Tab Options → Reset Name and Icon, Duplicate, Archive…, Separate from Split View | partial | Reference layout: Share Split View…, Separate All Tabs, Add Tab…, Convert orientation, a "Tab Options" header with one submenu per pane (Separate from Split View, Share, Mute, Copy Link (as Markdown), Archive Item), Change Icon…, Rename…, Duplicate, Move to, Archive Item and Archive Tabs Below. Cloud Share Split View via arc.net is out of scope; no placeholder UI. Local pane Share uses the system share sheet and remains incomplete |
| **Split View pane** (control menu): Move, Add Split, Separate Tab from Split, Convert orientation, Share Split View… | partial | Fresh Arc inspection: axis-specific Move backward/forward (unavailable edge stays visible), Add forward/backward Split, Separate Tab from Split, Convert orientation, Share Split View…. GPUI controls target the clicked pane and disable adding at four; fixed vector menu diagrams. Share and exact material/placement fidelity remain |
| **Space**: Change Space Icon…, Rename, Edit Theme Color…, Set Profile, New Folder, Live Folders, Share, Show/Hide Space Header, Manage Spaces…, Delete Space, New Profile, Export into Folder | partial | Captured Space targets from headers/dock/background; existing icon, rename, theme and folder flows retained. Set Profile lists sorted profiles with the current checkmark, opens the captured-Space confirmation directly, and offers New Profile…. Manage Spaces opens Library. Delete has the captured cancellable confirmation and atomic archive/Undo. Export lists Spaces with the source disabled, moves the hierarchy into a destination folder and supports Undo, including profile storage replacement. Destructive behavior is model/GPUI-tested, inferred from bundle reducers; live deletion/export is prohibited. Last-Space replacement, exact profile-sheet presentation and Live Folder/GitHub integration remain incomplete. Header visibility stays in Command Bar/Spaces menu. Cloud Share Space omitted. [Reference](CONTEXT_MENUS_REFERENCE.md) |
| **Favorite with a calendar** (Live Calendar): Show Hover Preview, Show Time to Next Meeting, Show Next Meeting Link, reminder 2/5/10 minutes before | missing | — |
| **Download**: Open, Copy, Show in Finder, Cancel Download, Hide from Arc, Move to Trash | partial | Completed rows provide type-specific Open/Copy, Show in Finder, Hide and Move to Trash through the shared menu overlay. Effects revalidate the captured presenter generation and canonical record; Hide removes metadata only and preserves identity watermarks. Active-transfer rows and Cancel Download remain unconnected. Direct row activation stays gated pending a usable Arc single-click reference |
| **Tab group header**: Remove header | missing | — |
| **Page** (Chromium's menu plus Arc's items) (?) | missing | The page context menu isn't set up through CEF (?) |

## 7. Archive and auto-archiving

| Feature | Status | What's missing |
|---|---|---|
| Auto-archiving idle Today tabs, 12 hours by default | done | One once-a-minute timer per profile in `profile.rs`; `core::archive_idle_except` protects selected tabs and complete split groups from every bound window |
| Auto Archive interval choices | partial | Settings → Profiles has the four observed choices: 12 hours (default), 24 hours, 7 days, 30 days. Selection updates the selected profile’s real archive policy and persists. Unsupported legacy durations remain stored until an explicit choice. The invented Command Bar picker and Archive menu entry were removed |
| Per-profile setting and the "apply to all profiles / only this one" question | missing | — |
| Separate "Archive Little Arcs after:" interval | missing | — |
| ⌘W and × send the tab to the archive | done | — |
| Archive page in Library: "Today/Yesterday", filters by Space, "How was the tab closed?" (Auto-Closed/Manually Closed), Little Arc | partial | Command Bar archive browser searches titles/URLs (including Split View panes), combines Space and reason filters, groups by local calendar day (Today/Yesterday/date), and loads older entries with Show More. Keyboard navigation and a bounded scrolling list are implemented. Dedicated Library layout and Little Arc remain missing |
| Restore ("Tab restored to …. Click to view."), Delete, Clear Archive | done | Restore a selected entry and activate its original Space with a toast; delete an entry with ×; clear all with confirmation |
| "Auto archived %d tabs" toast and the "We cleaned up!" banner | partial | Profile-driven toast; saved entries do not replay on startup/new windows. Education banner remains missing; its SwiftUI resource bundle has no nib/layout and existing education eligibility is false, so no count/action geometry is guessed |

## 8. Split View

The persistent model supports 2–4 independently focused pages, grouped pinning,
Favorites, moving, renaming, archive/restore and auto-archiving. Pane order,
axis, focus and weights survive sessions. CEF renders the pages together and
keeps loaded views through focus and orientation changes.

The installed Arc 1.169.0 was inspected on 2026-10-23 in a separate Blank
Window. Add Split View immediately creates an empty pane with the ordinary
window-wide Command Bar; Escape cancels it. Cmd+W closes only
the focused pane. The sidebar displays one segmented row, with one target per
pane, and the page headers have centered addresses without a shared title bar.
These flows now have GPUI regression coverage and real-app CEF checks. See
`docs/ARC_REFERENCE.md` for observed states and the visual parity rule.

| Feature | Shortcut | Status | What's missing |
|---|---|---|---|
| Add a pane (up to 4 panes, "Cannot Add New Pane" toast) | ⌃⇧= | partial | Immediately shows an empty pane and the ordinary window-wide 764 px Command Bar; narrow panes no longer clip its input or results. Escape, outside dismissal and Cmd+W cancel without archiving the source; drafts are excluded from saved sessions. Existing tabs, history and new URLs are supported. GPUI checks cover both orientations and a fourth pane. Exact placement during resize and full theme/animation fidelity remain |
| Close a pane | ⌘W / ⌃⇧- | done | Closes only the focused pane from GPUI and native page focus; the other pages stay loaded. Archive Item in the sidebar context menu still closes the entire item |
| Split pane corner contour and focus | — | partial | Shared GPUI/native geometry reserves a stable 1 px focus contour around every pane; focusing never resizes the document. The header/page seam is square and leaving split restores page corners without reloading. Fresh Arc measurement: 10 px inter-pane gap and 32 px outer headers including the contour. Addresses are centered over the full pane; inactive reload/options controls reveal on header hover. Exact corner radius, theme-dependent outline color and header material remain |
| Split sidebar item | — | partial | Equal-width pane segments fill a 36 px joined row with 28 px inner targets. Selected segments share one treatment; hover reveals a close target for each pane without resizing cells. Closing an inactive pane preserves the selected survivor and live pages; whole-group archive remains in the context menu. Inactive groups retain hairline separators. Favicons and custom group names are retained. Exact Favorite-group treatment, light-theme colors and remaining visual states need comparison |
| Split from a Pinned tab or Favorite | — | done | Creates independent copies in Today and retains the original pin/Favorite, including its URL, placement and identity. Selecting an existing protected tab for a pane also copies it |
| Resize Split View panes | drag | partial | GPUI divider hit targets in both axes; measured Arc thumb is 2×24 px for side-by-side panes and 64×2 px for stacked panes. Adjacent panes resize without replacing pages or changing selection; unrelated weights remain intact. A clean CoreGraphics measurement confirms Arc keeps its 1000×760 frame with four panes. Echo now retains the window frame and lays out 220 px minimum panes with 10 px gaps: overflow goes right horizontally and above vertically, keeping the final stacked pane bottom-aligned. Six GPUI regressions cover 2–4 panes and retained page identities. Native CEF checks passed for dragging, menu targeting and focused-pane closure. Exact resize animation and overflow interaction states remain |
| Create by dragging a tab to the middle of the window | drag | partial | Drag a standalone sidebar tab to a page edge for a themed half-pane preview with a 2 px contour; the root gradient stays aligned while the original document is covered without reflow. Dropping commits the existing page; leaving the edge or cancelling removes the passive overlay. Pinned/Favorite sources and protected targets are copied to Today, retaining originals. GPUI and native CEF checks passed. The retained half has a 6 px rounded exposed edge, painted with antialiased theme samples without resizing the document. A dark 128×250 px drag card follows the original grab point, with a 20 px radius, centered favicon/title and shadow, matching the earlier valid Arc preview (the reference cuts off the card’s right edge, so its full width remains inferred). GPUI coverage checks card size, original grab anchoring and movement. Exact hit zones, full card measurement and transition timing remain |
| Horizontal/vertical layout, Move Left/Right/Up/Down | — | done | Command Bar actions preserve loaded page views |
| Separate Page / Separate All Tabs / Expand Current Split | — | partial | All three actions work. Expand Current Split preserves pane identities and focus in both axes, keeps other panes visible, fits small windows and persists sizes. GPUI and native CEF checks passed; expansion animation fidelity remains |
| Focus pane N / next / previous | (?) | done | Command Bar actions and clickable GPUI pane headers |
| Split View title ("Split view title text field placeholder") | — | done | Shared sidebar item title; rename/reset and archive search preserve it |
| Share Split View (link) | — | out of scope | Permanently excluded: Arc service/account dependency; no UI, including placeholders |

## 9. Little Arc

| Feature | Shortcut | Status | What's missing |
|---|---|---|---|
| Small window for quick lookups | ⌥⌘N | partial | Independent lookup tabs and website storage, 1000×760 reference frame, flush 46 px toolbar, hostname editing/copy, inherited Space theme and contrast. The global entry opens a full Little window with the regular Command Bar; Arc instead begins with a 490×329 prompt. That prompt, saved placement and opening motion remain unfinished |
| Global shortcut "Open Little Arc when I press … in any app" | configurable | partial | OS registration for the default command/option N follows the existing Settings toggle, suppresses held-key repeats and unregisters on quit. Busy bindings retry. Background entry is verified on macOS with an extra Ctrl modifier while Arc owns the default. Custom key recording, Wayland support and Windows/X11 native verification remain |
| Links from other apps open in Little Arc | — | partial | HTTP/HTTPS application URL events create an independent Little window; macOS bundle registers both schemes. Air Traffic Control routes external HTTP(S) events; other platform launch integrations remain unfinished |
| ⌥⌘-click on a link opens it in Little Arc | — | missing | — |
| Toolbar: Open in (Space, "More Spaces", "Search Spaces…"), Change profile, Extensions, Copy URL, downloads | ⌘O | partial | Cmd+O and Open in promote directly; the 244 px Space list has 46 px rows, search, keyboard selection and dismissal. Copy URL and clicking the vertically centered, full-height address preserve the complete URL while the resting address shows the hostname. Same-profile promotion transfers the live document, history and already downloaded favicon; the dark profile chip opens a 96 px menu with 24 px rows, a calibrated vector check, keyboard selection and cancellation. Switching replaces only the lookup in the chosen local storage and reloads its URL. Profile-menu material tint, Extensions and downloads remain unfinished |
| Show/Close all Little Arc windows | — | missing | — |
| Tip for new users "Links Open in Little Arc" | — | missing | — |

## 10. Peek

| Feature | Status | What's missing |
|---|---|---|
| Links to other sites from Pinned/Favorites open in Peek (option) | partial | Main-frame user GET navigation across hosts opens an isolated child card, with source Space material backdrop and three vector controls. Existing Peek setting gates routing. The child follows owner resize without recreating pages; a 1200×800 reference produces a 778×776 card at x325/y12 with a 228 px sidebar. Subdomain rules and exact material/motion parity need further validation |
| Shift-click opens a link in Peek (option) | partial | CEF NEW_WINDOW user disposition opens the card, gated by the existing setting. Nested Peek/Little routing remains unfinished |
| Closing Peek (⌘Esc / Esc), Undo "Close Peek" | partial | Close control, backdrop click and Escape restore the source without archiving it. Arc 1.170.0 leaves the card open on Cmd+Escape; Echo matches. Closing captures the live document, hides the native page and scales its pixels for 180 ms toward the source link, with bounded capture fallback. Opening uses a damped spring. Undo/Redo retains the hidden live document and history without navigation; audio is muted while closed. Card shadow is calibrated to the reference’s 9 px edge falloff. Backdrop opacity is calibrated to the reference’s 70% Space material overlay. Control tint and exact motion calibration remain unfinished |
| Moving Peek into a tab or a Space ("Open Little Arc or Peek in…" ⌘O) | partial | Expand and Cmd+O create a Today tab in the source Space. Split control fills a new pane and preserves Pinned/Favorite originals. Same-profile expansion and Split View adopt the existing renderer and preserve history and the downloaded favicon. A dedicated Peek Space picker and cross-profile transitions remain unfinished |
| Swipe to dismiss the modal (`modal-v2-native-gestures`) (?) | missing | — |

## 11. Air Traffic Control (link routing)

| Feature | Status | What's missing |
|---|---|---|
| "New Route" rules: URL or domain "Is equal to" … → Space / Most Recent Space / Little Arc | partial | Install-local ordered/enabled model; URL/domain/app-source matching; Space/Little/profile destinations; external URL delivery and Little promotion defaults; referenced GPUI inline route sheet with matcher/destination/default menus, blank cancellation, editing and deletion. Reference and limitations: [ATC_REFERENCE.md](ATC_REFERENCE.md) |
| Access from Links → "Air Traffic Control…" and from the Command Bar | done | Both routes open the same modal sheet in Links; Command Bar closure cannot steal the new Settings window’s focus |
| Ways to open a link: new tab (focused or not), Split View, Peek, new window, Little Arc | partial | Popups (`window.open`, target=_blank) open as a new tab; Shift-click and cross-host Pinned/Favorite links support Peek. Other link disposition modes remain unfinished |

## 12. Profiles

| Feature | Status | What's missing |
|---|---|---|
| Profile: separate history, cookies, passwords, extensions | partial | Profile-owned history/Favorites and persistent CEF contexts are isolated; cookies, localStorage and IndexedDB survived a full process restart. Password/autofill and extension management remain unimplemented |
| Creating a profile: name and which Spaces use it | partial | Atomic local creation and Space assignment work; visual matching and Settings entry remain incomplete. Synchronization notices and icons are permanently out of scope |
| Changing a Space's profile, with warnings ("Switch to your … Profile?") | partial | Loaded-page switching preserves the Today URL and recreates the document in the selected storage; empty-window warning policy and final visual matching remain unresolved |
| Deleting a profile by typing its name, "Delete unused Profiles". The default profile can't be deleted | missing | — |
| Per-profile settings: search engine, search suggestions, auto-archiving, downloads folder, document app, clearing data | missing | — |
| Profiles don't sync between computers (same in Arc) | — | Reference |

## 13. Settings (Preferences, ⌘,)

Reference tabs: General, Profiles, Links, Shortcuts, Max, Advanced, Account,
Icon. Echo excludes Account and sync. There is also an internal Staff tab.
The separate 684 × 650 GPUI Settings window follows the installed Arc reference.
Browser Account and synchronization remain permanently out of scope; Max and
AI-only provider settings are in scope. Arc artwork and
referral icon unlocks are permanently out of scope. Controls for features without an
implementation persist intent only (this does not authorize excluded UI); action-only routes without backing behavior
remain unavailable. The installed release puts Auto-Archive in Profiles, not
General. Screenshots are recorded under `target/parity/settings/` as `arc_<Tab>.png` and `echo_<Tab>.png`. Visual parity remains partial: toolbar vector shapes, the Links illustration, materials and unavailable-action appearance still differ. Native inspection covers dark macOS only; Windows, Linux and light appearance are unverified. The post-reboot check verified all six tab windows, native Cmd+W, and the persisted 7-day archive policy; the full four-thread test suite passed.

| Tab → option | Status |
|---|---|
| Account → Echo/Arc account, browser-account sign-in, Arc Members, unified browser login, recovery phrases | out of scope |
| Cross-device sync → Spaces, tabs, Favorites, settings; sync settings/notices/icons | out of scope |
| Arc telemetry and our own cloud backend | out of scope |
| General → Arc is not the default browser ("Set as default" button) | missing |
| General → "Profile Settings…" (search engine, auto-archive, downloads) | partial (opens Profiles in the same Settings window; native capture reviewed) |
| General → "Archive tabs after" | partial (the installed release places this dropdown in Profiles; no Command Bar interval picker) |
| General → Built-in blocker: Block Ads, Block Trackers, Block Cookie Banners, "Ad Block Settings…" (Advanced Ad Block Settings), advice to keep a single blocker | missing |
| General → "Previews Settings…" (Show Arc Previews: Gmail, Calendar, Outlook, Linear/Notion/Figma) | partial (observed seven-checkbox panel; choices persist, previews themselves remain missing) |
| General → Warn before quitting | partial (toggle persists; quit confirmation is not integrated) |
| General → Automatically update my Arc | out of scope (Arc service/account dependency; no UI) |
| General → Export Data (Spaces and tabs as Markdown, "Arc Data Export.md") | missing |
| Profiles → profile list, creating and deleting, Default Search Engine, Include search engine suggestions, Search Settings…, auto-archive, Download Location, Default Document App, Privacy and Security → Clear Browsing Data | partial (real profile inventory and existing creation sheet; search engine and per-profile archive policy wired. Suggestions control the Command Bar; document/download choices persist. Deletion, search management and data-page routes remain unavailable) |
| Links → Air Traffic Control…, Peek (2 options), Little Arc (shortcut, links from other apps, ⌥⌘-click, Archive Little Arcs after) | partial (Peek toggles gate native link routing; referenced Air Traffic Control sheet and routing are implemented; remaining Little Arc launch preferences need parity) |
| Shortcuts → search ("Type a feature name or shortcut"), Default / Customized Shortcuts, recording a custom shortcut, conflicts ("is already in use by …", Remove Conflicting Shortcut), per-shortcut priority (Arc wins / Website wins / first press to the site, second to Arc), Reset Shortcut / Reset All, Extension Shortcuts…, "Include Favorites in ordering", "Learn the Essential Shortcuts" | partial (searchable observed shortcut catalog, with local Space names. Recording, conflict/priority editing, reset, extension routes and the Favorites-ordering menu remain missing) |
| Max → Settings tab: Ask on Page (⌘F), Tidy Tab Titles, Tidy Downloads, ChatGPT in the Command Bar, 5-Second Previews, Automatically enable future Max features | partial (installed 1.170.0 layout measured read-only; requested 1.169 version is not installed: Enable Max, future switch, Tidy Tabs, Instant Links, 5-Second Previews, Tidy Tab Titles, Tidy Downloads and ChatGPT; flags persist. Original artwork replaces Arc branding. Ask on Page is absent from this settings capture; feature integrations and exact artwork/link styling remain) |
| Max → "Sign in with ChatGPT" (AI-only OAuth) | partial (public OSS dynamic registration/PKCE under Echo’s own name; validates signed identity and plan scope, stores credentials in OS keychain, supports returning registrations, refresh and revocation. No third-party client ID or Echo account. Offline flow/security tests pass; live account consent/inference remain unverified) |
| Max → manual API key entry for OpenAI and other AI providers | partial (masked key entry, provider/base URL/model configuration and model listing; native keychain interface for macOS, Windows and Linux. Direct streaming OpenAI Responses, Anthropic Messages and compatible chat transports; optional Anthropic workspace ID supports unscoped personal/service keys. Mac keychain round-trip and loopback HTTP/SSE pass; live provider requests and Windows/Linux store validation remain) |
| Advanced → More Settings… (arc://settings), Reset Translated-site Prompts, Enable Boosts, Allow websites to get your theme data, Picture in Picture when leaving a tab with video, dragging the window by the top of the page, Shared Quotes, Show full URL when Toolbar is enabled, restoring windows on launch, Haptic feedback when reordering, Play Arc sound effects | partial (observed three-group layout with nine persisted toggles. Engine/platform integrations and action buttons remain unavailable) |
| Icon → alternative app icons (unlocked by referrals) | partial (current Echo icon shown; proprietary Arc artwork and referral service permanently out of scope (no UI), alternative Echo icons not implemented) |

## 14. Library (⇧⌘L)

| Section | Status | What's missing |
|---|---|---|
| Spaces (Space Manager: order of Spaces, folders, tabs) | partial | Interactive cards, horizontal reachability, existing tab/folder/split controls, profile switching and creation. Theme/overflow controls, Back navigation and exact visual parity remain incomplete |
| Archive (see section 7) | partial | Available in the Command Bar; dedicated Library page remains missing |
| Downloads (⇧⌘J): filter, filter by profile, "download flies into Library" animation, "File deleted or moved" | partial | Canonical completed inventory, filename search, a single-profile filter, retained right-hand page, popup responder restoration and calendar headings are connected. Completed rows now use type/source subtitles, native macOS file icons and bounded background Quick Look thumbnails, inset rounded hover/menu highlighting and a hover overflow menu. Default destination/preferences, active rows/cancellation, multiple-profile filter semantics, unknown-date/empty-database presentation, file-deleted presentation, animation and general continuation admission remain unfinished; see [Downloads integration](../DOWNLOADS_INTEGRATION.md) |
| Media: Captures and files from Desktop/Documents/Downloads, Open in Preview, Show in Finder, Hide from Arc, Copy | partial | Measured 112 px rail, 305 px search/list column, 289 × 36 px search field at y=10, 56 px empty icon and 100 × 28 px Learn more button. Surfaces inherit the Space canvas; selected browser documents remain live in the preview, while an empty Space cannot leak another Space's page. File discovery, results/actions, Learn more destination and full theme/material parity remain unfinished |
| Boosts: list, on/off, rename, delete | partial | Profile-owned searchable list with measured 304 px column, hover toggles and a site preview linked to the floating editor. Rename/delete remain |
| Easels & Notes | — | Out of scope (Easels). Notes are low priority |

## 15. Boosts

| Feature | Status |
|---|---|
| Editor: color (color wheel, Brightness, Original Saturation, Invert lightness, Advanced color controls), font, text size and case | partial — floating GPUI simple editor, tint, 20 font specimens, size/case, inversion and advanced sliders; measured offsets, neutral wheel shading and shadows refined against zoomed crops; visual comparison retained under target/parity/boosts/round2 |
| Local per-domain Boost model, enabled state, CSS/JS and Zap selectors persisted per profile | done |
| Zap: pick an element to hide (Pick Selector, "Zap all related elements") | partial — trusted hover/click element picker, persisted selectors, Zap undo and the native page-bottom Done bar; related-element selector controls remain |
| Custom CSS/JS ("Refresh to Run"); Boosts with JS can't be shared | done — floating CSS/JS editor, multiline input with clipboard/IME/local undo, live CSS, Refresh to Run JavaScript and the observed Developer Tools footer action; local only |
| Turning on and off per site, "Customize Page…" in the context menu, Boosts section in the Site Control Center | done — site controls expose the reference’s round Boost indicator, name and enabled status; editor opens from dock, native menu, Command Bar, site controls and tab context menu |
| Share Space/Folder/Easel via arc.net; publish Easels/Boosts to the gallery | out of scope |
| Sharing and installing someone else's Boost ("Add this Boost to Arc?"), Boost Gallery | out of scope (Arc service/account dependency; no UI) |

## 16. Page, top bar and Site Control Center

| Feature | Status | What's missing |
|---|---|---|
| The page sits in an inset "card" with rounded corners | partial | Installed Arc measurements: 10 px top/right/bottom inset and 6 px radius; hidden sidebar gives 10 px on all edges. An empty Space retains the same rounded frame with a translucent light panel over the aligned Space field (about 10% in Dark, two-thirds in Light) and an outside shadow. Light/Dark reference captures confirm the distinct panel; exact shadow/border calibration remains under visual review |
| Loading indicator (glowing progress, `GlowProgressLoadingIndicator`) | partial | CEF loading-progress values now follow page identity through host transfers into PageState. The glowing indicator is not yet drawn |
| Site Control Center: security icon, Copy URL, Capture / Full Page / Portrait, Share, Boosts, extensions (pin/unpin, Add Extension), Clear Cache, Clear Cookies, All Site Settings… | partial | Measured 236 px popover, 48×32 action tiles, 14 px labels, 33 px permission rows and 162 px More menu. Profile/origin-bound CEF settings back permission toggles; security uses the live SSL/certificate result, with the native certificate sheet on macOS. Clear Cache and Clear Cookies clear through the existing page context and reload only the same document; Boost management/creation work. Camera/microphone/location/notification requests now use the reference’s 236 px prompt with Not Now/Allow (Escape leaves the question open), engine-owned continuations, document cancellation and profile persistence. Share still copies the URL; Capture, extension management, All Site Settings, embedded cross-origin requests, media-usage indicators and exact material/arrow/motion fidelity remain partial |
| Status bar with the URL when hovering a link (`StatusTextView`) | partial | CEF link targets appear in a passive GPUI pill above the page, preserving query strings and fragments in the underlying target. A short preview expands after 1.5 seconds. Measured URL elision preserves the host and final path segment, replacing intermediate segments and overflowing query text with explicit ellipses, as observed in Arc. Moving into browser chrome, navigation and hiding a page clear it; hidden pages cannot revive stale targets. Split targets retain their pane and do not change focus. Exact theme, transitions and host/subdomain formatting fidelity remain |
| Find in Page ("No results found"), Find and Replace | partial | Small GPUI panel above CEF, live search, result counts, next/previous, close and focus restoration. Find and Replace remains missing |
| Page translation: "Translate this page to %@?", Use Original, Choose Another Language, Never Translate This Site | missing | — |
| Crashed tab page (Sad Tab) | missing | — |
| Swipe back/forward on the page (overscroll) | missing (?) | Not checked whether the gesture works in CEF |
| Dragging the window by the top of the page (option) | missing | — |
| Pointer lock: "Your cursor is hidden. Press 'esc'…" toast | missing | — |
| Page full screen: "To exit full screen, press …" toast | missing | — |
| Developer mode for a site, DevTools | partial | Focused live-page DevTools commands are implemented through the original embedded CEF frontend; see View → Developer above. Per-site Developer Mode is still missing |
| Chrome extensions (icons, "Open pinned extensions picker") | missing | — |
| Built-in ad and tracker blocker | missing | — |
| Quietly creating a tab while the sidebar is hidden ("New Tab Created in %@") | missing | — |

## 17. Media and Picture-in-Picture

| Feature | Status | What's missing |
|---|---|---|
| Mini player in the sidebar: Back to Tab, Open Picture in Picture, Previous/Next Track, Adjust Tab Volume, Toggle Microphone/Camera, Close Mini Player | partial | Background audio uses the tab favicon/title, five transport slots, a 33 px bottom row, 33 px hover header and inline volume row. Native Arc reference measured roughly 200 ms expansion and a two-second exit delay. Play/pause and registered previous/next handlers address the retained source; per-tab output gain preserves mute state. Return restores the source without navigation; Close dismisses the card while playback continues. Measured tooltips use a 6 px pointer offset and clear on playback transitions. Main-frame HTML media and explicit Media Session handlers are supported. Video/PiP, microphone/camera, iframe/unmanaged off-DOM media and remaining icon/raster, theme and platform fidelity are unfinished |
| Automatic PiP when leaving a tab with video (option) | missing | Guarded Wikimedia playback confirms a 224 × 126 floating window with approximately 9 px corners and a remembered position. Echo does not request PiP on tab switches; a GPUI host remains unimplemented |
| PiP window: Back to Tab, ±15 seconds, seeking by dragging up, Keep Window on Top, minimize or close, remembered position | missing | Arc’s video context-menu action enters the standard video PiP API and displays the page’s native placeholder. Its floating-window context menu offers Open automatically, Turn off, Copy URL and Mute. Guarded window/desktop captures have not reproduced hover controls. CEF’s native video PiP API opens a Chromium-owned window, but GPUI surface/control integration and return/close parity remain unimplemented |
| PiP for Google Meet (with chat) and Document PiP | missing | Document PiP requests are currently rejected until a GPUI host is prepared. Rejection preserves the source tab instead of opening an unrelated about:blank tab. Native macOS video/Document PiP API probes verified both paths; Meet/chat and a GPUI document surface remain unimplemented |
| Per-tab audio activity | partial | CEF loopback PCM callbacks and stereo replay scheduled on Chromium’s monotonic timestamps, with stale-packet rejection, silence hysteresis, and provenance-checked background updates. Output routes/errors rebuild the low-latency stream; rate conversion keeps existing captures stable. Native macOS flash/beep check: 29 pulses, maximum offset 43.9 ms; output survives BlackHole → speakers → BlackHole (eight post-switch loopback beeps). Capture falls back to native playback when output initialization is unavailable. Exact Favorites/theme fidelity and Windows/Linux native A/V checks remain |
| Mute/Unmute a tab, Mute All / Unmute All | done | Playing/muted sidebar controls and disabled silent context-menu actions. CEF tab audio control through Command Bar, Tabs menu and tab context menus. Persists per tab and applies before a restored page is shown; Mute All preserves unloaded tabs without loading them |

## 18. Notifications and toasts

The main toasts from the `Toasts` module. One reusable GPUI child-window
component renders content-sized feedback above CEF without taking page focus.
Arc 1.170.0 native checks (2026-10-24) distinguish copy feedback (48 px high,
16 px right / 19 px top inset, trailing share symbol) from plain feedback
(42 px high, 4 px top/right inset; 48 px with an inline keycap). Both use
14 px semibold text, 12 px corners
and a purple gradient. Native Arc recordings measure a spring entry (ζ=0.694,
ω=15.42), five-second lifetime, and 200 ms shrink/fade exit anchored at the top
center. Replacement overlaps the same corner with independent outgoing content
and expiration; all child windows close with their browser. Reduced motion is
respected. The copy accessory opens the native system share menu with an immutable
copy-time URL list; changing tabs/Spaces cannot change its payload. Native
service-menu presentation is outside application/entity/window borrows. The compact native service menu preserves the main window’s keyboard input
after dismissal; macOS 27 service icons are explicitly visible. Hover timing,
normal-pointer menu anchoring, Windows/Linux
sharing remain incomplete.
Reference inventory: [TOAST_REFERENCE.md](TOAST_REFERENCE.md). Native evidence:
`target/parity/toasts/`.
Web notification permission/delivery is not part of this section's inventory.
Ordinary download completion is excluded from the hook list: no Arc completion
string exists in the extracted toast inventory; downloads use the indicator/Library.
Max download rename/revert feedback awaits the later Max implementation.

| Toast | Status |
|---|---|
| Undo for actions ("“%@”. ⌘Z to Undo.") | done (generic single/bulk archive and Clear Today; clickable transaction-scoped Undo and keyboard Undo) |
| Copied Current URL / clean link without trackers / Copied item links / as quote | partial (URL, clean URL and Markdown done; quote text-fragment copy and immutable share payload done; existing item-link handler shares copied URLs; multi-selection UI remains) |
| Pinned Tab to “%@” / Un-Pinned Tab from “%@” | partial (shortcut and Command Bar; only with hidden sidebar as observed in Arc; drag pin/unpin and folder pin routes wired) |
| Auto archived %d tabs; Tab/Folder restored to … | partial (automatic archive and clickable tab restore done; archived-folder model/restore remains) |
| New Tab Created in %@ (with the sidebar hidden); New Space Created | partial (background Command Bar/CEF opening preserves focus and offers reveal; background Space creation route missing) |
| Tab moved! Click to go there.; Pinned to folder | done (Command Bar/folder moves, folder creation and drag results; clickable move reveal) |
| Cannot Add New Pane (Split View) | done |
| Import: Now importing… / Success / partial / failure | missing |
| Translation: Translated to %@ / back to %@ / error | missing |
| Updates: update pending / failed / in progress | out of scope (Arc service/account dependency; no UI) |
| Battery Saver Mode Activated/Deactivated | missing |
| Shortcut tips ("Copy links faster by pressing…", "… to easily show and hide your Sidebar!", Site Search) | missing |
| Screenshot Copied to Clipboard, Capture in Progress… | missing |

## 19. Onboarding and import

| Feature | Status | What's missing |
|---|---|---|
| Import from Chrome-like browsers, Safari, Firefox: history, cookies, passwords (Passwords.csv), autofill, bookmarks with folder selection | partial | Read-only Chrome/Edge/Brave, Firefox and Safari bookmarks/history/supported open-tab sessions; selected folders/Pinned bookmarks and ordered tabs publish into a new Space. GPUI selection, category progress/cancellation, resource-backed outcome toasts and CSV upload/drop to a macOS local Keychain boundary. A wholly failed read creates no Space/profile; new reads discard stale selections. Encrypted sessions/source password databases, modern Safari formats, cookies/autofill and later-screen visual parity remain. See docs/ONBOARDING_IMPORT_WIP.md |
| Choosing the profile to import into ("My Existing Arc Profile" / "Create a New Profile") | partial | Existing/new local profile selection and atomic publication covered by GPUI/native fixture checks; Arc destination-picker visuals remain unverified |
| Steps: theme ("Add some flair with a theme."), Favorites ("Choose the apps you use most."), ad blocker, Arc Max, default browser, member card | partial | Fresh profiles enter persisted/resumable local theme, Favorites, import and default-browser setup; older sessions skip it. Uses existing theme APIs and resource copy. Full onboarding layout/motion and Favorites catalog, blocker and later AI steps remain; account/sync/member-card steps excluded. See docs/ONBOARDING_IMPORT_WIP.md |
| "Let’s settle in! Here are the basics." screen (Pin tabs, Split View, Spaces, Open your first tab, with video) | partial | Local basics screen with resource copy and shortcuts; video and full Arc layout remain |
| "Essential Keyboard Shortcuts" window (⌘T, ⌃Tab, ⇧⌘C, ⌃⇧=, ⌥⌘N) | partial | Resource-derived shortcuts accessible from setup, with Esc returning to basics; separate-window geometry remains |
| Empty state: "Open your first tab. Click New Tab or press ⌘T…" | partial | We show a "No tab open / Press ⌘T to open a new tab" placeholder |
| Default browser prompts ("Try Arc as default for one week" etc.) | partial | Explicit first-run default request, skip/cancel behavior and app-menu action; recurring weekly prompts and final OS consent validation remain |

## 20. Local sessions and persistence

| Feature | Status | What's missing |
|---|---|---|
| Saving the session between launches (Spaces, tabs, archive) | done | One profile owner writes `session.json`, including after the original window closes. Uncommitted split drafts from all bound windows are excluded. Demo data only on first launch; URLs are saved, back/forward history isn't |
| "When opening Arc, restore windows from previous session" | missing | — |
| Cookies persist between launches | done | `persist_session_cookies` in the engine |

## 21. Gestures and feel

| Feature | Status | What's missing |
|---|---|---|
| Sidebar swipe: Spaces follow the fingers, resistance at the edges, flick by speed, snapping back after a short swipe | partial | Measured clock-based spring, halfway threshold, release velocity, short-drag return and moving default dock indicator implemented; exact flick cutoff, edge resistance, custom-icon and virtual-page motion await reference validation |
| Animated Space switching from the keyboard | done | — |
| Hidden sidebar slides in from the edge and hides after a delay | done | — |
| Swipe back/forward on the page | missing (?) | — |
| Animated tab dragging, dragging to the middle for Split | missing | — |
| Haptic feedback and Arc's sounds (`ARC_SoundEffects`) | missing | — |
| Springy click on sidebar items | partial | Measured ordinary-row compression/expansion implemented with elapsed time and Reduced Motion support; other item types and reorder settling remain unfinished |

## 22. AppleScript (`Arc.sdef`)

| Feature | Status | What's missing |
|---|---|---|
| Classes: window (active tab, active space, incognito, mode), tab (id, title, URL, loading, location: topApp/pinned/unpinned), space (id, title) | missing | macOS-specific. Could be replaced by a cross-platform automation API (?) |
| Commands: make, close, select, go back, go forward, reload, stop, execute (JavaScript), focus (space) | missing | — |

## 23. Other

| Feature | Status | What's missing |
|---|---|---|
| Capture (screenshot of an area, ⇧⌘2), saved to Library | missing | — |
| Quiet downloads and the downloads tray | missing | — |
| Battery Saver mode | missing | — |
| Task Manager, trace recording | missing | — |
| Shared Quotes (link to the selected text) | missing | — |
| Arc Max (AI: Tidy Tabs, tab/download renaming, 5-Second Previews, Ask on Page, ChatGPT in the Command Bar, Browse for Me, Instant Links) | missing | User-provider integrations, AI-only ChatGPT OAuth and manual API keys; credentials stored locally, no Echo intermediary server |

---

## Summary

Counted from the table rows above, including rows marked (?). Reference
rows without a status aren't counted. **Out of scope** rows are permanent
exclusions, not outstanding work; mixed rows count only their local status.

- **done**: 65
- **partial**: 109
- **missing**: 121
- **out of scope**: 10

### Biggest gaps (by importance for parity)

1. **Command Bar as the control center.** Needs actions by name (Type
   Travel) for remaining menu items, history, search suggestions and Site
   Search with Tab. Basic actions, Space switching and tabs from all Spaces
   are implemented.
2. **Archive.** Search, Space/reason filters, local date groups, older entries,
   restore/delete/clear work in the Command Bar; interval selection lives in Settings → Profiles.
   Needs the dedicated Library page and Little Arc entries.
3. **Sessions.** Back/forward history of tabs isn't saved.
4. **Favorites, folders, drag and drop and sidebar context menus.**
5. **Split View** (⌃⇧=, dragging to the middle).
6. **Peek, Little Arc (⌥⌘N) and Air Traffic Control.**
7. **Managing Spaces and the theme editor.** Named Spaces can be created
   and renamed. Needs icons, deleting, plus multi-color gradients, color sets, noise,
   Intensity and Light/Dark.
8. **Profiles.** Local ownership, persistent storage isolation, creation and
   reassignment foundations work. Finish visual matching, warning conditions,
   deletion UI, credentials, extensions and profile settings.
9. **Settings window**, including the shortcut editor with the "Arc or
   website" priority.
10. **Shortcuts.** Finish the remaining menu actions and shortcut customization/website-priority behavior.
11. **Basic browser features:** downloads,
    DevTools, print, loading indicator, new and incognito windows.
12. **Media controls and PiP, toasts with Undo, onboarding and import.**
