use super::*;
use crate::session;
use std::time::Duration;

const NOW: SystemTime = SystemTime::UNIX_EPOCH;

fn page(browser: &mut Browser, name: &str) -> TabId {
    let tab = browser.new_tab(format!("https://{name}.example"), NOW);
    browser.set_title(tab, name.to_uppercase());
    tab
}

fn ids(tabs: &[Tab]) -> Vec<TabId> { tabs.iter().map(|tab| tab.id).collect() }

#[test]
fn adding_reordering_and_separating_preserve_page_identities_and_enforce_four_panes() {
    let mut browser = Browser::new();
    let a = page(&mut browser, "a");
    let b = page(&mut browser, "b");
    let c = page(&mut browser, "c");
    let d = page(&mut browser, "d");
    let e = page(&mut browser, "e");
    let split = browser.add_split(a, b, Placement::Right, NOW).unwrap();
    assert_eq!(browser.active_split().unwrap().panes, vec![a, b]);
    assert_eq!(browser.active_tab().unwrap().id, b);
    browser.add_split(b, c, Placement::Below, NOW).unwrap();
    browser.add_split(b, d, Placement::Above, NOW).unwrap();
    assert_eq!(browser.active_split().unwrap().axis, Axis::Vertical);
    assert_eq!(browser.active_split().unwrap().panes, vec![a, d, b, c]);
    assert_eq!(browser.add_split(a, e, Placement::Right, NOW), Err(SplitError::LimitReached));
    assert_eq!(browser.all_tabs().count(), 5);
    assert!(browser.split_for(e).is_none());
    assert!(browser.set_split_axis(d, Axis::Horizontal));
    assert!(browser.set_split_weights(d, &[1., 2., 3., 4.]));
    assert!(!browser.set_split_weights(d, &[f32::NAN, 1., 1., 1.]));
    assert!(browser.move_split_pane(d, 1));
    assert_eq!(browser.active_split().unwrap().panes, vec![a, b, d, c]);
    assert_eq!(browser.active_split().unwrap().weights[2], 0.2);
    assert_eq!(browser.active_split().unwrap().id, split);
    browser.rename_tab(d, "Comparison");
    assert_eq!(browser.item_title(a), "Comparison");
    assert_eq!(browser.all_tabs().find(|tab| tab.id == d).unwrap().title, "D");
    browser.activate_sidebar_item(a, NOW);
    assert_eq!(browser.active_tab().unwrap().id, d);
    browser.focus_split_pane(0, NOW);
    browser.cycle_split_pane(-1, NOW);
    assert_eq!(browser.active_tab().unwrap().id, c);
    assert!(browser.separate_split_pane(d));
    assert!(browser.split_for(d).is_none());
    assert_eq!(browser.split_for(a).unwrap().panes, vec![a, b, c]);
    assert!(browser.separate_split(a));
    assert!(browser.space().splits.is_empty());
    assert_eq!(browser.all_tabs().count(), 5);
    assert_eq!(browser.item_title(a), "A");
}

#[test]
fn a_pinned_split_keeps_its_sidebar_position_and_resets_all_original_pages_on_close() {
    let mut browser = Browser::new();
    let first = page(&mut browser, "first"); browser.toggle_pin(first);
    let a = page(&mut browser, "a"); browser.toggle_pin(a);
    let last = page(&mut browser, "last"); browser.toggle_pin(last);
    let b = page(&mut browser, "b");
    browser.add_split(a, b, Placement::Right, NOW).unwrap();
    assert_eq!(ids(&browser.space().pinned), vec![first, a, b, last]);
    assert!(browser.space().today.is_empty());
    browser.rename_tab(b, "Reference");
    browser.navigate(a, "https://a.example/away");
    browser.navigate(b, "https://b.example/away");
    assert_eq!(browser.close_tab(b, NOW), Some(Closed::Unloaded));
    assert_eq!(browser.space().active_tab, Some(last));
    assert!(browser.archive.is_empty());
    assert_eq!(browser.all_tabs().find(|tab| tab.id == a).unwrap().url, "https://a.example");
    assert_eq!(browser.all_tabs().find(|tab| tab.id == b).unwrap().url, "https://b.example");
    assert_eq!(browser.item_title(a), "Reference");
    browser.activate_sidebar_item(a, NOW);
    assert_eq!(browser.active_tab().unwrap().id, b);
    browser.toggle_pin(b);
    assert_eq!(ids(&browser.space().today), vec![a, b]);
    assert!(browser.space().today.iter().all(|tab| tab.pinned_url.is_none()));
    assert_eq!(browser.space().active_tab, Some(b));
}

#[test]
fn split_favorites_are_one_shared_item_and_move_between_spaces_as_a_unit() {
    let mut browser = Browser::new();
    let personal = browser.space().id;
    let a = page(&mut browser, "a");
    let b = page(&mut browser, "b");
    let split = browser.add_split(a, b, Placement::Left, NOW).unwrap();
    browser.rename_tab(a, "Project");
    browser.set_tab_muted(a, true);
    assert!(browser.favorite_tab(a));
    assert_eq!(browser.favorite_count(), 1);
    assert_eq!(browser.favorites.len(), 2);
    let work = browser.add_space("Work", 20.);
    browser.switch_space(1);
    browser.activate_sidebar_item(a, NOW);
    assert_eq!(browser.active_tab().unwrap().id, b);
    assert!(browser.unfavorite_tab(b));
    assert!(browser.spaces[0].active_tab.is_none());
    assert_eq!(browser.space().splits[0].id, split);
    assert_eq!(browser.favorites.len(), 0);
    assert_eq!(browser.space().today.len(), 2);
    assert!(browser.move_tab(b, personal, Section::Pinned));
    assert_eq!(browser.space().id, work, "moving does not switch Spaces on its own");
    assert!(browser.space().splits.is_empty());
    assert!(browser.space().active_tab.is_none());
    browser.switch_space(0);
    assert_eq!(browser.space().splits[0].id, split);
    assert_eq!(browser.item_title(b), "Project");
    assert!(browser.all_tabs().find(|tab| tab.id == a).unwrap().muted);
    assert_eq!(browser.all_tabs().count(), 2);
    let restored = session::from_json(&session::to_json(&browser)).unwrap();
    assert_eq!(restored.space().splits[0].id, split);
    assert_eq!(restored.space().pinned.len(), 2);
}

#[test]
fn archive_and_restore_keep_the_group_layout_name_focus_and_unique_ids() {
    let mut browser = Browser::new();
    let a = page(&mut browser, "a");
    let b = page(&mut browser, "b");
    let split = browser.add_split(a, b, Placement::Below, NOW).unwrap();
    browser.set_split_weights(a, &[3., 1.]);
    browser.rename_tab(b, "Saved comparison");
    assert_eq!(browser.close_tab(b, NOW), Some(Closed::Archived));
    assert_eq!(browser.archive.len(), 1);
    assert_eq!(browser.archive[0].tab.id, a);
    assert_eq!(browser.archive[0].split.as_ref().unwrap().panes[0].id, b);
    assert!(browser.space().splits.is_empty());
    assert_eq!(browser.all_tabs().count(), 0);
    let mut json: serde_json::Value = serde_json::from_str(&session::to_json(&browser)).unwrap();
    json["next_id"] = 1.into();
    let mut restored = session::from_json(&json.to_string()).unwrap();
    assert!(page(&mut restored, "new").get() > split.get());
    assert_eq!(restored.restore_last_archived(NOW), Some(a));
    assert_eq!(restored.active_tab().unwrap().id, b);
    assert_eq!(restored.item_title(a), "Saved comparison");
    assert_eq!(restored.active_split().unwrap().axis, Axis::Vertical);
    assert_eq!(restored.active_split().unwrap().weights, vec![0.75, 0.25]);
    assert_eq!(restored.all_tabs().count(), 3);
}

#[test]
fn clear_today_archives_one_entry_per_split_and_preserves_pins_favorites_and_other_spaces() {
    let mut browser = Browser::new();
    let a = page(&mut browser, "a");
    let b = page(&mut browser, "b");
    browser.add_split(a, b, Placement::Right, NOW).unwrap();
    let pinned = page(&mut browser, "pinned"); browser.toggle_pin(pinned);
    page(&mut browser, "today");
    let favorite = page(&mut browser, "favorite"); browser.favorite_tab(favorite);
    browser.add_space("Work", 20.);
    browser.switch_space(1);
    page(&mut browser, "work");
    browser.switch_space(0);
    assert_eq!(browser.clear_today(NOW), 2);
    assert_eq!(browser.archive.len(), 2);
    assert_eq!(browser.archive.iter().filter(|entry| entry.split.is_some()).count(), 1);
    assert_eq!(browser.space().pinned[0].id, pinned);
    assert_eq!(browser.space().active_tab, Some(favorite));
    assert_eq!(browser.favorites.len(), 1);
    assert_eq!(browser.spaces[1].today.len(), 1);
    assert!(browser.space().splits.is_empty());
}

#[test]
fn auto_archive_keeps_active_splits_and_archives_idle_groups_atomically_in_each_space() {
    let mut browser = Browser::new();
    let a = page(&mut browser, "a");
    let b = page(&mut browser, "b");
    browser.add_split(a, b, Placement::Right, NOW).unwrap();
    browser.add_space("Work", 20.);
    browser.switch_space(1);
    let c = page(&mut browser, "c");
    let d = page(&mut browser, "d");
    browser.add_split(c, d, Placement::Right, NOW).unwrap();
    let foreground = page(&mut browser, "foreground");
    browser.switch_space(0);
    let later = NOW + Duration::from_secs(13 * 3600);
    assert_eq!(browser.archive_idle(later), 1);
    assert_eq!(browser.active_space, 0);
    assert!(browser.active_split().is_some(), "all panes of the active split are protected");
    assert_eq!(browser.spaces[1].active_tab, Some(foreground));
    assert_eq!(browser.archive[0].reason, ArchiveReason::Automatic);
    assert!(browser.archive[0].split.is_some());
    page(&mut browser, "leave");
    assert_eq!(browser.archive_idle(later), 1);
    assert_eq!(browser.archive.len(), 2);
}

#[test]
fn closing_a_pane_promotes_the_anchor_and_collapses_a_one_page_split_without_losing_pages() {
    let mut browser = Browser::new();
    let a = page(&mut browser, "a");
    let b = page(&mut browser, "b");
    let c = page(&mut browser, "c");
    let split = browser.add_split(a, b, Placement::Right, NOW).unwrap();
    browser.add_split(b, c, Placement::Right, NOW).unwrap();
    assert!(browser.close_split_pane(a, NOW));
    assert_eq!(browser.active_split().unwrap().id, split);
    assert_eq!(browser.active_split().unwrap().anchor, b);
    assert_eq!(browser.active_split().unwrap().panes, vec![b, c]);
    assert!(browser.close_split_pane(c, NOW));
    assert!(browser.active_split().is_none());
    assert_eq!(browser.active_tab().unwrap().id, b);
    assert_eq!(browser.archive.len(), 2);
    assert!(browser.archive.iter().all(|entry| entry.split.is_none()));
    assert_eq!(browser.restore_last_archived(NOW), Some(c));
    assert!(browser.split_for(c).is_none());
    assert_eq!(browser.all_tabs().count(), 2);
}

#[test]
fn favorite_limit_counts_groups_and_blocks_separation_without_discarding_tabs() {
    let mut browser = Browser::new();
    let a = page(&mut browser, "a");
    let b = page(&mut browser, "b");
    browser.add_split(a, b, Placement::Right, NOW).unwrap();
    browser.favorite_tab(a);
    for index in 1..crate::favorites::MAX_FAVORITES { let tab = page(&mut browser, &format!("app{index}")); assert!(browser.favorite_tab(tab)); }
    assert_eq!(browser.favorite_count(), 12);
    assert_eq!(browser.favorites.len(), 13);
    assert!(!browser.separate_split(a));
    assert!(!browser.separate_split_pane(b));
    assert_eq!(browser.favorites.len(), 13);
    browser.activate_tab(b, NOW);
    browser.add_space("Work", 20.);
    browser.switch_space(1);
    browser.activate_tab(b, NOW);
    assert!(browser.close_split_pane(b, NOW));
    assert!(browser.spaces.iter().all(|space| space.active_tab == Some(a)));
    assert_eq!(browser.favorite_count(), 12);
    assert_eq!(browser.favorites.len(), 12);
    assert!(browser.favorite_splits.is_empty());
}

#[test]
fn sidebar_order_treats_each_split_as_one_item_and_recalls_its_last_pane() {
    let mut browser = Browser::new();
    let a = page(&mut browser, "a");
    let b = page(&mut browser, "b");
    browser.add_split(a, b, Placement::Right, NOW).unwrap();
    let c = page(&mut browser, "c");
    browser.activate_item(Some(1), NOW);
    assert_eq!(browser.active_tab().unwrap().id, b);
    browser.cycle_tab(1, NOW);
    assert_eq!(browser.active_tab().unwrap().id, c);
    browser.cycle_tab(-1, NOW);
    assert_eq!(browser.active_tab().unwrap().id, b);
    browser.favorite_tab(a);
    browser.activate_item(Some(1), NOW);
    assert_eq!(browser.active_tab().unwrap().id, c);
    browser.activate_item(Some(0), NOW);
    assert_eq!(browser.active_tab().unwrap().id, b);
}

#[test]
fn session_repair_keeps_pages_when_groups_are_invalid_and_repairs_focus_and_weights() {
    let mut browser = Browser::new();
    let a = page(&mut browser, "a");
    let b = page(&mut browser, "b");
    browser.add_split(a, b, Placement::Right, NOW).unwrap();
    browser.spaces[0].splits[0].weights.clear();
    browser.spaces[0].splits[0].active = TabId(999);
    let restored = session::from_json(&session::to_json(&browser)).unwrap();
    assert_eq!(restored.space().splits[0].active, a);
    assert_eq!(restored.space().splits[0].weights, vec![0.5, 0.5]);
    browser.spaces[0].splits[0].panes.push(TabId(999));
    let restored = session::from_json(&session::to_json(&browser)).unwrap();
    assert!(restored.space().splits.is_empty());
    assert_eq!(restored.all_tabs().count(), 2);
}

#[test]
fn resetting_and_replacing_pinned_urls_apply_to_the_entire_split() {
    let mut browser = Browser::new();
    let a = page(&mut browser, "a"); browser.toggle_pin(a);
    let b = page(&mut browser, "b");
    browser.add_split(a, b, Placement::Right, NOW).unwrap();
    browser.navigate(a, "https://a.example/new");
    browser.navigate(b, "https://b.example/new");
    assert!(browser.replace_pinned_url(b));
    browser.navigate(a, "https://a.example/away");
    browser.navigate(b, "https://b.example/away");
    assert!(browser.reset_pinned_tab(a));
    assert_eq!(browser.all_tabs().find(|tab| tab.id == a).unwrap().url, "https://a.example/new");
    assert_eq!(browser.all_tabs().find(|tab| tab.id == b).unwrap().url, "https://b.example/new");
    assert_eq!(browser.space().active_tab, Some(b));
    browser.rename_tab(b, "Group name");
    assert!(browser.reset_tab_name(a));
    assert!(!browser.item_has_custom_name(b));
}

fn check_invariants(browser: &Browser, expected: &HashSet<TabId>) {
    let mut open = HashMap::new();
    for tab in &browser.favorites { assert!(open.insert(tab.id, (0, 0)).is_none()); }
    for (index, space) in browser.spaces.iter().enumerate() {
        for tab in &space.pinned { assert!(open.insert(tab.id, (index + 1, 1)).is_none()); }
        for tab in &space.today { assert!(open.insert(tab.id, (index + 1, 2)).is_none()); }
        if let Some(active) = space.active_tab {
            assert!(browser.favorites.iter().any(|tab| tab.id == active) || space.tabs().any(|tab| tab.id == active), "dangling active tab: {active:?}");
        }
    }
    let mut grouped = HashSet::new();
    let mut group_ids = HashSet::new();
    for (scope, views) in std::iter::once((0, &browser.favorite_splits)).chain(browser.spaces.iter().enumerate().map(|(index, space)| (index + 1, &space.splits))) {
        for view in views {
            assert!(group_ids.insert(view.id));
            assert!((2..=4).contains(&view.panes.len()));
            assert!(view.panes.contains(&view.anchor) && view.panes.contains(&view.active));
            let lane = open[&view.anchor];
            assert_eq!(lane.0, scope);
            for id in &view.panes { assert!(grouped.insert(*id)); assert_eq!(open[id], lane); }
            assert_eq!(view.weights.len(), view.panes.len());
            assert!(view.weights.iter().all(|weight| weight.is_finite() && *weight > 0.));
            assert!((view.weights.iter().sum::<f32>() - 1.).abs() < 0.0001);
        }
    }
    let mut all: HashSet<_> = open.keys().copied().collect();
    for entry in &browser.archive {
        assert!(all.insert(entry.tab.id));
        if let Some(saved) = &entry.split {
            let mut archived = HashSet::from([entry.tab.id]);
            for tab in &saved.panes { assert!(all.insert(tab.id)); assert!(archived.insert(tab.id)); }
            assert_eq!(saved.view.anchor, entry.tab.id);
            assert_eq!(saved.view.panes.iter().copied().collect::<HashSet<_>>(), archived);
        }
    }
    assert_eq!(&all, expected, "a page disappeared or its identity changed");
    assert!(browser.favorite_count() <= crate::favorites::MAX_FAVORITES);
}

#[test]
fn combinations_of_group_operations_preserve_every_page_and_valid_scope_and_focus() {
    let mut initial = Browser::new();
    let personal = initial.space().id;
    let a = page(&mut initial, "a"); let b = page(&mut initial, "b");
    initial.add_split(a, b, Placement::Right, NOW).unwrap();
    let work = initial.add_space("Work", 20.); initial.switch_space(1);
    let c = page(&mut initial, "c"); let d = page(&mut initial, "d");
    initial.add_split(c, d, Placement::Below, NOW).unwrap();
    let favorite = page(&mut initial, "favorite"); initial.favorite_tab(favorite);
    initial.switch_space(0);
    let expected = HashSet::from([a, b, c, d, favorite]);
    let snapshot = session::to_json(&initial);
    let apply = |browser: &mut Browser, operation: usize| {
        match operation {
            0 => browser.toggle_pin(a),
            1 => browser.toggle_pin(c),
            2 => { browser.favorite_tab(a); }
            3 => { browser.favorite_tab(c); }
            4 => { browser.unfavorite_tab(a); }
            5 => { browser.unfavorite_tab(favorite); }
            6 => { browser.move_tab(a, work, Section::Today); }
            7 => { browser.move_tab(c, personal, Section::Pinned); }
            8 => { browser.close_tab(a, NOW); }
            9 => { browser.close_tab(c, NOW); }
            10 => { browser.close_tab(favorite, NOW); }
            11 => { browser.close_split_pane(a, NOW); }
            12 => { browser.close_split_pane(d, NOW); }
            13 => { browser.separate_split(a); }
            14 => { browser.separate_split_pane(b); }
            15 => { browser.restore_last_archived(NOW); }
            16 => { browser.clear_today(NOW); }
            17 => { browser.archive_idle(NOW + Duration::from_secs(13 * 3600)); }
            18 => browser.switch_space(1),
            19 => browser.switch_space(0),
            20 => { browser.add_split(a, c, Placement::Right, NOW).ok(); }
            _ => unreachable!(),
        }
    };
    for first in 0..21 {
        for second in 0..21 {
            let mut browser = session::from_json(&snapshot).unwrap();
            apply(&mut browser, first);
            check_invariants(&browser, &expected);
            apply(&mut browser, second);
            check_invariants(&browser, &expected);
            let restored = session::from_json(&session::to_json(&browser)).unwrap();
            check_invariants(&restored, &expected);
        }
    }
}

#[test]
fn duplicate_group_ids_in_a_session_are_repaired_without_losing_either_group() {
    let mut browser = Browser::new();
    let a = page(&mut browser, "a"); let b = page(&mut browser, "b");
    browser.add_split(a, b, Placement::Right, NOW).unwrap();
    let c = page(&mut browser, "c"); let d = page(&mut browser, "d");
    browser.add_split(c, d, Placement::Right, NOW).unwrap();
    browser.spaces[0].splits[1].id = browser.spaces[0].splits[0].id;
    let restored = session::from_json(&session::to_json(&browser)).unwrap();
    assert_eq!(restored.space().splits.len(), 2);
    assert_ne!(restored.space().splits[0].id, restored.space().splits[1].id);
    check_invariants(&restored, &HashSet::from([a, b, c, d]));
}

#[test]
fn duplicating_today_and_pinned_items_preserves_labels_original_urls_and_independent_identity() {
    let mut browser = Browser::new();
    let a = page(&mut browser, "a");
    browser.rename_tab(a, "Named page");
    browser.set_tab_muted(a, true);
    let later = NOW + Duration::from_secs(60);
    let copy = browser.duplicate_item(a, later).unwrap();
    assert_eq!(ids(&browser.space().today), vec![a, copy]);
    assert_eq!(browser.active_tab().unwrap().custom_title.as_deref(), Some("Named page"));
    assert_eq!(browser.active_tab().unwrap().last_active, later);
    assert!(browser.active_tab().unwrap().muted);
    browser.toggle_pin(a);
    browser.navigate(a, "https://a.example/current");
    let pinned_copy = browser.duplicate_item(a, later).unwrap();
    assert_eq!(ids(&browser.space().pinned), vec![a, pinned_copy]);
    assert_eq!(browser.active_tab().unwrap().url, "https://a.example/current");
    assert_eq!(browser.active_tab().unwrap().pinned_url.as_deref(), Some("https://a.example"));
    browser.close_tab(pinned_copy, later);
    assert_eq!(browser.space().pinned[1].url, "https://a.example");
    assert_eq!(browser.space().pinned[0].url, "https://a.example/current");
}

#[test]
fn duplicating_a_split_copies_all_panes_order_focus_axis_weights_and_group_name() {
    let mut browser = Browser::new();
    let a = page(&mut browser, "a");
    let b = page(&mut browser, "b");
    let c = page(&mut browser, "c");
    browser.add_split(a, b, Placement::Right, NOW).unwrap();
    browser.add_split(b, c, Placement::Below, NOW).unwrap();
    browser.set_split_weights(a, &[1., 2., 3.]);
    browser.rename_tab(a, "Research comparison");
    browser.activate_tab(b, NOW);
    let original = browser.active_split().unwrap().clone();
    let copy = browser.duplicate_item(a, NOW).unwrap();
    let duplicate = browser.active_split().unwrap().clone();
    assert_ne!(duplicate.id, original.id);
    assert_eq!(duplicate.axis, original.axis);
    assert_eq!(duplicate.weights, original.weights);
    assert_eq!(duplicate.title, original.title);
    assert_eq!(duplicate.active, copy);
    assert_eq!(duplicate.active, duplicate.panes[1]);
    assert_eq!(duplicate.anchor, duplicate.panes[0]);
    assert!(duplicate.panes.iter().all(|id| !original.panes.contains(id)));
    for (source, copy) in original.panes.iter().zip(&duplicate.panes) {
        let tab = |id: &TabId| browser.all_tabs().find(|tab| tab.id == *id).unwrap();
        assert_eq!(tab(source).url, tab(copy).url);
        assert_eq!(tab(source).title, tab(copy).title);
    }
    browser.close_tab(copy, NOW);
    assert_eq!(browser.space().splits.len(), 1);
    assert_eq!(browser.space().splits[0].id, original.id);
    let restored = session::from_json(&session::to_json(&browser)).unwrap();
    assert_eq!(restored.archive[0].split.as_ref().unwrap().view.id, duplicate.id);
    assert_eq!(restored.all_tabs().count(), 3);
}

#[test]
fn favorite_duplication_is_one_dock_item_and_capacity_failure_is_atomic() {
    let mut browser = Browser::new();
    let a = page(&mut browser, "a");
    let b = page(&mut browser, "b");
    browser.add_split(a, b, Placement::Right, NOW).unwrap();
    browser.favorite_tab(a);
    browser.duplicate_item(a, NOW).unwrap();
    assert_eq!(browser.favorite_count(), 2);
    assert_eq!(browser.favorites.len(), 4);
    while browser.favorite_count() < crate::favorites::MAX_FAVORITES {
        let id = page(&mut browser, "extra");
        browser.favorite_tab(id);
    }
    let before = session::to_json(&browser);
    assert!(browser.duplicate_item(a, NOW).is_none());
    assert_eq!(session::to_json(&browser), before);
    assert!(browser.duplicate_item(TabId(u64::MAX), NOW).is_none());
    assert_eq!(session::to_json(&browser), before);
}
