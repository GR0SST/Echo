# Embedded DevTools renderer bridge

Engine foundation: `f4c23b0`. The parent passed the connected native v4 gate on
the immutable qa-v4 bundle using `inspector.html`: authenticated FrontendReady,
observed Elements, actual inspected DOM and Styles, Console evaluation, Network
requests, and node picking. Evidence and remaining gates are below. The current
desktop entrypoint `devtools_app.html` also passed native retesting without the remote screencast panel.
The implementation agent has not launched an app or used native input/focus.
These gates do not establish complete Arc parity.

## Integration API

```rust,ignore
let id = engine.open_inspector_probe(
    inspected_view, gpui_native_parent, content_bounds, InspectorPanel::Elements,
)?;
// Historical probe name retained. This now installs the real renderer host.
// For Inspect Elements, call immediately; readiness is handled by the engine:
engine.toggle_inspector_element_mode(id);

// After every Engine::pump(), outside borrows held across pumping:
let page_events = engine.take_browser_events(); // Ordered page/frontend input.
let inspector_events = engine.take_inspector_events();
// The second call SERVICES the bridge, including bootstrap protocol traffic.
// Keep calling it while opening; do not wait for FrontendReady before polling.

engine.show_inspector_panel(id, InspectorPanel::Console);
engine.show_inspector_panel(id, InspectorPanel::Network);
engine.focus_inspector(id);
engine.set_inspector_frame(id, content_bounds);
engine.set_inspector_visible(id, true);
engine.close_inspector(id); // Only the frontend closes; inspected DOM survives.
```

- `open_inspector_probe` coalesces by inspected ViewId and requests the panel.
  It does not implement Arc's menu/shortcut toggle decisions. Those belong to
  GPUI. The frontend uses the inspected host's actual RequestContext, verified
  with CEF `is_same`; there is no global-context fallback. Alloy is also checked.
- `show_inspector_panel(id, panel) -> bool`: accepts a request for an existing
  session. Before readiness the latest request is retained. After readiness it
  calls original `InspectorFrontendAPI.showPanel`. Acceptance does not prove
  panel selection; use `selected_panel` / `PanelChanged`.
- `toggle_inspector_element_mode(id) -> bool`: true means accepted. Pre-ready
  calls XOR one pending toggle, dispatched exactly once after readiness. Two
  early calls cancel each other. Initial context acquisition preserves intent;
  established-context navigation/release, failure, detach and close clear it.
  Ready calls each invoke original `InspectorFrontendAPI.enterInspectElementMode`.
  The original frontend owns node-pick exit and cancellation. Unknown/terminal
  sessions return false. Do not add a second parent-owned ready retry.
- `focus_inspector(id) -> bool`: focuses the CEF frontend without exposing its
  private ViewId. Unknown/detached sessions return false. This API was not
  invoked by the implementation agent.
- Existing `inspector_status`, frame/visibility setters, `close_inspector`, and
  trusted native `send_inspector_probe_message` remain available. Native probe
  replies are correlated separately and returned as `InspectorEvent::Protocol`;
  renderer replies are delivered internally. InspectorId remains opaque.

`InspectorStatus.panel` is the latest **requested** panel.
`InspectorStatus.selected_panel: Option<InspectorPanel>` is a supported observed
selection, never filled from that request. Original main-tab metrics report
Elements/Console/Network. Manual changes also report through the original
`panel-selected-tab` preference. Other/ambiguous panel reports invalidate the
known selection (`None`). A pending different panel request invalidates a stale
observation until the original frontend reports its actual selection. Reopening,
context replacement and revocation never reuse an old selection.

Inspector-only events:

- `Status(InspectorStatus)` includes lifecycle and observed selection updates.
- `PanelChanged { inspector, panel: Option<InspectorPanel> }` reports a changed
  observation. `None` means the supported main selection is unknown.
- `CloseRequested { inspector }` comes from original `Host.closeWindow`; the
  parent must close/layout the inspector. The engine does not destroy a CEF
  browser reentrantly in a V8 callback.
- `FocusRequested { inspector }` comes from original `Host.bringToFront`; the
  parent decides visibility and focus. `Focused { inspector }` is a native
  `on_got_focus` report, allowing GPUI to track frontend ownership.
- `Shortcut { inspector, shortcut }` routes the same configured window shortcuts
  as pages, without turning an inspector into a tab. Validate its owning
  inspected ViewId, then use the browser action path. F12 uses CEF's platform
  neutral Windows VKEY `0x7b`, including when unmodified_character is zero.
- `Detached`, `LoadFailed`, and native diagnostic `Protocol` remain separate.

Title/URL/favicon/loading/edit-focus/find/popup callbacks from the frontend do
not become tab metadata. GPUI docking, commands, menu toggles, action ownership,
close on user request, and keyboard focus policy are parent integration work.

## Exact bundled ABI and bootstrap

The installed CEF 154 `resources.pak` was independently decoded locally (v5 PAK,
Chromium Brotli envelope `1e 9b` + eight-byte header). Relevant IDs:

- 51409: desktop HTML; 51412: desktop module; 51423: default app registration.
- 51434: remote inspector HTML; 51418: remote inspector module.
- 51395: Host and original InspectorFrontendAPI.
- 51405: SDK / MainConnection; 51420: InspectorMain; 51424: Main bootstrap.
- 51766: Elements implementation; 51929: legacy UI and UIUserMetrics.

`editing.rs::on_context_created` installs a direct global InspectorFrontendHost
synchronously before frontend modules. Browser role comes only from native
`extra_info` in renderer `on_browser_created`. The explicit host methods cover
the installed stub prototype; nothing inherits hosted-stub methods/private fields.
Blink subsequently injects bundled `devtools_compatibility.js` (51410), which
unconditionally assigns an `InspectorFrontendHostImpl`. The authenticated
document therefore retains its native host through a nonconfigurable accessor
with an assignment-tolerant setter. Original `core/host/host.js` reads that host,
adds its original event dispatcher and creates the original InspectorFrontendAPI.
The Bun regression executes the full compatibility script, exact host-module
startup/API class/descriptors, and exact MainConnection in that native order.
This fixes the overwrite observed in the v3 native log; the parent confirmed
connected native acceptance in `/tmp/echo-devtools-bootstrap-v4.log`.
`isHostedMode()` is false, selecting the original MainConnection. Config and
preference callbacks resolve synchronously. Preferences are document-local,
bounded in-memory values; no host preference uses localStorage, filesystem,
account sync or another inspector's map. Existing private RequestContext
isolation is retained; native regular/private cookie isolation still needs the
parent acceptance test.

The sole trusted frontend URL is now
`devtools://devtools/bundled/devtools_app.html?can_dock=true` (exactly this
query, no fragment).
The former `inspector.html` entrypoint imports the desktop module plus
`screencast-meta.js`, whose order-1 ScreencastApp overrides the order-10 SimpleApp
and starts with screencast enabled. This is a remote-device presentation, not a
GPUI docking setting. The original desktop entrypoint uses SimpleApp directly,
showing InspectorView in RootView without the remote preview/navigation panel.
`can_dock=true` selects the frontend's own docked presentation, including its
toolbar close button (`Host.closeWindow`), as in Arc; GPUI still owns the
containing dock and only the right side is laid out. No `dockSide`, synthetic
preference, hidden DOM or custom frontend is needed. The Bun regression checks the exact
installed entrypoint resources and executes the original SimpleApp registration
and presentation. The URL allowlist remains exact equality, rejecting the old
entrypoint, the bare URL, other query parameters, fragments and lookalike hosts.

Original frontend modules create InspectorFrontendAPI. The engine never
substitutes that dispatcher or the frontend UI. Static host bootstrap is the
only evaluated bridge source. Backend JSON and control arguments are V8 string
arguments to a retained function, then original `dispatchMessage`, `showPanel`,
and `enterInspectElementMode`; data is never concatenated into evaluated code.

Backend messages flow **before** connectionReady. MainConnection creates host
event listeners before installing its message callback. The host buffers early
backend messages until its first `sendMessageToBackend`, which the installed SDK
only performs once `setOnMessage` is bound. That avoids both lost early messages
and the boot deadlock of waiting for connectionReady before answering requests.
At `loadCompleted`, original `setUseSoftMenu(true)` is dispatched.

FrontendReady requires, in the current authenticated document:

1. `loadCompleted`;
2. a successful CDP reply correlated against a renderer-origin request;
3. renderer acknowledgement after dispatching that reply to the original API;
4. original `connectionReady` and `readyForTest` (target initialization finished).

Load completion, URL equality, native diagnostic replies, snapshot replies,
errors, uncorrelated IDs, and forged/stale acknowledgement keys cannot satisfy
readiness. Initial requested panel and pending inspect toggle run after this
handshake. No timeout invents readiness.

Exact legacy UI evidence: `InspectorView.tabSelected` calls
`UIUserMetrics.panelShown`; that calls
`Host.recordUserMetricsAction('DevTools_PanelShown_' + name)`. Its supported main
names are observed rather than inferred from requested panels. The tabbed
location writes `<location>-selected-tab` only for user-gesture selection; the
main location is `panel`. The close button invokes `Host.closeWindow`. These
signals are checked against installed resource 51929 in the Bun tests.

## Authentication, transport, lifecycle

Both IPC directions require the immutable native inspector browser role,
monotonic InspectorId/generation, exact main-frame **string** identifier, exact
internal URL without query/fragment, current context epoch, correct process
direction, message name/version, five string arguments, supported operation,
payload schema and bounds. Normal page callbacks retain their existing path.
Renderer V8 calls additionally compare the current V8Context with the retained
live context. Browser IPC uses a per-inspector native client, not a URL-derived
role or renderer-supplied browser ID.

Context release, accepted main-document navigation, crash, detach, load failure,
and close revoke the capability and discard renderer request correlation,
readiness and pending inspect intent. A cancelled foreign navigation preserves
the original live context/transport. Queries and lookalike hosts are rejected.
Renderer crash is terminal for that session; close/reopen allocates a new
InspectorId, avoiding renderer epoch reuse after a process restart. Close drops
the observer and retained context without navigating the inspected page.

CDP IDs share the checked monotonic allocator with snapshots, but renderer and
trusted diagnostic correlations are separate. Flattened child sessionId routing
is preserved, including sessionless errors for vanished children. Old replies
cannot reach a new document, close/reopen generation, private inspector, or
snapshot caller. Notifications remain available to the original frontend and
CEF screenshot observers (observer returns unhandled). Protocol input is limited
to 16 MiB, pending requests to 8192; the native inbox is bounded by count and
32 MiB aggregate data. Overflow or renderer transport submission failure is
terminal rather than silently losing messages and continuing as ready.

## Limitations and native acceptance

The host is an engine bridge, not a complete desktop embedder service layer.
Optional ABI methods are explicit unsupported/no-op implementations. Native
clipboard, external links/windows, file saves/workspaces/search/indexing,
resource loading through the host (returns a structured 404), extensions,
surveys, sync, AI services, zoom/docking UI, certificates, eye dropper, discovery,
and telemetry are not integrated. Soft menus avoid unsupported native context
menu routing. Preferences reset with the frontend document/inspector lifetime.
The selected-panel contract covers Elements/Console/Network; other panel reports
clear the known selection rather than claiming an incorrect supported panel.

Parent-reported native v4 evidence (before the desktop entrypoint correction):

- `/tmp/echo-devtools-bootstrap-v4.log` and
  `/tmp/echo-devtools-connected-ready-elements.png`: FrontendReady, observed
  Elements, real `connected-live` DOM marker and correct Styles. Closing retained
  the typed input and original fixture UUID.
- `/tmp/echo-devtools-console-evaluation.png` and
  `/tmp/echo-devtools-console-toggle-closed.png`: product Cmd+Option+J evaluated
  `document.querySelector('#marker').value` as `console-marker`; repeating the
  shortcut from the frontend closed the dock and typing appended `after-console`
  to the original input, with the same DOM UUID.
- `/tmp/echo-devtools-network-open.png` and
  `/tmp/echo-devtools-network-request.png`: product Command Bar Network Inspector
  opened observed Network and captured the real `/payload?request=1` HTTP 200
  after the page button click.
- `/tmp/echo-devtools-inspect-picked.png`: Inspect Elements opened; moving to
  the actual page heading before clicking selected `h1#fixture-heading`, showed
  its Styles, and exited the blue pick mode. Click-only automation lacked the
  hover needed for picking; the successful move-and-click supersedes that run.

Parent verified the desktop entrypoint on immutable qa-v5:

- `/tmp/echo-devtools-desktop-console.png`: Console evaluates the live input as
  `desktop-live`; there is no remote screencast preview.
- `/tmp/echo-devtools-desktop-network-response.png`: original Network Response
  displays the fixture JSON body after the real page button fetch.
- `/tmp/echo-devtools-desktop-picked-heading.png`: picking selects the actual
  `h1#fixture-heading` and its styles.
- `/tmp/echo-devtools-desktop-closed.png`: closing the frontend preserves the
  document UUID and permits typing into the original page.
- `/tmp/echo-devtools-split-frontend-close-owner.png` (qa-v4): A frontend focus
  after typing in B makes Cmd+W close A, retaining B input and document.

Remaining native coverage includes
manual tab selection, frontend close request, F12, navigation cancellation,
reload/context replacement, snapshot coexistence and regular/private isolation.
This agent performed no native
app, focus, mouse, commit or push action. No ShowDevTools, remote debugging port,
websocket, network frontend, top-level CEF chrome or browser UI change was added.

## Validation

```sh
CEF_PATH=/Users/grosstgrosst/Documents/Bot/ChromiumFork/third_party/cef cargo test -p engine
CEF_PATH=/Users/grosstgrosst/Documents/Bot/ChromiumFork/third_party/cef bun test crates/engine/tests/devtools_host.test.js
```

Rust tests cover transport correlation, snapshot/diagnostic/renderer isolation,
child-session errors, bootstrap ordering, delivery-proof readiness, epoch/frame/
generation rejection, revoke/detach/close, schema/size/overflow, observed panel /
close/focus events, pre-ready inspect parity and cancellation, cancelled foreign
navigation, F12, shortcut routing and tab metadata filtering. Bun tests execute
the host adapter without a native browser, compare all installed Host prototype
methods, and execute the exact installed MainConnection and UIUserMetrics
classes extracted from PAK resources, plus actual compatibility/Host bootstrap
ordering and desktop SimpleApp presentation. Latest implementation-agent run:
8 Bun tests passed, 82 assertions. Cargo was not run by this agent; the parent
ran the full workspace: 348 browser, 124 core and 26 engine tests passed. Temporary success, rejection, protocol and
frontend-console logging has been removed. Remaining engine errors identify
bootstrap/delivery stages without logging protocol content or source URLs.
The PAK checks skip without CEF_PATH and fail if the expected installed ABI drifts.
