# Site settings

## Installed Arc reference (2026-10-25)

Arc's All Site Settings… command opens `arc://settings/content/siteDetails?site=…`
in Peek. Echo renders its corresponding `browser://settings/content/siteDetails`
document entirely in GPUI. No Chromium WebUI is embedded.

The installed reference exposes 31 permission rows on macOS. The order, labels,
chooser-specific choices, Sound's Automatic/Mute wording, reset/delete
confirmations, 48 px rows (76 px for Intrusive ads), 200 × 36 px selectors,
680 px maximum section width, dark surfaces and Peek presentation were inspected.
CEF 154 provides each content-setting type, including Local network and Apps on
device. Profile-context operations remain behind `crates/engine`; no engine-source
patch or extension bridge is involved.

`GetContentSetting` returns an effective value, not its source. Echo qualifies
exact origin exceptions from the same request context's preferences; ambiguous
wildcard/provider sources remain unknown rather than falsely labeling an explicit
value as default. The matching CEF implementation is
<https://raw.githubusercontent.com/chromiumembedded/cef/14c5a089e8452874cb1a556dc3fde26ed2d87723/libcef/browser/request_context_impl.cc>.

Permission edits/reset retain the captured canonical profile and origin, including
when the source tab is no longer the foreground document. Deleted profiles,
reassigned Spaces and unavailable contexts fail closed. Delete data additionally
requires the original source document to remain at the captured origin, and uses
its existing engine context to clear origin data and refresh it. Reset and Delete
require the reference's confirmation dialogs. Selectors support Up/Down/Enter/Escape; a GPUI interaction test reaches All Site
Settings through the seven-row SCC More menu. A scoped activation guard returns
keyboard input to the current Settings Peek after the covered source reloads.
It ignores unrelated/closing windows and expires with the clear-data transaction;
native AX checks confirm that Delete retains the Peek key window and Escape
returns to the source. Regression tests cover late native focus and expiry.

## Evidence

All paths are relative to this worktree's `target/parity/site-controls/`:

- Arc: `scc-settings-fast4/settings.png`, `settings-middle.png`,
  `settings-bottom.png`; `scc-dialogs-r3/dropdown.png`, `reset-dialog.png`,
  `delete-dialog.png`.
- Echo: `echo-settings-native8/echo-settings.png`, `echo-dropdown.png`,
  `echo-camera-blocked.png`, `echo-reset-dialog.png`, `echo-reset.png`,
  `echo-delete-dialog.png`, `echo-deleted.png`.

## Remaining parity limits

- Accurate storage-byte/cookie counts are unavailable through the current wrapper;
  Echo leaves Usage blank rather than reporting a fabricated value. Delete is
  available only while the captured source document remains valid.
- Global Settings navigation/search behind the header controls, some permission
  glyph geometry, the initial Back focus decoration, and exact native materials,
  typography and animation still need parity work.
- Full Tab traversal of the SCC's main action tiles/rows and site-details
  header/buttons remains incomplete;
  its More menu and site-details selector navigation work. Arc's default keyboard
  configuration routed Tab into the underlying page during the guarded inspection.
- A guarded live `getUserMedia({video:true})` test showed no camera badge in the
  installed Arc's regular toolbar or SCC, including after explicitly disabling
  Developer Mode. See `scc-media-focused-r2/camera-active-toolbar.png` and
  `camera-active-scc.png`. This does not establish indicator parity for microphone,
  conferencing pages or background tabs; no speculative replacement badge was
  added. The test stream was stopped and Camera restored to Ask (default), verified
  in `media-permissions-restored.png`; Developer Mode and toolbar mode were restored.
- Full-page/Portrait capture and extension actions retain their separately
  documented limitations. The extension source-build experiment remains on hold.

## Arc window cleanup

The first inventory contained 10 browser-sized windows (nine Echo Test, one
Grosst), plus menu/system surfaces that were excluded from the browser count.
Three duplicate empty test windows were reassigned to Work, Яремко and Playable;
six remaining duplicate test windows were closed using Shift-Command-W. The result
was four windows, one for each owner Space. No Space or tab was deleted. A guarded
Command-N check produced exactly one additional window: 4 → 5 → 4.

Evidence: `cleanup-round2/before.json`, `headers.png`, `after.json`,
`after-headers.png`, `resume.log`. Later reference sessions used the existing
Echo Test Space and closed their owned windows/tabs. No new Space was created.
