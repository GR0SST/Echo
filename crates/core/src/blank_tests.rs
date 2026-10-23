//! Blank-window prerequisites only; persistence and browser routing are separate.
use crate::{session, splits::Placement, Browser, Closed, Section, DEFAULT_ARCHIVE_AFTER};
use std::time::{Duration, SystemTime};

const NOW: SystemTime = SystemTime::UNIX_EPOCH;

#[test]
fn blank_is_fresh_normal_privacy_with_one_unshown_internal_space() {
    let blank = Browser::new_blank();
    assert!(blank.is_blank());
    assert!(!blank.is_private());
    assert!(!blank.can_edit_persistent_sidebar());
    assert_eq!(blank.spaces.len(), 1);
    assert_eq!(blank.space().name, "Space 1");
    assert!(!blank.space().show_header);
    assert!(blank.space().today.is_empty());
    assert!(blank.space().pinned.is_empty());
    assert!(blank.space().pinned_items.is_empty());
    assert!(blank.space().folders.is_empty());
    assert!(blank.favorites.is_empty());
    assert!(blank.favorite_splits.is_empty());
    assert!(blank.archive.is_empty());
    assert!(!Browser::new().is_blank());
    assert!(Browser::new().can_edit_persistent_sidebar());
    assert!(!Browser::new_private().is_blank());
    assert!(Browser::new_private().can_edit_persistent_sidebar());
}

#[test]
fn equal_tab_ids_in_two_blank_models_do_not_share_state_or_undo() {
    let mut a = Browser::new_blank();
    let mut b = Browser::new_blank();
    let a_tab = a.new_tab("https://a.example", NOW);
    let b_tab = b.new_tab("https://b.example", NOW);
    assert_eq!(a_tab, b_tab);
    a.navigate(a_tab, "https://a.example/changed");
    assert_eq!(b.active_tab().unwrap().url, "https://b.example");
    assert_eq!(a.close_tab(a_tab, NOW), Some(Closed::Archived));
    assert!(!b.undo_sidebar(NOW));
    assert!(b.archive.is_empty());
    assert!(a.undo_sidebar(NOW));
    assert_eq!(a.active_tab().unwrap().url, "https://a.example/changed");
    assert!(a.redo_sidebar(NOW));
    assert_eq!(a.restore_last_archived(NOW), Some(a_tab));
    assert_eq!(b.active_tab().unwrap().id, b_tab);
    assert_eq!(b.all_tabs().count(), 1);
}

#[test]
fn blank_rejects_pinning_favorites_folders_and_split_group_moves_without_side_effects() {
    let mut blank = Browser::new_blank();
    let space = blank.space().id;
    let single = blank.new_tab("https://single.example", NOW);
    let a = blank.new_tab("https://a.example", NOW);
    let b = blank.new_tab("https://b.example", NOW);
    blank.add_split(a, b, Placement::Right, NOW).unwrap();
    let before = session::to_json(&blank);
    for tab in [single, a, b] {
        blank.toggle_pin(tab);
        assert!(!blank.move_tab(tab, space, Section::Pinned));
        assert!(!blank.move_item_to_pinned_root(tab, space));
        assert!(!blank.favorite_tab(tab));
    }
    // Exercise the internal group routes independently of their public wrappers.
    blank.toggle_split_pin(b);
    assert!(!blank.move_split(b, space, Section::Pinned));
    assert!(!blank.favorite_split(b));
    assert_eq!(blank.create_folder(space, None, "Folder"), None);
    assert!(!blank.undo_sidebar(NOW));
    assert!(!blank.redo_sidebar(NOW));
    assert_eq!(session::to_json(&blank), before);
    assert_eq!(blank.related_tabs(a), vec![a, b]);
    assert!(blank.space().today.iter().all(|tab| tab.pinned_url.is_none()));
}

#[test]
fn blank_folder_routes_reject_existing_folder_ids_without_mutation() {
    let mut normal = Browser::new();
    let folder = normal.create_folder(normal.space().id, None, "Normal folder").unwrap();
    let normal_before = session::to_json(&normal);
    let mut blank = Browser::new_blank();
    let tab = blank.new_tab("https://blank.example", NOW);
    let space = blank.space().id;
    let before = session::to_json(&blank);
    assert!(!blank.move_item_to_folder(tab, folder));
    assert!(!blank.can_move_folder(folder, space, None));
    assert!(!blank.move_folder(folder, space, None));
    assert!(!blank.rename_folder(folder, "Changed"));
    assert!(!blank.set_folder_icon(folder, Some("A")));
    assert!(!blank.set_folder_collapsed(folder, true));
    assert_eq!(blank.set_all_folders_collapsed(space, true), 0);
    assert!(!blank.delete_empty_folder(folder));
    assert_eq!(blank.duplicate_folder(folder, NOW), None);
    assert_eq!(blank.folder_into_space(folder, NOW), None);
    assert_eq!(session::to_json(&blank), before);
    assert_eq!(session::to_json(&normal), normal_before);
}

#[test]
fn blank_today_reordering_split_and_local_archive_restore_preserve_other_models() {
    let mut normal = Browser::new();
    let normal_tab = normal.new_tab("https://normal.example", NOW);
    normal.close_tab(normal_tab, NOW);
    let normal_before = session::to_json(&normal);
    let mut other = Browser::new_blank();
    other.new_tab("https://other.example", NOW);
    let other_before = session::to_json(&other);
    let mut blank = Browser::new_blank();
    let a = blank.new_tab("https://a.example", NOW);
    let b = blank.new_tab("https://b.example", NOW);
    let c = blank.new_tab("https://c.example", NOW);
    assert!(blank.move_tab_relative(a, c, false));
    assert_eq!(blank.space().today.iter().map(|tab| tab.id).collect::<Vec<_>>(), vec![a, c, b]);
    let split = blank.add_split(a, b, Placement::Right, NOW).unwrap();
    assert!(blank.move_tab_relative(b, c, true));
    assert_eq!(blank.space().today.iter().map(|tab| tab.id).collect::<Vec<_>>(), vec![c, a, b]);
    assert!(blank.close_split_pane(b, NOW));
    assert!(blank.undo_sidebar(NOW));
    assert_eq!(blank.split_for(a).unwrap().id, split);
    assert_eq!(blank.close_tab(a, NOW), Some(Closed::Archived));
    assert_eq!(blank.restore_last_archived(NOW), Some(a));
    assert_eq!(blank.split_for(a).unwrap().panes, vec![a, b]);
    assert_eq!(session::to_json(&normal), normal_before);
    assert_eq!(session::to_json(&other), other_before);
    assert_eq!(normal.restore_last_archived(NOW), Some(normal_tab));
    assert!(other.archive.is_empty());
}

#[test]
fn blank_keeps_normal_history_and_configurable_auto_archive_until_browser_policy_decides() {
    let mut blank = Browser::new_blank();
    assert_eq!(blank.archive_after, Some(DEFAULT_ARCHIVE_AFTER));
    blank.record_visit("https://visited.example", NOW);
    assert_eq!(blank.history.len(), 1);
    let idle = blank.new_tab("https://idle.example", NOW);
    blank.new_tab("https://selected.example", NOW);
    blank.archive_after = None;
    let later = NOW + DEFAULT_ARCHIVE_AFTER + Duration::from_secs(1);
    assert_eq!(blank.archive_idle(later), 0);
    blank.archive_after = Some(DEFAULT_ARCHIVE_AFTER);
    assert_eq!(blank.archive_idle(later), 1);
    assert_eq!(blank.restore_last_archived(later), Some(idle));
}

#[test]
fn blank_runtime_kind_is_not_a_session_persistence_contract() {
    let blank = Browser::new_blank();
    let json = session::to_json(&blank);
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert!(value.get("blank").is_none());
    assert!(!session::from_json(&json).unwrap().is_blank());
}

#[test]
fn normal_and_private_persistent_sidebar_mutations_remain_available() {
    for mut browser in [Browser::new(), Browser::new_private()] {
        let a = browser.new_tab("https://a.example", NOW);
        browser.toggle_pin(a);
        assert_eq!(browser.space().pinned[0].id, a);
        assert!(browser.favorite_tab(a));
        assert!(browser.unfavorite_tab(a));
        let folder = browser.create_folder(browser.space().id, None, "Folder").unwrap();
        assert!(browser.move_item_to_folder(a, folder));
        let b = browser.new_tab("https://b.example", NOW);
        browser.add_split(a, b, Placement::Right, NOW).unwrap();
        assert!(browser.favorite_tab(b));
    }
}
