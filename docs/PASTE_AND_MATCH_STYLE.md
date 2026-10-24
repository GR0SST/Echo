# Paste and Match Style

Edit → Paste and Match Style and Shift+Command+V use one browser action.
The installed Arc 1.170 menu contains this label and action; its compiled
`Base.lproj/MainMenu.nib` was inspected without changing an Arc window.

GPUI text fields insert the clipboard's plain text through their existing
selection, single-line normalization, IME and undo path. The field consumes
the action even when empty, preventing it from reaching an underlying page.

For native content, the action captures the current page or DevTools frontend.
The deferred request validates the original profile, tab, URL, host attachment,
visibility and responder intent before calling CEF 154's
`CefFrame::PasteAndMatchStyle` on that browser's focused frame. A focused iframe
therefore remains the destination. Library's retained native preview is eligible;
Spaces and hidden pages are not. Inspector IDs are resolved to their own content
browser, never to the inspected document.

The owned request retains the exact native browser, context and runtime. Logical
view closure or shutdown invalidates it. Busy CEF dispatch retries revalidate
focus; no application, model, Pages or engine borrow survives native execution.
The command does not read clipboard content into browser diagnostics or modify
stored tab/session metadata.

## Validation boundaries

Regression coverage exercises GPUI selection/Unicode/newline replacement and
undo/redo, native split-pane routing, the CEF shortcut path, Library visibility,
separate inspector routing, newer UI intent, and reentrant native dispatch.

The full workspace suite passed with 1,039 tests. A macOS bundled-app check used
an owned loopback fixture with a bold, colored HTML clipboard sample. Ordinary
Paste preserved its markup; the new shortcut and Edit menu inserted plain text
with the destination's color/font. A cross-origin iframe received the text in
its own editor. The same document and iframe UUIDs survived Library navigation;
clicking native preview after Downloads search left the query unchanged. The
Command Bar consumed plain-text paste without navigating or editing the page.
Screenshots and fixture DOM reports were inspected; the owned QA app quit cleanly.

Windows/Linux native behavior, native DevTools editor behavior, platform-specific
menu enabled states, additional clipboard formats and exact visual parity require
platform acceptance. Static
inspection of the installed Arc menu is not a native Arc interaction test.
