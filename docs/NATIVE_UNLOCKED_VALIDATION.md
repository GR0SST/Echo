# Native validation after the macOS unlock

Reference: installed Arc 1.170.0, its bundled `Arc.sdef`, and the qualified Arc
folder-menu capture from the audit. Evidence paths below are relative to
`target/parity/native-unlocked/` in this worktree. Every interaction ran as a
bounded non-interactive script under `/tmp/echo-screen.lock`; Arc used the shared
guard and existing Echo Test header reference. No `tell application "Arc"`, owner
calls, new Spaces, or owner-profile preference resets were used.

## Developer Mode and folder menu

`../site-controls/unlocked-developer/` and `unlocked-developer-clean/` record Arc's
existing exact-host fixture, Developer menu state, Control-D off/on and restoration.
`../site-controls/echo-audit-unlocked/` records the fresh CEF bundle: Control-D,
the SCC On/Off row without dismissal, page offset, folder-menu display/dismissal,
and clean CEF shutdown. Reviewed 2× pairs:

- `pairs/developer-row.png`
- `pairs/developer-toolbar.png`
- `pairs/folder-menu.png`

The 282×247 folder root geometry and Developer Mode row are qualified. Typography,
menu edge/material calibration, curly quotes/Move to submenu, toolbar glyphs and
remaining capture/extension behavior remain partial; these captures do not claim
complete visual parity. The earlier audit's folder reference is explicitly listed
in `pairs/folder-menu-sources.txt` and uses the same installed Arc version.

The leftover audit tab was selected, its URL copied and verified as
`http://echo-devmode-audit.localhost:48763/`, and explicitly archived through
**Archive Tab**. `../site-controls/unlocked-developer-clean/fixtures-cleaned.png`
and `cleaned-ax.txt` show the cleared test sidebar. Closing its page alone had
preserved the sidebar item. No unrelated tab, Favorite or Space was removed.

## Advanced footer

`../site-controls/unlocked-advanced-r2/` records Arc's Advanced panel, native
export dialog (canceled), More Settings page and cleanup of that generated tab.
`echo-advanced-owner/` records the normal Preferences entry point in an isolated
Echo profile, export cancellation, a saved `exports/Echo Data Export.md`, More
Settings in the owning browser, Preferences closure and clean CEF shutdown.

Pairs: `pairs/advanced-footer.png`. The account/synchronization card and Arc-hosted
sharing remain excluded. Echo's More Settings is still a partial GPUI settings
page; global navigation and broader visual fidelity are not newly qualified.

Native reset initially failed although CEF registered the preferences and allowed
modification. `CefString::default()` encoded a null optional argument for CEF's
required error output. A scoped UTF-16 output buffer now supplies a real struct,
preserves errors and releases CEF storage. Fake callback tests cover the ABI and
ownership. `echo-translation-final-r2/app.log` confirms the normal Preferences
button now returns `accepted=true` in the bundled CEF runtime. Its subsequent
shutdown hit the CEF watchdog, so this is API/button acceptance, not persistence
qualification. A seeded retry preserved unrelated download preferences but missed
the reset click under load; another retry timed out capturing the native window.
Neither is counted as acceptance. Global/no-owner availability, managed-policy
cases and a clean seeded persistence check remain partial.

## AppleScript

`echo-scripting-r5/{read,eval}.applescript`, `app.log` and `echo-script.png` record a
successful native self-targeted AppleScript run in an isolated registered bundle.
It covers window/tab/Space IDs and names, negative/last tab indexes, properties,
URL/location creation, select/close, back/forward/reload/stop, normal/incognito
creation, window ordering and JavaScript denial before the native menu opt-in.
JavaScript undefined/null/Boolean/number/NaN/Infinity/array/object/BigInt/Symbol/
empty-string results were verified as dictionary-declared text; exceptions remain
errors. The run ended with `SCRIPT_OK`, process status 0 and CEF Complete.

The probe found a real `count properties of window` failure: the portable model
was enumerating child objects instead of counting the properties record. The
native read now passes, with a core regression test for all four object classes.
Arc's scripting dictionary was read directly; no AppleScript was sent to live Arc.
Range/predicate/relative selectors, runtime incognito mutation and broader window
state/native edge cases remain partial.

`echo-scripting-menu/` supplies the isolated native Developer submenu capture and
term names. Arc's matching menu/term evidence is in
`../site-controls/unlocked-task/scripting-developer-menu.{png,txt}`.

## Task Manager

`echo-task/` verifies real retained CEF CPU/memory, tab filtering, selected-renderer
termination while Echo remains alive, utility closure and clean shutdown. The
Browser PID 87784 and renderer PIDs 87799/87800 in the capture match the owned
process inventory in `processes.txt`. They are OS process IDs, not CEF task IDs.
Arc's live read-only Task Manager reference is
`../site-controls/unlocked-task/{task.png,task-ax.txt}`. Pair: `pairs/task-manager.png`.

`echo-task-gpu-r3/` additionally captures the real GPU Process row, tab filtering,
own-renderer termination and successful Command-W utility closure. Pair:
`pairs/task-manager-gpu.png`. That run ends with CEF's teardown watchdog (status 2)
while one cookie flush is pending, so GPU shutdown is explicitly **not** qualified.
The earlier `echo-task/` run uses `--in-process-gpu` and did complete cleanly.
Known-PID grouping has fake-sampler coverage; native shared-process grouping,
row icons, complete keyboard/presentation fidelity and PID expiry edge cases need
broader acceptance. GPU/utility/worker PIDs and all live network rates remain
unavailable in the prebuilt CEF API. The minimal API-extension plan remains in
[Other reference](OTHER_REFERENCE.md); no Chromium/CEF source patch or CEF source build was started.

## Shared-session cleanup

The final read-only inventory (`arc-final-windows.json`, `arc-final-ax.txt`) shows
four browser windows: Grosst, Work, Яремко and Playable. Arc had restarted outside
this probe, so the final window IDs differ from the initial inventory; verification
uses the Space names instead. All temporary windows claimed by this probe were
closed through the exact-window guard. No Space was created or deleted, and no
owner tab, permission or zoom value was changed. Developer Mode's original state
on the exact owned fixture host was restored before archiving that fixture.

## CEF shutdown follow-up

Both the seeded Preferences probe and out-of-process GPU probe reached
`FlushingCookies` with no browsers/owners, one pending flush and no reported CEF
failure, then logged `Teardown watchdog expired; recording dump and terminating.`
The normal native audit, scripting and in-process-GPU Task Manager runs completed
CEF shutdown. An isolated Rust experiment initialized cookie managers before
browser closure while retaining the existing post-close flush barrier; repeating
both probes still hit the watchdog. That experiment was reverted and is not part
of this change. No watchdog-disabling flag or early-exit/persistence bypass was
added. Follow-up should instrument cookie-ready/flush callbacks and CEF shutdown
entry before changing ownership or persistence ordering. No Chromium/CEF source
build or patch was attempted.

## Automated validation

`RUST_TEST_THREADS=4 cargo test` completed successfully in
`/tmp/echo-native-unlocked-full-r2.log`: 1,575 passed, one ignored, including all
doctest targets. The initial attempt passed the same unit/integration targets but
failed doctest loading after a missing local `multiversion_no_op` artifact; that
cache was rebuilt before the complete green run. Three regressions were added:
properties-record counting across all scripting classes, non-null CEF preference
error output with UTF-16 preservation, and error-storage destruction exactly once.
