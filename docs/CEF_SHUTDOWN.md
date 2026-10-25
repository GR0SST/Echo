# CEF shutdown investigation

Status: investigation, not a qualified fix. CEF 154.0.34 / Chromium 154.0.8037.98.

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

## Source boundary / next patch plan

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

These changes require a CEF-side build; there is no `src/` checkout in this
worktree. No Chromium/CEF source patch or source build was started. Do not carry
the earlier unproven scheduling changes forward as the fix.

## Reproduction / acceptance

Build with `cargo run -p bundle` (the shared machine limits Cargo to three jobs).
The macOS native suite uses `lockf`, `gtimeout`, and `ffmpeg` for the media cases.
Then run:

```sh
python3 scripts/check-shutdown.py --keep-going
python3 scripts/check-shutdown.py --appearance Light --case media-pip --case media-archived
python3 scripts/check-shutdown.py --profiles 4 --case profiles --repeat 3
```

The script freezes a private coherent bundle and creates fresh fixture profiles.
Every native launch acquires `/tmp/echo-screen.lock`; the complete holder has a
75-second hard timeout. Arc and owner profiles are untouched. Cases cover normal
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
