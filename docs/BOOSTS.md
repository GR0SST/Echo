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

## Remaining work

This is a model/engine foundation, not a completed user-facing Boost feature.
Creation commands, the simple and code editors, the element picker, Library,
site controls and the Boost indicator are still missing. Constructed stylesheets
do not support CSS `@import` rules. Updated configurations also need native
browser recreation or another document-start synchronization mechanism before
shipping the editor: a replacement renderer can currently receive the browser's
original creation snapshot instead of subsequent edits.

The installed Arc was inspected in the fresh `Echo Boosts Oct24` test Space.
Its simple editor is a separate narrow floating panel; the expanded Code panel
has CSS and JS tabs. Its editor content currently displays "Internal debugging
pages are currently disabled". No visual parity is claimed for Echo.

Evidence and native fixture logs are under `target/parity/boosts/`. The local
fixture checks that the first inline page script sees a value set by a Boost,
that CSS changes the background, and that a Zap selector hides an element.
