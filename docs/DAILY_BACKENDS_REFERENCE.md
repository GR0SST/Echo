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
explicitly enabled disposable drive profile. Save/autofill and the manager UI
remain pending; storage implementation alone does not close password parity.

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
