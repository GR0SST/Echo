# Pointer lock — verified reference and engine blocker

Arc's guarded disposable `Echo F27 Fixture` in the shared Echo Test Space acquired
real `document.pointerLockElement` after a user Space-key activation and released
it on Escape. Its exact toast is `Your cursor is hidden. Press 'esc' to show
cursor.` The ordinary toast layout is reused; no replacement copy is proposed.
The tab was archived and the one owned window closed, preserving the baseline.
No owner profile, permission, password or preference was changed.

Evidence under `target/parity/partial-closeout/`:

- `arc-f27-entered.png`, `arc-f27-toast-2x.png`, `arc-f27-entered-ax.txt`,
  `arc-f27-receipt.json` and `arc-f27-reference.log` establish the live reference,
  true/false pointer-lock transitions, exact copy and cleanup.
- `f27-diagnostic-fixed.log` records an actual fixture click followed by
  `UnknownError: If you see this error we have a bug. Please report this bug to
  chromium.` The synthetic page logs `/clicked` before the rejection; this is
  not a toast-only or missing-user-activation assumption. The initial baseline
  in `f27-baseline.log` also failed to acquire a lock. Neither is passed Echo
  pointer-lock acceptance.

Echo embeds CEF 154 with `RuntimeStyle::ALLOY`. In the canonical source checkout,
`cef/libcef/browser/alloy/alloy_browser_host_impl.{h,cc}` has no
`RequestPointerLock` override. The inherited Chromium implementation in
`content/public/browser/web_contents_delegate.cc` immediately responds with
`blink::mojom::PointerLockResult::kUnknownError`, matching the native failure.
CEF's permission prompt type includes pointer lock, but this Alloy path never
reaches that prompt: changing Rust's permission mask handling alone cannot make
the native request succeed.

A real completion needs an Alloy engine hook that preserves Chromium's requesting
widget/document, focus, sandbox, gesture/relock and cancellation checks, then the
Rust document-scoped notification and release handling. It must pass fresh
native acquisition/Escape, focus-loss/navigation/close and profile/frame checks.
No emulated cursor hiding, synthetic movement, fake Pointer Lock API or success
toast is substituted. The canonical CEF publisher owns any engine stack rebuild
and qualification; no shared CEF source, patch or generation was changed here.

Rank 27 remains functional and missing; its effort is L rather than a standalone
M toast task. The positive Arc reference is now available, and the precise engine
integration blocker is known.
