# Other utilities: installed Arc reference

Reference: installed Arc 1.170.0; guarded sessions use the existing Echo Test
Space. No owner Spaces were modified, and no source-engine build was started.

## Shared Quotes

Selecting `controls` in our local fixture and pressing Control-Shift-Command-C
returned `https://arc.net/l/quote/ncrechto` to the clipboard. This was observed
in the application; the URL was not opened or independently browser-verified.
The normal Shift-Command-C path in that session returned the page URL.

Evidence: `target/parity/site-controls/other-quotes3/{before,copied}.png` and
`manual-quote.txt`. This is Arc-hosted cloud sharing, permanently outside the
local-only scope. Echo's existing local text-fragment command is a separate
implementation and is not evidence of Shared Quotes parity.

## Record Trace

Help → Troubleshooting → Record Trace opens a 360 × 400 window with a duration
selector, an issue-description text area and Start Tracing. Its warning says URLs
will be shared with Arc's team. We did not start tracing or send a report. The
owned trace window was closed explicitly.

Evidence: `target/parity/site-controls/other-trace/{trace.png,trace-ax.txt}` and
`target/parity/other/arc-trace-clean.log`. Arc's team-reporting/telemetry flow is
outside scope. Local trace recording remains in scope and is still unimplemented;
the service exclusion does not close that part of the parity row.

## Task Manager

Help → Troubleshooting → Open Task Manager opens a separate 1043 × 838 utility.
Tabs & extensions / Browser / All tasks, Search, Task / Memory Footprint / CPU /
Network / Process ID columns, and End Task are the observed controls. Initial
selection is Browser, with Memory Footprint sorted descending. The native source
window was closed without terminating any task.

Reference: `target/parity/site-controls/other-task2/{task.png,task-ax.txt}`.
CEF 154's public task manager exposes IDs, titles, task kinds, killability, memory
and CPU usage, but its task-info structure omits process IDs and network usage.
Echo keeps CEF handles and its small public-C-ABI workaround inside engine;
cef-rs 154's generated `task_info` drops output written into its temporary copy.
Missing metrics remain unavailable rather than inferred from unrelated processes.

Task grouping, row icons, column dragging/context menus, full sorting/keyboard
behavior and exact presentation still require broader reference/native checks.


Echo evidence: `target/parity/other/task-native4/echo-task.png`, `echo-tabs.png`,
`echo-end-enabled.png`, `echo-ended.png`, and `app.log`. A retained observer is
required: the [matching CEF implementation](https://raw.githubusercontent.com/chromiumembedded/cef/14c5a089e8452874cb1a556dc3fde26ed2d87723/libcef/browser/task_manager_impl.cc)
creates a new observer on every factory call, with a two-second sample interval
on macOS. Recreating it per poll produced zero metrics; the utility now owns one
monitor until close/quit. Its runtime lease is released before CEF teardown.
UI tests cover category/search filtering, selected-ID termination intent, hidden
row rejection, stale selection and Command-W.

Completing missing engine metrics would require extending CEF task info with the
Chromium process ID and network-usage fields, enabling the network refresh flag,
and exposing task grouping/parent identity. That is a small API/wrapper patch but
requires a local CEF build to validate. No `src/` patch or source build was made.

## Battery Saver and quiet downloads

`target/parity/site-controls/other-battery/command.png` shows the guarded Battery
Saver query: web suggestions, with no exposed matching command in this build.
The machine was on AC at 69% at inspection; no OS power setting was changed to
force a mode. Automatic activation/deactivation and its native presentation need
a qualified reference. No inferred toggle or battery indicator was added.

Quiet downloads cannot be activated independently of normal download ownership
and destination policy. [Downloads integration](../DOWNLOADS_INTEGRATION.md)
retains the general admission/fence and continuation gaps; the receipt experiment
has not enabled production admission. No fake active transfers or bypass of that
gate was added. A tray still needs its live download integration and reference.

Capture observations and qualified local behavior are in [Capture](CAPTURE.md).
