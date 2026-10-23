# Chrome material foundation (integration proposal)

Status: the shared module is registered in the browser crate. No renderer adopts
a material palette yet; compilation, full tests and native verification remain
with the parent integration pass. This does not close an Arc parity row. The Rust module contains
no product palette defaults and makes no claim about Arc's exact opacity.

## Reference and scope

Inspected the supplied Arc screenshot:
`/Users/grosstgrosst/.t3/userdata/attachments/00f9b1e0-9165-4e44-8c94-d8757dffd14a-509054d8-32b3-4990-a901-ade77e146279.png`.
It shows distinct dark rail, list/search area, and lighter blank preview, with
soft background color variation. A screenshot alone cannot establish whether
each region uses native vibrancy, a tinted blur, or an opaque gradient. Do not
derive exact alpha from it; inspect Arc over controlled backdrops when tuning.

This preparation changes no geometry, interaction, controls, copy, native
window hierarchy, profile ownership, persistence, or website appearance.
No account or synchronization UI is introduced.

## Concrete API

`material.rs` adds `MaterialTheme`, six semantic `Surface` roles, authored
`SurfaceStyle`, resolved `SurfaceMaterial`, explicit light/dark `Appearance`,
and actual `Backdrop` state. A host constructs the palette once; sidebar,
Library and menus receive values. A renderer never reads profile storage.

Roles are Sidebar, LibraryRail, LibraryList, LibraryPreview, Menu and Hud.
Separate Library roles preserve the reference's regional hierarchy without
forcing a layout rewrite. Space cards retain their individual Space hues;
they are not a new global material role.

`SurfaceStyle::resolve` preserves every authored color and alpha verbatim when
blur is available. It performs no contrast calculation, palette rejection,
foreground substitution, or alpha adjustment. Actual unsupported blur or
Reduce Transparency changes only the fill alpha to 1; all authored color
channels, text, borders and state overlays stay unchanged. Text/icon colors
remain independently authored; never fade their entire parent container.

`SurfaceMaterial::validate_text_contrast` is an explicit opt-in authoring helper.
It reports normal/secondary text below 4.5:1 in idle, hover or selected states;
it never modifies values and must not gate rendering an authored reference
material. A failure is diagnostic, not permission to redesign Arc visuals or
make them less translucent. Translucent authored text is preserved by resolve;
the helper reports that it needs separate rendered-pixel validation.
Border and decorative icon contrast are not certified by this text calculation.
Hover and selected are checked as mutually exclusive single overlays.

The helper bounds all sRGB backdrop colors by source-over composition over
black and white. It handles foreground luminance inside that interval, where
checking only the two endpoint ratios would give a false pass. This conservative
authoring diagnostic is not a substitute for native rendered-pixel checks,
OS contrast preferences, focus-indicator checks, or font/antialiasing review.
No automatic contrast policy is installed. The host chooses authored reference
materials; neither the renderer nor helper invents a palette or preference.

## Theme ownership deliberately remains unresolved

The user wants appearance to depend on the profile theme. Accept an already
resolved render-time `MaterialTheme` at the window boundary now. Do not create
`UserProfile.appearance`, persist a theme, copy one between profiles, or use
GPUI `Profile` (the session owner) as though it were `UserProfile`.

Current local facts: `core/src/profiles.rs::UserProfile` has no appearance field;
`core/src/lib.rs::Space` stores an accent hue in degrees and a user profile ID;
`browser/src/profile.rs::Profile` owns the shared session, not one user profile.
`BrowserWindow::accent` / `pager_hue` currently produce normalized hue. Maintain
that degrees/normalized distinction at the eventual theme resolver.

The existing Arc appearance documentation distinguishes application appearance
from Space colors; it does not establish an Echo profile-specific preference
or its precedence. Resolve that product behavior from the user's intent and
Arc inspection before binding profile, automatic OS appearance and Space hue.
Until then preserve current behavior; module registration alone changes no UI.
Do not reuse this appearance value for CEF website dark mode.

## Verified local GPUI 0.2.2 source findings

Source root on this machine:
`/Users/grosstgrosst/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/gpui-0.2.2/src`.
Cargo.lock selects 0.2.2; no GPUI patch was found in the root manifest.

- `platform.rs:1309`: Opaque, Transparent and Blurred are distinct options;
  Blurred is explicitly not always supported.
- `window.rs:1711`, `window.rs:1789`: `appearance()` reads the OS/window mode;
  `set_background_appearance()` forwards a whole-window request and returns no
  success/capability result. `observe_window_appearance` is available for updates.
- `platform/mac/window.rs:1257`: transparency changes renderer/window backing;
  Blurred creates an effect view on modern macOS (a legacy blur path exists).
  `:2500` uses Selection material, active state. No new whole-window AppKit
  shim is needed to try GPUI blur; its appearance is not proof of Arc parity.
- `platform/windows/window.rs:758`: Blurred requests acrylic, Transparent
  requests a transparent gradient. These calls are not confirmation of actual
  compositor support on the running machine.
- `platform/linux/wayland/window.rs:949`, `:1133`: blur is conditional on a
  compositor-provided KDE blur manager. Generic Wayland is not guaranteed blur.
- `platform/linux/x11/window.rs:1380`: changes renderer transparency only;
  this path does not request blur. Keep the opaque fallback initially.
- No Reduce Transparency query was found in this GPUI release. The existing
  Echo native layer already isolates the analogous Reduce Motion query.

`Backdrop::Blurred` means the adapter has established/validated a blurred
backing for that surface, not simply that Blurred was requested. Start with
Opaque on unvalidated backends. The API intentionally has no cfg-based
`HAS_BACKDROP` replacement pretending to be runtime confirmation.

## Precise integration points after resize/Media work stabilizes

1. Foundation registration is complete: `crates/browser/src/material.rs` is
   registered by `mod material;` in `lib.rs`. Run its tests in the parent
   integration pass before adopting any palette. Core remains unchanged.
2. Add one window-owned resolved theme seam near
   `window.rs::BrowserWindow::render`. Existing root and
   `render_floating_sidebar` paint opaque `tint(hue, 0.82/0.72)` gradients.
   Replace their fills only when corresponding native blur is enabled and
   the light/dark foreground migration is complete. Leaving an opaque root
   beneath translucent children prevents the desktop from showing through.
   Paint the material once per exposed region; avoid stacking root and child
   tints and accidentally darkening/flattening the intended translucency.
3. Normal/Blank/Incognito windows are opened in `new_window.rs::open`,
   `blank_window.rs::open_for_source` and `lib.rs::open_incognito_window`.
   `WindowOptions.window_background` defaults to Opaque. Use GPUI's Blurred
   request for whole browser windows after validating capability, and
   `set_background_appearance` on actual preference/backdrop changes. Avoid
   invoking native appearance changes on every render or while engine state
   is borrowed. Preserve opaque native CEF page surfaces and their clipping.
4. `sidebar_navigation.rs::render_library_host` currently builds one
   hardcoded light `LibraryColors`. Extend the parent Media agent's final
   color input with semantic rail/list/preview fills, rather than editing
   its temporary layout. `library.rs` should continue receiving colors from
   the host. `card_surface: fn(f32)` is currently noncapturing; pass resolved
   per-card colors or a theme-aware callable when dark card styling is wired.
   Preserve the existing Library captured Space/profile context.
5. `panel.rs::{shell,menu_shell,fill,menu_fill}` currently use dark constants
   and `native::HAS_BACKDROP`. Make shells accept resolved material values.
   `tab_menu.rs::render` calls menu_shell; `new_menu.rs` and
   `site_controls.rs` call shell. Command Bar has a separate fill and backdrop
   in `window.rs::render_command_bar`; migrate it to Hud explicitly.
   Audit nested-menu and disabled-state text alpha separately: material
   resolution cannot fix descendant text overrides or container opacity.
6. Sparse overlays in `window.rs`, `tab_menu.rs`, `new_menu.rs`,
   `site_controls.rs`, `user_profiles.rs`, `find.rs`, `toast.rs` and
   `link_status.rs` must retain transparent empty regions. Do not globally
   set these full-size child windows to Blurred, or the entire web page can
   blur. Keep bounded effects where required. Never apply root `.opacity`.
7. Keep the existing bounded AppKit shim only for panel-sized materials that
   GPUI whole-window blur cannot express. `native.rs::mac::set_backdrop`
   currently forces VibrantDark and sets appearance/material only on creation.
   If integrating light mode, pass `Appearance` through that shared boundary
   and update an existing effect view's appearance/material on changes too.
   Remove effects when hidden and on reduced transparency. Its current bool
   reports true on macOS even when a native view is missing; do not treat that
   as verified capability. No additional native implementation is supplied.

## Parent validation before shipping

- `cargo test` after registration; the module includes authored-color/alpha preservation
  sweeps (including low contrast), unsupported/reduced-transparency fallbacks,
  nonmutating opt-in validation, and mid-luminance regression tests. No Cargo/build commands were run for this
  temporary preparation.
- Add user-style GPUI tests in `tests.rs` for theme propagation to existing
  sidebar, Library and open menu; mode change while submenu is open; unchanged
  focus/shortcuts/click targets. Assert unsupported/reduced-transparency fills
  are opaque. Keep the active theme when opening/dismissing Command Bar.
- With the bundled app and `scripts/drive.sh`, inspect light and dark palettes
  over bright, dark, patterned and saturated backgrounds. Compare Arc in the
  same conditions, active/inactive, idle/hover/selected and menus open.
  Inspect screenshots yourself. Confirm authored text styling is preserved without container fading and
  websites remain opaque/unfaded; inspect blank Library preview independently of native pages.
- Repeat sidebar resize, Library Media sizing and menu anchor/clipping flows;
  resizing must not leave stale or double-painted effects. Test window move,
  fullscreen, app activation, OS appearance and transparency preference changes.
- Validate macOS and Windows actual blur; Wayland with/without blur protocol;
  X11/headless fallback. Do not claim platform visual parity from source alone.
- Leave current theme/parity rows missing/partial until behavior and appearance
  are actually implemented and checked. Commit/push belongs to the parent
  only after its required green tests and app check.
