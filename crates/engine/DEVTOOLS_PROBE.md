# Embedded DevTools: local transport and loading probe

This is an engine foundation, not working user-facing DevTools. In particular,
there is **no renderer InspectorFrontendHost adapter yet**. The original bundled
frontend is requested as an ordinary Alloy content view; whether CEF 154 permits
that internal URL and supplies its assets remains a native runtime gate.
Do not mark `docs/ARC_PARITY.md` complete from this change.

## Integration

Use the same GPUI native parent pointer already passed to `create_view`:

```rust,ignore
let inspector = engine.open_inspector_probe(
    inspected_view,
    ui_view,
    right_hand_content_bounds,
    engine::InspectorPanel::Elements, // Console and Network are also recorded.
)?;

// After each Engine::pump(), outside any borrow held across pumping:
let page_events = engine.take_events();
let inspector_events = engine.take_inspector_events();

// Native-only transport probe, independent of frontend bootstrap:
let accepted = engine.send_inspector_probe_message(
    inspector,
    r#"{"id":1,"method":"Runtime.evaluate","params":{"expression":"document.URL","returnByValue":true}}"#,
);
// Continue pumping and drain InspectorEvent::Protocol; id is restored to 1.

engine.set_inspector_frame(inspector, updated_bounds);
engine.set_inspector_visible(inspector, true);
engine.close_inspector(inspector); // Closes only the frontend.
```

The example is schematic: the parent owns layout and handles the result type.
No browser menu, shortcut, focus operation, native automation or GPUI dock UI is
added here. Repeated opens for one inspected view return the same InspectorId
and update the requested panel. The stored panel is not yet sent to a frontend.
Closing the inspected page also drops its inspector and observer.

`InspectorStatus` exposes inspected ViewId, monotonic generation, requested
panel, observed URL and phase. Opening means only browser creation succeeded.
FrontendLoaded means only the exact requested URL stopped loading; an error
document or failed JavaScript bootstrap still requires native inspection.
LoadFailed includes the CEF main-frame error in InspectorEvent::LoadFailed.
FrontendReady is reserved and is never produced by this probe. There is no
frontend readiness handshake or renderer protocol IPC to trust accidentally.

The frontend view uses the inspected host's actual RequestContext. Missing
context fails closed; after creation, CEF `is_same` verifies context equality,
and `runtime_style` verifies Alloy. Either mismatch closes the probe. These
checks run for normal and private contexts. No global-context fallback is used
for inspectors, and the retained context and observer last until close.

Inspector callbacks are excluded from tab-facing title, URL, favicon, loading,
popup, focus, find, edit-focus and shortcut events. Inspector IDs cannot be used
as public ViewIds. Use the separate inspector event queue for diagnostics.

## Local protocol

No remote-debugging port or websocket is enabled. BrowserHost's observer copies
raw bytes to an Arc/Mutex queue and returns unhandled so snapshot observers still
receive their method callbacks. JSON parsing and routing happen when polled.

Frontend request IDs are remapped into the same checked monotonic allocator
used by snapshots. Response IDs are restored, child sessionId is preserved,
and unrelated/snapshot replies are discarded. Duplicate IDs are rejected within
the same child session. Failed submission frees correlation without reusing the
wire ID. Close/reopen uses a new generation; late callbacks cannot reach the
new session. Detach clears pending requests and blocks further sends.

The native probe API is trusted Rust integration, not renderer authorization.
Before adding renderer IPC, validate browser role, exact main-frame identity,
exact internal URL and current generation on every message. No existing page
IPC handler accepts a DevTools protocol message.

## Parent native probe

1. Build the bundle with these engine changes and the existing CEF 154 assets.
   In a temporary parent-owned harness, use a local inspected document with a
   stable DOM UUID. Place the probe on the right inside the same content card.
2. Observe Status/LoadFailed output and the rendered child view. Confirm the
   exact `devtools://devtools/bundled/inspector.html` URL serves the original
   assets. A matching URL or FrontendLoaded alone is insufficient evidence.
3. Send Runtime.evaluate through the native probe API; confirm the response
   identifies the inspected document, not the frontend. Run a snapshot request
   concurrently and confirm both callers receive their own result.
4. Close/reopen the inspector and verify the inspected DOM UUID survives.
   Repeat with private and regular inspected pages, using distinct fixtures;
   verify neither context can read the other's cookies/storage.
5. Only after the asset gate passes, install an adapter before frontend module
   initialization in renderer on_context_created. Use process messages and the
   identity validation above. Feed backend messages to the original
   InspectorFrontendAPI.dispatchMessage; send frontend requests through the
   existing remapping transport. A verified handshake, not load completion,
   should publish FrontendReady. Apply requested panels using the original
   frontend host/connection architecture after initialization.

If Alloy refuses the internal URL, stop at the reported native error. There is
no ShowDevTools, Chrome-style browser, separate native window, remote frontend,
or rewritten UI fallback in this change. Resolve original same-version bundled
asset serving before continuing the host adapter.

Parent's Arc observation: in the temporary normal window, CmdOptI toggled an
embedded right dock with Elements selected; CmdOptJ toggled Console. Closing
preserved the DOM UUID and the sidebar stayed unaffected. This is the current
profile's docking preference, not evidence of a factory default. No native
inspection was performed by this implementation agent.

## Source architecture reviewed

The official frontend's MainConnection sends through
InspectorFrontendHostInstance.sendMessageToBackend and listens for the host's
DispatchMessage/DispatchMessageChunk events. InspectorFrontendHost installs a
custom host and the original InspectorFrontendAPI dispatcher. Missing host
methods may fall back to the hosted stub; that must not be mistaken for an
embedded frontend being connected. References reviewed:

- https://raw.githubusercontent.com/ChromeDevTools/devtools-frontend/main/front_end/core/sdk/Connections.ts
- https://raw.githubusercontent.com/ChromeDevTools/devtools-frontend/main/front_end/core/host/InspectorFrontendHost.ts
- The corresponding host, Connections and devtools_compatibility source in the
  parent's Chromium checkout. These architecture references are not proof of
  the exact CEF 154 bundled host ABI; that remains to be checked before adapting.

## Validation

`CEF_PATH=/Users/grosstgrosst/Documents/Bot/ChromiumFork/third_party/cef cargo test -p engine`

Tests cover shared snapshot allocation, response remapping, flattened child
sessions, snapshot reply filtering, protocol errors, duplicate and malformed
requests, allocator exhaustion, generation isolation, repeated panel requests,
load failure, detach, observer/context binding lifetime, private/regular state
and reply isolation, and exclusion of inspector callbacks from tab events.
Private native cookie isolation and actual bundled loading require the parent
probe; opaque fake binding tests do not claim to prove CEF storage behavior.

## Native asset gate result (2026-10-23)

The parent loaded the exact bundled inspector.html URL in an ordinary Alloy
child under the existing GPUI parent. Screenshots showed the original Elements,
Console and Styles frontend, with no separate CEF-owned top-level window.
The frontend remained disconnected, as expected without its host adapter.
On a live localhost fixture, concurrent Runtime.evaluate and Page.captureScreenshot
both completed through their own correlations. Closing and reopening the probe
retained the inspected document's diagnostic UUID. The native run was guarded
for focus and screenshots were inspected. This proves asset loading and local
transport, not frontend readiness, working panels or private storage isolation.
