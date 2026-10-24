//! Production host, transparent-child routing and popup ownership regressions.
use super::*;
use crate::user_profile_dialog::{CreationSource, DialogEvent, ProfileRequest};
use browser_core::Browser;
use gpui::AppContext;

fn sheet(view: &Entity<BrowserWindow>, visual: &mut VisualTestContext) -> VisualTestContext {
    visual.run_until_parked();
    let overlay = view.read_with(visual, |view, _| view.user_profile_dialog.as_ref().unwrap().overlay.unwrap());
    VisualTestContext::from_window(overlay.into(), &visual.cx)
}

fn dialog(view: &Entity<BrowserWindow>, visual: &VisualTestContext) -> Entity<crate::user_profile_dialog::UserProfileDialog> {
    view.read_with(visual, |view, _| view.user_profile_dialog.as_ref().unwrap().dialog.clone())
}

fn input_focused(view: &Entity<BrowserWindow>, visual: &mut VisualTestContext) -> bool {
    let form = dialog(view, visual);
    sheet(view, visual).update(|window, cx| form.read(cx).input.read(cx).focus_handle(cx).is_focused(window))
}

#[gpui::test]
fn file_profile_sheet_focus_back_and_cancel_preserve_the_workspace(cx: &mut TestAppContext) {
    let (view, visual, fake) = open_pages(cx);
    let before = view.read_with(visual, |view, _| view.session_json());
    dispatch_menu(visual, "File", "New Profile");
    assert!(input_focused(&view, visual));
    let input = dialog(&view, visual).read_with(visual, |form, _| form.input.clone());
    let mut child = sheet(&view, visual);
    child.simulate_input("Test profile");
    click(&mut child, "profile-accept");
    click(&mut child, "profile-back");
    assert_eq!(dialog(&view, visual).read_with(visual, |form, _| form.input.clone()), input);
    assert!(input_focused(&view, visual));
    child.simulate_keystrokes("escape");
    visual.run_until_parked();
    assert!(view.read_with(visual, |view, _| view.user_profile_dialog.is_none()));
    assert_eq!(view.read_with(visual, |view, _| view.session_json()), before);
    assert_eq!(visual.cx.update(|cx| cx.windows().len()), 1);
    assert!(fake.take_calls().contains(&Call::FocusPage(1)));
}

#[gpui::test]
fn command_bar_opens_creation_sheet_and_applies_only_captured_source_space(cx: &mut TestAppContext) {
    let (view, visual, _) = open_pages(cx);
    let (source, default, favorite) = view.update(visual, |view, _| view.edit_browser(|browser| {
        let favorite = browser.active_tab().unwrap().id;
        browser.favorite_tab(favorite);
        (browser.space().id, browser.active_user_profile_id(), favorite)
    }));
    visual.simulate_keystrokes("cmd-t");
    bar(&view, visual).simulate_input("new profile");
    bar(&view, visual).simulate_keystrokes("enter");
    assert!(view.read_with(visual, |view, _| view.command_bar.is_none()));
    assert!(input_focused(&view, visual));
    let mut child = sheet(&view, visual);
    child.simulate_input("Created by sheet");
    click(&mut child, "profile-accept");
    click(&mut child, "profile-accept");
    visual.run_until_parked();
    view.read_with(visual, |view, _| {
        assert!(view.user_profile_dialog.is_none());
        let created = view.browser.user_profiles().find(|profile| profile.name() == "Created by sheet").unwrap();
        assert!(created.favorites.is_empty());
        assert_eq!(view.browser.spaces.iter().find(|space| space.id == source).unwrap().user_profile_id, created.id());
        assert!(view.browser.spaces.iter().filter(|space| space.id != source).all(|space| space.user_profile_id == default));
        assert_eq!(view.browser.user_profile(default).unwrap().favorites[0].id, favorite);
    });
}

#[gpui::test]
fn inactive_space_context_routes_captured_identity_without_selecting_it(cx: &mut TestAppContext) {
    let (view, visual, _) = open_pages(cx);
    let (source, active, named) = view.update(visual, |view, _| view.edit_browser(|browser| {
        (browser.spaces[1].id, browser.space().id, browser.create_user_profile("Other").unwrap())
    }));
    visual.update(|window, cx| view.update(cx, |view, cx| view.open_space_menu(source, gpui::point(gpui::px(40.), gpui::px(180.)), window, cx)));
    visual.run_until_parked();
    let mut menu = tab_menu(&view, visual);
    click(&mut menu, "tab-menu-set-profile");
    click(&mut menu, test_selector(format!("tab-menu-space-profile-{named:?}")));
    let mut child = sheet(&view, visual);
    view.read_with(visual, |view, _| {
        assert_ne!(view.browser.spaces.iter().find(|space| space.id == source).unwrap().user_profile_id, named);
        assert!(view.user_profile_dialog.is_some());
    });
    click(&mut child, "profile-accept");
    visual.run_until_parked();
    view.read_with(visual, |view, _| {
        assert_eq!(view.browser.space().id, active);
        assert_eq!(view.browser.spaces.iter().find(|space| space.id == source).unwrap().user_profile_id, named);
        assert_ne!(view.browser.active_user_profile_id(), named);
    });
}

#[gpui::test]
fn dropdown_replacement_ignores_old_callbacks_and_keeps_ui_focus(cx: &mut TestAppContext) {
    let (view, visual, fake) = open_pages(cx);
    dispatch_menu(visual, "Spaces", "Change Profile");
    let old = dialog(&view, visual);
    let mut child = sheet(&view, visual);
    fake.take_calls();
    click(&mut child, "profile-dropdown");
    click(&mut child, "profile-new");
    visual.run_until_parked();
    assert_ne!(dialog(&view, visual), old);
    assert!(input_focused(&view, visual));
    assert_eq!(visual.cx.update(|cx| cx.windows().len()), 2);
    let source = view.read_with(visual, |view, _| view.browser.space().id);
    old.update(visual, |_, cx| {
        cx.emit(DialogEvent::Closed);
        cx.emit(DialogEvent::Submit(ProfileRequest::Create { source: CreationSource::File(source), name: "Stale sender".into(), spaces: vec![source] }));
    });
    visual.run_until_parked();
    assert!(view.read_with(visual, |view, _| view.user_profile_dialog.is_some() && view.browser.user_profiles().count() == 1));
    assert!(input_focused(&view, visual));
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::FocusPage(_))));
}

#[gpui::test]
fn rapid_sheet_open_close_reopen_and_owner_close_leave_no_child(cx: &mut TestAppContext) {
    let (view, visual, _) = open_pages(cx);
    view.update(visual, |view, cx| {
        let source = view.browser.space().id;
        view.open_user_profile_creation(CreationSource::File(source), cx);
        view.close_user_profile_dialog(cx);
        view.open_user_profile_switch(source, cx);
    });
    visual.run_until_parked();
    assert_eq!(visual.cx.update(|cx| cx.windows().len()), 2);
    let old = dialog(&view, visual);
    view.update(visual, |view, cx| view.close_window(cx));
    old.update(visual, |_, cx| cx.emit(DialogEvent::Closed));
    visual.run_until_parked();
    assert_eq!(visual.cx.update(|cx| cx.windows().len()), 0);
}

#[gpui::test]
fn profile_sheet_minimize_teardown_invalidates_queued_peer_and_sheet_restoration(cx: &mut TestAppContext) {
    let (view, visual, fake) = open_pages(cx);
    dispatch_menu(visual, "File", "New Profile");
    let mut child = sheet(&view, visual);
    child.update(|window, _| window.activate_window());
    child.run_until_parked();
    fake.take_calls();
    view.update(visual, |view, cx| {
        view.close_user_profile_dialog(cx);
        let source = view.browser.space().id;
        view.open_user_profile_switch(source, cx);
        view.close_popups_for_window_operation(true, cx);
    });
    visual.run_until_parked();
    assert!(view.read_with(visual, |view, _| view.user_profile_dialog.is_none()));
    assert_eq!(visual.cx.update(|cx| cx.windows().len()), 1);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::FocusPage(_))));
    visual.update(|window, _| window.activate_window());
    visual.run_until_parked();
    assert!(fake.take_calls().contains(&Call::FocusPage(1)));
}

#[gpui::test]
fn native_page_metadata_and_focus_do_not_steal_the_profile_sheet(cx: &mut TestAppContext) {
    let (view, visual, fake) = open_pages(cx);
    dispatch_menu(visual, "File", "New Profile");
    fake.take_calls();
    fake.send(visual, Event::Focused { view: page(1) });
    fake.send(visual, Event::TitleChanged { view: page(1), title: "Background metadata".into() });
    assert!(input_focused(&view, visual));
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::FocusPage(_))));
    view.read_with(visual, |view, _| assert_eq!(view.browser.active_tab().unwrap().page_title.as_deref(), Some("Background metadata")));
}

#[gpui::test]
fn changed_space_names_in_another_window_close_stale_sheet_without_index_retarget(cx: &mut TestAppContext) {
    let (view, visual, _) = open_pages(cx);
    let (other, mut other_visual) = another_profile_window(&view, visual);
    dispatch_menu(visual, "Spaces", "Change Profile");
    let source = view.read_with(visual, |view, _| view.browser.space().id);
    other.update(&mut other_visual, |view, _| view.edit_browser(|browser| browser.rename_space(source, "Renamed elsewhere")));
    visual.run_until_parked();
    assert!(view.read_with(visual, |view, _| view.user_profile_dialog.is_none()));
    assert_eq!(visual.cx.update(|cx| cx.windows().len()), 2);
}

#[gpui::test]
fn profile_menu_routes_are_unavailable_for_private_and_blank_owners(cx: &mut TestAppContext) {
    cx.update(crate::init);
    for browser in [Browser::new_private(), Browser::new_blank()] {
        let (view, visual) = cx.add_window_view(|window, cx| BrowserWindow::new(browser, None, window, cx));
        visual.run_until_parked();
        assert!(!visual.update(|window, cx| window.is_action_available(&crate::menus::NewProfile, cx)));
        assert!(!visual.update(|window, cx| window.is_action_available(&crate::menus::ChangeProfile, cx)));
        view.update(visual, |view, cx| {
            let source = view.browser.space().id;
            view.open_user_profile_creation(CreationSource::File(source), cx);
            view.open_user_profile_switch(source, cx);
        });
        assert!(view.read_with(visual, |view, _| view.user_profile_dialog.is_none()));
    }
}
