# Folder archive and restoration — functional qualification

Reference unqualified. The guarded Arc session could not obtain exactly one
fresh owned window (zero/two fresh surfaces); owner windows were preserved.
The orchestrator authorized bundle/menu implementation with this limitation.
No Arc/Echo pair or visual completion is claimed.

Sources are the previously qualified ordinary folder menu (`Archive Folder`,
see `CONTEXT_MENUS_REFERENCE.md`) and the installed
`/Applications/Arc.app/Contents/MacOS/Arc` string block. The extraction in
`target/parity/partial-closeout/arc-strings-r4.txt`, lines 47523–47525, contains
`Folder restored to `, `. Click to view.`, and the comment identifying a folder
unarchived into a Space with a prompt to view it. Generic archive feedback reuses
the existing verified Undo component/copy; no folder-specific wording is invented.

Populated ordinary folders archive as one item, preserving their nested tree,
page identities/metadata, Split groups/geometry, selection and physical Pinned
order. Restoration merges with newer content rather than replacing a snapshot.
Library Restore and Command Bar archive search use the same durable model.
The exact restored-folder toast reveals and expands the captured destination;
stale/deleted targets and mismatched profile owners cannot redirect it.
Empty folders retain Delete; Live Folders are not included in this new operation.

Core regressions cover JSON restart, exact Undo/Redo, unrelated edits, identity
allocation repair, removed entries, malformed payloads and profile ownership.
GPUI regressions exercise the actual folder context action, toast Undo, Library
Restore, reveal and stale-click rejection. Native fixture/evidence lives under
`target/parity/partial-closeout/echo-folder-r4-*`; native acceptance and exit
status are recorded in `folder-native-r4-qualified.log` and
`echo-folder-r4-receipt.json`. The fixture serves only disposable local pages,
uses a copied complete canonical CEF bundle, and never opens Arc or owner data.
