# Native video Picture-in-Picture presentation

Echo uses Chromium's video surface and native always-on-top window. The engine
passes `--echo-pip-controls`; `patches/cef154/video-pip.patch` replaces only the
presentation of that window. Other CEF embedders retain the upstream behavior.
The patch applies on every platform; the existing isolated AppKit shim supplies
macOS's 224 x 126 initial geometry and remembered position.

## Arc reference

The hover controls were measured from the owner's Arc screenshot and a matching
live Arc 1.170 frame at 542 x 305 (`www.youtube.com`, automatic PiP):

| Element | Geometry at 542 x 305 |
| --- | --- |
| Back to Tab, Minimize, Close | 28 px tall pills, 16 px from the edges, 8 px gap, 12 pt semibold, white at about 10% |
| Origin | centered on the window, 13 pt semibold, no favicon |
| Play/pause | two 12 x 70 px rounded bars, 16 px apart, centered |
| Back/forward 15 seconds | 33-36 px rings with a bold "15", 78 px from the center |
| Progress | 5 px rounded bar, 16 px from the sides, 13 px from the bottom |

The idle window shows video only.

**Earlier conclusion withdrawn:** previous sessions recorded that Arc shows no
hover controls. Those checks teleported the pointer, which does not reliably
trigger hover tracking. Do not use teleporting pointer input as evidence that a
hover state is absent. Echo's own hover qualification glides the pointer with
real `mouseMoved` events.

## Behavior and limits

Hovering fades in the controls and a dimming scrim through Chromium's existing
animation; leaving the window fades them out. They stay visible while the pointer
rests inside. The controls are painted from fixed vector geometry, not stock icons
or font glyphs. The rest of the window remains a drag and resize area.

- Back to Tab keeps Chromium's localized accessible name, which the macOS return
  bridge matches to select the retained source tab.
- Minimize closes the window and leaves playback running. Close also pauses.
- Back/forward seek 15 seconds, as do the Left/Right keys. Space toggles playback.
- Clicking or dragging the progress bar seeks.
- Video calls keep Chromium's own call controls.

These choices are not verified against Arc and should be compared when possible:

- The scrim strength (black at 45%) and the hovered/pressed pill brightness.
- The paused-state play glyph, for which no Arc frame exists.
- Below 440 px wide, pills drop their labels; the origin hides when it no longer
  fits. Center controls scale with height down to 45%. Arc's small-window layout
  was not captured.
- What Arc's Minimize does beyond closing the window.

Arc's floating-window context menu and drag-up seeking remain unimplemented; do
not read this patch as completing section 17. Stock downloaded CEF ignores the
switch and keeps its own toolbar. The sidebar hover header retains the two-window
PiP icon for audio-only sources and requests PiP only for a retained video source.

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

Historical: this section qualified the withdrawn video-only presentation. Its
framework hash and hover-frame conclusions no longer describe the current patch.

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
