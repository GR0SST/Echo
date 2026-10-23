# Browser pre-quit integration

The browser coordinator is `src/quit.rs`; the engine contract remains in
`../engine/SHUTDOWN_INTEGRATION.md`. This browser change does not modify the
engine or delete any physical profile directory.

## Ordering and ownership

`init` retains one app-level foreground task, independent of window entities.
Menu/Cmd+Q, automation quit (including automation aborts), and macOS native
termination request the same attempt. The synchronous browser creation gate is
set immediately; deferred normal, private, Blank, profile-sheet and Command Bar
producers check it when executed. Chromium storage/fork adapters also check the
gate before calling the infallible engine context/fork APIs.

On the next foreground scheduling opportunity the coordinator saves every
window profile and the retained NormalProfile. A save error pauses quit without
starting engine shutdown. Session serialization retains the existing split-draft
rollback rules. Ordinary periodic/final session writers and auto-archive stop
changing saved state once the quit gate is set.

The browser weakly registers every PageStore. It extracts each actual engine
owner into a temporary vector and replaces it with an inert adapter, so stale
splitter/automation/deferred layout closures cannot retain CEF owners. This
covers shared normal pools, independent private/Blank pools, inspector and
snapshot registrations. It removes window page handles and the retained
NormalProfile global. Per-window pump/close callbacks hold only weak page
references, and pump loops stop when the synchronous gate is set. Native parent
browser windows are retained and reject close during the attempt.

After the GPUI update and every page/app/coordinator borrow have ended, the task
calls `Engine::begin_shutdown`, then drops the extracted owner vector. It calls
`Engine::poll_shutdown` once per scheduling opportunity outside every GPUI and
RefCell borrow. Its 10ms wake interval is scheduling only, never a completion
barrier. Missing callbacks stay pending. Only `Complete` permits final GPUI quit
or the preserved native termination; failures and polling errors stay sticky with
visible progress/failure UI. Repeated requests join the same attempt.

## Native termination

`src/native/termination.rs` intercepts `terminate:` BEFORE AppKit's native
termination machinery. It seals creation, captures/joins the native intent, and
RETURNS. It never runs CEF or borrows GPUI. Both a CEF message pump and a GPUI
main-dispatch task can therefore unwind before the shared app-level shutdown
task runs. The supported route does not return NSTerminateLater or cancel and
reissue a fresh quit.

The shim changes the exact existing GPUI NSApplication instance's class to a
no-ivar subclass overriding only `terminate:`. Original ivars, inherited methods
and protocols (including CEF's CrAppControlProtocol) are preserved. A saved
original IMP is invoked exactly once after Engine Complete; the first sender is
retained until that call. A no-ivar subclass of the same retained GPUI delegate
adds `applicationShouldTerminate:` solely to return NSTerminateNow at this final
barrier. GPUI's inherited `applicationWillTerminate:` remains unchanged. AppKit's
delegate registration is refreshed on the same object after installation.
Installed GPUI 0.2.2 has no optional should-terminate implementation; an unknown
preexisting implementation causes installation to fail rather than being silently
replaced. An unexpected should-terminate call before the final barrier is refused
with a sticky diagnostic and no subsequent retry, not treated as a successful
native quit.

### Original AppleEvent ownership and ordering

There are two native entries. The direct `terminate:` probe passed on the parent's
r4 copy, but an AppleScript quit reached AppKit's delegate WITHOUT calling
`terminate:` and failed closed. Therefore a terminate-only gate is insufficient.
The shim now additionally wraps ONLY the application's C dispatch-table entry
for `aevt/quit`, using `AEGetEventHandler` to save the exact original borrowed UPP
and pointer-sized SRefCon, then `AEInstallEventHandler` to replace that entry.
It never calls NSAppleEventManager set/remove handler: Cocoa's original
handler/target/selector mapping and opaque refcon owner remain untouched.

At raw C callback entry, Foundation's currentAppleEvent is not assumed to exist.
The wrapper prepares owned `AEDuplicateDesc` event AND reply copies, a context ID,
and registry capacity BEFORE `AESuspendTheCurrentEvent`. The successful suspend
is immediately registered, and the callback returns without entering Cocoa.
Copies are later disposed with `AEDisposeDesc`, with errors becoming diagnostics.
These APIs run only on the main thread, with no GPUI/page/gate/registry borrow
across native calls. The borrowed original UPP is never disposed.

The direct terminate path can also capture an already-current Foundation event
with `suspendCurrentAppleEvent`; its original suspension ID is stored rather
than retaining current-event/reply descriptors, which Foundation forbids.
All native contexts have browser-owned IDs and distinguish raw copies from
Foundation suspension IDs.

The FIRST intent records its sender and its own optional event context,
INCLUDING an explicit absence of context. The production `quit::request` seam
used by Cmd+Q/menu/automation records this absent native context immediately;
it leaves an already captured native-first context untouched. Native callbacks
and that production seam share the same main-thread gate. GPUI regressions call
`quit::request` itself to cover menu-first then AppleEvent, and native-first then
coordinator delivery. Joined contexts are stored separately.
A first sender A without an event cannot be replayed under a later sender B's
event. Subsequent events after sticky failure are rejected by writing failure to
their still-current reply and returning an error, BEFORE any new suspension.
They cannot create an unreleasable pending suspension.

After Engine Complete, with no app/page/coordinator/gate/registry borrow alive:

1. If the FIRST intent was a raw AppleEvent, call `AEResumeTheCurrentEvent` with
   its original event/reply copies and a custom dispatcher. That dispatcher
   invokes the preserved original UPP with the preserved original SRefCon.
   Standard table redispatch is NOT used (it would hit our capture wrapper).
   Foundation/AppKit thus handle the entire original event, including `why?`
   logout/restart/shutdown metadata. No replacement terminate call is invented
   for this source. If the preserved handler itself calls terminate:, only that
   single continuation is allowed through the saved original terminate IMP.
2. If the FIRST intent instead had a Foundation suspension ID, restore THAT
   event/reply using `setCurrentAppleEventAndReplyEventWithSuspensionID:` and
   invoke the saved terminate IMP with THAT sender. If the first intent had no
   event, invoke the saved terminate IMP without restoring any joined event.
3. The original native handling reaches applicationShouldTerminate only after
   the engine barrier and gets NSTerminateNow. IMPORTANT: the original quit
   AppleEvent UPP is allowed to RETURN noErr and enqueue a later terminate:
   continuation. The parent's r5 native log proved exactly this sequence. A
   handler's return is not itself a finalization failure or evidence of exit.
4. A zero original result with no existing nonzero reply error leaves the live
   reply UNCHANGED and successful. AEResume can send that original successful
   reply after the dispatcher returns, AFTER CEF Complete. The gate remains
   finalizing and keeps ONE original terminate continuation armed, whether the
   handler calls it inline or queues it for a later AppKit turn. Consuming that
   continuation cannot be rearmed or replay the original UPP a second time.
   Joined requests remain pending for native exit, with no failure cleanup merely
   because the primary handler returned noErr. If the continuation never arrives,
   the app stays pending; there is no timeout or reconstructed quit.
5. Actual nonzero original/reply errors are preserved on the LIVE reply before
   dispatcher return and become sticky failures. A zero result is never changed
   to errAEEventFailed solely because the original UPP returned. Captured-event
   rejection after an existing failure still uses errAEEventFailed when no
   existing nonzero error is present. Tests cover successful-return → later
   exact-once continuation, unchanged zero reply, and nonzero error precedence
   before the manager sends the reply.
6. Unexpected return from the saved original terminate IMP (as distinct from
   the AppleEvent UPP), actual original-handler errors, and API failures remain
   sticky. They do not authorize another original call, cx.quit fallback, forced
   exit, or acceptance. Failed joined contexts are completed in reverse order.
   A primary AEResume rejection before dispatcher entry returns ResumeRejected;
   cleanup skips that primary, keeping copies tracked with exactly ONE attempt
   rather than issuing a second no-dispatch resume.

Foundation resume can immediately send a reply, undo current context, and
invalidate a suspension ID. It is not an automatic handler replay. Raw Carbon
resume explicitly supports a custom dispatcher; its event/reply copies must be
kept alive until resume returns. None of these API contracts establishes actual
logout/session-manager acceptance on the installed macOS. Parent native testing
of the rebuilt raw AppleEvent route and joined requests remains required.

The manual C FFI matches the installed SDK: AEDesc uses `repr(C, packed(2))`
(size 12, alignment 2 on LP64); OSErr is i16; SRefCon is void*; the callback reply
is mutable, while AEResume's reply parameter is const. Dispatch/descriptor APIs
link CoreServices; suspension/resume link Carbon (HIToolbox exports). A macOS
pure ABI test checks the actual production descriptor/callback definitions.

Primary references:

- Installed Chromium `src/chrome/browser/chrome_browser_application_mac.mm`,
  lines 289–321: its pre-terminate override intentionally avoids Cocoa's special
  termination loop and includes Dock, logout, restart, and shutdown routes.
- https://developer.apple.com/documentation/appkit/nsapplication/terminate%28_%3A%29?changes=__4
- https://developer.apple.com/documentation/foundation/nsappleeventmanager/suspendcurrentappleevent%28%29?changes=__7
- https://developer.apple.com/documentation/foundation/nsappleeventmanager/setcurrentappleeventandreplyeventwithsuspensionid%28_%3A%29?changes=latest___6_1
- https://developer.apple.com/documentation/foundation/nsappleeventmanager/resume(withsuspensionid:)
- Installed SDK Foundation/NSAppleEventManager.h documents descriptor and
  resume ownership; CoreServices/AE/AppleEvents.h documents save/install of the
  exact UPP/refcon; Carbon/HIToolbox/AEInteraction.h:215–289 documents raw
  suspension, custom redispatch, and main-thread restrictions;
  CoreServices/AE/AEDataModel.h documents pack(2) / duplicate / dispose ownership;
  MacTypes.h documents OSErr/SRefCon; AERegistry.h documents kAEQuitReason.
- https://developer.apple.com/documentation/coreservices/1448596-aeinstalleventhandler?language=objc
- https://developer.apple.com/documentation/coreservices/1445631-aegeteventhandler?language=objc_3

Installed GPUI exposes cancellable per-window close callbacks but no shared
cancellable application termination callback. On Windows/Linux the last browser
window close requests pre-quit because those backends stop their event loop on
last-window removal; other ordinary window closes retain their existing policy.
macOS ordinary last-window close/Dock reopen remains unchanged.

Remaining native gaps: Windows session-end/query-end-session events and Linux
session-manager/signal termination need dedicated platform integration beyond
GPUI's exposed hooks. Native Wayland engine embedding is also outside this
change. A failed macOS shim installation is logged and cannot claim native OS
quit coverage. Uncatchable process kill, OS-forced termination, or loss of the
toolkit event loop cannot be made graceful by this coordinator.

## Diagnostics and parent validation

Automation `native-quit` (no arguments) directly sends AppKit `terminate:nil`
from its foreground task outside `cx.update` and app/page borrows, then ends the
script. It keeps the existing `require-focus` guard, does not activate windows,
and does not call the browser coordinator directly. Nonempty arguments log an
error and end the script. The command must now return from the pre-terminate
gate promptly; final logs must show CEF Complete, invoking the preserved original
exactly once, and applicationShouldTerminate with engine_complete=true.

`[quit-native]` logs hook installation, native intent capture (including raw
AppleEvent capture before Cocoa), request delivery, final original IMP or
UPP/refcon invocation, and unexpected returns/failure replies. No native modal
Later/reply loop is used. The r4 direct probe passed with Complete/Now/exit0;
the r4 AppleScript probe failed with a pre-Complete delegate bypass and was
cleaned up by the parent with TERM/exit143. Cleanup is not acceptance. The wrapper/review fixes were built by the parent for r5; this agent ran no
native actions. The successful-return continuation correction still needs a
parent rebuild and native probe (see the r6 acceptance evidence below).

The earlier delayed-selector probe FAILED. Its sample at
`/tmp/echo-native-quit-stall-r2.sample` shows BrowserWindow pump → CEF work →
native nextEvent → CFRunLoop timer → delayed terminate → AppKit nested quit wait.
The outer CEF pump and main-dispatch task stayed active; later Cmd+Q logged a
request without progress. This was not evidence of a missing timer or a
selector-cache failure. The intermediate direct-call/Later design is also not
accepted: Apple documents Later's modal loop, and libdispatch's main-queue drain
has a reentrancy guard (published `src/queue.c`, lines 7759–7764):
https://raw.githubusercontent.com/apple-oss-distributions/libdispatch/main/src/queue.c
Cleanup of a stuck test process is not graceful-shutdown acceptance.

The raw-wrapper r5 probe reached captured original AppleEvent → CEF Complete
with zero owners/browsers/flushes → original UPP → delegate Now → UPP return 0 →
later terminate:. The previous implementation wrongly converted that legitimate
noErr into -10000 and rejected the later native continuation as sticky failure.
That probe FAILED; the parent's cleanup is not acceptance. The corrected code
preserves noErr and the single later native continuation. This runtime evidence
corrects the previous assumption that the original UPP must never return. The
specific private AppKit scheduling implementation is opaque; no private selector
or invented replacement AppleEvent is used. The parent's subsequent r6 standard
AppleEvent probe passed as recorded below; actual logout semantics remain untested.


### Parent native acceptance: r6 standard AppleEvent quit

The parent tested the exact owned copy `/tmp/echo-profiles-qa-r6.app` using an
AppleScript quit addressed to that exact application path. Read-only inspection
of `/tmp/echo-profiles-appleevent-quit-r6.log` confirms this sequence:

- Raw original quit AppleEvent captured first, then delivered to the coordinator.
- ClosingBrowsers with browser [1], followed by two persistent cookie flushes.
- CEF Complete with owners=0, browsers=[], cookie_flushes=0, failures=[].
- Original quit UPP/refcon resumed exactly once; delegate received
  engine_complete=true; original UPP returned 0 with its live reply preserved.
- Original AppleEvent accepted, then its single later native terminate
  continuation passed through the preserved original terminate implementation.

The parent reports application process EXIT 0 (session 2914), AppleScript sender
EXIT 0 with empty `/tmp/echo-profiles-appleevent-sender-r6.log`, and no TERM,
force-quit, or other forced cleanup. This accepts the standard AppleEvent path
for that tested bundle. It does not establish actual logout/restart/shutdown
semantics, joined-event coverage, or full persistence/restart acceptance. Those
remain separate tests. The parent also reports full Cargo r11: 642 passing tests
before subsequent unrelated UI work. This agent ran no Cargo/native actions and
made no Rust edits while recording this evidence.

Automation `key home` / `key end` now emit the native function characters
U+F729 / U+F72B and keycodes 115 / 119, including parsed modifiers. A pure parser
regression checks all three event fields. Earlier builds interpreted these names
as their first literal character, so those scripts are not navigation evidence.

Product UI displays only saving/closing progress, or a sticky quit-paused
message directing the user to the diagnostic log. `[quit]` logs record request
and changed engine statuses (phase, owner count, browser IDs, cookie flush count,
failures); identical 10ms polls do not log. `[quit] CEF Complete` is the explicit
engine barrier before native exit. Process disappearance without that message
is not evidence of coordinated completion. No page URL or content is logged by
the coordinator.

Pure tests use a fake shutdown driver to verify repeated requests, independent
owner and callback gates, no poll-count timeout, sticky failure/error, and final
notification exactly once. Browser tests cover Cmd+Q sealing before final quit,
deferred producer cancellation, malformed/missing-session source rejection while
sealed, and actual pool release despite retained layout handles. Existing strict
malformed-session tests preserve the original file and prove no NormalProfile
owner or Chromium directory is created on the invalid-load path.

This implementation agent ran syntax parsing only, with no Cargo/build/native
app commands. Parent owns full Cargo tests, native termination/reentrancy tests,
and full-process persistence acceptance for Default and named profiles, normal,
private, Blank, fork, inspector, removed/closing pages, repeated/native quit,
last-window/Dock reopen, and cookies/localStorage/IndexedDB across restarts.


## Optional Downloads consumer barrier

Quit inspects an existing AppDownloads global without creating or waking one.
It takes the exclusive ShutdownDownloads capability, waits for the relinquishing
Running lease, then drains/ingests before the existing model-save and engine-begin
steps. Every later engine drain uses take_shutdown_download_events outside App,
coordinator and service borrows. CEF Complete does not yet permit native exit:
the final owned drain, canonical ingestion, retained normal-writer flush and
service finish must all succeed first. Successful final steps are remembered
across an InFlight turn; native polling cannot continue after Complete.

A genuine error from a Running batch that relinquishes ownership during quit
becomes sticky before its lease is released. A later successful persistence retry
cannot authorize exit past it. Intentional post-seal effect suppression is
classified separately. Absent-global behavior performs no new download drain;
the independent test probe remains its sole consumer. An installed service
prevents probe arming and pumping, including a post-pump reentrant check. Hot
replacement of an already-active probe with a service is not supported.

All 921 merged workspace tests pass, including shared begin/poll/finalization
helpers with actual generic reducer and canonical writers. Native installed-service
terminal-race/session persistence acceptance remains pending. Production
admission, destination policy and service creation remain disabled.


### Merged-barrier native regression — October 24, 2026

Immutable bundle binary SHA-256:
`215d4a6a1715f326f927e0c54b13d969d318f9cb5047f17746378b3abfdb3ab6`.
An actual loopback transfer was InProgress with 393,216 observed bytes when
`native-quit` sent terminate:nil. The app exited 0 after CEF Complete with zero
owners, browsers and cookie flushes. Its disposable output directory was empty,
and saved profile downloads had last_sequence 0 and no completed records. No
incomplete transfer was inferred complete. Screenshot inspected; local log:
`/tmp/echo-download-quit-active-native-r1.log`.

A separate normal instance then received a standard quit AppleEvent addressed
by its exact PID after proc_pidpath matched the owned immutable executable.
The sender used SDK CoreServices APIs, not global key input or bundle-ID lookup.
The log shows raw suspension, coordinator request, CEF Complete, one preserved
UPP/refcon replay, delegate Now, preserved noErr reply and one later original
terminate continuation. Sender status 0, reply error 0 and process exit 0 were
verified. Local evidence: `/tmp/echo-download-quit-ae-native-r1.log` and
`/tmp/echo-download-quit-ae-sender-r1.log`; screenshot inspected. No force exit
or Cancel/reterminate fallback was used.

Both runs exercise absent-global behavior. They do not certify the still-pending
installed-service terminal-race persistence fixture, OS logout or session-end
semantics on other platforms.
