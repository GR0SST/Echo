# Profiles, Settings, page chrome and toast closeout

Reference: installed Arc 1.170.0, 2026-10-25. Scope is the 30 rows marked
partial in sections 12, 13, 16 and 18 at the start of this pass. Other agents'
sections 2, 4, 5, 8 and 14 are not changed. Evidence is under
`target/parity/partial-closeout/`.

Every Arc session claims one new browser window, verifies the shared Echo Test
header and checks the recorded native window before input. Settings has its own
recorded identity and is closed with its native close button in the same locked
script. A failed close shortcut was followed by guarded cleanup of that exact
Settings window; it was not left for another agent. Baseline owner windows are
preserved. Only synthetic test tabs are opened and archived. No existing profile identity, assignment,
credential or route is changed. Zoom and Developer Mode are exercised only on
unique disposable localhost hosts and restored before session cleanup; no global
preference is changed.

## Settings navigation and Auto-Archive

The read-only `arc-settings-navigation-v2.log` records all seven Settings tabs.
`arc-profile-navigation.log` activates General → Profile Settings…, verifies the
same native window becomes Profiles, opens its archive interval menu and cancels.
It never changes the selected interval. Arc offers 12 hours (default), 24 hours,
7 days and 30 days under Profiles; the old General/Command Bar gap was not an
observed requirement of this installed release.

The action row uses the observed regular 13 px system face rather than inheriting
a heavier Settings label weight. `pair-profile-settings-action.png` compares its
556 × 48 px row and `pair-profile-navigation.png` compares the destination's
search group. Account and automatic-update controls are excluded and therefore
shift the General body vertically. Both native apps activate Profiles in the
same existing Settings window. `echo-settings-controls.log` records the click,
684 × 650 native frame and clean CEF Complete/exit 0. Moving both reference
windows onto the same display avoids cross-display screenshot resampling.
The existing GPUI Settings routing/profile/interval tests cover this flow. The
archive keyboard regression additionally checks that mouse opening/reopening has
no keyboard highlight and that arrow navigation renders one before commit.

`pair-archive-popup.png` records a remaining visual difference, rather than a
matching pair: Arc has a rounded native vibrant menu; Echo's menu still has a
flat fill/border. Mouse opening now shows the check without a blue keyboard
highlight; keyboard arrows select/commit as before. This row remains partial
for material/corner/text rasterization fidelity, with the four intervals and
per-profile policy already working.


## Page controls

`arc-page-qualified-v2.log` records a disposable localhost fixture. Transient
windows are registered from a before/after native inventory, bound to their
observed trigger and checked before subsequent input. Zoom/reset feedback and
the passive status HUD are not mistaken for the owned browser. Both attempted
page sessions were cleaned up in their own locked scripts.

`arc-localhost-policy.log` verifies the automatic developer toolbar for a fresh
`*.localhost` origin, an explicit Ctrl+D disable retained across reload, and the
restored toolbar before closing the synthetic tab. Echo now stores the negative
choice per profile as well as explicit positive choices. A GPUI regression
covers host/port revisits, toolbar/page layout, persistence and restoring the
choice; a core regression covers isolation from another profile and false
localhost suffix matches. Other loopback address policies are not inferred.

The source status capsule is 24 px high, has 12 px corners and sits 10 px from
the page's left/bottom edges. Its sampled Dark fill/text are `#0a0d4b` /
`#b6b7c9`, with 12 px medium text. Echo's pale rectangular Dark pill is
corrected to those observed values. Geometry assertions extend the existing
GPUI dwell/replacement/pane-owner regression. Light colors, shadows, transition
curves and remaining elision cases retain the row's partial status.

The fresh zoom reference exposes a static `110%` AX label, 60 × 42 geometry,
and the previously measured five-second feedback. Click semantics and exit
motion are still unqualified. The Find reference says `NOT FOUND`, and its
panel starts 10 px below the developer toolbar. Echo's Find HUD now offsets its
native child by that toolbar's 40 px height; a GPUI check covers the child
bounds. Arc's Edit → Find → Find and Replace is disabled on this ordinary
read-only document (`arc-find-replace-enabled.txt`); editable-context support
is not established by the existence of that menu string.

Page-card corner crops retain the 10 px inset / 6 px radius measurement; exact
empty-Space opacity, shadow and border calibration remain open. The previously
recorded loading pulse is retained as evidence; no new finishing curve or split
acceptance is claimed. Site Control Center still lacks Full Page/Portrait,
extension management, usage/media counts, main-panel keyboard traversal and
embedded cross-origin qualification. Its existing native profile/permission
checks do not establish those missing behaviors. None of the seven page rows
is promoted solely because its underlying action works.
