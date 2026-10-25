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


## Library and site refresh routes

Arc's runtime Window menu uses "View Media…" and "View Boosts…", while the
compiled nib retains older base labels without ellipses. The inspected native
references are `arc-library-menu-reference/media.png` and `boosts.png`. Both
leave the selected page visible to the right, translated without reflow. Echo
now routes both menu entries and Command Bar actions to the existing sections;
Boosts also opts into the existing retained-page viewport contract.

The View menu and Command Bar clear-site routes use the existing engine API:
origin/profile-scoped storage clearing or context cache clearing, followed by a
reload only if the same document epoch remains. Internal/hidden pages reject
the actions. Three new GPUI tests cover menu/Command Bar routing, retained live
Library pages, origin scoping and unavailable internal/hidden contexts.

`arc-refresh-reference/requests.json` confirms the unique fixture cookie before
clearing, its absence on the subsequent requests, and three main-document loads
across the initial page and the two commands. The final inspected page shows
"Echo Menu Refresh 3". The AX window-title polling exceeded eight seconds in one
stage, so this session is not an animation/timing measurement. No confirmation
dialog appeared in the all-layer inventory. Own fixture tabs and windows were
closed; cleanup errors and remaining-owned-window inventories are empty.

Native Echo acceptance in `echo-library-refresh/` passed both clear actions:
its request log has the initial owned cookie, no cookie on the reloads and
exactly three document loads. Media and Boosts then retain that third loaded
page. The inspected equivalent-page pairs are
`arc-library-refresh-pair/media.png` ↔ `echo-library-refresh/media.png` and
`arc-library-refresh-pair/boosts.png` ↔ `echo-library-refresh/boosts.png`.
Both preview left edges are at x=417 px (including the 1 px inset); the heading
and input align without a reflow. Inspected 2× crops are
`pairs/arc-boosts-live-preview-zoom.png` and
`pairs/echo-boosts-live-preview-zoom.png`. Arc's existing Boost records were only
read, and the isolated Echo profile has an empty list. No owner Boost was
modified. Developer Mode's additional toolbar/site-row presentation and
remaining platform acceptance are partial.


Full local validation for the Library/refresh group passed 1,072 browser, 237
core, 148 engine and 15 AI tests, plus importer/bundle tests. One existing
native-credential integration test remains ignored. The earlier full run caught
one stale Window-menu-order expectation, now updated from the captured runtime
menu. No handoff test waiver was used. Native acceptance exits zero after CEF
teardown. A refreshed fixture changed its title after timeout; its leftover tab
was subsequently located by exact URL and archived in
`arc-clean-refresh-reference/`, with an inspected clean Space and no owned
windows remaining. No other tab or Space was archived/deleted.


## File → Share

The inspected Arc reference is `arc-share-reference/share.png`: the standard
macOS URL share picker, with page-title preview, Reading List, the installed
system destinations, Copy and Edit Extensions. No destination was selected.
Echo's File and nib-derived "Share URL" Command Bar routes now use that native
presenter through the isolated platform interface. The original page identity,
URL and document epoch are validated before presentation; replacing/destroying
a picker happens outside GPUI and CEF borrows.

Native menu and Command Bar acceptance is in `echo-page-share/`, including
Escape cancellation, reopening after cancellation and zero-status CEF teardown.
Both settled picker bodies are approximately 240 × 262 px plus the top arrow.
The native URL metadata title resolves asynchronously. Echo's picker center is
6 px to the right of Arc because its existing sidebar is 12 px wider; both are
centered over their page content. Inspected 2× crops are
`pairs/arc-page-share-zoom.png` and `pairs/echo-page-share-zoom.png`.

Four GPUI tests cover page retention, Command Bar dismissal, same-address
reload rejection, owner-close cancellation and alignment with the actual page
bounds after collapsing the sidebar. Windows/Linux currently expose
no native presenter and the action is unavailable there. The row remains
partial. The exact-ID cleanup check for the extra window opened by Arc's
Copy Arc Info found it already gone after its owned parent closed; no unknown
or restored windows were closed by that cleanup (`arc-info-reference/cleanup-popup-after.json`).


The final native run is `echo-page-share-final/`: menu and Command Bar
presentation, cancellation/reopening and the collapsed-sidebar anchor all pass,
with inspected screenshots and zero-status CEF teardown. The nonempty native
anchor rectangle avoids AppKit's whole-view fallback. The final local suite
passed 1,083 browser, 239 core, 148 engine and 18 AI tests plus importer/bundle
checks (one existing native-credential integration test ignored).

The later Info reference identified the extra surface as Arc's noninteractive
"Copied Arc Info!" toast. With an active disposable page, the clipboard contains
structured JSON; only redacted field names were saved in
`arc-info-reference/info-format-redacted.txt`. It includes account, stable-device
and Sentry fields that must be omitted from Echo. The passive surface closes
with its owned parent, and the final cleanup inventories are empty.


## Copy Echo Info

Help → Troubleshooting and Command Bar now copy explicit local JSON diagnostics:
Echo/Chromium/OS versions, local profile count and the current window's retained
renderer inventory. This is an explicit clipboard action, with no telemetry,
account fields, stable device identifier, page URLs or profile names. Arc's
redacted source schema is in `arc-info-reference/info-format-redacted.txt`;
Echo's private-profile output is `echo-copy-info-final/info.json`.

The compact confirmation matches the captured 40 px height and 19 px top/right
inset, content-sized width, 13 pt semibold label and purple gradient. Its width
is 136 px versus Arc's 130 px because “Echo” replaces “Arc”. Regular link-copy
toasts retain their existing style. Compared captures are
`arc-info-reference/info-popup.png` and `echo-copy-info-final/copied.png`;
menu and Command Bar native acceptance both pass, with zero-status shutdown.
Fine font/shadow optics, other-platform OS version strings and a combined
all-window renderer inventory remain partial.

Two GPUI tests cover JSON scope/counts, retained-page behavior, Command Bar
dismissal and the compact toast without a Share action. The final full local
suite passes 1,085 browser, 239 core, 148 engine and 18 AI tests plus importer
and bundle checks; one existing native-credential integration test is ignored.


## Reveal Echo Data

The installed reference opens inside `~/Library/Application Support/Arc` in
Finder, rather than selecting that folder in its parent. The captured source
is `arc-reveal-data-cg/finder.png`. Owned Finder setup/result windows are closed
by exact CG IDs, leaving the previous Finder windows intact; the final owned
inventories and Arc cleanup errors are empty.

Help → Troubleshooting → Reveal Echo Data and the matching Command Bar command
open Echo's startup-resolved data directory through GPUI's platform URL opener.
This honors `BROWSER_DATA_DIR` and keeps profile migration/environment lookup
out of the action. The action dismisses drafts and suppresses deferred page
focus restoration so the platform file manager can take focus. No files are
modified by the action. Three GPUI tests verify directory URL escaping, retained
page identity, no reload/creation/late focus, and use without a browser window.
The final native run is `echo-reveal-data/`: its inspected Finder pair against
`arc-reveal-data-cg/finder.png` uses the same 920 × 492 px OS window, with the
private test data directory substituted for Arc's directory. Menu opening,
exact owned-window cleanup and zero-status CEF shutdown pass. The original
Finder window is preserved. The first two harness attempts needed fixes for
Cocoa metadata serialization and asynchronous Finder closure; the second
also hit the CEF shutdown watchdog while the harness had not restored Echo
focus. The corrected run exits zero, and no failed run is counted as acceptance.

The original no-window test incorrectly dispatched after closing the final
browser, which correctly begins shutdown. Separate Command Bar and freshly
initialized no-window tests now exercise the intended lifecycle. Native folder
opening on Windows/Linux remains unverified.


The corrected full local suite passes 1,095 browser, 241 core, 151 engine and
18 AI tests plus importer/bundle checks (one existing native-credential
integration test ignored). No handoff waiver is used.


## File → Capture… entry

The nib/runtime File order places Capture… after Share… and before full-page,
portrait, Save and Print. The menu now enters the existing scoped area-capture
implementation (upstream 9898165), including through the overlay action root
when a Command Bar draft owns focus. No new renderer or page navigation is
created. Full-page and portrait entry points remain missing.

Two new GPUI tests select the File item from the retained page and a typed
Command Bar draft, verify capture focus and cancellation, and reject renderer
creation/navigation/reload or late native focus. The full local suite passes
1,097 browser, 241 core, 151 engine and 18 AI tests plus importer/bundle checks;
one existing native-credential integration test is ignored. Native File entry,
draft dismissal, Escape and zero-status CEF shutdown pass in
`echo-file-capture/`.

The fresh guarded Arc File reference is
`arc-file-capture-reference/capture.png`, compared with
`echo-file-capture/capture.png`; both prompt bodies are 355 × 67 px and 35 px
above the page bottom. Inspected prompt crops are in `pairs/`. This is entry
integration, not complete capture visual parity: Arc initially says “Drag to
capture a portion of this page” with a gray page, while Echo initially queries
an invented center pointer, outlines an element and says “Click or drag…”.
Arc's instruction changes after interaction (`arc-capture-drag-reference/`).
Initial pointer/hover and instruction state, backdrop treatment and remaining
cursor/glyph/motion details need a follow-up. Arc cleanup inventories are empty;
no capture was saved to its Library.


## Standard editing submenus

The installed MainMenu nib and `arc-edit-menu-reference/` define Spelling and
Grammar, Substitutions, Transformations, Speech and Format → Font. GPUI owns
the menu structure and typed actions. The isolated macOS adapter binds the
standard selectors to the current native text responder, with Font Manager
trait tags 2/1 for Bold/Italic and nib-derived key equivalents. AppKit owns
validation, checkmarks and font samples; manually assigning an attributed font
made the Font menu 2 px too wide and was removed. Native calls that can open
panels run outside GPUI application/window borrows. Toolkit action dispatch
captures its actual window, including the Command Bar overlay, rather than
implicitly targeting the page window.

The inspected Arc/Echo submenu pairs are `arc-edit-menu-reference/<name>.png`
and `echo-standard-edit/menu-<name>.png`; 3× crops are in `pairs/`. Every enabled
state matches. Bodies measure, respectively: Spelling 268 × 141 px,
Substitutions 162 × 189, Transformations 140 × 82, Speech 122 × 58 and Font
136 × 82. The native uppercase result is `MISPELLED WORDS AND STYLED TEXT.`;
both browsers copy identical `<u>` HTML after Underline. Source evidence is
`arc-standard-edit-actions/` and native Echo acceptance is `echo-standard-edit/`.
Earlier clipboard assertions read the previous Copy result; the accepted
runners wait for clipboard change count and expected contents before comparing.

Standard panels are floating native windows at CG layer 3, so the reference
runner separately records/claims them while retaining the shared guard for its
owned root. Spelling and Grammar is 431 × 289 px and Substitutions is 360 × 150
in both apps. Only each directly-created panel is closed, without selecting
replacement/correction controls. Panel positions retain the OS's existing
per-app frame restoration. No preference toggle or speech playback is claimed
as tested. Three GPUI tests cover menu dispatch without navigation, preserving
the actions after sidebar Undo labels rebuild menus, and routing a Command Bar
draft's request to its own overlay without closing the draft or focusing a page.

Other native input contexts, preference toggles and hardware shortcut acceptance
remain partial; Windows/Linux do not expose these macOS-only standard menus.
Edit's remaining Find grouping/actions are a separate unfinished row.


Final validation for this group passes 1,105 browser, 241 core, 153 engine and
18 AI tests plus importer/bundle checks, with one existing native-credential
integration test ignored. The final scope-corrected bundle passes the native
actor with zero-status CEF teardown. All five 3× submenu pairs and both native
panel pairs were inspected. `arc-menu-key-probe/` additionally verifies the
physical Command-Shift-semicolon spelling shortcut in the owned Arc editor;
remaining hardware acceptance stays explicitly partial.
