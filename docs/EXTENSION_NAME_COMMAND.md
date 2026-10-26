# Extension commands — stale inventory verification

Rank 38's missing note predates the existing GPUI/CEF integration. No replacement
UI or action bridge is needed. The Command Bar already matches an enabled
extension by `Extension NAME`, presents its icon chip and invokes its native action
using a captured extension ID and website profile. It does not navigate the owner
page. Disabled catalog entries are filtered; invocation rechecks both current
profile and enabled state instead of retargeting a captured suggestion.

## Reference and native acceptance

The already accepted Arc references in [EXTENSIONS_UI.md](EXTENSIONS_UI.md) show
both the `Extension …` result and its action window. This verification reuses that
unchanged layout; it does not install, enable or modify any owner Arc extension.
A local copy of the source capture is
`target/parity/partial-closeout/arc-f38-existing-command-reference.png`.

A fresh disposable Echo profile loaded an unpacked test-only MV3 extension through
the existing debug acceptance loader. Real Command-T, clipboard paste and Enter
selected `Extension Echo Command Probe`. Its actual native action queried its
active owner tab and reported exactly the retained fixture URL to a loopback
server. Command-W closed only that action; the original page stayed selected and
the process completed CEF shutdown with strict exit status zero.

Evidence under `target/parity/partial-closeout/`:

- `pair-f38-command.png`: existing Arc / fresh Echo result layouts; catalog
  contents intentionally differ.
- `echo-f38-action.png`, `echo-f38-after-close.png`: real action and retained owner.
- `f38-native-receipt.json`, `f38-native-r2.log`: action callback URL, keyboard
  entry, close and strict zero exit.
- `echo-fullpage-r4-2rn2wwzu/stream.log`: fixture install and native shutdown.

The first exploratory probe rejected the fixture's small content-sized action
because the harness expected Dark Reader's dimensions. It sent no input through
that unknown surface. The final guard binds the action to the observed invocation,
owned process and browser-relative position while allowing its own content size.

Existing GPUI coverage includes
`extension_command_bar_captures_profile_and_stale_profile_actions_are_rejected`
and the general Command Bar selection/activation tests. Full workspace tests
remain green; this change corrects documentation and adds native acceptance
without changing production code or introducing redundant tests.
