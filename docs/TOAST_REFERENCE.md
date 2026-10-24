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
- Undo action shortcut hint (“. ⌘Z to Undo.”): generic archive feedback wired.
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

Remaining: hover duration, native sharing on Windows/Linux, background Space
creation, archived-folder restore, unsupported feature notifications and sidebar
banners. Round 3 below records the completed existing-action hooks. Native web notification
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

## Round 2 accessory and remaining inventory

The native copy share accessory is wired for URL, cleaned URL, Markdown and
the existing item-link copy handler. It captures cleaned URLs at copy time,
not the later active tab or clipboard contents. The revised full workspace
run passes 1,208 tests. Platform presentation is
isolated in `native/sharing.rs`; macOS uses NSSharingService service discovery
and delivery with a native NSMenu. These selectors are also in Arc’s binary.
The newer rich NSSharingServicePicker popover was rejected in the native check:
Arc exposes a compact service-only menu. The menu is retained by its browser
and released on browser close. Presentation runs from a foreground async task
after GPUI application/entity/window borrows have ended because AppKit enters
a nested tracking loop. A clicked child returns key status to its parent before
tracking begins; a selected destination can then activate its own application.
The menu excludes the Reading List service, matching the observed five entries,
and explicitly requests image visibility on macOS 27 (newly linked apps
otherwise hide menu icons). Native window logs confirm a non-key toast and
working Command Bar input after Escape while the toast is still visible.
Normal-pointer menu anchor precision and hover lifetime remain unverified. Windows/Linux native sharing remains unimplemented.

A GPUI click regression changes Spaces after copying and verifies the platform
request still contains the original URL. Native evidence is
`target/parity/toasts/r2/echo-share-menu.png`, `echo-share-keyboard.png` and
`pair-accessory.png`; the system menu is dismissed without choosing a service.

Unfinished hooks, rather than invented feedback:
- Generic Undo/archive, move-to-Space, background opening and drag pinning:
  the source inventory is known, but additional live flow/accessory checks
  remain. Arc's move message is “Tab moved! Click to go there.”; it is not
  evidence that Echo should immediately switch Spaces.
- Folder pin/restore and quote URL: destination/reveal and selection flows
  remain unverified. A compiled toast string alone does not establish layout.
- Downloads: source messages are Tidy Downloads rename/revert, awaiting the
  provider-direct Max implementation; no completion toast is invented.
- Education banner: `ARC_SidebarBanner.bundle` contains Assets.car and an
  update animation, no nib or layout file. Compiled SwiftUI view/state names
  and copy do not establish count/actions, padding or geometry. The global
  education eligibility preference was already false; it was not modified.

Additional Arc inputs were blocked while an unclaimed agent test window
covered the claimed main window (10205). Guard failures and snapshots are in
`target/parity/toasts/r2/recovery.log` and `recovery-state.json`. No input was
sent past that guard, no owner Space was changed, and no archive setting or
education preference was modified.


## Round 3 existing-action hooks

No new Arc session was needed: plain and copy-with-accessory geometry reuse the
approved pairs and recordings above. Exact sources in `arc-binary-strings.txt`:

| Hook | Arc string source | Echo behavior |
|---|---|---|
| Close/archive single or multiple items | line 47639, `. ^Z to Undo.` (Command key glyph at runtime) | Inline keycap and clickable Undo capture the exact archive transaction; a newer edit cannot be undone accidentally. Keyboard Undo retires the feedback. |
| Move by Command Bar, folder/root move or cross-Space drag | line 32879, `Tab moved! Click to go there.` | Click reveals the moved tab in its destination; stale/deleted or different-profile targets are ignored. |
| Background Command Bar (Cmd+Enter) and CEF background popup | line 47717, `New Tab Created in %@` | Current page remains selected; hidden-sidebar feedback clicks reveal the new tab. |
| Folder creation around a tab and move/drag into a folder | line 47606, `Pinned to ` | Dynamic folder name follows the exact prefix. Ordinary same-section reordering emits no pin feedback. |
| Pin/unpin by drag | lines 47602–47604, `Un-Pinned Tab from` / `Pinned Tab to` | Reuses the verified plain layout and Space-name formatting. |
| Archive restore | lines 47523–47527, `Tab restored to ` / `. Click to view.` | Captured restored tab is revealed on click. |
| Copy Current URL as Quote | command line 33315; feedback line 47618, `Copied Current URL` | CEF reads the explicit text selection through private renderer IPC, copies a cleaned text-fragment URL, and uses the verified copy/share layout. Navigation, stale replies and iframe selections are rejected. No unverified quote-specific sublabel is added. |

Ordinary **download completion is dropped from the hook list**: the extracted
Arc toast group contains only `Download renamed!` and `Download name reverted!`
for Max Tidy Downloads (lines 47511–47514), with no ordinary completion toast.
Downloads use the downloads indicator/Library; Echo must not invent completion
copy. Max rename/revert feedback remains for the later Max implementation.

The foreground-only Space creation route does not emit `New Space Created`:
that inventory entry describes background creation, which Echo does not yet
expose. Archived folders likewise have no restore model/route yet. Import,
translation, capture, battery saver and shortcut education await their owning
features. “We cleaned up!” remains partial with no reference or guessed UI.

Validation: five additional GPUI interaction regressions cover archive Undo
transaction safety, background opening from the Command Bar and CEF, move
reveal, and quote/navigation races. Existing real-drag regressions also assert
folder pin and cross-Space move feedback. Core tests cover text-fragment URL
encoding. The full workspace run passes 1,222 tests. Native evidence lives in
`target/parity/toasts/r3/`: `pairs.png` reuses the approved Arc layout crops;
`echo-quote.png` plus `quote-clipboard.txt` verify native CEF quote copying;
`echo-archive.png` / `echo-undo.png`, `echo-background.png` /
`echo-background-reveal.png`, and `echo-move.png` / `echo-move-reveal.png`
verify native actions. `echo-folder.png` and `echo-drag.png` show creation
around a tab and a real Today-to-folder drag. Logs show clean CEF shutdown.

## Max download renaming

The installed Arc binary string group (local extraction
`target/parity/arc-max/arc-strings.txt`, lines 47511–47514) supplies
“Download renamed!” and “Download name reverted!”, with comments explicitly
identifying Tidy Downloads. The accessory source group uses
`arrow.uturn.backward`. Echo reuses the plain/accessory layouts verified in
`d7ed3a3` above, using its existing vector Undo control. The owner's Arc Tidy
Downloads switch is disabled and was left unchanged: enabling it could send
owner filenames to Arc's AI. The exact download-specific toast visual remains
partial; a generic verified toast is not a direct download-toast capture.

Undo performs a local no-overwrite move and updates the captured completed
record. It does not request another model response. If the original name is
occupied, the undo leaves both files and inventory unchanged.
