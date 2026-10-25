# Command Bar and Sidebar review — Arc 1.170

This review accompanies the `command-sidebar` changes. Ordinary visual references use an owned window in the shared **Echo Test** Space at `{100,100,1000,760}`. Images are normalized to sRGB for pair inspection. No owner Space was entered, no Space was created/deleted, and appearance/owned windows were restored.

Evidence lives under `target/parity/command-sidebar/`. A fresh observation is distinguished from retained earlier evidence below. Existing implementation or a test passing alone does not establish visual parity. Rows without a successful positive source reference remain partial. Live Folders arrived on main during this work and retains its own upstream evidence.

| Row | Reference/check | Remaining scope / decision |
|---|---|---|
| Vertical sidebar with tabs | Fresh Light/Dark pairs: `pairs/sidebar-*`. | **done** — Done for ordinary row geometry; specialized states stay separate. |
| Resize sidebar | Existing native retention checks; no new drag reference in this pass. | **partial** — Hit area, snapping, persistence, new-window inheritance and other platforms. |
| "Show as Toolbar" mode (context menu: Show as Toolbar / Hide Toolbar) | Fresh `arc_sidebar_toolbar_Light.png`; retained native Echo toolbar baseline. | **partial** — Theme/material and mode-switch motion. Share/Capture routes already exist; they are not missing controls. |
| **Favorites** (Top Apps): icon grid above Spaces, shared by all Spaces of a profile | Fresh populated dock in `pairs/sidebar-*`; ordinary tiles corrected to 47 px. | **partial** — Dismissed-card drag states, authenticated unread/calendar content, theme and profile qualification. |
| **Pinned** section of a Space | Fresh Light/Dark `pairs/pinned-*`; native save/unload/reopen and invalid/cancel checks. | **partial** — Opening/closing motion and small shadow/keycap raster differences. |
| "Tidy Tab Titles": automatic shorter names on Pin | Earlier positive single-page reference retained; no new provider result in this pass. | **partial** — Split/Favorite transitions and feedback. No batch changes made to other agents’ tabs. |
| "Tidy Tabs": AI grouping of Today by topic | Earlier guarded local/public fixtures returned no Arc grouping. | **partial** — Successful source grouping, header menu and feedback. Shared tabs were not regrouped. |
| Folders: nested, collapse/expand, turn a folder into a Space, folder from selected tabs | Existing 308 px menu/nesting pairs retained; no conversion run in this pass. | **partial** — Sibling insertion, previews and folder multi-selection. No extra test Space created. |
| Live Folders (GitHub Pull Requests: Created by Me, Review Requests…) | Upstream row added during rebase; `target/parity/live-folders` remains the qualified evidence. | **partial** — Authenticated PR contents, badges and SSO reference; local-only connection adaptation retained. |
| Dragging tabs (between sections, into folders, into Favorites, into another Space) | Existing drag/drop and retained-page checks; not newly replayed. | **partial** — Edge auto-scroll and post-dismissal native drag visuals. |
| Haptic feedback when reordering tabs | Existing preference/typed-boundary coverage retained. | **partial** — Physical strength/timing cannot be established by screenshots; no owner intervention requested. |
| Multi-selecting tabs (⌘/⇧-click) | Existing ordinary-row interaction coverage; not newly replayed. | **partial** — Favorite/folder selection and dismissal states. |
| Renaming a tab by double-click ("Double-click to rename") | Fresh ordinary inline field and `arc_split_rename_Dark.png`. | **partial** — Arc uses a 452 × 224 px Rename Tab… dialog for a split; Echo’s group/Favorite editor placement still differs. |
| Custom tab icon ("Customize Tab") | Existing graphical-picker baseline retained; no new upload reference. | **partial** — Uploaded images, out-of-window positioning and native popup motion. |
| Audio indicator, Mute/Unmute in the context menu | Fresh `arc_audio_tab_Dark.png` and background/player source captures. | **partial** — Favorite placement, tooltips and complete theme qualification. |
| Camera/microphone indicator on a tab | Earlier guarded virtual-camera check did not expose a positive Arc timing reference. | **partial** — Activation motion/timing and Favorite/Split placement. |
| Split View item in the sidebar (tab group) | Fresh `arc_split_sidebar_Dark.png` and rename reference. | **partial** — Favorite group treatment, rename placement and full theme/hover states. |
| Sidebar interaction states and Space dock | Fresh Today/Pinned idle/hover/collapse source captures; ordinary pairs. | **partial** — Drag, playback and reorder animation beyond ordinary resting rows. |
| Browser chrome iconography | Fresh dock menu and Duplicate/Rename/Shortcuts action glyph references. | **partial** — Remaining action-specific glyphs and raster fidelity; no blanket icon completion. |
| "+" button in the dock: New Tab, New Space, New Folder, New Split, New Boost | Fresh Light/Dark `pairs/new-menu-*`: Arc 210 × 290, Echo 210 × 250 excluding Easel. | **partial** — Small glyph raster differences, popover translucency and motion. |
| "Springy" click animation on sidebar items | Earlier ordinary-row recordings retained (212 → 210 → 212 px). | **partial** — Favorite/group presses and reorder settling. |
| Incognito window sidebar ("Incognito Tabs") | Existing private baseline retained; no extra Arc root opened in this pass. | **partial** — Private menu/context states and full visual qualification. |
| Banners at the bottom of the sidebar: update, "We cleaned up!" after auto-archiving, speed-up (several ad blockers) | Earlier resource/eligibility audit retained; no live education banner available. | **partial** — Positive local education and blocker layouts; update/account banners excluded. |
| Media controls in the sidebar | Fresh source background-audio/player captures; prior retained native transport checks. | **partial** — Remaining section 17 transport, glyph and theme qualification. |
| Popup over the page | Fresh typed Light/Dark pairs in `pairs/typing-*`. | **partial** — Small Light tint difference, neutral/black Echo info button versus Arc’s white-shadowed/blue-tinted treatment, unpicked palettes and other-platform fallback. |
| Switching to an open tab | Fresh `pairs/typing-switch_selected-*` and multiword GPUI regressions. | **partial** — Group thumbnails/icons and native cross-Space behavior; owner Spaces were not visited. |
| Search engine suggestions (Chromium: Search Suggestion, Navigate Suggest) | Fresh `pairs/notice-*`; native Settings/Dismiss route checks. | **partial** — Own policy OS opening, three completion transports and Navigate Suggest ranking. |
| History and bookmarks in suggestions (Chromium: History / Bookmark / Search History) | Fresh rust language source result; multiword and cached-favicon fix. | **partial** — Cmd+L bookmark behavior, folder results and full ranking. |
| Calculator (Search Calculator Answer) | Fresh 2+2 in both Arc modes produced no answer row. | **partial** — Echo’s local answer is still a divergence; positive source answer/confirmation reference required. |
| Popular domains (Popular Domains, ARC_TopDomains) | Fresh `pairs/typing-domain-*`; prefix/search-row behavior tested. | **partial** — Full domain catalog and source ranking. |
| Site Search: Tab or Space after a site's name searches that site; has a management menu | Fresh `pairs/typing-site-*` and scoped pair. | **partial** — Automatic discovery/catalog and Search Settings Light body palette. |
| Removing a suggestion ("Delete Suggestion") | Existing local-history deletion coverage retained; no new remote-removal reference. | **partial** — Remote engine deletion is not implemented. |
| **Actions (Type Travel)**: every menu item is available by name and synonyms (`arcTypeTravelTriggers` in the nib) | Fresh Duplicate/Rename/Mute/Shortcuts and About/Import/Default queries. | **partial** — About and Import still lack Echo Command Bar routes; the installed Arc does not offer a Default Browser action for either “default browser” or “set as default browser”, so that earlier positive-reference claim was incorrect; the complete glyph/state matrix also remains incomplete. |
| Tabs: Duplicate Current Tab, Rename Current Tab…, Reset Tab, Replace Pin with Current Page, Mute/Unmute All Tabs | Fresh Duplicate/Rename/Mute queries and ordinary inline rename source. | **partial** — Group/Favorite rename placement and complete Reset/Mute result glyph/status fidelity. |
| Other: Toggle Sidebar, Manage Passwords, Search Settings, Air Traffic Control, Edit Keyboard Shortcuts, Check for Updates, Undo/Redo | Fresh Undo/Redo appearance round trip and Shortcuts source; native Echo pairs. | **partial** — All local routes now exist; empty-Dark action fill is Arc #4248d8 / Echo #464ddc, arrow raster differs, and the remaining action-specific glyph/status matrix is not fully paired. |
| Suggestions for an empty field (Air Traffic Control, Tips for Organizing, Help Center) | Fresh empty source and `pairs/empty-mru`; native recency correction. | **partial** — Full ranking, help-row eligibility/destination and Max suggestions. No guessed Contact the Team route. |
| ChatGPT in the Command Bar, "Browse for Me" (Arc Max) | Existing keyword/website handoff reference retained; no new positive Browse for Me layout. | **partial** — Installed 1.170 exposes no action; no replacement was invented. |
| Instant Links (Arc Max) | Existing loopback-provider native evidence retained; no new positive live Arc result. | **partial** — Source ranking, AI glyph/motion and live provider capability/billing. |

## Restoration

A title-based cleanup archived an existing shared test fixture. The exact item UUID `0A4AA323-B27D-41C9-A65C-6DD4293AB5AE` was restored through Archive after verifying its URL, original Echo Test Space and recency. A later persisted read confirms the original UUID under Echo Test Today and absent from Archive. Evidence: `target/parity/command-sidebar/source-sessions/command-sidebar-restore-detail-exact/restored-target.json`. No further reference page reused that title. Subsequent reference checks used no new pages where possible.

## Validation

Behavioral regressions cover completion-prefix navigation, nonadjacent multiword matches, profile-controlled site hints, pinned-URL editing and renderer retention, Shortcuts routing, disclosure settings/dismissal, empty-field MRU and Undo/Redo routing with editable-page precedence. Core coverage rejects stale appearance Undo without restoring unrelated data. Native Light/Dark sessions use disposable Echo profiles and the portable bundled CEF distribution. Full workspace tests run before each push and again under the push lock.


## Round 2

`target/parity/command-sidebar-r2/` contains the fresh drag reference, edge probes and native Echo captures. [Drag measurements](SIDEBAR_DRAG_SCROLL.md) document the common viewport and sustained-scroll calibration. The row remains partial for the ghost, edge arrow/scroll thumb, full speed curve and remaining drag states. The fresh Dark Favorite-rename dialog crop and Type Travel queries are retained separately from these changes. The first Light rename capture preceded the asynchronous dialog and is not a positive visual reference.
