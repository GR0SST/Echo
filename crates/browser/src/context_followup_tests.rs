use super::*;
use crate::tab_menu::{Command, Entry};
#[gpui::test]
fn context_followup_option_archive_keeps_the_observed_validation_and_archives_only_other_today_items(
    cx: &mut TestAppContext,
) {
    let (view, cx) = open(cx);
    let ids = view.update(cx, |view, _| {
        view.edit_browser(|browser| {
            let ids: Vec<_> = browser.space().today.iter().map(|tab| tab.id).collect();
            browser.archive_sidebar_items(&ids, std::time::SystemTime::now());
            for name in ["Alpha", "Beta", "Gamma"] {
                browser.new_tab(
                    format!("https://{}.example/", name.to_lowercase()),
                    std::time::SystemTime::now(),
                );
            }
            browser
                .space()
                .today
                .iter()
                .map(|tab| tab.id)
                .collect::<Vec<_>>()
        })
    });
    cx.run_until_parked();
    click(cx, test_selector(format!("tab-{}", ids[2].get())));
    modifier_click(
        cx,
        test_selector(format!("tab-{}", ids[1].get())),
        Modifiers {
            platform: true,
            ..Modifiers::none()
        },
    );
    right_click(cx, test_selector(format!("tab-{}", ids[1].get())));
    let mut menu = tab_menu(&view, cx);
    menu.simulate_modifiers_change(Modifiers {
        alt: true,
        ..Modifiers::none()
    });
    menu.run_until_parked();
    view.read_with(cx,|view,_|{
        let rows=view.menu_entries();
        assert!(rows.iter().any(|entry|matches!(entry,Entry::Item(item) if item.label.as_ref()=="Archive Other Tabs" && matches!(item.command,Some(Command::SelectionArchiveOthers)))));
        assert!(rows.iter().any(|entry|matches!(entry,Entry::Item(item) if item.label.as_ref()=="Archive Tabs Above" && !item.enabled)));
    });
    menu.simulate_keystrokes("alt-up");
    menu.simulate_keystrokes("alt-enter");
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert_eq!(view.browser.space().active_tab, Some(ids[2]));
        assert!(!view.browser.all_tabs().any(|tab| tab.id == ids[0]));
        assert!(ids[1..]
            .iter()
            .all(|id| view.browser.all_tabs().any(|tab| tab.id == *id)));
    });
    cx.simulate_modifiers_change(Modifiers::none());
    cx.simulate_keystrokes("cmd-z");
    cx.run_until_parked();
    assert!(view.read_with(cx, |view, _| ids
        .iter()
        .all(|id| view.browser.all_tabs().any(|tab| tab.id == *id))));
}
#[gpui::test]
fn context_followup_multi_move_retires_source_documents_before_named_storage_is_opened(
    cx: &mut TestAppContext,
) {
    let (view, cx, fake) = open_pages(cx);
    let (ids, target, profile, source) = view.update(cx, |view, _| {
        view.edit_browser(|browser| {
            let ids = browser
                .space()
                .today
                .iter()
                .take(2)
                .map(|tab| tab.id)
                .collect::<Vec<_>>();
            let target = browser.spaces[1].id;
            let profile = browser.create_user_profile("Move destination").unwrap();
            browser.assign_space_user_profile(target, profile).unwrap();
            (ids, target, profile, browser.space().id)
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
    fake.take_calls();
    right_click(cx, test_selector(format!("tab-{}", ids[1].get())));
    let mut menu = tab_menu(&view, cx);
    click(&mut menu, "tab-menu-move-to");
    click(
        &mut menu,
        test_selector(format!("tab-menu-selection-move-{target:?}")),
    );
    assert!(fake.take_calls().contains(&Call::Close(1)));
    view.read_with(cx, |view, _| {
        assert_eq!(view.browser.space().id, source);
        assert!(ids
            .iter()
            .all(|id| view.browser.user_profile_id_for_tab(*id) == Some(profile)));
    });
    fake.send(
        cx,
        Event::TitleChanged {
            view: page(1),
            title: "Old profile callback".into(),
        },
    );
    assert!(view.read_with(cx, |view, _| view
        .browser
        .all_tabs()
        .all(|tab| tab.title != "Old profile callback")));
    click(cx, "space-1");
    cx.executor().advance_clock(Duration::from_millis(400));
    cx.run_until_parked();
    assert_eq!(
        view.read_with(cx, |view, _| view.browser.space().id),
        target
    );
    click(cx, test_selector(format!("tab-{}", ids[0].get())));
    assert_eq!(
        view.read_with(cx, |view, _| view.browser.space().active_tab),
        Some(ids[0])
    );
    assert!(fake
        .0
        .borrow()
        .storage
        .values()
        .any(|storage| *storage == crate::pages::StorageSelection::for_profile(profile)));
}
#[gpui::test]
fn context_followup_split_move_uses_space_destinations_and_keeps_both_panes(
    cx: &mut TestAppContext,
) {
    let (view, cx, fake) = open_pages(cx);
    let (a, b, target, source) = view.update(cx, |view, _| {
        view.edit_browser(|browser| {
            let a = browser.active_tab().unwrap().id;
            let b = browser.new_tab("https://second.example/", std::time::SystemTime::now());
            browser
                .add_split(
                    a,
                    b,
                    browser_core::splits::Placement::Right,
                    std::time::SystemTime::now(),
                )
                .unwrap();
            (a, b, browser.spaces[1].id, browser.space().id)
        })
    });
    cx.run_until_parked();
    fake.take_calls();
    right_click(cx, test_selector(format!("tab-{}", a.get())));
    let mut menu = tab_menu(&view, cx);
    click(&mut menu, "tab-menu-move-to");
    click(
        &mut menu,
        test_selector(format!("tab-menu-selection-move-{target:?}")),
    );
    view.read_with(cx, |view, _| {
        assert_eq!(view.browser.space().id, source);
        assert_eq!(view.browser.related_tabs(a), vec![a, b]);
        let target = view
            .browser
            .spaces
            .iter()
            .find(|space| space.id == target)
            .unwrap();
        assert!(target.today.iter().any(|tab| tab.id == a));
        assert!(target.today.iter().any(|tab| tab.id == b));
    });
    assert!(!fake
        .take_calls()
        .iter()
        .any(|call| matches!(call, Call::Close(_))));
}
#[gpui::test]
fn context_followup_space_delete_confirmation_is_invalidated_by_profile_reassignment(
    cx: &mut TestAppContext,
) {
    let (view, cx) = open(cx);
    let source = view.read_with(cx, |view, _| view.browser.space().id);
    right_click(cx, "space-header-0");
    click(&mut tab_menu(&view, cx), "tab-menu-delete-space");
    view.update(cx, |view, _| {
        view.edit_profile(|browser| {
            let profile = browser.create_user_profile("Replacement").unwrap();
            browser.assign_space_user_profile(source, profile).unwrap();
        })
    });
    cx.run_until_parked();
    assert!(view.read_with(cx, |view, _| view.tab_menu.is_none()));
    // A confirmation callback already queued by the retired sheet is also inert.
    view.update(cx, |view, cx| view.confirm_space_delete(source, cx));
    cx.run_until_parked();
    assert!(view.read_with(cx, |view, _| view
        .browser
        .spaces
        .iter()
        .any(|space| space.id == source)));
}

#[gpui::test]
fn context_followup_four_selected_today_tabs_keep_the_active_page_first_and_focus_the_last_peer(
    cx: &mut TestAppContext,
) {
    let (view, cx) = open(cx);
    let (a, b, c, d) = view.update(cx, |view, _| {
        view.edit_browser(|browser| {
            let a = browser.new_tab("https://alpha.example", std::time::SystemTime::now());
            let b = browser.new_tab("https://beta.example", std::time::SystemTime::now());
            let c = browser.new_tab("https://gamma.example", std::time::SystemTime::now());
            let d = browser.new_tab("https://delta.example", std::time::SystemTime::now());
            (a, b, c, d)
        })
    });
    cx.run_until_parked();
    click(cx, test_selector(format!("tab-{}", a.get())));
    for id in [b, c, d] {
        modifier_click(
            cx,
            test_selector(format!("tab-{}", id.get())),
            Modifiers {
                platform: true,
                ..Modifiers::none()
            },
        );
    }
    right_click(cx, test_selector(format!("tab-{}", d.get())));
    click(&mut tab_menu(&view, cx), "tab-menu-selection-split");
    view.read_with(cx, |view, _| {
        assert_eq!(view.browser.active_split().unwrap().panes, vec![a, d, c, b]);
        assert_eq!(view.browser.space().active_tab, Some(b));
        assert_eq!(view.browser.active_split().unwrap().anchor, a);
    });
}

#[gpui::test]
fn context_followup_four_pane_sidebar_menu_disables_add_and_shares_only_the_chosen_pane(
    cx: &mut TestAppContext,
) {
    let (view, cx) = open(cx);
    let anchor = view.update(cx, |view, _| {
        view.edit_browser(|browser| {
            let anchor = browser.active_tab().unwrap().id;
            let others: Vec<_> = browser.space().today.iter().filter(|tab| tab.id != anchor).map(|tab| tab.id).collect();
            browser.archive_sidebar_items(&others, std::time::SystemTime::now());
            let mut peer = anchor;
            for name in ["one", "two", "three"] {
                peer = browser.new_tab(
                    format!("https://{name}.example/"),
                    std::time::SystemTime::now(),
                );
                browser
                    .add_split(
                        anchor,
                        peer,
                        browser_core::splits::Placement::Right,
                        std::time::SystemTime::now(),
                    )
                    .unwrap();
            }
            browser
                .spaces
                .iter_mut()
                .flat_map(|space| space.today.iter_mut())
                .find(|tab| tab.id == peer)
                .unwrap()
                .title = "Sharing peer".into();
            anchor
        })
    });
    cx.run_until_parked();
    right_click(cx, test_selector(format!("tab-{}", anchor.get())));
    view.read_with(cx, |view, _| assert!(view.menu_entries().iter().any(|entry| matches!(entry, Entry::Item(item) if matches!(item.command, Some(Command::Add(_))) && !item.enabled))));
    view.read_with(cx, |view, _| {
        let entries = view.menu_entries();
        assert!(entries.iter().any(|entry| matches!(entry, Entry::Item(item) if item.label.as_ref() == "Archive Tabs Below" && !item.enabled)));
        assert!(!entries.iter().any(|entry| matches!(entry, Entry::Item(item) if item.label.as_ref() == "Archive Other Tabs" || item.label.as_ref() == "Archive Tabs Above")));
    });
    // The system picker must receive the submenu's pane, not the group's active page.
    let mut menu = tab_menu(&view, cx);
    click(&mut menu, "tab-menu-sharing-peer");
    click(&mut menu, "tab-menu-share");
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.share_picker.as_ref().unwrap().urls,
            vec!["https://three.example/"]
        )
    });
}
