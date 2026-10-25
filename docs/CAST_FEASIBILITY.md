# Cast feasibility (2026-10-26)

The CEF 154 distribution used by Echo includes `CefMediaRouter` and per-request-context `GetMediaRouter`. It can discover local Cast/DIAL sinks, observe routes and create receiver-app routes. This is not equivalent to Arc's page-menu Cast action.

CEF's [router implementation](https://raw.githubusercontent.com/chromiumembedded/cef/master/libcef/browser/media_router/media_router_impl.cc) explicitly rejects HTTP/HTTPS sources because tab/desktop mirroring is unsupported (GetSource). Its [public interface](https://github.com/chromiumembedded/cef/blob/master/include/cef_media_router.h) accepts known Cast/DIAL application URNs. Echo uses Alloy-style embedded page hosts (`crates/engine/src/lib.rs`); opening Chrome's native Cast dialog would also violate the GPUI-only browser-UI boundary.

**Conclusion:** receiver-application routing is feasible; full Arc tab/desktop casting is not available through the present public CEF bridge. Do not expose a receiver-only picker as completed Cast parity. A future engine project must expose tab/desktop mirroring, source identity, permissions, discovery and route lifetime to GPUI, then qualify against physical receivers on each platform. No implementation or claim of hardware validation is made by this feasibility review. Cast stays missing; it is not reclassified as out of scope.
