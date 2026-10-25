# Daily-use backend reference

Reference: installed Arc 1.169.0, 2026-10-25. Evidence is local under
`target/parity/daily-backends/`. All browser inspections use the shared Echo Test
Space and close their test tab, Settings window (when opened), and owned browser
window in the same locked, bounded script. No owner passwords are read or exported.

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
