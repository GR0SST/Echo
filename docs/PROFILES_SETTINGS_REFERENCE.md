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

## Shortcut recording and runtime binding

The bundled catalog supplies exact action names/defaults. The accepted source
states are `arc-shortcut-recording.png`, `arc-shortcut-disabled-conflict.png`
and the earlier `parity/settings/arc_shortcut_edit.png`. Existing owner custom
bindings are reference content, never copied as Echo defaults. The conflict
copy uses the bundled “is already in use by” string and the other action’s
name. The source uses 64 × 64 keycaps, a 56 × 51 inset face at (4,4), rounded
system glyphs, and white 64 × 24 row keys even under Dark appearance. Source
Display P3 samples are converted to sRGB for GPUI, rather than treating raw
P3 components as sRGB. Row content starts 10 px below its section heading.

Install-local overrides distinguish an inherited default from an explicit
removal. Recording is intercepted before application bindings: Escape cancels
without mutation, Delete clears, and a valid modified/function key commits.
The catalog groups customized actions before defaults. Reset one/all rebuilds
the app keymap from its original bindings and refreshes native equivalents.
Changed defaults are removed together before replacements are added, allowing
a key released by another customization to be reused. Text-input bindings
retain their original focus predicates. Unsupported actions and Space
reassignment remain unavailable. Unknown stored action keys survive upgrades.

CEF callbacks read a shared live shortcut configuration, including retained
engine forks; replacement/reset reaches existing pages without calling CEF
under the configuration lock. Native runs use a private fixture, record a
conflict on Blank Window, create a Duplicate binding, invoke it with real web
focus, restart the entire process, invoke it again, reset it and verify that
the old key no longer duplicates. Ctrl+Option+1 did not reach the recorder during the first
native attempt; the accepted fixture uses Option+Command+1. All windows close
through native Cmd+Q / CEF Complete on successful runs.

Tooltip placement/timing, priority selection/two-press timing, Remove Conflicting Shortcut, extension
shortcut routes, system-reserved-key warnings, the no-binding/function-key/long-chord
selected appearance and remaining list materials stay partial. A conflict is
reported while the existing browser binding retains precedence; exact Arc
conflict-dispatch priority is not claimed. The source’s unverified More menu
placement is not reproduced.

The remaining action-only routes have no backing implementation: Search
Settings needs custom provider management beyond the existing six defaults;
profile-wide Clear Browsing Data needs an asynchronous, time-scoped deletion
API (the current engine clears one page origin or its context cache); Passwords
needs a credential-store UI/backend. They remain unavailable. Chromium WebUI
settings are not used as browser chrome.

Bindings with both a browser focus predicate and a global fallback retain both
contexts after customization. Configuration revisions advance even when a
reset returns to an earlier key vector, so a covered host cannot mistake an
old shared-engine configuration for the current one. GPUI regressions cover
both cases, in addition to native recording/dispatch/reset checks.

Final validation: `shortcut-full-tests-6.log` is green (1,109 browser, 242 core,
18 AI, 154 engine, two bundle and four importer tests; one AI test ignored).
`shortcuts-native-run.aUiY5k/{first,restart}.log` verifies the final bundle,
including its SHA-256 match, native web focus, persisted custom dispatch and
reset with strict exit 0. Reviewed final pairs are
`pair-shortcut-{conflict,custom,tooltip}-2x.png` and
`pair-shortcut-{conflict,custom}-keys-4x.png`. The conflict source has the
owner’s existing customized rows; Echo filters its private fixture to Blank
Window. Plain keycaps compare the source’s Add Split View chord with Echo’s
Duplicate chord; only the shared keycap layout is claimed. The final tooltip
pair intentionally records the remaining placement difference.


## Round 2 quit confirmation

A guarded, cancellation-only inspection in the shared Echo Test Space verified
that the already-enabled “Warn before quitting” preference opens Arc’s local
confirmation. No preference was changed and no source quit was accepted.
Both a browser-origin Cmd+Q and a Cmd+Q from owned Settings raised the browser’s
sheet. Settings and the one owned browser window were closed in the locked
script’s cleanup; baseline windows were preserved.

The settled reference is `target/parity/partial-closeout/arc-quit-warning-settled.png`.
The card is 450 × 248, centered in the 1000 × 760 host at (275, 256), with
17 px corners, a 50 px application icon at (44, 44), an 18 px bold heading,
and 37 px footer buttons 27 px above its bottom. The exact executable copy
contains U+2019 in “Quit, and don’t ask again”. The bundled Chromium string
“Hold $1 to Quit” is not the installed Arc flow. An initial entrance frame
placed the card above its settled position; it is retained as motion evidence,
not the final alignment reference. Echo uses its own application artwork/name.

The confirmation precedes the quit coordinator: Cancel/Escape leaves all
producers active; Return and Quit defer the existing data-safe shutdown;
remembered quit changes the install preference before saving. Repeated intent
raises the existing sheet. Private/Blank windows use the retained normal
settings, and focused browser overlays retain their parent identity. Forced
internal teardown removes the confirmation and retains all existing CEF/data
barriers. Native OS termination continues through its separately qualified gate;
this change does not establish Arc’s OS/no-window confirmation policy.

Native qualification uses only a disposable empty session. The probe verifies
Escape, a subsequent Command Bar open/dismiss, Return with exit 0, remembered
quit with exit 0, and a cold restart whose disabled preference quits without a
sheet. The explicit child-window activation was required for native Escape;
GPUI’s test platform alone did not expose the missing AppKit key-window handoff.
The row stays partial for Light, entrance/exit motion and remaining host/OS
entry conditions.

Reviewed enlarged pair:
`target/parity/partial-closeout/pair-quit-confirmation-2x.png` (settled Arc left,
Echo right). Native receipt/log:
`target/parity/partial-closeout/echo-quit-round2-final.log`, fixture
`echo-quit-review-stm8azll` (the log retains the authoritative full path).
The scripted source before/after pair also measures the dimmer without relying
on a guessed theme color. Four GPUI checks cover cancellation without sealing,
repeat presentation, remembered disable, disabled bypass and private preference
routing. The existing pre-quit producer test explicitly disables the warning.

## Round 2 shortcut footer actions

The live source selected-action footer uses Remove Shortcut, the conditional
Remove Conflicting Shortcut / Remove Conflicting Shortcuts, Reset All Shortcuts
and Extension Shortcuts…. The source references, temporary-binding restoration,
native CEF dispatch qualification and remaining functional limits are recorded
in [the closeout review](PARTIAL_ROW_REVIEW.md#round-2--shortcut-conflict-actions).
The selected footer is now present and all supported local binding actions are
connected. Extension editing and dynamic Space assignment remain unavailable.
Remaining menu material and text raster differences are cosmetic.

The successful native receipt is
`target/parity/partial-closeout/echo-shortcut-popup-retry.log` (exit 0, both CEF
shortcut outcomes, persisted Reset All). The subsequent correctly positioned
pair run repeats both action checks but reaches the drive timeout in CEF's
ShuttingDown phase; it does not qualify shutdown. Its log is
`echo-shortcut-functional-final.log`. No CEF source/build/publication was changed
to work around this separate teardown result.

Settings quit intent now resolves the retained browser owner before the fallback
window search. A disposable native two-window run verifies actual confirmation
bounds, Escape returning to that host, preservation of the first root and exit
0. See [Settings quit ownership](PARTIAL_ROW_REVIEW.md#round-2--settings-quit-ownership)
for the native receipt and the reused-layout Arc/Echo card pair. OS/no-browser
and other auxiliary-host policy remain partial; remaining Light/raster/motion
work is cosmetic.

## Round 3 shortcut priority

Guarded disposable-page probes in Echo Test establish the three priority
choices and descriptions. Prefer Arc consumes Copy URL before the website;
Prefer Websites lets a prevented event stay with the website and falls back to
Copy URL when the website leaves it unhandled. Use Both delivers both shortcut
presses to the website and also performs Copy URL on the second press. The
browser action must therefore not consume that second website event. Reset
Shortcut to Default restores the binding while retaining its independent Use
Both policy. Every temporary Copy URL policy was restored to Prefer Arc, its
original binding was preserved, and the sessions closed Settings and their own
browser without changing baseline windows.

Source receipts under `target/parity/partial-closeout/`:
`arc-shortcut-copy-policies-r3.log`, `arc-shortcut-use-both-r3.log`,
`arc-shortcut-use-both-results.json`, and
`arc-shortcut-defaults-r3.json`. The latter records the currently installed
policies, including owner-customized actions; it does not establish that a
customized binding is Arc's factory default. Reference images include
`arc-policy-prefer-browser.png` and `arc-policy-prefer-website.png`.

The isolated source boundary probes are
`arc-shortcut-boundary-isolated-r3.log` and
`arc-shortcut-boundary-fine-r3.log`. A prevented second press activates Copy URL
at a measured 5,633 ms gap and does not activate at 6,038 ms. The implementation
uses a six-second window. An ordinary X key and a Command Bar focus cycle do not
clear the pending first press (`arc-shortcut-consecutive-fresh-r3.log`). The
read-only expanded policy scan is `arc-shortcut-defaults-remaining-r3.json`.

The first native Echo probe exposed GPUI handling command-key equivalents
before CEF. A macOS adapter now forwards website-priority keys to the registered
focused Chromium responder before menu/toolkit equivalents; the shared engine
handler still controls website delivery and unhandled fallback. No CEF patch or
Chromium build is involved. Native `echo-priority-matrix-r3.log` qualifies all
three policies, handled/unhandled fallback, fast pairs, a delayed 5.4-second
pair and expiry after 6.1 seconds, with both keys reaching the prevented site.
Every case cold-loads its saved policy and exits with CEF Complete / status 0.
The current Ukrainian-PC input layout requires ASCII command-key normalization;
ASCII-capable layouts retain their own logical characters. Other platform/layout
variants remain unqualified. `echo-priority-picker-persist-r3.log` verifies native
keyboard selection, disk persistence and same-session Settings closure. The
native Settings picker has enlarged reviewed pairs
`pair-shortcut-priority-2x.png` and
`pair-shortcut-priority-menu-2x.png`. Menu material, typography and hover-state
differences remain cosmetic.

The tested private bundle contains the unchanged canonical framework
`20261025T192350Z-5008`, SHA-256
`55a60b71fd32579a392ee53f337a908cecb7b0f012971d6ad3317b4a32ec82aa`.
The executable was rebuilt in a frozen copy of that complete bundle; helper
protocols and the framework were unchanged. Full Rust validation passed
1,264 browser, 267 core and 185 engine tests, plus the remaining workspace
suites (`priority-production-full.log`). Individual reset/priority independence,
stale selection/conflict ownership, default live rules, renderer fallback, late
acknowledgements, native input timing, function-key names and layout
normalization have regression coverage. The Shortcuts row remains partial for
extension/Space/Favorites-ordering routes, the unobserved Reset All priority
scope and additional native layouts/platforms. Its remaining visual differences
are cosmetic.

## Round 3 — modifier link routing

Fresh guarded Arc references use only loopback fixtures in Echo Test:
`target/parity/partial-closeout/arc-modifier-{little,cmdclick,cmdshift,cmdaltprevent}-r3.log`
and their screenshots/AX receipts. Command-click loads a background Today tab;
Command-Shift-click selects a new Today tab. Option-Command-click opens Little
Arc in the source profile without replacing its normal tab. A page's prevented
click opens nothing. `arc-modifier-disabled-r3.log` verifies the Little Arc
checkbox disabled: the same Option-Command-click opens a right-hand Split View
pane. Its preference receipt records restoration of the original enabled value.
Split View also retains its separately verified context-menu action.

The renderer reports trusted modifier metadata through a private callback.
Only a subsequent CEF user-gesture navigation/pop-up with the same browser,
frame, document, epoch and target may consume it. Metadata does not cancel a
site's click or independently open a window. The Links preference gates the
Little Arc route; private, Peek and Little hosts do not enable the Little Arc
destination. Those host variants are not newly qualified.

CEF Alloy's `OnOpenURLFromTab` default loads the source frame even for a new-tab
disposition (`cef_request_handler.h`, `alloy_browser_host_impl.cc`). Echo now
accepts the existing foreground/background/Peek routes explicitly. Background
tabs materialize hidden CEF pages using their captured profile. Pending storage
initialization retries without reviving closed/replaced identities. Native
creation restores an already focused source page without pulling focus away
from an open Command Bar. No CEF patch, build or publication is involved.

The sidebar comparison also exposed Today ordering: linked tabs belong directly
below their opener rather than at the head of Today. CEF link requests now use the existing whole-item relative move;
Command Bar ordering and Pinned/Favorite placement are unchanged. The background
GPUI regression checks this order as well as visibility, selection and focus.
The existing relative-move helper keeps Split items together; native grouped-
opener placement is not newly qualified.

The initial native in-process-GPU background run passed destination loading,
source selection and trusted source key delivery, but timed out inside
`cef_shutdown` (strict status 124); it is not counted as a clean exit. A rerun
with production's separate GPU process passed the same assertions and exited
0 (`echo-modifier-cmdclick-r3-separate.log`). The canonical framework remains
`20261025T192350Z-5008`, SHA-256
`55a60b71fd32579a392ee53f337a908cecb7b0f012971d6ad3317b4a32ec82aa`.
The in-process automation shutdown variant remains unqualified.

The final Rust run is `modifier-full-split-r3.log`: 1,308 browser, 276 core
and 195 engine tests pass, plus the remaining workspace suites. Twenty targeted
background tests cover hidden loading, source focus, Command Bar drafts,
profile-context retry, closed-identity rejection and linked-tab ordering.
The Links row remains partial for custom global-shortcut recording and native
platform/routing variants outside these HTTP(S) fixtures. The background-toast
row retains its separate unqualified background Space-creation trigger.

All five production/separate-GPU cases pass with strict native exit 0:
`echo-modifier-{cmdclick,cmdshift,cmdalt,cmdaltdisabled,cmdaltprevent}-r3-split-qualified.log`.
The disabled Little preference opens a two-pane Split View and sends keyboard
input to its destination; it is not a background-tab fallback. The receipt is
`modifier-routing-qualification-r3.json`. Reviewed pairs in the same directory
are `pair-modifier-{cmdclick,cmdshift,cmdalt,cmdaltdisabled,cmdaltprevent}-r3.png`;
`pair-modifier-order-r3-2x.png` enlarges the source/below-opener ordering. Existing
Little-window width, Split toolbar appearance and row materials remain cosmetic.


## Global Little shortcut rebinding

The existing Arc Shortcuts recorder and Links preference are reused. The Echo
OS registration previously hard-coded Option-Command-N independently of that
recorder. It now follows the effective installed `New Little Echo Window`
action, unregistering its previous chord before registering a replacement.
An explicitly removed or conflicted action has no global route; disabling the
Links preference also retires it. Stale queued presses cannot invoke a retired
registration. An armed Settings recorder temporarily suspends registration so
it can receive the current OS chord. Individual reset restores the default without changing its
separate website priority.

Reference unqualified for Arc's system-wide remapping: the existing recorder
and default-global-entry references are qualified, but no owner's Arc shortcut
was changed for this check. No new UI or copy is introduced.

GPUI coverage records a custom chord through Settings, removes it, resets it
and records a conflicting Reload chord. Unit coverage checks the platform
modifier mapping, function keys, test-only extra Control, invalid chords and
press/release suppression. Full-workspace results are in
`target/parity/partial-closeout/global-little-full-r4-recording.log`. Native acceptance
uses a private profile, a complete canonical bundle and a disposable receiver
applet, with an extra Control modifier to avoid Arc's live default binding.
The production chord is never changed by that test modifier. Evidence is
`global-little-native-r4-recording-accepted.log`, `echo-global-little-r4-receipt.json` and
`echo-global-little-r4-38.png` / `echo-global-little-r4-40.png` in the same
directory. Additional platform/layout/chord coverage remains partial.
