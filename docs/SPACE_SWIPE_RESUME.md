# Space swipe verification state

See [motion measurements](SPACE_SWIPE.md) for the implementation, recorded
reference fits and remaining gaps. Evidence is local under
`target/parity/space-swipe/`; it is not bundled with Echo.

Arc input policy: keyboard/dock recordings only. Never send synthetic gesture
events or `tell application "Arc"`. Use the shared
`~/Documents/Bot/parity-evidence/tools/arc_guard.py` window guard under
`/tmp/echo-screen.lock`. Each session must create, claim and close its own
window. The owner's real Spaces must never be modified or deleted.
The test Spaces are `Space Swipe QA 20261024 A` and `B`; the Spaces menu
truncates their names, so verify the full header in screenshots before input.

Reference accessibility became blocked again during resumed dock recording.
Report blockers to the orchestrator; do not call the owner again. Existing
recordings contain sufficient spring, gain and halfway-threshold evidence,
but do not establish exact edge or flick-boundary physics.
