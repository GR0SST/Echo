# Profiles and Settings follow-up — Arc 1.170.0

Arc is the only reference. Earlier installed-app captures are retained at
`~/Documents/Bot/parity-evidence/parity/settings/`; this worktree's additional
captures and native logs are under `target/parity/profiles-settings/`.

The first read-only session claimed one new Arc window, used the existing
Echo Test Space and verified the shared header crop. Settings initially opened
its remembered Account pane; no account control was used. The new Settings
window was identified by its native ID and toolbar before selecting Profiles
and Shortcuts. Both windows were then closed. No profile, assignment or Space
was changed. A later double-click did not expose the expected rename sheet;
it is not evidence for rename behavior. Subsequent inspection stopped before
Cmd+N because Arc's owner windows had not restored. No replacement Space or
additional Arc window was created after that stop.

## Previews preferences

Source: `arc_previews.png` in the earlier Settings evidence, plus installed
Arc executable strings adjacent to `PreferencesFeature/PreviewSettingsDialog.swift`.
The sheet is 422 × 370 px, centered in the 684 × 650 Settings window, with
17 px corners and 24 px side insets. The seven choices use 16 px checkboxes
and 28 px row pitch. The 92 × 36 px Done button has the captured Enter keycap.
The measured interior sample is RGB (22, 28, 47); the Done fill is
(51, 57, 241). Product names use Echo's brand, with no Arc artwork.

The panel retains each choice immediately, supports Done/Enter/Escape,
blocks clicks reaching underlying controls, and exposes the precise support URL
from Arc's bundled link:
`https://resources.arc.net/hc/en-us/articles/19335284431639-Previews-Glance-Top-Sites`.
This is a preferences panel; site-specific hover previews are still missing.

Removed the legacy automatic-update control from General: Arc service updates
are explicitly out of scope in AGENTS.md, and no excluded placeholder is shown.

The GPUI regression covers measured geometry, immediate choice persistence,
backdrop isolation, dismissal/reopening and the exact help destination. Native
comparison and full test results are recorded below when validation completes.

Bundling exposed a stale Chromium credit-source path. Credits now use the same
resolved CEF distribution directory as the framework bundler, preserving the
installed version's notices rather than assuming a flat `third_party/cef` tree.

Native Previews validation: `preview-native-run.FpqArX/drive.log` exits with
status 0 after CEF completes. `echo-previews.png` / `pair-previews-3x.png` were
inspected at 3× against the original Arc capture. Card geometry, seven-row
pitch and button placement match; checkbox stroke, keycap/border treatment
and shared font rendering remain visibly different. The background omits the
excluded automatic-update row.

`preview-full-tests-2.log`: 1027 browser, 216 core, 13 AI, 136 engine, 2 bundle
and 4 importer tests passed; one AI test ignored. Existing action-button debug
selectors are preserved, including Profiles and Air Traffic Control routes.

## Profile management references

The initial recovery gate counted on-screen windows, which was too strict to
establish whether owner windows were missing. The full native inventory was
checked before resuming one-window guarded sessions. The attempted recovery
found existing windows and skipped the restart, leaving them untouched.

The fifth bottom control opens Rename your Profile, rather than Delete unused
Profiles. `arc-profile-button5.png` captures its 496 × 256 sheet. Its subtitle
is “This won’t affect the names of any of the Spaces associated with this
Profile.” The current name is selected; Cancel/ESC and Save/Enter are present.
No name was edited or saved.

Selecting the existing unused reference profile and pressing the fourth
control opens `arc-profile-delete.png`: a 450 × 350 sheet, 38 px content
inset, 64 px illustration, exact-name confirmation and disabled Delete button.
It was cancelled without typing or deleting. The Default profile's minus
button is disabled and exposes “The Default Profile cannot be deleted.”
Both references retain their owned Settings/main window identities before
input. No owner profile was renamed, deleted or reassigned.

Implementation: Settings now opens measured Rename and Delete sheets. Rename
retains the profile ID, storage key, pages, Space assignments and preferences.
Delete requires an exact case-sensitive name; Default and assigned profiles
are protected. Both sheets invalidate a captured target after rename, deletion
or reassignment, trap keyboard focus, and block underlying controls. Bulk
unused-profile deletion is atomic in the model; no uncaptured bulk UI is added.
Registry deletion does not reclaim CEF directories while other context users
may retain leases. Storage IDs are never reused.

Five GPUI regressions cover text entry, confirmation, keyboard focus, backdrop
isolation, cancellation and stale targets; two core regressions cover atomic
deletion and private-window projection protection. `profile-full-tests-2.log`:
1041 browser, 224 core, 13 AI, 136 engine, 2 bundle and 4 importer tests passed;
one AI test ignored. The canonical native fixture run is
`profile-manager-native-run.qHpVtJ/drive.log` (strict CEF shutdown, exit 0).
It renamed the same profile and deleted only the exact-name unused fixture;
assignments and the next ID remained intact.

Inspected pairs: `pair-profile-rename-3x.png` and
`pair-profile-delete-3x.png`, under `target/parity/profiles-settings/`.
Sheet geometry matches the captured reference. Shared font wrapping, borders
and keycaps still differ; illustrations use original Echo geometry instead of
Arc artwork. Source inspection never saved a profile or assignment change.

## Local application icon variants

Source: the existing native `arc_Icon.png` Settings capture. Its six rows
use 80 px content height plus a 1 px separator, 20 px radios, 52 px artwork
and the selected row's CURRENT ICON caption. The same layout contains Echo's
original, Candy Echo, Neon, Fluted Glass, Schoolbook and Colorful variants.
The alternatives are original concentric Echo geometry, with no Arc artwork,
referral controls, third-party artist attribution or account dependencies.

The install-wide choice persists outside website-profile preferences. Click,
Tab and Space activation update it only after native application succeeds.
macOS applies the SVG artwork immediately; startup restores the choice, and
Original clears the override to recover the bundle's full-resolution icon.
Other-platform native application remains unimplemented behind the shared
interface. The GUI test backend avoids calling AppKit.

The first native pair exposed GPUI 0.2's encoded SVG path leaving premultiplied
RGBA bytes in the BGRA image buffer. Preview rendering now converts those
bytes and removes transparent Dock padding for the measured optical frame.
The native Dock retains vector artwork and its own padding. A regression
checks all six decoders, transparent corners and the yellow variant's channel
order, alongside choice activation and session persistence.

Final native validation: `icon-native-run.11yrMX/drive.log` and `restart.log`
both reach CEF Complete with zero owners/browsers/flushes and exit 0. The fixture
clicked the six rows, saved Colorful without changing profile IDs or Space
assignments, then reopened a fresh process with Colorful still selected.
`pair-icons-3x.png` was inspected after correcting channel order, optical
artwork sizing, the bottom gutter and a row-wide focus fill absent from the
reference. Row/radio/artwork/caption geometry aligns; the shared panel shade,
font rendering and radio border treatment remain visibly different.

`icon-full-tests-final-pair.log`: 1051 browser, 226 core, 15 AI, 140 engine,
2 bundle and 4 importer tests passed; one AI test ignored. Two new GPUI tests
cover activation, persistence, profile identity preservation and preview
colour/alpha correctness. The native fixture explicitly activates its verified
process before clicking, since a first click into an inactive app may only
activate it.

## Creation and Space assignment presentation

The guarded source draft captured `arc-profile-create-name.png`,
`arc-profile-create-spaces.png` and `arc-profile-create-spaces-selected.png`.
Only the Echo Test draft checkbox was changed; Cancel discarded the draft.
No owner profile or assignment was created, renamed, deleted or reassigned.
The existing `target/parity/context-menus/arc-space-profile-confirm.png`
provides the confirmation reference.

The name sheet is 450 × 426; the Space choice is 450 × 506. Its cards are
64 × 96 with 15 px corners, a 16 px check 13 px below the tile, and a label
12 px below the check. Cards sample each actual Space theme, including its
noise, rather than substituting a single hue. Circle illustrations retain
original Echo vector geometry; no Arc artwork or sync notice is included.

Native font measurements and zoomed crops identify the wider system text face
as the previous heading mismatch. The 18 px bold display face preserves the
reference's stroke weight and measured width. Raw UTF-8 executable copy preserves the nonbreaking
space in “Profile — so”; the earlier ASCII string extraction lost it. GPUI
0.2 wraps at that character and counts overflowing trailing ASCII spaces.
Short static paragraphs now measure whole words and preserve NBSP groups,
explicit paragraph breaks and attributed byte offsets. Complex scripts and
overlong words use GPUI's normal fallback. Two regressions cover the observed
wrapping errors.

The 456 × 400 switch confirmation separates its two source paragraphs and
retains the bold Space name. Its footer follows content rather than covering
long profile names; a GPUI regression verifies expansion and cancellation
without changing the assignment. Existing creation/switch tests check the
updated dimensions and source copy.

Shortcut inspection safety: a cancelled Arc recording cleared Add Split View
rather than restoring its prior chord. The captured original ⇧⌘E / Use Both
was restored. `arc-shortcut-stability.log` verifies it in a fresh guarded
Cmd+N/Escape session without editing the recorder. Teardown explicitly moves
focus to Search and leaves Shortcuts before native AX close; Escape must not
be used to close an armed shortcut recorder. No conflict-layout capture is accepted from those initial attempts.

Final native validation: `profile-layout-native-run.PgNhBk/drive.log` completes
creation, assigns only the checked fixture Space, confirms its switch to the
second existing fixture profile and reaches CEF Complete / native Cmd+Q exit 0.
The profile registry, monotonic IDs and every other Space assignment remain
unchanged. Inspected 3× pairs: `pair-profile-{name,spaces,confirm}-final-3x.png`.
Source and fixture Space names/themes differ intentionally; card geometry,
warning wrapping and the confirmation footer align. Remaining differences are
original illustration geometry, shared font baselines, keycaps and material.
`profile-layout-full-tests-9.log`: 1069 browser, 237 core, 15 AI, 148 engine,
2 bundle and 4 importer tests passed; one AI test ignored.

## System appearance and profile selection

Owned Arc windows supplied `arc-{General,Links,Advanced,Shortcuts,Icon}-system-light.png`,
`arc-profiles-system-light.png` and `arc-previews-system-light.png`. System
Dark mode was restored to its original value. The Previews sheet's Done
control has no AX name: use its captured button index, rather than searching
for a button named Done. Cleanup must independently close each owned window and poll through native
close animations; a failed main-window check must never skip closing Settings. No owner profile or Space assignment was changed.

Settings now observes its own window appearance. Light uses white chrome,
near-white groups, measured gray borders/buttons, dark labels and the pale
blue profile selection. It also supplies the white Previews panel and its
20% dimming layer. The fixture confirms identical RGB (255,255,255) blank
window backgrounds and (204,204,204) dimmed backgrounds. Restoring macOS Dark
changes the same open Settings window back to RGB (41,43,45). Space appearance
remains independent. Profile selection is inset by 5 px vertically within
each 40 px row, with 6 px label insets and separately painted dividers.

`light-native-run.A2Htba/drive.log` captures General, Profiles, Icon, Links,
Shortcuts and Previews; its Advanced coordinate initially hit a toolbar gap.
`light-native-run.7UDJWI/drive.log` supplies the final General/Links/Advanced/Previews captures, asserting the selected titles explicitly. Both restore the original system appearance and reach native Cmd+Q / CEF Complete with exit 0. Reviewed pairs are
`pair-settings-{general,profiles,icon,links,shortcuts,advanced,previews}-light-2x.png`.
The source's excluded Account/update UI is absent. Original toolbar/feature
artwork, shared font rendering, inactive parent-control styling and Light
profile-management sheets remain partial.

A GPUI regression opens every tab under a Light test window while the browser
uses Dark Space appearance, verifies independence after changing the Space
preference, and exercises Previews dismissal. All 22 focused Settings tests
pass. `light-settings-full-tests-3.log` records the green full workspace run: 1,096 browser, 241 core, 18 AI, 151 engine, two bundle and four importer tests. One credential-dependent AI test is ignored.

A later guarded capture used only the already-disabled Blank Window shortcut
to show a conflict with Cmd+1: `arc-shortcut-disabled-conflict.png`. Arc shows
a red selected row and keycaps, a warning marker and the exact tooltip
“⌘1 is already in use by ‘Go to Sidebar Item 1’”. Delete restores the captured
`---` state; Escape cancels recording. The other action was never removed.
`arc-shortcut-readonly.log` verifies the original Add Split View ⇧⌘E / Use Both
and disabled Blank Window in a fresh session without recording inputs.
No new row-context-menu reference is accepted: that menu was not exposed.

The read-only shortcut session verified the restored bindings, then failed to
expose a row menu. Its cleanup assertion raced the main-window close animation
and skipped closing Settings. Before the queued follow-up cleanup acquired
the screen lock, Arc restarted and all recorded inspection IDs disappeared.
`arc-current-inventory.log` verifies the four restored owner windows by name
(Grosst, Work, Яремко and Playable). An unidentified 500 × 500 popup was left
untouched. No further Arc inputs were made. Historical inspection scripts
are not safe templates: always close Settings in its own nested finally.

## Profile isolation after rename and restart

`isolation-native-run.hMix7u/{first,restart}.log` and `reports.jsonl` verify three private fixture profiles visiting the same localhost origin. Each starts without another profile’s data, retains only its own session/persistent/HTTP-only cookies, localStorage and IndexedDB markers, and writes only its own URL path into its profile history. Renaming profile 2 preserves its ID, Space assignment and storage identity. After native quit / CEF Complete and a full process restart, newly generated documents read the original stores before writing anything. `echo-isolation-{A,B,C}-restart.png` show those read-only results. Password/autofill and extension management remain outside this check.
