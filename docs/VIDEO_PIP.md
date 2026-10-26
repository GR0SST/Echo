# Native video Picture-in-Picture presentation

Echo uses Chromium's video surface and native always-on-top window. The engine
passes `--echo-video-only-pip`; `patches/cef154/video-pip.patch` changes only the
presentation of that window. Other CEF embedders retain the upstream behavior.
The patch applies on every platform; the existing isolated AppKit shim supplies
macOS's measured 224 × 126 initial geometry and remembered position.

## Arc reference

A guarded Arc 1.170 session in the shared **Echo Test** Space played the public
Big Buck Bunny clip and switched to another tab, opening automatic PiP. A bounded
five-second screen recording moved the pointer from the center to the top-left
of its 224 × 126 floating window. Every recorded frame showed video only:
no origin, favicon, gradient, timeline or buttons. The PiP accessibility window
also exposed no buttons. This agrees with audit `new06-pip`.

Public source (cached locally for repeatable native playback):
https://upload.wikimedia.org/wikipedia/commons/transcoded/c/c0/Big_Buck_Bunny_4K.webm/Big_Buck_Bunny_4K.webm.240p.vp9.webm

Reference evidence in the media agent's worktree:
`target/parity/media-pip/arc-pip-hover-chrome-r4.mov`,
`arc-pip-chrome-r4-filmstrip.png`, `arc-pip-chrome-r4-hover.png`.
The native scripts use the shared screen lock, a 115-second hard timeout, the
Arc ownership/Space guards, and archive their opened test tabs before closing
the owned browser window.

## Behavior and limits

Pointer entry, movement and initial native focus keep the video unobstructed.
The invisible control container cannot consume clicks; Chromium's existing
window drag and resize hit testing continues to work. Keyboard input reveals
Chromium's existing controls. They stay mouse-usable until the pointer leaves
the window, preserving Echo's existing semantic return-button bridge. Leaving
the window restores the video-only presentation. This
keyboard fallback is an accessibility accommodation, not verified Arc parity.

The Developer-mode canonical candidate also removes a visibility race in this
fallback: Echo reveals controls synchronously before dispatching the key and
keeps them visible while the keyboard-reveal flag is set. Chromium's fade target
otherwise allowed photographed controls to disagree with hit testing. Pointer
exit still clears that flag. The native harness raises the owned PiP window,
verifies keyboard focus and uses normal Views clicks; its strict visibility
guard is unchanged. Two consecutive complete checks and the subsequent fresh
canonical check passed on framework
`c9d3ae396593a3f8333c1017c9f3b9c6b64d9394285c1504257461bec9092829`.

The sidebar hover header retains the two-window PiP icon for audio-only sources,
as in audit `old18-mini-pip`. It requests PiP only for a retained video source;
its page identity and document epoch guards still reject stale requests.

The observed Arc session did not reveal floating hover controls. Their exact
appearance and interaction, Arc's floating-window context menu and drag-up
seeking remain unqualified; do not interpret this patch as completing all of
section 17. Stock downloaded CEF ignores the switch and keeps its own toolbar.

## Build

`scripts/build-cef.sh` applies the patch idempotently before compilation and
packaging, under `/tmp/echo-cef-build.lock`. Use the source-built distribution:

```sh
scripts/build-cef.sh release
bun scripts/check-cef-canonical.ts
cargo run -p bundle
```

Never run two Chromium builds concurrently or replace a distribution while
another agent is compiling Rust against it. The original PiP qualification used a separate
`~/Documents/Bot/cef-src/echo-pip-final-dist` adapter and `pip-final-binary-distrib` package
root. The canonical recipe in [CEF_BUILD.md](CEF_BUILD.md#canonical-distribution)
now combines every patch in `echo-extensions-dist` and reruns native PiP against
its exact framework. Superseded packages are removed after all canonical gates pass.

## Original PiP qualification

Final native framework SHA-256:
`53ff7e0695104c6dfce463216d0f4e88eed44a7582d2a52d5ca3ec9027b744bf`.
Minimal archive SHA-1: `56f7a7813db43e7dd5727bc8b6766c3c108bc26c`.
The existing Rust bundle was retained while its ABI-compatible CEF framework was
updated; no Rust interface or helper ABI changed.

The bounded native run verified automatic PiP, manual HTML video PiP re-entry,
retained 224 × 126 bounds/position, unobstructed initial and pointer-hover frames,
Space pause, Tab reveal, mouse activation of Back to Tab and Close, and a graceful
zero-status process exit. Manual re-entry on the source page also verifies that
the return control selected the retained source rather than only closing PiP.
A separate audio-only public-clip run reported real audible playback, retained
the sidebar PiP icon, and did not enter video PiP. The full serial Rust suite
passed 1,612 tests with one existing ignored test.

Inspected pairs (3× crops):

- `target/parity/media-pip/pair-pip-hover-r4-zoom.png`
- `target/parity/media-pip/pair-mini-pip-r4-zoom.png`

Native bounds, control assertions and checksums are also recorded in
`target/parity/media-pip/pip-chrome-r4-qualification.json`; the driver log is
`echo-chrome-r4.log`. Evidence is backed up outside the worktree's target tree.
Keyboard activation of Back to Tab, paused-video entry from the sidebar and
remaining mini-player glyph/theme fidelity remain unqualified.
