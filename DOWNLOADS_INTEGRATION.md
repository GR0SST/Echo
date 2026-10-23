# Engine download foundation integration

`crates/engine/src/downloads.rs` is **registered for compilation and tests only**
through a private `mod downloads;` declaration in `engine/src/lib.rs`. Runtime
hooks remain pending: no native download client, ledger owner, persistence,
parity row, or browser UI is activated. Cargo, builds, native apps and tests were
not run by this agent. The parent can now compile and run the 19 authored tests.

## Public data and reducer API

- `DownloadProvenance::new(owner, storage, browser, private)` captures immutable,
  application-supplied opaque IDs. `owner` is the originating local profile;
  Blank uses its retained normal source profile. `storage` identifies the actual
  storage lease. `browser` is a never-reused browser generation, not a TabId.
  These values do not change when a Space/profile is selected or renamed.
- `Downloads::register_browser(provenance, allow_downloads)` returns a
  `DownloadRegistration`; its engine-only `handler()` supplies the exact CEF
  `DownloadHandler` for that browser's `Client`. Use a separate registration per
  native content browser. Inspector clients must deny downloads.
- `collect() -> DownloadEffects`, then `take_events() -> Vec<DownloadEvent>`.
  Events contain owned snapshots: `DestinationRequested` or `Updated`. Token
  identity is ledger-qualified and process-local. Never persist native IDs or
  tokens. Only Complete and Canceled are terminal. Interrupted transfers can
  resume: they retain their entry, destination and available control handle,
  accept resume/cancel, and continue processing later progress/completion updates.
  A resume request does not change the reported phase until CEF confirms it.
  The ledger retains terminal tombstones so late updates cannot revive completed
  or canceled downloads. Progress is coalesced only within a per-key progress run;
  before, terminal, paused and interrupted events are barriers and are never
  coalesced away. A missing control handle on a newer progress snapshot does
  not discard the last usable control handle.
- `resolve_destination(token, absolute_full_path)` returns deferred effects.
  This does not select a location or create/reserve a file. Empty, relative,
  non-UTF-8 and NUL-containing paths fail explicitly. It never substitutes CEF's
  temporary-directory default. The selected path is a requested destination;
  subsequent `DownloadInfo.full_path` is authoritative. Collision/overwrite,
  symlink, access and actual native filename behavior require separate policy
  and acceptance; no behavior is inferred here.
- `control(token, Cancel | Pause | Resume)` returns deferred effects. Cancel is
  sticky even before an update supplies its control callback. Dropping a pending
  before callback cancels the pending destination. Running cancellation remains
  a request until CEF reports a terminal phase; no synthetic completion is made.
  Pause/resume require an available control handle; CEF reports actual state.
- `snapshot(token)` gives a borrowed snapshot. All snapshots are ephemeral here,
  including private data; no serialization or disk writer is introduced.

## Required registration hooks (parent-owned existing-file edits)

1. The private module declaration in `engine/src/lib.rs` is present for
   compilation/tests. Runtime ownership and public facade exports remain pending.
   Keep all CEF-facing
   registrations/handlers inside engine; browser callers see only owned data,
   opaque tokens/provenance and controls. The parent may narrow public exports
   to its engine facade. Add a ledger to an engine/runtime owner independent of
   view routing, shared appropriately across pools/windows, with one delivery
   owner. Never drain it through `Engine::drain_events`' live-view filter or
   `PageStore::capture_event`'s live-page requirement.
2. Allocate immutable provenance and a registration before native browser
   creation. Create a per-browser Client containing its registration's handler.
   The current normal shared Client must be adapted; a handler cannot derive
   ownership from the active Space or an already-retired page at callback time.
   `Key` captures CEF's browser identifier and download ID inside that immutable
   registration. Application IDs are opaque and must not be reused while any
   registration/download can survive. Resolve profile/storage from the actual
   page identity and its storage selection, not a current window default.
3. CEF callbacks execute on the browser UI thread. The ledger/effects/registration
   are explicitly main-thread-only. The native handler retains an `Arc` inbox
   with atomic gates and a Mutex so native reference releases on other threads
   cannot touch non-thread-safe Rust ownership. The inbox holds only copied item
   data, native continuation/control handles and provenance; never Runtime,
   context leases, Browser, GPUI views or DownloadItem. It relies on the installed
   binding's existing ref-counted callback Send/Sync implementation; this module
   adds no unsafe Send/Sync implementations. No lock is held across a CEF call or
   displaced callback release.
4. Pump CEF with no model/engine borrows. Collect/reduce and remove returned
   effects while briefly borrowing the owner, then release that borrow and call
   `effects.execute()`. Continue/cancel/pause/resume and callback releases happen
   outside reducer/inbox borrows. Execute effects immediately on this thread;
   do not queue a resolved destination across teardown. An unexecuted effect
   owns its native handle; dropping Continue cancels instead of saving. Dropping
   a control effect only releases its handle and does not perform its command.
5. Collect before commands. Read destination events only after collection and
   route them by their captured profile/private provenance. Resolve with an
   externally chosen full path; this slice provides no picker, native Save As,
   download tray, Library layout, folder preference, or default location.
   `BeforeDownloadCallback::cont(path, 0)` is the sole continuation path. A
   destination event can precede a terminal event in the same batch: recheck
   `snapshot(token)` before requesting an external destination. The resolver
   independently rejects terminal/closing/canceled or already-resolved requests.

## Retirement, private lifetime and shutdown

Ordinary tab retirement must not delete the download ledger or stop its polling.
Keep the registration/queue reachable as long as native callbacks may arrive.
This module deliberately does not decide whether Arc continues an existing
transfer after closing a tab/window. Observe Arc before selecting that policy.

`close_registration(&registration)` is an explicit cancel/teardown hook, not the
ordinary tab-close hook. It closes admission, releases pending destination
callbacks, and produces cancellation effects for retained running controls.
Repeat close is harmless. Existing snapshots remain marked closing. Late native
progress is canceled without creating new entries. Do not report those snapshots
as terminal merely because close/cancel was requested.

`shutdown()` closes all admission, drains/rejects queued work, releases pending
continuations and produces cancellations for active transfers, including both
retained and queued interrupted transfers. Execute all effects BEFORE
the runtime's browser-close/context/cookie-flush/CEF shutdown barriers. A dropped
ledger only closes ingress/releases handles: explicit shutdown is needed to
execute cancellations. Existing external effects also have to be executed or
released before CEF unload. After shutdown, late handler updates cancel directly
without a queue borrow; terminal updates only release their callback.

**Context lease hook:** the parent must retain the exact originating
`StorageContext` registry lease for an active download (private included), and
release it after completion/cancellation or explicit cancel/shutdown teardown.
Interruption alone must not release that lease or install a terminal tombstone. Keep
that ownership outside callbacks/inbox and drop it before the existing runtime
owner barrier. Never add queue -> Runtime -> queue or callback -> ledger cycles.
The module does not itself promise downloads survive native browser destruction;
verify that against CEF/Arc and retain the necessary native owner if required.

Keep private metadata outside normal profile/session/history persistence and
regular Library delivery. Downloaded files remain user files. Private destination
inheritance, last-private-window cancellation and metadata lifetime require Arc
observation. Profile deletion remains metadata-only under existing engine storage
rules; do not remove downloaded files or Chromium storage directories.

## Exact local API evidence and validation still required

Installed `third_party/cef/include/cef_version.h` reports CEF
`154.0.34+g14c5a08+chromium-154.0.8037.98`; Cargo uses `cef 154.5.0+154.0.34`.
Primary contracts are `include/cef_download_handler.h`, `cef_download_item.h`,
and the installed Rust binding's `ImplDownloadHandler`,
`ImplBeforeDownloadCallback`, `ImplDownloadItemCallback`, `ImplDownloadItem`.

- Explicit `can_download` returns 1 only when the registration policy permits
  downloads and both admission gates are open. Rust's generated default is 0.
  `BrowserHost::start_download` bypasses can_download, so before/update callbacks
  enforce the same policy independently.
- `on_before_download` returns 1, retains its callback only for accepted valid
  items, and defers continuation. Rejected/invalid requests leave no retained
  continuation. Update may precede before. A DownloadItem is copied immediately
  after `is_valid`; the live item never leaves its callback.
- Unknown total/percentage and raw interrupt reason are preserved. Basetime is
  copied verbatim; it is not labeled as Unix time.

The module contains CEF-free reducer tests for ordering, destination cancel,
one-shot resolution, progress barriers, terminal/late updates, provenance and
ledger qualification, native browser binding, interruption/unknown totals,
interruption -> resume -> progress/completion, cancellation while interrupted,
and shutdown cancellation of retained/queued interrupted transfers,
deferred controls, closing, shutdown and callback release. Reentrant drop probes
check that coalescing and terminal callback release do not hold inbox borrows.
`copy_item(None)` covers the missing-item adapter branch; the same lazy validity
guard used by the adapter is tested to prevent all field reads on invalid items.
Actual non-null invalid CEF items need native/adapter acceptance rather than
fabricated unsafe CEF objects. These tests are registered for compilation but
unexecuted by this agent. Parent must compile/run and fix any integration
issues; this slice does not claim a green test run.

Native acceptance later: local attachment/redirect/slow/unknown-length fixtures,
cancel while choosing destination, cancel/pause/resume races, duplicate filenames,
unwritable destination, source retirement, two profiles with identical URLs,
Blank source routing, private lifetime, and quit with active transfers. Verify
bytes and reported final paths. Observe Arc UI first; parity rows remain missing
until their actual native surfaces and flows are verified. No account or service
integration is required.
