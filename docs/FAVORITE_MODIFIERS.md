# Favorite modifier clicks

Guarded Arc 1.170 checks in the shared Echo Test Space show that Command-click
and Shift-click activate the clicked Favorite. They do not extend the sidebar
selection. Echo already follows this behavior; adding Favorite multiselection
would introduce a divergence. Folder/mixed-row selection remains a separate gap.

## Evidence

- Arc: `target/parity/functional-sidebar/favorite-selection-v2/arc-command.png`
  and `arc-shift.png`. Started on Gmail, clicked YouTube with each modifier;
  YouTube became the active Favorite. Only sidebar images were captured.
- Echo: `target/parity/functional-sidebar/favorite-native/echo-command.png`
  and `echo-shift.png`, with isolated local A/B fixture pages.
- Inspected pairs: `target/parity/functional-sidebar/pairs/favorite-command.png`
  and `favorite-shift.png`. This verifies behavior, not icon/material parity.
- Native `result.json`: exit 0; A → Command-B → A → Shift-B, both loaded
  fixture pages requested once, Favorite identities and Today count preserved.
  Source restored; own fixture pages unloaded, own native process exited.
- GPUI regression: `favorite_modifier_clicks_activate_instead_of_extending_sidebar_selection`
  verifies both modifiers activate without extending selection, reloading or
  closing the existing page.

The native input helper explicitly releases each synthetic mouse press before
releasing its keyboard modifier. It checks that no mouse button remains down.
No Favorites in the owner profile were moved, renamed, removed or unpinned.
