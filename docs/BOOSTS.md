# Local Boost foundation

Boosts live in `UserProfile::boosts`, alongside that profile's history and
Favorites. `Boosts::create` accepts HTTP(S) URLs and stores a canonical ASCII
hostname. Matching is exact by hostname (including neither subdomains nor
lookalike suffixes); scheme and port do not change the site selection. New
Boosts are named "My Boost", as in the installed reference. IDs are local to
a profile, monotonic, and validated when restoring a session. Old sessions
start with an empty registry.

`Pages::sync` compiles enabled Boosts into engine-owned `SiteBoost` values
for the exact storage selection. It never exposes CEF types to the core.
The engine supplies the initial configuration through native browser creation
metadata. The renderer applies it in `on_context_created`, before page scripts.
CSS uses a constructed stylesheet, preserving page-owned adopted stylesheets
and avoiding dependence on inline-style CSP permissions or a document root.
Zap selectors append `display: none !important` rules. Each JavaScript Boost
is evaluated separately; syntax errors cannot stop other Boosts or their CSS.
Live configuration messages replace CSS, while JavaScript waits for the next
document. Inspector pages never receive Boost configuration.

## Editor and picker

New Boost is available from the dock menu, Spaces menu, Command Bar and site
controls. Customize Page opens an existing site Boost when available. The
floating GPUI panel retains the reference's 184 x 590 frame; Code expands to
480 x 590 while keeping the right edge fixed. CSS and JS use a multiline GPUI
text buffer with IME, clipboard, selection and local undo. Refresh to Run
reloads the captured page. Simple controls persist tint, font, size, case,
inversion, contrast, brightness and saturation independently of custom code.

The engine picker has a native capability scoped to a renderer context. Trusted
pointer events highlight an element and a trusted click reports a selector.
Browser events retain page/profile ownership before modifying a Boost; Undo
removes the last Zap selector. The native request handler sends the current
profile configuration again before navigation and when a render view is ready.

## Library

The Boosts route uses the reference's 304 px searchable column and 56 px rows.
Hover reveals a per-record toggle. Opening a record creates an independent site
preview with the captured profile's storage and Boosts; the floating editor
writes to the canonical profile while picker events are checked against the
preview's window, profile and tab. Preview edits are reflected immediately and
survive closing the preview. The preview does not allocate a regular tab.

## Remaining work

Library rename/delete, related-element Zap selector editing
and full color remapping fidelity remain unfinished. Constructed stylesheets do
not support CSS `@import` rules. Renderer replacement synchronization has native
hooks but still needs dedicated crash/recovery acceptance coverage.

The installed Arc was inspected in the fresh `Echo Boosts Oct24` test Space.
Its simple editor is a separate narrow floating panel; the expanded Code panel
has CSS and JS tabs. Its editor content currently displays "Internal debugging
pages are currently disabled". Arc/Echo crops are retained for visual comparison; exact color remapping parity is not claimed.

Evidence and native fixture logs are under `target/parity/boosts/`. The local
fixture checks that the first inline page script sees a value set by a Boost,
that CSS changes the background, and that a Zap selector hides an element.
