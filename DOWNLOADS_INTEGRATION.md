# Engine download integration

Production page creation and inspector clients still deny downloads. A narrow
loopback-only automation probe explicitly opts in for native verification.
Completed-record storage and the central browser router are registered but
dormant: no production destination policy, page admission, or Library Downloads
route is enabled. Download parity remains incomplete. The normal Arc lifetime
reference and engine continuation boundary are described below.

## Explicit opt-in seam

The only admitting creation method is:

```rust
Engine::create_view_in_context_with_download_owner(
    &mut self,
    context: &StorageContext,
    owner: DownloadOwner,
    ui_view: *mut c_void,
    url: &str,
    frame: Bounds,
    corner_radius: f64,
) -> Result<ViewId, String>
```

The original capability is
`DownloadOwner::cancel_before_native_close(captured_local_profile_id)`. A separate
normal continuation candidate and its mandatory handshake are described below.
Neither constructor establishes native acceptance.
The profile ID is opaque to engine. Blank uses its retained normal source profile;
never read the active Space/window profile later in a callback. Incognito data
must be routed separately and never serialized into normal sessions/history.

Before CEF creation, Runtime allocates immutable browser-generation and storage
identities, captures the owner, and retains the exact request-context registry Rc.
The context's configured cache path determines the existing private scope (empty
means private). This is configuration evidence, not proof of named-profile disk
persistence; the existing native restart acceptance requirement remains.

Each admitted browser gets its own native Client/DownloadHandler and admission
gate. Admission and native-owner liveness have distinct atomic gates: close
requests seal admission before canceling a still-live owner, while OnBeforeClose
seals native liveness immediately before queuing Closed. Every continuation and
control effect rechecks native liveness at execution, including effects prepared
before another effect caused native closure. CEF browser identity is bound under the creation guard, before callbacks
can be collected. Actual CEF context identity is checked; a mismatch or failed
bind seals the registration and queues native closure. Failed creation closes
admission immediately and releases deferred callback/context ownership through
the runtime pump, outside the caller's engine borrow.

Legacy `create_view`, `create_view_in_context`, forks and inspector creation
remain denied. The explicit deny handler blocks both user-initiated downloads
and the before/update paths used by StartDownload (which bypasses can_download).

## Original explicit native-close cancellation policy

`cancel_before_native_close` invokes download cancellation before submitting
CloseBrowser, retains the exact context until Closed and releases callback
handles/context leases without issuing controls against a closed native owner.
Unexpected OnBeforeClose seals its gate immediately. This capability remains
available with its original semantics; it is not the normal continuation policy.

CEF154's download delegate can continue associated transfers while the context
survives native browser destruction, but suppresses public handler updates once
the original browser is gone. A replacement blank browser/context alone cannot
provide a tracker. The separate candidate below retains the original browser in
an independently owned host; it preserves the OnBeforeClose release-only barrier.

Normal Arc source tab/window continuation has now been observed. Blank, private,
Little Arc, final-private-window and application quit policies remain outside
that evidence. Verify each separately before adoption.

## Owned facade and borrowing contract

The engine exports DownloadOwner, Token, Provenance, Snapshot, Info, Phase,
Destination, Event, Control and Error. CEF handlers/registrations and the raw
ledger remain private. Browser routing uses:

- `Engine::take_download_events() -> Result<Vec<DownloadEvent>, String>`: one
  process-wide central drain, independent of which engine pool/window owns a
  page. Route by captured owner, storage and private scope; never let independent
  window drains race to consume another profile's events.
- `Engine::download_snapshot(token) -> Result<Option<DownloadSnapshot>, String>`:
  owned clone for checking current state before acting on an older queued event.
- `Engine::resolve_download_destination(token, absolute_full_path)` and
  `Engine::control_download(token, Cancel | Pause | Resume)`: return Result<(),
  String>. They collect first, reduce briefly under runtime state borrow, then
  execute native effects only after that borrow has ended. Effects cannot escape
  the public facade and outlive CEF shutdown.

Call these static facade methods on the CEF initialization thread with **no
window, GPUI App/model, page-store or engine RefCell borrow held**. The future
browser router must schedule them outside its update closure. They reject a
missing/closing runtime and reject nested CEF creation, pumping or shutdown
polling. No method pumps or waits synchronously. Runtime pumping
first drains queued Closed notifications through release-only ownership cleanup,
then collects owned download callbacks after do_message_loop_work and before
new native close submissions. Each work batch executes after its RefCell borrow
ends. Closed cleanup repeats if native/context releases enqueue another closure;
unrelated callback order is preserved. Public facade and internal download
helpers also perform this closed-owner cleanup before collecting or canceling.

No path is synthesized. Resolution accepts only externally selected absolute,
nonempty UTF-8 full paths without NUL. Continue uses `(path, show_dialog=0)`.
Requested paths do not promise final names; subsequent CEF full_path is
authoritative. Collision/overwrite, symlink and writable-directory behavior need
separate acceptance. There is no temporary-directory fallback or silent Downloads
folder assumption.

## Reducer and ownership

Snapshot item fields are copied only after is_valid and never retain DownloadItem.
Updates can precede before-download. Only byte-progress runs coalesce; pending
destination, paused, interrupted and terminal transitions remain barriers.
Unknown length/percentage, interrupt reason and raw CEF Basetime values survive.
Missing newer control handles do not discard a usable previous control.

Only Complete/Canceled are terminal. Interrupted retains controls and destination
and accepts resume/cancel and later progress/completion. A resume request does
not fabricate progress. Terminal tombstones reject late resurrection. Tokens are
process-local and ledger-qualified, never persisted. Cancellation is a request
until CEF confirms it, except native teardown is explicitly marked closing.

Runtime owns one ledger across pools. Its browser registrations retain exact
`Rc<RequestContext>` leases, not StorageContext (which would retain Runtime).
Those leases conservatively last through native browser closure, including after
completion, because that live browser may initiate another transfer. Existing
Lifecycle remains the only owner of native Browser/host leases. Native callbacks
hold only Send/Sync inbox/gates and native continuation/control handles, never
Runtime, storage leases, Browser, GPUI state or live DownloadItem. There is no
Runtime -> callback -> Runtime cycle and no new public Runtime owner barrier.

## Shutdown

begin_shutdown seals download ingress only; it makes no download CEF calls while
the caller may still own application state. poll_shutdown, already required to
run from an app-owned task outside window/model borrows, shuts down downloads
before owner/browser/context/cookie-flush barriers. Queued Closed notifications
and release-only owner cleanup run BEFORE shutdown generates cancellation
effects. Retained or queued controls belonging to an already-closed owner are
released without invoking them. Cancellation still executes before submitting
NEW CloseBrowser requests for live owners. It cancels retained and queued
active/interrupted transfers, releases pending destination handles, and moves
context leases out of the runtime borrow. Native effects execute before their
final context releases. Repeated shutdown is harmless. Native clients retain
sealed queues/gates, not Runtime; final library unload cannot be held hostage by
download ownership. No external native effect batch is exposed by the facade.

## Verification state

The original 19 reducer tests passed in the parent's engine run before this
integration patch. Ten additional CEF-free ownership/wiring tests cover immutable
storage/browser generations, exact private registry lease survival after source
retirement, native binding rejection, deferred failed-creation cleanup, shutdown
lease release and isolation of late native closure from other owners' events.
An additional lifecycle guard test rejects nested public dispatch during creation,
pumping and shutdown polling. Existing reducer tests cover actual cancellation
effects with fake handles. Three ordering regressions use the same closed-first
lifecycle dispatcher and generic release-only reducer as production: queued
active/interrupted update -> Closed -> pump collection; retained interrupted
control -> queued Closed -> shutdown; and a precomputed native effect whose
owner closes before execution. Real owned control probes record zero calls to
closed owners, preserve unrelated owners and cancellation-before-new-close
ordering, and assert exact context/callback releases occur outside state borrows
and lifecycle queue locks. They construct no native CEF objects.

The October 23 checkpoint passed 82 engine tests and 786 workspace tests. It
verified only the denied production path. The October 24 checkpoints below add
an explicit disposable loopback probe; production page creation still does not
opt in. No account, browser login or cross-device service is in scope.

## Bundled regression check — October 23, 2026

The rebuilt macOS bundle loaded a disposable local HTML fixture and followed
its attachment link using real Tab/Enter events. The fixture received the
attachment request; the original page remained visible and no Save As window
opened. Screenshots were inspected. Ordinary application quit exited with
`CEF Complete`, zero owners, no browsers and zero pending cookie flushes.
This checks the existing denied admission path and teardown only. It does not
accept the explicit admitting API, file output, browser routing or Arc parity.

## Normal Arc lifetime reference — October 23, 2026

Native reference now establishes that normal transfers continue after closing
the source tab or source normal window while Arc remains running. Both local
32 MiB fixtures completed with exact expected bytes and hashes after closure;
see `docs/ARC_REFERENCE.md` and the detailed temporary report referenced there.
The existing cancel-before-native-close capability is consequently **not a
normal-browser parity policy**. Normal caller adoption remains disabled until
CEF continuation, callback routing and storage/host lifetime are implemented and
verified against that requirement. Final private-window behavior is unobserved.

## Normal continuation implementation (CEF154; bounded macOS acceptance below)

`DownloadOwner::continue_normal_after_source_close(captured_local_profile_id)`
is a separate, explicit policy from `cancel_before_native_close`. Private
request contexts reject the candidate (`UnsupportedPolicy`); this is not a
private-window behavior claim. Legacy page creation and inspectors still deny
all downloads. There is no normal production caller adoption in this engine
change. The normal Arc reference is `/tmp/echo-download-lifetime-native-20261023.md`;
CEF reasoning and remaining acceptance cases are in
`/tmp/echo-cef-download-continuation-design-r1.md`.

The browser-close handshake is exactly:

```rust
pub enum DownloadRetirementPreparation { Ready, Pending }
Engine::prepare_download_retirements()
    -> Result<DownloadRetirementPreparation, String>;
Engine::download_work_pending() -> bool;
Engine::download_retirement_status() -> Vec<DownloadRetirementStatus>;
```

After logically closing source views, call `prepare_download_retirements` on
the initialization thread **outside ALL App/model/engine/window borrows**, before
removing their GPUI window. `Ready` means the old parent can be destroyed: active
or pending sources have been synchronously reparented into independently owned
hidden native hosts; empty/terminal sources have queued ordinary native close.
`Pending` means a nested retirement operation still owns the handoff boundary;
retry after returning to the normal event loop. `Err` is a real native/mute/load
failure; do not remove the old window, select cancellation, or report successful
continuation. No runtime/no work returns `Ok(Ready)`. Pending destination or blank
navigation does **not** hold the old GPUI window once parking has succeeded.

`Engine::close` removes page routing, inspectors and snapshot namespace
immediately. It queues retirement without CEF calls or callback/context release.
The outside-borrows boundary collects queued Before/Update callbacks before
checking active work. New admission is closed on logical retirement. Already
queued callbacks are admitted before the known-item cutoff; while a loading
initiator is still unassociated, its late Before/Update callbacks are allowed to
establish association. Once associated, only known-item updates and delayed
Before callbacks are admitted. Native liveness remains independent and seals
only at OnBeforeClose. Pending, Paused and Interrupted all retain the source;
only Complete/Canceled release transfer leases. Interrupted cancellation still
requires native verification; no synthetic terminal state is produced.

The initiation boundary is deliberately observable. `waiting_for_association`
means the original loading document is retained/muted in the independent host,
with no StopLoad/navigation/timeout. A main-frame load error without an associated
item becomes an explicit retirement failure. An empty, nonloading source closes
promptly after callback collection. CEF has no public fence proving that an
unseen future item cannot arrive from a nonloading document; close-before-headers
and delayed sibling admission remain native acceptance gates, not guarantees.
Do not broaden the candidate to ordinary browsing on the strength of this seam.

Once **every** unfinished item's destination has been resolved, the engine
requests `about:blank` on the original browser. It verifies main-frame load-end,
URL and loading state, rather than treating LoadURL submission as completion.
The source is muted and mute is checked first. No stop_loading is used. A blank
load error is explicit failure. Pending quiescence is exposed by
`blank_requested && !blank_verified`; it is not native acceptance and the
central service/probe must surface stuck or failed cases. The normal probe must
require blank verification while the download remains active, plus exact file
bytes/hash and real later callbacks. This code does not certify that navigation
preserves transfers in the linked binary or that unload cannot stall it.

Lifecycle owns the **same** CEF browser, clip/native child and exact Rc of its
RequestContext until OnBeforeClose. It additionally owns an independent parent
lease installed *before* native attachment, including partial attachment failure.
No keeper points to Engine, StorageContext, Runtime or GPUI. macOS uses a never
ordered-front borderless NSWindow and moves the original retained clip, preserving
CEF's original parent_view. Windows uses a hidden nonactivating tool HWND; X11
uses an owned unmapped parent on a private XCB connection, with checked
CreateWindow/ReparentWindow/DestroyWindow/ConfigureWindow requests and exact-cookie
xcb_request_check error handling. No Xlib process-global handler is installed or
restored. Wayland-only or unavailable XCB hosts fail explicitly. Platform
compilation/native acceptance is still required independently.

The app-owned central download service must continue `Engine::pump()` and
`take_download_events()` outside borrows even after every source Engine/window
is gone. `download_work_pending` is a pure scheduling hint (no CEF or collection):
it includes inbox/events, failed creations, unfinished live entries and retired
browsers awaiting Closed. `download_retirement_status` is also pure. Each status
contains native browser ID, original provenance, parked/unfinished/association,
blank requested/verified, close requested, closed, host_released and failure.
Closed history survives keeper cleanup in memory. `closed` records receipt of
OnBeforeClose; `host_released` is set only after lifecycle drops its lease.
Neither is inferred from window visibility or output-file size. Native browser
IDs may be reused; always pair diagnostics with immutable provenance.

Final terminal collection queues native close outside borrows. The existing
DoClose path destroys only the selected native child; it never closes the hidden
parent or another browser as its close target. Closed-first dispatch seals
controls and releases download leases, then drops clip/context/hidden parent
outside lifecycle borrows. An unexpected Closed preserves the last actual
transfer phase and records failure, without fabricating completion/cancellation.
App shutdown is separate: it seals admission, overrides continuation, cancels
and releases download controls/context owners, and finishes the existing native
browser/cookie-flush/owner barriers. This remains Echo's safe-shutdown policy,
not an assertion about Arc quit behavior.

CEF-free regression sources cover queued update-before-Before, two-item lifetime,
known callbacks vs new admission, pending destination, paused/interrupted leases,
normal-only gating, shutdown override, unexpected native close with deferred exact
context release, reentrant dispatch exclusion, and install-before-attach host
failure/release through production generic seams. This implementation task only
syntax-parsed the files; parent must run Cargo tests, cross-platform checks and
the bounded native probes before adoption or a parity status change.


## Independent review fixes — October 24, 2026

Shutdown polling now excludes an active native retirement before it can drain
registration owners retained by the handoff snapshot. Closed-first dispatch
reduces already-copied metadata in a release-only mode before history counts and
native lease release: a genuine queued Complete remains Complete even when
Closed arrives before the ordinary drain. That mode cannot emit native controls
or destination continuations, including for rejected provenance. X11 traps now
attribute only the exact Display/request serial range and forward unrelated
errors to the previous handler outside the trap lock; unrelated errors cannot
turn successful hidden-parent creation into a failed result that loses ownership.
CEF/X-server-free regressions use the production shutdown gate, RuntimeDownloads,
release-only Ledger reducer and native request-attribution/result seams. These
changes were syntax-parsed only in the implementation task; parent rerun and all
previously open native acceptance gates remain required.


## R3 X11 review fix and copied-state regression — October 24, 2026

The R2 temporary Xlib trap was removed entirely. Forwarding scoped errors did
not make overlapping process-global handler installation/restoration safe: a
foreign scope could restore the engine callback later, allowing it to become its
own predecessor. X11 host operations now use a private runtime-loaded
`libxcb.so.1` connection and checked requests whose errors are returned directly
for their connection/cookie. There is no engine handler to capture, restore or
recursively forward to. The production checked-request seam has server-free
regressions for interleaved foreign request errors, unaffected foreign handler
state across two generations, exact own-request rejection and parent ownership
cleanup. Linux compilation and native host acceptance remain open.

The R2 engine run reported 93 passing tests and one obsolete closure assertion:
absence of the closed owner's token conflicted with the release-only copied-state
contract. Its replacement requires original copied phase/bytes, closing state,
no synthetic cancellation or terminal status, no retained controls/native owner,
and Resume returning Closed. Existing no-native-controls, deferred release,
unrelated-cookie ordering and live-other-owner control assertions remain.
These R3 changes were only syntax-parsed here; no test rerun is claimed.


## macOS continuation acceptance — October 24, 2026

The disposable fixture is reproducible with
`bun scripts/fixtures/download-probe.ts`. It serves a 32 MiB attachment on
loopback port 18473. Production admission and destination policy remain disabled.
The native probe resolves only its validated requested filename into a disposable
output directory, retaining exact owner/storage/browser provenance and token.

Baseline and source-tab closure passed on the immutable R4 bundle (binary
SHA-256 `63d4f2837e057ef40ea44b690966a14e43b9355a15f78fb9e7b2eaf133681e43`).
The original R3 baseline failed filename authorization and is not acceptance.
R4 uses `DownloadDestination::Requested.suggested_name`; empty later item names
are accepted only for the already-bound token/provenance and resolved exact path.

Source-window closure passed on the R5 bundle (binary SHA-256
`adecca45266426800d3ef848f5edc78ea1d72a2daffd8eeaaaf0e856198da5c6`).
The source normal root closed while a separate empty private window kept Echo
running. This tests a normal download, not private download lifetime. While the
transfer remained unfinished, status confirmed parking and main-frame blank
verification. Later genuine Complete reported 33,554,432 bytes. The output file
was independently hashed; all three successful runs matched
`46a3eca34e6d0d965224144558ccf51a14916a0075a09089f08da4868a020644`.

After completion, retirement reported Closed and host release with no failure.
Native window inventory no longer contained the independent hidden NSWindow.
Screenshots before source closure and of the surviving window were inspected.
All three processes exited 0 after CEF Complete with zero owners, no browsers
and no cookie flushes pending. Logs and screenshots remain under
`/tmp/echo-download-probe-{baseline,tab,window}-native-r{4,5}.log` and the
corresponding screenshot paths recorded by each script. These paths identify
local evidence, not portable test artifacts.

The current integrated workspace passed 894 tests: 627 browser, 165 core and
102 engine. These checks do not establish delayed sibling association, unseen
nonloading initiators, arbitrary unload behavior, final-private lifetime,
Windows/Linux native hosting, or full download UI parity.

## Shutdown-owned terminal ingestion

`Engine::take_shutdown_download_events()` consumes the same owned event queue
after shutdown begins, between polls and after Complete. It performs no native
effect or CEF call. Known identities are frozen at sealing; genuine terminal
metadata copied before or during cancellation survives native-handle release.
Eight production-seam regressions passed in the 102-test engine run.

The browser service exposes an exclusive `ShutdownDownloads` capability for
ingestion, canonical persistence and unique retained-writer flushing. Its actual
quit-coordinator hookup is still pending. Registration alone does not create a
second consumer or claim terminal-race persistence acceptance. Final ordering
must be Complete, final owned drain, persist, writer flush, service finish, then
the existing native exit continuation.
