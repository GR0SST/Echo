# Core user profile integration contract

This change is based on `0f7f415c837f7ec922d26c60e79f7db308b39b77` in
`/Users/grosstgrosst/.t3/worktrees/ChromiumFork/t3-51ecad91`.
Only `crates/core` is changed. Browser and engine integration and all execution
validation belong to the parent. No Cargo, builds, tests, native apps, input,
commits or pushes were run here. This foundation does not close GUI parity.

## One workspace

Keep one normal `Browser` owner and engine tab namespace. `spaces` and their
order, `active_space`, `next_id`, `archive`, and window selections remain workspace
state. Do not create a normal Browser for each named user profile. A
`WindowProjection` remains read-only, with the same whole-workspace inventory.

`Space.user_profile_id: UserProfileId` assigns ownership. `all_tabs()` is the
canonical union of every Space's Pinned/Today pages and every profile's Favorites,
once each. Engine metadata callbacks (`set_url`, `set_title`, muting) still find
pages globally by stable TabId. Resolving a selected page, sidebar activation,
numbered ordering, Favorites edits and dock limits use the current Space's owner.
Identical URLs in different profiles remain independent IDs and data.

## Profile access and settings

`UserProfileId` is an opaque, Copy/Eq/Hash/Ord, serde-persisted identity. Its
numeric representation is not a browser API. IDs are allocated monotonically;
deleting an identity does not release it for reuse. A display name is never an
identity or storage key.

```rust
Browser::default_user_profile_id(&self) -> UserProfileId
Browser::active_user_profile_id(&self) -> UserProfileId
Browser::user_profiles(&self) -> impl Iterator<Item = &UserProfile>
Browser::user_profile(&self, id: UserProfileId) -> Option<&UserProfile>
Browser::user_profile_mut(&mut self, id: UserProfileId) -> Option<&mut UserProfile>
Browser::active_user_profile(&self) -> &UserProfile
Browser::active_user_profile_mut(&mut self) -> &mut UserProfile
Browser::user_profile_id_for_tab(&self, tab: TabId) -> Option<UserProfileId>
UserProfile::id(&self) -> UserProfileId
UserProfile::name(&self) -> &str
UserProfileId::storage_key(self) -> Option<String>
```

Replace writable Browser fields `favorites`, `favorite_splits`, `history`,
`archive_after`, `empty_favorites_dismissed`, `favicon_cache`, and
`include_favorites_in_ordering` with the same public fields on `UserProfile`.
For UI projections use `active_user_profile()`. For callbacks, saved action
owners and background settings changes use `user_profile(id)`/`user_profile_mut(id)`.
Take care to end mutable profile borrows before other Browser operations.

Profile data vectors are the authoritative storage; do not mirror them onto
Browser or merge profiles based on URLs. Use existing Favorites/split APIs for
structural edits so split and selection invariants are maintained.

`storage_key()` returns `None` only for canonical workspace Default identity,
including when queried from a Blank. For other identities it returns an
immutable lowercase key such as `Some("user-profile-2")`, independent of rename.
This is compatible with `StorageKey::new(&str)`. It is not a directory name or
path; the engine adapter owns placement and uses `default_storage_context()`
for `None` or `named_storage_context(&StorageKey)` for a named key. Existing
Default CEF storage must remain unchanged. Profile deletion does not touch
disk; deferred cleanup must never repurpose a key.

A newly created profile has no Favorites, Favorite splits, history or favicon
cache, and an undismissed empty dock. Ordering starts enabled. Its retention
interval is initialized once from Default as implementation policy. Native
copy-versus-live propagation remains unverified; this API does not implement a
live binding. Public native interval choices are 12h (default), 24h, 7d, 30d.
Core retains `Option<Duration>` including private/internal `None`; legacy values
are preserved, with no conversion based on public menu choices.

## Mutations and adapter transition

```rust
Browser::create_user_profile(&mut self, name: &str)
    -> Result<UserProfileId, UserProfileError>
Browser::rename_user_profile(&mut self, id: UserProfileId, name: &str)
    -> Result<(), UserProfileError>
Browser::delete_user_profile(&mut self, id: UserProfileId)
    -> Result<(), UserProfileError>
Browser::assign_space_user_profile(&mut self, space: SpaceId, to: UserProfileId)
    -> Result<Option<UserProfileTransition>, UserProfileError>
```

Types are in `browser_core::profiles`; `UserProfileId` is also re-exported at the
crate root. Errors are `UnknownProfile`, `UnknownSpace`, `EmptyName`,
`DefaultProfile`, `AssignedProfile`, `WindowLocalModel`. Names are trimmed and
must be nonempty. Default may be renamed but cannot be deleted. No assigned
profile may be deleted. Creation and assignment reject Blank/private models.
The core allows an unassigned registry entry; the browser creation form owns its
assignment requirement and source-dependent preselection.

Same-owner assignment returns `Ok(None)`. A changed assignment returns:

```rust
UserProfileTransition {
    space: SpaceId,
    from: UserProfileId,
    to: UserProfileId,
    recreate_tabs: Vec<TabId>,
}
```

The model has applied the assignment when it returns. All local Pinned/Today
TabIds and Space order survive. Old-profile Favorites stay with that profile;
any Favorite selection in the reassigned Space is cleared. The list contains
all local pages, including split panes, and excludes profile Favorites.

Before rendering the new ownership, the browser adapter must retire the old CEF
pages, invalidate/advance callback generations, then recreate these TabIds in
the target context. Late callbacks/history/favicon events must retain their
captured owner/generation and must not be rerouted through current selection.
This is an explicit integration requirement, not a claim about unverified Arc
loaded-tab reassignment behavior.

Existing boolean tab/split/folder move APIs reject transfers across profiles;
they cannot report a storage-context transition. Same-profile moves preserve
the existing behavior. Copy APIs allocate new identities in their destination.
Do not silently reuse an old loaded page for a different owner's copied tab.

Undo Favorite scopes capture their profile ID; replay can restore the original
profile even when another window/profile is selected. Assignment and deletion
invalidate undo/redo. A source ownership mismatch invalidates a captured edit
instead of silently moving its payload into the active profile.

## Background history and favicon API changes

```rust
Browser::record_visit(&mut self, profile: UserProfileId, url: &str, now: SystemTime)
Browser::update_history_title(&mut self, profile: UserProfileId, url: &str, title: &str)
Browser::delete_history_entry(&mut self, profile: UserProfileId, url: &str)
Browser::cache_favicon(&mut self, profile: UserProfileId, page: &str, png: Vec<u8>) -> bool
Browser::favicon(&self, profile: UserProfileId, page: &str) -> Option<&[u8]>
```

Unknown/deleted owners are no-ops for history, `false` for favicon writes and
`None` for reads. They never resolve to Default. Private visits/title updates
remain suppressed. Supply the source tab's captured owner to background
navigation/title/favicon callbacks. A profile-specific History UI reads that
profile's history, even if its request outlives a Space switch.

## Blank and private

`Browser::new_blank()` retains its existing Default-local standalone behavior.
For an actual source-bound Blank use:

```rust
Browser::new_blank_for_user_profile(&self, source: UserProfileId)
    -> Result<Browser, UserProfileError>
Browser::blank_source_user_profile_id(&self) -> Option<UserProfileId>
```

The result has an empty local Today/archive namespace and captures the source
identity, label and interval. Its `default_user_profile_id()` remains canonical
workspace Default, while `active_user_profile_id()` is the captured source. It does not copy the workspace or its Favorites,
history or favicon data. Blank tabs may overlap regular TabIds, as before;
window/model ownership still distinguishes their engine handles. Route the
Blank's pages to its captured owner's normal storage, and forward visits/title
updates explicitly to that normal workspace profile. Read normal history and
favicon data from that owner, not the empty local registry. Never copy the
Blank Browser back over the workspace.

When Blank is invoked from Incognito the browser adapter resolves the retained
normal source identity; private Default is not evidence of which named normal
profile should be used. The multi-profile native selection rule is unverified.
Private Browsers remain separate ephemeral models with history disabled and
`archive_after = None`. Runtime window kinds and source-bound Blank models are
not a session persistence contract. Serialize only the normal workspace.

## Archive and sessions

`ArchivedTab.user_profile_id` records origin provenance. `Browser.archive` stays
one shared unfiltered collection: global Archive visibility is unresolved.
Restore uses the original Space only if its owner still matches, otherwise a
Space with the same source owner. With no matching Space it returns `None` and
leaves the entry untouched, preventing restore into unrelated authentication.
Profile deletion leaves existing archive provenance intact.

Use `session::to_json/from_json/load/save`, not raw Browser deserialization, for
saved sessions. New flat JSON includes `format_version: 2`, the `profiles` map,
`default_user_profile_id`, `next_user_profile_id`, and explicit ownership on
Spaces/archive entries. There are no global writable profile aliases.

Unversioned legacy JSON moves all old global data into Default and assigns old
Spaces/archive entries to that identity. Valid existing Tab/Space/Split/Folder
IDs, Space order, settings including disabled/custom intervals, history and
favicon bytes survive. New IDs are repaired against all profiles, archived
split panes and folders. Duplicate physical tab IDs are repaired within their
own containers without combining profiles or URLs; corresponding local split,
folder and selection references follow the repair.

Any registry/version marker selects strict new-format loading. Missing/broken
Default identity, unknown Space ownership, invalid registry key/id/name,
unsupported/missing version, or invalid identity watermark returns an error.
Partial ownership fields cannot trigger legacy migration. The browser must
surface `InvalidData`; it must not catch this error and silently instantiate a
new Default workspace/CEF context. Profile ID watermarks include deleted IDs.

## Parent validation gates

Run the core tests plus complete workspace tests after browser conversion.
New regression coverage is in `src/profiles/tests.rs`; existing core consumer
tests were adapted to the accessors and explicit callback APIs. Perform native
assignment/generation/focus checks and persistent CEF process-restart checks in
the parent. No native GUI parity or successful runtime validation is claimed by
this isolated core change. Preserve the user's native reference Space/profile.

## Changed files

- `crates/core/PROFILE_MIGRATION.md`
- `crates/core/src/audio.rs`
- `crates/core/src/blank_tests.rs`
- `crates/core/src/bulk_archive.rs`
- `crates/core/src/favicons.rs`
- `crates/core/src/favorites.rs`
- `crates/core/src/folders.rs`
- `crates/core/src/folders/convert.rs`
- `crates/core/src/folders/tests.rs`
- `crates/core/src/history.rs`
- `crates/core/src/icons.rs`
- `crates/core/src/lib.rs`
- `crates/core/src/preferences.rs`
- `crates/core/src/profiles.rs`
- `crates/core/src/profiles/repair.rs`
- `crates/core/src/profiles/tests.rs`
- `crates/core/src/session.rs`
- `crates/core/src/splits.rs`
- `crates/core/src/splits/tests.rs`
- `crates/core/src/undo.rs`
- `crates/core/src/undo/archive.rs`
- `crates/core/src/undo/pane.rs`
- `crates/core/src/windows.rs`
