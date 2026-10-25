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
Final header reinspection is blocked by the locked macOS session.
