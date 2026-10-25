# Scripting

`core::scripting` defines portable typed commands, window/tab/Space references,
snapshots and object resolution using the installed Arc dictionary codes. It
handles IDs, names and index selections without depending on macOS or CEF.
The JavaScript AppleEvent opt-in is persisted locally and defaults off.

The native transport and browser dispatcher are integration work.

The engine provides asynchronous, owned JavaScript evaluation through its local
CEF DevTools channel. Results and exceptions are bounded; page navigation,
closure, URL changes and timeouts cancel pending work. No remote debugging port
or inspector frontend is opened.
