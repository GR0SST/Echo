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
| Expand/Collapse Pinned Section | hidden | done | Click the Pinned header or use the Command Bar; state persists per Space |
| Reveal Current Tab | ⌃Space (hidden) | missing | — |
| Stop Loading | ⌘. | done | CEF stop_load; also available in the Command Bar |
| Refresh the Page / Force Refresh | ⌘R / ⇧⌘R | done | Reload and reload_ignore_cache through CEF |
| Clear Cookies and Refresh / Clear Cache and Refresh | — | missing | — |
| Add Split View | ⌃⇧= | missing | See "Split View" |
| Close this Split Pane | ⌃⇧- | missing | — |
| Separate Page from Split View / Expand Current Split | — | missing | — |
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
| New Folder… | ⇧⌘N per nib (?) | missing | The nib gives it the same shortcut as Incognito; the real binding is set in code |
| Duplicate / Rename Current Item / Collapse All Folders / Expand All Folders | hidden | partial | Rename Current Tab is implemented; duplicate and folder actions remain missing |
| Next Tab / Previous Tab | ⌥⌘↓ / ⌥⌘↑ | done | — |
| Tab Switcher forward/back ("Recent Items" overlay, MRU, like ⌘Tab) | ⌃Tab / ⌃⇧Tab | partial | Our ⌃Tab steps through tabs in sidebar order. No overlay, no most-recently-used order |
| Cycle Tab Forward/Backward | ⌃⌘→ / ⌃⌘← (hidden) | missing | — |
| Go to Sidebar Item 1–8 / last item (with or without Favorites) | ⌘1…⌘9 (?) | partial | Pinned, then Today. No Favorites and no "Include Favorites in ordering" option |
| Switch to Split View 1–4 / next / previous | (?) | missing | — |
| Reveal Tab in Sidebar | — | missing | — |
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

| Feature | Shortcut/gesture | Status | What's missing |
|---|---|---|---|
| Vertical sidebar with tabs | — | done | CEF site favicons are cached for unloaded/restored tabs, with letter fallback. Width can't be changed (?) |
| Window buttons (traffic lights) inside the sidebar | — | done | — |
| Address field at the top of the sidebar (click opens the Command Bar with the URL) | — | done | No back/forward/reload buttons, no Site Control Center button |
| Hiding the sidebar and sliding it in when hovering the left edge | ⌘S | done | — |
| "Show as Toolbar" mode (context menu: Show as Toolbar / Hide Toolbar) | ⇧⌘D | missing | — |
| **Favorites** (Top Apps): icon grid above Spaces, shared by all Spaces of a profile | — | partial | Shared persistent dock (up to 12) with cached site favicons, click/keyboard activation and title/URL tooltips. Close unloads and resets the original URL; Favorite/Unfavorite and moves preserve the live page. Drag/drop, unread badges, calendar captions and profile isolation remain missing. Fresh visual check awaits an unlocked desktop |
| Hover previews for Favorites, Pinned and folders (Gmail, Google/Outlook Calendar, Linear, Notion, Figma, Confluence) | hover | missing | Configured in "Previews Settings…" |
| **Pinned** section of a Space | ⌘D | partial | Closing keeps the tab pinned and resets it to its pinned URL. Missing: "Edit Pinned Page" (custom URL). Reset to Original URL and Replace Pinned URL with Current are available in the Command Bar; section can be collapsed |
| **Today** section (unpinned) and the "+ New Tab" button | — | done | — |
| "Clear" button above Today (archives unpinned tabs) | ⇧⌘K | done | Click Clear above Today; preserves Pinned and other Spaces |
| "Tidy Tabs": AI grouping of Today by topic | — | missing | Arc Max feature, can wait |
| Folders: nested, collapse/expand, turn a folder into a Space, folder from selected tabs | ⇧⌘N (?) | missing | — |
| Live Folders (GitHub Pull Requests: Created by Me, Review Requests…) | — | missing | — |
| Dragging tabs (between sections, into folders, into Favorites, into another Space) | drag | missing | — |
| Haptic feedback when reordering tabs | — | missing | Option in Advanced |
| Multi-selecting tabs (⌘/⇧-click) | — | missing | — |
| Renaming a tab by double-click ("Double-click to rename") | double-click | done | Double-click or Rename Current Tab… in the Command Bar. Custom names persist through navigation, archive and sessions; Reset Tab Name restores the page title. Custom icons remain a separate missing feature |
| Custom tab icon ("Customize Tab") | — | missing | — |
| Close button on hover (× for Today, − for Pinned) | hover | done | — |
| Audio indicator, Mute/Unmute in the context menu | — | partial | Mute/Unmute works from the context menu; muted tabs/Favorites show a clickable Unmute indicator. Actual audio-playing detection remains missing |
| Camera/microphone indicator on a tab | — | missing | — |
| Split View item in the sidebar (tab group) | — | missing | — |
| Space header (name), Show/Hide Space Header | — | partial | The name is shown but can't be hidden |
| Spaces dock at the bottom (Space icons) | click | partial | We show colored dots; Arc shows Space icons or emoji |
| "+" button in the dock: New Tab, New Space, New Folder, New Split (right/bottom), New Note, New Boost | — | partial | Dock + creates a Space. The multi-action menu remains missing |
| "Springy" click animation on sidebar items | — | missing | — |
| Incognito window sidebar ("Incognito Tabs") | — | missing | — |
| Banners at the bottom of the sidebar: update, "We cleaned up!" after auto-archiving, speed-up (several ad blockers) | — | missing | — |
| Media controls in the sidebar | — | missing | See "Media and PiP" |

## 3. Spaces

| Feature | Shortcut/gesture | Status | What's missing |
|---|---|---|---|
| Several Spaces, each with its own Pinned/Today and active tab | — | done | — |
| Two-finger swipe on the sidebar: Spaces follow the fingers and rubber-band at the edges | gesture | done | — |
| Switching Spaces from the keyboard (animated) | ⌥⌘→/← | done | — |
| Creating a Space ("Create a Space" dialog: name, icon, theme, profile) | — | partial | Named Spaces can be created from the dock or Command Bar with an automatic accent. Icon/theme/profile controls remain missing |
| Renaming a Space | — | done | Name editor via the Command Bar; persisted with the same Space identity |
| Space icon (emoji or symbol, "Change Space Icon…", "Remove icon") | — | missing | — |
| Deleting a Space (with Undo) | — | missing | — |
| Reordering Spaces ("Move Space") | drag | missing | — |
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
| Switching to an open tab | — | partial | Lists matching tabs across all Spaces, identifies the destination Space and switches there. Split View remains missing |
| Search engine suggestions (Chromium: Search Suggestion, Navigate Suggest) | — | missing | Also needs the suggestions privacy screen |
| History and bookmarks in suggestions (Chromium: History / Bookmark / Search History) | — | partial | Local browsing history matches title/URL in ⌘T and ⌘L, with keyboard/click selection; open tabs match original page titles and suppress duplicate history rows. Bookmark/folder sources remain missing |
| Calculator (Search Calculator Answer) | — | missing | — |
| Popular domains (Popular Domains, ARC_TopDomains) | — | missing | — |
| Site Search: Tab or Space after a site's name searches that site; has a management menu | Tab / Space | missing | — |
| Removing a suggestion ("Delete Suggestion") | — | partial | × removes a local history suggestion and its saved visit without opening it; the query and keyboard selection remain usable. Remote search suggestions are not implemented |
| **Actions (Type Travel)**: every menu item is available by name and synonyms (`arcTypeTravelTriggers` in the nib) | — | partial | Implemented actions with names and synonyms: sidebar, pin/unpin, close/archive, clear Today, restore, reload/force reload, stop, zoom, Find in Page, back/forward, next/previous tab, window controls, Space/tab management and archive. Remaining menu actions follow their features |
| Space actions: "Focus on <space>", "Switch to …" | — | done | Matches Space names and synonyms; Enter or click switches with the pager animation |
| Moving a tab: "Move to Pinned in <space>", "Move to Today in <space>", "Move to Folder" | — | partial | Move to Pinned/Today in any Space and Pin to <space> are implemented. Identity, custom name and loaded page are preserved; destination is focused. Move to Folder remains missing |
| Favorites: "Favorite Tab", "Unfavorite Tab", "Pin to Top Apps" | — | done | Command Bar and Tabs menu; Unfavorite returns the tab to Today in the current Space |
| Split: Add Top/Bottom/Left/Right Split, Convert to Vertical/Horizontal | — | missing | — |
| Folders: Create New Folder, Collapse/Expand All Folders | — | missing | — |
| Tabs: Duplicate Current Tab, Rename Current Tab…, Reset Tab, Replace Pin with Current Page, Mute/Unmute All Tabs | — | partial | Rename Current Tab, Reset Tab Name and Mute/Unmute All are implemented; Reset Tab and Replace Pin with Current Page work for modified pins. Duplicate remains missing |
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
| **Tab**: Copy Link, Copy link as Markdown, Share, Duplicate, Rename (?), Edit Pinned Page → Replace Pinned URL with Current, Add/Open in Split View, Move to… (Spaces, profile), New Folder with…, Mute/Unmute, Archive Tab / Close Tab / Remove Pin / Remove Favorite, Archive Other Tabs / Tabs Below / Tabs Above, Customize Page… (Boost) | partial | GPUI right-click menus on tabs/Favorites: Copy Link/Markdown, Rename, Pin/Unpin, Favorite/Unfavorite, Reset/Replace Pinned URL, Mute/Unmute, Close/Archive. Up/Down/Enter/Escape and outside dismissal; actions target the clicked tab while preserving other active tabs. Remaining actions follow their missing features; fresh visual check awaits an unlocked desktop |
| **Multi-selection**: Archive Items, Copy links (as Markdown), New Folder with N items, Move to… | missing | — |
| **Folder**: Share, Duplicate, Archive Folder / Delete, New Nested Folder…, Turn into Space, Copy All Links (as Markdown) | missing | — |
| **Live Folder**: Pull Requests, Repositories, Single Sign-On, Delete Live Folder | missing | — |
| **Split View in the sidebar**: Share Split View…, Separate All Tabs, Add Split View, Convert to Vertical/Horizontal, Tab Options → Reset Name and Icon, Duplicate, Archive…, Separate from Split View | missing | — |
| **Split View pane** (control menu): Separate Tab from Split, Share Split View…, Convert orientation, Move Left/Right/Up/Down, Add Left/Right/Top/Bottom Split | missing | — |
| **Space**: Change Space Icon…, Rename, Edit Theme Color…, Set Profile, New Folder, Live Folders, Share, Show/Hide Space Header, Manage Spaces…, Delete Space, New Profile, Export into Folder | missing | — |
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
| Archive page in Library: "Today/Yesterday", filters by Space, "How was the tab closed?" (Auto-Closed/Manually Closed), Little Arc | partial | Command Bar archive browser searches title/URL, filters all/current Space, shows reason and relative age (8 results at a time). Library layout, calendar groups, reason filters and Little Arc remain missing |
| Restore ("Tab restored to …. Click to view."), Delete, Clear Archive | done | Restore a selected entry and activate its original Space with a toast; delete an entry with ×; clear all with confirmation |
| "Auto archived %d tabs" toast and the "We cleaned up!" banner | missing | — |

## 8. Split View

The persistent core model supports 2–4 pages as one sidebar item, including
grouped pinning, Favorites, moving, renaming, closing, archive/restore and
auto-archiving. Pane order, axis, focus and weights survive sessions. GPUI
controls and the creation picker are not wired yet. CEF now renders all panes
at once, lays them out horizontally or vertically and reports native focus;
loaded pages survive pane focus and orientation changes. Native surface geometry
is checked; page-click focus and fresh pixels await an unlocked desktop. The unit behavior
follows Arc's [2022 release notes](https://resources.arc.net/hc/en-us/articles/20498417809815-Arc-for-macOS-2022-Release-Notes);
the four-pane limit and preserving Pinned placement are confirmed in its
[2024–2026 release notes](https://resources.arc.net/hc/en-us/articles/20498293324823-Arc-for-macOS-2024-2026-Release-Notes).

| Feature | Shortcut | Status | What's missing |
|---|---|---|---|
| Add a pane (up to 4 panes, "Cannot Add New Pane" toast) | ⌃⇧= | partial | Persistent grouping, four-pane limit and native CEF pane layout are implemented; the picker and shortcuts remain missing |
| Close a pane | ⌃⇧- | missing | — |
| Create by dragging a tab to the middle of the window | drag | missing | — |
| Horizontal/vertical layout, Move Left/Right/Up/Down | — | missing | — |
| Separate Page / Separate All Tabs / Expand Current Split | — | missing | — |
| Focus pane N / next / previous | (?) | missing | — |
| Split View title ("Split view title text field placeholder") | — | missing | — |
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
| Cannot Add New Pane (Split View) | missing |
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
2. **Archive.** Search, Space filters, restore/delete/clear and interval
   selection work in the Command Bar. Needs the dedicated Library page,
   calendar groups, reason filters and Little Arc entries.
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
