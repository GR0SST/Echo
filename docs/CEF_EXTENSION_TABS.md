# Echo extension tab and window metadata

The source bridge reads tab and window state from Echo's model. Alloy pages do
not have a Chrome `BrowserWindowInterface`; constructing a hidden Chrome window
would introduce a second source of truth and is not the implementation.

The approved surface is `tabs.query/get/getCurrent/create/update/remove/reload`,
`tabs.onActivated/onUpdated/onRemoved/onCreated`, ordinary `tabs.sendMessage`,
`windows.get/getCurrent/getLastFocused/getAll`, and `windows.onFocusChanged`.
All query filters, profile boundaries and Chromium permission scrubbing apply.

## Identity and ownership

Echo sends complete profile snapshots containing its windows and open tabs,
including tabs whose renderers have not been created yet. Internal model keys
are strings, not JavaScript numbers, so a Rust `u64` is never rounded. Native CEF
allocates session IDs for those keys. A page adopts its reserved ID before
Chromium attaches extension helpers; that ID remains stable when a lazy tab is
opened or its renderer is replaced. Existing pages retain their original IDs.
The normal Chromium messaging and activeTab permission machinery therefore use
the same IDs exposed in query results.

A snapshot is submitted through an existing Alloy browser in its exact CEF
profile. Native code validates the complete snapshot before replacing state.
Hidden service hosts, DevTools and popup WebContents are not model tabs. Service hosts use a dedicated creation path that cancels
CEF navigation/system focus requests. A hidden 1×1 page must not become a
keyboard target while an empty window's Command Bar is opening. The ordinary
page creation path still accepts focus; no service-host tab identity is invented
to forward its input. The native private-window write/restart matrix covers this
startup race as well as cookie isolation.
Windows and tabs have one model owner; a moved tab keeps its ID. Focus changes
come from GPUI activation observations, including loss of application focus.

## Reads, mutations and events

Native read adapters serialize the model's state, using Chromium's normal
`ExtensionTabUtil` permission filtering for URLs, titles, favicons and pending
URLs. An action popup resolves its current window through its exact owner;
background workers use the model's last-focused window in the same profile.
There is no global-profile fallback.

Mutations use a bounded asynchronous request/response queue. The originating
extension, profile, request and model keys are retained by CEF. GPUI validates
that the target still belongs to that profile, performs the ordinary browser
action, publishes the resulting snapshot, and completes the request. Chromium
validates navigation URLs before they reach GPUI. Closing the profile or owner
cancels outstanding work. A completion never approves an extension install.

Events are derived from successive accepted snapshots and are dispatched per
extension, with permission filtering before delivery. `tabs.sendMessage` keeps
Chromium's existing sender/frame/document routing and uses the page's stable
session ID; no renderer JavaScript API is replaced.

The native fixture covers all 18 `tabs.query` filter keys in the pinned schema,
including positive combined matching and a negative check for each key. A second
fixture omits tab/host permissions and verifies both field redaction and that URL
filters cannot leak tab existence through result counts. The GPUI acceptance
fixture exercises create/update/remove/reload, stable lazy renderer IDs, normal
content-script messages and sender metadata, lifecycle events and application
focus loss/restoration. Run it against a frozen candidate bundle:

```sh
ECHO_CEF_DIST=/absolute/path/to/candidate cargo run -p bundle
ECHO_CEF_DIST=/absolute/path/to/candidate bun scripts/check-cef-extension-model.ts
```

The checker creates its own `Echo Test` profile and loads the unpacked fixture
through the native bridge in that process only. Its explicit automation loader
requires a debug build, `BROWSER_SCRIPT`, and `ECHO_EXTENSION_ACCEPTANCE=1`.
Chromium's unsafe extension loading switch is absent from ordinary launches.
The harness uses ordinary extension APIs and real input; it does not attach a
debugger. Native and JSON results retain the exact framework/executable hashes.

Loading and navigation events force a fresh native projection even when GPUI's
intended URL is unchanged. Event diffs retain the preceding permission-scrubbed
visible values, so a later renderer commit emits `onUpdated.url` and a cached
reload does not collapse its loading/complete transitions. Split IDs come from
the GPUI model and stay stable across repeated observations and window handoffs.

The published default is generation `20261025T192350Z-5008`, framework SHA-256
`55a60b71fd32579a392ee53f337a908cecb7b0f012971d6ad3317b4a32ec82aa`.
It passed codec, popup/profile isolation, all 20 GPUI model checks, sixteen clean
shutdown exits, recovery, browsing-data and native PiP gates, plus real uBlock
Origin Lite and Dark Reader checks on HTTPS. [Build and evidence record](CEF_BUILD.md#published-metadata-bridge-generation).
The management/pinning/shortcuts UI retains a separate Arc parity gate.
