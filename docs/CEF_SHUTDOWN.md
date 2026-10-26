# CEF shutdown investigation

Status: the current six-patch generation `20261026T015700Z-65318` passed **16/16
clean native exits**, all twelve HTTP cookie checks and the separate PiP input
harness. The forced-timeout regression exits normally, reaps its network helper,
and verifies retained cookie/localStorage/IndexedDB stores on restart. Corrupt
storage is preserved and rejected. CEF 154.0.34 / Chromium 154.0.8037.98.
The original 8/16 watchdog baseline and earlier CHECK/exit-133 policy are
retained below as history. Exact build/qualification records are in
[CEF_BUILD.md](CEF_BUILD.md#native-qualification-and-publication).

## Later exit-133 regression and bounded recovery

The five-patch PiP qualification at
`~/Documents/Bot/cef-src/qualification/canonical-2026-10-25T14-26-06.837Z/pip-soapy02w/`
found another real failure: the parent CHECK fired after its ten-second network
wait, while network helper 54499 was still draining ThreadPool work. Its shutdown
log eventually reports **79,816,387 µs**. The Network Persistent State atomic
flush only began near the end of that interval and itself took **4,354 µs**;
this evidence identifies delayed pending work, rather than an 80-second fsync.
The parent had already crashed (exit 133), leaving the helper running temporarily.
An earlier clean 16-exit matrix therefore did not justify that CHECK policy.

The revised source policy waits up to 60 seconds for graceful network shutdown
in the background. It atomically records an interrupted-shutdown marker before
terminating a late helper, escalates and reaps within two seconds, and returns
normally. The outer watchdog reserves 72 seconds including ordinary teardown.
Quit hides the window immediately. Next launch uses a separate bounded headless
process and the same bundled CEF to verify cookie/storage integrity before CEF
opens the profile. Failed checks preserve stores and the marker. See the current
canonical qualification and exact commands in [CEF_BUILD.md](CEF_BUILD.md).

The deterministic native regression shortens the grace period to 500 ms and
delays only the test network helper by five seconds. Both overrides require
`ECHO_CEF_TEST_MODE` and `ECHO_SHUTDOWN_TRACE`; neither is active in production.
It asserts normal parent exit, marker creation, helper reaping, successful
cookie/localStorage/IndexedDB recovery on restart, and rejection of an untouched
corrupt SQLite fixture. Production 16-exit/PiP runs forbid those overrides and
require every network helper to exit gracefully.

### Second-audit SIGTRAP reports

The queued second-audit failures are the same old timeout CHECK, rather than a
new failure in the current generation. PID correlation matches
`reaudit2-fixes/echo-refinements-Light/drive.log` to
`~/Library/Logs/DiagnosticReports/Echo-2026-10-25-143122.ips` (PID 70347), and
the Dark log to `Echo-2026-10-25-143412.ips` (PID 78533). They trap **10,173.919**
and **10,157.388 ms** after the logged quit request. Both reports identify CEF
UUID `4c4c44f8-5555-3144-a14f-c139a2d87f2b` and image offset `0x321c1b4`.

A retained exact-UUID framework in
`~/Documents/Bot/cef-src/qualification/shutdown-final.app` has SHA-256
`5bddc3b2ea8c251ee8325be37768a775f5e05619082b8db86efe19eaba1c6f6e`,
the phase-three `c5a5134` build. Offline `llvm-objdump` shows the network service
name, `browser_process_io_thread.cc` and shutdown diagnostics in this function.
At `0x321c07c`/`0x321c080`, it loads a **10,000,000 µs** wait argument; after
the wait call it tests the returned success flag and exit code, branching at
`0x321c094` directly to **`brk #0` at `0x321c1b4`** on failure. This is the old
`BrowserProcessIOThread::ProcessHostCleanUp` assertion
`CHECK(echo_exited && echo_exit_code == 0)`. Stripped-symbol reports label nearby
code with the nearest `_v8_internal_Node_Print` export; that label does not
identify a V8 crash. No debugger was attached.

Commit `75bc810` already replaced this assertion with the bounded recovery
policy above. The current `c9d3ae…` framework contains that fix, so reproducing
these historical reports does not require another CEF patch or rebuild. The
window-hide call precedes the shutdown work; the recorded fatal instruction is
inside CEF's network cleanup. Pending network ThreadPool work explains the wait;
the measured atomic flush itself was short, as detailed above.

The fresh acceptance matrix can exercise the exact native termination entry
point with `--quit-command native-quit`. It sends `terminate:nil`, verifies both
the request and captured native gate in every log, and retains the same strict
barrier/exit/cookie/network checks. The ordinary coordinator entry remains the
default. Fixtures disable the persisted quit-warning preference explicitly.
Offline proof, selected crash frames and bounded disassembly commands are in
`~/Documents/Bot/cef-src/qualification/quit-trap-reaudit-20261026/offline-proof.json`.

The fresh native-entry matrix passed **16/16 actual zero exits** on main
`517494b`, executable SHA-256
`d10458c51459f42448c96bc12def60586802d6f0051f8de50fa4a57dbb11642f`,
with current framework `c9d3ae396593a3f8333c1017c9f3b9c6b64d9394285c1504257461bec9092829`.
Every exit records the native request/capture and exactly one CEF shutdown and
final terminate. All eight restart cases retained cookies, every network helper
exited gracefully, and no owned process remained. The accompanying deterministic
forced-timeout recovery also exited zero, reaped its helper and verified cookie,
localStorage and IndexedDB retention. Both ran within one bounded long screen
session, with the unchanged visibility guard and no debugger. Evidence:
`quit-trap-reaudit-20261026/matrix/run-d8x6e7sg/` and
`quit-trap-reaudit-20261026/recovery-i51yrbpw/`. The current distribution's
`native-quit-qualification.json` links the follow-up evidence. No CEF rebuild or
new generation was required; no new SIGTRAP was reproduced on the fixed build.

## What the old log concealed

`FlushingCookies browsers=[] cookie_flushes=1` was the last coordinator status
printed *before* `poll_shutdown()` synchronously entered `cef_shutdown()`.
That status is not proof of a missing cookie callback. Timestamped engine
instrumentation now distinguishes manager initialization, each flush request,
request acceptance, callback delivery, callback collection, context releases,
and entry/return of global CEF shutdown. A default, ungated ShuttingDown line
is emitted immediately before the blocking call so ordinary runs no longer leave
FlushingCookies as the last diagnostic. Helper logs distinguish service cleanup
from return through `cef_execute_process`.

The reproduced failures collect every requested cookie flush and release all
application browser/context/manager handles before entering CEF shutdown. The
native macOS teardown watchdog then terminates inside the blocking CEF call.

## Native evidence

All paths below are relative to `target/parity/shutdown-watchdog/`.

- `local-dump/run-51skv4x2/1-media-archived/`: natural watchdog exit 2 with
  local-only CEF crash reporting. `local-dump/stack.json` and
  `local-dump/symbolicated-main.txt` are offline analyses of that dump. The main
  thread is in `BrowserProcessIOThread::ProcessHostCleanUp` →
  `Process::WaitForExitWithTimeoutImpl`, waiting for the network-service process.
- `helper-return-r2/run-9avumwq7/1-media-archived/`: network helper returns code 0
  only after the parent watchdog has fired. Cookie completion precedes the global
  shutdown entry. CEF logs identify that helper as `network.mojom.NetworkService`.
- `network-snapshot-r4/run-ijyl9n5g/1-media-archived/`: **destructive diagnostic,
  not acceptance**. A temporary fixture-only timer deliberately aborted its own
  network helper to ask CEF Crashpad for a local snapshot. No debugger was
  attached, reports had no upload endpoint, and system crash forwarding was off.
  Offline `network-snapshot-r4/network-stack.json` shows the helper main thread
  in `TaskTracker::CompleteShutdown`. Its background worker is in
  `ImportantFileWriter::WriteFileAtomicallyImpl` → `File::Flush` at
  `F_BARRIERFSYNC`. This establishes a durable-write wait in service cleanup;
  a sample taken during a long natural failure is still needed to establish
  whether this is the same wait throughout the complete watchdog interval.
- `fsync-measured-r2/run-c9swe1ce/`: an ignored, temporary syscall timing probe
  measured writes of Network Persistent State / TransportSecurity and checked
  restart cookies. This timing experiment passed both exits; it is not a fix.

CEF symbols were downloaded from the exact official build matching the installed
framework UUID, and inspected offline with `atos` / `minidump-stackwalk`.

The supplied `echo-mini-pip-validated/drive.log` and `echo-mini-pip-dark/drive.log`
from the main-menu worktree close the same browser IDs (`[1,3]`, then `[1]`),
then diverge at global teardown. Their old status lines do not identify the
blocking subsystem. No evidence establishes theme appearance as the cause.

## Loaded-machine baseline matrix

`diagnostic-matrix/run-p4x2hzos/` uses a coherent diagnostics-only bundle.
No proposed scheduling, connection, or power-management fix is present.

| Scenario | Write exit | Restart exit | Persistent profile cookies verified |
| --- | --- | --- | --- |
| normal | 2 | 2 | 1 |
| profiles | 2 | 0 | 3 |
| private | 0 | 2 | 1 |
| reset | 0 | 0 | 1 |
| gpu | 2 | 2 | 1 |
| combined | 0 | 2 | 3 |
| media-pip | 2 | 0 | 1 |
| media-archived | 0 | 0 | 1 |

All 12 persistent-profile HTTP cookie checks passed, as did the two new
private-context empty-cookie checks and the empty-cookie reload after site reset.
Six of eight scenarios nevertheless failed native shutdown qualification: eight
of sixteen launches ended with the teardown watchdog. The two passing scenarios
were reset and archived PiP; this is an intermittent shutdown failure, not a
claim that those scenarios cannot trigger it. `results.json`,
`native-failures.json`, `requests.json` and both per-phase engine/CEF logs retain
the complete evidence. The acceptance script exits nonzero for this matrix.

## Eliminated experiments

Dispatch-main → CFRunLoop handoff, a late BeforeWaiting observer, and an
asynchronous `CloseAllConnections` fence before flushing cookies, and scoped
macOS NSProcessInfo user-initiated and latency-critical quit activities all still
reproduced watchdog termination. The activity passed its first write/restart,
then failed both exits on the next repeat (`activity/run-u6g0m8d8/2-normal/`).
The latency-critical variant also passed its first pair, then failed both exits
on the next repeat (`critical-io-repeat/run-l2jq0uue/1-normal/`).
None is retained as a production fix. No
watchdog suppression, early process exit, shortened persistence barrier,
forced helper termination, or disabled filesystem synchronization is shipped.
The destructive diagnostic helper and syscall interposer exist only in ignored
evidence bundles; the normal bundle contains neither.

## Deadline collision

Chromium's macOS release teardown watchdog allows ten seconds from
`OnBrowserTearDownStarted`. Later in that same teardown, `ProcessHostCleanUp`
allows the network service up to ten seconds to exit. These equal budgets leave
no room for the earlier teardown or the cleanup after that wait. The natural
watchdog dump confirms the parent is still inside this network-process wait.
This identifies the failure mechanism at the parent; the exact reason for a
particular long network-helper rundown still needs source-level timing.

## Original source boundary / patch plan

The prebuilt CEF API has a cookie-store flush and a connection-close callback,
but neither acknowledges the network service's pending ImportantFileWriter
writes. A CookieManager callback therefore cannot be used as a barrier for all
network-profile metadata. Returning success early would conceal this failure.

The next CEF-side change should first add monotonic begin/end timings around the
network-process exit wait and the network helper's atomic file flushes, including
file identity, worker priority and shutdown state. This avoids drawing a causal
conclusion from a single helper snapshot.

The watchdog budget must account for the already-bounded network wait plus the
rest of teardown, rather than racing the same ten-second interval. A source
patch can share those budget constants and add a fake-clock regression covering
the complete bounded wait. It must preserve the cookie callbacks, the network
service's existing graceful rundown and actual `cef_shutdown` completion; no
success may be reported while the network data is still being written.

If file-write priority is the cause of the long rundown, the smallest additional
experiment is a macOS `ScopedBoostPriority(ThreadType::kDefault)` spanning
`tmp_file.Flush()` / atomic replacement in `base/files/important_file_writer.cc`.
**Priority starvation is not proved, and this is not an approved fix.** If writes
remain slow, CEF needs an asynchronous durable network-storage acknowledgement
before synchronous global teardown, rather than a shorter storage deadline.

That investigation required a CEF-side build, and its worktree had no `src/`
checkout. It did not start a source patch or build. The subsequent source work
is documented below; the earlier unproven scheduling changes were not retained.

## Historical phase-three correction (2026-10-25)

`patches/cef154/shutdown-durability.patch` corrects the confirmed enclosing-deadline
collision. `content/public/common/shutdown_timeouts.h` now supplies the network
process wait to both its caller and the macOS graceful watchdog. The release
watchdog reserves **10 seconds for existing teardown work plus the unchanged
10-second network wait**, rather than starting an equal ten-second deadline
before that wait can begin. Slow-build watchdogs likewise retain their existing
rundown allowance in addition to the network wait. The profiling/ChromeOS network
waits remain 100/1 seconds respectively. SessionEnding's distinct 15-second
release deadline is unchanged. Both normal teardown and SIGTERM use the shared
graceful policy; watchdogs remain active and disarm only on real completion.

This follows the natural parent stack: ChildProcess cleanup waits for the network
service; its ThreadPool waits for BLOCK_SHUTDOWN atomic writes. Cookie completion
does not include that metadata barrier. The parent deadline cannot safely be the
same length as a later nested wait. No fsync, atomic replacement, cookie callback,
network wait, helper lifecycle or CEF completion barrier is bypassed or shortened.

With `ECHO_SHUTDOWN_TRACE=1`, the patch records monotonic atomic-flush begin/end,
file identity and worker thread type, child ThreadPool shutdown timing, and the
network wait's duration, PID and actual exit code. A final cleanup record reports
whether every network helper exited with code zero before the existing
TerminateAll stage. `check-shutdown.py --require-network-exit` rejects a missing
record, timeout or nonzero network exit even if the primary application exits 0.
Its frozen bundle manifest now also records the CEF framework SHA-256.
On macOS, a network timeout or nonzero helper exit now fails explicitly before
falling through to TerminateAll. CEF's void shutdown API must not return apparent
success after truncating a pending durable write. The existing bounded graceful
wait remains unchanged; the normal zero-status helper path is unaffected.

The new instrumented, unchanged-timeout normal/GPU probe passed **4/4 exits** at
current load. Network waits were 9.3–13.9 ms; the largest observed atomic flush
was 35.5 ms. Evidence is outside Git at
`/Users/grosstgrosst/Documents/Bot/cef-src/qualification/shutdown-instrumented/run-29z75mtd/`.
Several shutdown metadata writes ran at the default thread type. Neither this
probe nor a separate small barrier/QoS diagnostic established priority starvation,
so **no priority boost is shipped**. The reason for the earlier intermittent long
kernel flush is still unproved; the source fix addresses the demonstrated parent
deadline race rather than assigning that flush an unsupported scheduling cause.
The historical 8/16 baseline and new qualification are different runs, not a
matched-load statistical A/B measurement.

`scripts/check-cef-shutdown-budget.sh` compiles a constexpr fake clock against the
actual production header. It covers work before the network wait, its entire
bounded duration, work after it and shutdown completion for normal/slow builds.
A local negative-control copy using only the old rundown allowance fails the
static assertions. `scripts/build-cef.sh` always applies this six-file patch and
runs the regression after its serialized Release arm64 build. The extension
bridge remains a separate opt-in patch. All source builds still use nice 10 and
Ninja -j 8.

The matrix under qualification uses the same Echo/GPUI/helper executables from
phase two with the complete freshly built framework (including matching V8
resources) substituted into a private coherent bundle. It does not change Rust
shutdown sequencing. The selected source build includes the earlier codecs and
extension bridge. The original prebuilt fallback does not contain this patch;
select a newly built distribution as documented in [CEF_BUILD.md](CEF_BUILD.md).

## Source-build acceptance result

The complete eight-scenario matrix passed **16/16 native exits** with
`--require-network-exit`, versus **8/16 clean exits and 8/16 watchdog exits** in
the retained loaded-machine prebuilt baseline. All sixteen network helpers
returned code **0** before cleanup; no timeout was converted into apparent
application success. All twelve persistent-profile HTTP cookie checks passed,
as did both private-context empty-cookie checks and the reset reload check.
Every phase had one actual `cef_shutdown` return, the complete CEF barrier,
exactly one final native terminate, and no watchdog or automation failure.

| Scenario | Write exit | Restart exit | Persistent profile cookies verified |
| --- | --- | --- | --- |
| normal | 0 | 0 | 1 |
| profiles | 0 | 0 | 3 |
| private | 0 | 0 | 1 |
| reset | 0 | 0 | 1 |
| gpu | 0 | 0 | 1 |
| combined | 0 | 0 | 3 |
| media-pip | 0 | 0 | 1 |
| media-archived | 0 | 0 | 1 |

External evidence:
`/Users/grosstgrosst/Documents/Bot/cef-src/qualification/shutdown-final/run-etf4kagk/`.
`results.json`, `requests.json`, empty `native-failures.json`, per-phase engine/CEF
logs, screenshots and `source-summary.json` retain the results. The maximum
network wait was **329.834 ms**; the largest atomic flush was **743.031 ms**;
maximum synchronous global CEF shutdown was **370 ms**. All logged atomic
flushes succeeded. The fixture screenshot was reviewed; PiP acceptance uses its
explicit HTTP events and engine PiP/audible assertions, including the archived
source case. The framework SHA-256 was
`5bddc3b2ea8c251ee8325be37768a775f5e05619082b8db86efe19eaba1c6f6e`.
The frozen Rust/GPUI/helper executables were from current-main commit `f1d3ffe`
(which contains `3b7c88a`). The selected final distribution passed the full
workspace tests: **1,610 passed, zero failed, one existing ignored**. The matrix
freezes the bundle so later builds cannot change an ongoing run. In addition to
CEF's wait result, all sixteen network-helper `cef_execute_process` return logs
were independently matched by PID and verified to report code zero.

An earlier budget-only variant also passed 16/16 exits at
`qualification/shutdown-qualified/run-5tdebg49/`. Review then added the explicit
macOS failure before the inherited helper-termination fallback; the complete
matrix was repeated on that final build, producing the results above.

Exact qualification command:

```sh
python3 scripts/check-shutdown.py \
  --bundle /Users/grosstgrosst/Documents/Bot/cef-src/qualification/shutdown-final.app \
  --keep-going --require-network-exit \
  --output /Users/grosstgrosst/Documents/Bot/cef-src/qualification/shutdown-final
```

The source patch corrects the demonstrated deadline collision. These successful
runs do not establish a cause for every slow kernel flush under every load, and
no storage-priority or filesystem-sync change is claimed.

## Reproduction / acceptance

Build with `cargo run -p bundle` (the shared machine limits Cargo to three jobs).
The macOS native suite uses `lockf`, `gtimeout`, and `ffmpeg` for the media cases.
Then run:

```sh
python3 scripts/check-shutdown.py --keep-going
python3 scripts/check-shutdown.py --appearance Light --case media-pip --case media-archived
python3 scripts/check-shutdown.py --profiles 4 --case profiles --repeat 3
# Canonical qualification: all 16 exits in one uninterrupted screen session.
zsh scripts/check-shutdown-matrix.sh --keep-going --require-network-exit
# Same matrix through NSApplication terminate:nil (second-audit reproduction).
zsh scripts/check-shutdown-matrix.sh --quit-command native-quit --keep-going --require-network-exit
```

The script freezes a private coherent bundle and creates fresh fixture profiles.
Individual runs acquire `/tmp/echo-screen.lock` with a 120-second hard timeout.
The canonical wrapper instead holds that lock across the entire matrix, creates
`/tmp/echo-screen-long`, and removes the marker with an EXIT trap. Its hard timeout
is 700 seconds, within the screen watchdog's 720-second allowance. Inner drives
use `--screen-locked` to avoid taking the same lock again. A CGWindowList preflight
checks for leftover Arc windows above layer zero; the explicitly authorized Arc
restart clears those panels if necessary. It never edits Arc session files or
owner profiles. Cases cover normal
quit, multiple isolated profiles, a private window still open, site-data reset,
separate GPU, combined cases, playing background media with native PiP, and
archiving the PiP source before quit. The PiP fixture has explicit play/enter
controls and verifies HTTP events and engine PiP/audible state.

Each case restarts the same profile and verifies real HTTP Cookie headers,
including profile isolation, reset behavior and absence of private cookies.
Even an exit-2 run is restarted to measure actual cookie durability; it still
fails qualification. Exit 0, the complete CEF barrier, exactly one native shutdown
and final native terminate, and absence of watchdog/automation failures are all
required. `--keep-going` records all failures and exits nonzero at the end.
The actions are deterministic; watchdog occurrence remains intermittent.

Set `BROWSER_DRIVE_LOCAL_CRASH_DUMPS=1` for local-only CEF watchdog reports.
Set `ECHO_SHUTDOWN_PERFETTO=1` for Chromium shutdown tracing. Crashpad can capture
its own watchdog failure without an external debugger. Chromium stops its normal
startup trace before the relevant service teardown, so a completed trace file
alone cannot establish successful global shutdown.

## Primary implementation references

- [CEF shutdown](https://raw.githubusercontent.com/chromiumembedded/cef/14c5a089e8452874cb1a556dc3fde26ed2d87723/libcef/browser/main_runner.cc)
- [Network-service exit wait](https://raw.githubusercontent.com/chromium/chromium/154.0.8037.98/content/browser/browser_process_io_thread.cc)
- [macOS teardown watchdog](https://raw.githubusercontent.com/chromium/chromium/154.0.8037.98/chrome/browser/shutdown_watchdog_mac.cc)
- [Durable file flush](https://raw.githubusercontent.com/chromium/chromium/154.0.8037.98/base/files/file_posix.cc)
- [Atomic writer](https://raw.githubusercontent.com/chromium/chromium/154.0.8037.98/base/files/important_file_writer.cc)
- [CEF local crash configuration](https://raw.githubusercontent.com/chromiumembedded/cef/14c5a089e8452874cb1a556dc3fde26ed2d87723/include/cef_crash_util.h)

The freshly reassembled local `target/bundle/Echo.app` was also checked after the
final workspace tests: its normal write/restart pair passed **2/2 exits** with
`--require-network-exit`, cookie persistence and the full CEF/native terminate
barriers. Its framework hash matches the final sixteen-exit matrix exactly.
Evidence: `qualification/shutdown-final-bundle/run-moxctlik/`. Re-linking the Rust
wrapper changed the main executable's hash, so this separate pair qualifies the
actual local bundle as well as the frozen full-matrix fixture.

## Quit visibility and latency

Quit now calls GPUI's application-hide operation as soon as its first intent is
sealed. It retains windows, engine owners, downloads, sessions and all existing
shutdown barriers until real completion. Ordinary teardown opens no progress
window. A failed barrier presents one diagnostic window and keeps termination
paused, rather than discarding browsing data. GPUI currently implements app hide
on macOS; Windows/Linux visibility parity remains unqualified.

`scripts/check-quit-latency.py` copies the assembled bundle into isolated test
profiles, acquires the screen lock with a 300-second hard holder limit (90 seconds
per process, allowing the bounded background teardown), and polls
CoreGraphics for that exact process's on-screen windows every 5 ms. The three
native request-to-window-gone observations were **23.928, 30.363 and 32.290 ms**;
all three processes exited zero after `CEF Complete`. These are idle development
measurements, not a guarantee under arbitrary I/O load. Before-quit screenshots
were inspected. Evidence: `target/parity/quit-latency/{results.json,run-*.log,before-*.png}`.
A separate normal write/restart check retained its cookie and required graceful
network cleanup: `/Users/grosstgrosst/Documents/Bot/cef-src/qualification/quit-hidden/run-5wodi57c/`.
The full 1,610-test suite passed after the behavior change; the added GPUI
regression also passed, checking no ordinary progress window, one diagnostic on
failure, retained termination gating, and repeated Quit joining that state.

The metadata candidate `20261025T192350Z-5008` repeated the measurement with a
frozen app and framework `55a60b71fd32579a392ee53f337a908cecb7b0f012971d6ad3317b4a32ec82aa`.
Under concurrent build/test load, its three observations were **44.278, 42.471
and 57.494 ms** (median **44.278 ms**, 5 ms polling). All three owned `Echo Test`
processes completed normally. Evidence:
`/Users/grosstgrosst/Documents/Bot/cef-src/qualification/final-quit-latency-192350/`.

A separate real Store-extension/profile-switch session on this exact framework
hid its window immediately and then completed CEF teardown in **23.530 seconds**.
The network helper exited zero, `all_exited=1`, without a timeout/dirty marker.
Its thread-pool shutdown took 21.758 seconds; the observed final atomic writes
took 0.781–6.249 ms. This remains a background latency case, not a visible wait
or a forced termination. The native log is
`target/parity/extensions-ui/qualify/final-real-extensions-cef.log`.

The API-v4/PiP canonical generation `20261026T015700Z-65318` repeated the full
matrix on main `4cf9291` + Developer tools, including accepted-quit policy commits
`a70317e` and `1bf393b`. Fresh fixture sessions explicitly disable the normal
`settings.toggles.warn_before_quitting` preference; no confirmation clicks mask
the quit path. All sixteen actual exits were zero, all eight restart scenarios
retained cookies, and all network helpers completed normally. Recovery and
browsing-data deletion used the same rebased executable. Evidence:
`~/Documents/Bot/cef-src/qualification/canonical-2026-10-26T02-34-03.543Z/`.
The separate forced-timeout recovery case also exited zero and terminated its
owned helper, then verified storage on restart. The normal matrix has no dirty
shutdown, watchdog or crash failure.

With this rebased app, three further native accepted-quit measurements were
**37.973, 28.818 and 44.412 ms** to window disappearance (median **37.973 ms**,
5 ms CoreGraphics polling), all followed by actual exit zero. Before-quit
screenshots were inspected. Framework SHA-256 is
`c9d3ae396593a3f8333c1017c9f3b9c6b64d9394285c1504257461bec9092829`;
executable SHA-256 is
`bf91401563d7f13f345269611fd4460a10cf3683d0c7daead9f6dfe10dc005c1`.
Evidence: `~/Documents/Bot/cef-src/qualification/developer-rebased-quit-latency/`.
