use super::*;
use crate::session;
use std::time::SystemTime;
const NOW: SystemTime = SystemTime::UNIX_EPOCH;

#[test]
fn duplicating_a_folder_preserves_nested_order_and_split_metadata_with_independent_ids() {
    let mut browser = Browser::new();
    let space = browser.space().id;
    let parent = browser.create_folder(space, None, "Projects").unwrap();
    let source = browser.create_folder(space, Some(parent), "Reading").unwrap();
    let nested = browser.create_folder(space, Some(source), "Reference").unwrap();
    let a = pin(&mut browser, "a");
    let b = pin(&mut browser, "b");
    let c = pin(&mut browser, "c");
    browser.move_item_to_folder(a, source);
    browser.move_item_to_folder(b, nested);
    browser.add_split(b, c, crate::splits::Placement::Below, NOW).unwrap();
    browser.set_split_weights(b, &[1., 3.]);
    browser.rename_tab(b, "Research pair");
    browser.set_item_icon(b, Some("🌗"));
    browser.set_folder_icon(source, Some("📚"));
    browser.set_folder_collapsed(source, true);
    browser.set_folder_collapsed(nested, true);
    browser.navigate(a, "https://a/visited");
    browser.set_tab_muted(a, true);
    browser.activate_tab(c, NOW);
    let originals: Vec<_> = browser.folder_tabs(source).into_iter().map(|id| browser.all_tabs().find(|tab| tab.id == id).unwrap().clone()).collect();
    let source_items = browser.folder(source).unwrap().items.clone();
    let source_split = browser.split_for(b).unwrap().clone();
    let later = NOW + std::time::Duration::from_secs(100);
    let copy = browser.duplicate_folder(source, later).unwrap();
    assert_eq!(browser.folder(parent).unwrap().items, vec![PinnedItem::Folder(source), PinnedItem::Folder(copy)]);
    assert_eq!(browser.space().active_tab, Some(c));
    assert_eq!(browser.folder(source).unwrap().items, source_items);
    let copy_folder = browser.folder(copy).unwrap();
    assert_eq!(copy_folder.name, "Reading");
    assert_eq!(copy_folder.custom_icon.as_deref(), Some("📚"));
    assert!(copy_folder.collapsed);
    let PinnedItem::Folder(copy_nested) = copy_folder.items[0] else { panic!("nested folder order was changed"); };
    assert_ne!(copy_nested, nested);
    assert_eq!(browser.folder(copy_nested).unwrap().name, "Reference");
    assert!(browser.folder(copy_nested).unwrap().collapsed);
    let copies = browser.folder_tabs(copy);
    assert_eq!(copies.len(), originals.len());
    for (id, original) in copies.iter().zip(&originals) {
        let copied = browser.all_tabs().find(|tab| tab.id == *id).unwrap();
        assert!(!originals.iter().any(|tab| tab.id == *id));
        let mut expected = original.clone();
        expected.id = *id;
        expected.last_active = later;
        assert_eq!(serde_json::to_value(copied).unwrap(), serde_json::to_value(expected).unwrap());
    }
    let split = browser.split_for(copies[0]).unwrap();
    assert_ne!(split.id, source_split.id);
    assert_eq!(split.axis, source_split.axis);
    assert_eq!(split.weights, source_split.weights);
    assert_eq!(split.title, source_split.title);
    assert_eq!(split.custom_icon, source_split.custom_icon);
    assert_eq!(split.panes, copies[..2]);
    assert_eq!(split.active, copies[1]);
    assert_consistent(&browser);
    let mut loaded = session::from_json(&session::to_json(&browser)).unwrap();
    assert_eq!(loaded.folder_tabs(copy), copies);
    loaded.rename_folder(copy, "Independent");
    loaded.navigate(copies[2], "https://copy-only");
    loaded.toggle_pin(copies[0]);
    assert_eq!(loaded.folder(source).unwrap().name, "Reading");
    assert_eq!(loaded.all_tabs().find(|tab| tab.id == a).unwrap().url, "https://a/visited");
    assert_eq!(loaded.split_for(b).unwrap().panes, source_split.panes);
    assert_consistent(&loaded);
}

#[test]
fn duplicating_empty_and_maximum_depth_folders_keeps_other_spaces_and_focus_unchanged() {
    let mut browser = Browser::new();
    let space = browser.space().id;
    let root = browser.create_folder(space, None, "Root").unwrap();
    let mut deepest = root;
    for _ in 1..MAX_FOLDER_DEPTH { deepest = browser.create_folder(space, Some(deepest), "Nested").unwrap(); }
    let tab = pin(&mut browser, "deep");
    browser.move_item_to_folder(tab, deepest);
    let other = browser.add_space("Work", 100.);
    let index = browser.spaces.iter().position(|space| space.id == other).unwrap();
    browser.switch_space(index);
    let active = browser.new_tab("https://work", NOW);
    let copy = browser.duplicate_folder(root, NOW).unwrap();
    assert_eq!(browser.space().id, other);
    assert_eq!(browser.space().active_tab, Some(active));
    assert_eq!(browser.folder_ancestors(browser.folder_tabs(copy)[0]).len(), MAX_FOLDER_DEPTH);
    let empty = browser.create_folder(other, None, "Empty").unwrap();
    let empty_copy = browser.duplicate_folder(empty, NOW).unwrap();
    assert!(browser.folder(empty_copy).unwrap().items.is_empty());
    assert_eq!(browser.space().active_tab, Some(active));
    let before = session::to_json(&browser);
    assert!(browser.duplicate_folder(FolderId(u64::MAX), NOW).is_none());
    assert_eq!(session::to_json(&browser), before);
    assert_consistent(&browser);
}

#[test]
fn nested_folders_pin_existing_pages_and_round_trip_without_cloning_them() {
    let mut browser = Browser::new();
    let space = browser.space().id;
    let tab = browser.new_tab("https://page", NOW);
    let root = browser.create_folder(space, None, "Projects").unwrap();
    let child = browser.create_folder(space, Some(root), "Browser").unwrap();
    assert!(browser.move_item_to_folder(tab, child));
    assert_eq!(browser.all_tabs().count(), 1);
    assert!(browser.space().today.is_empty());
    assert_eq!(browser.space().pinned[0].id, tab);
    assert_eq!(
        browser.space().pinned[0].pinned_url.as_deref(),
        Some("https://page")
    );
    assert_eq!(browser.folder_path(child), "Projects / Browser");
    assert_eq!(browser.folder_ancestors(tab), vec![root, child]);
    let loaded = session::from_json(&session::to_json(&browser)).unwrap();
    assert_eq!(loaded.folder_ancestors(tab), vec![root, child]);
}

fn pin(browser: &mut Browser, name: &str) -> TabId {
    let id = browser.new_tab(format!("https://{name}"), NOW);
    browser.set_title(id, name);
    browser.toggle_pin(id);
    id
}
fn tab_ids(browser: &Browser) -> HashSet<TabId> {
    browser
        .all_tabs()
        .map(|tab| tab.id)
        .chain(browser.archive.iter().flat_map(|entry| {
            std::iter::once(entry.tab.id).chain(
                entry
                    .split
                    .iter()
                    .flat_map(|split| split.panes.iter().map(|tab| tab.id)),
            )
        }))
        .collect()
}
fn assert_consistent(browser: &Browser) {
    let mut folder_ids = HashSet::new();
    for space in &browser.spaces {
        let rows = browser.pinned_rows(space.id, true);
        let mut seen = HashSet::new();
        let mut pages = HashSet::new();
        for row in rows {
            assert!(
                seen.insert(row.item),
                "duplicate tree placement: {:?}",
                row.item
            );
            assert!(row.depth <= MAX_FOLDER_DEPTH);
            if let PinnedItem::Folder(id) = row.item {
                assert!(folder_ids.insert(id));
            } else {
                let anchor = browser.pinned_item_tab(space.id, row.item).unwrap();
                pages.extend(browser.related_tabs(anchor));
            }
        }
        assert_eq!(pages, space.pinned.iter().map(|tab| tab.id).collect());
        assert_eq!(
            seen.iter()
                .filter(|item| matches!(item, PinnedItem::Folder(_)))
                .count(),
            space.folders.len()
        );
        for split in &space.splits {
            if space.pinned.iter().any(|tab| tab.id == split.anchor) {
                assert!(split
                    .panes
                    .iter()
                    .all(|id| space.pinned.iter().any(|tab| tab.id == *id)));
            }
        }
    }
}

#[test]
fn unpinning_favoriting_and_moving_pages_do_not_revive_stale_folder_membership() {
    let mut browser = Browser::new();
    let space = browser.space().id;
    let folder = browser.create_folder(space, None, "Projects").unwrap();
    let tab = pin(&mut browser, "a");
    browser.move_item_to_folder(tab, folder);
    browser.toggle_pin(tab);
    assert!(browser.folder(folder).unwrap().items.is_empty());
    browser.toggle_pin(tab);
    assert!(browser.folder_ancestors(tab).is_empty());
    browser.move_item_to_folder(tab, folder);
    browser.favorite_tab(tab);
    assert!(browser.folder(folder).unwrap().items.is_empty());
    browser.unfavorite_tab(tab);
    browser.toggle_pin(tab);
    assert!(browser.folder_ancestors(tab).is_empty());
    browser.move_item_to_folder(tab, folder);
    let other = browser.add_space("Work", 20.);
    browser.move_tab(tab, other, Section::Pinned);
    assert!(browser.folder(folder).unwrap().items.is_empty());
    assert!(browser.folder_ancestors(tab).is_empty());
    assert_consistent(&browser);
}

#[test]
fn creating_splitting_and_closing_panes_keeps_the_anchor_folders_position() {
    let mut browser = Browser::new();
    let space = browser.space().id;
    let a = pin(&mut browser, "a");
    let b = pin(&mut browser, "b");
    let folder = browser.create_folder(space, None, "Project").unwrap();
    browser.move_item_to_folder(a, folder);
    let split = browser
        .add_split(a, b, crate::splits::Placement::Left, NOW)
        .unwrap();
    assert_eq!(
        browser.folder(folder).unwrap().items,
        vec![PinnedItem::Split(split)]
    );
    assert_eq!(browser.folder_ancestors(b), vec![folder]);
    let c = browser.new_tab("https://c", NOW);
    browser
        .add_split(b, c, crate::splits::Placement::Right, NOW)
        .unwrap();
    browser.close_split_pane(a, NOW);
    assert_eq!(
        browser.folder(folder).unwrap().items,
        vec![PinnedItem::Split(split)]
    );
    assert_eq!(browser.folder_ancestors(c), vec![folder]);
    browser.close_split_pane(b, NOW);
    assert_eq!(
        browser.folder(folder).unwrap().items,
        vec![PinnedItem::Tab(c)]
    );
    assert_consistent(&browser);
}

#[test]
fn separating_and_duplicating_items_keep_every_page_inside_the_same_folder() {
    let mut browser = Browser::new();
    let space = browser.space().id;
    let folder = browser.create_folder(space, None, "Project").unwrap();
    let a = pin(&mut browser, "a");
    let b = browser.new_tab("https://b", NOW);
    browser.move_item_to_folder(a, folder);
    browser
        .add_split(a, b, crate::splits::Placement::Right, NOW)
        .unwrap();
    let original = browser.pinned_item(a);
    let copy = browser.duplicate_item(a, NOW).unwrap();
    let copied = browser.pinned_item(copy);
    assert_eq!(
        browser.folder(folder).unwrap().items,
        vec![original, copied]
    );
    browser.separate_split_pane(a);
    assert_eq!(
        browser.folder(folder).unwrap().items,
        vec![PinnedItem::Tab(a), PinnedItem::Tab(b), copied]
    );
    browser.separate_split(copy);
    assert_eq!(browser.folder(folder).unwrap().items.len(), 4);
    assert_consistent(&browser);
}

#[test]
fn moving_a_nested_subtree_between_spaces_preserves_split_metadata_and_all_page_identities() {
    let mut browser = Browser::new();
    let source = browser.space().id;
    let root = browser.create_folder(source, None, "Projects").unwrap();
    let nested = browser
        .create_folder(source, Some(root), "Research")
        .unwrap();
    let a = pin(&mut browser, "a");
    let b = browser.new_tab("https://b", NOW);
    browser
        .add_split(a, b, crate::splits::Placement::Right, NOW)
        .unwrap();
    browser.move_item_to_folder(a, nested);
    browser.rename_tab(a, "Comparison");
    browser.set_item_icon(a, Some("🔬"));
    browser.set_split_weights(a, &[1., 3.]);
    let split = browser.split_for(a).unwrap().clone();
    let ids = tab_ids(&browser);
    let destination = browser.add_space("Work", 100.);
    let parent = browser.create_folder(destination, None, "Saved").unwrap();
    assert!(browser.move_folder(root, destination, Some(parent)));
    assert_eq!(browser.folder_space(nested), Some(destination));
    assert_eq!(browser.folder_path(nested), "Saved / Projects / Research");
    assert!(browser.spaces[0].pinned.is_empty());
    assert!(browser.spaces[0].splits.is_empty());
    assert_eq!(browser.spaces[0].active_tab, None);
    assert_eq!(tab_ids(&browser), ids);
    assert_eq!(browser.split_for(a).unwrap().id, split.id);
    assert_eq!(browser.split_for(a).unwrap().weights, split.weights);
    assert_eq!(browser.item_icon(a), Some("🔬"));
    assert_consistent(&browser);
    let loaded = session::from_json(&session::to_json(&browser)).unwrap();
    assert_eq!(loaded.folder_path(nested), "Saved / Projects / Research");
    assert_consistent(&loaded);
}

#[test]
fn collapse_keeps_the_focused_item_visible_without_unfolding_the_folder_and_never_auto_archives_it()
{
    let mut browser = Browser::new();
    let space = browser.space().id;
    let folder = browser.create_folder(space, None, "Project").unwrap();
    let a = pin(&mut browser, "a");
    let b = pin(&mut browser, "b");
    browser.move_item_to_folder(a, folder);
    browser.set_folder_collapsed(folder, true);
    browser.move_item_to_folder(b, folder);
    assert!(browser.folder(folder).unwrap().collapsed);
    browser.activate_tab(a, NOW);
    assert_eq!(
        browser.pinned_rows(space, false),
        vec![
            PinnedRow {
                item: PinnedItem::Folder(folder),
                depth: 0
            },
            PinnedRow {
                item: PinnedItem::Tab(a),
                depth: 1
            }
        ]
    );
    browser.new_tab("https://active", NOW);
    assert_eq!(browser.pinned_rows(space, false).len(), 1);
    browser.archive_idle(NOW + std::time::Duration::from_secs(86400));
    assert_eq!(browser.space().pinned.len(), 2);
    assert_eq!(browser.set_all_folders_collapsed(space, false), 1);
    assert_eq!(browser.pinned_rows(space, false).len(), 3);
}

#[test]
fn cyclic_moves_unknown_destinations_blank_names_and_excess_depth_are_rejected() {
    let mut browser = Browser::new();
    let space = browser.space().id;
    let root = browser.create_folder(space, None, "Root").unwrap();
    let child = browser.create_folder(space, Some(root), "Child").unwrap();
    let before = session::to_json(&browser);
    assert!(!browser.move_folder(root, space, Some(child)));
    assert!(!browser.move_folder(child, SpaceId(9999), None));
    assert!(!browser.rename_folder(root, "   "));
    assert!(browser
        .create_folder(space, Some(FolderId(9999)), "No")
        .is_none());
    assert_eq!(session::to_json(&browser), before);
    let mut parent = child;
    for index in 2..MAX_FOLDER_DEPTH {
        parent = browser
            .create_folder(space, Some(parent), &format!("Nested {index}"))
            .unwrap();
    }
    assert!(browser
        .create_folder(space, Some(parent), "Too deep")
        .is_none());
    let tab = pin(&mut browser, "deepest");
    browser.move_item_to_folder(tab, parent);
    assert_eq!(browser.folder_ancestors(tab).len(), MAX_FOLDER_DEPTH);
    assert_consistent(&browser);
}

#[test]
fn invalid_sessions_repair_cycles_orphans_duplicate_ids_and_references_without_dropping_pages() {
    let mut browser = Browser::new();
    let space = browser.space().id;
    let a = pin(&mut browser, "a");
    let b = pin(&mut browser, "b");
    let folder = browser.create_folder(space, None, "Project").unwrap();
    browser.move_item_to_folder(a, folder);
    let original_ids = tab_ids(&browser);
    let mut json: serde_json::Value = serde_json::from_str(&session::to_json(&browser)).unwrap();
    let duplicated = json["spaces"][0]["folders"][0].clone();
    json["spaces"][0]["folders"]
        .as_array_mut()
        .unwrap()
        .push(duplicated);
    json["spaces"][0]["folders"][0]["items"] = serde_json::json!([
        {"kind":"folder","id": folder.get()}, {"kind":"tab","id":a.get()},
        {"kind":"tab","id":a.get()}, {"kind":"tab","id":9999},
    ]);
    json["spaces"][0]["pinned_items"] =
        serde_json::json!([{"kind":"folder","id":folder.get()}, {"kind":"folder","id":8888}]);
    let mut loaded = session::from_json(&json.to_string()).unwrap();
    assert_eq!(tab_ids(&loaded), original_ids);
    assert!(loaded.space().pinned.iter().any(|tab| tab.id == b));
    assert_eq!(loaded.space().folders.len(), 2);
    assert_consistent(&loaded);
    let new = loaded.create_folder(space, None, "New").unwrap();
    assert!(
        new.get()
            > loaded
                .space()
                .folders
                .iter()
                .filter(|folder| folder.id != new)
                .map(|folder| folder.id.get())
                .max()
                .unwrap()
    );
}

#[test]
fn valid_folder_operation_combinations_keep_all_pages_and_round_trip_consistently() {
    for first in 0..8 {
        for second in 0..8 {
            let mut browser = Browser::new();
            let space = browser.space().id;
            let folder = browser.create_folder(space, None, "Project").unwrap();
            let a = pin(&mut browser, "a");
            let b = browser.new_tab("https://b", NOW);
            browser
                .add_split(a, b, crate::splits::Placement::Right, NOW)
                .unwrap();
            browser.move_item_to_folder(a, folder);
            let ids = tab_ids(&browser);
            for operation in [first, second] {
                match operation {
                    0 => {
                        browser.toggle_pin(a);
                    }
                    1 => {
                        browser.favorite_tab(a);
                    }
                    2 => {
                        browser.unfavorite_tab(a);
                    }
                    3 => {
                        browser.move_item_to_folder(a, folder);
                    }
                    4 => {
                        browser.separate_split(a);
                    }
                    5 => {
                        browser.close_split_pane(a, NOW);
                    }
                    6 => {
                        browser.move_item_to_pinned_root(a, space);
                    }
                    7 => {
                        browser.set_all_folders_collapsed(space, true);
                    }
                    _ => unreachable!(),
                }
                assert_eq!(tab_ids(&browser), ids, "operations {first}, {second}");
                assert_consistent(&browser);
                browser = session::from_json(&session::to_json(&browser)).unwrap();
                assert_consistent(&browser);
            }
        }
    }
}

#[test]
fn regrouping_pages_from_two_folders_keeps_the_anchor_folder_and_reparenting_keeps_children() {
    let mut browser = Browser::new();
    let space = browser.space().id;
    let left = browser.create_folder(space, None, "Left").unwrap();
    let right = browser.create_folder(space, None, "Right").unwrap();
    let a = pin(&mut browser, "a");
    let b = pin(&mut browser, "b");
    browser.move_item_to_folder(a, left);
    browser.move_item_to_folder(b, right);
    browser
        .add_split(a, b, crate::splits::Placement::Right, NOW)
        .unwrap();
    assert!(browser.folder(right).unwrap().items.is_empty());
    assert_eq!(browser.folder_ancestors(b), vec![left]);
    assert!(browser.move_folder(left, space, Some(right)));
    assert_eq!(browser.folder_ancestors(b), vec![right, left]);
    assert!(browser.rename_folder(left, "Renamed"));
    assert_eq!(browser.folder_path(left), "Right / Renamed");
    assert!(!browser.delete_empty_folder(right));
    assert!(!browser.delete_empty_folder(left));
    assert!(browser.move_folder(left, space, None));
    assert_eq!(browser.folder_ancestors(b), vec![left]);
    assert!(browser.delete_empty_folder(right));
    assert_consistent(&browser);
}

#[test]
fn old_sessions_without_folder_fields_gain_a_complete_root_order() {
    let mut browser = Browser::new();
    let a = pin(&mut browser, "a");
    let b = pin(&mut browser, "b");
    let mut json: serde_json::Value = serde_json::from_str(&session::to_json(&browser)).unwrap();
    json["spaces"][0]
        .as_object_mut()
        .unwrap()
        .remove("pinned_items");
    json["spaces"][0].as_object_mut().unwrap().remove("folders");
    let loaded = session::from_json(&json.to_string()).unwrap();
    assert_eq!(
        loaded.space().pinned_items,
        vec![PinnedItem::Tab(a), PinnedItem::Tab(b)]
    );
    assert_consistent(&loaded);
}
