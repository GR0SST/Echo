# Space creation controller integration

These new modules are deliberately unregistered while the parent validates the
profile/shutdown foundation. No existing source file was edited for this work.

Once released, add `pub mod space_creation;` to `crates/core/src/lib.rs`, and
`mod sidebar_pages; mod space_creation;` to `crates/browser/src/lib.rs`.
Each module includes its dedicated test file automatically. Parent owns Cargo
validation, real GPUI interaction tests, native verification, and publication.

## Window ownership and entry routes

Store one `SpaceCreationController` per BrowserWindow, initialized from its
window projection. Do not store it in the shared workspace or session. Its draft
has no Space ID and consumes no identities. Bind the form's existing TextInput
to `edit_draft(|draft| draft.name = text)`. The draft holds only currently
supported model values: name, icon, profile ID and hue. It has no account/sync
state. Icon and hue default to no custom icon and the origin's hue; this is a
controller initialization policy, not verified native theme-picker behavior.

Route menu New Space, Command Bar New Space, Library Spaces plus, and the right
edge swipe to `begin(&projection, originating_real_space_id)`. This selects the
virtual CreateSpace sidebar page; do not open a modal or Command Bar. Calling
begin again preserves an existing draft. `select(CreateSpace)` is shorthand
using the projection's current real Space; pass an explicit origin to begin
when the action came from another Space. Parent focuses its sidebar TextInput.
Do not add an invented profile dropdown or a reduced theme picker: those native
controls still require reference inspection.

Use `controller.pages(&projection)` for pager positioning. It always derives
real IDs in the model's current order: Library, real Spaces, and one creation
entry. A live draft sits immediately after its origin; otherwise creation is
the right edge. Never convert a page index directly to a Space index. Route
drop handling through `SidebarPage::drop_destination`; both virtual pages
return None. Parent owns swipe physics and page rendering.

`select(Library)` changes only controller state. `select(Space(id))` also needs
the parent's existing identity-based WindowSelection update. Keep page selection
in sync when another existing path switches the real Space. `cancel` returns
the destination page (the original Space if still present) for that same hook.
Call `reconcile` after shared-model/projection refreshes. Reorder keeps the draft
anchored by identity; a removed origin cancels it. A removed draft profile stays
invalid rather than silently changing the chosen data owner.

## Atomic submission

Obtain an owned `request()` and release all borrows of controller/view state.
The recommended production seam on the authoritative shared workspace is
`browser.create_space_after_in_window(&mut selection, &request)`. This validates
BEFORE `edit_in_window` can apply the originating window's selection, then
revalidates inside the atomic creation operation. Calling only
`edit_in_window(..., |browser| browser.create_space_after(...))` is insufficient:
even an invalid request can change the canonical serialized selection from
window B to window A before its closure returns Err.

If the host must use its own `edit_browser` wrapper, first call
`authoritative_browser.validate_space_creation_request(&request)?` outside that
wrapper, then immediately call `create_space_after` inside it. Keep authoritative
validation and mutation in one synchronous borrow/operation, without an await,
callback, engine pump, or other intervening model mutation. Never preflight only
against a WindowProjection: it can be stale. Retain atomic API revalidation.
Do not clone/restore the entire model as rollback. The dedicated window seam
encapsulates this order and should be preferred where possible.

On success call `controller.committed(id)` and run the normal persistence/shared-
window reconciliation path. On preflight Err do not call any window-edit wrapper,
refresh selection, or persist the session. The atomic API
inserts after the origin's current position, assigns the explicit profile/icon/
hue, selects the new real Space with no selected tab, and leaves all profile
data intact. Do not open the Command Bar or copy Favorites/history. Creation
requires no profile engine transition because the new Space has no pages.

On error keep the draft and display the appropriate inline validation state.
The API validates local-window restrictions, origin/profile existence, trimmed
nonempty name, nonempty optional icon, finite hue in 0..=360, and allocator
capacity before changing the model. Cancellation never calls a model edit or
persists a session. Session-byte assertions include both allocator watermarks.
Core regression coverage includes distinct A/B window selections with canonical
B active: invalid creation via the production seam preserves serialized session
bytes and both WindowSelections. Additional cases cover one allocation at
`next_id = u64::MAX - 1` followed by unchanged exhaustion, live/archived identity
pools across session round trips, and controller rejection of private/Blank
creation without changing its existing page or draft.

Never commit on a WindowProjection clone. A successful shared-model creation
must update the originating window's WindowSelection before applying a stale
projection, otherwise the newly selected Space can appear to be lost.

## GPUI sidebar form

The next unregistered module is `src/space_creation_view.rs`; register it with
`mod space_creation_view;` in the browser root. Its dedicated GPUI tests are
included by the module. Call `space_creation_view::bind_keys(cx)` explicitly
for the existing form-scoped bindings: Return retains the draft, and Escape
first leaves the name input, then cancels from the form root. This row-layout
change preserves the parent's latest keyboard implementation and adds no bindings.

After `controller.begin`, create one `SpaceCreationView::new(draft,
current_profile_name, cx)` entity and call `focus_name(window, cx)` after
mounting it. The form fills its parent's width and height, retaining 14 px left
and 13 px right row gutters at both the 250 px reference and 240 px host. Parent
owns the themed sidebar background, blank main card, visibility, and dock;
render this as the sidebar page, never as a modal overlay or Command Bar.
The measured row positions are relative to the full sidebar top: name y=237,
Profile y=279, Theme y=323. The Create Space button starts at height−132,
and Cancel's center sits at height−72. Supply the full sidebar height rather
than a content area already shortened for the dock. The original Echo stack
illustration is fixed vector geometry; all control icons use the existing
vector icon helpers and the bundled licensed Ionicons contours. Existing palette
values are unchanged; this work does not introduce a dark-mode implementation.

The Profile row itself is filled and rounded at 7 px, matching the Name row.
Its 12 px person icon is centered at reference x=31 and the semibold label starts
at x=50. The selected-profile pill follows the label with a 6 px gap, uses 13 px
text and a fixed vector chevron, and keeps its natural width capped at 160 px
and the actual remaining row space. Long names truncate inside that pill. The
separate 22 px info hit target stays fixed at reference center x=225; it shifts
with the right gutter at narrower widths. The Theme row has the same leading
12 px brush icon and x=50 semibold label alignment. The empty Name icon uses
an 18 px dashed outline square with a catalog vector plus inside. Custom emoji
remain content. Popover intents and their captured draft payloads are unchanged.

Subscribe to `SpaceCreationEvent`:

- `NameChanged(text)`: update `controller.edit_draft` without a model edit.
- `ChooseIcon(draft)`, `ChooseProfile(draft)`, `ProfileInfo(draft)`, and
  `ChooseTheme(draft)`: captured requests for parent-approved popovers. The
  module deliberately contains no picker/menu implementation. Reconcile the
  captured origin/profile against the live model before acting on a response.
- `Submit(draft)`: copy the captured form values into the controller's draft,
  then use the atomic submission flow above. The view blocks empty/whitespace
  submission; core remains the authority for all other validation. Parent
  should guard against repeated submission while a commit is in flight.
- `Cancel`: call `controller.cancel(&projection)` and navigate to its returned
  real page. Drop the view and its subscriptions; do not persist anything.

After an approved picker changes the controller, call
`view.sync_draft(controller.draft().unwrap(), profile_name, cx)`. This refreshes
the selected profile/icon/hue without resetting the input's selection when its
text is unchanged. The view is a form snapshot; the controller remains owned
by the window. Neither entity nor input belongs in the workspace/session.

The new GPUI tests cover user typing and disabled empty submission, captured
picker intents, cancellation routed back through the controller, measured
geometry, bottom anchoring across resizes, approved picker synchronization,
and the existing scoped keyboard actions. Row regression tests additionally
exercise compact-pill sizing, long-name truncation, distinct profile/info click
targets, icon bounds and 240 px host geometry. Only rustfmt parsing/format checks
were run by the implementation sub-agent; parent must run Cargo and native
validation after registration. Do not report these tests as passing yet.

R11's long-profile growth assertion used `Default`, which NoopTextSystem already
clamps at 240 px: its 0.6-em ASCII advances leave 83.2 px for an 86.6 px natural
pill. The test now starts with `A`, verifies it fits naturally, then verifies
the long label grows to the available-width clamp. Existing containment, info
separation, resize and full-intent checks remain. Product layout is unchanged;
native screenshots remain the gate for actual font appearance and truncation.
