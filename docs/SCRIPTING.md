# Scripting

Echo's macOS `Echo.sdef` uses the class, property and command codes in the
installed Arc dictionary. It is installed by the bundle crate, with
`NSAppleScriptEnabled` and `OSAScriptingDefinition` registration.

```applescript
tell application "Echo"
    tell front window
        set pageTab to make new tab with properties {URL:"https://example.com/"}
        select pageTab
    end tell
    get URL of pageTab
    -- First enable View → Developer → Allow JavaScript from Apple Events.
    execute pageTab javascript "document.title"
end tell
```

The portable model is `core::scripting`: typed commands, window/tab/Space
references, snapshots and object resolution. The browser dispatcher applies edits
to canonical profile data. The macOS transport only converts descriptors and
suspends/resumes AppleEvents; it does not access GPUI or CEF from a native event
callback. This model can support another transport later; no CLI or IPC server
is exposed by this change.

Implemented surfaces:

- Window/tab/Space reads, properties records, IDs, names, indexes, all/first/last/
  middle/negative-index and named/ID selections; active tab and Space, private
  mode, loading and `topApp`/`pinned`/`unpinned` location.
- New normal/incognito windows; tabs in a window or Space, and the application's
  write-only tab collection for Little Echo. URL and tab location setters.
- Select, close, Space focus, back/forward, reload/stop and JavaScript execution.
- JavaScript defaults off and requires the Developer-menu opt-in. Evaluation uses
  the existing page's local CEF DevTools channel; there is no debugging port or
  inspector window. Results and exceptions complete an owned request, while
  navigation, closure, timeout and shutdown cancel it. Delivery rechecks the
  canonical profile, page identity, current URL and opt-in, rejecting stale replies. Permission and preference
  state remain local.

The locked-session expansion adds typed URL/location creation properties (validated
before mutation), plural select/close/navigation/location operations using stable
references, lossless signed 64-bit AppleEvent integers and browser-only window-index
writes. Core tests cover raw index/name/ID selectors, all dictionary window properties,
command codes, invalid creation values and ordering anchors; GPUI tests cover creation
location and plural closure. Execute results remain text for every JavaScript value, as
declared by Arc.sdef; the engine uses JavaScript String conversion and preserves
exceptions as errors. The unlocked native probe now passes these
selector/creation/window-order and JavaScript text-result categories; it also exposed
and fixed `count properties` (record counting rather than child enumeration). See
[native validation](NATIVE_UNLOCKED_VALIDATION.md). Bulk mutations and signed 64-bit
transport retain their GPUI/unit coverage; their broader native edge cases remain
partial.

Compatibility limits: range/predicate/relative object selectors and beginning/relative
insertion locations are not implemented (rejected explicitly). Window mode is fixed at creation. An
unloaded tab has no live JavaScript target; scripts should select/load it before
execution. Window-state setters and normal/Little creation need broader native
coverage. macOS AppleEvent consent for an external controller is owned by the OS.

Native validation used a copied bundle with a unique identifier and separate
profile, running real AppleScript inside that process. This prevents a controller
from selecting the owner's Echo instance and avoids asking for another app's
Automation access. The reference Arc sessions only used bounded System Events,
the shared guard and Echo Test; no `tell application "Arc"` was issued.

Evidence: `target/parity/scripting/native-1/{read,eval}.applescript`, `app.log`,
`echo-script.png`, and `target/parity/scripting/native-scoped2.log`. The successful run
covered object references/properties, reads/counts, tab creation/selection,
URL changes, navigation, pin/unpin, denial before opt-in, JavaScript text and
exception results, incognito creation, and tab/window closure, ending with CEF
shutdown and process status 0. Unit coverage includes descriptor ABI and object
record flags, cross-window identity isolation, stale references, canonical
mutations, default-deny persistence and late evaluation replies after navigation.
