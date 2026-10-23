# Sidebar resizing integration checkpoint

Integrated in the browser. Full workspace validation passed 751 tests. Native
macOS acceptance covers docked resizing, retained documents/input, min/max, reset,
collapse/reopen and active cancellation. Floating native resizing, exact reference
geometry and remaining platform checks are still open.

## Files and ownership

BrowserWindow owns retained `sidebar_width`, the optional live resize gesture and
its deferred-child generation. Initial width stays 240 for every new host. The
model/ProfileHandle/WindowSelection/session format is unchanged. Width changes do
not publish a shared-model edit, write the session, navigate, or recreate views.

`sidebar_resize_state.rs` contains pure arithmetic; `sidebar_resize.rs` is a child
module of window.rs so existing private host lifecycle fields stay private.
`sidebar_resize_tests.rs` is also included under window for real widget tests.
Pages adds an opaque focus snapshot and identity-checked restoration helpers;
`pages_resize_focus_tests.rs` tests host/document/frontend rejection directly.
There is no engine API change.

## Interaction and layout

The GPUI stripe lies inside the right edge, with ResizeLeftRight cursor. A capture
listener prevents ancestor focus changes. Native focus is captured before routing
to GPUI for cancellation; UI input keeps its focus. Window-level mouse listeners
continue a live gesture outside the stripe/root hitbox. Escape or a lost button
cancels; deactivation/minimization cancel without activating the window. Owner
teardown invalidates the deferred child resize callbacks.

The retained open width and transient width are separate. Changed raw values use
round-nearest-away on beginning width + translation. Raw <82 displays zero width;
otherwise clamp164..500. End normally clamps164..500. The pinned-open host maps to
the observed mode-zero collapse branch: end raw<=82 hides and retains the clamped
BEGIN open width. Cancellation restores the exact beginning width/visibility.
Double-click reset is228 raw; no screenshot offset is added.

All prior instance240 geometry uses host width: clipping, native content inset,
Space body travel, fixed dock/Favorites, hidden sidebar offset, floating panel and
child size, native controls, split content extent and Command Bar split anchoring.
Pager position remains in fractional derived-page units during resizing; resizing
does not change its selected identity or progress. Library cards remain240 and
render their real row/split components under a temporary240 render-width override.
Creation form keeps its existing responsive containment.

Floating child resizing is deferred after releasing host/PageStore borrows, with
both overlay identity and latest resize generation validation. Pending callbacks
cannot resize a replaced/closing child. No CEF pump or await occurs under these
borrows. Native page layout follows existing Pages bounds updates.

## Focus safety

Pages snapshots include the exact store, originating host/native parent, page
identity/profile/generation, current view and attachment generation; inspector
snapshots also retain the frontend ID/generation. Restoration rejects a moved,
retired, hidden, replaced or unselected target. Metadata/event FIFO is untouched.
The host also checks popup generation, current UI ownership and virtual-page
visibility. UI focus is never stolen back from a newly opened popup/editor.
Inactive cancellation restores only validated intent, without engine focus; the
normal activation path revalidates the retained snapshot after reconciling the
authoritative selection and only then activates that original live responder.

## Verified contract versus remaining native gates

Source-backed Arc1.170 arithmetic:164/500 limits,82 threshold,228 reset, nearest-
away rounding and the mode-zero end branch. Official behavior: edge drag,
double-click reset and all-left hide; native cursor name confirmed in bundle.

The initial240 is the preserved Echo baseline, not a newly asserted Arc pixel
measurement. Mapping pinned-open to Arc's semantic mode0 is our host interpretation;
actual tag meaning/floating-end policy remains a native acceptance gate. The4px
inside stripe is an explicit unverified hit-target width. Actual CEF pointer-stream
continuation when crossing native views must be tested in the bundle. Floating
mode and inactive cancellation need native input verification as well.

There is no persistence or New Window inheritance: neither was established by the
reference. Snap eligibility, velocity,0.5s timing/haptics and exact native feel are
incompletely proved and deliberately not invented. Do not mark resize parity
complete until the parent validates these boundaries and native geometry.

## Parent validation

Run the registered browser/workspace tests, then immutable bundle input: actual
edge drag in both directions across CEF content, min/max, exact82 release, live
hide and recovery before release, Escape/lost-button, double-click, page/inspector
and text-input focus, popup opened during drag, floating panel child bounds,
window minimize/close, two shared-workspace windows. Confirm URL/ViewId survives
and there are no Navigate/create calls. Inspect fixed dock/Favorites during pager
animation, fixed240 Library cards, creation form gutters and window controls.
No native parity claim is made by this checkpoint.


## Acceptance checkpoint, 2026-10-23

The immutable resize r2 bundle passed native docked input checks. A local fixture
reported one document load and the same UUID while the sidebar changed width and
input continued afterward. Screenshots confirmed widths 164, 380 and 500, reset
to 228, and reset followed by collapse/reopen. Held mouse-down followed by Cmd+S
restored usable page input without window reactivation. Shutdown reached CEF
Complete with no remaining owners, browsers or cookie flushes.

The floating panel appeared, but its attempted resize used a root-window drag
helper and an incorrect inner-edge coordinate. That probe does not establish a
product defect or a native pass. Route drag start to the actual child window,
keep subsequent events bound to that window, and repeat the floating check.
