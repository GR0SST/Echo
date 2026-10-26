# Folder and mixed sidebar selections

Status: ordinary folder/mixed selection and grouping behavior verified natively.
Parity rows remain partial for the explicit residuals below.

## Arc reference

Guarded checks used one owned root window in the shared Echo Test Space, with
only namespaced empty folders and a controlled local HTTP page. Owner windows
71881, 71882, 71883 and 71885 were preserved. No Spaces were deleted.

- `target/parity/functional-sidebar/folder-selection-v2/arc-mixed.png` and
  `arc-context.png`: Command/Shift select two folder headers plus Today without
  replacing the current page. The menu offers Copy Links, Open in Split View
  (disabled for these empty folders), Move to, New Folder with 3 Items, Delete.
  Arc's cloud Share Items action is excluded from Echo.
- `folder-grouping-dialog/arc-group-dialog.png` is an immediate **inline rename**,
  despite the old filename: the command creates Untitled, groups the folders and
  pins Today immediately. Escape cancels naming, not the new group. The fixture
  was removed in `folder-group-cleanup-v3`; canonical sidebar records were checked.
- `folder-group-undo-reference/arc-{inline,grouped,undo,redo}.png`: the Edit menu
  reports Undo Rename Item, followed by Undo New Folder…. First-Undo rendering
  and the second structural Undo still need qualification. That session's
  cleanup initially left its owned test group. A fresh guarded cleanup in
  `folder-final-cleanup/` removed every fixture; canonical records were checked
  by IDs/namespace. Owner windows were preserved, the mouse was up and no Arc
  auxiliary layers remained. No owner-Space data was changed.
- `folder-empty-undo-reference/selection-menu.json`: selecting two empty folders
  also included the current item. Grouping was refused; both own folders were
  removed without touching the existing page.
- `folder-range-reference/result.json` and `arc-{command-a,shift-b,toggle-source}.png`:
  explicitly flagged mouse down/up events give 2 items after Command-A, 3 after
  Shift-B, and still 3 after Command-clicking the current controlled page. Arc
  retains the current item outside the extended range and does not toggle it
  off. Echo now follows this rule. The original page remains active.

## Implementation

A selection captures typed folder/tab roots, source Space and local profile.
Ancestor selection covers descendants. Missing roots or reassignment invalidate
an open menu instead of applying an action to an accidental smaller selection.
Favorites retain Arc's activation behavior.

Copy expands roots in sidebar/tree order. Grouping creates Untitled at the first
selected sibling and starts the sidebar text editor. Enter/blur saves the name;
Escape retains Untitled. Naming and creation have separate property/placement
Undo entries. Moves preserve folder trees, Split identities and loaded pages;
cross-profile folder moves are rejected. Delete handles empty trees, archived
folder trees and complete Pinned splits as one transaction.

Undo retains placement and individual properties, not a browser snapshot. A
trial model rejects invalid operations atomically, retaining newer navigation,
metadata and unrelated tabs. History limits count top-level user actions, not
children of a large selection.

## Native acceptance

`target/parity/functional-sidebar/folder-native/result.json` records the final
uninstrumented run (`6bb3e7b`), native exit 0:

- Menu counts 2 → 3 → 3 match Arc's Command / Shift / current-item toggle.
- Untitled appears as a selected inline name; paste/Enter saves Grouped locally.
- Undo first restores Untitled, then restores the original tree. Redo restores
  group ID 300 and its name; the final Undo restores folders 201/202 and Today 100.
- The original document's random marker stays RGB (114, 162, 174) through grouping,
  Undo, Redo and restoration; its HTTP URL is requested once. IDs never change.
- The fixture explicitly disables Warn before quitting so its native quit needs no
  confirmation input; the install/owner preference is untouched. CEF Complete
  and exactly-once original terminate are recorded on the merged shutdown code.
- The native process quits normally. Clipboard formats are restored, and the
  helper releases any mouse press before leaving the screen lock.

Inspected 2× pairs under `target/parity/functional-sidebar/pairs/`:
`folder-shift.png`, `folder-current-toggle.png`, `folder-inline.png`.
These qualify behavior and editor placement, not pixel parity: menu pointer
positions differ between captures, Echo's inherited menu is wider/darker,
selection ink and folder glyph raster differ, and the excluded cloud Share row
is absent. Further pixel polishing is deferred by the release strategy.

The first native candidate exposed a focus race that GPUI's ordinary input test
did not catch. Pending rename now owns the UI during menu teardown; native
responder ownership rejects an older CEF Focused callback after the editor has
focus. Real page focus still commits the name. The regression simulates both
callbacks plus delayed sidebar focus. Other platforms retain their current focus
behavior when no native responder query is available.

All 49 folder GPUI tests and the full workspace suite pass. The private batch
regression also rejects disabled persistent-folder destinations; disabled submenu
clicks now use the same enabled guard as hover/keyboard handling. Linux GNU and Windows
MSVC workspace checks pass in an isolated cache. The first shared-cache Windows
attempt reused an engine enum from another worktree; no extensions source change
was needed. Seven core tests cover identity-preserving grouping, rename Undo,
empty/mixed Delete, Pinned splits, stale scopes, atomic invalidation and 130-item
history transactions.

## Acceptance remaining

- Second structural Undo in Arc, hidden/collapsed-tree selection and cross-window deactivation.
- Live Folder mixed actions, folder sibling insertion, compound Split creation, cross-profile folder
  promotion and cross-window/alternate-state qualification remain partial.
- Color/glyph/material/motion polish is deferred by the release strategy.

## Extended range qualification, 2026-10-26

Two fresh guarded Echo Test references qualify a discontiguous selection with
three empty owned folders A/B/C and a current Today page. Command-click A,
Command-click C, Shift-click B selects all three folders plus the current page.
Escape and a click on the page preserve that selection. Repeating Shift-B
replaces the range with B/C/current; Shift-A expands to A/B/C/current.
Command-B removes B (its right-click then offers the single-folder menu), and
Shift-C retains A/C/current. This fixes the former sticky-first-click anchor,
which dropped C on the first Shift and produced different subsequent ranges.
A new GPUI regression drives this complete sequence and menu/page dismissal.
It fails against the old behavior before the fix.

Arc evidence: `target/parity/functional-sidebar/folder-extended-reference/`
(nonce `70972b5`) and `folder-range-shrink-reference/` (`f757604`), with screenshots,
menu inventories and cleanup receipts. The second wrapper reached its time limit
after its measurements and cleanup; its receipts show no owned windows or drag
layers left. Both sessions protected the two windows present in the fresh
inventory (Work 72640 and Grosst 72508); earlier recovered IDs were already gone
before input. Unrelated system file-access prompts were untouched.

Folder insertion remains unqualified. The hover-only `folder-insertion-reference/`,
`folder-insertion-quartz-reference/` and `folder-insertion-icon-reference/` attempts
never exposed a drag card or insertion marker, even when the shared left-button
state was confirmed down. They are diagnostic, not positive drag references.
Every attempt sent Escape and mouse-up, cleaned its own empty folders and page,
closed its root, and verified no held button or Arc layer>0 full-screen window.
No drop was performed. No insertion UI or behavior was inferred from these attempts.

Native before/after checks both terminate normally (exit 0):
`target/parity/functional-sidebar/folder-extended-before-native/result.json`
(`2aeb7b9`) reproduces the missing C and incorrect subsequent ranges;
`folder-extended-native/result.json` (`f10854e`) verifies the corrected live page.
Its document marker stays RGB (160, 101, 90) across all actions, Source is requested
once, IDs 201/202/203 and Today 100 remain unchanged, and CEF Complete precedes
exactly-once native termination. Both isolated fixtures disable Warn before
quitting; owner preferences are untouched.

Inspected 2× Arc / Echo-before / Echo-after crops:
`target/parity/functional-sidebar/pairs/folder-range-{first,repeat,expanded,last}.png`.
Selected roots and menu counts match (4 → 3 → 4 → 3). These are behavior evidence;
selection tint, folder glyphs, menu material/geometry and the excluded cloud Share
row still differ. The native after-page capture and GPUI menu/page dismissal
checks retain the corrected selection without navigation.
