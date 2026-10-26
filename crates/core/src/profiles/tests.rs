use super::*;
use crate::{Section, session, splits::Placement};
use std::time::SystemTime;
const NOW: SystemTime = SystemTime::UNIX_EPOCH;

fn second_profile(browser: &mut Browser) -> (UserProfileId, SpaceId) {
    let id = browser.create_user_profile("Work").unwrap();
    let space = browser.add_space("Work Space", 100.);
    browser.assign_space_user_profile(space, id).unwrap();
    (id, space)
}

#[test]
fn favorites_share_ids_only_inside_their_owner_and_global_inventory_includes_every_profile() {
    let mut browser = Browser::new();
    let default = browser.default_user_profile_id();
    let favorite = browser.new_tab("https://same.example", NOW);
    browser.favorite_tab(favorite);
    browser.add_space("Same profile", 100.);
    browser.switch_space(1);
    browser.activate_tab(favorite, NOW);
    assert_eq!(browser.active_tab().unwrap().id, favorite);
    let (other, _) = second_profile(&mut browser);
    browser.switch_space(2);
    browser.activate_tab(favorite, NOW);
    assert!(browser.active_tab().is_none());
    assert!(!browser.favorite_tab(favorite));
    assert!(!browser.is_favorite(favorite));
    let independent = browser.new_tab("https://same.example", NOW);
    assert!(browser.favorite_tab(independent));
    assert_ne!(favorite, independent);
    assert_eq!(browser.all_tabs().count(), 2);
    assert_eq!(browser.user_profile_id_for_tab(independent), Some(other));
    assert_eq!(
        browser.user_profile(default).unwrap().favorites[0].id,
        favorite
    );
    let loaded = session::from_json(&session::to_json(&browser)).unwrap();
    assert_eq!(loaded.all_tabs().count(), 2);
    assert_eq!(loaded.active_tab().unwrap().id, independent);
}

#[test]
fn background_history_and_icons_use_explicit_owner_even_when_another_profile_is_selected() {
    let mut browser = Browser::new();
    let default = browser.default_user_profile_id();
    let (other, _) = second_profile(&mut browser);
    browser.switch_space(1);
    browser.record_visit(default, "https://same.example", NOW);
    browser.update_history_title(default, "https://same.example", "Default title");
    browser.record_visit(other, "https://same.example", NOW);
    let png = [b"\x89PNG\r\n\x1a\n".as_slice(), &[1]].concat();
    assert!(browser.cache_favicon(default, "https://same.example", png.clone()));
    assert!(browser.favicon(other, "https://same.example").is_none());
    assert_eq!(
        browser.favicon(default, "https://same.example"),
        Some(png.as_slice())
    );
    browser.delete_history_entry(other, "https://same.example");
    assert!(browser.user_profile(other).unwrap().history.is_empty());
    assert_eq!(
        browser.user_profile(default).unwrap().history[0].title,
        "Default title"
    );
    let missing = UserProfileId(99999);
    browser.record_visit(missing, "https://wrong.example", NOW);
    browser.update_history_title(missing, "https://same.example", "Wrong");
    browser.delete_history_entry(missing, "https://same.example");
    assert!(!browser.cache_favicon(missing, "https://same.example", png));
    assert_eq!(browser.user_profile(default).unwrap().history.len(), 1);
    assert_eq!(
        browser.user_profile(default).unwrap().history[0].title,
        "Default title"
    );
}

#[test]
fn assignment_preserves_space_order_tab_ids_and_reports_context_recreation_without_foreign_selection()
 {
    let mut browser = Browser::new();
    let favorite = browser.new_tab("https://favorite", NOW);
    browser.favorite_tab(favorite);
    let local = browser.new_tab("https://local", NOW);
    let source = browser.space().id;
    browser.activate_tab(favorite, NOW);
    let selection = browser.window_selection();
    let other = browser.create_user_profile("Work").unwrap();
    let order: Vec<_> = browser.spaces.iter().map(|space| space.id).collect();
    let transition = browser
        .assign_space_user_profile(source, other)
        .unwrap()
        .unwrap();
    assert_eq!(transition.from, browser.default_user_profile_id());
    assert_eq!(transition.to, other);
    assert_eq!(transition.recreate_tabs, vec![local]);
    assert_eq!(
        browser
            .spaces
            .iter()
            .map(|space| space.id)
            .collect::<Vec<_>>(),
        order
    );
    assert!(
        !browser
            .window_projection(&selection)
            .active_tab()
            .is_some_and(|tab| tab.id == favorite)
    );
    assert!(browser.active_tab().is_none());
    assert_eq!(
        browser.assign_space_user_profile(source, other).unwrap(),
        None
    );
    assert!(browser.all_tabs().any(|tab| tab.id == favorite));
    assert_eq!(browser.user_profile_id_for_tab(local), Some(other));
}

#[test]
fn projections_validate_each_remembered_space_against_its_own_profile() {
    let mut browser = Browser::new();
    let a = browser.new_tab("https://a", NOW);
    browser.favorite_tab(a);
    let (other, _) = second_profile(&mut browser);
    browser.switch_space(1);
    let b = browser.new_tab("https://b", NOW);
    browser.favorite_tab(b);
    let selection = browser.window_selection();
    browser.switch_space(0);
    let projection = browser.window_projection(&selection);
    assert_eq!(projection.spaces[0].active_tab, Some(a));
    assert_eq!(projection.spaces[1].active_tab, Some(b));
    assert_eq!(projection.active_user_profile_id(), other);
    assert_eq!(projection.active_tab().unwrap().id, b);
}

#[test]
fn profile_guards_names_and_deleted_identity_watermark_survive_reload() {
    let mut browser = Browser::new();
    assert_eq!(
        browser.create_user_profile(" "),
        Err(UserProfileError::EmptyName)
    );
    let default = browser.default_user_profile_id();
    assert_eq!(
        browser.delete_user_profile(default),
        Err(UserProfileError::DefaultProfile)
    );
    let (other, space) = second_profile(&mut browser);
    assert_eq!(
        browser.delete_user_profile(other),
        Err(UserProfileError::AssignedProfile)
    );
    browser.rename_user_profile(other, "Renamed").unwrap();
    assert_eq!(browser.user_profile(other).unwrap().name(), "Renamed");
    browser.assign_space_user_profile(space, default).unwrap();
    browser.delete_user_profile(other).unwrap();
    assert_eq!(
        browser.delete_user_profile(other),
        Err(UserProfileError::UnknownProfile)
    );
    let mut loaded = session::from_json(&session::to_json(&browser)).unwrap();
    let fresh = loaded.create_user_profile("New").unwrap();
    assert_ne!(fresh, other);
    assert_ne!(fresh.storage_key(), other.storage_key());
    loaded.record_visit(other, "https://deleted", NOW);
    assert!(loaded.user_profile(default).unwrap().history.is_empty());
}

#[test]
fn new_profile_data_is_empty_and_retention_initializes_from_default_without_public_interval_restrictions()
 {
    let mut browser = Browser::new();
    let default = browser.default_user_profile_id();
    browser.user_profile_mut(default).unwrap().archive_after = None;
    browser.active_user_profile_mut().empty_favorites_dismissed = true;
    browser
        .active_user_profile_mut()
        .include_favorites_in_ordering = false;
    browser.record_visit(default, "https://default", NOW);
    let favorite = browser.new_tab("https://favorite", NOW);
    browser.favorite_tab(favorite);
    let other = browser.create_user_profile("New").unwrap();
    let data = browser.user_profile(other).unwrap();
    assert!(
        data.favorites.is_empty()
            && data.favorite_splits.is_empty()
            && data.history.is_empty()
            && data.favicon_cache.is_empty()
    );
    assert!(!data.empty_favorites_dismissed);
    assert!(data.include_favorites_in_ordering);
    assert_eq!(data.archive_after, None);
}

#[test]
fn undo_favorite_scope_is_captured_and_assignment_invalidates_old_history() {
    let mut browser = Browser::new();
    let default = browser.default_user_profile_id();
    let favorite = browser.new_tab("https://favorite", NOW);
    browser.favorite_tab(favorite);
    browser.toggle_pin(favorite);
    let (other, _) = second_profile(&mut browser); // Assignment invalidates all prior entries.
    assert!(!browser.undo_sidebar(NOW));
    browser.favorite_tab(favorite);
    browser.toggle_pin(favorite);
    browser.switch_space(1);
    assert!(browser.undo_sidebar(NOW));
    assert_eq!(
        browser.user_profile(default).unwrap().favorites[0].id,
        favorite
    );
    assert!(browser.user_profile(other).unwrap().favorites.is_empty());
    assert!(browser.redo_sidebar(NOW));
    assert!(browser.user_profile(default).unwrap().favorites.is_empty());
    browser.switch_space(0);
    browser.toggle_pin(favorite);
    let space = browser.space().id;
    browser.assign_space_user_profile(space, other).unwrap();
    assert!(!browser.undo_sidebar(NOW));
}

#[test]
fn legacy_global_data_migrates_exact_ids_default_storage_and_internal_disabled_timer() {
    let mut browser = Browser::new();
    let favorite = browser.new_tab("https://favorite", NOW);
    browser.favorite_tab(favorite);
    let pane = browser.new_tab("https://pane", NOW);
    browser
        .add_split(favorite, pane, Placement::Right, NOW)
        .unwrap();
    let split = browser.split_for(favorite).unwrap().id;
    let archived = browser.new_tab("https://closed", NOW);
    browser.close_tab(archived, NOW);
    let default = browser.default_user_profile_id();
    browser.record_visit(default, "https://visited", NOW);
    browser.active_user_profile_mut().archive_after = None;
    browser.active_user_profile_mut().empty_favorites_dismissed = true;
    let mut legacy = serde_json::to_value(&browser).unwrap();
    let profile = legacy["profiles"]["1"].as_object().unwrap().clone();
    let object = legacy.as_object_mut().unwrap();
    for key in [
        "profiles",
        "default_user_profile_id",
        "next_user_profile_id",
    ] {
        object.remove(key);
    }
    for key in [
        "favorites",
        "favorite_splits",
        "history",
        "archive_after",
        "empty_favorites_dismissed",
        "favicon_cache",
        "include_favorites_in_ordering",
    ] {
        object.insert(key.into(), profile[key].clone());
    }
    for space in legacy["spaces"].as_array_mut().unwrap() {
        space.as_object_mut().unwrap().remove("user_profile_id");
    }
    for entry in legacy["archive"].as_array_mut().unwrap() {
        entry.as_object_mut().unwrap().remove("user_profile_id");
    }
    let migrated = session::from_json(&legacy.to_string()).unwrap();
    assert_eq!(migrated.default_user_profile_id().storage_key(), None);
    assert_eq!(migrated.active_user_profile().favorites[0].id, favorite);
    assert_eq!(migrated.split_for(favorite).unwrap().id, split);
    assert_eq!(migrated.archive[0].tab.id, archived);
    assert_eq!(migrated.archive[0].user_profile_id, default);
    assert_eq!(migrated.active_user_profile().archive_after, None);
    assert!(migrated.active_user_profile().empty_favorites_dismissed);
    assert_eq!(migrated.active_user_profile().history.len(), 1);
    let roundtrip = session::from_json(&session::to_json(&migrated)).unwrap();
    assert_eq!(session::to_json(&roundtrip), session::to_json(&migrated));
}

#[test]
fn malformed_versioned_registry_never_falls_back_to_default() {
    let good: serde_json::Value = serde_json::from_str(&session::to_json(&Browser::new())).unwrap();
    for key in [
        "profiles",
        "default_user_profile_id",
        "next_user_profile_id",
        "format_version",
    ] {
        let mut broken = good.clone();
        broken.as_object_mut().unwrap().remove(key);
        assert!(session::from_json(&broken.to_string()).is_err(), "{key}");
    }
    for (path, value) in [
        ("profiles", serde_json::json!({})),
        ("default_user_profile_id", 2.into()),
        ("next_user_profile_id", 1.into()),
        ("format_version", 99.into()),
    ] {
        let mut broken = good.clone();
        broken[path] = value;
        assert!(session::from_json(&broken.to_string()).is_err(), "{path}");
    }
    let mut broken = good;
    broken["spaces"][0]["user_profile_id"] = 999.into();
    assert!(session::from_json(&broken.to_string()).is_err());
}

#[test]
fn id_repair_sees_inactive_profile_favorites_splits_archived_panes_and_folders() {
    let mut browser = Browser::new();
    let (other, _) = second_profile(&mut browser);
    browser.switch_space(1);
    let a = browser.new_tab("https://a", NOW);
    let b = browser.new_tab("https://b", NOW);
    browser.add_split(a, b, Placement::Right, NOW).unwrap();
    browser.favorite_tab(a);
    let archived = browser.new_tab("https://archived", NOW);
    let archived_pane = browser.new_tab("https://archived-pane", NOW);
    browser
        .add_split(archived, archived_pane, Placement::Right, NOW)
        .unwrap();
    browser.close_tab(archived, NOW);
    let folder = browser
        .create_folder(browser.space().id, None, "Folder")
        .unwrap();
    browser.switch_space(0);
    let mut json: serde_json::Value = serde_json::from_str(&session::to_json(&browser)).unwrap();
    json["next_id"] = 1.into();
    let mut loaded = session::from_json(&json.to_string()).unwrap();
    let new = loaded.new_tab("https://new", NOW);
    assert!(new.get() > folder.get());
    assert_eq!(loaded.user_profile(other).unwrap().favorites.len(), 2);
    assert_eq!(loaded.all_tabs().count(), 3);
    assert_ne!(new, archived_pane);
}

#[test]
fn blank_captures_normal_source_identity_without_workspace_copyback_and_private_stays_ephemeral() {
    let mut browser = Browser::new();
    let (other, _) = second_profile(&mut browser);
    browser.record_visit(other, "https://source", NOW);
    let mut blank = browser.new_blank_for_user_profile(other).unwrap();
    assert!(blank.is_blank() && !blank.is_private());
    assert_eq!(blank.blank_source_user_profile_id(), Some(other));
    assert_eq!(blank.active_user_profile_id(), other);
    assert_eq!(
        blank.default_user_profile_id(),
        browser.default_user_profile_id()
    );
    assert_eq!(blank.default_user_profile_id().storage_key(), None);
    assert_eq!(
        other.storage_key(),
        Some(format!("user-profile-{}", other.0))
    );
    assert!(blank.active_user_profile().history.is_empty());
    let local = blank.new_tab("https://blank", NOW);
    assert!(!blank.favorite_tab(local));
    assert_eq!(browser.all_tabs().count(), 0);
    browser.record_visit(
        blank.blank_source_user_profile_id().unwrap(),
        "https://blank",
        NOW,
    );
    assert_eq!(browser.user_profile(other).unwrap().history.len(), 2);
    assert!(
        browser
            .user_profile(browser.default_user_profile_id())
            .unwrap()
            .history
            .is_empty()
    );
    let mut private = Browser::new_private();
    private.record_visit(private.active_user_profile_id(), "https://private", NOW);
    assert!(private.active_user_profile().history.is_empty());
    assert_eq!(private.active_user_profile().archive_after, None);
    assert_eq!(
        private.create_user_profile("Forbidden"),
        Err(UserProfileError::WindowLocalModel)
    );
    assert_eq!(
        blank.create_user_profile("Forbidden"),
        Err(UserProfileError::WindowLocalModel)
    );
}

#[test]
fn auto_archive_uses_each_spaces_policy_and_preserves_global_archive_provenance() {
    let mut browser = Browser::new();
    let default = browser.default_user_profile_id();
    let idle = browser.new_tab("https://idle", NOW);
    browser.new_tab("https://selected", NOW);
    let (other, _) = second_profile(&mut browser);
    browser.user_profile_mut(other).unwrap().archive_after = None;
    browser.switch_space(1);
    browser.new_tab("https://other-idle", NOW);
    browser.new_tab("https://other-selected", NOW);
    assert_eq!(browser.archive_idle(NOW + DEFAULT_ARCHIVE_AFTER), 1);
    assert_eq!(browser.archive[0].tab.id, idle);
    assert_eq!(browser.archive[0].user_profile_id, default);
    assert_eq!(browser.space().today.len(), 2);
}

#[test]
fn moves_refuse_silent_context_transfer_and_restore_does_not_use_reassigned_space() {
    let mut browser = Browser::new();
    let tab = browser.new_tab("https://tab", NOW);
    let (other, target) = second_profile(&mut browser);
    assert!(!browser.move_tab(tab, target, Section::Today));
    browser.close_tab(tab, NOW);
    let source = browser.space().id;
    browser.assign_space_user_profile(source, other).unwrap();
    assert_eq!(browser.restore_archived(tab, NOW), None);
    assert_eq!(browser.archive[0].tab.id, tab);
}

#[test]
fn duplicate_favorite_ids_are_repaired_per_profile_with_owning_space_selections() {
    let mut browser = Browser::new();
    let a = browser.new_tab("https://same", NOW);
    browser.favorite_tab(a);
    let (other, _) = second_profile(&mut browser);
    browser.switch_space(1);
    let b = browser.new_tab("https://same", NOW);
    browser.favorite_tab(b);
    let mut json: serde_json::Value = serde_json::from_str(&session::to_json(&browser)).unwrap();
    json["profiles"][other.0.to_string()]["favorites"][0]["id"] = a.get().into();
    json["spaces"][1]["active_tab"] = a.get().into();
    json["next_id"] = 1.into();
    let loaded = session::from_json(&json.to_string()).unwrap();
    let repaired = loaded.user_profile(other).unwrap().favorites[0].id;
    assert_ne!(repaired, a);
    assert_eq!(loaded.spaces[0].active_tab, Some(a));
    assert_eq!(loaded.spaces[1].active_tab, Some(repaired));
    assert_eq!(
        loaded
            .all_tabs()
            .map(|tab| tab.id)
            .collect::<std::collections::HashSet<_>>()
            .len(),
        2
    );
}

#[test]
fn favorite_pane_undo_uses_captured_profile_when_another_profile_is_selected() {
    let mut browser = Browser::new();
    let default = browser.default_user_profile_id();
    let (other, _) = second_profile(&mut browser);
    let a = browser.new_tab("https://a", NOW);
    let b = browser.new_tab("https://b", NOW);
    browser.add_split(a, b, Placement::Right, NOW).unwrap();
    browser.favorite_tab(a);
    assert!(browser.close_split_pane(b, NOW));
    browser.switch_space(1);
    assert!(browser.undo_sidebar(NOW));
    assert_eq!(browser.user_profile(default).unwrap().favorites.len(), 2);
    assert!(browser.user_profile(other).unwrap().favorites.is_empty());
    assert!(browser.redo_sidebar(NOW));
    assert_eq!(browser.archive[0].user_profile_id, default);
    assert!(browser.user_profile(default).unwrap().favorites.is_empty());
}

#[test]
fn assignment_invalidates_archive_undo_and_keeps_deleted_profiles_archive_provenance() {
    let mut browser = Browser::new();
    let (other, space) = second_profile(&mut browser);
    browser.switch_space(1);
    let tab = browser.new_tab("https://work", NOW);
    browser.close_tab(tab, NOW);
    let default = browser.default_user_profile_id();
    browser.assign_space_user_profile(space, default).unwrap();
    assert!(!browser.undo_sidebar(NOW));
    browser.delete_user_profile(other).unwrap();
    assert_eq!(browser.archive[0].user_profile_id, other);
    assert_eq!(browser.restore_archived(tab, NOW), None);
    let loaded = session::from_json(&session::to_json(&browser)).unwrap();
    assert_eq!(loaded.archive[0].user_profile_id, other);
    assert!(loaded.user_profile(other).is_none());
}

#[test]
fn split_id_collision_with_foreign_favorite_preserves_pinned_folder_and_root_positions() {
    use crate::folders::PinnedItem;
    for in_folder in [false, true] {
        let mut browser = Browser::new();
        let source = browser.space().id;
        let before = browser.new_tab("https://before", NOW);
        browser.toggle_pin(before);
        let a = browser.new_tab("https://a", NOW);
        let b = browser.new_tab("https://b", NOW);
        let original_split = browser.add_split(a, b, Placement::Right, NOW).unwrap();
        browser.toggle_pin(a);
        let after = browser.new_tab("https://after", NOW);
        browser.toggle_pin(after);
        let folder = if in_folder {
            let folder = browser
                .create_folder(source, None, "Keep this order")
                .unwrap();
            for tab in [before, a, after] {
                assert!(browser.move_item_to_folder(tab, folder));
            }
            Some(folder)
        } else {
            None
        };
        let expected = vec![
            PinnedItem::Tab(before),
            PinnedItem::Split(original_split),
            PinnedItem::Tab(after),
        ];
        let root_order = browser.space().pinned_items.clone();
        assert_eq!(
            folder.map_or_else(
                || root_order.clone(),
                |id| browser.folder(id).unwrap().items.clone()
            ),
            expected
        );

        let (other, _) = second_profile(&mut browser);
        browser.switch_space(1);
        let foreign = browser.new_tab("https://foreign-favorite", NOW);
        assert!(browser.favorite_tab(foreign));
        let mut json: serde_json::Value =
            serde_json::from_str(&session::to_json(&browser)).unwrap();
        json["profiles"][other.0.to_string()]["favorites"][0]["id"] = original_split.get().into();
        json["spaces"][1]["active_tab"] = original_split.get().into();
        json["next_id"] = 1.into();

        let loaded = session::from_json(&json.to_string()).unwrap();
        let repaired = loaded.split_for(a).unwrap().id;
        assert_ne!(repaired, original_split);
        assert!(!loaded.all_tabs().any(|tab| tab.id.get() == repaired.get()));
        assert_eq!(
            loaded.user_profile(other).unwrap().favorites[0].id.get(),
            original_split.get()
        );
        let expected = vec![
            PinnedItem::Tab(before),
            PinnedItem::Split(repaired),
            PinnedItem::Tab(after),
        ];
        if let Some(folder) = folder {
            assert_eq!(loaded.folder(folder).unwrap().items, expected);
            assert_eq!(loaded.spaces[0].pinned_items, root_order);
            assert!(
                !loaded.spaces[0]
                    .pinned_items
                    .contains(&PinnedItem::Split(repaired))
            );
        } else {
            assert_eq!(loaded.spaces[0].pinned_items, expected);
        }
        let roundtrip = session::from_json(&session::to_json(&loaded)).unwrap();
        assert_eq!(session::to_json(&roundtrip), session::to_json(&loaded));
    }
}

#[test]
fn unused_profile_deletion_is_atomic_and_preserves_storage_identity_watermarks() {
    let mut browser = Browser::new();
    let a = browser.create_user_profile("First unused").unwrap();
    let b = browser.create_user_profile("Second unused").unwrap();
    let default = browser.default_user_profile_id();
    let space = browser.space().id;
    browser.assign_space_user_profile(space, b).unwrap();
    let before = session::to_json(&browser);
    assert_eq!(browser.delete_unused_user_profiles(&[a,b]), Err(UserProfileError::AssignedProfile));
    assert_eq!(session::to_json(&browser), before);
    assert_eq!(browser.delete_unused_user_profiles(&[a,default]), Err(UserProfileError::DefaultProfile));
    assert_eq!(session::to_json(&browser), before);
    browser.assign_space_user_profile(space, default).unwrap();
    assert_eq!(browser.delete_unused_user_profiles(&[a,b,a]), Ok(2));
    let mut loaded = session::from_json(&session::to_json(&browser)).unwrap();
    let c = loaded.create_user_profile("First unused").unwrap();
    assert_ne!(c.storage_key(), a.storage_key());
    assert_ne!(c.storage_key(), b.storage_key());
    assert_eq!(loaded.user_profiles().count(), 2);
}

#[test]
fn profile_management_never_edits_private_or_blank_projections() {
    for mut browser in [Browser::new_private(), Browser::new_blank()] {
        let id=browser.default_user_profile_id();
        let before=session::to_json(&browser);
        assert_eq!(browser.rename_user_profile(id,"Changed"),Err(UserProfileError::WindowLocalModel));
        assert_eq!(browser.delete_user_profile(id),Err(UserProfileError::WindowLocalModel));
        assert_eq!(browser.delete_unused_user_profiles(&[id]),Err(UserProfileError::WindowLocalModel));
        assert_eq!(session::to_json(&browser),before);
    }
}

#[test]
fn localhost_developer_choices_are_profile_owned_and_survive_sessions() {
    let mut browser = Browser::new();
    let first = browser.active_user_profile_id();
    let other = browser.create_user_profile("Other").unwrap();
    browser.user_profile_mut(first).unwrap()
        .set_developer_mode_for_host("fixture.localhost".into(), false);
    browser.user_profile_mut(first).unwrap()
        .set_developer_mode_for_host("web.example".into(), true);
    let restored = session::from_json(&session::to_json(&browser)).unwrap();
    assert!(!restored.user_profile(first).unwrap().developer_mode_for_host("fixture.localhost"));
    assert!(restored.user_profile(other).unwrap().developer_mode_for_host("fixture.localhost"));
    assert!(restored.user_profile(first).unwrap().developer_mode_for_host("localhost"));
    assert!(!restored.user_profile(first).unwrap().developer_mode_for_host("localhost.example"));
    assert!(!restored.user_profile(first).unwrap().developer_mode_for_host("notlocalhost"));
    let blank = restored.new_blank_for_user_profile(first).unwrap();
    assert!(!blank.active_user_profile().developer_mode_for_host("fixture.localhost"));
    assert!(blank.active_user_profile().developer_mode_for_host("web.example"));
}

#[test]
fn retired_profile_storage_is_explicit_persistent_and_never_reuses_a_deleted_identity() {
    let mut browser = Browser::new();
    let keep = browser.create_user_profile("keep").unwrap();
    let delete = browser.create_user_profile("delete").unwrap();
    assert!(browser.retired_profile_storage_keys().is_empty());
    browser.delete_user_profile(delete).unwrap();
    let restored = crate::session::from_json(&crate::session::to_json(&browser)).unwrap();
    assert_eq!(restored.retired_profile_storage_keys(), vec![delete.storage_key().unwrap()]);
    assert!(restored.user_profile(keep).is_some());
    for id in [restored.default_user_profile_id(), keep] {
        let mut value: serde_json::Value = serde_json::from_str(&crate::session::to_json(&restored)).unwrap();
        value["retired_user_profiles"] = serde_json::json!([id]);
        assert!(crate::session::from_json(&value.to_string()).is_err());
    }
}
