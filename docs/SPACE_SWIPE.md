# Space switching motion

The reference is the installed `/Applications/Arc.app`, recorded at a fixed
1000 × 760 window frame. Native reference recordings and frame measurements
are saved in `target/parity/space-swipe/` (local QA artifacts, not bundled assets).

## Recorded motion

- `arc-clean.mov`: keyboard Next Space moves the outgoing list left and the
  incoming list right-to-left. Favorites, toolbar and dock remain stationary.
  Cmd+digits select tabs, Ctrl+digits select Spaces; Next/Previous Space uses
  Cmd+Option+Right/Left. Ctrl+Option+arrows are not a Space command.
- The incoming folder center is 184, 95, 40 and 30 px at 46, 101, 176 and
  246 ms after onset; its final center is 26 px. The page pitch is 228 px.
  A critically damped spring with angular frequency 24.7/s fits the recorded
  samples with 1.08 px RMS error. It reaches within 1 px in approximately
  307 ms. No overshoot is measurable in this sequence.
- `arc-gestures-fixed.mov`: a 120-unit precise scroll moves the list 60 logical
  pixels while the fingers remain down. A paused release returns to its origin.
  The return fits a critical spring at 34.3/s (1.40 px RMS error), reaching
  within 1 px in approximately 180–200 ms. Favorites remain fixed throughout.
- Paused threshold probes at a 228 px pitch cancel at 112 and 114 px, and
  commit at 116 px: the displacement must pass halfway. A 19.5 px flick at
  approximately 356 px/s commits. This establishes a successful flick speed,
  not the exact velocity boundary. Echo uses a provisional 300 px/s cutoff.
- Native Echo keyboard fits: 24.8/s before the final dock correction and
  22.6/s in the final capture (4.94–4.97 px RMS error; approximately 306–335 ms
  to within 1 px). The latter contains repeated presented frames. Logical
  spring evaluation remains 24.7/s; native presentation timing still varies. Variable-rate movie frames
  must be decoded with `-fps_mode passthrough` to preserve their timestamps.
- The reference's default dock dots stay fixed while one 8 px active dot moves
  between their centers (`dock-review.png`). Echo interpolates actual layout
  centers without changing the sidebar agent's dot sizes or icon artwork.

These are observations of the captured reference, not a claim about Arc's
private implementation or its physical trackpad calibration. The scroll gain
converts native precise scroll units; the list follows each event immediately.

## Implementation

The pager evaluates an analytic spring against GPUI's monotonic clock instead
of repeatedly multiplying remaining distance per frame. A delayed repaint
therefore catches up to the correct position. Release velocity is preserved;
a new horizontal gesture takes over at the currently visible position, and a
vertical gesture or touch without movement resumes the interrupted slide.
Momentum after release cannot initiate a second Space switch. Movement in the
begin and end events is included, and a reversing flick takes precedence over
the earlier drag distance.

The model selects the destination document at commit; only the sidebar lists
slide. Theme blending continues to consume the pager position. The active
default dock dot travels with the pager; custom icons retain their existing selected/idle rendering.

Regression tests cover native-event phases, distance, flick/pause/reversal,
direction, interrupted motion, delayed frames and the measured spring samples.
Boundary resistance, the exact flick-speed cutoff, custom-icon motion and
virtual-page rendering still require validation. The existing 0.3 edge
resistance is retained. Theme rendering remains owned by the theme workstream;
its blend API consumes the same pager position, but exact cross-fade timing
has not been quantitatively verified.

After Arc was restarted, guarded System Events sessions captured keyboard
switching in both directions. System Events timed out on a dock click and then
on setting Arc frontmost in the next session. Further reference probing was
stopped. No synthetic gestures or Arc scripting commands were sent during
these resumed sessions. Earlier gesture recordings supply the drag evidence.
The fresh dock capture remains incomplete; do not treat its partial movie as
proof of dock parity.

## Final native verification

`echo-native.mov` records Ctrl+digits, clicks on the existing dock marks, a
held 120 px drag, release and commit with loaded CEF fixture pages. The window
reports `onScreen=true`; the movie recorder is bounded independently of its
requested duration. `echo-before.png`, `echo-drag.png` and `echo-after.png`
confirm the unchanged document during drag and the committed destination.
`arc-echo-10-frames.png` and `comparison.html` compare ten requested samples
16 ms apart, with each actual recorded timestamp retained. Header/list crop
origins differ to account for the different Favorites fixtures.

The rebased full suite passes: 838 browser, 177 core and 105 engine tests.
Dock regression coverage now checks the stationary marks during travel and
existing 8/4 px idle endpoints after settling. Dark-theme dot contrast remains
with the theme/sidebar workstreams; this change preserves their mark colors.
