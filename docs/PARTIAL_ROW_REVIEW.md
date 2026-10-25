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
preserved. Only synthetic test tabs are opened and archived. No owner profile,
credential, route or preference is changed.

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
