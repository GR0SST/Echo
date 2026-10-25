# Extension UI qualification

Echo owns the extension interface in GPUI. The source-built CEF supplies the
catalog, verified Store installer, actions and command preferences. See
[CEF154_EXTENSIONS.md](CEF154_EXTENSIONS.md) for that boundary and
[CEF_BUILD.md](CEF_BUILD.md) for canonical distribution selection.

## Common controls

The Site Control Center displays every enabled action in four columns of
48×32 tiles, with 8 px gaps, 16 px icons and a trailing Store tile. Pinning is
profile-local toolbar placement; unpinning keeps the action available in the
grid. Toolbar pins appear with Show Toolbar (⌘⇧D). Context actions use the
existing GPUI menu, including keyboard selection and profile/target guards.

`browser://extensions` is a GPUI management page. The measured Arc reference
has a 56 px header, 400×160 cards, 12 px card gaps and 36 px action buttons.
Search filters the native catalog; Escape clears and closes search. Details
uses a real internal tab URL, with native enable/disable and confirmed removal.
Enabled state changes only when Chromium sends its catalog update.

The header's Developer mode toggle persists per profile. It reveals actual
version/ID metadata. Developer tools are a separate parity row: Load unpacked,
Update, worker inspection, permission tools and Pack are not advertised until
their native operations are implemented and qualified.

Settings → Shortcuts lists each extension command with its own manifest
label (or command identifier), rather than labelling every command as an
action. Recording updates Chromium's command preferences; clearing removes the
binding. Browser shortcut conflicts are rejected. Chromium does not accept
Option/Alt combined with Command or Control; the UI explains that restriction
before submitting it. Runtime Control bindings use `Ctrl`, and named keys use
Chromium's spellings such as `Left`, `Space`, `Comma` and `Period`.

Command Bar entries use `Extension NAME`, with the extension's icon on a chip.
The Store route opens the actual Chrome Web Store. The bridge retains normal
manifest, signature, policy and permission verification and requires an explicit
confirmation in GPUI.

## Native evidence

The owned profile is under
`target/parity/extensions-ui/qualify/runtime/browser-drive-profile`; its two
Spaces are Echo Test and Echo Test Isolation. No live owner Space was deleted
or reassigned. All final scripted gates use the shared screen lock, a hard
process timeout, `require-focus`, and strict zero exit. The temporary gate
wrapper also rejects automation `aborted`, `rejected` and `unknown command`
messages. `requirefocus` is not a valid automation command; early exploratory
captures using that spelling are not acceptance receipts.

The framework is canonical generation `20261025T192350Z-5008`, SHA-256
`55a60b71fd32579a392ee53f337a908cecb7b0f012971d6ad3317b4a32ec82aa`.
No Chromium rebuild is required for this GPUI pass.

The following files were inspected under `target/parity/extensions-ui/`:

| Check | Arc reference | Echo evidence |
| --- | --- | --- |
| Manager/card geometry | `arc-reference/manage-populated-developer-off.png` | `qualify/common-manager-normal.png` |
| Developer metadata | `arc-reference/manage-populated-developer-on.png` | `qualify/common-manager-developer.png` |
| Removal sheet | `arc-reference/manage-remove-confirmation.png` | `qualify/popup-fixed-sheet.png` |
| Context menu | `arc-reference/extension-context-menu.png` | `qualify/common-context.png` |
| Command Bar entry | `arc-reference/command-extensions.png` | `qualify/common-command.png` |

Real Store removal/reinstallation of uBlock Origin Lite 2026.1021.1354 passed:
`qualify/store-removed.png`, `store-install-prompt.png`, `store-installed.png`
and their engine logs. The final Store page showed Remove from Chrome.
Dark Reader 4.9.133 and uBOL subsequently operated on
`https://www.rfc-editor.org/rfc/rfc2606.html`.

`qualify/settings-action-shortcut.png` shows recording the uBOL action shortcut.
`shortcuts-and-isolation-engine.log` records persisted Alt+8 (action) and Alt+9
(picker), both exercised natively. `shortcut-action-popup.png` and
`shortcut-named-picker.png` show their results. `common-reader-light.png` and
`common-reader-dark.png` show actual page changes. `common-isolated-grid.png`
and the same log show an empty second profile, followed by restoration to the
first profile.

## Remaining presentation gates

The fresh Arc action reference opens a small titled window from both the grid
and the Command Bar: `arc-reference/{scc,command}-dark-reader-window.png`.
Its observed frame is 276×612 (about 32 px of titlebar above the extension),
positioned 40 px right and 10 px below the retained browser's top-left.
The borderless action host still needs that window presentation and native
close/lifetime qualification. Arc's Open Extensions shortcut opens the SCC with
the first action highlighted; that keyboard entry point is also outstanding.
These gaps keep the combined extensions parity row partial. Developer tooling
remains a separate task after the common controls.
