# Engine shutdown and pre-quit integration

This is an engine prerequisite, not a finished application quit implementation.
No GUI hooks, GPUI final-quit future, native termination coordinator, profile
removal, or persistence acceptance result is provided by this change.

## Stable public API

All three methods are associated functions on `Engine`, returning
`Result<ShutdownStatus, String>`. Call them on the thread that initialized CEF.
They do not require a surviving engine pool or window.

- `Engine::begin_shutdown()` seals creation immediately and idempotently.
- `Engine::poll_shutdown()` performs one nonblocking progress step, including
  CEF work and queued lifecycle processing. It never waits for an elapsed time.
- `Engine::shutdown_status()` reads status without pumping.

`ShutdownStatus` exposes `phase`, `pending_owners`, `pending_browsers` (native
CEF IDs), `pending_cookie_flushes`, and `failures`. Phases are `Running`,
`WaitingForOwners`, `ClosingBrowsers`, `FlushingCookies`, `ShuttingDown`,
`Complete`, and `Failed`. Only `Complete` permits final application termination.
An absent callback remains pending indefinitely. `Failed` is sticky and does
not trigger a forced shutdown; report the diagnostics and keep termination
deferred. An `Err` (wrong thread or reentrant polling) is not completion.
Calling `poll_shutdown()` before `begin_shutdown()` does not initiate shutdown.

Existing Engine/create/context/fork/snapshot/inspector signatures are preserved.
Fallible constructors/context/fork/inspector APIs return errors once closing;
page creation returns `None`, and new snapshots return `false`. The existing
infallible `fork_with_shared_storage`, `storage_context`, and
`StorageContext::clone` APIs panic if used after sealing. Stop all creation
producers before releasing pools, including pending window/profile tasks.
Never catch these panics as part of ordinary quit orchestration.

## Required browser/native hooks (parent-owned)

1. Route every application quit intent through one pre-quit coordinator BEFORE
   `App::quit` or GPUI's final quit callbacks. Include menu/shortcut quit,
   automation quit, OS/native application termination, and any last-window
   policy that terminates the process. On macOS the parent must defer the native
   termination reply; GPUI's non-cancellable 100ms final-quit future cannot do it.
2. Stop page/window/profile/inspector creation producers and save the core/app
   session while page metadata is still available. Call `begin_shutdown`.
   Repeated quit requests must join this same attempt.
3. Take and DROP every `Engine` pool and every public `StorageContext` handle,
   including normal/private windows, Blank windows, detached/reparented pools,
   cached handles, temporary owners and deferred tasks. Release snapshot and
   DevTools registrations by dropping pools. Closing all views while keeping a
   pool is insufficient: `pending_owners` stays nonzero. No separate engine
   prepare hook is needed. Engine drop only queues close requests.
4. Keep native parent hosts/windows valid during close when possible. Retaining
   native window leases is the GUI coordinator's responsibility. On macOS the
   engine independently retains each exact clip host until `OnBeforeClose` has
   returned; it does not retain the CEF child NSView (that would prevent dealloc).
5. Start/retain ONE application-level task on the init thread, independent of
   any window entity. At each toolkit scheduling opportunity call
   `poll_shutdown()`, then yield to the toolkit event loop. Do not hold app,
   window, engine, or page `RefCell` borrows across this call. CEF may dispatch
   native events; the engine suppresses recursive pumping/polling and queues
   callback data rather than touching application entities. Task wake cadence
   is scheduling only; no timer duration or pump count establishes success.
6. Once status is `Complete`, the engine has called `CefShutdown` exactly once.
   Then allow GPUI final quit and finish/reply to native deferred termination.
   Subsequent pumps are no-ops; reinitialization in this process is rejected.
   The initialized Runtime shell and macOS framework loader stay retained,
   with application CEF handles/registries released before `CefShutdown`.

The app-level task must remain runnable when every window is closed, hidden, or
removed. Do not put the only shutdown pump in a per-window render/timer hook.
The existing macOS mouse-tracking/native-queue safeguards remain in the pump;
the toolkit must continue dispatching native events. Pending status is honest
if the event loop cannot make progress.

## Ownership and ordering

All app-created browser frontends are synchronous Alloy child browsers.
Runtime registers each by native browser ID, independently of pool-local
`ViewId`s, and retains its CEF Browser, exact `Rc<RequestContext>`, and native
host lease. Inspector frontends use the same path. Removed views and dropped
pools remain registered until the callback. `OnAfterCreated`, `DoClose`, and
`OnBeforeClose` only enqueue scalar data; callback objects own the queue, not
Runtime, so there is no Runtime -> Browser -> Client -> Runtime cycle.

The exact context is held by the creation call until synchronous creation
returns and registers it. Reentrant pump/callback draining is suppressed during
that interval. Unknown browser IDs fail shutdown rather than disappearing from
accounting. This implementation does not expose asynchronous native browser
creation or CEF ShowDevTools popup creation; adding either requires installing
its lease before lifecycle callbacks are drained.

After public owners reach zero, all remaining browsers receive one queued
`CloseBrowser(true)` request. This deliberately follows the existing forced-tab
close policy: JavaScript before-unload cancellation is not a quit-cancellation
UI. Each close still waits for `OnBeforeClose`; the force flag is not a success
barrier. Do not issue repeated `CloseBrowser` calls to complete native close.

After all browser callbacks have returned, each persistent context is retained
while `GetCookieManager` initializes its storage through a callback. Then
`FlushStore` must both accept the request and deliver its completion callback.
Immediate rejection wins over a queued/late callback. Managers and persistent
registries are released before `CefShutdown`. Private contexts are not flushed
as persistent storage. Default is included even if the first pool was private.

## Exact installed CEF154 close evidence

Installed `third_party/cef/include/cef_version.h` identifies
`154.0.34+g14c5a08+chromium-154.0.8037.98`, full CEF commit
`14c5a089e8452874cb1a556dc3fde26ed2d87723`. The installed life-span,
request-context and cookie headers supply the callback/flush contracts. Native
source inspected at the matching commit:

- https://raw.githubusercontent.com/chromiumembedded/cef/14c5a08/libcef/browser/alloy/alloy_browser_host_impl.cc
- https://raw.githubusercontent.com/chromiumembedded/cef/14c5a08/libcef/browser/native/browser_platform_delegate_native_mac.mm
- https://raw.githubusercontent.com/chromiumembedded/cef/14c5a08/libcef/browser/native/browser_platform_delegate_native_win.cc
- https://raw.githubusercontent.com/chromiumembedded/cef/14c5a08/libcef/browser/native/browser_platform_delegate_native_linux.cc
- https://raw.githubusercontent.com/chromiumembedded/cef/14c5a08/libcef/browser/native/window_x11.cc

`CloseContents` calls `DoClose`; returning true resets destruction state instead
of destroying the browser. A successful native destruction must subsequently
call `WindowDestroyed`, which sets `window_destroyed_` and re-enters CEF's close
implementation without calling `DoClose` or targeting a top-level host again.

- macOS default `CloseHostWindow` calls `performClose:` on the child's NSWindow,
  which would close GPUI. `DoClose` returns true and queues ONLY the CEF
  `CefBrowserHostView` for `removeFromSuperview` after the callback returns.
  This wrapper's `dealloc` calls `WindowDestroyed`. The outer clip is retained
  until `OnBeforeClose` has returned. Delayed native release stays pending.
- Windows default `CloseHostWindow` posts `WM_CLOSE` to `GA_ROOT`, which would
  close GPUI. `DoClose` returns true and queues `DestroyWindow` on ONLY the CEF
  child HWND. CEF's `WM_NCDESTROY` calls `WindowDestroyed`. Native API failure
  produces a sticky failure; no parent close/repeated close fallback exists.
- Linux CEF's `CloseHostWindow` targets `CefWindowX11::Close`, not GPUI's parent.
  `DoClose` returns false on Linux to use that exact path: WM_DELETE_WINDOW,
  accepted `TryCloseBrowser`, child destruction, then `WindowDestroyed`.
  Raw XDestroyWindow is deliberately not used: this source calls
  `WindowDestroyed` in its WM_DELETE_WINDOW handler. The repository's current
  native embedding/reparenting path is X11. Native Wayland host support is not
  implemented by this change; unsupported/missing close callbacks stay pending.

These are source-backed implementations, not native acceptance results. macOS
child deallocation and Windows/Linux close event delivery still need real-app
validation. The parent must also handle any unsolicited native/page self-close
in its UI lifecycle; keeping stale UI views after a native browser closes is
not a supported path to continuing page operations.

## Validation and persistence limits

CEF-free unit tests in `src/shutdown.rs` cover exact lease retention, independent
owner/browser/flush gates, repeated quit/finalization, missing callbacks,
immediate rejection, late callbacks, unknown browsers, creation reentrancy and
queue ownership without a Runtime cycle. No Cargo, builds, native apps or Git
write commands were run for this subtask. Parent must compile and run the full
suite and real native acceptance after integrating pre-quit.

Required acceptance remains a FULL process restart of Default and at least two
named contexts, checking session/persistent cookies and localStorage isolation,
normal/private/fork/inspector owners, removed/closing pages, repeated quit,
last-window/native quit, and no surviving engine/browser/flush diagnostics.
Cookie flush alone does not prove Preferences or localStorage persistence.
The observed Crashed preference/missing restore preference/session cookie is
research evidence with unproven root cause, not a fixed diagnosis. No shutdown
state, cookie callback, or dropped handle authorizes physical profile deletion.
