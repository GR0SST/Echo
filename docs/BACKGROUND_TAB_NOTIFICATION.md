# Background-tab notification acceptance

The release inventory incorrectly called the hidden-sidebar notification missing. Both Command Bar Command-Enter and renderer background-open events already call `show_background_tab`, which retains the current selection and captures the new tab and local profile in the toast action. The action rejects removed or reassigned targets. This change corrects the inventory rather than adding a second notification path.

The source for the exact message `New Tab Created in %@` is Arc 1.170.0's extracted ToastController string group (line 47717 in the original reference extraction); see [the existing toast reference](TOAST_REFERENCE.md#round-3-existing-action-hooks). The regular toast presentation reuses the previously qualified toast shell. Background Space creation is tracked separately.

The original pre-qualification native acceptance uses `target/parity/background-tab/check-native.py`, a disposable session, a local HTTP fixture and real NSEvents. After hiding the sidebar, Command-Enter creates a background target while the source remains selected. Clicking the visible toast selects and loads the target. The first run observed exactly one request per document and a zero-status native quit. Two repeated separate-GPU runs and an in-process-GPU run pass the UI steps but reach CEF’s teardown watchdog after `ShuttingDown`; the latter writes `echo-dirty-shutdown.json` and terminates its network helper. That original framework did not clear the clean-quit integration gate; the later canonical requalification below supersedes that shutdown result. Preserved failure logs: `target/parity/background-tab/separate-gpu-watchdog.log` `separate-gpu-watchdog-drive.log`, `in-process-watchdog.log`, and `in-process-watchdog-drive.log`. Execution traces are in `target/parity/background-tab/native/`. Repeat screenshots used stale marker files and are not accepted parity evidence; the retained local helper now clears both phase markers before a future rerun. This check does not qualify new pixel or motion parity.

Existing GPUI user-flow coverage: `background_command_bar_keeps_the_page_and_toast_reveals_the_created_tab` and `background_engine_request_keeps_focus_and_uses_the_hidden_sidebar_feedback` in `crates/browser/src/tests.rs`. No implementation change or redundant regression test is needed for the verified existing behavior.

## Canonical requalification — 2026-10-26

Three consecutive disposable, separate-GPU production-bundle runs now pass the
entire source-focus → background load → visible-toast click → target-focus flow.
Trusted renderer key events identify the selected document before and after the
click; each document loads once. Every run reaches CEF Complete, releases its
application contexts, exits zero and writes no dirty-shutdown marker. The
framework SHA-256 is `c9d3ae396593a3f8333c1017c9f3b9c6b64d9394285c1504257461bec9092829`.
This clears the former repeatable-clean-quit gate without changing toast code.

Evidence under `target/parity/partial-closeout/`:
`f87-production-repeat.log`, the three fixture-local `receipt.json` files listed
in `f87-production-receipts.json`, and the fixture-local native/driver logs.
These are fresh profiles and phase events, not the stale marker captures noted
above. The canonical backend also now covers background-tab exits in its broader
matrix; see [CEF shutdown qualification](CEF_SHUTDOWN.md).

Direct activation of Arc's hidden-sidebar notification remains reference
unqualified. The guarded source attempt encountered an additional 980 × 350
Command Bar surface and stopped rather than guessing ownership or entering it.
Its two owned fixture tabs were subsequently archived using the exact owned
window and document identity; all four owner windows were retained. Earlier
ad hoc Echo input probes also missed clicks and remain failed, unqualified
traces; the three consecutive production passes above are the accepted current
integration evidence. The parent row stays partial for its positive-reference
and presentation gates. Do not keep reporting a demonstrated current CEF
shutdown failure for this row.
