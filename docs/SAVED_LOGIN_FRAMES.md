# Saved-login frames and account choice

The password bridge authenticates every form against the CEF browser, frame,
current V8 document token and HTTP(S) origin. A document token cannot select a
sibling frame or survive navigation. Sandboxed opaque documents are excluded.
Vault lookup remains scoped to the retained page's local profile.

Same-origin iframe logins can fill automatically. As observed in Arc on the
local synthetic fixture, a cross-origin iframe stays empty until the user
chooses an account. Focusing its username field offers an origin-labelled
account dropdown. The browser retains no plaintext credential in session JSON;
credentials remain in zeroizing values and are never included in Debug output.
The selected fill is bound to the requesting field/form/password elements and
rejects a stale field or a typed password. Lookup replies and picker callbacks
also recheck page ownership, document, vault revision and popup generation.

The GPUI dropdown is a small child window, so the CEF document can retain input
focus. Up/Down, Return and Escape are intercepted only for its originating
browser while it is open. Switching tabs, changing the vault, navigating,
resizing, moving focus away or opening another browser popup closes it.
Iframe postMessage traffic carries geometry only, never a password, username,
origin authority or document token.

## Reference and qualification

The Arc reference used only `127.0.0.1` synthetic accounts in the shared Echo
Test Space. No owner website credentials or password extension settings were
changed. Owned reference tabs/windows were closed after every session.

Reference pairs live under `target/parity/functional-logins/`:

- `arc-iframe-accounts.png` and `echo-iframe-accounts-pair.png`;
- `arc-iframe-accounts-crop.png` and `echo-iframe-accounts-crop.png`;
- `arc-account-selected.png` records explicit account selection.

Arc's measured popup is 225 × 171 px for the top-level form and 233 × 171 px
for the third-party form (including pointer and shadow). Rows are 56 px, with
an inset hover region; the footer is 45 px. Echo uses its own globe/key artwork.
Arc visually previews the hovered username in the field without committing it.
Echo highlights the row only: copying that preview safely needs Blink's suggested
value rather than exposing a password through a DOM write on hover. Exact glyph,
font raster, large account lists and other platform presentation remain partial.

Run `bun scripts/password-frames-fixture.ts` for the local acceptance site.
The debug-only `password-store-fixture frames-seed` command admits only an
explicitly enabled disposable drive profile and seeds only the listed localhost
synthetic accounts. Rebuilding an unsigned app can require macOS Keychain access
for an existing vault; use a fresh disposable profile to test a new build without
changing an existing key's permissions. This is not a release-persistence claim.

GPUI coverage exercises same-origin fill, cross-origin explicit choice,
profile separation, keyboard and mouse selection, navigation invalidation and
cross-frame dismissal. Engine tests cover frame/document authority, bounded
registrations, wire validation, secret redaction and native Return mapping.

Accepted native run: `echo-final-v2.log`/`echo-final-v2-session.log` in the
pair directory. The fixture reports the expected unique same-origin account,
empty cross-origin fields, keyboard-selected beta and mouse-selected alpha;
three renderer fill acknowledgments succeed, CEF completes and native exit is 0.
Final full workspace suite: 1,269 browser, 266 core, 179 engine tests and all
other enabled suites pass (`login-full-tests-delivery.log` in the task build
cache); one pre-existing AI test is ignored.
