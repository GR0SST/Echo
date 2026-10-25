# Main menu parity validation

The installed Arc 1.170.0 compiled `MainMenu.nib` is the source for menu order,
labels, separators and default bindings. Evidence and the decoded, read-only
catalog are under `target/parity/main-menu/` (ignored, no Arc assets shipped).

## About and Essential Keyboard Shortcuts

- Arc About: 360 × 262 px; standard 32 px titlebar, centered 50 px product icon,
  13 px product title, 11 px version/engine/copyright/acknowledgement text.
  Echo uses its own icon, Cargo package version and embedded CEF Chromium version.
- Credits are generated from resolved host-platform Cargo dependencies and their
  available upstream license/notice files, plus the existing Gemoji, Ionicons and
  CEF/Chromium acknowledgements. This does not assign a source license to Echo.
- Installed documents use an isolated resource-layout interface. Echo's own
  privacy document describes local storage and direct website/provider requests.
- Arc Help panel: 475 × 462 px; 30 px horizontal inset, 73 px row spacing,
  28 px keycaps with 4 px gaps. View All dismisses the panel and opens Settings →
  Shortcuts. Learn the Essential Shortcuts returns to the panel.
- Arc's captured Add Split chord is customized to Shift+Command+E on this install.
  Echo retains the nib's default Control+Shift+=. The reference's proprietary
  title font is replaced by the platform rounded system font; no Arc fonts copied.
- About and Help actions work with no browser window. Repeated actions reuse
  their existing window. Escape/Command+W close the focused utility window.
- Tests cover About/Help reuse and closing, the Help → Settings → Help round trip,
  and cancellation of already queued Command Bar page-focus restoration. Native
  acceptance checks physical Escape from a Command Bar-opened About, click View
  All, click Learn the Essential Shortcuts, and closed-window identities. New
  utility activation is reasserted on its first frame after native child teardown.
- Accepted pairs: `arc-about/about.png` ↔ `echo-group1-final/about-1.png`, and
  `arc-shortcuts-panel/shortcuts.png` ↔ `echo-group1-final/shortcuts-2.png` beneath
  the evidence root. Magnified text/keycap crops are in `pairs/`. Native round-trip
  screenshots and ownership assertions are in `echo-help-roundtrip/`.
- Remaining optical differences: rounded system title font and keycap bevels;
  About link tint. External OS opening of packaged legal documents remains
  unvalidated. These rows remain partial rather than claiming exact parity.

Local pre-rebase validation: `RUST_TEST_THREADS=4 cargo test` passed with 971
browser, 198 core, 130 engine, six importer and 12 AI tests; one existing native-credential
integration test is ignored. The credits escaping test passed as well. Both native
utility acceptance scripts exit with status zero after the CEF quit barrier.
No snapshot/handoff test waiver is used.

## Remaining section 1 audit

This is an active implementation task, not a completion claim. Remaining rows
retain their existing statuses in ARC_PARITY.md. The next groups are document
saving/capture, native editing menus, appearance and site refresh actions,
context exit, Recent Items MRU, Back history, Library routes, troubleshooting,
Developer Mode and remaining in-scope menu integrations. Excluded accounts,
cloud sharing, referral and update/telemetry features must not gain UI.

## Reference ownership

Use the shared Echo Test Space, standard/compact recorded header crops and the
shared guard. Floating Help panels use CG layer 3, so the guard's layer-0 window
inventory needs a supplementary exact-PID/all-layer ownership record. View All
closes that panel before creating the separate Settings window; claim the new
Settings window as the one click-created owned dialog, then close it. Never
send cleanup input after an Arc process-ID change, and never use snapshot
window differences from a previous process to close windows in a new process.


## Recent Items reference and implementation

`arc-mru-panel/` contains a three-tab forward-cycle/commit reference;
`arc-mru-motion/` contains six disposable visited tabs and the resulting five-card
limit. The active page remains on screen while the highlight cycles, and
Control release commits the selection. All fixture tabs were archived; the
cleaned Space image and post-animation window inventory confirm cleanup.

For three entries the panel is approximately 578 × 190 px, centered in the
parent. Each item is 182 × 158 px with 12 px padding, a 158 × 105 px preview and
17 px label row separated by 12 px. Previews preserve viewport width and crop
from the top. The card has a 24 px radius, 16 px outer padding, and a broad lower
shadow; the selected item has an approximately 8 px radius. Five entries use
approximately 942 px width. The transparent native overlay expands horizontally
beyond its parent to accommodate the shadow. The local screen recording in `arc-mru-video/` shows full-size entry and exit;
its variable-rate frames do not resolve an intermediate opacity/scale state.
Echo does not add an unmeasured presentation tween. Finer motion comparison
remains pending.

Echo now keeps window-local visits, presents at most five frozen candidates and
commits on Control release. Forward/backward cycling, Escape cancellation,
hover selection, quick taps and closing the owner have GPUI coverage. The core
model also covers Split survivors, explicit Pinned unload and cross-window
isolation. The existing renderer and navigation state are retained; previewing
does not navigate or reorder the sidebar. A 250 ms capture-readiness fallback
keeps the keyboard usable when a renderer capture stalls. Split thumbnails
currently show the active pane rather than a composed group. Native three-item acceptance is in `echo-mru/`: Control-Tab advances
without selecting the page, releasing Control selects the highlighted existing
page, and the owned child disappears. The script exits zero after CEF teardown.
The pair is `arc-mru-panel/mru.png` ↔ `echo-mru/mru.png`; inspected 2× crops
are `pairs/arc-mru-zoom.png` and `pairs/echo-mru-zoom.png`. The native macOS
modifier-state query also handles a flags-changed event missed during child
attachment. Other backends retain event-driven release. Labels use the existing
grapheme-aware width renderer; thumbnails are top-cropped and downsampled before
their rounded image mask. Split composition, five-item native acceptance,
precise shadow/font matching and finer motion remain partial.

Validation for this group: 1,020 browser, 218 core, 136 engine and 13 AI tests
passed, plus importer/bundle tests. One existing native-credential integration test is ignored.
Eight GPUI switcher tests and six core recent-item tests cover the new behavior;
a thumbnail test verifies top cropping, BGRA conversion and cache reuse.


Recovery safety: do not use window-list differences across Arc restarts as
proof of ownership. Restored windows can appear after an initially empty
snapshot. A failed/ambiguous claim must never feed an unrestricted difference
set into cleanup. Legacy Arc scripts in this evidence directory need explicit
owned-ID cleanup before reuse; `arc-save-dialog-recovered.sh` is unsafe.


## Save Page As

The accepted reference is `arc-save-owned/save.png`; the format choices are in
`arc-save-formats/formats.txt`. Arc's native save sheet offers, in order,
Webpage, HTML Only; Webpage, Single File; and Webpage, Complete (the default).
The format row uses a 46 px label, 8 px gap and approximately 182 × 24 px popup.
The ordinary File routes now follow the nib's relative order and separators.

The engine captures MHTML from the retained CEF document over the local DevTools
transport. Each consumer owns its reply and observer lease, with document-epoch
validation for same-address reloads, explicit source closure, timeout and size
limits. No remote debugging port or reload is used. HTML and CSS are parsed
locally when producing a complete resource folder or HTML-only document.

Filesystem publication creates a new resource folder, preserves pre-existing
resource folders, and atomically publishes the main file after its assets. An
error removes only the new owned folder. Seven GPUI/publication tests, six core
export tests, four engine request tests and one shortcut-layout test pass. The
full run passed 1,042 browser, 228 core and 141 engine tests, plus AI/import/bundle
tests (one existing native-credential integration test remains ignored).

Native acceptance in `echo-save-export/` saved all three formats, reopened
Complete and MHTML with the fixture server stopped, and retained the source
load counter and same-document Back history. Both offline screenshots were
inspected, including CSS imports, SVG and frame content. The Arc pair is
`arc-save-owned/save.png`; Echo format sheets are
`echo-save-export/Complete.html.panel.png`, `Only.html.panel.png` and
`Single.mhtml.panel.png`. Native Cancel preserves the document. The panel starts
expanded in Arc and collapsed in the isolated Echo test preferences.

The physical Shift-Command-S chord did not open a panel under either the
original keyboard layout or a temporarily selected ASCII input source; Echo
remained frontmost. Native File actions work. The GPUI shortcut and CEF
virtual-key conversion have tests; physical-chord acceptance remains partial.
The temporary input source and full clipboard contents were restored. The final
Format label uses its measured native text width and secondary label color;
`echo-save-final/save.png` and `pairs/echo-save-format-final-zoom.png` were
inspected against the Arc Format crop.

Windows and
Linux currently have the shared path prompt with Complete as the default,
without the macOS native format accessory. Save-folder/format persistence and
full Chromium serializer equivalence remain unverified.

New Arc reference sessions use `target/parity/main-menu/owned_arc_session.py`: a
settled process/window baseline, exact owned IDs, no inline restart and no
difference-based cleanup. The old shell scripts are disabled. Successful Save
reference sessions leave no owned windows and remove only this task's fixture
tabs. An unowned Arc Settings window causes a read-only abort.


## Appearance menu

`arc-appearance-reference/appearance.png` is the inspected 269 × 106 px native
submenu: a disabled "Websites, Easels, and Notes will use:" heading, then
Automatic, Light and Dark. The active mode has a native checkmark. The menu and
nib-derived Command Bar titles now use the existing shared appearance setting
and material/color-scheme APIs. The Space palettes remain unchanged. A minimal
macOS adapter supplies checked state because GPUI's menu model lacks it.

Two GPUI tests cover menu color-scheme updates without new renderers/navigation,
the disabled heading, cross-window settings propagation, Command Bar dismissal
and appearance retention through sidebar Undo. Native acceptance is in `echo-appearance/`: Light, Dark and Automatic update
`prefers-color-scheme`, preserve the fixture load counter at 1 and show exactly
one native checkmark. The inspected menu pair is
`arc-appearance-reference/appearance.png` ↔
`echo-appearance/automatic-menu.png`; both are 269 × 106 px. Inspected 3× crops
are `pairs/arc-appearance-menu-zoom.png` and
`pairs/echo-appearance-menu-zoom.png`. The isolated profile is discarded on exit.
Windows/Linux checked-state presentation and native acceptance remain partial;
Incognito retains the pre-existing forced-dark material policy.
