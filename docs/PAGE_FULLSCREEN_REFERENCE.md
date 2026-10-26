# Document fullscreen instruction

## Arc reference

A fresh guarded Arc window in the shared **Echo Test** Space opened a disposable
localhost fixture. A real user-gesture `requestFullscreen()` changed the document
state to true, expanded the native browser to the display, and showed
**“To exit full screen, press ⎋.”**. Exiting the document restored its original
window. The temporary tab and owned window were closed; owner windows were retained.

Evidence under `target/parity/partial-closeout/`:

- `arc-f26-entered.png`, `arc-f26-entered-ax.txt`: exact copy and live placement.
- `arc-f26-receipt.json`, `arc-f26-reference-r3.log`: real renderer transition
  and guarded cleanup.
- `pair-f26-toast-2x.png`: Arc/Echo instruction card crops. Card crops compare
  content; they do not qualify absolute fullscreen placement.
- `echo-f26-entered.png`, `echo-f26-exited.png`, `f26-native-receipt.json`:
  native Echo document entry, Escape release, instruction dismissal, original
  window-size restoration and strict zero exit.

## Implementation and acceptance

CEF’s public `on_fullscreen_mode_change` callback supplies actual document
state. It is scoped to the retained page identity and owning host. Only a
visible active page can enter; rejected background requests are released.
The requesting pane uses the complete window viewport while other Split panes
are hidden, preserving their models and retained native pages. Exit restores
the ordinary layout. A pre-existing native fullscreen window stays fullscreen.

Unmodified Escape releases the actual CEF document before ordinary browser key
routing. Native window exit, selection/host changes, renderer failure and quitting
also release ownership. The instruction uses the reusable toast with Arc’s exact
copy and a fixed vector Escape glyph. Its delayed entry is generation-scoped:
a rapid exit/re-entry cannot revive an earlier instruction, and dismissal cannot
remove a later unrelated toast.

Five GPUI tests cover document/native fullscreen, delayed cancellation,
background rejection, unrelated-toast retention, native exit, Split preservation
and renderer failure. The full workspace suite and Linux GNU/Windows MSVC checks
passed (`f26-full-final.log`, `f26-targets-final.log`). Native acceptance uses a
separate disposable profile, actual renderer state reports and real NSEvents.

Remaining work is **cosmetic**: the existing toast child window is clamped below
the fullscreen menu-bar region on macOS, so its vertical placement differs from
Arc’s top-edge instruction. Exact entry/exit motion is deferred. Other-platform
runtime appearance remains unqualified; no CEF source patch is used.

## Quitting during document fullscreen

Native acceptance exposed a checkpoint interaction: quitting a windowed browser
while its document remained fullscreen saved `mode: fullscreen`, although its
normal 1000×760 frame was retained. Temporary document presentation now preserves
the original window checkpoint. A genuine browser fullscreen receipt still
persists normally. The GPUI regression supplies the native `WindowBounds`
receipt explicitly because the headless platform models only its fullscreen flag.

`f26-checkpoint-native-baseline.log` records the previous fullscreen checkpoint.
`f26-checkpoint-native-r2.log`, `f26-checkpoint-native-receipt.json` and
`f26-checkpoint-during-fullscreen.json` verify actual renderer fullscreen, quitting
without document exit, a retained windowed 1000×760 checkpoint and strict exit 0.
These are disposable Echo-profile checks. Arc quit-during-fullscreen is reference
unqualified: the owner's four Arc windows were preserved rather than quit.
