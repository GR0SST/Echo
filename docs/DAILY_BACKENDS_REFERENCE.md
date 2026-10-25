# Daily-use backend reference

Reference: installed Arc 1.170.0, 2026-10-25. Evidence is local under
`target/parity/daily-backends/`. Browser inspections use the shared Echo Test
Space under locked, bounded scripts. A save-sheet closing race required a
separate guarded recovery; it restored the Space assignment and closed the
recorded test window while preserving every baseline owner window. No owner passwords are read or exported.

## Password storage

CEF 154.5's public `RequestContext`, browser and renderer interfaces do not expose
Chromium's password store. Echo therefore owns an AES-256-GCM vault per immutable
profile storage key. Its independent random 256-bit key lives in the native OS
credential store (`dev.echo.browser.password-vault`). The vault authenticates its
install/profile identity, uses a fresh random 96-bit nonce for each write, and
publishes ciphertext atomically through a private temporary file (0600 on Unix).
Keys and decoded payload buffers are zeroed on drop; credentials never implement
Debug or enter browser/session JSON. Missing keys, unavailable native storage,
invalid origins, malformed files and authentication failures fail closed.

Onboarding Passwords.csv now writes through this shared store. Earlier Echo
imports are migrated once from **Echo's** previous root-scoped Keychain service;
the original entries are retained. Migration never queries Arc's credential
service or files. Creation/rename of a display profile does not rename its vault.

Storage tests cover disk persistence, random nonces, ciphertext-only files,
profile substitution, lost keys, damaged ciphertext, CSV origin normalization,
update/delete/time filtering and one-time legacy migration. Native acceptance
uses only `echo-password-test-69cec731.invalid` and synthetic credentials in an
explicitly enabled disposable drive profile. Main-frame login save/autofill now share this vault; the GPUI manager now shares this vault. Storage implementation alone does not close password parity.

## Arc entry points and packed UI source

- Settings → Profiles → Search Settings… opens the observed exact URL
  `arc://settings/searchEngines`. `arc-search.png` captures the engine table and
  Space-or-Tab / Tab controls. No owner search preferences are changed.
- Settings → Profiles → Clear Browsing Data opens the observed exact URL
  `arc://settings/privacy`. Its Delete browsing data row opens
  `arc://settings/clearBrowserData`; `arc-clear.png` and `arc-clear-dialog.png`
  capture both stages. The dialog uses chips (Last 15 min, Last hour, Last 24 hours,
  Last 7 days, More), a collapsible category list, Manage other data, Cancel, and
  Delete from this device. Passwords are managed separately. No deletion is run.
- Arc's executable contains `arc://password-manager/passwords`. Arc's own
  `ArcCore.framework/Versions/A/Resources/resources.pak` resource 20047 contains
  the password-manager UI, including Router and `UrlParam.SEARCH_TERM = "q"`.
  RouteObserverMixin applies the route synchronously in connectedCallback before
  the asynchronous credential-group request returns. The list filters groups by
  this term and hides the list while groups are null. Any live manager reference
  must open with the unique synthetic test identifier already in `q`; never open
  the unfiltered owner list or run owner export/checkup actions.

The packed assets remain reference evidence only. Echo ships no Arc artwork,
logos, or bundled UI assets.

The legacy macOS migration enumerates attributes first, filters Echo's exact
profile key, then fetches one matching account's data at a time. Combining
`kSecReturnData` with `kSecMatchLimitAll` is explicitly unsupported by macOS;
see [Apple's keychain search contract](https://developer.apple.com/documentation/security/searching-for-keychain-items?changes=l__2&language=objc).
The native acceptance fixture seeds an older **test-only Echo** Keychain entry,
imports a second test CSV entry, restarts Echo, verifies both and another profile's
empty vault, and deletes the two test passwords and the legacy test entry.

Additional read-only captures: `arc-clear-ranges.png` shows More → Last 4 weeks /
All time; `arc-clear-expanded.png` shows Autofill form data, Site settings and
Hosted app data. `arc-password-filtered.png` shows the synthetic query's empty
manager result; `arc-password-add.png` shows Add new password (Site, Username,
Password, Note, Cancel / Save). The Add dialog is cancelled without saving.

Validated storage evidence: `vault-native-run.jPD2XT/{write,verify}.log` and PNGs.
Both independent app launches finish native termination with CEF Complete and
exit status 0. The acceptance harness also checks the encrypted vault and session
file directly for all synthetic plaintext markers. The two native test records
and the legacy test Keychain item are deleted by verification. This is backend
acceptance evidence, not an Arc/Echo visual-parity pair.

Search management references: `arc-search.png`, `arc-search-sites.png`,
`arc-search-engine-menu.png`, `arc-search-edit.png`, and `arc-search-add.png`.
Arc's Search Settings button opens `arc://settings/searchEngines`; Add/Edit uses
a 512 × 356 card, 20 px field insets, 36 px fields and pill-shaped footer buttons.
The packed English resource 348 supplies `system-ui, sans-serif`; resource 20047
supplies the shared 15 px dialog title. The captures were read-only: editors were
cancelled and the test tab, Settings and owned browser window were closed.
Echo uses `browser://settings/searchEngines?profile=<local profile id>` so a
Settings selection owns its changes even without a Space assigned to that profile.
Custom templates never inherit the previous engine's remote suggestion transport.
Factory entries retain Echo's six existing engines and Wikipedia; Arc's owner
site-search inventory is not imported. Generic engine icons, the wider Chromium
settings navigation, light colors and detailed hover states remain partial.

Search native acceptance: `search-native-run.jtZraf/drive.log` (CEF Complete,
exit 0) and `search-native-v5.log` verify Add → Make default → Command Bar search
and persisted custom-provider identity/URL, using a disposable Echo profile.
Inspected pairs: `search-page-pairs.png`, `search-add-pairs.png`, and the zoomed
`search-add-pairs-2x.png`. Dialog label tracking (Arc CSS: 0.4 px; GPUI lacks a
letter-spacing style) remains a documented typography gap. Title/label line
heights come from Arc's packed shared dialog/input styles (15 / 16 px).
Five GPUI tests cover editor input, custom default, Tab/Space policy, keyboard
scroll/focus, cancellation, deleted owners, and a profile switch before repaint.

## Password-manager metadata and isolated save reference

Version 2 authenticates its format tag as well as its install/profile identity,
while retaining version-1 reads and the existing OS-held encryption key. It adds
encrypted notes and the per-origin never-save policy. Imports and login updates
preserve notes; password edits/deletions preserve the independent policy. An
identity edit is one atomic write and rejects both stale rows and collisions.
Notes use Arc's 1,000 UTF-16-unit limit (`PASSWORD_NOTE_MAX_CHARACTER_COUNT` in
packed resource 20047), including the correct two-unit count for supplementary
characters. Notes and decoded credential buffers are zeroed on drop.

Eight vault tests cover legacy reads/upgrades, authenticated format tags, note
limits, import retention, stale/colliding edits, encrypted policy persistence,
profile isolation and existing ciphertext/key/migration cases. The disposable
native fixture now verifies notes and never-save policy across restart and
checks that imports preserve the manager's note. It removes its two synthetic
passwords, policy entry and legacy test Keychain item at the end.

The safe live password fixture is **Echo Password Fixture 69cec731**, created
without a new Space. Only Echo Test was temporarily assigned to it.
`arc-password-isolated-save-prompt.png` and its AX controls record Arc's actual
450 × 273 save card: “Save password for 127.0.0.1?”, “That way, it’ll be auto-filled
next time”, Never on this site / No / Save. The synthetic POST was verified and
the sheet was cancelled; no password was saved in Arc. The closing sheet's
animation invalidated a cached popup owner, so recovery used recorded IDs.
`arc-recovery-v31.log` confirms Echo Test restored to Grosst, all baseline
windows preserved and the owned window closed. `arc-password-clean-sidebar.txt`
contains neither fixture tab title nor its local host. The fixture profile is
now unused. Owner profiles and their passwords were never edited/read/exported.

## Clear-data native preflight

`cef-removal-probe.log` and `extensions-2026-10-25T10-55-17.449Z/` prove the newly
available opt-in CEF bridge can invoke `chrome.browsingData.remove` in an empty,
disposable Alloy profile. The other test profile cannot invoke that extension.
This is an API-capability probe, not seeded-data/time-range deletion qualification.
A direct browser-process BrowsingDataRemover bridge was prepared in
`data-native-source/browsing-data-bridge.patch`, but remains unapplied: the
shared CEF source preflight rejects `Digest mismatch:
content/browser/renderer_host/navigation_request.cc`. No shared source or
distribution was changed. That work needs the source stack qualified before
compilation; the GPUI clear-data dialog and production wiring remain pending.

Validated metadata acceptance: `password-notes-native-run.CShUY5/{write,verify}.log`
(each exits 0 with CEF Complete) and `password-notes-native-v2.log`. The harness
checks ciphertext and session JSON for all synthetic password/note/policy
markers. Full `password-notes-full-tests-v2.log` is green: 1,171 browser, 252
core, 166 engine, 18 AI, two bundle and four importer tests, plus one native
bootstrap test; one existing AI test remains ignored. This validates storage,
not the still-pending GPUI save sheet, autofill or password manager.

## Website login bridge

The CEF renderer reports only its main-frame HTTP(S) document. A native private
V8 callback is captured by an isolated closure, never installed on `window`.
Registration binds a random document token to CEF's browser/frame/origin; the
browser process supplies the immutable tab/profile ownership. Fill replies must
still match the current context, origin and token. A prior document may report a
submission across a successful navigation, but can never receive a fill reply.
Private request contexts do not install the bridge. No remote debugger is used.

Only trusted edits/submits from same-origin forms produce save candidates. The
GPUI sheet waits until the form disappears, suppresses unchanged saved passwords
and never-save origins, and expires pending requests after 30 seconds. Failed
forms that remain on screen do not trigger a prompt. Accepted Save/Never writes
run off the UI thread and hold application quit until their atomic vault update
finishes. Autofill chooses a sole account or the exact typed username; typed
passwords are never overwritten. Notes survive subsequent login updates.

The sheet uses the isolated `arc-password-isolated-save-prompt.png` reference:
450 × 273 px, 18 px corners, 62 px key tile, and Arc's Save / No / Never on this
site copy and keyboard hints. Source executable strings are also indexed in
`target/parity/toasts/arc-binary-strings.txt` at the Save-password entries.
Native `password-flow-native-run.1N0wuY/{form,autofill}.log` verifies a trusted
synthetic POST, encrypted accepted save, immediate native quit, cold restart and
a second POST with the exact autofilled test credentials. Both processes reached
CEF Complete and exited 0. `password-save-pairs.png` and its `-2x` crop compare
Arc (left) and Echo (right), inspected locally; minor font/icon/pixel-rounding
differences remain. `autofill.png` shows only the synthetic username and a masked
password. Fifteen focused tests cover origin/token authority, save/no/never,
expired documents, foreground-only presentation, per-profile policy, ciphertext
and quit leases. Cross-origin/iframe/closed-shadow forms, account-choice UI,
light sheets and native non-macOS behavior remain unverified. A vanished form is
a conservative success heuristic, not Chromium's internal login-success signal.

The first native attempt used the optional local source-built CEF and failed
with SIGTRAP inside CEF shutdown. It is not a passing acceptance run. The passing
run explicitly used the portable CEF distribution. No shared native source or
CEF distribution was changed. Two further guarded manager-reference attempts
stopped at the header/sidebar checks, before profile reassignment; their owned
windows were closed and every baseline window preserved.


## GPUI password manager

Settings → Profiles → Passwords and the Manage Passwords Command Bar action open
`browser://password-manager/passwords`. A Settings selection is carried as an
explicit profile ID, including profiles without a Space. Invalid/deleted owners
never fall back to the default vault. Search uses the observed `q` route parameter.
Add/Edit, multiline encrypted notes, copy/reveal, immediate Delete and five-second
Undo share the login/import store. Atomic insert refuses duplicate replacement;
identity edits reject stale/colliding rows. Accepted writes retain the existing
quit lease, and per-profile revisions refresh other manager windows. Filter,
editor and asynchronous results are tied to the original tab/profile/route.
Site/help links retain the manager's profile, creating a profile window if needed.
No Chromium WebUI is used, and no account/sync/cloud controls are included.

The live list reference `arc-password-manager-fixture-list.png` contains only the
synthetic `echo-password-test-69cec731` credential in our isolated Arc profile
`Echo Password Fixture 69cec731`; the URL contains the synthetic filter before
first paint. `arc-password-add.png` supplies the 512 × 521 Add card. Labels,
1,000 UTF-16-unit notes, detail/edit structure and deletion/Undo strings come from
Arc's packed resource 20047 and English locale entries 8390–8518. The exact help
URL `https://support.google.com/chrome?p=settings_password` is present as UTF-16
in ArcCore at offset 231591584 (`arc-password-help-source.txt`). Echo draws its
own key, globe, eye and row-arrow geometry; no Arc assets ship.

Inspected Arc-left/Echo-right comparisons: `password-manager-list-pairs.png`,
`password-manager-add-pairs.png` and both `-2x.png` crops. Small typography/icon
and focus-state differences remain. The detail-entry reference attempt remained
on Arc's list, so the source-derived detail/edit layouts, device-presence auth,
light appearance, affiliated-domain grouping and advanced error states remain
partial, not visually verified. Reveal/copy re-read the OS-backed vault, but do
not implement an additional Touch ID/device-presence challenge.

Native `password-manager-native-run.lPRXJr/{add,reopen}.log` verifies imported CSV
and GPUI Add use the canonical encrypted profile vault, cold restart, row opening,
Delete and Undo with actual clicks. Both native processes reach CEF Complete and
exit 0; ciphertext and session JSON contain neither synthetic passwords nor note.
Seven GPUI tests exercise editing, multiline input, cancellation, duplicates,
unused/deleted owners, late filter events, cross-window refresh and login-store
integration, plus atomic duplicate coverage in the vault tests.

Several manager-reference sessions stopped on newly appearing owned Arc popups;
separate guarded recovery scripts restored Echo Test to Grosst and closed all
recorded owned windows while preserving baseline windows. No owner profile or
password was inspected or changed. The isolated fixture profile retains only its
synthetic password; no Space was created or deleted. Final sidebar verification
is recorded in `arc-password-final-cleanup.log`.

Clear Browsing Data remains blocked at native-build qualification: the final
read-only preflight reports `Digest mismatch: cef/BUILD.gn`
(`data-preflight-final.log`). The direct browser-process BrowsingDataRemover C ABI
patch under `data-native-source/` is prepared but unapplied and uncompiled.
Shared native sources/distributions were not modified. No dialog is shipped that
silently substitutes cookie-only deletion or ignores the selected time range.

Final manager full-suite run (`password-manager-full-tests-final.log`) passed:
1,196 browser, 252 core, 169 engine, 18 AI (one existing ignored), two bundle,
four importer and one bootstrap test. The integration rerun
`password-flow-native-run.fKLwUW/{form,autofill}.log` also passed save → immediate
native quit → cold-restart autofill, with both processes exiting 0.

## Clear Browsing Data native integration

The GPUI privacy page and 512 px deletion dialog use the inspected Arc routes,
chips, seven categories, Show more, Manage other data and Cancel/Delete footer.
`arc-clear-manage-other.png` records the separate Other data card. Arc's packed
resource 21246 gates Google search-history/My Activity links on browser sign-in;
Echo omits those account-dependent rows. Other search engines retain Arc's exact
explanation. Passwords opens Echo's profile-owned manager; no password category
is silently included in browsing-data removal.

Each accepted operation freezes its explicit profile, category mask and Unix
microsecond start time (`-1` for All time). It retains the storage context and
quit write lease until the native callback finishes. Only a zero failed-data-type
mask publishes local history/download metadata removal. Cancellation does no
engine work; a native failure keeps the dialog and local metadata. The operation
survives navigation away from its source tab and uses the selected Settings
profile even when that profile owns no Space. Download history never deletes files.
Legacy download records expose completion time rather than start time, so bounded
local download-history removal uses completion time; unknown dates are preserved.
Permission usage records have no dates, so their local display inventory is reset
only for All time; Chromium remains the authority for effective permissions.

The native patch is `patches/cef154/browsing-data-bridge.patch` (SHA256
`be7e0d89282967b03e00e5ca78edfb2b76153ec4ad8fc2fa4bcc13c2eb0cc978`).
The engine adapter loads only its own bundled framework and checks ABI size/version.
**ABI v1 ownership:** CEF's generated `CppToC_Unwrap` consumes one incoming C
reference. Each call transfers a cloned RequestContext reference; the registry's
reference remains owned by Rust. Status 1/3 reject before unwrap; 0/2/4 consume
the transferred reference. Accepted callbacks keep the runtime/context/library
alive independently of the polling ticket. No default-context fallback exists.
Chromium supplies site-data/cache/form/settings/app counter strings; Echo counts
its own history and download records for the same selected range.

Disposable direct-ABI tests passed for Default and named profile 2 against staged
framework `50b56c0f28e5c5220d14b721914edced8d18d99e5fd5ee5231ea2f8ac2ec05fe`:
`refcounted-abi-smoke/abi-smoke-t0kh_veu/result.json` and
`named-abi-smoke/abi-smoke-5nup0pk9/result.json`. Each seeds a synthetic cookie,
localStorage, IndexedDB and CacheStorage; the callback reports failed bits zero,
all four values disappear, and CEF completes native termination with exit 0.
The CEF agent owns final candidate qualification and canonical publication.

Real-button native deletion also passes for Default and named profile 2:
`ui-release-smoke/abi-smoke-vb0y7isd/result.json` and
`ui-named-release-smoke/abi-smoke-wdksnmgq/result.json`. Those runs use Command Bar
navigation, category clicks, More → All time and Delete from this device,
then revisit the local fixture and require CEF Complete/exit 0. GPUI regressions
also cover cancellation/draft reset, native failure, accepted-operation ownership
across navigation, direct-route Enter isolation, focus after deletion, and the
Settings entry for an unused profile.

Comparison crops: `clear-dialog-pair.png`, `clear-ranges-pair.png`,
`clear-other-pair.png`; zoomed inspection is `clear-dialog-zoom.png`. These compare the component
at its original pixel dimensions, accounting for the existing sidebar-width
difference. Shared sidebar/header, exact typography/icons and non-macOS rendering
remain partial; these captures do not close those independent rows.
