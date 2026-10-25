# Engine download integration

Normal-profile production pages now use explicit captured-owner admission, a
profile-selected default folder, staged no-clobber publication and the existing
canonical writer. The Library completed-record route is mounted. Ask-where
requests currently defer; chooser, explicit Save Link/Image As and active surfaces
are being integrated separately. Private/Blank and inspector clients retain deny
admission. See [ordinary download acceptance](docs/ORDINARY_DOWNLOADS.md).
Historical checkpoints below describe the individual dormant slices before
this activation; they are not a claim that every lifetime/platform is qualified.

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
ingestion, canonical persistence and unique retained-writer flushing. The quit
coordinator now takes this capability only when AppDownloads already exists.
It does not install or wake a service. After the in-flight Running batch releases
its lease, the coordinator drains once before engine shutdown and uses only the
shutdown-safe drain during polling and after Complete. Final ordering is
Complete, final owned drain, persist, unique writer flush, service finish, then
the existing native exit continuation. Installed-service terminal-race native
persistence acceptance remains pending; absent-global probe behavior is separate.


## Authoritative destination metadata

The browser router now preserves the entire copied destination state. Only an
actual DestinationRequested event paired with Requested authorizes a destination
policy call. `DestinationRequest` freezes the callback's suggested name, exact
provenance and original item snapshot independently of later item-name updates.
The default remains Defer. Explicit retry reuses that same frozen request.

A repeated callback name cannot replace its original snapshot. Conflicting
request metadata is rejected; a genuine same-token/provenance terminal Updated
is still ingested and persisted, while the conflict remains an error. During
shutdown that error deliberately blocks exit rather than being cleared by a
successful later write. No default path or filesystem collision policy is added.

All 22 router tests passed, including the shared production conversion seam,
both callback orders, repeated/empty names, conflicts, terminal persistence and
explicit retry. The pre-quit-integration workspace checkpoint passed 912 tests
(645 browser, 165 core, 102 engine). These tests use fictional keys through the
same conversion helper, not fabricated engine-issued DownloadTokens; actual
service-owned native persistence acceptance remains separate.


## Exclusive quit handoff — October 24, 2026

The coordinator waits on InFlight before saving/detaching source models or
beginning engine shutdown. Each relinquishing Running batch publishes its own
genuine ingestion, persistence, drain and native-effect errors before releasing
the lease. Those errors remain sticky across successful write retries. Historical
diagnostics are not reclassified, and intentionally suppressed post-seal effects
are not reported as native failures.

Shared production orchestration tests inject reentrant handoff and actual
generic reducer/writer failures. They assert zero engine begin/poll/exit calls
after failure, ordered successful final ingestion, deduplicated canonical
flushes and one exit authorization. InFlight retries retain successful final
steps rather than redraining after a successful flush. Native event/sender replay
remains owned by the existing termination shim.

The merged workspace passed 921 tests (654 browser, 165 core, 102 engine).
The production service remains dormant: no owner registration, default
destination, production admission, service startup or Downloads route is enabled
by this change. A preinstalled service excludes the independent disposable probe.
The opt-in [one-consumer fixture](crates/browser/DOWNLOAD_ACCEPTANCE.md) now
exercises installed-service persistence without manufacturing engine-issued
tokens. Production startup and admission remain disabled.

## Installed-service acceptance — October 24, 2026

The R6 workspace passed 949 tests (682 browser, 165 core, 102 engine). Its
immutable macOS bundle binary SHA-256 was
`5a088a1d859bbab2556ad45d026ffff9b26e093ec43c281d1b317c0a22d94baf`.

A real fixture delayed response headers by three seconds. After a single arm,
status showed sequence zero with admission Claimed, `running=true` and legacy
consumer exclusion, before the first download callback. The initial status had
admission Waiting; this demonstrates the observed readiness transition rather
than a synthetic test-only retry.

The source used named application profile 2, independently bound to opaque
engine owner 1. After genuine nonzero progress, automation selected the second
Space and closed the captured source tab. The same consumer received genuine
Complete at 33,554,432 bytes, acknowledged canonical record `(profile 2,
sequence 1)`, and reported inventory agreement with no failures or service
errors. The output SHA-256 was
`46a3eca34e6d0d965224144558ccf51a14916a0075a09089f08da4868a020644`.
After process exit 0 and CEF Complete, the core session loader found one record
in profile 2 and zero in default profile 1. The screenshot was inspected.
Evidence: `/tmp/echo-appdownloads-native-r6.{log,json,png}` and
`/tmp/echo-appdownloads-native-r6-driver.log`.

Earlier R2/R5 fixture runs failed: the engine's authorized Resolved snapshot
retained an empty previous item path, which the audit rejected. R6 accepts that
nonterminal transition only for the bound request and exact destination. It
preserves Info and still requires a genuine exact item path for active and
completed acceptance. Wrong paths and terminal conflicts remain failures.

This is bounded named-profile/source-tab evidence. It does not establish
source-window loss through AppDownloads, a final-poll completion race, delayed
sibling admission, a full native request fence, production no-clobber
publication, or Windows/Linux acceptance.

The R7 workspace passed 952 tests (685 browser, 165 core, 102 engine). Native
active quit passed with bundle binary SHA-256
`121e7c2aa9520460d0824c62c3a2276ac547f2467041b8326701a5e338661f0b`:
the installed consumer accepted genuine progress at 393,216 bytes, then the
native terminate gate delivered quit through normal shutdown. CEF reached
Complete with zero owners, browsers and cookie flushes; the original terminate
ran once and the process exited 0. Reload found zero completed records in both
profiles. The screenshot was inspected, and no owned helper remained afterward.
Evidence: `/tmp/echo-appdownloads-native-active-quit-r7.{log,json,png}` and its
driver log.

The preceding R6b active-quit run was not accepted: a genuine Canceled snapshot
cleared its item path, which the fixture misclassified as a sticky conflict.
That disposable process required exact-PID/executable-checked SIGTERM cleanup.
R7 recognizes bound cancellation separately from completion; empty/wrong-path
Complete still fails. A separate R6 launch hit a CEF/libdispatch SIGTRAP before
automation or arm; it is recorded as a startup failure, not fixed by the audit
change. The successful R7 run does not establish a final-poll completion race.

## Staged publication through the existing consumer

The service retains an explicit staging reservation against the frozen callback
request, original opaque token/full provenance and canonical normal ProfileHandle.
No default destination, filename suffix policy, picker, startup or production
admission is installed. The direct disposable acceptance fixture remains separate.

Both Running and shutdown divert retained staged events before the legacy
completion constructor. A stage path cannot become a canonical completed record.
Only a genuine bound Updated Complete after acknowledged Continue creates owned
Publication work. Cancellation retains bytes and cannot be resurrected; interrupted
transfers remain resumable.

Running moves its current ConsumerLease into staged Work and receives that same
lease back before retirement preparation and the timer. There is no second lease,
event drain, task or collector. The shared driver detaches eligible work under a
short service borrow, then releases that borrow before filesystem operations,
Continue, proof validation or the canonical writer. Queued observations overtake
every subsequent effect. State and failures return before the lease can be released;
reentrant quit continues to observe InFlight until this boundary.

Production shutdown ingestion invokes the same driver after copied-event reduction
and before retained-writer flush/finish. It can publish an already completed,
authorized stage but cannot Continue an unresolved destination. An unresolved
stage blocks final persistence until cancellation or accepted completion resolves
it. Genuine cancellation is disposal, not a synthetic persistence failure.

The private proof supplies the exact final native path. After outside-borrows
validation, record_profile calls the captured ProfileHandle. Its Result errors
occur before core mutation; an unwind remains Unknown because Work marks the
writer uncertain before entry. A definite writer failure requires explicit retry
and revalidates the same retained proof; uncertain native/filesystem/writer outcomes
block replay. Historical diagnostics and genuine quit handoff failures remain.

An explicit staged-only policy may now be installed before the first transfer.
Pure plans are captured at their exact Requested event before later observations
in the copied batch. Work reserves outside borrows under the current lease, retains
the owned result before reducing queued cancellation/quit, and rechecks before
Continue. Failed/unknown reservation cannot automatically repeat. Genuine terminal
metadata without an authorized stage is retained and blocks legacy persistence;
a staged-only policy cannot choose a direct final target.

Disposable automation accepts stage-normal and stage-collision-normal in addition
to the original continue-normal mode. These create only owned TMP destinations.
Collision mode records a newly created sentinel's identity/bytes; CEF receives an
absent stage payload, never that final target. Audit distinguishes actual native
transfer path from validated published path, and canonical inventory uses the
latter. Publication/witness checks run outside all borrows before the real writer.
Status/wait commands still read only; the same sole service owns every event.
This fixture introduces no default Downloads policy or normal startup admission.

Production folder/preferences and filename rules, stage-aware active projection,
control/retry surfaces, crash recovery and remaining platform qualification remain
open. Native acceptance of each new staged fixture flow must be recorded separately;
the original direct fixture cannot establish it.

Filesystem gates remain as documented in crates/browser/DOWNLOAD_PUBLICATION.md:
crash recovery, durability, hostile concurrent path replacement, platform filesystem
qualification and service-level quarantine preservation. General source-close
admission still requires the custom engine's proven early correlation/seal fence;
known-ID probes and these service tests do not establish that fence.


Validation of the staged service runtime (macOS, 2026-10-24): root `cargo test`
passed 999 tests, including 23 staged filesystem/lease/writer scenarios. The six
runtime additions cover lease return, reentrant quit/cancellation, sticky identity
failure, final shutdown publication before flush, unresolved-stage suppression and
the real ProfileHandle proof adapter. Existing seventeen staged tests are retained.
A bundled direct-fixture regression completed 32 MiB after source-tab closure,
verified the exact file hash, reloaded one canonical record under the captured
named profile and none under Default, and exited normally after CEF Complete.
The screenshot was inspected. This direct-fixture run is regression evidence;
it does not establish native staged publication or a full admission fence.


Completed staged Jobs keep their genuine metadata/ID tombstone but release folder,
stage and final-file pins after canonical acknowledgement, outside state borrows.
Failures retain their original publication/proof for reconciliation; cancellation
retains owned partial-stage state. A macOS/Linux descriptor-path regression checks
actual OS descriptors in the owned fixture directory, plus idempotent record replay.


The staged-policy/collision fixture update passed 1,011 root workspace tests on
macOS (744 browser, 165 core, 102 engine). Bundled Echo then received a genuine
32 MiB Complete after source-tab closure, published the second explicit `.bin`
candidate without changing the original sentinel, verified the expected file hash,
and reloaded one canonical final-path record in the captured named profile and
none in Default. The final binary includes completed-pin release. Its screenshot
was inspected. This establishes that controlled known-ID fixture only; it does
not establish arbitrary sibling admission, a full CEF fence, default Downloads
policy, crash recovery, or remaining platform qualification.


The R6 active-quit fixture exposed a separate shutdown ordering gate: CEF reached
Complete, but staged state still blocked the final writer barrier. That run timed
out; its process was later absent without a verified exit cause, and is not a pass.
No partial was recorded as completed. R7 therefore holds native close submission
for known resolved InProgress writers until an exact genuine terminal copy arrives.
The Cancel command/closing flag, silence and unrelated metadata do not release it.
This preserves the CEF browser association while asynchronous cancellation runs.
No-ack/interrupted outcomes still require separately accepted recovery policy;
this change does not synthesize cancellation or weaken final persistence barriers.


The integrated R7 root workspace passed 1,014 tests (744 browser, 165 core,
105 engine). Its final immutable bundle repeated the 32 MiB staged-collision /
source-tab-close / captured-profile reload check successfully. Separate staged
and direct active-quit runs both returned exit 0, invoked original native terminate
once after CEF Complete, and reloaded zero completed records in both profiles.
The staged sentinel stayed unchanged; all screenshots were inspected and no owned
helper remained. These do not claim a native final-poll Complete race, interrupted
shutdown recovery, general admission fence, default destination or remaining
cross-platform acceptance.


## Library completed-record route

`LibraryDownloadsHost` is mounted by the actual BrowserWindow through Window →
View Downloads…, ⇧⌘J (including CEF focus), Command Bar → View Downloads, and the
Library rail. It reads the canonical normal workspace; it does not install a
consumer, admit a download, choose a destination, or expose file actions. Private
and Blank routes remain unavailable. One presenter retains its query/filter across
section changes. Every route departure invalidates its captured event generation,
even if departure and return occur before a paint. Canonical notifications refresh
hidden inventory; an active-only presentation timer refreshes calendar wording.

The list and controls use the existing Downloads presenter. The right rectangle
mounts the existing selected-page renderer and measured Pages viewport. Search,
Filter, Done, chips and list-background hits transfer native intent to the exact
available GPUI control. Passive GPUI focus observations (including root-window
reactivation) can reconcile an existing UI cache but cannot create UI intent or
replace a saved native responder. Popup dismissal revalidates that responder; newer native
or History input supersedes the list cache.

`downloads_presentation.rs` orders valid completed timestamps newest first, retains
stable tie order, and groups by the displayed heading. On macOS it uses the current
system calendar for Today/Yesterday and Foundation's full numeric relative date
formatter for older/future instants. This follows the available native observations
and static Home-grouping candidate; the exact Downloads-to-Home call chain and all
boundary cases have not been proved. No 30-day month cutoff is inferred. Other
platforms currently use Today/Yesterday/calendar dates, pending their qualification.
Unknown/unrepresentable dates remain present after dated records with no invented
heading. No unfiltered-empty copy is invented. File-type/source subtitles remain
unfilled pending an established type policy; row file actions stay disabled.

These routes establish partial Downloads presentation, not default production
download activation or complete Arc parity. Multiple-profile combinations, file
thumbnails/types/actions, missing-file states, active transfers, animation, exact
visual/material parity, platform formatting and general admission fences remain
open alongside the previously listed publication/recovery gates.

The integrated route passed 1,022 workspace tests on macOS (752 browser, 165 core,
105 engine). An initial native attempt exposed passive GPUI activation invalidating
the saved native responder; that failed run is not acceptance. After separating
Observed from Requested control focus, the final bundle passed the same scenario:
search retained input after Command Bar dismissal, an empty Profile filter toggled
off once through actual Space key release, and the same native document UUID
received subsequent input after popup dismissal without another click. Screenshots
were inspected. A reload through the actual core session reader retained the one
original genuine completed record under its captured named Profile, with none in
Default. These checks used a fresh copy of previously verified owned fixture data;
no new admission, destination policy, file action or full-fence capability was enabled.

The source-controlled [CEF154 receipt experiment](docs/CEF154_DOWNLOAD_EXPERIMENT.md)
now preserves the qualified engine patch set, source pins and selected-test
record. It includes native pre-close GET/POST relay and same-document coverage;
the ordered public fence and production adoption remain incomplete.
