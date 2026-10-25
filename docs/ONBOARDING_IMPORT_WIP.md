# Local onboarding and browser import

## Reference and scope

Reference: installed Arc 1.170, `/Applications/Arc.app`. The first-run flow was
**not launched** on the owner's installation. Copy, colors, and illustrations
were inspected in `ARC_AuthFeature.bundle` (`Assets.car`, `static-bg.png`) and the
installed executable's strings. Extracted resources remain only in ignored
`target/parity/onboarding-import`; no Arc artwork is shipped with Echo.

Arc's installed menu places **Set as Default Browser** and **Import from Another
Browser…** in the application menu, rather than File. Echo follows that observed
location. Arc account, sync, referral, and member-card steps are excluded. AI
sign-in is outside this change.

The source picker and Safari Full Disk Access sheet were inspected live using
the shared guard, one owned normal window, and its directly opened import popup
in the shared `Echo Test` Space. No Space was deleted, no profile switch/import
was confirmed in Arc, and no disk permission or system default was changed.
Safari's Next button opens System Settings automatically. The newly created
Settings process and both owned Arc windows were closed after capture.

## Implemented interfaces

- `crates/importer`: explicit macOS/Windows/Linux browser paths and read-only
  readers for Chrome, Edge, Brave, Firefox, and Safari. Chromium JSON bookmarks,
  SQLite history, and unencrypted SNSS sessions; Firefox Places and JSON/LZ4
  sessions; Safari binary/XML bookmarks, History.db, and LastSession.plist.
- `Browser::import_local`: transactional publication into a new Space with
  selected destination profile, Pinned bookmarks/folders, open-tab order and
  history merge. Existing Spaces and profile data remain intact.
- GPUI import source/destination/bookmark selection, category progress,
  cancellation, password CSV chooser/drop, and resource-backed completion toasts.
  There is no additional completion screen; onboarding continues directly to
  its default-browser step. A wholly failed read without passwords publishes
  neither a new Space nor a new profile. Read errors are
  surfaced per category. A closed owner cancels reading and rejects late data.
- CSV passwords use a separate credential-store interface, never session JSON.
  The macOS implementation stores generic passwords in the local Keychain,
  explicitly without synchronization. Windows/Linux return unsupported. Source
  CSV buffers and stored credential strings are zeroed on release; CSV decoder
  internals are not guaranteed to be erased.
- First-run state is persisted and resumed, while existing sessions skip setup:
  theme (existing material API), three verified Favorites, import, explicit
  default-browser consent, basics, and shortcuts.
- Default-browser interface: macOS LaunchServices registration and modern
  NSWorkspace HTTP/HTTPS handler requests with OS consent. Cancellation stops
  the second request; simultaneous app-menu/setup requests are serialized.
  Windows/Linux have explicit stubs. Tests inject a fake and never change the
  machine's default browser.

## Evidence and measurements

All paths below are relative to `target/parity/onboarding-import/`:

- `arc-app-menu/file-menu.png` and `file-menu.txt`: actual menu placement.
- `arc-import-safari-retry/selected.png`: source picker reference.
- `echo-import-first/selected.png`: Echo source picker comparison.
- Source card: both 580 × 454, 20 px radius, centered in a 1000 × 760 owner.
  Footer inset 28 px, 8 px button gap; source rows 176 × 49 px idle,
  182 × 52 px selected including the selection border. Echo uses its own
  icon and the installed source browser's icon (Safari is resolved through its
  application symlink, avoiding the Finder alias badge). The selected-row border, close icon and footer widths received final optical
  corrections; use the final picker zoom pair listed below.
- `arc-import-next-held/next.png`: Safari access sheet, 790 × 632.
- `echo-import-first/{bookmarks,passwords,complete,imported}.png` and
  `session-after.json`: fixture import, excluded bookmark absent, selected
  folder/Pinned bookmark, history, and ordered tabs in a new local profile.
- `ContentIntro1.png`, `ContentIntro2.png`, `auth-assets.json`, `arc-strings.txt`:
  resource-only onboarding evidence. These are not licensed product assets.
- Native onboarding/relaunch evidence: `echo-onboarding-first/` (see validation
  notes below). The final captures follow the corrected parent/child resize
  path; setup completes and remains dismissed after relaunch.

## Validation

- `RUST_TEST_THREADS=4 cargo test`: 939 browser, 195 core, 126 engine,
  2 importer unit and 4 importer integration tests passed before rebasing
  (`full-tests-prepush.log`). The first push-lock run after rebase passed 960
  browser, 198 core, 128 engine and 6 importer tests, plus the AI tests.
  Final outcome-flow validation (`full-tests-outcomes.log`) passed 965 browser,
  198 core, 128 engine and 6 importer tests; AI tests passed with their existing
  live-provider test ignored. The required push-lock sequence repeats the suite.
- Fourteen GPUI tests cover disabled Next/cancel, bookmark selection/new profile,
  owner retirement during reads, explicit default consent/cancel, resuming theme
  and Favorites without duplication, shortcuts Esc, concurrent default requests,
  credential-store/session separation, owner resize, direct onboarding continuation,
  partial-success feedback, wholly failed imports, long profile lists, and discarding old tree/password selections on a new read.
- Native fixture import (`echo-import-first/complete.png`) closes the picker
  and displays the resource-backed success toast. The import-inclusive first-run
  run (`echo-onboarding-import-final/default.png`) proceeds directly to default
  setup; `session-after.json` clears onboarding after Done, and `reopened.png`
  confirms it remains dismissed. Both scripts exit cleanly through CEF teardown.
- `arc-source-picker-zoom.png` and `echo-source-picker-zoom.png` are 2× crops
  inspected directly. Final fixture session proves history, folders, excluded
  bookmark and ordered-tab behavior. No owner passwords/defaults were changed.

## Remaining partial work

- Onboarding layout, procedural background, transitions, full Favorites catalog,
  tutorial video, separate shortcuts-window geometry, blocker and AI steps are
  not 1:1 validated. The current 200 ms fade is provisional, with reduced-motion
  handling; Arc timing has not been established from the resource-only evidence. The current local flow uses the observed import-card layout
  and resource copy; do not claim full onboarding visual/motion parity.
- Source chooser geometry was compared; destination, bookmark/progress, and
  password screens use resource copy but were not visually verified in Arc.
- Safari access-sheet illustration is a GPUI approximation; Restart Echo is
  disabled. Complete restart/FDA recovery and modern Safari profile/session
  formats remain outstanding.
- Encrypted Chromium v5 sessions, encrypted source password databases, cookies,
  autofill, browser-managed password lookup/autofill, and native Windows/Linux
  credential storage are not implemented. CSV Keychain writes were verified
  through an injected store; actual owner Keychain data was not modified.
- Application actions currently need an open BrowserWindow; last-window-closed
  fallback remains to be implemented.
- Default request shim compiles on macOS; OS consent/cancellation was not driven
  against the owner's defaults. Weekly follow-up prompts and Windows/Linux
  registration remain outstanding.

## Format/API references

- Chromium session commands:
  https://chromium.googlesource.com/chromium/src/+/main/components/sessions/core/session_service_commands.cc
- Chromium session storage versions/marker:
  https://chromium.googlesource.com/chromium/src/+/main/components/sessions/core/command_storage_backend.cc
- Safari bookmark structure:
  https://chromium.googlesource.com/chromium/src/+/main/chrome/utility/importer/safari_importer.mm
- macOS browser registration/consent:
  https://chromium.googlesource.com/chromium/src/+/main/chrome/browser/shell_integration_mac.mm

## Additional export adapters (2026-10-26)

The existing source sheet accepts one dropped Safari export ZIP/HTML or Firefox
JSON/JSONLZ4 bookmark backup. Source recognition does not read file contents on the
UI thread. ZIP history uses Apple’s version-1 metadata/content schema rather than
localized names; all included histories target the explicitly selected local
profile. Firefox profiles missing places.sqlite fall back to a dated backup.
Existing password upload can read one CSV from a Safari ZIP; passwords remain
explicit and outside the browser session/model. Archives are read in memory, never
extracted; file count, expansion, nesting, node count and password size are bounded.
This closes an export-format subset, not cookie/autofill or encrypted live-database
import. Source-sheet export selection, step guards and publication have GPUI coverage;
fixtures never contain owner data. See FUNCTIONAL_GAPS.md for remaining contracts.
