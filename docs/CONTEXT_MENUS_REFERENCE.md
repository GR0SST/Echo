# Context menus — Arc 1.170.0, 2026-10-24

Installed `/Applications/Arc.app` is the reference. Evidence is under
`target/parity/context-menus/` in this worktree. Bounded sessions hold the
shared screen lock, claim one new Arc window and use the existing Echo Test
Space and shared header crop. Other agents' fixture tabs are preserved; only
uniquely named Echo CM Alpha/Beta/Gamma tabs are opened and archived by these
scripts. No Space was created, deleted or reassigned. The Delete confirmation
was inspected and cancelled. Accessibility presses do not activate these
SwiftUI controls, so discrete guarded mouse events are used, without gestures.

## Multi-selection

`arc-multi-selected.png` and `arc-shift-selected.png` show command-toggle and
shift-range selection without navigating the active page. Active row fill is
stronger than the other selected rows. The live Today selection menu is
`arc-multi-menu.png`: Copy Links; Share 2 Items…; separator; disabled Change
Icon… and Mute on silent pages; separator; Open in Split View, Duplicate (⌥D),
Move to, New Folder with 2 Items; separator; Archive Tabs, Archive Tabs Below.
The menu measures 224×284 px, uses 24 px rows, 11 px separators and the existing
native-style shared menu surface. `arc-multi-move.png` shows Space destinations,
a check on the current Space, and New Space…. Option changes the last two rows
to Archive Other Tabs / Archive Tabs Above (`arc-multi-option.png`).

Share 2 Items… opens the Arc-server Folder sharing dialog (`arc-share-popup.png`),
not the system share sheet. It is deliberately omitted in Echo under AGENTS.md.
The dialog was cancelled with no publication; its new CG window and retained
owned parent were verified before dismissal. Other menu rows retain their order.
`arc-multi-archive.png` / `arc-multi-undo.png` record archiving and Undo on the
owned tabs. No new toast text is invented for this action. Copy feedback uses
"Copied item links" / "Copied clean links without trackers", from the previously
extracted ToastController source block. Markdown uses the existing Edit command
and shortcut; the context menu does not gain an unobserved extra row.

Echo stores selection per window and Space, in visible sidebar order. Command
and Shift clicks consume selection changes before page activation. Archive is
one identity-scoped Undo transaction across Today, Pinned and complete splits;
stale transactions are validated in full before applying. Folder creation,
duplication, mute, same-profile Space moves and plain-tab Split View creation
reuse existing actions. A cancelled New Space or folder draft cannot move a
later ordinary creation's items. Five GPUI regressions and two core regressions
cover selection, copying, folder/Space moves, split creation and atomic Undo.

Unfinished selection cases: Favorites/folder-header selection, cross-profile
moves, combining already-grouped splits, exact inline folder-creation UI,
Option-alternate archive commands and native selection dismissal timing.
The follow-up matches the observed plural Edit labels Copy URLs / Copy URLs as
Markdown, omits Archive Tabs Below for mixed/Pinned selections, and offers
Unmute when every selected item is muted.
Ordinary Today/Pinned/Favorite/folder menu layout from the sidebar-states work
is preserved.

## Additional inspected menus

`arc-space-menu.png`, `arc-space-profile.png`, `arc-space-live.png`,
`arc-space-export.png`, `arc-space-delete-confirm.png` record Space actions and
the cancelled confirmation. Set Profile is a submenu with alphabetically sorted
profiles, current checkmark, separator and New Profile…. Live Folders shows
GitHub. Export lists Spaces in sidebar order, with the source disabled. Delete
confirms: “Delete your ‘Echo Test’ Space?” / “This will archive all the tabs and
folders inside it.”, Cancel/Esc and Delete/Return. The 450×274 centered dialog
has an 18 px radius; Arc branding must be replaced by Echo's own identity.

`arc-page-link.png`, `arc-page-image.png` and `arc-page-blank.png` capture web
contexts. The owner-installed proxy-inspector extension contributes a proxy
row; that row is not an Arc browser feature and must not be copied. Platform
Speech, Writing Tools and Services entries are separate from browser commands.

Multi-selection validation before rebase: full `RUST_TEST_THREADS=4 cargo test`
passes 947 browser, 194 core and 128 engine tests. Native screenshots and
`echo-multi.log` record the private CEF fixture; fixture Favorites are local
placeholders, so the header content differs from the Arc owner's Favorites.

Inspected native pair: `pair-multi-menu-3x.png`. Its Arc column retains the
excluded Share row, so Echo is one row shorter. Menu material and font
rasterization still differ; the existing shared menu surface is unchanged.
The accepted native rerun uses `multi-native-run.zu4ad3/drive.log`, initializes
CEF, verifies the restored live Alpha renderer, and exits 0 after CEF Complete.
Earlier retries reused a fixture directory and produced mixed logs; they are
invalid evidence. The harness now creates a fresh directory for each run and
rejects CEF-unavailable/existing-session fallback.

`arc-space-profile-confirm.png` records selecting Work in the profile submenu,
then cancelling “Switch to your Work Profile?” before any assignment. This
confirms that selecting a profile opens the confirmation directly.

`arc-selected-copy-url.txt` / `arc-selected-copy-markdown.txt` verify the two
existing copy shortcuts on the same selection. Plain copy joins clean URLs in
sidebar order; Markdown uses one `- [title](url)` bullet per selected page, with
no trailing newline. Both keep the active page unchanged.


## Space action follow-up

Set Profile uses the captured sorted submenu, checked current profile and
New Profile… entry. Named profile selection opens the existing confirmation
directly, with its captured Space identity retained across inactive-Space
invocation. Cancelling changes nothing. Manage Spaces uses the existing Library
host. Cloud Share Space is omitted. Live Folders contains only the observed
GitHub entry; its integration and Live Folder context menu remain missing.

Delete uses the observed 450×274 confirmation, text, keyboard hints and button
ordering, with Echo's own icon. The last Space is preserved pending a reference
for its replacement flow. Delete records a Space-specific archive transaction;
Undo/Redo never replace the whole session or revert unrelated data.

Export's source/destination semantics are inferred from installed bundle
`TurnSpaceIntoFolderOptions` fields sourceSpaceID/destinationSpaceID,
`turnSpaceIntoFolder` / `undoTurnSpaceIntoFolder` reducers and the “Turn into
Folder” Undo label. The inspected picker lists all Spaces and disables its
source. The model moves source contents into a named folder, preserves tab,
folder and split identities, and restores the removed Space on Undo. Later
navigation and unrelated destination tabs survive. Changed container structure
invalidates Undo atomically. Cross-profile exports adopt destination storage;
the UI immediately retires old documents, including on Undo, before processing
late engine events. Cookies and native documents never transfer between profiles.

No live Delete or Export execution was attempted, as AGENTS.md prohibits live
Space removal. Destructive behavior/focus and export feedback remain unverified;
no toast has been invented. Six core regressions and six GPUI regressions cover
these transactions, cancellation, inactive targets, storage replacement and
stale-event rejection. The existing profile confirmation presentation remains
partial. Native Echo inspection exercises menus and Delete cancellation only.

Space follow-up validation: full suite 957 browser, 201 core, 12 AI and 128
engine tests passed (one AI test ignored). `space-native-run.euTmgm/drive.log`
records CEF initialization and clean exit after profile/Delete cancellation.
Reviewed crops: `pair-space-menu-3x.png`, `pair-space-profile-3x.png`,
`pair-space-export-3x.png`, `pair-space-delete-confirm-3x.png`. Menu row metrics
and the 246 px root match the inspected Echo Test case; cloud Share omission
shortens it one row. Fixture Space/profile names and custom icons differ from
the owner's content. Shared material/font rendering, profile-sheet presentation
and confirmation keycap details remain partial. No live Space was removed or
profile assignment applied.


## Native page context adapter

`arc-page-link.png`, `arc-page-image.png`, `arc-page-blank.png` supply the item
wording/section order. `arc-page-input-context.png` additionally records the
editable-field menu, including native Look Up, Password, Language and Writing
Direction actions. This extended editing menu is not implemented: existing CEF
editing/spelling commands are preserved and Inspect appended. Media menus,
Speech and long-selection presentation remain incomplete. macOS adds Summarize,
Writing Tools and Services automatically through the native CEF host. Their
complete interaction states remain unverified.
No owner extension row is copied.

CEF's MenuModel owns native tracking/material; its handler sends browser actions
without retaining ContextMenuParams or borrowing the GPUI application. Native
commands are invalidated by the existing document epoch (including same-URL
reloads), dismissal and view retirement/transfer. Queued context input is not
retargeted as metadata. The browser validates the active owned page, profile,
generation and captured document URL before acting. New-window links inherit
the captured profile without changing the source window's selection. Split
links use the existing draft path, keeping original Pinned/Favorite items.
Little and Incognito use their existing isolated window models.

Copy Link/Image Address preserves the exact target URL. Copy Image retrieves
through the source CEF request context and returns a bounded PNG. A weak native
view lifetime, document epoch, UI request identity, timeout, active document and
later browser-copy cancellation guard delayed replies; no toast is invented.
Save Link/Image/Page remain disabled pending their feature backends (ordinary
download admission is still gated); Capture Image, Cast, Translate and Copy Link
to Highlight also remain unavailable. Inspect opens the existing authenticated
Elements picker; revealing the original clicked node across dock reflow and
cross-origin frames is not implemented.

Eight GPUI regressions cover exact copying, stale documents/generations,
background focus, pin-preserving split creation, profile-preserving windows,
Little/Incognito routing and asynchronous image-copy cancellation. An engine
regression covers native menu invalidation on reload/retirement; existing
callback-transfer coverage now includes context input.

The remaining cloud Share Split View placeholders are removed from both pane
and sidebar menus using the previously inspected Split View references. Local
pane sharing still requires completing the nested system-service menu.
The follow-up four-pane Arc session stopped at claim with “expected one new
Arc window, got 0”; no input followed that failure and no window was claimed.
The earlier two-pane session completed and closed its owned window/tabs.


Native page validation: `page-native-run.Ph7wlq/drive.log` records exact link
copying, background opening, native image/blank/editable menus, a two-pane split
and clean CEF shutdown. `echo-page-image-clipboard.txt` confirms PNG clipboard
data from the 80×80 bitmap fixture. The earlier SVG fixture did not produce a
clipboard image; SVG copy remains incomplete. Initial keyboard-selection and
SVG retries are diagnostic failures, not successful evidence. The successful
harness uses observed menu-row positions and a known clipboard sentinel.

Reviewed Arc-left / Echo-right crops: `pair-page-link-3x.png`,
`pair-page-image-3x.png`, `pair-page-blank-3x.png`, `pair-page-editable-3x.png`.
Native blank-menu metrics/material/font match; Save/Cast/Translate are visibly
disabled while their backends are unavailable. Arc's proxy-extension row is
omitted, shortening the link/image panels. The missing Speech submenu also
shortens the link panel. Editable menus visibly differ and remain partial.
`MainMenu.nib` in Arc's Base.lproj contains Speech, Start Speaking, Stop Speaking
and their AppKit selectors; this supplies exact copy for a later native speech
adapter without needing an owner-window interaction.

Full validation before the two additional window-routing tests: 987 browser,
208 core, 12 AI, 136 engine, 2 bundle and 4 importer tests passed; one AI test
was ignored. All eight page-context GPUI tests and the new engine regression
also passed together in `page-tests-5.log`.
