# Platform build health

Echo's shared Rust model and GPUI interface target macOS, Windows and Linux.
The runnable app and packaging currently target macOS. A successful foreign
`cargo check` verifies Rust types and conditional compilation; it does not
establish a working browser, native linking, GPU initialization or OS integration.

## Repeatable checks

Use a rustup-managed Cargo and rustc from the same toolchain. On machines with
Homebrew Rust, put `$HOME/.cargo/bin` before Homebrew in `PATH` first.

```sh
rustup target add x86_64-unknown-linux-gnu x86_64-pc-windows-msvc
scripts/check-targets.sh
# Or select one target, suitable for a native CI job:
scripts/check-targets.sh x86_64-unknown-linux-gnu
scripts/check-targets.sh x86_64-pc-windows-msvc
```

The script attempts every requested target and returns nonzero if any fails.
It leaves Cargo's configured job limit unchanged. It enables `engine/check-only`,
which forwards to CEF's `dox` feature: the actual target-specific CEF Rust bindings
and Echo engine code compile, while CEF's native download, CMake wrapper and
link setup are skipped. This feature is for `cargo check` only; omit it for tests,
runnable builds and distribution.

Native Linux CI needs a C/C++ compiler, pkg-config and development packages for
ALSA, fontconfig, FreeType, xkbcommon, Wayland and X11/XCB (including EGL/GL).
Native Windows CI needs Git Bash to run the script, the MSVC C++ tools and
Windows SDK. The script converts Rust's native library path for Git Bash globbing. Rust targets alone do
not supply these headers or compilers. Default dev-profile checks are intended;
GPUI's Windows release shaders require the Windows shader build tools as well.

For cross-checking from macOS:

- Install `cargo-xwin` and LLVM (`clang-cl` and LLVM tools) in `PATH`. The script
  selects `cargo-xwin check` for a foreign MSVC target when available. Its Windows
  SDK/CRT cache can be selected with `XWIN_CACHE_DIR`.
- Install Zig in `PATH` and supply `ECHO_LINUX_SYSROOT` pointing to an extracted
  **real x86_64 Linux development SDK**, including the packages above. The script
  selects Zig for target C/C++ compilation and isolates pkg-config to that SDK.
  It does not fabricate pkg-config metadata or use macOS headers for Linux.
  Alternatively configure your own target C compiler and pkg-config environment
  and leave `ECHO_LINUX_SYSROOT` unset.

CEF 154's dependency build script can download distributions for all three
platforms. For a future native linked build, use a separate cache, for example
`CEF_PATH="$PWD/third_party/cef/x86_64-unknown-linux-gnu" cargo build --workspace
--target x86_64-unknown-linux-gnu` (without `engine/check-only`). Do not point a
foreign build at the existing macOS distribution root. The check script selects
separate target cache paths automatically; `ECHO_CEF_ROOT` overrides their parent.
Foreign native CEF builds and runtime behavior still need platform CI validation.

GPUI's async tar dependency still uses xattr 0.2.3. The workspace patches that
crate locally to use Linux/Android's `ENODATA` instead of the removed libc
`ENOATTR` alias. See [the patch note](../third_party/rust/xattr/ECHO_PATCH.md);
attribute handling remains enabled and the original licenses are preserved.

## Checked configuration

On 2026-10-25, Rust 1.96.0 checked the complete workspace for
`x86_64-unknown-linux-gnu` and `x86_64-pc-windows-msvc` through
`scripts/check-targets.sh` from an Apple Silicon macOS host. Linux used Zig
0.15.2 with Debian bookworm amd64 development headers/pkg-config metadata;
Windows used cargo-xwin 0.23.1 with the downloaded Microsoft SDK/CRT and LLVM.
These are compile checks with `engine/check-only`, not foreign native-link or
runtime tests. Normal macOS builds/tests use native CEF without that feature.

## Integration coverage

“Implemented” below describes source coverage, not successful foreign runtime
validation. Unsupported integrations fail closed or have the stated no-op fallback.

| Feature / boundary | macOS | Windows | Linux |
|---|---|---|---|
| Tabs, Spaces, archive, sessions, GPUI UI | Shared Rust | Shared Rust | Shared Rust |
| CEF engine child-window hosting | NSView implementation | Win32 implementation; runtime unverified | X11/XCB implementation; runtime unverified; XWayland required on Wayland sessions |
| GPUI parent handle passed to CEF | AppKit bridge | HWND bridge; device-pixel placement/focus/visibility; runtime unverified | Xlib/XCB XID bridge; device-pixel placement/focus/visibility; runtime unverified |
| Native overlays / passive click-through | AppKit implementation | Owned HWND / transparent style; runtime unverified | X11 transient / ShapeInput; runtime unverified |
| Traffic lights / material backdrops | AppKit implementation | No-op / absent; GPUI fallback | No-op / absent; GPUI fallback |
| Default-browser registration | LaunchServices / system consent | Unavailable; directs user to OS settings | Unavailable; directs user to OS settings |
| Imported website password storage | Local Keychain | Unsupported; no plaintext fallback | Unsupported; no plaintext fallback |
| AI provider credential storage | Keychain via keyring | Credential Manager via keyring; runtime unverified | Secret Service via keyring; requires D-Bus service; runtime unverified |
| AppleScript application bridge | Implemented | No native bridge | No native bridge |
| Debug `BROWSER_SCRIPT` native automation | Implemented | Not compiled | Not compiled |
| Reorder haptics | NSHapticFeedbackManager | No-op | No-op |
| Reduced-motion OS preference | Queried | Stub returns false | Stub returns false |
| PiP video request | Shared CEF/DOM path | Shared path; parent bridge implemented, runtime unverified | Shared path; parent bridge implemented, runtime unverified |
| PiP geometry, rounded corners, return events | Native Arc-sized window shim | No geometry/event shim; Chromium defaults unverified | No geometry/event shim; Chromium defaults unverified |
| Native precise page-swipe input | Cocoa bridge | Missing native input bridge; shared physics present | Missing native input bridge; shared physics present |
| Share sheet and native certificate viewer | Implemented | Unsupported / hidden | Unsupported / hidden |
| Hide app / hide other apps | Implemented | Unsupported / disabled | Unsupported / disabled |
| Little Arc global shortcut | global-hotkey backend | Backend present; runtime unverified | Backend present; X11/session requirements; runtime unverified |
| Audio output | CPAL/CoreAudio | CPAL/WASAPI source path; runtime unverified | CPAL/ALSA source path; runtime unverified |
| App packaging | `.app` bundle utility | Utility exits unsupported; installer absent | Utility exits unsupported; packaging absent |

The parent-handle bridge and overlay/input implementations below must be
checked on native hosts before claiming Windows/Linux browser parity. The
macOS-only bundle implementation is isolated in `crates/bundle/src/macos.rs`.

## Native engine window operations

The engine's native host interface converts GPUI logical edges to device pixels
for HWND/X11 children, notifies CEF on viewport size/DPI changes, shows/hides the
actual native child and restores GPUI focus after blurring CEF. Hidden pages
reject explicit focus requests; becoming visible does not restore focus by
itself. Shared geometry/focus tests cover these policies without a native OS.
CEF keyboard callbacks preserve platform-native event arguments inside the engine
and support Windows/Linux key-down forms and Ctrl letter/digit normalization.
These operations retain the existing macOS clip/responder and keyboard behavior.

## Windowed parent bridge (runtime validation pending)

GPUI's AppKit/Win32/Xlib/XCB raw handles now cross the existing opaque native
parent interface. CEF uses `WindowInfo::set_as_child` (Alloy) on Windows and X11.
Windows requests per-monitor V2 process DPI awareness before any HWND exists
(respecting awareness already established by a manifest/host). Windows/X11 child
placement converts logical GPUI edges to rounded device pixels,
including fractional display scale and negative origins. The current parent scale
is refreshed during GPUI layout; unchanged logical frames are repositioned when
that scale changes. CEF receives resize notifications, native children are
shown/hidden, hidden pages reject explicit focus requests, and returning focus to
GPUI first blurs CEF. macOS retains its NSView clip and responder implementation.

Native Wayland `wl_surface` parenting is **not implemented**: windowed CEF's Linux
parent is an X11 XID, not a Wayland surface. Echo selects GPUI's X11 backend before
workers start and selects Chromium's `--ozone-platform=x11`. A Wayland session
therefore requires XWayland and a valid `DISPLAY`; a Wayland-only session fails
at startup with an explicit explanation. Off-screen CEF rendering is a possible
future native-Wayland implementation, not a fallback provided by this change.

Required on real Windows and Linux/X11 (and Linux/XWayland) machines:

- Build/link without `engine/check-only` against a matching target CEF distribution;
  configure the helper subprocess and resources. The macOS bundle tool does not
  yet package other platforms.
- Open/navigate pages, switch tabs, resize/split/reparent windows, minimize/restore,
  close a page and quit. Verify that hidden children neither paint nor take input.
- Move between 100%, 125%, 150% and 200% displays; verify page and split boundaries,
  viewport metrics and mouse coordinates after a DPI change, including negative
  desktop coordinates. Check Windows process DPI awareness supplied by GPUI/CEF.
- Focus the page, then Command Bar/sidebar, Escape back to the page, and repeat
  with an editable page field and a background tab. Confirm IME/text editing.

Cross-target type checking cannot validate native parenting, X server requests,
window-manager policy, graphics/compositing, process DPI awareness or focus.

## Owned GPUI overlays and keyboard callbacks

Windows overlays remain **owned top-level GPUI HWNDs**, not Win32 child
swapchains: `GWLP_HWNDPARENT` supplies the owner, native frame decorations are
removed, and Win32 keeps them above/minimized with their owner. X11 overlays use
ICCCM `WM_TRANSIENT_FOR`, a dialog window type and decoration hints. Their owner
bounds observer moves them with the browser; full-window surfaces follow both
axes, while narrow sidebar surfaces keep their logical width. Placement tracks
fractional DPI changes without activating passive overlays. GPUI `Overlay` release
removes its native tracking registration. Native movement runs outside the tracking
registry borrow because Win32 synchronously delivers move/resize callbacks; lease
identity prevents an old failed update from removing a re-entrantly replaced overlay.
This is a separate GPUI window above
the CEF child, preserving the existing engine/UI boundary.

Passive HUDs use Win32 transparent/no-activate extended styles or an empty X11
ShapeInput region. A missing Shape extension or rejected native request is logged;
there is no claim of working click-through on an untested window manager.
Windows DWM and Linux compositor transparency/shadows remain native-runtime checks.

CEF's platform-specific MSG/XEvent callback arguments stay inside the engine.
The shared `OnPreKeyEvent` path intercepts registered shortcuts and queues them
back to GPUI. Windows/Linux accept native KEYDOWN and RAWKEYDOWN, normalize Ctrl
letter/digit chords through CEF's virtual keys, and leave KEYUP/CHAR/text editing
to Chromium. macOS retains its existing RAWKEYDOWN and Command-key behavior.
Bindings retain their configured modifiers: `cmd` is GPUI's platform key
(Command on macOS, Windows/Super elsewhere); this change does not silently remap
Cmd to Ctrl or resolve those platforms' OS-reserved chords. Users may configure
Ctrl shortcuts; GPUI and CEF use the same installed binding list.

Additional native checks: open Command Bar, Find, floating sidebar and context
menus over a live page; move/resize/minimize the owner and open another app;
verify stacking, decorations, focus return, transparent pixels and passive HUD
click-through. Repeat across displays/DPI and under multiple X11 window managers
and XWayland compositors. Test default/custom Ctrl and Windows/Super bindings,
editable-field Undo/Redo, keyboard layouts, IME, Escape, and helper-process exit.
No Windows/Linux runtime execution was performed for this implementation.
