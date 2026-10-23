# Controls audit: broken or non-Arc behavior

Audit of every clickable control and menu item in our UI (as of
2026-10-23, commit after `9832635`), compared with Arc 1.169.0
(`/Applications/Arc.app`). Each row is a fix task. Main-menu and
shortcut gaps are tracked separately in `docs/ARC_PARITY.md`.

Context for whoever picks this up:

- The UI is GPUI (Rust), `crates/browser/src`. Menus are built in
  `tab_menu.rs` (tabs, Favorites, Split View, address bar),
  `space_menu.rs`, `folders.rs`, `new_menu.rs` (dock "+"),
  `site_controls.rs`; the sidebar and Command Bar live in `window.rs`.
- Menu rows created with `pending(...)` are drawn but have no command.
- Many flows reuse the Command Bar as a generic text prompt
  (`open_command_bar(Target::...)`). Arc uses dedicated in-place UI for
  them; those are listed under "Works differently".
- Rules: match Arc 1:1 (look, behavior, animation), cover every UI
  interaction with a test in `crates/browser/src/tests.rs`, verify in the
  real app with `scripts/drive.sh`. See `AGENTS.md`.
- Rows marked **(check in Arc)** describe Arc behavior from memory; confirm
  against the installed app before implementing.

## 1. Works differently from Arc

| # | Control | Where (code) | Ours | Arc |
|---|---|---|---|---|
| 1 | Change Icon… (tab / Favorite / Pinned / Split View row) | `tab_menu.rs` `Command::Icon` → `Target::Icon` | Command Bar text prompt: "Enter one symbol or emoji" | Icon picker popover next to the item: grid of emoji and symbols, search, reset. The choice applies immediately |
| 2 | Change Space Icon… | `space_menu.rs` `Command::SpaceIcon` | Same Command Bar text prompt | Same icon picker, anchored to the Space header (check in Arc) |
| 3 | Change Folder Icon… | `folders.rs` `FolderIcon` | Same Command Bar text prompt | Same icon picker, anchored to the folder row (check in Arc) |
| 4 | Rename… (Favorite / Split View) | `tab_menu.rs` `Command::Rename`, `window.rs` and `split.rs` double-click handlers | Favorites and Split View groups still open the Command Bar. Today/Pinned single-page tabs now edit inline | Ordinary tab titles edit inline; Enter or blur commits, Esc cancels, and clearing restores the page title. Exact Favorite/Split View editor placement still needs direct verification |
| 5 | Rename Space… | `space_menu.rs` `SpaceRename` | Command Bar prompt | Inline edit of the name in the Space header (check in Arc) |
| 6 | Folder Rename… | `folders.rs` `FolderRename` | Command Bar prompt | Inline edit in the folder row |
| 7 | New Folder (dock "+", Space menu, "Move to › New Folder…", "New Nested Folder…") | `folders.rs` `open_folder_creator` | Command Bar asks for a name first, then creates | The folder appears immediately in the sidebar with its name field in edit mode (check in Arc) |
| 8 | New Space (dock "+") | `new_menu.rs` `Item::Space` → `Target::NewSpace` | Command Bar "Name your Space…" | Space creation flow in the sidebar: name, icon, theme color, profile (check in Arc) |
| 9 | Move to › Folder… (tab) | `tab_menu.rs` `Command::MoveToFolder` | Opens a Command Bar folder picker | The "Move to" submenu itself lists destinations (Spaces, their folders, Favorites) (check in Arc) |
| 10 | Folder › Move to… | `folders.rs` `FolderMove` | Command Bar picker | Submenu of destinations, like row 9 (check in Arc) |
| 11 | Share › (tab, address bar, Split pane options) | `tab_menu.rs` `pending("Share")` | Submenu holds only "Copy Link as Markdown" | System share options (Mail, Messages, AirDrop, etc.) plus copy variants (check in Arc) |
| 12 | Site controls → Share tile | `site_controls.rs` `site-share` | Copies the URL | Opens the share menu (check in Arc) |
| 13 | View Archive (Command Bar action) | `archive.rs` | Archive is a Command Bar list | Archive is a Library page; see `ARC_PARITY.md` |
| 14 | Archive tabs after… (Command Bar action) | `Target::ArchiveInterval` | Command Bar list | A setting in Settings → profile (no Settings window yet) |
| 15 | Command Bar appearance | `window.rs` `render_command_bar` | Always dark | Follows the system light/dark appearance |
| 16 | Command Bar rows | `window.rs` `render_command_bar` | No audio indicator; split tabs say "Switch to Tab" | Speaker icon on playing tabs; split tabs say "Switch to Split View" with both favicons |

## 2. Present but does nothing

| # | Control | Where (code) | Expected (Arc) |
|---|---|---|---|
| 17 | Library button (Space bar, bottom left) | `window.rs` `corner("library")` | Opens the Library (Archive, Downloads, Media, Spaces, Boosts) |
| 18 | Tidy (Today header, on hover) | `window.rs` `tidy-today` (no `on_click`) | Groups/renames Today tabs (Arc Max "Tidy Tabs") |
| 19 | New Boost (dock "+") | `new_menu.rs` `Item::Boost` | Opens the Boost editor for the current site |
| 20 | Command Bar ⓘ button | `window.rs` `render_command_bar` | Opens the Command Bar tips/help (check in Arc) |
| 21 | Site controls → Boost tile | `site_controls.rs` `site-boost` | Boost for the site |
| 22 | Site controls → Capture tile | `site_controls.rs` `site-capture` | Area capture |
| 23 | Site controls → Screenshot tile | `site_controls.rs` `site-screenshot` | Full-page screenshot |
| 24 | Site controls → Extensions "+" | `site_controls.rs` `site-add-extension` | Opens the extension store / list |
| 25 | Site controls → Secure / Not Secure | `site_controls.rs` `site-security` | Connection and certificate details, site permissions |
| 26 | Site controls → "…" | `site_controls.rs` `site-more` | Site settings menu (permissions, cookies, clear site data) |
| 27 | Address bar menu → Capture… | `tab_menu.rs` `MenuTarget::Address` | Area capture |
| 28 | Address bar menu → Capture Full Page | same | Full-page capture |
| 29 | Address bar menu → Show as Toolbar | same | Shows a classic URL toolbar above the page |
| 30 | Address bar menu → Disable Shared Quotes | same | Turns off Arc's shared-quote highlights |
| 31 | Favorite menu → Show Unread Badge (always checked) | `tab_menu.rs` | Toggles the unread badge on the Favorite |
| 32 | Tab menu → Customize Page… | `tab_menu.rs` | Opens the Boost editor for the page |
| 33 | Split View menu → Share Split View… | `tab_menu.rs` | Shares the split as a link |
| 34 | Space menu → Edit Theme Color… | `space_menu.rs` | Space theme editor (color picker, gradient, noise) |
| 35 | Space menu → Set Profile › (only "Default", checked) | `space_menu.rs` | Lists real profiles; assigns the Space to one |
| 36 | Space menu → Live Folders › GitHub Pull Requests / GitHub Issues | `space_menu.rs` | Creates an auto-updating Live Folder |
| 37 | Space menu → Share Space… | `space_menu.rs` | Shares the Space as a link |
| 38 | Space menu → Export "…" into Folder › (Space list) | `space_menu.rs` | Moves this Space's items into a folder of the chosen Space |
| 39 | Space menu → Manage Spaces… | `space_menu.rs` | Opens Library → Spaces |
| 40 | Space menu → Delete Space | `space_menu.rs` | Confirms, then deletes the Space |

## 3. Verify against Arc (may differ)

| # | Control | Note |
|---|---|---|
| 41 | Clear (Today header) | Ours archives Today tabs at once. Check whether Arc animates the sweep and offers an undo toast |
| 42 | Split pane header buttons (back, forward, reload, address, menu, close) | Compare hover states and the menu contents with Arc's pane header |
