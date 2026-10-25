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

## Process metrics expansion (code/test only; native validation pending)

CEF 154.5.0's bound `cef_task_info_t` contains task ID/type/title/killability,
process CPU, processor count, memory and GPU memory. There is no OS PID, network
counter/rate, process-group ID or parent ID. `GetTaskIdsList` promises consecutive
rows for a shared process but supplies no boundaries; neither task IDs nor equal
memory/CPU values establish process identity. The generated local bindings were
checked directly; no screen or Arc interaction occurred while macOS was locked.

The engine facade now distinguishes GPU and utility/zygote/sandbox-helper tasks,
exports optional OS PID and bytes/second fields, and exposes a thread-affine
`TaskSampler` used by both CEF and fake samplers. The browser's own PID is known
from `std::process::id`. The renderer helper also reports its own PID over a
CEF process message when its main-frame V8 context is created. The browser accepts
only renderer-origin messages from its valid current main frame, associates them
with the document epoch, and uses CEF's browser-ID-to-task-ID mapping. Identities
expire on navigation, renderer termination and browser closure; nothing is exposed
to page JavaScript, and termination still uses CEF task IDs, never OS PIDs. This path
needs native acceptance after unlock. Unreported renderers, GPU/utility/worker PIDs
and all network rates remain unavailable with the prebuilt backend. CPU and memory
remain real retained-CEF measurements.
Unavailable values display a dash, while measured zero remains zero. A task ID
is never displayed as a PID.

Rows with a known shared PID remain adjacent through sorting; unknown PIDs remain
separate. The saved Arc AX evidence (`other-task2/task-ax.txt`) identifies shared
process rows using “Another task in the same task group” and leaves follower metrics
empty. Echo displays process metrics on the first row only, without adding shared
process metrics together; a measured network zero displays `0`, as in that evidence.
Network and Process ID columns sort when supplied. No unobserved group headers or other
new chrome are introduced. Native sorting/group presentation still needs Arc/Echo
acceptance after unlock; this does not close the parity row.

Tests inject a fake sampler into the actual polling loop, covering CPU over 100%,
network zero/unavailable, changing samples, shared tab/extension process ordering,
GPU/utility category membership, termination intent and vanished selections. Engine
unit tests cover CEF task classification and rejecting inferred process identity.

### Minimal CEF bridge plan (not implemented)

1. Extend the versioned CEF task-info API with OS PID and sampled network bytes/sec
   from Chromium's task manager, preserving the existing task ID as the command
   identity. Use explicit availability flags/sentinels, never zero as unavailable.
2. Enable Chromium's network-usage refresh alongside CPU/memory in the retained
   CEF observer; copy process identity and rate in `get_task_info`. Document the
   process-level sharing semantics and retain stable task ordering.
3. Generate matching C headers and Rust bindings, rebuild CEF and every helper
   against that exact ABI. Never reinterpret a larger struct against the prebuilt
   CEF library. Keep all native API access inside `crates/engine`.
4. Adapt the CEF sampler's two optional fields and validate a local transfer,
   shared renderers/extensions, process exit/reuse, task termination and shutdown.
   Then inspect Arc/Echo pairs before changing presentation or marking parity.

No Chromium/CEF source patch or source build was started. The prebuilt API cannot
complete GPU/utility PID or network reporting; that bridge remains the
explicit dependency for finishing per-process metrics and native grouping.
