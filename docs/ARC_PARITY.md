# Parity with Arc

Comparison with Arc for macOS version 1.169.0 (`/Applications/Arc.app`).
Our code: `crates/core`, `crates/browser`, `crates/engine` as of this
document's date.

**Where the data comes from.** Arc was never launched. The initial inventory comes from
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

Statuses: **done**, **partial**, **missing**.

---

## 1. Main menu and keyboard shortcuts

Shortcuts come from the nib. Menu items have an `arcKeyBindingID`, so users
can rebind them (see "Settings → Shortcuts"). Items marked "hidden" exist in
the menu but aren't shown; usually they are alternative shortcuts.

### Arc (app menu)

| Feature | Arc shortcut | Status | What's missing |
|---|---|---|---|
| About Arc | — | missing | About window with licenses (Arc's `Credits.html`) |
| Preferences… | ⌘, | missing | No settings window at all |
| Share Arc (referral link) | — | missing | Out of parity (Arc service) |
| Set as Default Browser | — | missing | Registering as the default browser, prompts |
| Import from Another Browser… | — | missing | See "Onboarding and import" |
| Check for Updates… | — | missing | Auto-update |
| Privacy Policy | — | missing | — |
| Hide Arc / Hide Others / Show All | ⌘H / ⌥⌘H / — | partial | Native menu and shortcuts wired through the platform interface on macOS, omitted elsewhere. Final native behavior check awaits an unlocked desktop |
| Sign Out | — | missing | No account |
| Quit | ⌘Q | done | No "Warn before quitting" confirmation |

### File

| Feature | Arc shortcut | Status | What's missing |
|---|---|---|---|
| New Tab… (opens the Command Bar) | ⌘T | done | — |
| New Window | ⌘N | missing | We have a single window |
| Blank Window | ⌃⌘N | missing | — |
| New Incognito Window | ⇧⌘N | missing | Incognito window with its own sidebar ("Incognito Tabs") |
| New Little Arc Window | ⌥⌘N | missing | See "Little Arc" |
| Restore Last Closed Tab | ⇧⌘T | done | Brings the last archived tab back to its Space |
| Open Command Bar (on the current URL) | ⌘L | done | — |
| New Boost / Edit Boost / Boost Gallery | hidden | missing | See "Boosts" |
| New Profile | — | missing | No profiles |
| New Note | ⌃⇧N | missing | Notes (Arc Notes) |
| New Note (in Split) | ⌃⌥N (hidden) | missing | — |
| Close Window | ⇧⌘W | missing | — |
| Archive Tab | ⌘W | done | Today tabs go to the archive; a pinned tab closes, stays in Pinned and returns to its pinned URL |
| Share… | — | missing | System share menu |
| Capture… | ⇧⌘2 | missing | Screenshot of a page area |
| Capture Full Page / Capture in Portrait Mode | — | missing | — |
| Save Page As | ⇧⌘S | missing | — |
| Print | ⌘P | missing | — |

### Edit

| Feature | Arc shortcut | Status | What's missing |
|---|---|---|---|
| Undo / Redo (including sidebar actions: "Undo moving a space", "Close Peek" etc.) | ⌘Z / ⇧⌘Z | missing | No undo manager for the sidebar. Undo in the Command Bar input not checked either (?) |
| Cut / Copy / Paste / Select All | ⌘X/⌘C/⌘V/⌘A | partial | Work in `TextInput`; Edit menu includes OS editing selectors for native page responders. Page editing through CEF still needs an unlocked visual check |
| Copy URL (clean link without trackers) | ⇧⌘C | done | Removes known advertising query parameters, preserves signed URLs, copies to the clipboard and shows a toast |
| Copy URL as Markdown | ⌥⇧⌘C | done | Escaped page title and clean link; also available in the Command Bar |
| Copy URL as Quote (link to the selected text) | ⌃⇧⌘C | missing | — |
| Paste and Match Style | ⇧⌘V | missing | — |
| Find… / Find and Replace / Find Next / Find Previous / Jump to Selection | ⌘F / ⌥⌘F / ⌘G / ⇧⌘G / ⌘J | partial | Find in Page via CEF, live matches/count, next/previous (Enter/Shift+Enter, ⌘G/⇧⌘G), Escape to close. Find and Replace and Jump to Selection remain missing |
| Spelling, Substitutions, Transformations, Speech, Format (Bold/Italic/Underline) | ⌘: ⌘; ⌘B ⌘I ⌘U | missing | Standard macOS submenus |

### View

| Feature | Arc shortcut | Status | What's missing |
|---|---|---|---|
| Appearance: websites use Automatic / Light / Dark | — | missing | — |
| Show/Hide Sidebar | ⌘S | done | — |
| Show/Hide Toolbar (top bar mode) | ⇧⌘D (hidden) | missing | No URL bar at the top |
| Expand/Collapse Pinned Section | hidden | done | Click the Space name or use the Command Bar; pins retract/expand beneath the name, with no separate Pinned label. Animation reverses on another click; state persists per Space |
| Reveal Current Tab | ⌃Space (hidden) | missing | — |
| Stop Loading | ⌘. | done | CEF stop_load; also available in the Command Bar |
| Refresh the Page / Force Refresh | ⌘R / ⇧⌘R | done | Reload and reload_ignore_cache through CEF |
| Clear Cookies and Refresh / Clear Cache and Refresh | — | missing | — |
| Add Split View | ⌃⇧= | partial | Picker, direction and four-pane limit implemented; fresh native visual review remains |
| Close this Split Pane | ⌃⇧- | done | Menu, Command Bar, pane header and engine shortcut |
| Separate Page from Split View / Expand Current Split | — | partial | Separate Page implemented; Expand Current Split remains |
| Zoom to Actual Size / Zoom In / Zoom Out | ⌘0 / ⌘+ (⌘=) / ⌘- | done | Chromium zoom steps from 25% to 500%, reset to 100%; also in the Command Bar |
| Cast | — | missing | — |
| Developer: View Source, Developer Tools, Inspect Elements, JavaScript Console, Network Inspector | ⌥⌘U, ⌥⌘I, ⌥⌘C, ⌥⌘J, — | missing | DevTools not wired up through CEF |
| Developer: Allow JavaScript from Apple Events | — | missing | — |
| Toggle Developer Mode (per site) | ⌃D | missing | — |
| Enter/Exit Full Screen | ⌃⌘F | partial | View menu, page shortcuts and overlays target the main window; Enter/Exit commands work in GPUI tests. Final native behavior check awaits an unlocked desktop |
| Exit (leave a context: Peek, full screen) | ⌘Esc | missing | — |

### Spaces

| Feature | Arc shortcut | Status | What's missing |
|---|---|---|---|
| New Space… | — | partial | Create a named Space from the dock + button or Command Bar. Icon, theme and profile choices in the creation dialog remain missing |
| Edit Theme… | — | missing | See "Space themes" |
| Rename Space | — | done | Command Bar action opens a focused name editor; Escape cancels |
| Change Profile | — | missing | — |
| Next Space / Previous Space | ⌥⌘→ / ⌥⌘← | done | — |
| Go to Space N ("Jumps over to your %@ Space") | ⌃1…⌃9 (?) | done | Default shortcuts aren't in the nib |
| Manage Spaces… (Space Manager in Library) | — | missing | — |

### Tabs

| Feature | Arc shortcut | Status | What's missing |
|---|---|---|---|
| New Tab… | ⌘T | done | — |
| New ChatGPT Message… | ⌥⌘G (hidden) | missing | Arc Max feature |
| Join Meeting (link to the next meeting) | ⌃⌘J (hidden) | missing | — |
| Peek Email… (code from an email) | ⇧⌘E (hidden) | missing | — |
| Pin/Unpin Tab | ⌘D | done | — |
| Open Little Arc or Peek in… (pick a Space) | ⌘O | missing | — |
| New Folder… | ⇧⌘N per nib (?) | partial | GPUI name editor from Spaces menu and Command Bar; the nib's shortcut conflicts with Incognito, so no binding is assigned yet |
| Duplicate / Rename Current Item / Collapse All Folders / Expand All Folders | hidden | done | Tab duplication/rename and current-Space folder collapse/expand are available from menus and the Command Bar |
| Next Tab / Previous Tab | ⌥⌘↓ / ⌥⌘↑ | done | — |
| Tab Switcher forward/back ("Recent Items" overlay, MRU, like ⌘Tab) | ⌃Tab / ⌃⇧Tab | partial | Our ⌃Tab steps through tabs in sidebar order. No overlay, no most-recently-used order |
| Cycle Tab Forward/Backward | ⌃⌘→ / ⌃⌘← (hidden) | done | Same grouped sidebar order as Next/Previous Tab; intercepted inside CEF pages |
| Go to Sidebar Item 1–8 / last item (with or without Favorites) | ⌘1…⌘9 (?) | done | Favorites (optional), Pinned, Today; Split Views count as one item. Include/Exclude Favorites in Ordering is persistent and available in the Command Bar and Tabs menu |
| Switch to Split View 1–4 / next / previous | (?) | done | Command Bar actions |
| Reveal Tab in Sidebar | — | done | Command Bar and Tabs menu show the sidebar, unfold Pinned and scroll to the active item; Split View pane focus and loaded pages are preserved |
| Clear Unpinned Tabs (Today) | ⇧⌘K | done | — |
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
| Minimize / Zoom / Bring All to Front | ⌘M | partial | Window menu and Command Bar actions wired through GPUI. The locked desktop did not confirm minimization; final native behavior check remains |
| View Library | ⇧⌘L | missing | See "Library" |
| View Downloads | ⇧⌘J | missing | — |
| View Media / View Boosts | — | missing | — |
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
| Vertical sidebar with tabs | — | done | CEF site favicons are cached for unloaded/restored tabs, with letter fallback. Width can't be changed (?) |
| Window buttons (traffic lights) inside the sidebar | — | done | — |
| Reopen after closing the last window | Dock | done | Restores the saved session in a new window; keeps CEF initialized across window lifetimes |
| Address field at the top of the sidebar (click opens the Command Bar with the URL) | — | done | No back/forward/reload buttons, no Site Control Center button |
| Hiding the sidebar and sliding it in when hovering the left edge | ⌘S | done | Docked and floating panels move native window buttons on the same horizontal track. Reversible docked slide, 100 logical px pointer buffer past the floating panel, without extending its input surface. Reduce Motion skips travel |
| "Show as Toolbar" mode (context menu: Show as Toolbar / Hide Toolbar) | ⇧⌘D | missing | — |
| **Favorites** (Top Apps): icon grid above Spaces, shared by all Spaces of a profile | — | partial | Shared persistent dock (up to 12) with cached site favicons, click/keyboard activation and title/URL tooltips. Close unloads and resets the original URL; Favorite/Unfavorite and moves preserve the live page. Drag items into a nonempty dock, insert at an icon, reorder existing icons or move Favorites into folders/Pinned/Today. Split Views stay atomic, current page/pane stays focused during sorting, and saved order drives numbered shortcuts. An empty-dock drop target, unread badges, calendar captions and profile isolation remain missing. Fresh visual check awaits an unlocked desktop |
| Hover previews for Favorites, Pinned and folders (Gmail, Google/Outlook Calendar, Linear, Notion, Figma, Confluence) | hover | missing | Configured in "Previews Settings…" |
| **Pinned** section of a Space | ⌘D | partial | Closing keeps the tab pinned and resets it to its pinned URL. Missing: "Edit Pinned Page" (custom URL). Reset to Original URL and Replace Pinned URL with Current are available in the Command Bar; section collapses from the Space name without a separate Pinned header |
| **Today** section (unpinned) and the "+ New Tab" button | — | done | — |
| "Clear" button above Today (archives unpinned tabs) | ⇧⌘K | done | Click Clear above Today; preserves Pinned and other Spaces |
| "Tidy Tabs": AI grouping of Today by topic | — | missing | Arc Max feature, can wait |
| Folders: nested, collapse/expand, turn a folder into a Space, folder from selected tabs | ⇧⌘N (?) | partial | Persistent ordered nested Pinned tree with GPUI creation, rename, icons, collapse/expand and folder/tab context menus. Move pages, complete Split Views or folder subtrees across Spaces without recreating pages; tabs and complete folder subtrees can also be dragged onto folder headers or Space icons. Hover a Space icon during a drag to reveal its folders. Invalid self/descendant or over-depth folder drops are rejected. Closed folders stay closed and reveal the active item. Reveal Tab unfolds ancestors. Turn into Space promotes the children into a new Space with the folder's name/icon, preserving page and group identities. Folder sibling insertion, previews and multi-selection remain missing; fresh visual check awaits an unlocked desktop |
| Live Folders (GitHub Pull Requests: Created by Me, Review Requests…) | — | missing | — |
| Dragging tabs (between sections, into folders, into Favorites, into another Space) | drag | partial | Drag Today, Pinned or Favorite items onto folders, Pinned/Space headers, Today headers / New Tab, or a nonempty Favorites dock. Drop on a row's upper/lower half to insert before/after it and adopt its section/folder; an insertion line shows the edge. Space icon drops preserve Today/Pinned (Favorites become Pinned); hovering 500ms switches Spaces so the drag can continue to a folder or row. Leaving early cancels the hover. Complete Split Views stay atomic and keep their source pane even when the destination Space had another pane selected; loaded CEF views are reused. Closed folders stay closed. The Space name also accepts nested folders at the Pinned root. Favorites can be reordered or inserted at an icon; a full dock allows sorting but rejects new items. Drops outside targets cancel without activating an inactive tab. An empty Favorites drop target and edge auto-scroll remain missing |
| Haptic feedback when reordering tabs | — | missing | Option in Advanced |
| Multi-selecting tabs (⌘/⇧-click) | — | missing | — |
| Renaming a tab by double-click ("Double-click to rename") | double-click | done | Double-click or Rename Current Tab… in the Command Bar. Custom names persist through navigation, archive and sessions; Reset Tab Name restores the page title. Unicode custom icons use their own editor |
| Custom tab icon ("Customize Tab") | — | partial | One Unicode symbol/emoji for tabs and Split View groups, with grapheme validation and Reset Name and Icon. Persists through pinning, Favorites, moves, duplication, archive and sessions. Graphical icon catalog and uploaded images remain missing |
| Close button on hover (× for Today, − for Pinned) | hover | done | — |
| Audio indicator, Mute/Unmute in the context menu | — | partial | Mute/Unmute works from the context menu; muted tabs/Favorites show a clickable Unmute indicator. Actual audio-playing detection remains missing |
| Camera/microphone indicator on a tab | — | missing | — |
| Split View item in the sidebar (tab group) | — | done | One item for 2–4 pages; shared title, Favorites, pinning, archive and last focused pane |
| Space header (name), Show/Hide Space Header | — | done | Visibility persists per Space; Command Bar and Spaces menu. The name and Space icon form the collapse/expand control for Pinned tabs and folders; hover replaces the icon with a vector chevron and reveals Space actions at the right. Collapsed state keeps the rightward chevron visible; the actions button does not toggle pins |
| Browser chrome iconography | — | partial | Portable vector geometry for existing navigation, sidebar, folder, split, search, sound, history and action controls replaces font glyphs and default folder/mute emoji. Sidebar navigation and toggle controls are visible beside native window buttons. User-selected emoji and website favicons remain content. Full graphical Space icon catalog is still missing |
| Spaces dock at the bottom (Space icons) | click | partial | User-selected symbols/emoji persist per Space and render in the dock. Default icons remain colored dots; graphical icon catalog remains missing |
| "+" button in the dock: New Tab, New Space, New Folder, New Split (right/bottom), New Note, New Boost | — | partial | Dock + creates a Space. The multi-action menu remains missing |
| "Springy" click animation on sidebar items | — | missing | — |
| Incognito window sidebar ("Incognito Tabs") | — | missing | — |
| Banners at the bottom of the sidebar: update, "We cleaned up!" after auto-archiving, speed-up (several ad blockers) | — | missing | — |
| Media controls in the sidebar | — | missing | See "Media and PiP" |

## 3. Spaces

| Feature | Shortcut/gesture | Status | What's missing |
|---|---|---|---|
| Several Spaces, each with its own Pinned/Today and active tab | — | done | — |
| Two-finger swipe on the sidebar: Spaces follow the fingers and rubber-band at the edges | gesture | done | Distance or a fast flick commits the gesture; pausing before releasing a short drag springs back. Velocity uses GPUI's clock, so distance/flick/pause behavior is covered deterministically |
| Switching Spaces from the keyboard (animated) | ⌥⌘→/← | done | — |
| Creating a Space ("Create a Space" dialog: name, icon, theme, profile) | — | partial | Named Spaces can be created from the dock or Command Bar with an automatic accent; Turn into Space converts a folder and inherits its name/icon. Space icons can be edited afterward. Theme/profile controls in the creation dialog remain missing |
| Renaming a Space | — | done | Name editor via the Command Bar, Spaces menu or header/dock context menu; persisted with the same Space identity. Context actions can target an inactive Space without changing the current page |
| Space icon (emoji or symbol, "Change Space Icon…", "Remove icon") | — | partial | Persistent Unicode symbol/emoji editor via Spaces menu, Command Bar or the clicked Space's context menu; blank input removes the icon. Graphical icon catalog remains missing |
| Deleting a Space (with Undo) | — | missing | — |
| Reordering Spaces ("Move Space") | drag | done | Drag dock icons onto another Space to move them to that position. Stable Space/page/group identities and active pane are preserved; the pager stays on the selected Space without animating through unrelated Spaces. Dropping outside the dock cancels; order persists in the session |
| Turning a Space into a folder ("Export “%@” into Folder") | — | missing | — |
| Live Folders for a Space | — | missing | — |
| Space Manager (Library → Spaces) | — | missing | — |
| Assigning a profile to a Space ("Set Profile", "Edit Space profile") | — | missing | — |

## 4. Space themes and colors (Space Style Editor)

| Feature | Status | What's missing |
|---|---|---|
| The Space color tints the sidebar and window; during a swipe the color blends smoothly | partial | We only have a hue and the blend during swipes. No gradients or grain |
| Theme editor ("Edit Theme Color…", "Tap to pick a color for this space") | missing | — |
| Palette with several colors ("Add color" / "Remove color"), gradient of 2–3 colors (?) | missing | — |
| Preset color sets in pages: Brand, Drab, Greyscale, Pastel | missing | — |
| Noise texture ("Cycle noise texture") | missing | — |
| "Intensity" slider | missing | — |
| Mode: Automatic / Light / Dark Appearance | missing | — |
| Undo "Update Theme" | missing | — |
| Websites can read the theme ("Allow websites to get your theme data", CSS variables `--arc-palette-*`) | missing | Option in Advanced |

## 5. Command Bar (⌘T / ⌘L)

| Feature | Shortcut | Status | What's missing |
|---|---|---|---|
| Popup over the page | ⌘T | done | Uses the full browser window bounds, including when opened from the floating sidebar |
| Entering a URL or searching ("Search or Enter URL…") | — | done | The search engine is hardcoded (Google). Arc lets you pick it in profile settings |
| Editing the current URL | ⌘L | done | Enter on an unchanged URL reloads the page and preserves its title |
| Navigation with ↑/↓, ⌃N/⌃P, Enter, Esc; clicking outside closes it | — | done | Repeating ⌘T or ⌘L dismisses its mode; switching between them replaces the input in the same overlay |
| Switching to an open tab | — | partial | Lists matching tabs across all Spaces, identifies the destination Space and switches there. Split View results are deduplicated by group, match pane titles/URLs and focus the matching page |
| Search engine suggestions (Chromium: Search Suggestion, Navigate Suggest) | — | missing | Also needs the suggestions privacy screen |
| History and bookmarks in suggestions (Chromium: History / Bookmark / Search History) | — | partial | Local browsing history matches title/URL in ⌘T and ⌘L, with keyboard/click selection; open tabs match original page titles and suppress duplicate history rows. Bookmark/folder sources remain missing |
| Calculator (Search Calculator Answer) | — | missing | — |
| Popular domains (Popular Domains, ARC_TopDomains) | — | missing | — |
| Site Search: Tab or Space after a site's name searches that site; has a management menu | Tab / Space | missing | — |
| Removing a suggestion ("Delete Suggestion") | — | partial | × removes a local history suggestion and its saved visit without opening it; the query and keyboard selection remain usable. Remote search suggestions are not implemented |
| **Actions (Type Travel)**: every menu item is available by name and synonyms (`arcTypeTravelTriggers` in the nib) | — | partial | Implemented actions with names and synonyms: sidebar, pin/unpin, close/archive, clear Today, restore, reload/force reload, stop, zoom, Find in Page, back/forward, next/previous tab, window controls, Space/tab management and archive. Remaining menu actions follow their features |
| Space actions: "Focus on <space>", "Switch to …" | — | done | Matches Space names and synonyms; Enter or click switches with the pager animation |
| Moving a tab: "Move to Pinned in <space>", "Move to Today in <space>", "Move to Folder" | — | done | Search destinations by Space and full folder path; direct Pin to <folder> actions identify the Space when names overlap, and Pin to New Folder opens a name editor around the current item. Move to Folder creates a folder if none exist. Identity, custom name, selected Split View pane and loaded pages are preserved; destination is focused. Follows Arc's [organization actions](https://start.arc.net/command-bar-actions) |
| Favorites: "Favorite Tab", "Unfavorite Tab", "Pin to Top Apps" | — | done | Command Bar and Tabs menu; Unfavorite returns the tab to Today in the current Space |
| Split: Add Top/Bottom/Left/Right Split, Convert to Vertical/Horizontal | — | done | Command Bar actions and pane context menus; existing pages or URLs can fill up to four panes |
| Folders: Create New Folder, Collapse/Expand All Folders | — | done | Name editor, current-Space collapse/expand, named Expand Folder / Rename Folder, Pin to <folder> and Pin to New Folder actions |
| Tabs: Duplicate Current Tab, Rename Current Tab…, Reset Tab, Replace Pin with Current Page, Mute/Unmute All Tabs | — | partial | Rename Current Tab, Reset Tab Name and Mute/Unmute All are implemented; Reset Tab and Replace Pin with Current Page work for modified pins. Duplicate works for ordinary tabs, Pinned, Favorites and entire Split Views, creating independent page identities |
| Other: Toggle Sidebar, Manage Passwords, Search Settings, Air Traffic Control, Edit Keyboard Shortcuts, Check for Updates, Undo/Redo <action> | — | partial | Toggle Sidebar is available; remaining actions need their features |
| Running an extension by name ("Extension …") | — | missing | — |
| Join next video call / Zoom | — | missing | — |
| Suggestions for an empty field (Air Traffic Control, Tips for Organizing, Help Center) | — | missing | — |
| ChatGPT in the Command Bar, "Browse for Me" (Arc Max) | — | missing | AI features, lower priority |

## 6. Context menus

Strings come from the `ContextMenus/SidebarMenu+*` module. Short item names
(under 15 bytes: Copy Link, Rename…, Duplicate etc.) are stored separately
in code, so some were reconstructed from translator comments (?).

| Menu / item | Status | What's missing |
|---|---|---|
| **Tab**: Copy Link, Copy link as Markdown, Share, Duplicate, Rename (?), Edit Pinned Page → Replace Pinned URL with Current, Add/Open in Split View, Move to… (Spaces, profile), New Folder with…, Mute/Unmute, Archive Tab / Close Tab / Remove Pin / Remove Favorite, Archive Other Tabs / Tabs Below / Tabs Above, Customize Page… (Boost) | partial | GPUI right-click menus on tabs/Favorites: Copy Link/Markdown, Duplicate, Rename, Pin/Unpin, Favorite/Unfavorite, Reset/Replace Pinned URL, Customize Tab Icon, Reset Name and Icon, Add Split View, Mute/Unmute, Close/Archive and Archive Other Tabs/Tabs Above/Tabs Below (Today items only; Split Views stay atomic). Up/Down/Enter/Escape, scrolling in short windows and outside dismissal; actions target the clicked tab. Copy/rename/pin/mute preserve other active tabs; Duplicate focuses its independent copy. Remaining actions follow their missing features; fresh visual check awaits an unlocked desktop |
| **Multi-selection**: Archive Items, Copy links (as Markdown), New Folder with N items, Move to… | missing | — |
| **Folder**: Share, Duplicate, Archive Folder / Delete, New Nested Folder…, Turn into Space, Copy All Links (as Markdown) | partial | GPUI context menu supports rename, nested creation, moving the complete subtree, icon changes, collapse/expand, deleting empty folders and copying all nested page links (plain/Markdown). Duplicate creates an independent ordered subtree with names, icons, Pinned URLs and Split View geometry intact; focus stays on the current page and copied engines load on selection. Turn into Space moves the children into a new Space, preserving active pane and engine views; empty folders create empty Spaces. Share and archiving nonempty folders remain missing |
| **Live Folder**: Pull Requests, Repositories, Single Sign-On, Delete Live Folder | missing | — |
| **Split View in the sidebar**: Share Split View…, Separate All Tabs, Add Split View, Convert to Vertical/Horizontal, Tab Options → Reset Name and Icon, Duplicate, Archive…, Separate from Split View | partial | Group separation, duplication, add pane, orientation, reset name, pin/Favorite, group mute and archive implemented. Share and graphical icon catalog remain |
| **Split View pane** (control menu): Separate Tab from Split, Share Split View…, Convert orientation, Move Left/Right/Up/Down, Add Left/Right/Top/Bottom Split | partial | GPUI ellipsis and right-click menus target the clicked pane; all listed local actions implemented. Share remains |
| **Space**: Change Space Icon…, Rename, Edit Theme Color…, Set Profile, New Folder, Live Folders, Share, Show/Hide Space Header, Manage Spaces…, Delete Space, New Profile, Export into Folder | partial | GPUI right-click menu on headers and dock icons: Rename, Change Space Icon, Show/Hide Space Header and New Folder. Actions target the clicked Space; editing name/icon/header preserves the current page, including for inactive Spaces. A hidden header can be restored from the dock. Other actions follow their missing features |
| **Favorite with a calendar** (Live Calendar): Show Hover Preview, Show Time to Next Meeting, Show Next Meeting Link, reminder 2/5/10 minutes before | missing | — |
| **Download**: Open, Copy, Show in Finder, Cancel Download, Hide from Arc, Move to Trash | missing | — |
| **Tab group header**: Remove header | missing | — |
| **Page** (Chromium's menu plus Arc's items) (?) | missing | The page context menu isn't set up through CEF (?) |

## 7. Archive and auto-archiving

| Feature | Status | What's missing |
|---|---|---|
| Auto-archiving idle Today tabs, 12 hours by default | done | A once-a-minute timer in `window.rs`, logic in `core::archive_idle` |
| Intervals: Never, 1 hour (?), 6 hours, 12 hours, 24 hours, 7 days, 30 days | done | Command Bar → Archive Tabs After… opens a searchable picker; current interval first, persisted in the session |
| Per-profile setting and the "apply to all profiles / only this one" question | missing | — |
| Separate "Archive Little Arcs after:" interval | missing | — |
| ⌘W and × send the tab to the archive | done | — |
| Archive page in Library: "Today/Yesterday", filters by Space, "How was the tab closed?" (Auto-Closed/Manually Closed), Little Arc | partial | Command Bar archive browser searches titles/URLs (including Split View panes), combines Space and reason filters, groups by local calendar day (Today/Yesterday/date), and loads older entries with Show More. Keyboard navigation and a bounded scrolling list are implemented. Dedicated Library layout and Little Arc remain missing |
| Restore ("Tab restored to …. Click to view."), Delete, Clear Archive | done | Restore a selected entry and activate its original Space with a toast; delete an entry with ×; clear all with confirmation |
| "Auto archived %d tabs" toast and the "We cleaned up!" banner | missing | — |

## 8. Split View

The persistent model supports 2–4 independently focused pages, grouped pinning,
Favorites, moving, renaming, archive/restore and auto-archiving. Pane order,
axis, focus and weights survive sessions. CEF renders the pages together and
keeps loaded views through focus and orientation changes.

The installed Arc 1.169.0 was inspected on 2026-10-23 in a separate Blank
Window. Add Split View immediately creates an empty pane with the ordinary
Command Bar centered inside that pane; Escape cancels it. Cmd+W closes only
the focused pane. The sidebar displays one segmented row, with one target per
pane, and the page headers have centered addresses without a shared title bar.
These flows now have GPUI regression coverage and real-app CEF checks. See
`docs/ARC_REFERENCE.md` for observed states and the visual parity rule.

| Feature | Shortcut | Status | What's missing |
|---|---|---|---|
| Add a pane (up to 4 panes, "Cannot Add New Pane" toast) | ⌃⇧= | partial | Immediately shows an empty pane and the ordinary URL/search input centered inside it. Escape, outside dismissal and Cmd+W cancel without archiving the source; drafts are excluded from saved sessions. Existing tabs, history and new URLs are supported. Exact menu glyphs and full theme/animation fidelity remain |
| Close a pane | ⌘W / ⌃⇧- | done | Closes only the focused pane from GPUI and native page focus; the other pages stay loaded. Sidebar group close still closes the entire item |
| Split pane corner contour | — | done | Each pane has one rounded outer contour shared by its GPUI header and native CEF page. The header/page seam is square; only the page's bottom corners are rounded. Leaving split restores all page corners without reloading. The inter-pane gap exposes the window theme |
| Split sidebar item | — | partial | Equal-width pane segments fill the row without a reserved close-button gutter. Long titles truncate inside each segment; hover reveals the group close action over the last segment without resizing cells. Independent pane focus targets, favicons and custom group names are retained. Exact Favorite-group treatment and remaining visual states need comparison |
| Split from a Pinned tab or Favorite | — | done | Creates independent copies in Today and retains the original pin/Favorite, including its URL, placement and identity. Selecting an existing protected tab for a pane also copies it |
| Create by dragging a tab to the middle of the window | drag | missing | — |
| Horizontal/vertical layout, Move Left/Right/Up/Down | — | done | Command Bar actions preserve loaded page views |
| Separate Page / Separate All Tabs / Expand Current Split | — | partial | Separate Page and Separate All Tabs are implemented; expanding a pane is missing |
| Focus pane N / next / previous | (?) | done | Command Bar actions and clickable GPUI pane headers |
| Split View title ("Split view title text field placeholder") | — | done | Shared sidebar item title; rename/reset and archive search preserve it |
| Share Split View (link) | — | missing | Arc service |

## 9. Little Arc

| Feature | Shortcut | Status | What's missing |
|---|---|---|---|
| Small window for quick lookups | ⌥⌘N | missing | — |
| Global shortcut "Open Little Arc when I press … in any app" | configurable | missing | — |
| Links from other apps open in Little Arc | — | missing | — |
| ⌥⌘-click on a link opens it in Little Arc | — | missing | — |
| Toolbar: Open in (Space, "More Spaces", "Search Spaces…"), Change profile, Extensions, Copy URL, downloads | ⌘O | missing | — |
| Show/Close all Little Arc windows | — | missing | — |
| Tip for new users "Links Open in Little Arc" | — | missing | — |

## 10. Peek

| Feature | Status | What's missing |
|---|---|---|
| Links to other sites from Pinned/Favorites open in Peek (option) | missing | — |
| Shift-click opens a link in Peek (option) | missing | — |
| Closing Peek (⌘Esc / Esc), Undo "Close Peek" | missing | — |
| Moving Peek into a tab or a Space ("Open Little Arc or Peek in…" ⌘O) | missing | — |
| Swipe to dismiss the modal (`modal-v2-native-gestures`) (?) | missing | — |

## 11. Air Traffic Control (link routing)

| Feature | Status | What's missing |
|---|---|---|
| "New Route" rules: URL or domain "Is equal to" … → Space / Most Recent Space / Little Arc | missing | — |
| Access from Links → "Air Traffic Control…" and from the Command Bar | missing | — |
| Ways to open a link: new tab (focused or not), Split View, Peek, new window, Little Arc | partial | Popups (`window.open`, target=_blank) open as a new tab. No other modes |

## 12. Profiles

| Feature | Status | What's missing |
|---|---|---|
| Profile: separate history, cookies, passwords, extensions | missing | The engine uses a single `Default` cache |
| Creating a profile: name and which Spaces use it | missing | — |
| Changing a Space's profile, with warnings ("Switch to your … Profile?") | missing | — |
| Deleting a profile by typing its name, "Delete unused Profiles". The default profile can't be deleted | missing | — |
| Per-profile settings: search engine, search suggestions, auto-archiving, downloads folder, document app, clearing data | missing | — |
| Profiles don't sync between computers (same in Arc) | — | Reference |

## 13. Settings (Preferences, ⌘,)

Tabs: General, Profiles, Links, Shortcuts, Max, Advanced, Account (sync),
Icon. There is also an internal Staff tab. We have no settings window;
rows marked partial expose the setting from the Command Bar instead.

| Tab → option | Status |
|---|---|
| General → Arc is not the default browser ("Set as default" button) | missing |
| General → "Profile Settings…" (search engine, auto-archive, downloads) | missing |
| General → "Archive tabs after" | partial (available from Command Bar; settings window remains missing) |
| General → Built-in blocker: Block Ads, Block Trackers, Block Cookie Banners, "Ad Block Settings…" (Advanced Ad Block Settings), advice to keep a single blocker | missing |
| General → "Previews Settings…" (Show Arc Previews: Gmail, Calendar, Outlook, Linear/Notion/Figma) | missing |
| General → Warn before quitting | missing |
| General → Automatically update my Arc | missing |
| General → Export Data (Spaces and tabs as Markdown, "Arc Data Export.md") | missing |
| Profiles → profile list, creating and deleting, Default Search Engine, Include search engine suggestions, Search Settings…, auto-archive, Download Location, Default Document App, Privacy and Security → Clear Browsing Data | missing |
| Links → Air Traffic Control…, Peek (2 options), Little Arc (shortcut, links from other apps, ⌥⌘-click, Archive Little Arcs after) | missing |
| Shortcuts → search ("Type a feature name or shortcut"), Default / Customized Shortcuts, recording a custom shortcut, conflicts ("is already in use by …", Remove Conflicting Shortcut), per-shortcut priority (Arc wins / Website wins / first press to the site, second to Arc), Reset Shortcut / Reset All, Extension Shortcuts…, "Include Favorites in ordering", "Learn the Essential Shortcuts" | missing |
| Max → Ask on Page (⌘F), Tidy Tab Titles, Tidy Downloads, ChatGPT in the Command Bar, 5-Second Previews, Automatically enable future Max features | missing (AI, lower priority) |
| Advanced → More Settings… (arc://settings), Reset Translated-site Prompts, Enable Boosts, Allow websites to get your theme data, Picture in Picture when leaving a tab with video, dragging the window by the top of the page, Shared Quotes, Show full URL when Toolbar is enabled, restoring windows on launch, Haptic feedback when reordering, Play Arc sound effects | missing |
| Account → Arc Sync (E2E), Recovery Card and recovery phrase, changing name, email and password, Delete Account, deleting sync data, legacy iCloud Sync | missing |
| Icon → alternative app icons (unlocked by referrals) | missing (we don't copy assets) |

## 14. Library (⇧⌘L)

| Section | Status | What's missing |
|---|---|---|
| Spaces (Space Manager: order of Spaces, folders, tabs) | missing | — |
| Archive (see section 7) | partial | Available in the Command Bar; dedicated Library page remains missing |
| Downloads (⇧⌘J): filter, filter by profile, "download flies into Library" animation, "File deleted or moved" | missing | — |
| Media: Captures and files from Desktop/Documents/Downloads, Open in Preview, Show in Finder, Hide from Arc, Copy | missing | — |
| Boosts: list, on/off, rename, delete | missing | — |
| Easels & Notes | — | Out of scope (Easels). Notes are low priority |

## 15. Boosts

| Feature | Status |
|---|---|
| Editor: color (color wheel, Brightness, Original Saturation, Invert lightness, Advanced color controls), font, text size and case | missing |
| Zap: pick an element to hide (Pick Selector, "Zap all related elements") | missing |
| Custom CSS/JS ("Refresh to Run"); Boosts with JS can't be shared | missing |
| Turning on and off per site, "Customize Page…" in the context menu, Boosts section in the Site Control Center | missing |
| Sharing and installing someone else's Boost ("Add this Boost to Arc?"), Boost Gallery | missing (Arc service) |

## 16. Page, top bar and Site Control Center

| Feature | Status | What's missing |
|---|---|---|
| The page sits in an inset "card" with rounded corners | done | — |
| Loading indicator (glowing progress, `GlowProgressLoadingIndicator`) | missing | The engine reports loading, but it isn't shown |
| Site Control Center: security icon, Copy URL, Capture / Full Page / Portrait, Share, Boosts, extensions (pin/unpin, Add Extension), Clear Cache, Clear Cookies, All Site Settings… | missing | — |
| Status bar with the URL when hovering a link (`StatusTextView`) | missing | — |
| Find in Page ("No results found"), Find and Replace | partial | Small GPUI panel above CEF, live search, result counts, next/previous, close and focus restoration. Find and Replace remains missing |
| Page translation: "Translate this page to %@?", Use Original, Choose Another Language, Never Translate This Site | missing | — |
| Crashed tab page (Sad Tab) | missing | — |
| Swipe back/forward on the page (overscroll) | missing (?) | Not checked whether the gesture works in CEF |
| Dragging the window by the top of the page (option) | missing | — |
| Pointer lock: "Your cursor is hidden. Press 'esc'…" toast | missing | — |
| Page full screen: "To exit full screen, press …" toast | missing | — |
| Developer mode for a site, DevTools | missing | — |
| Chrome extensions (icons, "Open pinned extensions picker") | missing | — |
| Built-in ad and tracker blocker | missing | — |
| Quietly creating a tab while the sidebar is hidden ("New Tab Created in %@") | missing | — |

## 17. Media and Picture-in-Picture

| Feature | Status | What's missing |
|---|---|---|
| Mini player in the sidebar: Back to Tab, Open Picture in Picture, Previous/Next Track, Adjust Tab Volume, Toggle Microphone/Camera, Close Mini Player | missing | — |
| Automatic PiP when leaving a tab with video (option) | missing | — |
| PiP window: Back to Tab, ±15 seconds, seeking by dragging up, Keep Window on Top, minimize or close, remembered position | missing | — |
| PiP for Google Meet (with chat) and Document PiP | missing | — |
| Mute/Unmute a tab, Mute All / Unmute All | done | CEF tab audio control through Command Bar, Tabs menu and tab context menus. Persists per tab and applies before a restored page is shown; Mute All preserves unloaded tabs without loading them |

## 18. Notifications and toasts

The main toasts from the `Toasts` module. Passive GPUI child-window toasts
are implemented for copied links; the remaining notifications follow below.

| Toast | Status |
|---|---|
| Undo for actions ("“%@”. ⌘Z to Undo.") | missing |
| Copied Current URL / clean link without trackers / Copied item links / as quote | partial (URL, clean URL and Markdown done; multi-item links and quotes remain) |
| Pinned Tab to “%@” / Un-Pinned Tab from “%@” | missing |
| Auto archived %d tabs; Tab/Folder restored to … | missing |
| New Tab Created in %@ (with the sidebar hidden); New Space Created | missing |
| Cannot Add New Pane (Split View) | done |
| Import: Now importing… / Success / partial / failure | missing |
| Translation: Translated to %@ / back to %@ / error | missing |
| Updates: update pending / failed / in progress | missing |
| Battery Saver Mode Activated/Deactivated | missing |
| Sync errors (password change, outdated device etc.) | missing |
| Shortcut tips ("Copy links faster by pressing…", "… to easily show and hide your Sidebar!", Site Search) | missing |
| Screenshot Copied to Clipboard, Capture in Progress… | missing |

## 19. Onboarding and import

| Feature | Status | What's missing |
|---|---|---|
| Signing in and signing up for an Arc account | missing | Out of parity (Arc service). Our equivalent is a local profile |
| Import from Chrome-like browsers, Safari, Firefox: history, cookies, passwords (Passwords.csv), autofill, bookmarks with folder selection | missing | — |
| Choosing the profile to import into ("My Existing Arc Profile" / "Create a New Profile") | missing | — |
| Steps: theme ("Add some flair with a theme."), Favorites ("Choose the apps you use most."), ad blocker, Arc Max, default browser, member card | missing | — |
| "Let’s settle in! Here are the basics." screen (Pin tabs, Split View, Spaces, Open your first tab, with video) | missing | — |
| "Essential Keyboard Shortcuts" window (⌘T, ⌃Tab, ⇧⌘C, ⌃⇧=, ⌥⌘N) | missing | — |
| Empty state: "Open your first tab. Click New Tab or press ⌘T…" | partial | We show a "No tab open / Press ⌘T to open a new tab" placeholder |
| Default browser prompts ("Try Arc as default for one week" etc.) | missing | — |

## 20. Sync

| Feature | Status | What's missing |
|---|---|---|
| Saving the session between launches (Spaces, tabs, archive) | done | `session.json` in the profile folder; demo data only on the first launch. URLs are saved, back/forward history isn't |
| "When opening Arc, restore windows from previous session" | missing | — |
| Arc Sync: syncing the sidebar (Spaces, Pinned, folders) between devices with end-to-end encryption and a recovery phrase | missing | — |
| Synced settings (`SyncedPreferences`) | missing | — |
| Cookies persist between launches | done | `persist_session_cookies` in the engine |

## 21. Gestures and feel

| Feature | Status | What's missing |
|---|---|---|
| Sidebar swipe: Spaces follow the fingers, resistance at the edges, flick by speed, snapping back after a short swipe | done | — |
| Animated Space switching from the keyboard | done | — |
| Hidden sidebar slides in from the edge and hides after a delay | done | — |
| Swipe back/forward on the page | missing (?) | — |
| Animated tab dragging, dragging to the middle for Split | missing | — |
| Haptic feedback and Arc's sounds (`ARC_SoundEffects`) | missing | — |
| Springy click on sidebar items | missing | — |

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
| Arc Max (AI: Ask on Page, Tidy Titles and Downloads, 5-Second Previews, ChatGPT) | missing | Lower priority than core parity |

---

## Summary

Counted from the table rows above, including rows marked (?). Reference
rows without a status aren't counted.

- **done**: 49
- **partial**: 31
- **missing**: 215

### Biggest gaps (by importance for parity)

1. **Command Bar as the control center.** Needs actions by name (Type
   Travel) for remaining menu items, history, search suggestions and Site
   Search with Tab. Basic actions, Space switching and tabs from all Spaces
   are implemented.
2. **Archive.** Search, Space/reason filters, local date groups, older entries,
   restore/delete/clear and interval selection work in the Command Bar.
   Needs the dedicated Library page and Little Arc entries.
3. **Sessions.** Back/forward history of tabs isn't saved.
4. **Favorites, folders, drag and drop and sidebar context menus.**
5. **Split View** (⌃⇧=, dragging to the middle).
6. **Peek, Little Arc (⌥⌘N) and Air Traffic Control.**
7. **Managing Spaces and the theme editor.** Named Spaces can be created
   and renamed. Needs icons, deleting, plus multi-color gradients, color sets, noise,
   Intensity and Light/Dark.
8. **Profiles.** Currently one cache for everything.
9. **Settings window**, including the shortcut editor with the "Arc or
   website" priority.
10. **Shortcuts.** Missing ⇧⌘L.
11. **Basic browser features:** downloads,
    DevTools, print, loading indicator, new and incognito windows.
12. **Media controls and PiP, toasts with Undo, onboarding and import.**
