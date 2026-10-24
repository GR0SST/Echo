# Echo

Echo is a browser in development that aims to recreate Arc's interface and
browsing experience. It uses Rust and [GPUI](https://www.gpui.rs/) for
the interface, with Chromium embedded through
[CEF](https://bitbucket.org/chromiumembedded/cef) to render web pages.

Echo is local-only: there is no Echo account, unified browser login or
cross-device sync. Profiles, sessions and browsing data stay on your device;
you can still sign in to websites normally.

Echo is independent of The Browser Company.

## Current status

Echo is an early development build, not a complete replacement for Arc.
Existing functionality includes a vertical sidebar, Pinned and Today tabs,
Spaces, a Command Bar, tab auto-archiving and saved sessions. Features and
interaction details are still being developed. The
[Arc comparison](docs/ARC_PARITY.md) tracks what is done, partial or missing.

The current app bundle and development instructions target **macOS**.
Windows and Linux are intended targets, but working builds on those platforms
are not established by the current setup.

## Build and run on macOS

You need Git, a current Rust toolchain with Cargo, Apple's Xcode Command Line
Tools, and an internet connection for dependencies and the initial CEF download.
Install the command-line tools with `xcode-select --install` if needed.

```sh
git clone https://github.com/GR0SST/Echo
cd Echo
cargo run -p bundle
open target/bundle/Echo.app
```

The first build downloads CEF binaries into `third_party/cef` and may take a
while. Building Echo with these prebuilt binaries does not require a Chromium
source checkout.

Press **⌘T** to open the Command Bar and enter a URL or search query.
**⌘L** opens it for the current page.

## Contributing

Source and issue tracking: [GR0SST/Echo](https://github.com/GR0SST/Echo).
See the [Arc comparison](docs/ARC_PARITY.md) for known gaps and
[reference notes](docs/ARC_REFERENCE.md) for observed behavior.
[AGENTS.md](AGENTS.md) contains the repository's implementation and testing
guidance.

```sh
cargo test                 # unit and UI tests
cargo run -p browser       # interface only; pages are placeholders
```

The Rust workspace is organized as follows:

| Directory | Purpose |
|---|---|
| [crates/browser](crates/browser) | GPUI interface and input handling |
| [crates/core](crates/core) | Tabs, Spaces, archiving and session model |
| [crates/engine](crates/engine) | CEF integration and browser subprocess helper |
| [crates/bundle](crates/bundle) | macOS app packaging |

### Optional: build the Chromium fork

Engine work can require the separate
[GR0SST/chromium](https://github.com/GR0SST/chromium) repository on branch
`browser/main`. The helpers expect that checkout in `src/` and Chromium's
`depot_tools/` beside it. Neither is included when cloning Echo.

Follow Chromium's
[macOS build instructions](https://chromium.googlesource.com/chromium/src/+/main/docs/mac_build_instructions.md)
for checkout prerequisites. With the checkout and tools prepared, use
[Bun](https://bun.sh/) to run the repository scripts:

```sh
bun run sync               # fetch Chromium dependencies and run hooks
bun run configure          # generate build files; also run by build
bun run build              # build Chromium in src/out/Default
bun run browser            # launch that Chromium build, not Echo
```

The supplied [build configuration](config/args.gn) targets Apple Silicon.
These commands build the standalone Chromium fork; they do not replace the
prebuilt CEF used by Echo.

The separate [CEF154 download experiment](docs/CEF154_DOWNLOAD_EXPERIMENT.md)
documents the pinned engine patches and their validation boundary.

## License

This repository does not currently include a top-level license for Echo's
own code. Chromium, CEF and other dependencies retain their own licenses;
their license files and third-party notices must accompany redistribution.
Bundled Ionicons include their [license](crates/browser/assets/ionicons/LICENSE).
