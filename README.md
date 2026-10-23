# Echo

The Browser Company is shutting down the Arc browser on December 22, 2026.
This project is a cross-platform browser that recreates Arc one to one:
its features, behavior and feel.

**Deadline: December 22, 2026.** Until then the only goal is parity with Arc.
New features of our own come after that.

Not affiliated with The Browser Company. We copy Arc's functionality and UX,
not its name, logos or assets.

## Status

What Arc has and what we already have is tracked in
[`docs/ARC_PARITY.md`](docs/ARC_PARITY.md), built from the
Arc 1.169.0 app bundle. Each item is marked done, partial or missing.

Working so far: the vertical sidebar with Pinned and Today tabs, Spaces that
follow two-finger swipes, the Command Bar (⌘T / ⌘L), auto-archiving,
Arc's keyboard shortcuts for tabs and Spaces, and sessions that survive
a restart.

## Stack

- **Rust** for all our code, so the browser runs on macOS, Windows and Linux.
  Platform-specific code stays small and behind shared interfaces.
- **[GPUI](https://www.gpui.rs/)** (the UI framework behind Zed) for the
  whole interface: sidebar, Spaces, Command Bar, settings, everything.
- **Chromium** only as the engine that renders web pages, embedded through
  [CEF](https://bitbucket.org/chromiumembedded/cef) (the `cef` crate).
  Chromium's own UI is not used.

## Layout

| Path | What |
|---|---|
| `crates/browser` | GPUI app: windows, views, input |
| `crates/core` | Browser model without UI or engine: Spaces, tabs, pinning, archiving, sessions |
| `crates/engine` | CEF wrapper; the rest of the code talks to the engine only through it. Also `browser_helper`, Chromium's subprocesses |
| `crates/bundle` | Builds the macOS `.app` (CEF only runs from a bundle there) |
| `docs/` | Parity list and other notes |
| `scripts/` | Automation driver and Chromium checkout helpers |
| `src/` | Chromium fork ([GR0SST/chromium](https://github.com/GR0SST/chromium), branch `browser/main`), a separate repository, only for engine changes CEF can't do |

## Development

```sh
cargo test                       # unit and UI tests
cargo run -p bundle              # build target/bundle/Echo.app
open target/bundle/Echo.app      # run with the engine
cargo run -p browser             # UI only; pages are placeholders
```

CEF binaries are downloaded to `third_party/cef` on the first build.

`scripts/drive.sh "<script>"` runs a copy of the app with a separate profile
and replays real input, e.g. `scripts/drive.sh "key cmd-t; type example.com;
key enter; wait 3; shot /tmp/a.png; quit"`. Commands are listed in
`crates/browser/src/automation.rs`.

For focus-sensitive checks, start the script with `activate; require-focus;`.
The driver then stops before the next input command (and between typed characters)
if another app takes focus or the browser becomes occluded. Command Bar and
menu child windows count as browser focus. An aborted run exits with status 1
and must be repeated before using its screenshots as evidence.
`windows`, `window`, `reopen` and `quit` remain available after deliberately closing the
window, so a guarded script can inspect closure and test Dock restoration.
`window <index>` or `window last` binds subsequent commands to another browser
root without changing focus; use `activate` explicitly when needed. Child
panels are excluded from this index.

For native page-lifetime checks, `transfer-new-host` moves the loaded standalone
page into a temporary GPUI host. `transfer-back` restores the original host and
closes the temporary window. Use `shotstack` to capture the temporary host.
Transfers wait for the source viewport capture before reparenting. `snapshot
<file.png>` saves the active page's PNG through the local CEF capture API.
`position <x> <y>` places the current test window in native screen coordinates
without moving the pointer; `shotall <prefix>` captures every app window.

`menu Edit > Undo` invokes an enabled native menu item without moving the
system pointer. Keyboard events follow the focused browser child window,
including the Command Bar; point coordinates remain relative to the main window.

Rules for contributors and coding agents are in [`AGENTS.md`](AGENTS.md).
In short: match Arc, write Rust, build the UI in GPUI, cover
UI behavior with tests, and commit each finished change to `main`.

### Chromium fork

Only needed for engine patches. The Chromium checkout lives in `src/` with
`depot_tools/` next to it, both ignored by this repository:

```sh
bun run sync     # fetch Chromium dependencies (slow)
bun run build    # GN + Ninja build of src/out/Default
```

See the official
[macOS build instructions](https://chromium.googlesource.com/chromium/src/+/main/docs/mac_build_instructions.md).

## License

Chromium and CEF are distributed under their own licenses; keep their
LICENSE files and third-party notices when redistributing.
