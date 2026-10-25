use super::*;
#[gpui::test]
fn advanced_footer_exports_current_sidebar_and_cancel_keeps_it_unchanged(cx: &mut TestAppContext) {
    let (view, visual, _) = open_pages(cx);
    let before=view.read_with(visual,|v,_|v.profile.handle.settings_snapshot());
    visual.simulate_keystrokes("cmd-,");
    let (_,mut settings)=settings_window(visual);
    click(&mut settings,"settings-tab-Advanced");
    click(&mut settings,"settings-export-data");
    assert!(settings.did_prompt_for_new_path());
    settings.simulate_new_path_selection(|_|None);
    settings.run_until_parked();
    assert_eq!(view.read_with(visual,|v,_|browser_core::session::to_json(&v.profile.handle.settings_snapshot())),browser_core::session::to_json(&before));
    let root=tempfile::tempdir().unwrap();let path=root.path().join("Echo Data Export.md");
    click(&mut settings,"settings-export-data");
    settings.simulate_new_path_selection(|_|Some(path.clone()));
    settings.run_until_parked();
    assert_eq!(std::fs::read_to_string(path).unwrap(),before.export_sidebar_markdown());
}
#[gpui::test]
fn advanced_footer_resets_only_the_selected_live_profile_and_more_settings_opens_in_owner(cx: &mut TestAppContext) {
    let (view, visual, fake)=open_pages(cx);
    let other=view.update(visual,|v,_|v.edit_profile(|b|b.create_user_profile("Other").unwrap()));
    visual.simulate_keystrokes("cmd-,");
    let (settings,mut panel)=settings_window(visual);
    settings.update(&mut panel,|settings,_|settings.selected_profile=other);
    click(&mut panel,"settings-tab-Advanced");fake.take_calls();
    click(&mut panel,"settings-reset-translation");
    assert!(fake.take_calls().contains(&Call::ResetTranslation(crate::pages::StorageSelection::for_profile(other))));
    click(&mut panel,"settings-more");visual.run_until_parked();
    assert_eq!(active_url(&view,visual).as_deref(),Some("browser://settings/"));
    assert!(visual.debug_bounds("more-settings-page").is_some());
    assert!(!fake.take_calls().iter().any(|call|matches!(call,Call::Create(_,url) if url=="browser://settings/")));
}
