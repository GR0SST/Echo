# CEF allocator startup incident

## Evidence

The retained native run `target/parity/arc-max/drive-max-colors` trapped before
its first automation command. The saved report is
`~/Library/Logs/DiagnosticReports/Echo-2026-10-25-030839.ips`.

- CEF 154.0.34.0, arm64 UUID `4C4C448E-5555-3144-A180-7DB8F7267284`.
- Faulting queue: `com.apple.NSXPCConnection.user.endpoint`.
- Stack: CEF offset `0x2169138`, `_dispatch_dispose`, `dispatch_release`,
  `_dispatch_mach_msg_invoke`.
- The main thread was running dyld framework initializers.
- The exported `ChromeWebAppShortcutCopierMain` label is only the nearest
  stripped export, with a large offset; it does not identify the crashing code.

The preserved framework UUID matches the report. Disassembly at `0x2169054`
reads all malloc zones, tries each zone's size callback, and traps at
`0x2169138` when none claims the allocation. The call at `0x21690a0` goes through
`0xa1683f0` → `0xba9c97c` → GOT `0xcf2df38`, bound to
`malloc_get_all_zones`. This matches Chromium's
[TryFreeDefaultFallbackToFindZoneAndFree](https://chromium.googlesource.com/chromium/src/%2B/150979b30b232291e2efc8de7114caadda909920/base/allocator/partition_allocator/src/partition_alloc/shim/allocator_shim_apple.cc).

Chromium explicitly documents a registration race: while its default malloc
zone is being replaced, another thread can free memory while the original
zone is temporarily absent. See
[allocator_shim_override_apple_default_zone.h](https://github.com/chromium/chromium/blob/main/base/allocator/partition_allocator/src/partition_alloc/shim/allocator_shim_override_apple_default_zone.h).
The observed initializer/free overlap strongly supports this diagnosis. The
precise timing has not been reproduced: the old binary launched and quit
successfully three times with separate copies of the preserved profile.
The original profile remains unchanged. This does not establish a profile bug.

## Mitigation

Echo now loads CEF before `Application::new`, on the main thread, following
[CEF's macOS sample startup order](https://github.com/chromiumembedded/cef/blob/master/tests/cefclient/cefclient_mac.mm).
CEF initialization, profiles and pages still start through the existing engine
runtime. A process-lifetime framework guard retains its allocator code; runtime
teardown cannot unload it while a dispatch worker may still free an allocation.
Background-thread preload attempts fail before loading the framework. A missing
framework still permits the existing unbundled UI-only development executable;
an incomplete bundled framework fails before application creation.

## Regression checks

`framework_preload_rejects_a_worker_before_touching_the_allocator` covers the
thread guard. `absent_framework_keeps_unbundled_ui_available` covers framework
path detection. `scripts/check-cef-startup.py TEST_PROFILE` acquires the shared
screen lock, copies the explicit test profile, and launches/quits three isolated
bundles. Each run must log framework preload before application hooks and exit
cleanly through the CEF shutdown barrier. Its artifacts are saved under
`target/parity/arc-max/startup-guard/`.

Local validation: the full suite passed (1,090 browser tests, 241 core tests,
153 engine tests, plus the other workspace suites). Both new framework tests
passed. The native guard ran three copied-profile launches, all exit 0, with
preload before application hooks; each restored a live public Wikipedia page.
The final Max screenshot session also exited 0. An earlier screenshot harness
had only a 25-second total deadline and expired during cookie flushing after
capture; the bounded 60-second rerun completed through the normal barrier.
Evidence: `target/parity/arc-max/startup-guard/observations.json`, its three
`launch-*.log` files, and `echo-max-round2-native.log`.
 Repeated successful launches validate startup ordering and shutdown;
they do not prove elimination of an intermittent allocator race.
