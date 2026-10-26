# Functional release gaps

## Active code/test-only claims (2026-10-26)

- `t3-71384893` claims **#80 Little lookup retention during app/final-window quit**. Verifiable-by-tests: collect open lookup archives before canonical writers flush, exactly once, with promotion/empty/private exclusions and synthetic session files. Restore-destination policy and native quit qualification are excluded.

- `t3-71384893` claims **#65 held folder-drag auto-scroll**. Verifiable-by-tests: reuse the existing tab edge-scroll timer/physics for a validated folder payload, including hold, cancellation, Space/profile/workspace changes and drop cleanup. No new native gesture or visual policy.

- `t3-71384893` owns **#71 missing Check for Updates Command Bar route** (implemented and tested). Verifiable-by-tests: route to the existing app-owned update action/window, typed aliases, source retention and cancellation. No transport, installer or presentation changes.

- `t3-71384893` owns **#39 remote suggestion dismissal** (implemented and tested). Verifiable-by-tests: profile/provider-scoped persisted suppression, existing delete-control routing, stale callback guards and history isolation. Supported transport rows only; no provider-account deletion API or native/visual qualification.

- `t3-71384893` owns **#63 sidebar width inheritance** (implemented and tested): fresh normal-window requests capture the committed source width, while restored-window width is already persisted. Scope: request plumbing and GPUI/session tests, verifiable-by-tests; native drag/visual/platform acceptance excluded.

- `t3-71384893`: **#55 private-window action availability** implemented; **#81 moved/deleted completed downloads** implemented and tested. Scope: shared GPUI/core logic and automated tests only; no screen, Arc reference or native-window qualification. These parts are verifiable-by-tests by inspection; broader platform/visual acceptance stays partial.

Inventory baseline: `fcf128de1cabf4dbe4c6625bde9b071577c632f1`; reviewed 2026-10-25. All 208 rows whose status was `partial` or `missing` in [ARC_PARITY.md](ARC_PARITY.md) are classified below. Line numbers are baseline locators; feature names are the durable identifiers. No Arc sessions or pixel work were used for this triage.

**Counts: F 103, C 52, B 53; total 208.** Counts are inventory rows, not unique features. Duplicate umbrella/menu rows remain visible; implement the canonical feature once.

F = demonstrated missing capability, behavior difference, integration or a concrete functional acceptance gate. C = only appearance/motion/copy remains; defer for the first release. B = authentication/signing, a missing positive reference, or constrained acceptance prevents a defensible defect claim. B does **not** mean permission to build excluded account/sync/cloud features. Platform gaps are F when an adapter is known absent; mere unperformed platform acceptance is B. Mixed rows are F if any functional work remains.

Ranking prioritizes data retention, login/navigation, everyday retrieval and recoverability before secondary tools. S is a focused action/adapter/verification change, M crosses a few modules, L is a backend/platform project. Estimates exclude account access and all pixel polish. Not every inventory note is current: stale entries are verification tasks, explicitly identified, rather than assumed missing code.

## Ranked functional work

| Rank | Baseline row / feature | Effort | Concrete fix / acceptance |
|---:|---|:---:|---|
| 1 | L571: "When opening Arc, restore windows from previous session" | L | **Completed:** atomic per-window checkpoint, Space/tab/Split/folder/sidebar/frame/display restoration, lazy background pages, private exclusion and shared crash recovery. Native macOS relaunch/forced-stop/disabled-preference acceptance; see [window restoration](WINDOW_RESTORATION.md). Windows/Linux runtime qualification remains B. |
| 2 | L416: Profile: separate history, cookies, passwords, extensions | L | **Fixed sub-gap:** iframe autofill and profile-bound username account selection now pass native synthetic checks; parent profile row remains partial. See delivery evidence. |
| 3 | L192: Add Extension… / Manage Extensions… | L | Connect the qualified extension catalog/install/action engine bridge to Add/Manage Extensions. |
| 4 | L499: Built-in ad and tracker blocker | L | **Reclassified — not active Arc 1.170 desktop parity:** live Settings and Site Controls expose no built-in blocker. Dormant bundle resources are insufficient; user-installed extensions cover blocking. See reference verification below. |
| 5 | L293: History and bookmarks in suggestions (Chromium: History / Bookmark / Search History) | M | **Known frequency gap fixed:** per-profile totals persist, typed Today/history results use bounded frequency/age ranking, and Pinned/Favorites order plus empty MRU remain intact. Native six-vs-one open/closed cases match Arc; exact weights, cross-Space/bookmark rules remain B. See [ranking evidence](HISTORY_SUGGESTIONS.md). |
| 6 | L112: Undo / Redo (including sidebar actions: "Undo moving a space", "Close Peek" etc.) | M | **Regrouping fixed:** single-tab archive Redo closes its original target after a committed Split; the next Undo restores that Split without reloading its survivor. Pinned unload is verified to preserve move Undo and empty selection; no new unloading Undo is needed. Batch regrouping and Favorite unloading remain unqualified; see [Undo evidence](SIDEBAR_UNDO.md). |
| 7 | L99: Save Page As | S | **Fixed:** accepted format persists per local profile, initializes the picker/extension, survives native relaunch, and is unchanged by Cancel. Native NSEvent shortcut and File action pass; the physical System Events chord also fails in the saved Arc reference and remains unqualified. See [format evidence](SAVE_PAGE_FORMAT.md). |
| 8 | L129: Reveal Current Tab | S | **Fixed:** Control-Space now reaches the existing reveal/unfold/scroll action in both GPUI and renderer focus; see delivery evidence. |
| 9 | L410: Ways to open a link: new tab (focused or not), Split View, Peek, new window, Little Arc | M | Complete user link-disposition routing while preserving source pages, focus and profile storage. |
| 10 | L399: Shift-click opens a link in Peek (option) | M | **Fixed sub-gap:** ordinary/target-blank lookup navigation and live Peek source return retain the correct document/profile. Nested Shift-click remains unqualified; see [nested lookup evidence](NESTED_LOOKUP_NAVIGATION.md). |
| 11 | L599: Quiet downloads and the downloads tray | M | **Fixed sub-gap:** accepted native replacement and quiet keep-both pass file-byte, canonical-record and native-shutdown checks; staging/provenance authority is retained. See delivery evidence. |
| 12 | L440: General → Arc is not the default browser ("Set as default" button) | S | **Fixed:** General shows the conditional callout, uses the existing serialized registration interface, handles declined requests and refreshes OS status on activation. Native test-adapter acceptance preserves the owner’s real OS default; non-default Arc layout remains B. See [General evidence](DEFAULT_BROWSER_GENERAL.md). |
| 13 | L298: **Actions (Type Travel)**: every menu item is available by name and synonyms (`arcTypeTravelTriggers` in the nib) | S | **Sub-gap fixed:** About Echo, Import and conditionally available Set as Default Browser now route to existing handlers; the complete in-scope action catalogue remains partial. |
| 14 | L492: Crashed tab page (Sad Tab) | M | **Functional gap fixed:** retained-page failure/replacement state, scoped reload link and keyboard recovery, healthy Split isolation, popup-focus protection and native history-preserving recovery. Remaining illustration/layout work is C; see [recovery evidence](CRASHED_TABS.md). |
| 15 | L558: Import from Chrome-like browsers, Safari, Firefox: history, cookies, passwords (Passwords.csv), autofill, bookmarks with folder selection | L | **Export sub-gap implemented:** Safari ZIP bookmarks/history and standalone HTML and Firefox JSON/JSONLZ4 bookmark backups, including profile fallback; password ZIP upload uses the existing vault path. Cookies/autofill, encrypted source databases and opaque live sessions remain partial; see delivery evidence. |
| 16 | L183: Go Back / Go Forward | M | **Functional gap fixed:** ordinary-link and typed-address right-click menus select retained history IDs without reloading URLs or discarding Forward entries. Arc holds through 2.3 s only navigate on release; no hold menu is invented. Remaining visual/motion qualification is C and keeps the parity row partial; see [history evidence](NAVIGATION_HISTORY.md). |
| 17 | L143: Exit (leave a context: Peek, full screen) | S | Route Exit context to leave native fullscreen while retaining the observed Peek Command-Escape behavior. |
| 18 | L227: Folders: nested, collapse/expand, turn a folder into a Space, folder from selected tabs | M | **Ordinary mixed-selection gap fixed:** folder/tab range and toggle selection, recursive link copying, grouped creation with inline rename, same-profile moves and atomic Delete/Undo. Folder sibling insertion, hidden/collapsed-tree and compound-Split states stay F; ordinary menu/page dismissal and repeated ranges are now native-qualified. See [folder evidence](FOLDER_SELECTION.md). |
| 19 | L231: Multi-selecting tabs (⌘/⇧-click) | M | **Favorites verified compatible:** normal, Command and Shift clicks activate the Favorite in Arc and Echo; do not introduce Favorite multiselect. Folder/mixed-row selection and scoped batch actions are implemented; native counts, inline grouping, name commit, two-step Undo/Redo and no-reload acceptance pass. Discontiguous and repeated ranges (4 → 3 → 4 → 3), Escape and page dismissal are native-qualified. Hidden/collapsed-tree, cross-window deactivation and compound-Split routes stay partial. See [modifier evidence](FAVORITE_MODIFIERS.md) and [folder evidence](FOLDER_SELECTION.md). |
| 20 | L419: Deleting a profile by typing its name, "Delete unused Profiles". The default profile can't be deleted | M | **Storage sub-gap implemented:** persisted explicit retirements, loaded-context refusal, restart quarantine and background cache/vault/current + legacy credential reclamation. Existing exact-name single-profile deletion remains. Bulk UI is unverified in installed Arc; do not invent an entry. [Reclamation evidence](PROFILE_RECLAMATION.md). |
| 21 | L420: Per-profile settings: search engine, search suggestions, auto-archiving, downloads folder, document app, clearing data | M | **Creation action implemented:** the Command Bar opens the selected profile’s Confluence/Google/Notion/Word creation URL, with Google as Arc’s — fallback; stale profile/preference actions are rejected. [Reference](DOCUMENT_CREATION.md). |
| 22 | L137: Cast | L | **Feasibility reviewed:** CEF exposes Cast/DIAL receiver routes, but not tab/desktop mirroring through its public router API. Full Arc Cast needs an engine bridge and physical-device qualification; no substitute picker implemented. See [Cast feasibility](CAST_FEASIBILITY.md). |
| 23 | L491: Page translation: "Translate this page to %@?", Use Original, Choose Another Language, Never Translate This Site | L | **Out of scope without a user-supplied provider:** Arc bundles Chromium’s Google-server translation endpoint; reproducing that service is excluded by the task’s local-only ruling. See [translation feasibility](TRANSLATION_FEASIBILITY.md). |
| 24 | L337: **Page** (Chromium's menu plus Arc's items) (?) | L | **Implemented subset:** Save As uses the existing exporter; checked video Loop/Controls, open/save/address/PiP and editable selected-provider Search/Print are document-bound. Native loop/controls/address/save-cancel pass. Intrinsic frame export, Capture Image integration, full editing submenus, highlight links and clicked-node inspection remain partial; Cast/translation classified separately. [Evidence](PAGE_MENU_ACTIONS.md). |
| 25 | L98: Capture Full Page / Capture in Portrait Mode | M | Capture the full document or portrait viewport through a bounded retained-document engine request. |
| 26 | L496: Page full screen: "To exit full screen, press …" toast | S | **Functional gap fixed:** actual CEF document entry/exit, scoped instruction, Escape release and original native-window/Split restoration pass GPUI and disposable native acceptance. Quitting during document fullscreen retains the original window checkpoint; genuine browser fullscreen still persists. Remaining top-edge placement/motion is cosmetic. See [fullscreen evidence](PAGE_FULLSCREEN_REFERENCE.md). |
| 27 | L495: Pointer lock: "Your cursor is hidden. Press 'esc'…" toast | L | **Engine blocker verified:** Arc acquires/releases real pointer lock and shows the exact toast; Echo Alloy inherits Chromium’s `RequestPointerLock` default, which returns the native-observed `kUnknownError`. A qualified CEF engine hook is required before scoped notification/release wiring; no fake success UI. See [pointer-lock evidence](POINTER_LOCK_REFERENCE.md). |
| 28 | L279: Websites can read the theme ("Allow websites to get your theme data", CSS variables `--arc-palette-*`) | M | **Functional gap fixed:** the existing Advanced option applies Arc’s 12 main-document variables, clears retained pages on opt-out and updates selection/navigation/appearance without reload. Hidden-first documents and child-frame exclusion match live Arc; private/Little models consume the install preference. Native and GPUI acceptance pass; exact palette colors remain cosmetic. See [reference](THEME_DATA_REFERENCE.md). |
| 29 | L257: Creating a Space ("Create a Space" dialog: name, icon, theme, profile) | M | Connect existing theme/profile selectors to the window-local Space-creation draft before atomic publication. |
| 30 | L265: Assigning a profile to a Space ("Set Profile", "Edit Space profile") | M | Allow choosing the profile in a new-Space draft without reassigning an existing Space. |
| 31 | L387: Global shortcut "Open Little Arc when I press … in any app" | M | Support recording the Little shortcut and implement missing platform global-key registration. |
| 32 | L388: Links from other apps open in Little Arc | M | Register and dispatch external URL launches on Windows/Linux through the existing local ATC router. |
| 33 | L389: ⌥⌘-click on a link opens it in Little Arc | S | **Stale gap verified closed:** already-qualified trusted Option-Command navigation opens Little in its source profile; disabled preference opens Split View and a canceled website click opens nothing. Five native separate-GPU routing cases exit cleanly; ordinary modifier routing preserves source focus/Today order. See [reference and receipts](PROFILES_SETTINGS_REFERENCE.md#round-3--modifier-link-routing). |
| 34 | L391: Show/Close all Little Arc windows | M | Implement Show/Close All Little windows with correct archive and renderer teardown semantics. |
| 35 | L401: Moving Peek into a tab or a Space ("Open Little Arc or Peek in…" ⌘O) | M | Add the dedicated Peek destination picker and safe cross-profile storage transition. |
| 36 | L166: Open Little Arc or Peek in… (pick a Space) | M | Complete reference-qualified cross-profile Little/Peek promotion without leaking storage or losing the lookup on failure. |
| 37 | L390: Toolbar: Open in (Space, "More Spaces", "Search Spaces…"), Change profile, Extensions, Copy URL, downloads | M | Connect Little's extension/download controls to existing profile-scoped services. |
| 38 | L306: Running an extension by name ("Extension …") | S | **Stale gap verified closed:** the existing enabled, profile-owned `Extension NAME` result invokes its real action via keyboard Enter without navigating the owner page. Disposable native action/Cmd+W/clean-exit acceptance and existing GPUI profile-capture/stale-action coverage pass. See [qualification](EXTENSION_NAME_COMMAND.md). |
| 39 | L297: Removing a suggestion ("Delete Suggestion") | M | **Test-verifiable sub-gap implemented:** the existing delete control suppresses supported remote completion text locally per profile/provider, without deleting history or the typed search. Popup/request/provider/profile guards reject stale callbacks; session persistence and bounded storage are tested. Provider-account deletion and native/visual qualification are excluded. |
| 40 | L296: Site Search: Tab or Space after a site's name searches that site; has a management menu | M | Discover supported site-search templates and persist per-profile activation/management. |
| 41 | L292: Search engine suggestions (Chromium: Search Suggestion, Navigate Suggest) | M | Add missing provider completion transports and distinguish navigable suggestions from search text. |
| 42 | L295: Popular domains (Popular Domains, ARC_TopDomains) | M | Expand the independent common-domain catalog and qualify ordering against local/history results. |
| 43 | L329: **Folder**: Share, Duplicate, Archive Folder / Delete, New Nested Folder…, Turn into Space, Copy All Links (as Markdown) | M | Wire nonempty-folder archiving as one scoped Undo transaction and replace the destination picker with a real submenu. |
| 44 | L328: **Multi-selection**: Archive Items, Copy links (as Markdown), New Folder with N items, Move to… | M | **Ordinary folder/mixed batch actions fixed:** Copy, Delete, New Folder and same-profile Move with captured roots and transaction Undo. Discontiguous/repeated ranges and ordinary dismissal are now qualified; combining existing split groups, hidden/collapsed-tree and cross-window/alternate states remain F. Favorite modifier clicks activate in Arc; Favorite multiselect is not a gap (see FAVORITE_MODIFIERS.md). |
| 45 | L327: **Tab**: Copy Link, Copy link as Markdown, Share, Duplicate, Rename (?), Edit Pinned Page → Replace Pinned URL with Current, Add/Open in Split View, Move to… (Spaces, profile), New Folder with…, Mute/Unmute, Archive Tab / Close Tab / Remove Pin / Remove Favorite, Archive Other Tabs / Tabs Below / Tabs Above, Customize Page… (Boost) | M | Complete captured-tab Share/Move and playback-dependent menu availability without selecting the target. |
| 46 | L118: Find… / Find and Replace / Find Next / Find Previous / Jump to Selection | M | Complete context-eligible Find actions including Jump to Selection and editable replacement. |
| 47 | L139: Developer: Developer Tools, Inspect Elements, JavaScript Console, Network Inspector | M | Support DevTools dock positions/resizing and persist dimensions while retaining frontend/source focus. |
| 48 | L100: Print | M | Implement the missing Linux print handler and validate native Windows cancellation/printing. |
| 49 | L96: Share… | M | Provide platform-native share presenters on Windows/Linux behind the existing captured-page interface. |
| 50 | L72: Set as Default Browser | M | Implement Windows/Linux default registration adapters and qualify OS consent/cancellation. |
| 51 | L119: Spelling, Substitutions, Transformations, Speech, Format (Bold/Italic/Underline) | M | Connect remaining editing preference toggles and platform responder adapters with context-sensitive enablement. |
| 52 | L76: Hide Arc / Hide Others / Show All | M | Provide equivalent supported window-hide operations on remaining desktop platforms. |
| 53 | L77: Quit | M | Qualify non-macOS quit visibility and ship the patched CEF teardown path rather than the risky fallback. |
| 54 | L84: New Window | M | Qualify live-page ownership transfer and window restoration on Windows/X11. |
| 55 | L86: New Incognito Window | M | **Test-verifiable sub-gap fixed:** private Command Bar, root/overlay/page shortcuts and direct folder/pin dispatch cannot move a tab into unrendered Pinned/Favorites/folder/Space destinations. Private New opens the Command Bar; ordinary duplicate/close/restore remains available. Broader reference/menu presentation remains partial. See code/test-only delivery evidence. |
| 56 | L85: Blank Window | M | Complete Blank-window profile selection, Archive behavior and relaunch persistence. |
| 57 | L70: Preferences… | M | Close the remaining Settings backend integrations listed in the per-setting rows below. |
| 58 | L73: Import from Another Browser… | L | Complete the supported local import formats and categories; use the detailed onboarding/import row as the canonical task. |
| 59 | L87: New Little Arc Window | M | Finish the Little prompt/placement and platform entry paths listed in section 10. |
| 60 | L90: New Boost / Edit Boost / Boost Gallery | M | Expose local Boost rename/delete controls using existing profile-owned model operations; omit cloud gallery. |
| 61 | L149: New Space… | M | Finish theme/profile editing within the existing Space-creation draft. |
| 62 | L152: Change Profile | M | Complete the empty-window warning policy for captured-Space profile reassignment. |
| 63 | L214: Resize sidebar | S | **Test-verifiable sub-gap fixed:** fresh normal windows inherit the committed source width; app-level creation uses the latest normal checkpoint, with the existing default if none exists. Window-local widths are checkpointed and restored independently; restored width wins over inheritance. Root/overlay/renderer, retained-document and session regressions cover the change. Native drag/visual/platform qualification remains partial. |
| 64 | L220: **Favorites** (Top Apps): icon grid above Spaces, shared by all Spaces of a profile | M | Complete unread/calendar/profile behavior separately from the already-working Favorites dock. |
| 65 | L229: Dragging tabs (between sections, into folders, into Favorites, into another Space) | M | Extend held-drag auto-scroll to folder drags and qualify edge cancellation across target types. |
| 66 | L233: Custom tab icon ("Customize Tab") | M | Support uploaded custom tab images with bounded local decoding and persisted identity. |
| 67 | L244: Incognito window sidebar ("Incognito Tabs") | M | Complete private New/context-menu availability without exposing regular-profile operations. |
| 68 | L246: Media controls in the sidebar | M | Complete the media-control behaviors tracked in the detailed mini-player row. |
| 69 | L264: Space Manager (Library → Spaces) | M | Complete Space Manager theme/overflow actions and behavioral routing independently of card styling. |
| 70 | L272: Theme editor ("Edit Theme Color…", "Tap to pick a color for this space") | S | Clamp the theme picker to the work area and anchor Library entry to the clicked card so controls stay reachable. |
| 71 | L305: Other: Toggle Sidebar, Manage Passwords, Search Settings, Air Traffic Control, Edit Keyboard Shortcuts, Check for Updates, Undo/Redo | S | **Missing route implemented:** Check for Updates… and its aliases call the existing Echo updater action, reusing its utility window and in-flight check. Existing sidebar/password/search/ATC/shortcut/Undo/Redo routes remain. GPUI tests cover invocation, cancellation, deduplication, retry and source-page retention. Arc’s feed is excluded; native presentation remains unqualified. |
| 72 | L348: Archive page in Library: "Today/Yesterday", filters by Space, "How was the tab closed?" (Auto-Closed/Manually Closed), Little Arc | M | Reconcile the stale Library/Little archive note with the implemented cross-profile inventory and remaining restoration gaps. |
| 73 | L374: Resize Split View panes | M | Verify and fix overflow hit testing for four-pane horizontal/vertical splits. |
| 74 | L375: Create by dragging a tab to the middle of the window | M | Qualify and implement initially-unarmed center-drop semantics without changing retained-page identities. |
| 75 | L386: Small window for quick lookups | M | Open the initial Little lookup prompt and persist its placement instead of immediately showing a full window. |
| 76 | L408: "New Route" rules: URL or domain "Is equal to" … → Space / Most Recent Space / Little Arc | M | Close remaining rule matching/destination limitations documented in ATC_REFERENCE without touching user rules. |
| 77 | L418: Changing a Space's profile, with warnings ("Switch to your … Profile?") | M | Implement the empty-window warning policy for a Space profile change. |
| 78 | L443: General → Built-in blocker: Block Ads, Block Trackers, Block Cookie Banners, "Ad Block Settings…" (Advanced Ad Block Settings), advice to keep a single blocker | L | **Reclassified with #4:** no such controls are exposed by the installed desktop reference; do not invent them. |
| 79 | L452: Max → 5-Second Previews | M | Complete automatic-site eligibility and image/system-share payload handling for 5-Second Previews. |
| 80 | L463: Archive (see section 7) | M | Restore Little archive items to the correct destination and retain final-window/app-quit lookups exactly once. |
| 81 | L464: Downloads (⇧⌘J): filter, filter by profile, "download flies into Library" animation, "File deleted or moved" | S | **Test-verifiable sub-gap fixed:** published-path availability refreshes the existing subtitle on entry, activation, periodic refresh and failed file actions. Open/Show/Copy/Trash revalidate the canonical path; Hide remains metadata-only. Restoring a file at that path clears the transient state without changing identity or filters. Native/visual and completion-flight qualification remain partial. See code/test-only delivery evidence. |
| 82 | L465: Media: Captures and files from Desktop/Documents/Downloads, Open in Preview, Show in Finder, Hide from Arc, Copy | L | Implement bounded local media discovery and real file actions; avoid inventing a populated Arc layout. |
| 83 | L466: Boosts: list, on/off, rename, delete | S | Expose local Boost rename/delete operations through the existing editor/list interaction contract. |
| 84 | L488: Site Control Center: security icon, Copy URL, Capture / Full Page / Portrait, Share, Boosts, extensions (pin/unpin, Add Extension), Clear Cache, Clear Cookies, All Site Settings… | L | Complete cross-origin permission requests, main-panel keyboard traversal and global site-settings navigation. |
| 85 | L494: Dragging the window by the top of the page (option) | S | Honor the saved drag-window-from-top-of-page option through a portable host interface. |
| 86 | L498: Chrome extensions (icons, "Open pinned extensions picker") | L | Connect the extension picker/action host to the qualified engine bridge; replace the stale approval-gated note. |
| 87 | L500: Quietly creating a tab while the sidebar is hidden ("New Tab Created in %@") | S | **Stale missing-feature note corrected; integration gate open:** existing background Command Bar and renderer routes retain source focus, notify only with the sidebar hidden and reveal the captured target. Native UI steps pass, but repeated separate-GPU quits reach CEF’s teardown watchdog. Keep F until the clean-quit gate is repeatable; visual qualification remains separate. See [evidence](BACKGROUND_TAB_NOTIFICATION.md). |
| 88 | L506: Mini player in the sidebar: Back to Tab, Open Picture in Picture, Previous/Next Track, Adjust Tab Volume, Toggle Microphone/Camera, Close Mini Player | M | Handle paused-video PiP, active PiP state and supported iframe media/microphone/camera controls. |
| 89 | L507: Automatic PiP when leaving a tab with video (option) | M | Qualify automatic PiP eligibility for split panes and iframe/unmanaged videos across platforms. |
| 90 | L508: PiP window: Back to Tab, ±15 seconds, seeking by dragging up, Keep Window on Top, minimize or close, remembered position | L | Complete PiP context controls, drag-up seeking and cross-run bounds while retaining the live source. |
| 91 | L509: PiP for Google Meet (with chat) and Document PiP | L | Host Document PiP through a scoped GPUI surface; Meet/chat acceptance requires a separate authenticated fixture. |
| 92 | L548: Translation: Translated to %@ / back to %@ / error | M | **Out of scope with the Google translation backend:** no provider supplied; see [translation feasibility](TRANSLATION_FEASIBILITY.md). |
| 93 | L551: Shortcut tips ("Copy links faster by pressing…", "… to easily show and hide your Sidebar!", Site Search) | S | Connect reference-backed shortcut education to real eligibility/dismissal events without repeated prompts. |
| 94 | L560: Steps: theme ("Add some flair with a theme."), Favorites ("Choose the apps you use most."), ad blocker, Arc Max, default browser, member card | M | Complete the local Favorites catalog and AI setup steps; built-in-blocker setup is not active desktop parity; exclude accounts/sync/member cards. |
| 95 | L564: Default browser prompts ("Try Arc as default for one week" etc.) | S | Implement the qualified recurring default-browser reminder policy without repeated OS requests after cancellation. |
| 96 | L573: CEF startup and framework lifetime | L | Remove the allocator-registration startup race with deterministic native regression acceptance. |
| 97 | L582: Swipe back/forward on the page | M | Implement Windows/Linux gesture input bridges while preserving scroll-container cancellation policy. |
| 98 | L583: Animated tab dragging, dragging to the middle for Split | M | Finish initially-unarmed center-drop behavior and platform drag routing; defer raster/settling polish. |
| 99 | L591: Classes: window (active tab, active space, incognito, mode), tab (id, title, URL, loading, location: topApp/pinned/unpinned), space (id, title) | M | Support range/predicate selectors and verify runtime incognito/window-state scripting snapshots. |
| 100 | L592: Commands: make, close, select, go back, go forward, reload, stop, execute (JavaScript), focus (space) | M | Complete Little creation and unloaded-target scripting behavior using canonical profile mutations. |
| 101 | L598: Capture (screenshot of an area, ⇧⌘2), saved to Library | M | Complete private/Blank capture destination behavior and functional local-media publication; omit Easels/cloud sharing. |
| 102 | L601: Task Manager, trace recording | L | Add GPU/utility/worker process IDs, network sampling and local trace recording through the CEF API extension. |
| 103 | L603: Arc Max (AI: Tidy Tabs, tab/download renaming, 5-Second Previews, Ask on Page, ChatGPT in the Command Bar, Browse for Me, Instant Links) | M | Qualify live direct-provider capability/error handling and remaining automatic preview coverage; do not invent unavailable Ask/Browse controls. |

## 2026-10-26 qualification blockers

- **#16, navigation history:** the ordinary-link flow is now positively qualified in Arc and implemented through retained CEF entry IDs. A right-click at Three lists Two/One; selecting One leaves Forward listing Two/Three. The old hold-based assumption was not observed through 2.3 seconds. A fresh guarded typed-address check now positively exposes the same menu immediately after two Cmd+L replacements. Native Echo selects One and Three in the same tab, requests each URL once and exits 0; the earlier failed synthetic attempts are superseded. Remaining work in this row is cosmetic qualification. See [history evidence](NAVIGATION_HISTORY.md).
- **#17, Exit context:** entering native full screen creates AppKit snapshot/toolbar windows, including a transparent 32 px strip and a 600×600 auxiliary surface. The mandatory one-extra-window guard aborts before Command-Escape can be qualified. Subsequent Cmd-N attempts returned either no new window or multiple full-screen surfaces; an authorized Arc restart did not produce a qualified reference. Existing Peek Command-Escape behavior is unchanged. Evidence: `target/parity/reaudit2/sessions/exit-context-functional*` and `document-app-functional/after-command-n.json`.
- **#21, document preference:** Arc's binary contains `Creates a new note. Document types can be set in the Profiles tab.` and the creation destinations `https://confluence.new`, `https://doc.new`, `https://app.notion.com/new`, and `https://word.new`. The previous proposed fix misidentified this as opening downloaded documents. The Command Bar reference remains blocked by fresh-window ownership checks; no account-backed document was created.
- **#87, hidden-sidebar notification:** the feature already exists. Fresh native UI steps succeed; repeated bundled CEF shutdown reaches its watchdog in both GPU modes. Keep the integration gate open; see [the qualification note](BACKGROUND_TAB_NOTIFICATION.md).

No rows in this qualification pass are marked done. These blockers do not authorize touching owner Spaces, bypassing the shared guard, or waiving native termination failures.

## Cosmetic-only inventory

| Baseline row / feature | Reason to defer |
|---|---|
| L91: New Profile | Local profile creation and Settings entry already work; stale inventory note, no remaining functional gap identified. |
| L97: Capture… | Capture works; remaining pointer/instruction/backdrop/glyph/motion differences are cosmetic. |
| L125: Appearance: websites use Automatic / Light / Dark | Website color schemes work; remaining platform checkmark presentation/acceptance is cosmetic. |
| L133: Add Split View | Split creation/cancellation works; remaining native visual review only. |
| L135: Separate Page from Split View / Expand Current Split | Separate/expand work; expansion animation fidelity only. |
| L150: Edit Theme… | Theme editing exists; remaining appearance work is documented in theme rows. |
| L184: View History | History search/open/delete works; stale visual-review note only. |
| L195: View Downloads | Downloads routing/progress/cancel works; remaining motion/presentation only. |
| L219: "Show as Toolbar" mode (context menu: Show as Toolbar / Hide Toolbar) | Toolbar mode and address actions work; material/mode-switch motion only. |
| L222: **Pinned** section of a Space | Pinned URL editing/reset/collapse work; dialog shadow/keycap/motion only. |
| L230: Haptic feedback when reordering tabs | Haptic preference/reorder feedback works; exact tactile timing and glyph raster only. |
| L232: Renaming a tab by double-click ("Double-click to rename") | Renaming works; Favorite/Split editor placement only. Unfinished polish is stashed. |
| L235: Audio indicator, Mute/Unmute in the context menu | Audio activity and mute work; Favorites placement/tooltips/theme only. |
| L236: Camera/microphone indicator on a tab | Capture indicator works; activation motion and Favorite/Split placement only. |
| L237: Split View item in the sidebar (tab group) | Split row behavior works; Favorite-group/hover/theme presentation only. |
| L239: Sidebar interaction states and Space dock | Interaction controls work; remaining drag/playback/reorder presentation only. |
| L240: Browser chrome iconography | Icon system exists; remaining per-action glyph/raster differences only. |
| L242: "+" button in the dock: New Tab, New Space, New Folder, New Split, New Boost | All in-scope New menu routes work; popover motion/material/vector raster only. |
| L243: "Springy" click animation on sidebar items | Click feedback works; other item animation and reorder settling only. |
| L255: Two-finger swipe on the sidebar: Spaces follow the fingers and rubber-band at the edges | Space swipe follows fingers and snaps; exact curve/flick/resistance/virtual-page motion only. |
| L259: Space icon (emoji or symbol, "Change Space Icon…", "Remove icon") | Space icon selection/removal works; external popup position/animation only. |
| L271: The Space color tints the sidebar and window; during a swipe the color blends smoothly | Theme fields/blending work; exact shader/inactive material calibration only. |
| L273: Palette with several colors ("Add color" / "Remove color"), gradient of 2–3 colors (?) | Add/remove colors works; added pigment/handle placement and spatial shader differences are cosmetic. |
| L274: Preset color sets in pages: Brand, Drab, Greyscale, Pastel | Palette selection works; transition and gamut rendering only. |
| L275: Noise texture ("Cycle noise texture") | Noise selection/strength work; procedural texture and dial contrast differences only. |
| L276: "Intensity" slider | Intensity is persisted/live; backing/wave/response calibration only. |
| L277: Mode: Automatic / Light / Dark Appearance | Appearance mode works/persists; picker/shader and fresh restart qualification only. |
| L287: Popup over the page | Command Bar works; info-button/material/platform fallback calibration only. |
| L304: Tabs: Duplicate Current Tab, Rename Current Tab…, Reset Tab, Replace Pin with Current Page, Mute/Unmute All Tabs | Tab actions work; rename dialog placement and action glyph/status presentation only. |
| L331: **Split View in the sidebar**: Share Split View…, Separate All Tabs, Add Split View, Convert to Vertical/Horizontal, Tab Options → Reset Name and Icon, Duplicate, Archive…, Separate from Split View | Split menu operations work; third-level hover and exact remaining menu presentation only. |
| L332: **Split View pane** (control menu): Move, Add Split, Separate Tab from Split, Convert orientation, Share Split View… | Pane operations work; menu material/placement only. |
| L344: Auto Archive interval choices | All observed Auto Archive choices already persist and drive the policy; stale partial status. |
| L369: Add a pane (up to 4 panes, "Cannot Add New Pane" toast) | Add/cancel pane works; empty header/corner/body/motion calibration only. |
| L371: Split pane corner contour and focus | Split focus/header/SCC works; corner raster/opacity/tooltips/material only. |
| L372: Split sidebar item | Split row closing/naming works; Favorite-group/fill/favicon presentation only. |
| L377: Separate Page / Separate All Tabs / Expand Current Split | Separate/expand work and preserve live pages; animation only. |
| L400: Closing Peek (⌘Esc / Esc), Undo "Close Peek" | Peek close/Undo retains document; exact glyph/raster/spring only. |
| L417: Creating a profile: name and which Spaces use it | Profile creation/assignment works; baselines/borders/material/illustration differences only. |
| L462: Spaces (Space Manager: order of Spaces, folders, tabs) | Space manager actions work; picker anchor is ranked separately; card/menu material polish only. |
| L485: The page sits in an inset "card" with rounded corners | Inset page/empty frame works; shadow/border only. |
| L486: Loading indicator (glowing progress, `GlowProgressLoadingIndicator`) | Loading progress works; remaining glow rendering/motion calibration only. |
| L487: Site-specific zoom indicator | Live zoom indicator works; remaining appearance qualification only. |
| L489: Status bar with the URL when hovering a link (`StatusTextView`) | Status URL behavior works; Light/shadow/transition/host formatting calibration only. |
| L547: Import: Now importing… / Success / partial / failure | Import progress and success/partial/failure toasts already exist; stale missing inventory status. |
| L552: Screenshot Copied to Clipboard, Capture in Progress… | Capture copy/progress feedback already exists; reconcile stale inventory status before changing presentation. |
| L559: Choosing the profile to import into ("My Existing Arc Profile" / "Create a New Profile") | Import destination selection/publication works; picker visuals only. |
| L561: "Let’s settle in! Here are the basics." screen (Pin tabs, Split View, Spaces, Open your first tab, with video) | Basics/shortcuts route works; instructional video and layout are presentation. |
| L562: "Essential Keyboard Shortcuts" window (⌘T, ⌃Tab, ⇧⌘C, ⌃⇧=, ⌥⌘N) | Shortcut help works; separate-window geometry only. |
| L563: Empty state: "Open your first tab. Click New Tab or press ⌘T…" | Empty page is usable with Command-T; placeholder copy/layout only. |
| L579: Sidebar swipe: Spaces follow the fingers, resistance at the edges, flick by speed, snapping back after a short swipe | Space swipe works; exact resistance/flick/icon motion only. |
| L584: Haptic feedback and Arc's sounds (`ARC_SoundEffects`) | Haptic/sound preferences work; exact timing/cue only; meeting cue belongs to blocked calendar integration. |
| L585: Springy click on sidebar items | Click animation exists; remaining item-type/settling animation only. |

## Blocked or unqualified inventory

| Baseline row / feature | Specific blocker |
|---|---|
| L69: About Arc | OS legal-document opening acceptance has no demonstrated failure; link tint is cosmetic. |
| L74: Check for Updates… | Apple signing/notarization and available-update acceptance require the release signing environment; banner polish is deferred. |
| L75: Privacy Policy | Packaged document opening needs OS acceptance; no broken route established. |
| L92: New Note | Current Arc Notes creation reference is unavailable; confirm present product scope before building an obsolete editor. |
| L93: New Note (in Split) | Same unavailable Arc Notes flow as New Note; do not invent a Split Notes editor. |
| L113: Cut / Copy / Paste / Select All | Remaining platform/non-input editing contexts lack acceptance evidence, not a demonstrated failure. |
| L116: Copy URL as Quote (link to the selected text) | Arc clipboard-format/native acceptance missing; existing local selection route is implemented. |
| L117: Paste and Match Style | Remaining platform/menu-state acceptance only; no demonstrated missing editing path. |
| L127: Show/Hide Toolbar (top bar mode) | Remaining address-menu matrix is unqualified; toolbar switching already works. |
| L132: Clear Cookies and Refresh / Clear Cache and Refresh | Other-platform behavior remains unqualified; captured macOS clearing works. |
| L140: Developer: Allow JavaScript from Apple Events | Remaining menu/profile state acceptance only; opt-in denial/evaluation already work. |
| L141: Toggle Developer Mode (per site) | Localhost policy was subsequently implemented; remaining extension/portrait capabilities are ranked separately and the split-state matrix is unqualified. |
| L142: Enter/Exit Full Screen | Other-platform fullscreen acceptance is pending; macOS entry/exit works. |
| L155: Manage Spaces… (Space Manager in Library) | Repeated manager entry needs qualification; no established broken route. |
| L162: New ChatGPT Message… | Separate new-message command eligibility/reference is unqualified; ChatGPT composer handoff already exists. |
| L163: Join Meeting (link to the next meeting) | Calendar/next-meeting integration needs authenticated reference. |
| L164: Peek Email… (code from an email) | Email-code integration needs authenticated reference and provider/permission contract. |
| L167: New Folder… | Conflicting native shortcut is unresolved; do not override Incognito without positive reference. |
| L170: Tab Switcher forward/back ("Recent Items" overlay, MRU, like ⌘Tab) | Five-item native/combined-thumbnail acceptance remains; three-item MRU cycle already works. |
| L185: View Archive | Remaining Archive acceptance is covered by the detailed Library row; no separate defect established. |
| L193: Minimize Window / Zoom / Bring All to Front | Native Zoom/Bring All ordering and remaining platform acceptance are unqualified. |
| L194: View Library | Umbrella Library note does not establish another defect beyond its detailed section rows. |
| L196: View Media / View Boosts | Remaining populated Library content/platform acceptance is tracked below; no separate route failure. |
| L197: Help: Getting Started, Essential Keyboard Shortcuts, Release Notes, Contact the Team, Help Center, Restore Data, Export Arc Notes | Current local help/restore/export destinations need a qualified contract; never recreate Arc team/account routes. |
| L198: Troubleshooting: Record Trace, Task Manager, Reveal Arc Data, Copy Arc Info | Trace sampling gaps are ranked under Task Manager; the remaining umbrella route matrix is unqualified. |
| L221: Hover previews for Favorites, Pinned and folders (Gmail, Google/Outlook Calendar, Linear, Notion, Figma, Confluence) | Third-party authenticated previews/reference explicitly deferred. |
| L225: "Tidy Tab Titles": automatic shorter names on Pin | AI Split/Favorite eligibility/feedback lacks a positive reference; ordinary title shortening works. |
| L226: "Tidy Tabs": AI grouping of Today by topic | Arc returned no successful grouping reference; local provider grouping exists. |
| L228: Live Folders (GitHub Pull Requests: Created by Me, Review Requests…) | Authenticated Arc PR/badge/SSO presentation is unverified; local PAT implementation already exists. |
| L245: Banners at the bottom of the sidebar: update, "We cleaned up!" after auto-archiving, speed-up (several ad blockers) | Education eligibility/layout has no positive reference; blocker gap is ranked separately. |
| L260: Deleting a Space (with Undo) | Native destructive last-Space deletion is prohibited; tests exist, no demonstrated defect. |
| L262: Turning a Space into a folder ("Export “%@” into Folder") | Live Space export removes a Space and is prohibited; tested implementation is present. |
| L263: Live Folders for a Space | Authenticated Live Folder rows are unqualified; local creation/reuse works. |
| L291: Switching to an open tab | Native cross-Space acceptance conflicts with the shared-test-only rule; local switching has regression coverage. |
| L294: Calculator (Search Calculator Answer) | Installed Arc returned no calculator answer; positive reference required before claiming or changing answer semantics. |
| L307: Join next video call / Zoom | Authenticated next-meeting/calendar reference required. |
| L308: Suggestions for an empty field (Air Traffic Control, Tips for Organizing, Help Center) | Help-row eligibility/destination and Max empty-result policy lack a qualified reference. |
| L309: ChatGPT in the Command Bar, "Browse for Me" (Arc Max) | Desktop 1.170 exposes no Browse for Me control; do not invent one. |
| L310: Instant Links (Arc Max) | Live provider billing/capability and Arc ranking are unqualified; local streaming/result path exists. |
| L330: **Live Folder**: Pull Requests, Repositories, Single Sign-On, Delete Live Folder | Authenticated cookie-SSO/status visuals cannot be validated with the available local PAT reference. |
| L333: **Space**: Change Space Icon…, Rename, Edit Theme Color…, Set Profile, New Folder, Live Folders, Share, Show/Hide Space Header, Manage Spaces…, Delete Space, New Profile, Export into Folder | Destructive last-Space/native export and authenticated Live Folder states cannot be exercised under the shared-session rules. |
| L334: **Favorite with a calendar** (Live Calendar): Show Hover Preview, Show Time to Next Meeting, Show Next Meeting Link, reminder 2/5/10 minutes before | Authenticated calendar/meeting reference required. |
| L335: **Download**: Open, Copy, Show in Finder, Cancel Download, Hide from Arc, Move to Trash | Active Cancel is already implemented in ordinary-download evidence; direct row activation lacks a usable positive Arc reference. |
| L336: **Tab group header**: Remove header | Header removal semantics and current eligible reference are unavailable. |
| L350: "Auto archived %d tabs" toast and the "We cleaned up!" banner | No positive education-banner eligibility/layout reference; archive notification already works. |
| L392: Tip for new users "Links Open in Little Arc" | No positive new-user eligibility/presentation reference; do not invent a repeated tip. |
| L398: Links to other sites from Pinned/Favorites open in Peek (option) | Subdomain edge-policy acceptance lacks a positive reference; cross-host Peek works. |
| L402: Swipe to dismiss the modal (`modal-v2-native-gestures`) (?) | Gesture is inferred from a string; no qualified current dismiss gesture reference. |
| L490: Find in Page ("No results found"), Find and Replace | Editable-context Find/Replace availability has no positive Arc reference; ordinary Find works. |
| L497: Developer mode for a site, DevTools | Other loopback-address policy is unqualified; current site developer mode works. |
| L510: Per-tab audio activity | Windows/Linux native A/V acceptance remains; measured macOS route/timestamp behavior works. |
| L550: Battery Saver Mode Activated/Deactivated | Automatic Battery Saver active state/eligibility has no qualified reference. |
| L600: Battery Saver mode | No exposed manual command; automatic active-state reference required before adding controls. |

## Delivery evidence

No status is promoted by triage alone. Keep a mixed row partial if broader functional or cosmetic qualification remains. A visual pair is required only when the change introduces or changes presentation.

### Reveal Current Tab — completed

Added Control-Space to the single shared GPUI/CEF shortcut registry. Existing Settings shortcut rebinding can now find its baseline action. The existing reveal transaction handles hidden/floating sidebar, collapsed ancestors and Split anchors without creating or navigating a page.

- Full workspace: `CARGO_TARGET_DIR=/tmp/echo-audit-build-9e64e52b CARGO_BUILD_JOBS=3 RUST_TEST_THREADS=4 scripts/with-cef.sh cargo test`, green; log `/tmp/echo-functional-baseline-test.log`.
- Eight focused Reveal tests pass, including a new renderer-origin shortcut regression and a root-keyboard collapsed-Pinned test. Fake-engine calls assert no create/close/navigation.
- Native: disposable bundled profile, `https://example.org/`, frame `{100,100,1000,760}`; sidebar dumps `false` then `true`, selected URL unchanged, CEF Complete and process exit zero. Evidence: `target/parity/functional-release/reveal/engine.log`, `owned-windows.json`, `revealed.png` (inspected). No Arc windows, Spaces or system default were changed. Driver uses a canonical executable path, explicit bundle activation and bounded cleanup.
- `Reveal Current Tab` is promoted to done. Baseline counts above remain an audit snapshot; one of its 103 F rows is now closed.

The preliminary Save Page shortcut probe reached its handler and prepared the native sheet, but the driver rejected sheet focus before cancellation. It is **not** a passed Save Page acceptance check; format persistence and the end-to-end gate remain open.

### App-menu Command Bar routes — three missing routes completed; parent row partial

About Echo, Import from Another Browser… and Set as Default Browser now call the existing utility/setup/registration handlers. Import is unavailable in private, Blank and Little contexts. A Command Bar snapshots the registration state once; the existing serialized request rechecks it before requesting platform consent. Ours and Unavailable states omit the action.

- Five new GPUI tests exercise actual typed queries/Enter, utility creation, import Escape with no engine navigation, private/Blank rejection, availability refresh and one serialized mocked request. All five pass. The full workspace suite passes: 1,259 browser tests, 264 core tests and all other enabled suites; `/tmp/echo-functional-command-test-passed.log`.
- Native About open/Cmd-W and default query → in-memory request → action hidden are under `target/parity/functional-release/commands/`. `BROWSER_TEST_DEFAULT_BROWSER=1` installs the existing automation-only registration fixture; no OS registration/default is changed. The import fixture is a disposable empty directory, not an owner's browser profile. Native exit is zero.
- The combined System Events probe did **not** establish Import acceptance: its screenshot retained the Command Bar. A fresh standard NSEvent-driver check independently opens the actual picker, captures it, cancels with Escape, dumps `command_bar=None` before/after, retains the one `https://example.org/` tab and exits zero. Accepted Import evidence is `target/parity/functional-release/import-route/{engine.log,import-screen.png}`; its screenshot was inspected. Rapid utility-to-utility re-entry remains unqualified; do not treat the combined probe's window count as proof of a specific dialog.
- No new Arc session, pixel calibration, owner Space or system default modification. Existing action-row/dialog presentation is reused; no new visual parity claim. The broader Type Travel row stays partial for the remaining action catalogue. Baseline classification counts remain unchanged; only Reveal is promoted to done in this batch.

The pre-strategy rename-dialog polish is preserved in the local stash `WIP rename dialog before functional release triage`; it is intentionally excluded from release-gap implementation.

### Saved-login iframe fill and account selection — functional sub-gap completed

Same-origin iframe forms autofill from the retained page's profile vault. As in
the captured Arc reference, cross-origin forms remain empty until explicit
selection. The GPUI username dropdown supports mouse, Up/Down, Return and Escape.
CEF validates the current frame/document/origin; selected fills additionally bind
the exact username/form/password elements. Native focus loss preserves this field
capability; a DOM focus move revokes it. Navigation, other-profile credentials,
stale vault replies and stale picker callbacks cannot retarget a choice.

- Native current canonical CEF: `target/parity/functional-logins/echo-final-v2.log`
  and `echo-final-v2-session.log`, three successful renderer fill acknowledgments,
  CEF Complete and exit zero. `echo-frame-unique-final.png` verifies the unique
  same-origin account; `echo-frame-cross-empty-final.png` verifies no cross-origin
  automatic fill; `echo-frame-keyboard-final-v2.png` verifies beta;
  `echo-frame-mouse-final-v2.png` verifies switching to alpha by mouse. All use
  only the bundled local synthetic fixture and disposable profile.
- Arc/Echo pair: `target/parity/functional-logins/{arc-iframe-accounts.png,
  echo-iframe-accounts-pair.png}` and corresponding `*-crop.png`; inspected at
  zoom. Owned Arc tabs/windows were closed in the shared Echo Test Space.
- GPUI/engine regressions cover profile/frame/document isolation, explicit
  cross-origin choice, stale navigation/dismissal, keyboard/mouse account choice,
  geometry and CEF Return normalization. See [reference](SAVED_LOGIN_FRAMES.md).
- Broader profile parity stays partial. Arc's safe suggested-value preview on
  hover, exact glyph/font raster, closed-shadow forms and native other-platform
  acceptance remain qualified. Existing unsigned-build vault Keychain access
  blocked one cold-restart probe; a fresh disposable profile was used for the
  accepted native run. No Keychain permissions were changed.
- Final full workspace gate passed: 1,269 browser, 266 core, 179 engine tests
  and all other enabled suites; one pre-existing AI test ignored. Log:
  `~/Library/Caches/EchoBuildHealth/t3-71384893/logs/login-full-tests-delivery.log`.

### Deliberate download replacement — functional sub-gap completed

An accepted native save choice can replace only its exact regular-file target
after genuine owned Complete, original-file identity/version revalidation and
source-path proof. Quiet naming retains the original no-clobber candidate list.
Arc uses native Cancel/Replace; no extra conflict picker was added.

- Arc/Echo pair: `target/parity/download-replacement/{arc-replace-crop.png,
  echo-replace-crop.png}`, inspected at 2×; full native confirmations remain
  beside those crops and in `echo-native-v5/`.
- `echo-native-v5/{accepted.json,quiet-accepted.json}` verify unchanged old bytes
  before Replace, exact replacement bytes, two distinct suffixed quiet files,
  three canonical paths/names, CEF Complete and native exit zero. All files and
  profiles are disposable synthetic fixtures. Earlier controller probes failed
  to identify AppKit's unlabelled filename field and are not accepted evidence.
- All 29 publication tests pass, including five new real-filesystem cases for
  completion, replay, changed targets, late collisions and symlinks. Full workspace
  passes: 1,299 browser, 274 core, 185 engine tests and all other enabled suites;
  one pre-existing AI test ignored. Build-cache logs: `download-replace-full.log`
  and `download-replace-focused-final.log`.
- Parent Quiet Downloads parity stays partial for motion and platform/runtime
  qualifications. [Implementation and boundaries](DOWNLOAD_DESTINATION_REPLACEMENT.md).

### Desktop blocker reference verification

Read-only installed Arc 1.170 verification, 2026-10-26: General has update/quit/previews/profile links; Profiles and Advanced expose no built-in ads/trackers/cookie-banner controls. Public example.org Site Controls exposes its Extensions grid and ordinary permissions, without a built-in blocker toggle or settings action. No blocking preferences, extensions or owner accounts were changed. The unique public fixture tab was archived and every owned window closed; cleanup records are empty.

The binary still contains `use_built_in_ad_blocker`, `Block Ads & Trackers`, and built-in-blocker status strings, while `ARCClients_BlockListClient.bundle` contains EasyList/EasyPrivacy and a Dia-hosted manifest. These are dormant/shared resources, not evidence of an enabled desktop flow. This supersedes the resource-only inventory inference for ranks 4/78 and the related onboarding sub-gap. Blocking through installed extensions remains in scope. Baseline triage counts are historical and unchanged.

Local, inspected evidence: `target/parity/blocker-verification/arc-{general,Profiles,Advanced}.png`; `target/parity/reaudit2/sessions/blocker-readonly-site/{site-controls.png,cleanup-errors.json,owned-windows-left.json}`. No screenshot or proprietary resource is committed.

### Additional local import formats — partial

The existing source sheet accepts a single dropped Safari ZIP/HTML or Firefox JSON/JSONLZ4 bookmark export, then uses the existing destination, bookmark selection and publication flow. Firefox profiles without `places.sqlite` can read their newest dated bookmark backup. Safari ZIP history is selected by `metadata.data_type`, so localized filenames and per-profile suffixes work; included source histories merge only into the destination explicitly selected by the user. Netscape HTML retains folder order/Reading List. Explicit password upload accepts a Safari ZIP containing one CSV and uses the existing encrypted profile vault; ordinary bookmark/history reads never import passwords.

Formats follow [Apple’s Safari export specification](https://developer.apple.com/documentation/SafariServices/importing-data-exported-from-safari) and [Mozilla’s local bookmark backups](https://support.mozilla.org/en-US/kb/restore-bookmarks-from-backup-or-move-them?redirectlocale=en-US&redirectslug=Backing+up+and+restoring+bookmarks). Archives are never extracted, reject unsafe paths/links, and cap file count, expanded bytes and password-entry size; selected buffers are zeroed. Category errors publish no partial category, cancellation publishes nothing, and sources stay unchanged.

Remaining functional scope: cookie and autofill import have no destination adapters; encrypted Chromium source passwords/OS-bound keys, Firefox NSS/Primary Password and opaque Safari/Chromium encrypted live sessions are not decoded. They need separate consent/key-unlocking and destination publication contracts. Safari payment cards/extensions/OTP notes are not imported by these adapters. The canonical import row remains partial, including existing later-screen visual qualification.

Validation: full workspace suite green before publication (1,307 browser, 276 core, 193 engine and all remaining enabled suites). Four new importer integration cases cover localized ZIP/HTML/history/password access, Firefox direct/profile-backup recovery, path/schema failures and nesting/password/ambiguous-CSV limits. A GPUI test follows export selection through the existing sheet and rejects retargeting after leaving Sources. The native external-file drag publishes the synthetic Safari folder/bookmark and history; source SHA-256 stays unchanged, drag operation is Copy, CEF reaches Complete and the private process exits zero. Inspected screenshots and machine-readable proof: `target/parity/blocker-verification/native-import/{echo-export-bookmarks.png,echo-export-published.png,verified.json,drag-ended.json}`. No owner files or credentials were imported.

### Nested lookup navigation — functional sub-gap completed

Little target-blank navigation retains its one live document and history. Peek
opens a normal owner tab, parks its live child and restores the same lookup when
returning to the source. Profile/document/attachment checks reject stale routing;
source retirement and owner teardown release retained pages. Covered-source
hover/focus cannot order a sibling window over the restored lookup. Internal
Site Settings sheets retain their existing stale-action behavior.

- Seven GPUI regressions pass, alongside the five Site Settings regressions.
- Native Peek: `target/parity/nested-navigation/echo-native-v5/accepted.json`,
  physical `return-source-screen.png`, original/new/restored window IDs,
  retained Back, CEF Complete and exit zero. Native Little:
  `echo-little-v2/accepted.json`, four single-tab URL states and clean shutdown.
- Arc/Echo pairs and inspected crops are listed in
  [nested lookup evidence](NESTED_LOOKUP_NAVIGATION.md). Nested Shift-click,
  background-focus reference cases, broader popup insertion order and other-OS
  native acceptance remain qualified; the parent parity row stays partial.
- Arc cleanup guard ambiguity prevented archiving a remaining synthetic Root
  fixture; no unknown/owner window was used as a cleanup target.

### Private action availability — code/test-only sub-gap

The private sidebar renders only its own Today list. Persistent-sidebar commands
are omitted from typed suggestions and rejected again at dispatch. Root, popup
and renderer pin shortcuts keep the selected private tab in that visible list.
Folder-creation endpoints reject private requests, and regular New-menu dispatch
cannot open a Space/folder menu in a private window. The tab context menu keeps
its existing layout with unavailable folder destinations disabled. Core's generic
model capabilities and normal/Blank behavior are unchanged.

Four GPUI regressions cover typed availability, root/popup/renderer routing,
captured direct dispatch and supported private duplicate/close/restore. No Arc
session, native app interaction or visual qualification is part of this change.
The parent rows remain partial for the unqualified reference/platform states.

Validation for #55: all four focused regressions and the full workspace suite
pass (1,365 browser and 284 core tests; all other enabled suites green, one
existing AI test ignored). Logs: `~/Library/Caches/EchoBuildHealth/t3-71384893/logs/private-availability-{focused,full}.log`.

### Missing completed downloads — code/test-only sub-gap

The Library samples the canonical published path on host refresh and uses its
existing subtitle for `File deleted or moved` when the path is absent or no longer
a regular file. Re-entry, window activation, the existing 60-second presenter
clock and failed file actions refresh this state. A restored file at that exact
path regains its type/source subtitle. No record is deleted or retargeted, and
query/profile selection remains unchanged. Open, Reveal, Copy and Trash recheck
availability at activation; Hide is always metadata-only.

Three GPUI regressions use disposable files and the real Library presenter: move
and restore across activation/re-entry, directory/restored-file timer transitions
with a retained query, and stale file-action rejection plus Hide. They assert
canonical IDs/paths and retained-page isolation. No platform Open/Reveal/Trash invocation, Arc
session, screen work or new layout is part of this change. Native appearance and
the completion animation remain unqualified; the parent row remains partial.

Validation for #81: all three focused GPUI regressions and the full workspace
suite pass (1,368 browser and 284 core tests; all other enabled suites green,
one existing AI test ignored). Logs: `~/Library/Caches/EchoBuildHealth/t3-71384893/logs/missing-download-{focused,full}.log`.

### Sidebar width inheritance — code/test-only sub-gap

Normal New Window and normal-source link-window requests capture the committed
source sidebar width. Requests with no explicit normal source use the latest
normal checkpoint, then the existing default. An in-progress presentation width
is not captured. Each restored window retains its own saved width, which overrides
inheritance before its checkpoint is written. Existing sidebar visibility and
page selection behavior are retained. No setting, native geometry shim or drag
physics is introduced.

Four GPUI regressions cover root/overlay/renderer creation, explicit source
precedence over another window's checkpoint, app-level fallback, persisted
width round-trip and saved-width precedence. Fake-engine calls verify no source
page creation, closure or navigation. Native resizing and exact presentation
remain unqualified; the parent parity row remains partial.

Validation for #63: all four focused regressions and the full workspace suite
pass (1,372 browser and 284 core tests; all other enabled suites green, one
existing AI test ignored). `scripts/check-targets.sh` passes both Linux GNU and
Windows MSVC for the combined code/test-only batch. Logs:
`~/Library/Caches/EchoBuildHealth/t3-71384893/logs/sidebar-width-{focused,full}.log`
and `code-only-targets.log` in the same directory.

### Remote suggestion dismissal — code/test-only delivery

Supported Google/Bing/DuckDuckGo completions retain their provider provenance
in the Command Bar. The existing delete control stores a local profile-owned
suppression key for that provider/text, bounded to 256 recent choices. It never
removes local history or blocks typing the same search directly. A fresh remote
reply is filtered against current preferences, including another window's
dismissal. New request generations, popup replacement, provider changes and
profile changes revoke captured delete callbacks. No account deletion endpoint
is called, and no suggestion transport was added.

GPUI tests use only synthetic completion payloads. They exercise the real delete
control, retained history/typed search/page identity, reopened popups, provider
isolation and stale request/popup/profile callbacks. Core tests cover bounded
eviction, normalization, unsupported payloads, legacy defaults and per-profile
session round trips. Native appearance remains unqualified.

Validation for #39: the full workspace suite passes (1,374 browser, 287 core
and all remaining enabled suites; one existing AI test ignored). New coverage:
two GPUI and three core regressions. Log: `~/Library/Caches/EchoBuildHealth/t3-71384893/logs/suggestion-dismiss-full.log`.

### Check for Updates Command Bar route — code/test-only delivery

`Check for Updates…`, `check updates`, `software update` and `update echo`
reach the same app-owned action as the existing app menu. The existing updater
owns check serialization, failure retry, its one utility window and cancellation.
The route closes the Command Bar through the existing utility-focus handoff.
No release transport, installer, feed or window presentation was changed.

Two GPUI regressions drive typed aliases/Enter and Escape, compare the retained
source document, reuse the app-menu window and retry a synthetic failure state.
Tests make no network request or native app/window interaction.

Validation for #71: two focused GPUI regressions and the full workspace suite
pass (1,380 browser, 287 core and all remaining enabled suites; one existing
AI test ignored). Logs: `~/Library/Caches/EchoBuildHealth/t3-71384893/logs/update-command-{focused,full}.log`.
