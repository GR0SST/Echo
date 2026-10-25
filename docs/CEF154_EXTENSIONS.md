# CEF 154 extension feasibility

Baseline investigation for Arc extension parity, 2026-10-25. The findings below
describe upstream CEF. An owner-approved, opt-in source bridge now proves a real
Alloy MV3 popup, profile isolation, catalog/toggle callbacks and verified Web Store
installation. Build, qualification evidence and remaining limits are recorded in
[CEF_BUILD.md](CEF_BUILD.md#opt-in-extension-bridge). GPUI extension UI is a separate task.

Echo pins `cef = 154.5.0`, backed by CEF
`154.0.34+g14c5a08+chromium-154.0.8037.98` (CEF commit
`14c5a089e8452874cb1a556dc3fde26ed2d87723`, Chromium commit
`b859317bf11f6be47f9b7799ec690a0a42a1fb33`). It embeds native child views with
`RuntimeStyle::ALLOY` in `crates/engine/src/lib.rs`. On macOS, supplying
`CefWindowInfo.parent_view` forces Alloy, including when Chrome style is requested.
Changing that enum is therefore not an extension solution for this embedding.
CEF Views-hosted Chrome windows are an alternative host architecture, not a
drop-in native child-view replacement; that migration was not prototyped.
See the [pinned runtime definitions](https://github.com/chromiumembedded/cef/blob/14c5a089e8452874cb1a556dc3fde26ed2d87723/include/internal/cef_types_runtime.h)
and [macOS window definitions](https://github.com/chromiumembedded/cef/blob/14c5a089e8452874cb1a556dc3fde26ed2d87723/include/internal/cef_types_mac.h).

## Findings

The old `CefExtension`, `CefExtensionHandler`, and
`CefRequestContext::LoadExtension` API was removed in M128. This is separate from
the continued existence of Alloy *style* under the Chrome bootstrap.
[CEF migration issue](https://github.com/chromiumembedded/cef/issues/3685).

| Capability | CEF 154 / implication for Echo |
|---|---|
| Unpacked MV3 loading | Startup `--load-extension` and browser-target CDP `Extensions.loadUnpacked` succeeded in an isolated Echo process; its service worker started. No old Alloy API is needed for this limited capability. |
| Enumeration | Browser-target CDP `Extensions.getExtensions` returned the probe. The page-target calls returned `Method not available.`; browser-target calls succeeded. The pinned implementation enumerates unpacked extensions only and uses the default browser context; this is not a complete per-profile catalog for installed Web Store extensions. |
| Content scripts in Alloy | Verified in two Alloy fixture targets: the MV3 document-start content script wrote its extension ID and inserted a visible marker; it still ran after reload. This is useful support, not full action/installation parity. |
| Packaged CRX | No public CEF per-context CRX installer API. CDP `loadUnpacked` takes a directory, not CRX. Chrome's installer machinery exists internally; direct signed-CRX installation in Echo was not validated. |
| Web Store installation | Chrome-style browsers expose Chrome's store/installation UI. An HTTP store page loading in Alloy does not prove its install button works. Echo needs installation permission continuations and completion/error events for GPUI; these are not public CEF extension APIs. No Web Store installation was performed in the owner's Arc profile. |
| Action popups / pinning / shortcuts | No public CEF action catalog, action invocation, pinning, or extension-command API for Alloy. Chromium CDP `triggerAction` dereferences a Chrome `BrowserWindowInterface`, its `ExtensionsContainer`, and toolbar action model. It is not a safe substitute for a GPUI action host; it was deliberately not invoked on Alloy. |
| Manage Extensions | `chrome://extensions` is excluded from Alloy's WebUI allowlist. Native navigation returned `net::ERR_ABORTED` and retained the fixture. A separate Chrome-style window can display it, but making that the product management UI violates this repository's GPUI architecture. |

The [pinned CDP implementation](https://github.com/chromium/chromium/blob/b859317bf11f6be47f9b7799ec690a0a42a1fb33/chrome/browser/devtools/protocol/extensions_handler.cc)
provides the enumeration/context and action dependencies above. CEF's maintainer
explicitly distinguishes request-context functionality from unsupported Alloy
Chrome-toolbar functionality in [issue 3859](https://github.com/chromiumembedded/cef/issues/3859).
The [pinned Alloy allowlist](https://github.com/chromiumembedded/cef/blob/14c5a089e8452874cb1a556dc3fde26ed2d87723/libcef/browser/alloy/alloy_browser_host_impl.cc)
and [Chrome-only toolbar API](https://github.com/chromiumembedded/cef/blob/14c5a089e8452874cb1a556dc3fde26ed2d87723/include/views/cef_browser_view.h)
confirm the UI boundary.

## Approved source-patch experiment

Keep GPUI and Alloy embedding. Add an opt-in CEF bridge backed by Chromium's
existing extension services, rather than restoring the removed old Alloy API or
hosting a hidden Chrome toolbar. First prove one vertical slice before committing
to the full feature: a fixture MV3 action opens its real popup on an Alloy
page without a Chrome toolbar, and is isolated from a second profile. If that
requires a broader browser-window rewrite, stop and reassess before expanding
the patch. Proposed bridge scope:

1. Add request-context-owned catalog/change callbacks: ID, localized name,
   enablement, action/icon/badge, required/granted permissions and commands.
   Include packaged extensions. Enable/disable/uninstall must use Chromium's
   services and emit actual completion results. No global-context fallback.
2. Bind action invocation to the exact CEF page/tab and request context. Use
   Chromium's existing extension action/activeTab machinery; expose an
   extension popup WebContents for a client-owned child view, preferred-size
   changes, close/focus, and lifetime cancellation. Do not replace an action by
   navigating a normal tab to its HTML. Reuse CEF's existing
   [Alloy tab lookup](https://github.com/chromiumembedded/cef/blob/14c5a089e8452874cb1a556dc3fde26ed2d87723/libcef/browser/chrome/extensions/chrome_extension_util.cc).
3. Expose Web Store/CRX installation confirmation and lifecycle callbacks using
   Chromium's installer and verification. The existing
   [Web Store flow](https://github.com/chromium/chromium/blob/b859317bf11f6be47f9b7799ec690a0a42a1fb33/extensions/browser/api/webstore_private/webstore_private_api.cc)
   creates install prompts; route their permission decisions to GPUI and cancel
   on originating page/profile teardown. Do not bypass signature, provenance,
   policy, permission, or update checks.
4. Expose registered extension commands and dispatch with the selected tab and
   user gesture. Keep pin order per local profile in Echo; pinning need not
   expose or instantiate Chrome's toolbar.

Likely patch surfaces: new CEF extension bridge header/C API and implementation,
CEF browser/context lifecycle and popup host integration, and narrow Chromium
extension-action/install-prompt hooks where they assume a Chrome window. Exact
file/hunk count requires the vertical-slice build; this is a patch plan, not a
claim that adding one header is sufficient. Target the pinned CEF 154 checkout,
then carry the minimal Chromium changes to the fork's newer branch. Do not modify
`depot_tools` or the repository's `src/` before approval.

After that bridge works, keep all FFI in `crates/engine`, expose owned Rust
records/events to GPUI, and implement the measured Arc grid, action popup anchor,
pin menu, management page, shortcut settings and Command Bar actions. Validate
unpacked and signed Web Store MV3, restart persistence, two profiles, incognito
opt-in, content scripts/messaging/storage, action-only and popup actions,
activeTab grants, uninstall/disable during popup, and close/navigation races.
The bridge's popup/installation proof is the gate for that UI work.

## Native evidence

The isolated probe used a copied Echo bundle, its own profile, and loopback
DevTools with `--load-extension=<fixture>` and
`--enable-unsafe-extension-debugging`. These flags/ports are diagnostic only;
none was added to production initialization. Its `protocol.log` records exact
responses and the build version; both probe processes exited normally.

- `target/parity/extensions/native-alloy/protocol.log`: worker, content script,
  reload, browser/page CDP and blocked WebUI results.
- `target/parity/extensions/native-alloy/content-script.png` and
  `alloy-content-script-crop.png`: visible content-script marker.
- `target/parity/site-controls/extensions-inspect/site-center.png` and
  `target/parity/extensions/arc-extension-grid-crop.png`: measured four-column
  48×32 extension tiles, 8 px gaps, 16 px icons, trailing plus tile.
- `target/parity/site-controls/extensions-inspect2/{manage,store,command-extensions}.png`:
  native menu routes to `arc://extensions/` and the store's Extensions section;
  Command Bar shows “Extension” with an icon/name token. No action was executed.

Sessions use the shared
Echo Test Space and the existing guard reference. No extension was installed,
removed, enabled, disabled, pinned, or unpinned in the owner's Arc profile.
Own fixture, management and store tabs were archived, owned windows closed,
and the fixture server stopped. No replacement Space was created. This
investigation does not close the missing parity rows.
