//! Context actions exercise captured IDs through real GPUI menu controls.
use super::*;

#[gpui::test]
fn space_context_profile_picker_preselects_the_clicked_space_and_confirms_before_assignment(
    cx: &mut TestAppContext,
) {
    let (view, cx) = open(cx);
    let (target, profile, original) = view.update(cx, |view, _| {
        view.edit_profile(|browser| {
            let profile = browser.create_user_profile("Context profile").unwrap();
            (
                browser.spaces[1].id,
                profile,
                browser.active_user_profile_id(),
            )
        })
    });
    right_click(cx, "space-1");
    let mut menu = tab_menu(&view, cx);
    click(&mut menu, "tab-menu-set-profile");
    click(
        &mut menu,
        test_selector(format!("tab-menu-space-profile-{profile:?}")),
    );
    let overlay = view.read_with(cx, |view, _| {
        view.user_profile_dialog.as_ref().unwrap().overlay.unwrap()
    });
    let mut sheet = VisualTestContext::from_window(overlay.into(), &cx.cx);
    sheet.run_until_parked();
    assert!(sheet.debug_bounds("profile-switch-warning").is_some());
    assert!(view.read_with(cx, |view, _| {
        view.browser
            .spaces
            .iter()
            .find(|space| space.id == target)
            .unwrap()
            .user_profile_id
            == original
    }));
    click(&mut sheet, "profile-cancel");
    cx.run_until_parked();
    assert!(view.read_with(cx, |view, _| {
        view.browser
            .spaces
            .iter()
            .find(|space| space.id == target)
            .unwrap()
            .user_profile_id
            == original
    }));
    right_click(cx, "space-1");
    let mut menu = tab_menu(&view, cx);
    click(&mut menu, "tab-menu-set-profile");
    click(
        &mut menu,
        test_selector(format!("tab-menu-space-profile-{profile:?}")),
    );
    let overlay = view.read_with(cx, |view, _| {
        view.user_profile_dialog.as_ref().unwrap().overlay.unwrap()
    });
    let mut sheet = VisualTestContext::from_window(overlay.into(), &cx.cx);
    sheet.run_until_parked();
    click(&mut sheet, "profile-accept");
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.browser
                .spaces
                .iter()
                .find(|space| space.id == target)
                .unwrap()
                .user_profile_id,
            profile
        );
        assert_eq!(view.browser.active_user_profile_id(), original);
    });
}

#[gpui::test]
fn space_context_delete_confirmation_cancels_then_virtual_delete_and_undo_restore_identity(
    cx: &mut TestAppContext,
) {
    let (view, cx) = open(cx);
    let original = view.read_with(cx, |view, _| view.browser.space().id);
    right_click(cx, "space-header-0");
    click(&mut tab_menu(&view, cx), "tab-menu-delete-space");
    let mut modal = tab_menu(&view, cx);
    let bounds = modal.debug_bounds("delete-space-dialog").unwrap();
    assert_eq!(bounds.size, gpui::size(gpui::px(450.), gpui::px(274.)));
    click(&mut modal, "delete-space-cancel");
    assert!(view.read_with(cx, |view, _| {
        view.browser.spaces.iter().any(|space| space.id == original)
    }));
    right_click(cx, "space-header-0");
    click(&mut tab_menu(&view, cx), "tab-menu-delete-space");
    let mut modal = tab_menu(&view, cx);
    modal.simulate_keystrokes("enter");
    modal.run_until_parked();
    cx.run_until_parked();
    assert!(view.read_with(cx, |view, _| {
        !view.browser.spaces.iter().any(|space| space.id == original)
    }));
    cx.simulate_keystrokes("cmd-z");
    cx.run_until_parked();
    assert_eq!(
        view.read_with(cx, |view, _| view.browser.space().id),
        original
    );
    assert!(
        view.read_with(cx, |view, _| view.sidebar_navigation.controller.selected()
            == crate::sidebar_pages::SidebarPage::Space(original))
    );
}

#[gpui::test]
fn space_context_export_moves_content_into_the_selected_folder_and_undo_restores_the_space(
    cx: &mut TestAppContext,
) {
    let (view, cx) = open(cx);
    let (source, target, tabs) = view.read_with(cx, |view, _| {
        (
            view.browser.space().id,
            view.browser.spaces[1].id,
            view.browser
                .space()
                .pinned
                .iter()
                .chain(&view.browser.space().today)
                .map(|tab| tab.id)
                .collect::<Vec<_>>(),
        )
    });
    right_click(cx, "space-header-0");
    let mut menu = tab_menu(&view, cx);
    click(&mut menu, "tab-menu-export-personal-into-folder");
    click(
        &mut menu,
        test_selector(format!("tab-menu-space-export-{target:?}")),
    );
    view.read_with(cx, |view, _| {
        assert!(!view.browser.spaces.iter().any(|space| space.id == source));
        assert_eq!(view.browser.space().id, target);
        let folder = view
            .browser
            .space()
            .folders
            .iter()
            .find(|folder| folder.name == "Personal")
            .unwrap();
        let moved = view.browser.folder_tabs(folder.id);
        assert!(tabs.iter().all(|id| moved.contains(id)));
    });
    cx.simulate_keystrokes("cmd-z");
    cx.run_until_parked();
    assert_eq!(
        view.read_with(cx, |view, _| view.browser.space().id),
        source
    );
}

#[gpui::test]
fn space_context_manage_spaces_opens_library_without_changing_the_selected_document(
    cx: &mut TestAppContext,
) {
    let (view, cx, fake) = open_pages(cx);
    let before = active_url(&view, cx);
    fake.take_calls();
    right_click(cx, "space-header-0");
    click(&mut tab_menu(&view, cx), "tab-menu-manage-spaces");
    assert!(
        view.read_with(cx, |view, _| view.sidebar_navigation.controller.selected()
            == crate::sidebar_pages::SidebarPage::Library)
    );
    assert_eq!(
        view.read_with(cx, |view, _| view.sidebar_navigation.library_section),
        crate::library::LibrarySection::Spaces
    );
    assert_eq!(active_url(&view, cx), before);
    assert!(!fake.take_calls().iter().any(|call| matches!(
        call,
        Call::Create(..) | Call::Navigate(..) | Call::Close(..)
    )));
}

#[gpui::test]
fn mixed_selection_menu_omits_relative_archive_and_all_muted_items_can_be_unmuted(
    cx: &mut TestAppContext,
) {
    let (view, cx) = open(cx);
    let ids = view.read_with(cx, |view, _| {
        view.browser
            .space()
            .today
            .iter()
            .take(2)
            .map(|tab| tab.id)
            .collect::<Vec<_>>()
    });
    view.update(cx, |view, _| {
        view.edit_browser(|browser| {
            browser.toggle_pin(ids[0]);
        })
    });
    cx.run_until_parked();
    click(cx, test_selector(format!("tab-{}", ids[0].get())));
    modifier_click(
        cx,
        test_selector(format!("tab-{}", ids[1].get())),
        Modifiers {
            platform: true,
            ..Modifiers::none()
        },
    );
    right_click(cx, test_selector(format!("tab-{}", ids[1].get())));
    view.read_with(cx, |view, _| assert!(!view.menu_entries().iter().any(|entry| matches!(entry, crate::tab_menu::Entry::Item(item) if item.label.as_ref() == "Archive Tabs Below"))));
    tab_menu(&view, cx).simulate_keystrokes("escape");
    cx.run_until_parked();
    view.update(cx, |view, _| {
        view.edit_profile(|browser| {
            let space = &mut browser.spaces[0];
            for tab in space.pinned.iter_mut().chain(&mut space.today) {
                if ids.contains(&tab.id) {
                    tab.muted = true;
                }
            }
        })
    });
    cx.run_until_parked();
    right_click(cx, test_selector(format!("tab-{}", ids[1].get())));
    view.read_with(cx, |view, _| assert!(view.menu_entries().iter().any(|entry| matches!(entry, crate::tab_menu::Entry::Item(item) if item.label.as_ref() == "Unmute" && item.enabled))));
    click(&mut tab_menu(&view, cx), "tab-menu-mute");
    assert!(view.read_with(cx, |view, _| ids.iter().all(|id| {
        !view
            .browser
            .all_tabs()
            .find(|tab| tab.id == *id)
            .unwrap()
            .muted
    })));
}

#[gpui::test]
fn space_context_cross_profile_export_retires_old_documents_before_opening_destination_storage(
    cx: &mut TestAppContext,
) {
    let (view, cx, fake) = open_pages(cx);
    let (tab, target, profile, original) = view.update(cx, |view, _| {
        view.edit_browser(|browser| {
            let profile = browser.create_user_profile("Export destination").unwrap();
            let target = browser.spaces[1].id;
            browser.assign_space_user_profile(target, profile).unwrap();
            (
                browser.active_tab().unwrap().id,
                target,
                profile,
                browser.space().id,
            )
        })
    });
    fake.take_calls();
    right_click(cx, "space-header-0");
    let mut menu = tab_menu(&view, cx);
    click(&mut menu, "tab-menu-export-personal-into-folder");
    click(
        &mut menu,
        test_selector(format!("tab-menu-space-export-{target:?}")),
    );
    assert!(fake.take_calls().contains(&Call::Close(1)));
    click(cx, test_selector(format!("tab-{}", tab.get())));
    assert!(
        fake.0
            .borrow()
            .storage
            .values()
            .any(|storage| *storage == crate::pages::StorageSelection::for_profile(profile))
    );
    fake.send(
        cx,
        Event::TitleChanged {
            view: page(1),
            title: "Stale source document".into(),
        },
    );
    assert!(view.read_with(cx, |view, _| {
        view.browser
            .all_tabs()
            .find(|entry| entry.id == tab)
            .unwrap()
            .title
            != "Stale source document"
    }));
    fake.take_calls();
    cx.simulate_keystrokes("cmd-z");
    cx.run_until_parked();
    assert_eq!(
        view.read_with(cx, |view, _| view.browser.space().id),
        original
    );
    assert!(
        fake.take_calls()
            .iter()
            .any(|call| matches!(call, Call::Close(_)))
    );
    let state = fake.0.borrow();
    let latest = state.storage.keys().max().unwrap();
    assert_eq!(
        state.storage[latest],
        crate::pages::StorageSelection::Default
    );
}
