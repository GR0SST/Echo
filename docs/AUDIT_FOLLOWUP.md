# Site controls audit follow-up

Installed Arc 1.170.0 is the reference. The audit brief is retained at
`/Users/grosstgrosst/.t3/worktrees/ChromiumFork/t3-85848ccf/target/parity/reaudit/AUDIT.md`.
Evidence below is relative to this worktree's `target/parity/site-controls/`.

## Developer Mode (#14)

Guarded `audit-developer-r5/` captures the Settings heading, 33 px Developer Mode
row, 32 px hammer circle and On/Off state. Clicking the row toggles without
closing SCC. Control-D changes the toolbar too. `audit-toolbar-r2/` records the
actual tooltips: Copy Link, Portrait Capture, portion capture, Console, Network,
Inspect, Extensions and Split View. Each tooltip window was separately claimed.
The fixture's original On state was restored and owned windows closed. Two fixture
tabs were archived in the final successful toolbar probe; one tab from an earlier
guard-aborted probe remains in Echo Test. Its exact-host cleanup script is ready
(`arc-clean-audit-tabs.sh`), but macOS locked before that guarded cleanup could run. An early wrong coordinate opened an extension popup; no extension setting
was changed. Earlier unqualified captures are retained only as trace evidence.

Echo stores Developer Mode host choices in the local profile, independent of
ports and paths. The SCC row, Control-D/CEF shortcut, menu and Command Bar share
one command. Its 40 px blue toolbar retains the sidebar address/navigation and
uses existing capture and DevTools commands. No engine source patch is needed.

`echo-audit-native-r2/` checks the real CEF bundle, on/off while SCC remains open,
and clean shutdown. Reviewed 2× crops:

- `audit-pairs/developer-row-arc-echo-before-final-polish.png`
- `audit-pairs/developer-toolbar-arc-echo-before-final-polish.png`

These pairs precede final icon/spacing corrections. Those corrections are not
native-qualified: macOS subsequently became locked (`com.apple.loginwindow`),
and guarded inspection stopped before creating a window or sending browser
input. Portrait capture, extension execution, localhost default detection,
split variants and exact glyph/material/motion fidelity remain partial.

## Window cleanup

The earlier cleanup reduced ten browser windows to four: one each for Grosst,
Work, Яремко and Playable. Cmd-N produced exactly one additional window (4→5→4).
No Spaces or tabs were deleted during that cleanup. The later qualified AX/CG
inventory is in `final-cleanup/`; its header montage was inspected. A subsequent
read-only CG check after the audit still found the same four owner window IDs
32153–32156. The offscreen, non-AX surface 32164 is excluded from browser counts.
Final header reinspection is blocked by the locked macOS session. The restoration
check found AX reporting zero while CG retained all four owner windows; the
orchestrator confirmed they remain intact. No restoration windows were created.

## Folder menu (#16)

The installed reference is `arc_folder_context_Dark.png` beside the audit brief.
Echo now uses a 282 px minimum width and 247 px minimum root height. Its bounded
native menu material is installed in the parent content view, above CEF and below
the GPUI overlay, and removed on dismissal. An expired overlay cannot reinstall
that material. This fixes the opaque appearance over a native page.

`echo-audit-native-r2/folder-menu.png` verifies the page-backed material and
working dismissal with clean CEF shutdown. The reviewed 2× comparison is
`audit-pairs/folder-arc-echo-before-final-polish.png`. It precedes the final
one-pixel height and tint adjustment, which need another native capture after
unlock. The GPUI menu test checks 282×247 geometry. Arc's Move to submenu,
curly-quote wording and exact edge/material calibration remain partial; the
existing destination picker is retained. Arc-hosted sharing stays out of scope.

## Advanced footer (#17)

Guarded `audit-advanced-r1/` shows that More Settings closes Preferences and opens
`arc://settings/` in the owning browser. `audit-export-r1/export-data.png` shows
the native save panel and Markdown filename; it was canceled and the owned
Preferences window closed. No export of owner data was written. The installed
bundle's description of Reset Translated-site Prompts specifies automatic and
never-translate choices for languages and sites; that action was not invoked in
live Arc, preserving the owner's translation choices.

Echo's three footer buttons now have handlers:

- More Settings opens `browser://settings/` as GPUI in the normal owner, closing
  Preferences. The observed Settings header and local profile card are rendered;
  Arc's account/synchronization section is excluded. Global navigation/search and
  account-only card navigation are unfinished; this is not a full global Settings
  implementation, and no Chromium WebUI is used.
- Export Data presents the platform save panel for `Echo Data Export.md` and
  writes the persistent sidebar: Space/folder headings and plain URL bullets,
  profile Favorites and original pinned destinations. Today tabs, sessions,
  credentials and browsing history are not included. The write runs off the UI
  thread; cancellation leaves the model unchanged.
- Reset Translated-site Prompts resolves the selected existing profile through
  the retained engine storage boundary. CEF restores registered defaults for the
  automatic/never-translate lists and prompt counters, after checking mutability
  of every registered preference. No profile fallback or engine-source patch is
  used. Availability in the bundled CEF runtime, persistence and managed-policy
  behavior still require the isolated native check. A missing owner/context
  rejects the operation; global action availability in that state is unfinished.

The engine mapping follows Chromium's
[registered preference names](https://chromium.googlesource.com/chromium/src.git/%2B/refs/heads/main/components/translate/core/browser/translate_pref_names.h)
and [TranslatePrefs reset implementation](https://chromium.googlesource.com/chromium/src/%2B/master/components/translate/core/browser/translate_prefs.cc).
Arc's installed bundle remains the UI reference.

GPUI tests cover export write/cancel, selected-profile reset routing, More Settings
ownership and absence of a CEF settings renderer. The core export test checks
nested hierarchy, original pinned URLs and model immutability. Full workspace
validation: `audit-full-r3.log` (1,137 browser tests; 1,562 total passed, one ignored).
Footer native verification/pairs and final developer/folder polish are blocked by
the locked macOS session. The orchestrator authorized publishing these audit commits with native
validation explicitly pending. Publication does not qualify the final visual
corrections or footer runtime behavior. Arc must remain untouched while locked.


## Validation after unlock

The pending audit pieces were recaptured in the real CEF bundle and compared with
Arc after macOS unlocked. [Native validation](NATIVE_UNLOCKED_VALIDATION.md) records
Developer Mode on/off, the final folder geometry/material capture, footer export
save/cancel and owning-browser More Settings, and the two native-discovered fixes
for scripting property counts and CEF's required preference-error output.
`target/parity/native-unlocked/pairs/` contains the reviewed crops; its source
manifests identify the reused same-version Arc folder reference. Remaining visual,
managed-policy/no-owner, extension and broader settings gaps stay partial.

The exact-host leftover fixture was explicitly archived through Archive Tab;
closing its page had preserved its sidebar entry. The shared Echo Test Space and
all owner Spaces/Favorites were preserved. Earlier locked-session notes above are
historical and are superseded for the qualified checks by this unlocked report.
