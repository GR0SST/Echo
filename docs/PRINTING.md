# Native printing

File → Print, Cmd+P inside GPUI or a native document, and Command Bar →
Print Page share one action. It targets the focused native document, including
an active split pane or a retained Library preview. Internal pages, Library
Spaces, absent native views, and unsupported backends disable the action.

## Dispatch and ownership

The browser captures the tab, profile, URL, native attachment identity, and
window-local request generation. It closes competing popups and queues dispatch
after their removal and focus restoration. Owned-popup removal receives a
250 ms activation grace period because the native parent key notification can
arrive later than the removal callback. Only an actually active original window
can dispatch; expiry cancels the request, so a later activation cannot revive it.
No popup-free inactive request receives this grace. Every retry validates the original
window and document; navigation, profile reassignment, closure, quit, a newer
popup, or a newer print request invalidates stale work.

The engine prepares a one-shot request retaining the exact CEF browser, context,
and runtime. A weak logical-view token prevents a retired view from printing,
even when a download retains its native browser. Busy creation, pumping,
shutdown polling, or retirement defers dispatch. Native Print executes after
all App, window, Pages, model, engine and lifecycle borrows have ended, because
the platform dialog can enter a nested event loop.

`Submitted` means CEF received the request. It does not establish successful
printing, PDF creation, or user acceptance. Cancellation does not mutate tabs
or create another document.

## Platform scope and validation

macOS and Windows use CEF BrowserHost Print. Linux remains unavailable until
its required CefPrintHandler is implemented. Windows native behavior is not yet
validated. The embedded macOS CEF path uses the system print dialog rather than
a Chromium browser-UI preview.

Eleven GPUI regressions cover common entry routes, split selection, Library
eligibility, busy retry, stale targets, root closure, popup supersession,
UI-only backends and reentrant dispatch outside borrows. The full workspace passes 1,033 tests.

The macOS QA bundle opened the real system Print panel through File → Print
and the native-page shortcut. The final clean bundle also opened it through
Command Bar after the activation-handoff correction. Cancel was selected without changing printer
settings or submitting a job. The automation timer pauses inside the modal
panel, so native accessibility control performs Cancel; screenshots produced
by the resumed script are post-cancellation evidence, not panel screenshots.

Physical printing, PDF output, renderer failure, closure while a panel is open,
Windows/Linux native acceptance and exact Arc dialog parity remain unverified.
