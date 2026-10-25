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


## Profile draft ownership

`arc-settings-drafts.log` opens the source creation draft from Profiles and
cancels without entering a name or saving an assignment. Settings and the
claimed browser are closed in the same script. `pair-profile-create-draft.png`
compares the 450 × 426 card at 2×; original Echo artwork and omission of the
excluded sync notice are intentional. Paragraph baselines, field/keycap border
and material differences remain, so creation is still partial.

The native Echo check exposed an ownership defect: closing Settings could leave
its browser behind another window while the transparent sheet was raised.
The host now activates its own browser before attaching the sheet. The GPUI
regression observes the browser activation transition, focused profile input
and Escape dismissal. `echo-profile-final.log` records the clean native run;
the corrected full-window image shows the disposable empty browser beneath the
dimmer, rather than a peer's window. No owner profile was created or edited.

Same-profile Blank-window projections also retain explicit developer-site
choices. A core regression checks positive and negative overrides after session
restoration and projection; creating another profile retains its independent
localhost default. This does not qualify the remaining profile-switcher visuals.


## Export Data

The fresh Arc Advanced footer opens an 880 × 448 native Save sheet attached to
its 684 × 650 Settings window. It was cancelled without exporting owner data.
Echo previously used GPUI's detached application-modal chooser. Export now
prepares the existing native sheet against its Settings parent, presents it
outside application/model borrows and retains the exact suggested Echo filename.
Ordinary downloads keep their next-free-name policy. The added regression checks
both naming policies against an existing file without modifying it; the existing
GPUI footer regression checks real Markdown writes and cancellation.

`pair-export-sheet.png` and `pair-export-field-3x.png` compare the settled native
sheet and field/button geometry. Wait for the file chooser's accessibility tree
and filename validation before taking this pair: early cold-start captures showed
an initializing gray pane, rather than the final focused state. The shared native
sheet's activation behavior is unchanged.

`echo-export-minimal-save-final.log` records Save enabled, a real write to the
run's disposable `native-export/Echo Data Export.md`, exact expected Markdown,
Settings retained, sheet exit after its closing animation, CEF Complete and exit
0. `echo-export-focus-final.log` independently checks Escape closes only the
sheet and preserves Settings, followed by clean shutdown. No owner data/file was
exported or replaced. This closes the stale locked-session qualification note.

The final Rust/native bundle uses the published canonical framework SHA-256
`9210e6a73e91c143fb62b68261990425226796b962d5c814492290ddb4129324`.
CEF source, patches, builds and publication are owned by the CEF agent and were
not changed here. `final-stack-tests.log` is the green four-thread workspace run;
its browser suite has 1235 passing tests, with one live-provider AI test ignored.

Two of the original 30 partial rows are now done: General → Profile Settings…
and Advanced → Export Data. The other 28 retain the specific limits below.

## Remaining row audit

Read-only Settings overview pairs are named `pair-settings-<tab>-overview.png`
for General, Profiles, Links, Shortcuts, Max, Advanced and Icon. They establish
the current control inventory and visible differences; they are not acceptance
of unexercised backend actions. Earlier approved credential/search/data pairs
remain under `target/parity/daily-backends/`. No real password, provider key,
account consent or owner profile mutation was used to make a row look complete.

| Section / row | Why the row remains partial |
|---|---|
| 12 — isolated profile data | Existing disposable-profile cookie/storage/history/vault checks pass; iframe/cross-origin login forms, account choice, manager detail/device-auth visuals and extensions are still incomplete. List/Add/Light pairs do not prove those paths. |
| 12 — profile creation | The fresh 450 × 426 draft pair still differs in paragraph baselines, keycap/border treatment and material. The owning browser is now raised correctly. The following Space-choice stage and non-macOS rendering retain the earlier qualification limits. |
| 12 — Space profile warning | The existing captured confirmation is implemented; empty-window warning conditions, switcher/material and split/window variants are not established by the new profile-list overview. No assignment was saved. |
| 12 — profile deletion | Exact-name single-profile deletion works in the model/GPUI tests; the bundle still contains Delete unused Profiles and its warning. A bulk UI and lease-safe disk reclamation are absent. The observed fifth footer button is Rename, not a bulk-delete control. |
| 12 — per-profile settings | Search, archive, download preferences and native data removal work. New documents still persist an app choice without the document-app integration; the overview does not close that backend gap. |
| 13 — Archive tabs after | Four choices and selection work. Cosmetic: the fresh menu crop still differs in vibrant material/corners/rasterization. |
| 13 — Previews Settings | The fresh 422 × 370 pair still differs in checkbox, keycap and border treatment; site-specific hover previews are missing. |
| 13 — Warn before quitting | The measured local confirmation now gates menu/Cmd+Q before teardown is sealed. Native Cancel, Return, remembered disable and cold-restart bypass pass with a disposable session. Private/Blank preference routing has GPUI coverage. Settings uses its retained browser owner; the native two-window check preserves both roots and returns to that owner. OS/no-window entry and other auxiliary hosts remain unqualified. Cosmetic: Light and animation are deferred. |
| 13 — Profiles tab | Rename/delete/search/data/manager entry points exist. The broader row still includes bulk deletion, document-app behavior and incomplete search/manager visual details. |
| 13 — Links tab | Existing ATC is retained; remaining Little Arc launch preferences are not closed by the read-only Links overview. |
| 13 — Shortcuts | Selected footer Remove Shortcut, Remove Conflicting Shortcut(s) and Reset All now work, including live CEF dispatch and stale-menu guards. Priority/two-press dispatch, extension bindings, Space reassignment and Favorites ordering remain missing. Menu material/raster differences are cosmetic. Source temporary disabled-test binding was restored; no competing owner binding was removed. |
| 13 — Max tab | Fresh overview retains link/font differences; no exposed Ask-on-Page control was observed and the owner's disabled download-renaming preference was not changed. Plain/accessory toast layout reuse is not qualification of that Max feedback. |
| 13 — 5-Second Previews | Anonymous public-page/provider fixture checks cover the implemented panel; automatic-site coverage, exact blur/fonts/icons/motion, image/system-share payloads and non-macOS behavior remain incomplete. |
| 13 — ChatGPT OAuth | Offline security/loopback checks do not establish live account consent and inference. No owner account was used. |
| 13 — manual provider credentials | Local keychain/loopback streaming checks do not establish live providers or Windows/Linux credential stores. No owner key was read. |
| 13 — Advanced tab | Export's stale locked-session note is handled separately. Seeded translation-reset persistence, full global Settings, managed/no-owner behavior and remaining integration/visual states stay unqualified. |
| 13 — alternative icons | The original Echo variants are intentional; remaining radio/material/font differences and Windows/Linux native icon application keep this partial. Arc art/referral controls are excluded. |
| 16 — page card | Fresh empty/loaded corner crops retain 10 px insets and 6 px radius. Cosmetic: opacity, shadow and border calibration are deferred. Sidebar width is owned by another section. |
| 16 — loading glow | Prior measured pulse evidence remains valid for the implemented phase; exact finishing curve and split/other-platform variants are not newly qualified. |
| 16 — zoom badge | Fresh 110% geometry agrees; click/dismissal motion, material/theme and other-platform input behavior remain unqualified. |
| 16 — Site Control Center | The camera tile now enters the existing scoped region capture; native Escape preserves browser/Command Bar input. Full Page/Portrait, extension management, usage/media counts, main-panel keyboard traversal and embedded cross-origin permission coverage remain missing/unqualified. Remaining visual differences are cosmetic. |
| 16 — hovered URL | Fresh Dark capsule geometry/colors/font are corrected. Shadow/transition curves, Light and remaining elision/host variants are not accepted. |
| 16 — Find | The fresh NOT FOUND panel now clears the developer toolbar. Arc disables Find and Replace on this ordinary document; editable-context replacement remains unqualified. |
| 16 — Developer Mode | Fresh localhost default/disable/reload/restoration is implemented and tested. Toolbar glyph/material polish, split variants and other loopback-address defaults remain unqualified. |
| 18 — copied links | Ordered tab/Split/folder copying is wired; the vague old advanced-selection note is not treated as an absent toast hook. The verified copy pair is reused. Normal-pointer share-menu anchoring/hover and non-macOS sharing remain incomplete. Arc-hosted Shared Quotes are excluded; the local text-fragment action is separate. |
| 18 — pin/unpin | Fresh plain-feedback pairs confirm exact text, position, height and route conditions; Cosmetic: small width/edge/text raster differences remain. Drag and folder hooks retain their existing regression coverage. |
| 18 — archive/restore | Auto-archive and tab restore feedback work; the archived-folder model/restore is absent. Library ownership remains with the section 14 agent. |
| 18 — background tab/Space | Background tab reveal works; the New Space Created trigger is not implemented/qualified. No extra test Space was created to force a string-only state. |

No ordinary download-completion toast was added. The inventory contains Max
rename/revert copy, not an ordinary completion string. “We cleaned up!” remains
partial without a safe live/layout reference; no banner layout is guessed.

## Round 2 — shortcut conflict actions

Arc's actual pointer-opened footer menu was inspected in the shared Echo Test
Space. A previously disabled Blank Window binding was temporarily assigned for
the conflict reference and restored to disabled before closing Settings and the
owned browser. No competing owner binding was removed. Exact singular/plural
copy also appears in the installed executable. References are
`arc-shortcut-native-menu.png` and `arc-shortcut-menu-no-conflict.png` beneath
`target/parity/partial-closeout/`.

The selected footer exposes Remove Shortcut, conditional Remove Conflicting
Shortcut(s), Reset All Shortcuts and Extension Shortcuts…. Extension editing
remains disabled until its GPUI route exists. Conflict removal revalidates both
the selected assignment and complete conflicting set, clears all supported
competitors and reapplies runtime dispatch. Dynamic Space conflicts remain
disabled because their reassignment is not implemented. Three new GPUI checks
cover default/custom and plural conflicts, selected removal, Reset All, actual
action dispatch and stale menu state.

The disposable native fixture records Copy URL as Command-R, removes Refresh's
competing binding, proves Command-R copies the local page URL without reloading,
then uses the footer's Reset All and proves native Command-R reloads again. The
existing 3× pair is `pair-shortcut-conflict-menu-3x.png`. Native-menu material and
font raster are cosmetic; the row stays partial for priority/two-press policy,
extension editing, dynamic Space reassignment and Favorites ordering.

Per the functional-only closeout instruction, remaining shadows, pixel offsets,
raster differences and motion timing are cosmetic and are deferred; they are
not promoted to done.

## Functional-only scope limits

The Little Arc global shortcut, external-link routing and expiry choices already
have consumers; the remaining launch toggle is Option-Command clicking a page
link. Its setting currently persists without a click-routing consumer. Site
Previews are also persisted choices rather than an implemented site-preview
backend. Neither is accepted by its Settings overview pair.

Shortcut website priority/two-press dispatch, extension bindings and dynamic
Space reassignment remain functional work. The existing Command Bar Favorites
ordering toggle works; its missing Settings menu is an entry-point gap, not an
absent ordering model. Folder archive/restore needs a canonical archived-folder
model. Background Space creation has no qualified source trigger: creating a
new source Space only for this observation would violate the shared Echo Test
rule. The shared source Space is nonempty and contains other agents' tabs, so
an empty-Space profile-warning condition cannot be manufactured by clearing it.
No owner profile or assignment was changed for this review.

SCC full-page/Portrait capture, extension management, counters/media indicators,
main-panel keyboard traversal and embedded permission qualification remain
separate functional gaps. The ordinary region capture entry is handled below.
The source full-page action was observed copying a screenshot and showing
"Screenshot Copied to Clipboard" with Save/Share; this is not an ordinary
download-completion toast and does not qualify a new full-page backend.

## Round 2 — SCC region capture entry

A fresh guarded source session clicked the SCC camera on a disposable local page
in Echo Test. It enters region selection; Escape returns to the page. The script
archived only its own fixture tab, closed its one browser and preserved every
baseline window. No Settings window or extra Space was created.

Echo now routes this formerly inert tile through the existing document/profile
capture scope and checked capture backend. Two GPUI regressions drive the tile,
Escape and a subsequent Command Bar open, and reject an unavailable backend.
The native fixture also navigates through the Command Bar after Escape, verifies
two persisted local fixture tabs, CEF Complete and exit 0. Its log is
`target/parity/partial-closeout/echo-scc-camera-round2.log`.

Reviewed pairs in that directory: `pair-scc-region-entry.png` and
`pair-scc-region-instructions-2x.png`. These qualify entry/dismissal, not remaining
hover-state or pixel parity. Both instruction variants in the pair (Drag / Click
or drag) occur in the installed executable. Initial pointer/hover-state parity remains unqualified; remaining capture
material/raster differences are cosmetic and deferred. Full Page/Portrait and the
other functional SCC gaps above remain partial.

## Round 2 — Settings quit ownership

Settings now supplies its retained browser handle to quit confirmation instead
of falling back to the first browser in the install. The existing guarded Arc
Settings-to-browser warning reference and the approved card layout are reused;
no owner Arc window or profile was modified. A new GPUI regression opens two
normal hosts and verifies the warning belongs to Settings' second host.

The native disposable two-window fixture opens Settings from the second root,
invokes Command-Q, compares the confirmation's actual CG bounds to that root,
then verifies Escape returns to it while preserving the first root. CEF Complete
and exit 0 are recorded in `echo-quit-settings-owner.log`; the captured IDs and
bounds are in `echo-quit-settings-owner-receipt.json`. The inspected reused-layout
pair is `pair-quit-settings-owner-2x.png`, all beneath
`target/parity/partial-closeout/`. OS/no-browser and other auxiliary-host entry
conditions remain unqualified. The remaining card/font/Light/motion differences
are cosmetic and deferred. No row is promoted to done on that evidence.

## Round 3 — website/browser shortcut priority

Supported action rows now expose Arc's three priority choices. Policies persist
independently of bindings, follow the winning assignment after conflict removal,
and reach each retained native page. The macOS key-equivalent adapter prevents
GPUI/application menus from preempting website-first keys, including the current
Ukrainian-PC command layout. Chromium supplies unhandled fallback; Use Both
preserves website delivery for both presses and queues the browser once for the
second press. Native Arc observations bracket its six-second window.

All six disposable Echo native cases passed, including cold policy load,
handled/unhandled fallback, fast and 5.4-second pairs, 6.1-second expiry and clean
CEF exit. Native picker keyboard selection persisted to disk and Settings closed
in the same session. Full tests passed: 1,264 browser, 267 core, 185 engine, and
the remaining workspace suites. Reviewed pairs and receipts are recorded in
[the priority reference](PROFILES_SETTINGS_REFERENCE.md#round-3-shortcut-priority).
The row remains partial for extension/Space/Favorites-ordering routes, Reset All
priority scope and additional native layouts/platforms. Visual differences are
cosmetic; no other parity section was changed.
