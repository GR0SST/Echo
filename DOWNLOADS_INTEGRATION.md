# Engine download integration

Engine-only wiring is present. **All existing creation APIs and inspector clients
deny downloads.** No browser caller opts in, no default destination is selected,
and no UI, file picker, Save As dialog, settings, persistence or account service
is introduced. Download parity rows remain missing pending native Arc reference.

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

Construct the capability with
`DownloadOwner::cancel_before_native_close(captured_local_profile_id)`. Its name
requires an explicit teardown decision; it does not establish Arc's behavior.
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

## Native-close policy requiring reference before browser adoption

Local CEF154 `include/cef_life_span_handler.h:279-290` says browser destruction
aborts associated network requests and restricts browser methods after
OnBeforeClose. Retaining a Rust Browser or context wrapper is not evidence that
the native page/download remains usable after its host window is destroyed.

This patch therefore does not keep a retired native browser/child view alive or
invent a detached download host. The opt-in capability explicitly selects
**cancel before native close**: the existing lifecycle ledger invokes download
cancellation before submitting CloseBrowser. It retains the exact context lease
until Closed, then releases callback handles/context leases without issuing
controls against the closed native owner. Unexpected OnBeforeClose seals its
gate immediately; subsequent lifecycle collection releases that owner's state.

The shared download ledger and captured owner outlive live view-map removal;
progress is never passed through Engine/PageStore live-page event filters.
This independence does not promise transfers continue after native destruction.

**Unresolved native question:** Does Arc continue normal/Blank/private/Little Arc
transfers after source tab/window closure, and what happens at final private
window closure? Observe this before any browser caller adopts the opt-in seam.
If continuation is required, design and verify the native browser/host lifetime
separately; do not weaken the existing CEF destruction barrier or preserve a raw
child-view pointer after its parent window is gone.

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

The integrated engine passed all 82 engine tests and the combined workspace
passed 786 tests on October 23, 2026. Native acceptance of the explicit admitting
API with disposable local download fixtures remains outstanding. No current caller opts in, so applying
it alone must leave existing downloads denied and must not change UI behavior.
Keep Arc native-close observation as a gate for later browser adoption. No
account, browser login or cross-device service is in scope.

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
