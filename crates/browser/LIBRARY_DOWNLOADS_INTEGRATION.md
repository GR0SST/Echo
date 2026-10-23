# Library Downloads controller integration

The controller and presenter focus API are registered in the checkout.
All eleven controller GPUI tests pass; the complete workspace passes 905 tests
(638 browser, 165 core, 102 engine). The only parent compilation correction was
importing GPUI's Focusable trait in the new test module. Independent round-two
review found no material issues. Production BrowserWindow route wiring, observed
formatting and native controller acceptance remain pending. No admission,
destination policy or second engine-event consumer is enabled by registration.

`library_downloads_host.rs` owns canonical completed-record projection and
window-local query/filter state. `DownloadsControlFocused` reports exact control
intent independently of existing DownloadsIntent subscribers. The module is
registered in lib.rs and downloads_view::bind_keys is called exactly once there.

The following remaining host contract was reviewed against the retained preview
implementation. Statements requiring parent wiring describe work still pending.

## Owner and route lifecycle (parent must implement)

1. Store one Option<LibraryDownloadsHost> per BrowserWindow plus two retained
   Subscription slots, a route-active flag, inventory-dirty flag and an optional
   `(DownloadsOwner, FocusHandle)` popup responder. Keep the same host/presenter
   across section transitions. Never recreate it just because query, filter,
   active Space, inventory, palette or section changes.
2. Construct only from `self.profile.handle.clone()` after checking the existing
   normal Library scope. The constructor rejects private and Blank handles via
   download_inventory; do not resolve Blank's history_source to bypass that gate.
   Inject actual DownloadsColors and CompletedPresentation. The latter accepts
   `(Vec<core::downloads::CompletedDownload>, explicit_profile_filter: bool)` and
   returns DownloadsList<CompletedDownloadId>. Parent owns observed chronology,
   heading/date/time handling, file type/source subtitle and artwork. This module
   assigns no dates, MIME labels, file extensions, alpha values or active rows.
   For empty explicit Profile results return FilteredEmpty. For empty database
   without explicit filters, the parent must supply an evidence-backed result;
   there is deliberately no fabricated default empty-state copy here.
3. Construct DownloadsPreview using the real r5 callback below. It is never
   substituted with a download-row preview. The controller's unique lifetime
   gate blanks it while inactive or after drop, then calls the supplied callback
   unchanged when active. The existing r5 Pages viewport generation still guards
   native canvas work. No layout arithmetic or reparenting belongs here.
4. The active predicate is existing normal Library eligibility AND
   SidebarPage::Library AND LibrarySection::Downloads. On each predicate
   transition first clear the two subscription slots, call set_active(active),
   clear the cached Downloads responder, and bind fresh subscriptions only if
   active. Capture `let owner = host.owner()` and `let view = host.view()` at
   binding time. Never compute a new owner ticket inside a deferred callback.
   Before handling any callback recheck the actual normal Library/Downloads
   predicate and current canonical ProfileHandle. Exact Entity equality,
   generation and same_workspace are checked again by the host.
5. Use Context<BrowserWindow>::subscribe_in(&view, window, ...) for BOTH
   DownloadsIntent<CompletedDownloadId> and DownloadsControlFocused. Pass the
   subscription's actual sender Entity and captured owner into handle_intent or
   control_focus. On handle_intent == false, distinguish the intentional rejected
   action (row, unknown ID, unobserved second live chip, stale owner) from a
   projection failure: if refresh still fails for the current owner, unmount and
   retire rather than displaying stale records. A mismatched canonical workspace
   requires a new controller and subscriptions; never replace its handle in place.
6. In the existing receive_profile_projection path, mark Downloads inventory
   dirty. The existing ProfileBinding observer already refreshes the window and
   notifies after canonical writes; do not expose ProfileHandle.owner or add a
   polling loop. At the next render with the real Window, consume that dirty flag
   ONCE and call host.refresh(current_handle, current_colors, window, cx), even
   when Downloads is hidden. Also refresh before section mount and on palette
   changes. Do not refresh unconditionally every frame: set_projection notifies.
   Read canonical inventory, never the BrowserWindow WindowProjection snapshot.
   If a notification arrives after consuming dirty, it sets it again normally.
7. Mount host.view() in render_library's existing other_section slot, after the
   existing rail. Parent owns the normal Navigate Downloads branch and existing
   invalidate_sidebar_focus -> section assignment -> sync_navigation_visibility
   -> defer_sidebar_focus sequence. Set LibraryActions.downloads only after this
   full host path is in place and tested. No existing route is enabled by this
   patch. Do not introduce MenuOpenLibrary, View-menu entries or other routes;
   retain the current `Open Library…` label and verified placement policy.

Actual r5 retained preview callback (from the inspected INTEGRATION.md):

```rust
let owner = cx.entity().downgrade();
let preview: crate::downloads_view::DownloadsPreview = Rc::new(move |_, cx| {
    owner.update(cx, |this, cx| {
        if this.sidebar_navigation.controller.selected() != SidebarPage::Library
            || this.sidebar_navigation.library_section != LibrarySection::Downloads
        {
            return div().into_any_element();
        }
        this.render_library_page_preview(cx)
    }).unwrap_or_else(|_| div().into_any_element())
});
```

The parent callback must use its own BrowserWindow weak owner, not a captured
Pages clone. Keep it inside downloads-preview. Do not set a CEF rectangle from
112+304 or any provisional list width. With no selected page the real renderer
returns its blank surface. Spaces must keep the existing full-width/hidden native
viewport path. Private/Blank receive no new route. Do not touch page creation,
production download admission, destination policy, shutdown or consumer gates.

## Focus integration

The presentation delta adds a separate DownloadsControlFocused event so existing
DownloadsIntent subscriber contracts/test logs remain unchanged. A new focus
scope is attached ONLY to downloads-column. Its focus-in observer, control
keyboard moves and Filter/Done/Profile activations emit responder intent. Search
pointer hits and column pointer hits also emit, since CEF can own the actual
responder while GPUI retains its old focus handle. Duplicate focus signals are
allowed and must be idempotent. No callback is attached to the right preview.

P2 revision: the column's bubbling mouse-down handler resolves its focus scope
before emitting the event. If a child control is currently available, it preserves
that exact handle. If GPUI focused the column itself on a disabled row/list
background hit (or retained a preview handle), it focuses Search in normal mode
or Done in filter mode, then prevents enclosing default focus. The scope handle
is deliberately NOT an available popup responder. This makes control_focus return
an exact restorable control for background hits too, so the parent can perform
its normal Pages::focus_ui and stale-preview-intent cleanup. No query, caret,
filter visibility or selected profile is rewritten by this handler.

On DownloadsControlFocused, call host.control_focus(captured_owner, actual_sender,
current_handle, window, cx). If Some(handle), invalidate the parent's old popup
and navigation-focus work as for Media, clear preview_ui_focus,
preview_native_focus and preview_native_intent, then Pages::focus_ui. If an
overlay already owns its native responder, preserve the existing
request_ui_focus path instead of stealing it. Cache THIS exact control handle
with the captured owner; do not replace a focused chip/Filter with the presenter's
Focusable default (which chooses search or Done). Revalidate route, owner and
focused_control on capture. The parent must not call focus_ui on all root mouse
events: History/Split inside the retained preview own their own responders.

Before opening an overlay retain the existing
remember_library_popup_focus(window, cx) helper from r5. At popup dismissal or
activation restore a cached Downloads control handle only if owner ticket,
current host Entity and normal Library Downloads route still match, the handle
still identifies an available control, and the popup continuation is current.
Use host.restore_control(captured_owner, actual_presenter, current_handle,
&cached_focus, window, cx) after those parent continuation checks. It validates
availability independently of the overlay's currently focused handle. A removed focused chip already transfers to Done in set_projection; recapture
that current target instead of restoring the removed handle. Existing real native
Focused handling must clear cached Downloads control intent and invalidate stale
restores, just as it does for Media. A GPUI History preview responder stays in
the r5 preview cache; never relabel it as Downloads control focus.

## Selection and search boundaries

- Query is retained verbatim and presenter-owned input/caret is never rewritten
  on inventory refresh. Filter/Done and reopening do not clear it or visibility.
- The existing project_completed_search chooses SearchEmpty from the exact raw
  query. It performs ASCII case-insensitive filename substring matching only;
  no trim, source/type matching, word splitting or invented Unicode normalization.
- Only a real canonical UserProfileId can be toggled. One live chip can be toggled
  on/off. Clicking a different live chip while one is selected is intentionally
  rejected: native combination/exclusive replacement behavior is UNACCEPTED.
  This patch does not claim multi-selection parity or add disabled styling.
  Parent must resolve this native gate before claiming full filter acceptance.
- A removed selected ID remains a tombstone and matches no records. No Default,
  active Space profile or All fallback is assigned. An explicit click on an actual
  inventory chip can replace that tombstone. Rename/add/remove comes from a fresh
  canonical inventory. No profile or Space assignment is written.
- Stable CompletedDownloadId is passed end-to-end. The existing adapter rejects
  fabricated tokens, wrong profile identity and duplicate rows and replaces titles
  with canonical filenames. Parent formatter must include all accepted records in
  observed newest-first/date grouping order; the adapter is not a sorting policy.
- Rows remain disabled; even manually supplied RowPressed is rejected. No open,
  reveal, remove, row selection, context menus or progress controls are introduced.

## GPUI tests and remaining acceptance gates

Eleven real GPUI host tests pass. The original ten test bodies are preserved.
They use actual TextInput,
DownloadsView, BrowserWindow and canonical ProfileHandle metadata: query/filter
intents and retained Entity/section reopening, exact raw SearchEmpty, shared
canonical writer notification/projection, wrong Entity/generation/workspace,
rename/add/remove and tombstone behavior, exact control focus and callback
passthrough, private/Blank exclusion, actual r5 weak-owner renderer with empty page
and Spaces guard, disabled rows, and a queued old event crossing section entry.
The harness subscribes to the real model-owning BrowserWindow notification because
this proposal cannot modify the production parent. Its generic preview fixture
is explicitly separate from the additional test invoking the ACTUAL r5 renderer.
The added P2 regression uses real Pages with a recording WebEngine and real GPUI
pointer dispatch. Its preview fixture establishes a native ResizePageFocus intent
without emitting Downloads control focus. Disabled row and empty list background
clicks in both Search and filter modes must reach the accepted-focus subscriber,
clear the harness's retained native preview snapshot and call Pages::focus_ui.
It covers both a retained exact Search/Profile control and a retained GPUI preview
handle (resolved to Search/Done), checks the exact focused/restorable control,
unchanged query and
selected profile, and unchanged rows/filter visibility. The harness handoff is
explicitly parent-shaped test wiring, not production BrowserWindow integration;
it does not access or prove cleanup of BrowserWindow's private preview caches.
No synthetic engine Complete is used or claimed as native acceptance. Test
DownloadCompletion values are canonical model fixtures only, with Unknown time.

Compilation and all eleven tests are verified. Remaining acceptance gates:
production window route/inventory/focus wiring; actual same native
page/document retention and measured canvas bounds;
popup/activation interleavings including removed chips; real CEF search/preview
responder switching and Source/History/Split; normal empty page and Spaces hide;
observed date/type/heading/palette formatter; empty database/unknown time;
second-chip combination semantics, Unicode matching, row/file actions, active UI,
production admission and destination policy, engine downloads and shutdown native
acceptance. Keep those gates explicit. Do not change ARC_PARITY status to done.
The integrated workspace run is recorded in
`/tmp/echo-downloads-host-full-tests-r1.log`; prior implementation-agent counts
are superseded by that parent run. No accounts, sync, new menus or Space deletion.

The frozen proposal and replay remain under
`/tmp/echo-library-downloads-host-r1`. Parent tests supersede its original
syntax-only validation. Do not reapply that patch over the registered modules.


## Registration regression check

The rebuilt macOS app retained the Media preview and accepted document input
through Command Bar dismissal after controller/key-binding registration. The
loopback fixture recorded one load, one UUID and `WIDGETOK`; the inspected
screenshot is `/tmp/echo-downloads-host-native-r1.png`. The process exited 0
after CEF Complete with zero owners, no browsers and no pending cookie flushes.
Bundle binary SHA-256:
`bdf86c441c6c13689b962135ace0a55011a32b6332a6ad9c934363a61e973b59`.
This is registration regression coverage. It does not mount or accept the still
unwired production Downloads controller. Its eleven GPUI tests provide the
controller evidence described above.
