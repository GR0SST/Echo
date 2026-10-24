# Manage Spaces

Choose **Spaces → Manage Spaces…**, or use **Manage Spaces** in the Command Bar,
to open the Spaces section of Library. The existing Space cards remain the
management surface. This command does not create a Space or change its profile.
Returning to the original Space keeps its loaded document. An unfinished New
Space draft remains local to its window.

The route is unavailable in Private and Blank windows. It has no default
keyboard shortcut. Command Bar search also recognizes the reference's
“Space Management”, “Organize”, “Toggle Space Manager” and “View Spaces” terms.

## Reference and validation

The installed Arc 1.170.0 menu archive identifies **Manage Spaces…** under
Spaces, after the Next/Previous Space group. Its key equivalent is empty, its
action is `toggleSpaceManager:`, and its Command Bar identifier is
`openSpaceManager`. The archive's SHA-256 is
`c05560f6665c062b78dd326c4ce56307154010f981a45f4959671fb460ab1963`.
This establishes the entry's name, location and search terms; the selector's
name alone does not establish repeated-invocation behavior.

GPUI regressions exercise menu and Command Bar entry, the Downloads-to-Spaces
transition, native-page preservation, Find and Command Bar popup routing,
retained creation text, focused Media preview handoff, and Private/Blank exclusion.

A separate macOS QA bundle was checked through the native Spaces menu, Command
Bar, Find popup and a focused Library Media preview. Each route displayed the
Spaces cards. Returning to the original Space restored typing without clicking
the document again; the local fixture retained one document UUID and one load.
The QA process completed normal CEF shutdown. The already-running Echo instance
and its bundle were left unchanged.

Full manager presentation, reference toggle behavior and cross-platform native
acceptance remain open.
