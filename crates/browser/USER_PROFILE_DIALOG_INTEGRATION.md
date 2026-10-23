# Profile sheet integration contract

This contribution adds only `src/user_profile_dialog.rs`, its test module, and
this guide. Existing browser files remain owned by the routing integrator.
No Cargo commands, builds, native apps, native input, commits or pushes were run.
The GPUI tests are supplied for parent execution after wiring the module.

## Stable API

Add `mod user_profile_dialog;` to `src/lib.rs` and call
`user_profile_dialog::bind_keys(cx)` from initialization. Tests are included by
the module itself; do not declare the test file a second time.

Construct inside `cx.new` using:

- `UserProfileDialog::creation(&browser, CreationSource::File(space), cx)`
- `UserProfileDialog::creation(&browser, CreationSource::Settings, cx)`
- `UserProfileDialog::switching(&browser, space, cx)`

All return `Result<Self, DialogError>`; `&WindowProjection` coerces to `&Browser`.
Private and Blank models are rejected. File creation preselects exactly its
captured originating Space; Settings preselects none. Name starts empty with
placeholder Personal. Back retains the same TextInput entity and its contents.
Spaces are a horizontal strip in workspace order, never a searchable picker
or a wrapping grid. Cards are 64 × 95 px with 12 px gaps: five fit inside the
450 px sheet's 38 px side padding. The sheet is dark navy (0x151c2e), with white
text, a saturated blue-purple primary button and muted purple secondary buttons.
These colors follow the supplied reference direction; exact calibration remains
unverified. Escape on the Spaces step invokes Back, retains the name and focuses
its input; Escape on the name or switching step cancels. The Spaces footer groups Back and Create Profile at the right, starting at
x = 165 px and x = 270 px respectively. Cancel/Back show a
separate ESC keycap; primary buttons show Echo's Icon::Enter vector in a keycap.
The creation heading is “Create a new Profile” at 18 px bold; its field label
is “What do you want to call this Profile?”. Switching names the captured Space
in “Change <space>’s Profile”, followed directly by the warning and dropdown.
The first switching sheet follows the parent's fresh Arc 1.170 measurement:
450 × 338 px for a short heading, art at (38,38), heading at (38,126), and a 374 × 54 px body
at (38,160). The exact body reads “Each Profile has different browser data, so
you might get logged out of some websites, see different Favorites up top, or
need to re-install extensions.” At y = 228, a bold “<space>’s Profile” label
sits beside a compact purple selected-profile pill and vector down chevron.
The heading wraps without ellipsis. GPUI shapes the actual bold 18 px text at
374 px width to count wrapped lines; every additional line adds the observed
19 px to the sheet, heading box and body/row/footer offsets. A 19 px line height
with 1.5 px vertical padding preserves the original 22 px first-line box and
baseline. The native “Echo Space Reference 20261023” heading produces two lines
and a 450 × 357 px sheet; the footer retains its 28 px bottom gutter. Positions
after the extra line are derived consistently, not all independently verified
against native baselines. The bold row label uses 12 px text so the observed
full Space name fits beside Grosst. Arbitrarily long row/profile labels truncate
within the remaining 374 px row and a maximum 160 px pill to avoid overlap;
these extreme-name limits are implementation bounds, not verified native rules.
The pill has an explicit natural width measured with GPUI's selected-name text
at 13 px regular weight, plus 44 px for padding, gap and the vector chevron,
capped at 160 px. A short name therefore leaves its unused width to the full
Space label instead of stretching the flex column to the cap. The observed
Grosst native acceptance requires a sub-cap pill and the complete 12 px Space label.
GPUI 0.2.2 TestPlatform installs NoopTextSystem (`platform/test/platform.rs`),
whose `platform.rs` layout_line ignores font runs and assigns each ordinary
character an advance of 0.6 × font size. For the 39-character reference row
label this gives 280.8 px; Grosst's pill becomes 91 px, exceeding the 374 px row
by 5.8 px with its gap. The GPUI regression therefore logs actual font/width
metrics and requires full text whenever measured content fits, otherwise
bounded truncation using the full remaining row width. Production font size
and padding are unchanged; full native variable-width label fit remains a
parent screenshot acceptance gate, not a claim from the fake-font test.
The short-sheet footer starts at y = 274 with 36 px-high Cancel (110 px) and Change
Profile (148 px) buttons, separated by 8 px, with 38 px right and 28 px bottom
gutters. The menu anchors to the pill using GPUI's window-fitting anchored
element and does not expand the sheet. The switching footer uses only its
absolute inset, with no auto top margin; auto-margin placement is reserved for
the flow-based confirmation footer. The geometry regression checks the footer
container and its buttons before and after opening the menu. Menu dimensions
beyond the observed
compact trigger remain qualified; existing bounded scrolling is retained.
The deferred menu surface blocks mouse hit-testing of controls underneath it
and consumes mouse-down propagation. Each option consumes its click before
changing selection or emitting New Profile. This prevents overlapping footer
Cancel/Change Profile buttons from arming or firing through the menu. The
regression clicks the measured intersection of an option and a footer button,
then requires the chooser to remain open with no event or model edit; a separate
subsequent Change Profile click opens confirmation without applying anything.
Clicking Change Profile advances locally to a second confirmation sheet without
emitting an event or editing the model. The heading is “Switch to your
<destination> Profile?”; the body preserves the captured Space name and the
observed warning about logouts, Favorites and extension reinstallation. A
separate paragraph says “This action cannot be undone.” Only Cancel and
“Switch it up” appear in its footer; Escape cancels and no Back is shown.
Only the final “Switch it up” action emits the existing Submit/Switch request.
No same-profile shortcut or bypass has been added; native same-profile behavior
remains unverified. Public event and request types are unchanged.
Body type is 14 px with 18 px leading and muted 0xb8b8c0 text. The creation
sheets use definite 450 × 426 px and 450 × 506 px dimensions, measured by the
parent from Arc 1.170's native AX sheets. Creation content cannot flex-shrink.
The illustration starts at (38, 38); the heading starts at y = 120 and the body
at y = 154. The exact observed creation descriptions replace the abbreviated
copy: four lines on Name, five on Spaces. Name's label starts at y = 244, its
374 × 48 px input at y = 268. The owner explicitly requested omitting the
synchronization notice and its icon. The idle input border is subtle gray. Name's footer starts at
(206, 362), with 110 px Cancel and 88 px Next buttons separated by 8 px.
Spaces' 64 × 95 px tiles start at y = 254; selection circles are below the
tiles (centers approximately y = 372), followed by labels. Its footer starts
at y = 442. Both footers are 36 px tall with a 28 px bottom gutter. The root
is still the existing full-window Overlay host; native child sheet attachment
and route decisions remain the integrator's responsibility.
A 64 px top illustration uses Echo's own Sidebar vector in a rounded tile;
no Arc assets are included. Parent screenshot comparison remains required.
There is no Alt+Left shortcut. Theme cards use existing Space hue; selection and dropdown
controls use Echo's existing vector geometry. Names and warning copy reflect
observed concepts; exact fonts/colors/copy remain subject to native comparison.

Keyboard access uses the same controls and keeps their geometry. Tab/Shift+Tab
cycles through the current form's controls, skipping a disabled primary button.
Space cards have individual focus targets: Left/Right moves across cards, Space
toggles the focused card, and Enter retains the default submit action. Focused
cards scroll into view. The profile dropdown opens with Space/Enter or Up/Down;
Up/Down moves the active option, Home/End reaches the ends (including New
Profile), and Enter/Space commits the option. Escape dismisses an open dropdown
before cancelling the sheet. Tab moves out of the list without committing an
unconfirmed option. Buttons support Enter/Space. Focus colors and active-option
highlighting use the existing surfaces without adding visible controls.
These are keyboard accessibility behaviors, not newly verified Arc shortcuts.
The dropdown keeps short lists at natural height and caps long lists at 240 px
or the remaining window-height budget after the closed sheet and window gutter.
Its GPUI scroll handle reveals the active option on opening and every arrow,
Home or End movement, including New Profile. The initial switching sheet keeps
its measured geometry; the anchored menu snaps within window margins. A
many-profile keyboard regression checks the active row against the actual
scroll viewport before choosing an offscreen destination and New Profile.
All menu options use 32 px single-line rows. Name text uses layout ellipsis
within a shrinking label, leaving room for the selection check; the complete
name stays in the text element and model. GPUI 0.2.2 exposes no separate
accessible-name/ARIA label API in its Div implementation, so this change does
not claim a native accessibility label implementation. No tooltip is added.
A long-name regression opens the menu, verifies row/label bounds and full-row
keyboard visibility through End/Home/arrows, selects the original identity,
and verifies the unabridged name remains intact.

The entity implements Render and Focusable. Call `focus(window, cx)` in the
child after attachment and activation. Call `close(cx)` to invalidate it before
removing its host. `submission_failed(message, cx)` re-enables a rejected submit
and displays the adapter's message. Do not pass an unverified native warning.

Subscribe to `DialogEvent`:

- `Submit(ProfileRequest)`: apply through the owner BrowserWindow, then close on
  success. A pending guard prevents double-submit. Do not mutate the form
  reentrantly from the subscription; use `cx.defer` when required.
- `NewProfile(source)`: the old form has invalidated itself; replace with a
  creation form using the supplied source. This event replaces Closed; there
  is no intermediate page-focus restoration.
- `Closed`: take the matching owner slot and close its host once.

`BrowserWindow::apply_user_profile_dialog(&request)` returns
`Result<ProfileApplied, DialogError>`, with `profile` and `transitions` fields.
It invokes one `edit_browser`, performs creation/assignments on a cloned model,
and publishes only after every operation succeeds. Validation errors cannot
leave an orphan profile or partly reassigned Spaces. It revalidates source
existence and normal-workspace status at submission; switching also verifies
the captured original owner to avoid applying a stale sheet. Duplicate assigned
Space IDs are deduplicated. Names of Spaces and existing profiles are untouched.
New profile Favorites remain empty according to the core contract.

## Exact narrow hooks in existing files

1. `BrowserWindow`: add `user_profile_dialog: Option<ProfileDialogHost>` initialized
   to None. The host holds `Entity<UserProfileDialog>`, its event subscription,
   and `Option<WindowHandle<Overlay>>`. Keep the entity identity or a generation
   token in every deferred open/close callback. Refresh the normal projection
   before taking the construction snapshot.
2. `OverlayKind`: add `UserProfileDialog`; its render branch reads the owner's
   current host and returns `dialog.clone().into_any_element()` (or an empty
   div if absent). Observe the owner as existing Overlay does. Normal child
   entity notifications handle form changes. Use the existing overlay shell.
3. Open route: reject private/Blank before any focus change. Finish inline
   rename, close competing Command Bar/find/menus, and request UI focus through
   Pages. Defer opening until the current update ends. Size the transparent
   child against `main_window.bounds()`, using the Command Bar WindowOptions
   and `native::attach_overlay(parent, child, false)`. After attaching, activate
   the child and focus the form. Confirm the same host identity before storing
   the returned handle or focusing. If obsolete, remove only that new child.
4. Close route: take the matching host first, invalidate its form, and use
   `close_popup_overlay(host.overlay, policy, true, cx)`. Include the dialog in
   `popup_owns_focus`, popup pending/restoration guards, window-operation close,
   deactivation, owner release/closure and any generic popup cleanup. Preserve
   `PopupFocus::OnActivation` on minimize. Deferred callbacks must verify owner
   liveness and identity; an old Closed event must not take a newer host.
5. Submit route: identity-check the sender, call `apply_user_profile_dialog`,
   then route all returned transitions through the routing agent's generation
   invalidation/CEF retirement/recreation path before rendering new ownership.
   `edit_browser` itself publishes a projection, so finish transition routing
   synchronously in the same owner update, before notification is rendered or
   the loop pumps. Include other windows showing those Spaces. Never pump CEF
   while a BrowserWindow, Pages, or model borrow is held. Focus restoration
   occurs only after retiring the sheet; no stale old-profile page focus.
6. Actions/menus/Command Bar: File → New Profile passes the originating normal
   Space, captured at invocation. Spaces → Change Profile and the Space context
   action pass that Space's stable ID, rather than resolving active Space later.
   Use normal browser action routing in overlay shells and engine shortcuts.
   Settings Add will pass Settings when that window exists; do not add a
   substitute settings window or expose a searchable Command Bar form.
7. New Profile from dropdown: replace the captured host atomically, avoiding a
   restore-focus gap. Close its native child without focusing the page, open the
   new form using the original Space source, and retain UI focus intent.

The dialog deliberately owns no session persistence, engine storage lookup,
window creation, native event pumping or loaded-page behavior. Those belong to
routing. Reconcile stale snapshot inventory by closing/reopening on workspace
Space/profile deletion or rename; never silently retarget selections by index.

## Validation and evidence boundary

The supplied GPUI tests drive actual TextInput focus/input and hit-tested
buttons, Back, cards, cancellation, dropdown selection, disabled actions,
the two-stage switch with no first-stage request/mutation, final-submit
deduplication, retained destination/Space labels and confirmation Cancel/Escape,
keyboard-only Settings creation with card selection, keyboard-only destination
selection and two-stage switching, keyboard New Profile, Tab/Shift+Tab focus,
dropdown Escape dismissal and arrow navigation,
Spaces-step Escape followed by focused-name Escape cancellation,
submission deduplication, model creation/assignments, empty new Favorites,
unchanged existing Favorites/name, and rollback on an invalid selected Space.
Run the workspace suite after wiring, plus native transparent-child focus and
late owner-close checks. Verify keyboard traversal and native visual geometry.
Rendered-geometry regressions also check the illustration/heading/body gaps,
field and note spacing, card/check/label order, and footer alignment and bottom
gutters, including after typing and returning with Back. Initial test helpers
wait for GPUI readiness before inspecting bounds. These checks have not been
run by this isolated implementation task. Parent must run Cargo and compare
new bundle screenshots against the native reference before claiming parity.

Reference: `docs/ARC_REFERENCE.md`, newest Profile settings/creation follow-up,
and `crates/core/PROFILE_MIGRATION.md`. Native creation successfully assigned
only the parent's test Space and left its name unchanged with empty Favorites.
The parent's latest permitted native switch confirmed that the existing Today
tab kept its URL, reloaded with a fresh document UUID, and used destination
storage: the source's owned local/cookie marker was absent in Default. This
supports the host's generation-retirement/recreation route; the form does not
implement engine handling. The temporary Space was switched back to Default,
leaving its test profile unassigned, and the test window was closed. Deletion,
same-profile confirmation behavior and settings propagation remain unverified.
This sheet implementation does not establish complete Arc parity.

The latest direct native reference from the parent's own empty New Window
showed Change Profile applying immediately without the second confirmation.
The parent restored Grosst to Default with its Favorites and closed only that
window; no Space was created or deleted. This observation has not been turned
into a conditional rule: confirmation behavior is unchanged pending further
evidence. The separate native Space-key mismatch also remains unproven; no
normalization or keybinding workaround is included in this visual correction.
