# Page-menu action qualification (2026-10-26)

Read-only Arc 1.170 references use public, locally served fixtures in Echo Test. Native menu captures are retained locally under `target/parity/functional20-24/arc_context_{blank,image,editable,video}.png`. No account, owner page or owner profile is modified.

The blank-page Save As… action now uses Echo's existing document-bound HTML/MHTML/Complete exporter. It is enabled by the engine's archive capability, independently of ordinary download availability. GPUI covers cancellation without navigation or renderer replacement.

Video context menus now follow the captured Arc order, including native checked Loop and Show All Controls rows. Open Video in New Tab, Save Video As…, Copy Video Address and Picture in Picture route through the captured document. Loop, controls and PiP bind the clicked main-frame video once using a short-lived weak reference and opaque token. Execution requires the same live document, connected video and source; it never falls back to another element. Frame-context controls remain unavailable pending equivalent frame binding.

Native Echo acceptance records actual loop/control property changes on the fixture, exact video-address clipboard output, and the real Save Video As chooser followed by Cancel. Evidence: `echo_context_video_before.png`, `echo_context_video_after.png`, `echo_context_video_loop_off_after.png`, `echo_video_save_cancel_after.png`, and `native-after-57eaa942/media-actions.json` in that evidence directory. PiP's existing action is wired here but its native context-menu launch is not independently qualified in this pass.

Editable selections retain CEF's editing/spelling entries and gain Arc's selected-provider Search and Print entries. Search now respects custom default providers instead of the built-in engine enum; tests cover exact encoded selection, source retention and stale-document rejection.

The row remains partial. Intrinsic video-frame save/copy, Capture Image's floating-image editor integration, link-to-highlight generation, complete OS editing submenus and clicked-node inspection remain unfinished. Arc's SVG Copy Image produced a 180×80 PNG (`arc_svg_copied.png`); Echo SVG conversion is still unqualified. Cast requires a mirroring engine bridge (see CAST_FEASIBILITY.md); provider-free Google-backed translation is excluded (TRANSLATION_FEASIBILITY.md). No substitute UI is invented for these gaps.

Arc Capture Image briefly displayed its causal floating preview with “Saved to Library”; a subsequent read-only Library check was empty (`arc_library_fixture_capture.png`). The fixture tab and every owned Arc window were closed; no identifiable Library asset was present to remove. All reference screenshots remain local and uncommitted.

The mandatory `scripts/check-targets.sh` gate passes for Linux GNU and Windows MSVC with the real target-specific CEF bindings (`engine/check-only`). It caught Windows' signed representation of the CEF media-state bitfield; the menu adapter now normalizes it to `u32`. These compile checks do not establish foreign native UI/runtime parity.
