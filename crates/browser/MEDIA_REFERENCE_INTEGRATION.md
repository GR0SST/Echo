# Media reference correction — applied for parent validation

Prepared in disjoint snapshots under
`/tmp/echo-media-reference-correction-20261023` during the root resize freeze.
Applied after the parent's explicit release following resize commit 67f0319
(commit identifier supplied by the parent, not independently checked with Git).
Only the six manifest files were changed. No Git/Cargo/build/native/focus
actions were run. Tests and visual acceptance remain pending with the parent.

## Inspected source and exact copy

Inspected the supplied 707 × 2000 image at:
`/Users/grosstgrosst/.t3/userdata/attachments/00f9b1e0-9165-4e44-8c94-d8757dffd14a-509054d8-32b3-4990-a901-ade77e146279.png`.
AGENTS.md was read. No reference app was opened or focused; no Space was
created, reassigned or deleted.

Media consists of a narrow list and a separate blank preview. The supplied
image has a rail near x=0–106, list x=106–390, search x=112–382/y=17–51,
preview x=390 onward/y=17. These are image-pixel measurements only, not
verified logical points. The image establishes a narrow list with its empty
group centered below the search toolbar and a separate large blank preview;
it does not establish a different Library navigation width on Media.

The visible title is `Nothing here yet!`. Body copy is preserved with only the
explicitly authorized Arc → Echo app-name substitution:

> Save media to your Documents, Desktop,
> and Download Folders to see them here,
> alongside your Captures in Echo.

The control says `Learn more`. No target URL was verified or invented, no help
copy was added, and no account/synchronization behavior exists. The visible
folder names are reference copy only: this slice does not enumerate, request
access to, watch or index any directory. It must not be reported as working
Media discovery.

## Integrated files and stable ownership

The core patch changes only `src/library.rs`, appends this handoff's correction
to `LIBRARY_INTEGRATION.md`, and adds:

- `src/library_media.rs`: window-local GPUI Media entity.
- `src/library_media_tests.rs`: five GPUI presentation/input tests.
- `MEDIA_REFERENCE_INTEGRATION.md`: this guide.

`library.rs` declares `#[path = "library_media.rs"] pub(crate) mod media;`,
so no `lib.rs` registration edit is needed. Its existing public render API,
intents, captured IDs and Space card rendering are unchanged. Every Library
section keeps the same 112 px rail, 96 × 80 tiles, 100 px pitch and centered
580 px group. No route-specific navigation scaling is introduced. Spaces
cards retain x=140, width=240, gap=24 and their full overview width.

`MEDIA_REFERENCE_COLUMN_WIDTH` is the named, tunable provisional Media list
width (284), derived from the uncalibrated reference raster. This value is not
claimed as a verified native logical-point width. It remains separate from
the shared rail geometry and should be calibrated by the parent's native
visual gate. Search begins 6 px into the list; the preview begins at the
list's right edge. Consequently the current proposal places list/preview at
x=112/x=396, preserving a stable rail rather than forcing the screenshot's
uncalibrated x=106/x=390 boundaries into navigation logic. The structural
correction is the list plus separate preview, not a rail jump.
Downloads' new presenter remains separate, unregistered and unchanged.

The host hook was prepared separately against a TMP snapshot of
`sidebar_navigation.rs` and applied as three narrow hunks after release. The
current file was not replaced wholesale; its resize render-width overrides
and all unrelated fields/logic were preserved and checked by reversing only
those three replacements against the release checkpoint.
It adds one cached `Option<Entity<library::media::MediaView>>` to window-local
`SidebarNavigation`, initializes it to None, and replaces the full-area centered
Media placeholder with the persistent entity as `other_section`.

## Presenter hooks

`MediaView::new(MediaColors, cx)` creates the real existing `TextInput`, using
`Search Media...` as shown in the supplied image. No auto-focus is inferred.
The host's existing `text_input::bind_keys` initialization remains required.
Keep the entity per window while switching Library sections. `query(&App)`
returns its retained query; `MediaIntent::SearchChanged(String)` emits changes.
No search matching, debounce or search-result/permission copy is invented.

`MediaIntent::LearnMoreRequested` is an owned event with no URL. The proposed
host installs no navigation subscriber until the exact native target is
verified. The reference control is rendered and emits only that event; do not
add a guessed resources URL, fallback toast, tooltip or help page.

`MediaColors` requires semantic `column`, `preview`, `text`, `muted`, `control`
and `hover` colors from the parent. `set_colors(colors, cx)` preserves query and
notifies only when colors actually change, avoiding a render/notify loop when
the host updates them during rendering. The tiny host patch inherits its
current Library surfaces and Space theme mapping rather than changing App
appearance. The existing host's hardcoded light Library palette is not fixed
by this slice; its owner must supply the actual semantic theme values for
light/dark visual acceptance. No `App::set_appearance` or global theme mutation
is introduced.

The empty symbol redraws generic stacked photo frames using Echo-owned fixed
vectors shared with the rail. No Arc logo, original asset or font glyph is
used as a control. Media body text contains Echo as required.

## Tests and acceptance still required

Five authored GPUI tests cover:

1. A provisional reference-width list after the shared rail, search inset,
   preview separation and list-centered empty group, including the 48 px icon
   and 94 × 26 button. These are proposal-layout checks, not assertions that
   the screenshot established native logical points.
2. Empty-group centering inside the narrow list at 1280 × 800 and 640 × 480.
3. Real input, query retention across Media → Spaces → Media and semantic
   color changes; this also asserts the original Space card x/width.
4. Learn more emitting only a target-free intent, retaining the blank preview.
5. Unchanged rail, group, every tile and icon bounds while switching through
   all six Library sections; at 800 px height the shared group stays at y=110.

Existing `library_tests.rs` remains byte-for-byte unchanged from this snapshot;
its original native capture/card/scroll assertions still apply to Spaces and
Downloads. New selectors: `library-media`, `library-media-column`,
`library-media-toolbar`, `library-media-search`, `library-media-list-body`,
`library-media-empty`, `library-media-empty-icon`, `library-media-empty-title`,
`library-media-empty-body`, `library-media-learn-more`, `library-media-preview`.

Only rustfmt parsing/check and whitespace checks were performed in TMP.
The root integration is now frozen for the parent to run Cargo, check
keyboard/focus routing in the real same-window Library, and compare screenshots
visually. Exact before/after hashes and the six-file manifest are saved in the
TMP `release-checkpoint` directory. Do not mark Media
inventory/search behavior or whole Library parity complete from these tests.
