# Sidebar interaction states verification

This change is a partial parity improvement. See `ARC_PARITY.md` for outstanding work.

## Implementation

Ordinary Today, Pinned and Favorite menus use the captured Arc section order.
Remove Pin archives in one Undo/Redo transaction and restores the original pinned
URL and folder placement. The row minus continues to reset/unload a pin.
Folder menus use the captured top-level sections; their destination picker still differs.
Ordinary close/reset hover targets are 24 × 24 px with 5 px corners, centered in
existing rows at a 6 px right inset. Today close glyphs use a 22 px vector canvas.
Segmented Split View styles and geometry are unchanged. Menu corner radius is 12 px.
The 8 px active / 4 px inactive dock marks from the theme work are preserved and
covered by geometry tests. No palette or material changes belong to this change.

## Native evidence

Fresh reference Space: Sidebar States Verify. Never manipulate owner Spaces.
Evidence lives under `target/parity/sidebar-states/` in this worktree.
Shared Arc guard: `~/Documents/Bot/parity-evidence/tools/arc_guard.py`.
Each successful capture session opens, claims and closes its own new Arc window.

Inspected at 3× zoom:

- `pair-close-3x.png`: Arc and Echo ordinary close hover.
- `pair-today-menu-3x.png`: active Today menu sections, spacing and dimensions.
- `pair-dock-3x.png`: 8 px active and 4 px inactive marks.
- `pair-pinned-menu-3x.png`: active Pinned menu and Remove Pin section.

Arc active Today menu measures approximately 208 × 307 px versus Echo 180 × 308 px.
Pinned menus are approximately 200 px (Arc) and 180 px (Echo) wide.
Both use 24 px item rows, 11 px separators, 13 px type and 16 px text inset.
Arc opens approximately (+1, -6) px from the click; Echo opens at the click.
The close hover square is 24 px with a 6 px right inset in both apps.
Colors differ in the initial pair because theme materials were implemented in parallel;
final Echo captures use the rebased bundle. No full visual-parity claim is made.

Some diagnostic screenshots have misleading provisional names (pin/new-tab attempts).
Only the explicit pairs above count as accepted comparison evidence.

## Remaining gaps

Spring click/reorder timing and implementation; pressed/dragged states; playing-audio
indicator and playback-dependent Mute enablement; native submenu widths/placement;
unloaded-tab menu differences; Move to Space/folder submenus; pinned URL Edit;
Share, Boost and unread-badge actions; complete folder nesting/open/closed comparison;
New Tab, Favorite and dock menu native appearance verification.

Fresh click recording `arc-click.mov` (79 variable-rate frames) shows 212 px idle
width, 210 px at 0.697–0.855 s, restored to 212 px at 0.867 s. This establishes
a roughly 170 ms sampled compression interval for a 150 ms held click, but does
not resolve the complete spring curve. `click-contact.png` and extracted frames
were inspected locally. The first click also entered rename, so it is not a clean
reorder reference and was not used to invent an animation.
Pre-reboot Pinned/Favorite/folder captures informed the menu changes, but their
`/tmp` files were lost. Pinned was reproduced as `arc-pinned-real-menu.png`;
Favorite and folder still need fresh reproduction.

Five GPUI regression tests cover menu ordering, Remove Pin/Undo, Favorite and folder
menus, dock geometry and close targets. Two core tests cover atomic pin removal,
original URL/folder restoration, Redo and rejected Today/Favorite requests.

## Round 2: menu material and width

Today and Pinned menus now have measured minimum widths of 208 and 200 px,
respectively. GPUI tests verify real menu bounds after right-clicking each row.
The bounded native effect uses the shared BackdropConfig API and records actual
installation; failed/unsupported effects render an opaque fallback. Menu tint is
HSL(accent, 0.08, 0.17) at 0.5 alpha over the dark active Menu effect. Text is
white at 0.85 alpha, sampled to match Arc's approximately 226/255 peak text.
The initial flat gray tint and heavy black rim were removed. Space palette and
Split View menu geometry are unchanged.

Inspected pairs: `pair-r2-menu-material-3x.png`,
`pair-r2-pinned-material-3x.png`. Neutral body samples are approximately RGB
(48, 47, 50) in Echo; Arc varies with its underlying Space/page from neutral
(48, 48, 48) to tinted (51, 48, 60). The sample profiles use different underlying
Space materials, so exact blur/color parity remains partial.

## Round 2: submenu geometry

`pair-r2-submenu-3x.png` was inspected after native capture. Edit Pinned Page is
232 × 58 px, overlaps the parent by 5 px, and aligns its first row with the
parent item. Root menus open (+1, −6) px from the click. Font-shaped intrinsic
widths replace the 260 px positioning estimate; submenus flip using their actual
width and clamp vertically. Overlay windows still limit menus to the browser
frame; Arc's ability to extend onto the surrounding desktop remains a gap.
Borders now paint over the frame rather than adding layout insets; item text
starts 16 px from the panel edge and Today/Pinned menu height is 307 px. Native
material installation defers refresh until after paint, fixing a submenu that
otherwise stayed opaque until the next pointer event. A GPUI regression test
checks overlap, height, fixed Edit width, left flipping and bottom-edge placement.


## Round 2: folder rows

Arc references `arc-r2-depth-two.png`, `arc-r2-folder-same-idle.png`,
`arc-r2-folder-same-hover.png` and gutter/icon/text hover captures show 16 px
nesting at each level, a 41 px row stride and an unchanged open/closed folder
icon on hover. There are no inline add or ellipsis controls. Echo now follows
those measurements and uses the ordinary sidebar hover opacity (0.10 in dark,
0.30 in light) rather than 0.35. Creation and actions remain in the context menu.
Inspected pairs: `pair-r2-folder-open-3x.png`,
`pair-r2-folder-hover-3x.png`, `pair-r2-folder-closed-3x.png`. The nesting and
hover controls now match; the existing catalog icon silhouettes remain smaller
and less angled than Arc and need a separate vector correction.
The existing 96 px deep-tree indentation cap and active-descendant treatment
remain unverified. GPUI coverage checks two nesting levels, collapse/reopen,
icon/title bounds on hover and context-menu invocation without toggling.

Audio follow-up: CEF 154 exposes capture callbacks, but the capture loopback
creator sets `mute_source = true`; installing that handler silences normal page
output. A non-disruptive audibility signal is still required before wiring real
playing indicators and Mute availability. Native playing references are
`arc-r2-audio-playing.png`, `arc-r2-audio-hover.png`, `arc-r2-audio-menu.png`.
Arc positions the playing indicator between favicon and title. No audio capture
handler or fake playing-state detection was added.

Motion references: `arc-r2-press.mov`, `arc-r2-reorder.mov`,
`reorder-contact.png`. Reorder shows a lifted card, insertion marker, retained
source slot and settling motion after release. These are measurement artifacts;
Echo's row spring and reorder animation are not complete.
