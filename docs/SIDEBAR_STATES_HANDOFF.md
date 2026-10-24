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
