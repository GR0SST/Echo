# Native quit confirmation policy

The existing Arc-qualified warning card and copy remain the visual reference:
`target/parity/partial-closeout/arc-quit-warning-settled.png`,
`pair-quit-confirmation-2x.png` and the Round 2 quit section of
`PROFILES_SETTINGS_REFERENCE.md`. No source quit was accepted, no owner window
was closed, and no Arc preference/profile was changed for this extension.
Arc's Dock/OS/no-window policy is **reference unqualified**; the existing
`Warn before quitting` preference and card are reused for these quit routes.

Previously native termination callbacks sealed shutdown immediately and the
coordinator bypassed the preference. They now capture the original intent and
suspended AppleEvents without sealing, then ask the same confirmation policy.
Only acceptance seals shutdown and enters the existing CEF barrier. The
original UPP/refcon, sender and exact-once native continuation remain intact.
Warnings disabled in the install preference continue directly to that barrier.

Cancel snapshots and clears that intent synchronously before any subsequent
quit can become its replacement. Its native resumes run outside GPUI borrows,
reply `errUserCanceled` (-128), and never redispatch the original quit handler.
The installed macOS SDK's `AEInteraction.h` defines `kAENoDispatch` as zero for
an already handled event; the Rust ABI therefore uses `None` for this resume.
Failed resumes retain their context and mark a sticky failure. Cancellation
cannot clear a failure or reverse accepted/complete shutdown. New quit requests
can become a fresh first intent after successful cancellation.

When there is no browser window, confirmation reads the retained normal install
profile. It reuses the same 450 × 248 card in a centered standalone window.
Private/Blank windows still use the normal install preference, and Settings
still selects its browser owner. No browser window is created to quit.

Regressions cover joined native intents, duplicate-token rejection, cancellation
and retry, sticky failure, accepted-barrier rejection and Cancel/Accept with no
browser owner. Both foreign workspace compile checks pass in
`target/parity/partial-closeout/quit-targets-r4.log`.

Native qualification uses only disposable Echo profiles and the frozen
`quit-qualified-r4.app`. The OS sender targets its exact running PID through
NSRunningApplication; other apps and the owner's Arc windows are not quit.
All three native cases passed with strict exit 0: `quit-native-window-r4.log`,
`quit-native-no-window-settled-r4.log` and `quit-native-disabled-r4.log` in the
same evidence directory. Their JSON receipts bind the disposable fixture and
mode. Repeated OS events joined one warning; Cancel produced no shutdown;
a later intent accepted and exited through the preserved original handler.
The no-window case canceled OS quit, then accepted Command-Q without recreating
a browser window. Warning-disabled OS quit continued directly to the barrier.
The final no-window native frame is exactly 450 × 248; earlier rejected probes
sampled its entrance animation before it settled. No additional animation or
pixel changes were made.

Reviewed `pair-quit-native-r4-2x.png` reuses the existing source card beside the
new no-window Echo card; it qualifies that component, **not** Arc's OS/no-window
policy. Native window captures are `echo-quit-window-r4.png` and
`echo-quit-no-window-r4.png`. `quit-policy-full-r4.log` passed 1,361 browser,
284 core and 203 engine tests, plus all remaining workspace suites.

The General warning row remains partial: Arc's OS/no-window policy is reference
unqualified, other auxiliary-host routes require qualification, and Light
appearance and motion remain cosmetic.
