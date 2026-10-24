use super::*;
const NOW: SystemTime = SystemTime::UNIX_EPOCH;
#[test]
fn cross_profile_space_move_keeps_the_complete_pinned_group_without_copying_profile_data() {
    let mut b = Browser::new();
    let source = b.space().id;
    let original_profile = b.active_user_profile_id();
    let keep = b.new_tab("https://keep.example", NOW);
    let a = b.new_tab("https://a.example", NOW);
    let c = b.new_tab("https://c.example", NOW);
    let d = b.new_tab("https://d.example", NOW);
    b.add_split(a, c, Placement::Right, NOW).unwrap();
    b.add_split(a, d, Placement::Below, NOW).unwrap();
    b.toggle_pin(a);
    let folder = b.create_folder(source, None, "Source folder").unwrap();
    assert!(b.move_item_to_folder(a, folder));
    b.activate_tab(c, NOW);
    let split = b.spaces[0]
        .splits
        .iter_mut()
        .find(|split| split.panes.contains(&a))
        .unwrap();
    split.title = Some("Kept title".into());
    split.custom_icon = Some("ion:code".into());
    split.weights = vec![0.2, 0.3, 0.5];
    let expected = serde_json::to_value(&*split).unwrap();
    let target = b.add_space("Destination", 40.);
    let profile = b.create_user_profile("Destination profile").unwrap();
    b.assign_space_user_profile(target, profile).unwrap();
    let profiles = serde_json::to_value(&b).unwrap()["profiles"].clone();
    assert!(b.move_sidebar_item_to_space(c, target));
    assert_eq!(b.space().id, source);
    assert_eq!(b.active_user_profile_id(), original_profile);
    assert_eq!(b.space().active_tab, Some(keep));
    assert!(b.folder_tabs(folder).is_empty());
    assert_eq!(
        serde_json::to_value(b.split_for(a).unwrap()).unwrap(),
        expected
    );
    assert!(b
        .related_tabs(a)
        .iter()
        .all(|id| b.user_profile_id_for_tab(*id) == Some(profile)));
    assert_eq!(serde_json::to_value(&b).unwrap()["profiles"], profiles);
    assert!(b
        .spaces
        .iter()
        .find(|space| space.id == target)
        .unwrap()
        .active_tab
        .is_none());
    let before = crate::session::to_json(&b);
    assert!(!b.move_sidebar_item_to_space(a, target));
    assert_eq!(crate::session::to_json(&b), before);
}
#[test]
fn cross_profile_favorite_move_is_rejected_without_touching_either_profile() {
    let mut b = Browser::new();
    let favorite = b.new_tab("https://favorite.example", NOW);
    assert!(b.favorite_tab(favorite));
    let target = b.add_space("Destination", 40.);
    let profile = b.create_user_profile("Destination profile").unwrap();
    b.assign_space_user_profile(target, profile).unwrap();
    let before = crate::session::to_json(&b);
    assert!(!b.move_sidebar_item_to_space(favorite, target));
    assert_eq!(crate::session::to_json(&b), before);
}
#[test]
fn incomplete_group_move_is_rejected_before_removing_its_sidebar_anchor() {
    let mut b = Browser::new();
    let a = b.new_tab("https://a.example", NOW);
    let c = b.new_tab("https://c.example", NOW);
    b.add_split(a, c, Placement::Right, NOW).unwrap();
    let target = b.add_space("Destination", 40.);
    let profile = b.create_user_profile("Destination profile").unwrap();
    b.assign_space_user_profile(target, profile).unwrap();
    b.spaces[0].today.retain(|tab| tab.id != c);
    let before = crate::session::to_json(&b);
    assert!(!b.move_sidebar_item_to_space(a, target));
    assert_eq!(crate::session::to_json(&b), before);
}
