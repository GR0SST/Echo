# Disposable download acceptance

The opt-in automation fixture exercises the same AppDownloads consumer, captured
profile owner, destination dispatcher, canonical writer and quit handoff used by
the dormant download service. It does not enable production admission, a Downloads
folder policy, or the Library Downloads route.

Run a loopback fixture separately:

```sh
ECHO_PROBE_PORT=18474 ECHO_PROBE_HEADER_DELAY_MS=3000 bun scripts/fixtures/download-probe.ts
```

Launch an isolated bundle with a fresh `BROWSER_DATA_DIR`. Automation commands:

```text
app-download-arm http://127.0.0.1:18474 continue-normal
wait 1
app-download-status
app-download-wait-active 15
app-download-close-source
wait 70
app-download-wait-complete 5
app-download-status
quit
```

Arm reserves a new owned temporary directory internally. It accepts no arbitrary
output path. Normal windows with a real native parent are required; private,
Blank, occupied and quitting processes are rejected. The existing one-use Pages
admission binds the exact source identity. The legacy probe and AppDownloads
fixture exclude each other.

Status and waits only read application-owned snapshots: they cannot wake, pump,
drain, resolve destinations or manufacture completion. The consumer remains alive
through context readiness and quiet first-callback intervals using its existing
awaited timer. Status distinguishes scheduled/running/busy and reports the actual
consumer owner and legacy exclusion.

An authorized `Resolved` destination can precede CEF's actual item path. That
transition preserves the copied Info unchanged and cannot establish active or
completed status. Active requires nonzero genuine progress with the exact item
path; completion additionally requires a genuine terminal Updated event, the
canonical completed-record ID and inventory agreement. Source closure addresses
the captured tab even after selecting another Space.

A genuine bound Canceled update may clear its item path while retaining the
exact authorized destination. It records cancellation without creating a
completed record or proving a path. Complete with an empty path remains an error.

The service retains the canonical normal profile handle independently of source
windows. Verify the completed bytes/hash and reload the saved session through the
core session parser after exit; a status snapshot alone is not durability proof.
Opaque engine owner/storage numbers are not application profile IDs.

The output-directory checks support controlled disposable tests, not hostile
same-user path replacement or a general no-overwrite Downloads policy. Artifacts
are retained. Default production destinations remain deferred. Delayed sibling
admission, source-window loss, active-quit races and supported-platform behavior
require separate evidence; passing one fixture does not establish a full CEF
request-settlement fence.
