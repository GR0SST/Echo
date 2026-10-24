# Toast reference — Arc 1.170.0, 2026-10-24

Source: /Applications/Arc.app/Contents/MacOS/Arc, Toasts/ToastController.swift
and Toasts/ShinyToastView.swift string/comment groups. Resource bundles:
ARC_SidebarBanner.bundle and ARC_StorableSidebar.bundle; they do not contain
English Localizable.strings for these messages. arc-binary-strings.txt preserves
the extracted source block (lines 47429–47720). No branded assets were copied.

Observed on the fresh Echo Toasts Oct24 test Space:
- Copy: “Copied Current URL”; content-sized 203×48, 16 px right/19 px top,
  12 px corners, 14 px semibold, trailing share symbol on 32×28 inset surface.
- Pin through Command Bar: no toast with visible sidebar. Hidden sidebar:
  “Pinned Tab to “Echo Toasts Oct24”” / “Un-Pinned Tab from “Echo Toasts Oct24””.
  Text-only, 42 px high, 4 px top/right inset. No leading checkmark.
- Copy entry grows around its center and fades in; the captured sequence
  starts at roughly 75% scale and reaches full size. Copy disappears within
  the captured ~5 second sequence. Existing three-second lifetime is retained.
- The guard sees plain toast windows as front Arc windows. Every next input
  waits for the owned main window to become front again. Cleanup closes only
  the owned window after notifications clear.

Local inventory from source groups (available actions / missing feature):
- Undo action shortcut hint (“. ⌘Z to Undo.”): generic action feedback missing.
  Clear Today observed/implemented: “Cleared Tabs! Use ⌘Z to undo.”, 48 px
  high, 4 px inset, with a 28 px-high inline keycap.
- Copied Current URL / clean URL / item links / clean links / quote URL.
- Pinned Tab to / Un-Pinned Tab from / pinned to folder.
- Auto archived %d tabs; Tab/Folder restored to; archive cleared.
- New Space Created (background creation); New Tab Created in %@ (background).
- Cannot Add New Pane.
- Pointer lock / full screen exit instruction.
- Screenshot Copied to Clipboard; Capture in Progress; capture saved/failed.
- Import start / success / partial / failure (import missing).
- Translation complete / revert / error (translation missing).
- Battery Saver Mode Activated / Deactivated (feature missing).
- Shortcut tips: sidebar, capture, copy links, Site Search (education missing).
- Download renamed / reverted is Arc Max Tidy Downloads, not ordinary download
  completion. No ordinary “download finished” toast in this source block.
- Boost installed / developer info copied (other features).
- Arc update-feed, account, sync, referrals and cloud sharing through Arc servers
  are excluded. Provider-direct Arc Max features remain in scope; their feedback
  awaits the corresponding feature implementations.

Sidebar auto-archive education bundle copy:
“Arc archives tabs that you haven’t used in the past 12 hours. You can adjust
this in %@Settings%@.” Title/dismiss label are compiled short strings; the
requested title is “We cleaned up!”. No safe live trigger was found for its
layout/count/actions, so the banner has not been guessed or marked complete.

Remaining: exact exit/stacking/hover behavior, actionable accessories, generic
Undo toast, background-created items, drag pinning, folder restore, quote link,
unsupported feature notifications and sidebar banners. Native web notification
permission/delivery is not listed in section 18 and was not added.

## Echo validation

Four GPUI regressions cover content bounds/page focus, hidden-sidebar pin
feedback and Undo, Clear Today feedback and Undo, and automatic-archive notice
replay suppression across windows. The full workspace run passes 1,048 tests.

Native CEF checks use an isolated fixture profile. Copy, hidden-sidebar pin and
unpin, and Clear Today/Undo are captured at 1000×760. A separate 61-second run
ages one inactive fixture tab and verifies the real profile timer: one new
archive entry and a passive 42 px notification. Saved historical entries are
not announced on reopening the fixture. Logs confirm on-screen, non-key,
mouse-passthrough child windows and orderly CEF shutdown.

Evidence: `target/parity/toasts/pair-copy.png`, `pair-pin.png`,
`pair-unpin.png`, `pair-clear.png`, combined `pairs.png`, and
`echo-auto-archive.png`. The pairs are zoomed 2× and visually inspected; native
window shadows are enabled through the isolated platform interface. Exact font
metrics/material/animation fidelity remains partial, as stated in section 18.
