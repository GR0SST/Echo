# GitHub Live Folders reference

Reference: installed Arc 1.170, October 25, 2026. All Arc interactions used
bounded, owned-window sessions with the shared guard and screen lock, in the
shared **Echo Test** Space. No GitHub account was connected. The inspection’s
Live Folder was removed, appearance restored, and each owned window closed;
guard cleanup records report no owned windows left. No Space was deleted.

## Observed surfaces

- Space header/sidebar context: **Live Folders → GitHub** immediately adds a
  **Pull Requests** folder. This installation showed no authentication sheet.
  The native Spaces menu itself contains no Live Folder command.
- Live Folder context: **Pull Requests** submenu, **Move to** submenu,
  **Change Icon…**, **Rename…**, **Delete Live Folder**.
- Pull Requests submenu: disabled **Last fetch: …**, **Refresh**, separator,
  independently checked **Created by Me**, **Review Requests**,
  **Team Review Requests**. All three were checked initially. The brief’s
  **Assigned** filter is supported in the REST client, but is not invented as a
  fourth control in the observed Arc 1.170 menu.
- Context menu width 173px; PR submenu width 184px; 24px rows / 11px separators.
  At frame `{100,100,1000,760}`, right-clicking the folder at `{90,204}` places
  the menu at approximately `{91,198}`. The child overlaps by 5px.
- Delete confirmation: centered 450×292 panel, radius18, app icon52,
  **Delete your Pull Requests Live Folder?**,
  **You can restore this folder at any time by right-clicking the sidebar.**
  Cancel / ESC and Delete Folder / Return controls. The title is calibrated
  to the native 311px ink span; GPUI requires 17.2px with its system-font shaping
  and a 1px baseline/inset correction.
- Arc's executable strings additionally define **Repositories** and
  **This list is generated based on your currently active pull requests.**
  Repository options are therefore derived from PR results, not an invented
  all-repositories picker.

No Arc assets or brand glyphs are copied. Echo uses original vector marks and
its own app icon.

## Local-only adaptation and limits

Creation opens a masked PAT form if the local profile has no GitHub connection.
The form uses the measured Arc sheet palette, system typography and 450px width;
its token field and Connect/Cancel actions are an explicitly required local-only
adaptation, not an observed Arc authentication sheet. Connect GitHub… /
Reconnect GitHub… is added to the PR submenu for credential maintenance.
There is no Echo backend and no reused OAuth client ID.

The account reference survives folder deletion so recreating a folder does not
require re-entry. Changing a Space's local profile removes the former profile's
cached PR metadata and adopts only the destination's account. Cross-profile
folder moves are rejected. A canceled/stale form cannot attach a connection to
another profile; an unaccepted successful connection is retired on a worker.

Rows, badges and unread dots work against mocked REST data. Actual authenticated
Arc item layout, sorting/status treatment, long-list/show-more presentation, exact unread semantics, repository
submenu geometry and website-cookie SSO remain **visually unverified**. Do not
claim complete authenticated parity. Fine-grained token access is limited to its
selected resource owner/repositories; Team Review Requests requires an
organization-owned token. API details: [GitHub client](../crates/github/README.md).

## Evidence and validation

Full-frame pairs (all 1000×760) are saved locally under
`target/parity/live-folders/`:

| Surface | Arc | Echo |
| --- | --- | --- |
| Empty folder, Dark | `arc_github_creation.png` | `echo_github_creation_Dark.png` |
| Context, Dark | `arc_github_context.png` | `echo_github_context_Dark.png` |
| Filters, Light | `arc_github_filters_Light.png` | `echo_github_filters_Light.png` |
| Filters, Dark | `arc_github_filters_Dark.png` | `echo_github_filters_Dark.png` |
| Delete, Light | `arc_delete_live_folder.png` | `echo_delete_live_folder_Light.png` |
| Delete, Dark | `arc_delete_live_folder_Dark.png` | `echo_delete_live_folder_Dark.png` |

`echo_github_connection_{Light,Dark}.png` records the PAT adaptation. Echo uses
an isolated profile with inert/public tab fixtures; its placeholder Favorites
favicons and surrounding tab contents are not parity claims. Initial captures
are retained in `before/`. Both native appearances exited cleanly with status 0.

Added browser tests:
- `live_folder_creation_masks_token_and_cancel_never_connects`
- `live_folder_connect_refresh_filters_and_repository_visibility_preserve_read_state`
- `live_folder_delete_requires_confirmation_and_preserves_space`
- `live_folder_sheet_rejects_profile_reassignment_without_connecting`
- `repeated_pull_request_ids_keep_independent_live_folder_controls`

Five core tests cover unread revisions, session persistence, connection reuse,
profile isolation and rejection of static children without moving/allocating data. Nine mocked client tests cover ETag/304, pagination, query
filters/team membership, rate limits, cancellation, invalid/incomplete responses,
credential validation and disconnect. Full `cargo test` is required before push.
