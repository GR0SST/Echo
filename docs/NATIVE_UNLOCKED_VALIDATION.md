# Native validation after the macOS unlock

Reference: installed Arc 1.170.0, its bundled `Arc.sdef`, and the qualified Arc
folder-menu capture from the audit. Evidence paths below are relative to
`target/parity/native-unlocked/` in this worktree. Every interaction ran as a
bounded non-interactive script under `/tmp/echo-screen.lock`; Arc used the shared
guard and existing Echo Test header reference. No `tell application "Arc"`, owner
calls, new Spaces, or owner-profile preference resets were used.

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

## Shared-session cleanup

The final read-only inventory (`arc-final-windows.json`, `arc-final-ax.txt`) shows
four browser windows: Grosst, Work, Яремко and Playable. Arc had restarted outside
this probe, so the final window IDs differ from the initial inventory; verification
uses the Space names instead. All temporary windows claimed by this probe were
closed through the exact-window guard. No Space was created or deleted, and no
owner tab, permission or zoom value was changed. Developer Mode's original state
on the exact owned fixture host was restored before archiving that fixture.

