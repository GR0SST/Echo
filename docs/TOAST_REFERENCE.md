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
- Copy entry uses an underdamped spring (measured ζ=0.694, ω=15.42 rad/s),
  overshooting about 4.8% and settling by 650 ms. Dismissal begins 4.986 s
  after onset, then shrinks/fades toward the top center over about 200 ms.
  Echo uses a five-second lifetime plus the measured exit. Reduced motion
  presents/dismisses immediately. Replacement overlaps the same corner; it
  does not form a vertical stack. Each outgoing toast retains its own text.
- The copy accessory opens the native macOS share menu (AirDrop, Notes,
  Freeform, Journal, Shortcuts observed); no destination was selected.
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

Remaining: hover duration, actionable accessories, generic
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

## Round 2 lifecycle evidence

Bounded native `screencapture -v -V 8` recordings (12-second hard kill):
`target/parity/toasts/r2/arc-copy.mov` and `arc-stack.mov`. Frame PTS and
purple-surface bounds are saved in `copy-motion.json`; `motion-fit.json`
records the fitted spring and exit. `pair-lifecycle.png` compares 2× Arc/Echo
entry, settled, and exit crops; `echo-copy.mov` preserves the native Echo check.
`exit-timeline.png` shows zoomed exit
crops. `arc-accessory.png` records the native share menu.

Two additional GPUI regressions cover five-second lifetime, intermediate exit
geometry, overlapping replacement content, and closing all incoming/outgoing
child windows when the browser closes. The full workspace run passes 1,196
tests. Native toast windows are borderless so overshoot padding exposes no
AppKit frame rim. Toast frames redraw only their own
window through a read-only browser snapshot, preserving main-window feedback
expiry and avoiding CEF/main-window frame churn. Sidebar banner layout remains
unverified; no banner or count/action layout was invented.
