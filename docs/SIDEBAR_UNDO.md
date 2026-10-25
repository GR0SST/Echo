# Sidebar Undo and Redo

## Archive Redo after committed regrouping

A guarded Arc check in Echo Test opens disposable pages A and B, archives B, restores it with Undo, and commits C beside B in a Split. Redo removes only B; C and A remain. The following Undo rebuilds B/C and keeps C selected. Generic Edit Undo/Redo labels were enabled throughout; labels alone are not evidence of the affected action. The screenshots were inspected. Reference: `target/parity/command-sidebar-r2/arc_undo-regroup-{before-redo,redone,second-undo}.png`; contract and cleanup: `target/parity/reaudit2/sessions/archive-redo-regroup-functional/`.

Echo's single-item archive transaction now captures the current Split placement before removing its original tab. Undo reuses pane reconstruction, retaining current surviving-page navigation, group identity, proportions and selection. Repeated cycles continue targeting the original archived tab rather than archiving its newly joined neighbor. This is distinct from pane-close Redo, whose existing Arc-qualified behavior archives the entire reconstructed item.

The transaction still rejects moved/profile-reassigned targets and unqualified regrouping of multi-item batches before mutation. Core coverage checks repeated cycles, identity, proportions, selection and survivor navigation. GPUI coverage exercises real shortcuts and committing a Split through the Command Bar, and checks the survivor is neither closed nor recreated.

## Remaining scope

Multi-item archive regrouping and remaining sidebar actions need their own positive behavior references. Dynamic action-specific labels remain cosmetic follow-up. A second guarded Arc check pins an owned disposable tab, unloads it with Command-W, then invokes Undo. Undo reverses the pin move: the item returns to Today while the empty canvas remains selected. It does not reopen the unloaded document. Echo already matches this behavior; core and GPUI regression tests now cover it. Reference: `target/parity/command-sidebar-r2/arc_undo-pinned-{unloaded,restored}.png`, with the action contract in `target/parity/reaudit2/sessions/pinned-unload-undo-functional-action/`. The disposable item is identified by its exact saved URL even when Arc renames its title, unpinned if necessary, and archived before closing the owned root. Favorite unloading remains unqualified rather than a demonstrated functional defect.

## Native acceptance

The disposable Echo fixture performs the same archive → Undo → commit Split → Redo → Undo sequence through real shortcuts, then quits natively. It verifies B is the only removed page after Redo, B/C is reconstructed after Undo, C remains selected, and the fixture server receives exactly one C document request. Evidence: `target/parity/sidebar-undo/native/{grouped,redone,restored}-screen.png`, `result.json`, `engine.log`; Arc counterparts: `target/parity/sidebar-undo/arc-{grouped,redone,restored}.png`. The final screenshots were inspected. An earlier run used the artifact built before the edit and showed the old rejection; the completed rebuild and repeated native run passed using canonical CEF generation `20261025T192350Z-5008`.
