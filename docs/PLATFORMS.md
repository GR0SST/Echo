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
| CEF engine child-window hosting | NSView implementation | Win32 implementation; runtime unverified | X11/XCB implementation; runtime unverified; no Wayland hosting |
| GPUI parent handle passed to CEF | AppKit bridge | **Missing**; currently returns `None` | **Missing**; currently returns `None` |
| Native overlays, click-through, traffic lights, material backdrops | AppKit implementation | No-op / absent; opaque GPUI fallback | No-op / absent; opaque GPUI fallback |
| Default-browser registration | LaunchServices / system consent | Unavailable; directs user to OS settings | Unavailable; directs user to OS settings |
| Imported website password storage | Local Keychain | Unsupported; no plaintext fallback | Unsupported; no plaintext fallback |
| AI provider credential storage | Keychain via keyring | Credential Manager via keyring; runtime unverified | Secret Service via keyring; requires D-Bus service; runtime unverified |
| AppleScript application bridge | Implemented | No native bridge | No native bridge |
| Debug `BROWSER_SCRIPT` native automation | Implemented | Not compiled | Not compiled |
| Reorder haptics | NSHapticFeedbackManager | No-op | No-op |
| Reduced-motion OS preference | Queried | Stub returns false | Stub returns false |
| PiP video request | Shared CEF/DOM path | Shared path; runtime blocked by parent bridge | Shared path; runtime blocked by parent bridge |
| PiP geometry, rounded corners, return events | Native Arc-sized window shim | No geometry/event shim; Chromium defaults unverified | No geometry/event shim; Chromium defaults unverified |
| Native precise page-swipe input | Cocoa bridge | Missing native input bridge; shared physics present | Missing native input bridge; shared physics present |
| Share sheet and native certificate viewer | Implemented | Unsupported / hidden | Unsupported / hidden |
| Hide app / hide other apps | Implemented | Unsupported / disabled | Unsupported / disabled |
| Little Arc global shortcut | global-hotkey backend | Backend present; runtime unverified | Backend present; X11/session requirements; runtime unverified |
| Audio output | CPAL/CoreAudio | CPAL/WASAPI source path; runtime unverified | CPAL/ALSA source path; runtime unverified |
| App packaging | `.app` bundle utility | Utility exits unsupported; installer absent | Utility exits unsupported; packaging absent |

The parent-handle bridge and overlay/input integrations must be implemented and
checked on native hosts before claiming Windows/Linux browser parity. The
macOS-only bundle implementation is isolated in `crates/bundle/src/macos.rs`.
