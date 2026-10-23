# AGENTS.md

Rules for every agent (Claude, Codex and others) working in this repository.
`CLAUDE.md` is a symlink to this file; edit only `AGENTS.md`.

## Goal

The Browser Company is shutting down the Arc browser on December 22, 2026.
Our task: **by 22.12.2026, at minimum, ship a complete copy of Arc**.
Anything beyond that (our own features, departures from Arc) comes after.

- Arc is the reference for features and behavior. When unsure how something
  should work, do what Arc does. That includes the feel: gestures,
  animations, physics (e.g. a swipe on the sidebar drags Spaces along with
  the fingers instead of switching with a jump).
- Match Arc 1:1 in behavior and visual presentation, including menus,
  dropdowns, sidebar items, spacing, and interaction states. Inspect the
  installed Arc app/reference before implementing a UI flow; do not invent
  replacement pickers or controls. Functional similarity alone is not parity.
- Split View parity must include a new empty pane with the Command Bar open,
  closing only the focused pane with Cmd+W, Arc's grouped sidebar presentation,
  and retaining original Favorites/Pinned items when Arc duplicates them into
  a split. Verify these flows in Arc and keep regression coverage.
- Parity with Arc by the deadline comes first. Don't spend time on features
  Arc doesn't have until parity is reached.
- Key Arc features we must have: vertical sidebar with tabs, Spaces and
  profiles, Pinned and Favorites tabs, tab auto-archiving, Split View,
  Little Arc, Command Bar, Peek, Boosts, Easels, Library, Air Traffic
  Control, Picture-in-Picture, Space themes and colors, sync.
- The full list of differences from Arc (built from the Arc 1.169.0 bundle)
  is `docs/ARC_PARITY.md`, with statuses done / partial / missing. Take tasks
  from there and update the row's status in the same change that closes it.

## Platforms and language

- The browser is **cross-platform**: macOS, Windows, Linux. No macOS-only
  code in our own logic.
- Our code is written **in Rust**. No Swift/Objective-C (they tie us to the
  Mac), except unavoidable platform shims, kept minimal and isolated.
- Isolate platform-specific code behind shared interfaces so the rest of
  the logic builds on every platform.
- Everything in the repository is in English: docs, comments, commit
  messages, UI strings.

## Architecture

- **The whole browser UI is built with [GPUI](https://www.gpui.rs/)** (Zed's
  UI framework, crate `gpui`): sidebar, Spaces, Command Bar, Split View,
  Little Arc, settings, everything. Chromium's UI (Views, WebUI pages such
  as settings) is not used or extended for the browser's interface.
- **Chromium is only the engine** that renders web pages. It is embedded in
  the GPUI window through CEF (Chromium Embedded Framework, crate `cef`).
- The Rust app is a Cargo workspace at the repository root (`Cargo.toml`,
  `crates/`):
  - `crates/browser`: the GPUI app (windows, views, input handling);
  - `crates/core`: the browser model without UI or engine (Spaces, tabs,
    pinning, archiving, sessions), covered by unit tests;
  - `crates/engine`: the CEF wrapper. The rest of the code talks to the
    engine only through it, so the engine can be replaced. It also holds
    the `browser_helper` binary, Chromium's subprocesses;
  - `crates/bundle`: builds the `.app` (on macOS CEF only runs from a bundle).
- Chromium pages are native views on top of the GPUI window, so GPUI can't
  draw over them in the same window. UI that must lie over a page (Command
  Bar etc.) is a separate transparent child window (`Overlay` in
  `window.rs`). Browser shortcuts pressed inside a page are intercepted by
  the engine (`WINDOW_SHORTCUTS` in `window.rs` is the single list).
- `Engine::pump()` (CEF) may process AppKit events inside itself: don't hold
  a `borrow` of window or page state while pumping, or it re-enters. It also
  leaves queued events to the toolkit's loop, otherwise AppKit's mouse
  tracking (window buttons, resizing) breaks.
- Edit `src/` (the Chromium fork) only for changes to the engine itself that
  CEF can't provide. Prebuilt CEF lags behind the fork (CEF 154 vs Chromium
  158 in `src/`), so engine patches are later ported to the Chromium branch
  CEF supports.

## Repository layout

See `README.md`. In short:

- `src/`: Chromium sources, fork `https://github.com/GR0SST/chromium`,
  working branch `browser/main`; `upstream` is official Chromium.
- `depot_tools/`: official Chromium tools, do not edit.
- `config/args.gn`: build settings; `scripts/`: command wrappers.
- Commands: `bun run sync`, `bun run build`, `bun run browser`.

## Working rules

- Tests: `cargo test`. App with the engine: `cargo run -p bundle`, then
  `open target/bundle/browser.app` (`cargo run -p browser` is UI only,
  pages are placeholders). CEF binaries live in `third_party/cef`
  (`.cargo/config.toml`) and are downloaded automatically.
- Dev builds run Chromium with `--use-mock-keychain`, otherwise macOS asks
  for the keychain password after every rebuild.
- After the tests, check the change in the real app:
  `scripts/drive.sh "<script>"` runs a copy of the bundle with a separate
  profile and executes a script from `crates/browser/src/automation.rs`
  (`key cmd-t; type text; click x y [hold_ms]; drag x1 y1 x2 y2;
  swipe x y dx [hold]; shot file.png; shotstack file.png; views; windows;
  dump; quit`). Look at the screenshots yourself.
  Input is sent as real `NSEvent`s, so it catches focus and key routing
  bugs that GPUI tests miss. Window buttons and resizing need a held click
  (`click 20 20 300`) or `drag`: that is how AppKit's mouse tracking loops
  work, as with a real hand.
  If `windows` shows `onScreen=false`, the window is covered by other
  windows and macOS doesn't redraw it: screenshots and `dump` go stale,
  that is not a bug.
- Cover any UI behavior (shortcuts, clicks, text input, focus) with a test
  in `crates/browser/src/tests.rs`. Such a test opens a real window on
  GPUI's test platform and acts like a user: `simulate_keystrokes`,
  `simulate_input`, clicking a `debug_selector` after hovering it. Don't
  report "done" until `cargo test` is green.
- The Command Bar (⌘T) is the main way to open a page, as in Arc:
  ⌘T doesn't create an empty tab, it opens a popup input.
- Our code (everything except `src/` and `depot_tools/`) lives in
  `https://github.com/GR0SST/browser`, branch `main`. Commit and push each
  finished change (`cargo test` green, checked in the app) straight to
  `main` as a separate small commit, so it can be rolled back if needed.
  Don't let uncommitted work pile up.
- Changes in `src/` go as separate meaningful commits on `browser/main`,
  so they are easy to carry over when updating Chromium.
- Keep edits to existing Chromium code minimal: put new logic in the Rust
  crates and leave only thin integration points in Chromium. That makes
  rebasing onto a new upstream easier.
- Keep LICENSE files and third-party notices.
- Don't use Arc / The Browser Company trademarks, logos or assets: we copy
  the functionality and UX, not the brand.
