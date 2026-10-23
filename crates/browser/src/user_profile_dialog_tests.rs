//! Real GPUI form interactions. Included by user_profile_dialog.rs; no native input.
use super::*;
use gpui::{Modifiers, TestAppContext, VisualTestContext};
use std::{cell::RefCell, rc::Rc, time::SystemTime};

type Events = Rc<RefCell<Vec<DialogEvent>>>;

fn open(
    cx: &mut TestAppContext,
    browser: Browser,
    source: CreationSource,
    switch: bool,
) -> (
    Entity<BrowserWindow>,
    Entity<UserProfileDialog>,
    &mut VisualTestContext,
    Events,
    Subscription,
) {
    cx.update(|cx| {
        crate::init(cx);
        bind_keys(cx);
    });
    let snapshot = browser.clone();
    let owner = {
        let (owner, _) =
            cx.add_window_view(|window, cx| BrowserWindow::new(browser, None, window, cx));
        owner
    };
    let (dialog, visual) = cx.add_window_view(|window, cx| {
        let dialog = if switch {
            let CreationSource::File(space) = source else {
                panic!("switch needs a Space")
            };
            UserProfileDialog::switching(&snapshot, space, cx).unwrap()
        } else {
            UserProfileDialog::creation(&snapshot, source, cx).unwrap()
        };
        dialog.focus(window, cx);
        dialog
    });
    let events: Events = Rc::default();
    let log = events.clone();
    let target = owner.downgrade();
    let subscription = visual.update(|_, cx| {
        cx.subscribe(&dialog, move |_, event: &DialogEvent, cx| {
            log.borrow_mut().push(event.clone());
            if let DialogEvent::Submit(request) = event {
                target
                    .update(cx, |owner, _| owner.apply_user_profile_dialog(request))
                    .unwrap()
                    .unwrap();
            }
        })
    });
    visual.run_until_parked();
    (owner, dialog, visual, events, subscription)
}

fn click(cx: &mut VisualTestContext, selector: &'static str) {
    cx.run_until_parked();
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("missing {selector}"));
    cx.simulate_mouse_move(bounds.center(), None, Modifiers::none());
    cx.run_until_parked();
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
}

fn fixture() -> Browser {
    let mut browser = Browser::new();
    let tab = browser.new_tab("https://existing.example", SystemTime::now());
    browser.favorite_tab(tab);
    browser.add_space("Work", 20.);
    browser
}

/// Compare rendered relationships, including the footer's remaining bottom
/// gutter, so a tall sheet with collapsed content at its top cannot pass.
fn assert_name_geometry(visual: &mut VisualTestContext) {
    visual.run_until_parked();
    let sheet = visual.debug_bounds("user-profile-sheet").unwrap();
    let illustration = visual.debug_bounds("profile-illustration").unwrap();
    let heading = visual.debug_bounds("profile-heading").unwrap();
    let body = visual.debug_bounds("profile-body").unwrap();
    let label = visual.debug_bounds("profile-name-label").unwrap();
    let input = visual.debug_bounds("profile-name").unwrap();
    let cancel = visual.debug_bounds("profile-cancel").unwrap();
    let next = visual.debug_bounds("profile-accept").unwrap();
    assert_eq!(sheet.size, gpui::size(px(450.), px(426.)));
    assert_eq!(illustration.origin.y - sheet.origin.y, px(38.));
    assert_eq!(heading.origin.y - illustration.bottom(), px(18.));
    assert_eq!(body.origin.y - heading.bottom(), px(12.));
    assert_eq!(body.size.height, px(72.)); // Four lines at 18 px.
    assert_eq!(label.origin.y - body.bottom(), px(18.));
    assert_eq!(input.origin.y - label.bottom(), px(6.));
    assert_eq!(input.size, gpui::size(px(374.), px(48.)));
    assert_eq!(next.origin.y - input.bottom(), px(46.));
    assert_eq!(next.origin.y, cancel.origin.y);
    assert_eq!(next.size.height, px(36.));
    assert_eq!(next.origin.x - cancel.right(), px(8.));
    assert_eq!(sheet.right() - next.right(), px(38.));
    assert_eq!(sheet.bottom() - next.bottom(), px(28.));
}

#[gpui::test]
fn name_sheet_keeps_reference_spacing_after_typing_and_back(cx: &mut TestAppContext) {
    let (_, _, visual, _, _subscription) = open(cx, fixture(), CreationSource::Settings, false);
    assert_name_geometry(visual);
    visual.simulate_input("Personal Profile");
    assert_name_geometry(visual);
    click(visual, "profile-accept");
    click(visual, "profile-back");
    assert_name_geometry(visual);
}

#[gpui::test]
fn creation_focus_disabled_next_back_and_file_assignment(cx: &mut TestAppContext) {
    let browser = fixture();
    let source = browser.spaces[0].id;
    let old = browser.default_user_profile_id();
    let old_favorite = browser.active_user_profile().favorites[0].id;
    let (owner, dialog, visual, events, _subscription) =
        open(cx, browser, CreationSource::File(source), false);
    assert_eq!(
        visual
            .debug_bounds("user-profile-sheet")
            .unwrap()
            .size
            .height,
        px(426.)
    );
    assert!(visual.debug_bounds("profile-cancel-keycap").is_some());
    assert!(visual.debug_bounds("profile-enter-keycap").is_some());
    assert_eq!(
        visual.debug_bounds("profile-illustration").unwrap().size,
        gpui::size(px(64.), px(64.))
    );
    assert!(visual.update(|window, cx| {
        dialog
            .read(cx)
            .input
            .read(cx)
            .focus_handle(cx)
            .is_focused(window)
    }));
    click(visual, "profile-accept");
    assert!(dialog.read_with(visual, |dialog, _| dialog.step == Step::Name));
    assert!(visual.update(|window, cx| {
        dialog
            .read(cx)
            .input
            .read(cx)
            .focus_handle(cx)
            .is_focused(window)
    }));
    visual.simulate_input("   ");
    assert_eq!(
        dialog.read_with(visual, |dialog, cx| dialog
            .input
            .read(cx)
            .text()
            .to_string()),
        "   "
    );
    visual.simulate_keystrokes("enter");
    assert!(dialog.read_with(visual, |dialog, _| dialog.step == Step::Name));
    visual.simulate_keystrokes("cmd-a");
    visual.simulate_input("Personal Profile");
    assert_eq!(
        dialog.read_with(visual, |dialog, cx| dialog
            .input
            .read(cx)
            .text()
            .to_string()),
        "Personal Profile"
    );
    click(visual, "profile-accept");
    assert!(dialog.read_with(visual, |dialog, _| dialog.step == Step::Spaces));
    assert_eq!(
        dialog.read_with(visual, |dialog, _| dialog.selected_spaces.clone()),
        [source].into()
    );
    click(visual, "profile-back");
    assert_eq!(
        dialog.read_with(visual, |dialog, cx| dialog
            .input
            .read(cx)
            .text()
            .to_string()),
        "Personal Profile"
    );
    assert!(visual.update(|window, cx| {
        dialog
            .read(cx)
            .input
            .read(cx)
            .focus_handle(cx)
            .is_focused(window)
    }));
    click(visual, "profile-accept");
    click(visual, "profile-accept");
    click(visual, "profile-accept"); // pending submission cannot create twice
    assert_eq!(events.borrow().len(), 1);
    owner.read_with(visual, |owner, _| {
        let assigned = owner.browser.spaces[0].user_profile_id;
        assert_ne!(assigned, old);
        assert_eq!(owner.browser.user_profiles().count(), 2);
        assert_eq!(owner.browser.spaces[0].name, "Personal");
        assert_eq!(owner.browser.spaces[1].user_profile_id, old);
        assert_eq!(
            owner.browser.user_profile(assigned).unwrap().name(),
            "Personal Profile"
        );
        assert!(
            owner
                .browser
                .user_profile(assigned)
                .unwrap()
                .favorites
                .is_empty()
        );
        assert_eq!(
            owner.browser.user_profile(old).unwrap().favorites[0].id,
            old_favorite
        );
    });
}

#[gpui::test]
fn settings_requires_selection_and_cancel_has_no_model_effect(cx: &mut TestAppContext) {
    let (owner, dialog, visual, events, _subscription) =
        open(cx, fixture(), CreationSource::Settings, false);
    visual.simulate_input("Work Profile");
    click(visual, "profile-accept");
    assert!(dialog.read_with(visual, |dialog, _| dialog.selected_spaces.is_empty()));
    click(visual, "profile-accept");
    assert!(events.borrow().is_empty());
    click(visual, "profile-space-1");
    assert!(dialog.read_with(visual, |dialog, cx| dialog.enabled(cx)));
    click(visual, "profile-space-1");
    assert!(!dialog.read_with(visual, |dialog, cx| dialog.enabled(cx)));
    click(visual, "profile-space-0");
    visual.simulate_keystrokes("escape");
    visual.run_until_parked();
    click(visual, "profile-cancel");
    visual.simulate_keystrokes("enter");
    dialog.update(visual, |dialog, cx| dialog.close(cx));
    assert_eq!(events.borrow().len(), 1);
    assert!(matches!(events.borrow()[0], DialogEvent::Closed));
    assert_eq!(
        owner.read_with(visual, |owner, _| owner.browser.user_profiles().count()),
        1
    );
}

#[gpui::test]
fn settings_assigns_only_checked_spaces(cx: &mut TestAppContext) {
    let (owner, _, visual, _, _subscription) = open(cx, fixture(), CreationSource::Settings, false);
    visual.simulate_input("Another");
    visual.simulate_keystrokes("enter");
    click(visual, "profile-space-1");
    click(visual, "profile-accept");
    owner.read_with(visual, |owner, _| {
        assert_eq!(
            owner.browser.spaces[0].user_profile_id,
            owner.browser.default_user_profile_id()
        );
        assert_ne!(
            owner.browser.spaces[1].user_profile_id,
            owner.browser.default_user_profile_id()
        );
    });
}

#[gpui::test]
fn switching_dropdown_current_profile_and_selection(cx: &mut TestAppContext) {
    let mut browser = fixture();
    let source = browser.spaces[0].id;
    let other = browser.create_user_profile("Other").unwrap();
    let (owner, dialog, visual, events, _subscription) =
        open(cx, browser, CreationSource::File(source), true);
    let before = owner.read_with(visual, |owner, _| {
        browser_core::session::to_json(&owner.browser)
    });
    assert_eq!(
        dialog.read_with(visual, |dialog, _| dialog.selected_profile),
        Some(owner.read_with(visual, |owner, _| owner.browser.default_user_profile_id()))
    );
    click(visual, "profile-dropdown");
    click(visual, "profile-choice-1");
    assert!(events.borrow().is_empty());
    assert!(
        dialog.read_with(visual, |dialog, _| dialog.step == Step::Switch
            && !dialog.closed
            && !dialog.pending
            && !dialog.dropdown_open)
    );
    assert_eq!(
        dialog.read_with(visual, |dialog, _| dialog.selected_profile),
        Some(other)
    );
    click(visual, "profile-accept");
    assert!(events.borrow().is_empty());
    assert_eq!(
        owner.read_with(visual, |owner, _| browser_core::session::to_json(
            &owner.browser
        )),
        before
    );
    assert_eq!(
        owner.read_with(visual, |owner, _| owner.browser.spaces[0].user_profile_id),
        owner.read_with(visual, |owner, _| owner.browser.default_user_profile_id())
    );
    dialog.read_with(visual, |dialog, _| {
        assert!(dialog.step == Step::ConfirmSwitch);
        assert_eq!(dialog.confirmation_heading(), "Switch to your Other Profile?");
        assert_eq!(dialog.confirmation_body(), "Each Profile has different browser data, so your Personal Space might look a little different! For example, you might get logged out of some websites, see different Favorites up top, or need to re-install an extension.");
    });
    assert!(!dialog.read_with(visual, |dialog, _| dialog.dropdown_open));
    assert!(visual.debug_bounds("profile-switch-warning").is_some());
    assert!(visual.debug_bounds("profile-switch-irreversible").is_some());
    click(visual, "profile-accept");
    click(visual, "profile-accept");
    assert_eq!(events.borrow().len(), 1);
    assert_eq!(
        owner.read_with(visual, |owner, _| owner.browser.spaces[0].user_profile_id),
        other
    );
    assert!(matches!(
        events.borrow()[0],
        DialogEvent::Submit(ProfileRequest::Switch { .. })
    ));
}

#[gpui::test]
fn switch_confirmation_cancel_or_escape_preserves_model(cx: &mut TestAppContext) {
    for escape in [false, true] {
        let mut browser = fixture();
        let source = browser.spaces[1].id;
        let destination = browser.create_user_profile("Destination").unwrap();
        let (owner, dialog, visual, events, _subscription) =
            open(cx, browser, CreationSource::File(source), true);
        let before = owner.read_with(visual, |owner, _| {
            browser_core::session::to_json(&owner.browser)
        });
        click(visual, "profile-dropdown");
        click(visual, "profile-choice-1");
        assert!(events.borrow().is_empty());
        assert!(
            dialog.read_with(visual, |dialog, _| dialog.step == Step::Switch
                && !dialog.closed
                && !dialog.pending)
        );
        click(visual, "profile-accept");
        assert!(events.borrow().is_empty());
        dialog.read_with(visual, |dialog, _| {
            assert!(dialog.step == Step::ConfirmSwitch);
            assert_eq!(dialog.selected_profile, Some(destination));
            assert_eq!(dialog.switch_space.as_ref().unwrap().0, source);
            assert_eq!(dialog.switch_space.as_ref().unwrap().1, "Work");
            assert_eq!(
                dialog.confirmation_heading(),
                "Switch to your Destination Profile?"
            );
            assert!(dialog.confirmation_body().contains("your Work Space"));
        });
        if escape {
            visual.simulate_keystrokes("escape");
            visual.run_until_parked();
        } else {
            click(visual, "profile-cancel");
        }
        assert!(matches!(events.borrow().as_slice(), [DialogEvent::Closed]));
        assert_eq!(
            owner.read_with(visual, |owner, _| browser_core::session::to_json(
                &owner.browser
            )),
            before
        );
    }
}

#[gpui::test]
fn dropdown_new_profile_retains_origin_and_invalidates_old_form(cx: &mut TestAppContext) {
    let browser = fixture();
    let source = browser.spaces[1].id;
    let (owner, dialog, visual, events, _subscription) =
        open(cx, browser, CreationSource::File(source), true);
    click(visual, "profile-dropdown");
    click(visual, "profile-new");
    dialog.update(visual, |dialog, cx| dialog.close(cx));
    assert!(
        matches!(events.borrow().as_slice(), [DialogEvent::NewProfile(CreationSource::File(id))] if *id == source)
    );
    assert_eq!(
        owner.read_with(visual, |owner, _| owner.browser.user_profiles().count()),
        1
    );
}

#[gpui::test]
fn escape_closes_without_creating(cx: &mut TestAppContext) {
    let (owner, dialog, visual, events, _subscription) =
        open(cx, fixture(), CreationSource::Settings, false);
    visual.simulate_input("Unsaved");
    visual.simulate_keystrokes("escape");
    assert!(dialog.read_with(visual, |dialog, _| dialog.closed));
    assert!(matches!(events.borrow().as_slice(), [DialogEvent::Closed]));
    assert_eq!(
        owner.read_with(visual, |owner, _| owner.browser.user_profiles().count()),
        1
    );
}

#[gpui::test]
fn model_rejects_window_local_sources_and_rolls_back_invalid_assignment(cx: &mut TestAppContext) {
    for browser in [Browser::new_private(), Browser::new_blank()] {
        let (_, visual) = cx.add_window_view(|_, cx| {
            assert!(matches!(
                UserProfileDialog::creation(&browser, CreationSource::Settings, cx),
                Err(DialogError::Model(UserProfileError::WindowLocalModel))
            ));
            // A valid entity is needed only as this test window's root.
            UserProfileDialog::creation(&Browser::new(), CreationSource::Settings, cx).unwrap()
        });
        visual.run_until_parked();
    }
    let (owner, _, visual, _, _subscription) = open(cx, fixture(), CreationSource::Settings, false);
    let mut foreign = Browser::new();
    for index in 0..20 {
        foreign.add_space(format!("Foreign {index}"), 0.);
    }
    let missing = foreign.add_space("Missing", 0.);
    let source = owner.read_with(visual, |owner, _| owner.browser.spaces[0].id);
    let before = owner.read_with(visual, |owner, _| {
        browser_core::session::to_json(&owner.browser)
    });
    let result = owner.update(visual, |owner, _| {
        owner.apply_user_profile_dialog(&ProfileRequest::Create {
            source: CreationSource::File(source),
            name: "Rollback".into(),
            spaces: vec![source, missing],
        })
    });
    assert!(matches!(
        result,
        Err(DialogError::Model(UserProfileError::UnknownSpace))
    ));
    assert_eq!(
        owner.read_with(visual, |owner, _| browser_core::session::to_json(
            &owner.browser
        )),
        before
    );
}

#[gpui::test]
fn spaces_escape_returns_to_focused_retained_name_then_cancels(cx: &mut TestAppContext) {
    let (owner, dialog, visual, events, _subscription) =
        open(cx, fixture(), CreationSource::Settings, false);
    visual.simulate_input("Retained Name");
    click(visual, "profile-accept");
    click(visual, "profile-space-1");
    visual.simulate_keystrokes("escape");
    visual.run_until_parked();
    assert!(
        dialog.read_with(visual, |dialog, _| dialog.step == Step::Name
            && !dialog.closed)
    );
    assert_eq!(
        dialog.read_with(visual, |dialog, cx| dialog
            .input
            .read(cx)
            .text()
            .to_string()),
        "Retained Name"
    );
    assert!(visual.update(|window, cx| {
        dialog
            .read(cx)
            .input
            .read(cx)
            .focus_handle(cx)
            .is_focused(window)
    }));
    assert!(events.borrow().is_empty());
    visual.simulate_keystrokes("escape");
    visual.run_until_parked();
    assert!(dialog.read_with(visual, |dialog, _| dialog.closed));
    assert!(matches!(events.borrow().as_slice(), [DialogEvent::Closed]));
    assert_eq!(
        owner.read_with(visual, |owner, _| owner.browser.user_profiles().count()),
        1
    );
}

#[gpui::test]
fn five_space_cards_fit_observed_sheet_width(cx: &mut TestAppContext) {
    let mut browser = fixture();
    for name in ["Third", "Fourth", "Fifth"] {
        browser.add_space(name, 180.);
    }
    let (_, dialog, visual, events, _subscription) =
        open(cx, browser, CreationSource::Settings, false);
    visual.simulate_input("Layout");
    click(visual, "profile-accept");
    assert!(
        dialog.read_with(visual, |dialog, _| dialog.step == Step::Spaces
            && !dialog.closed)
    );
    assert!(visual.debug_bounds("profile-back-keycap").is_some());
    assert!(visual.debug_bounds("profile-enter-keycap").is_some());
    let sheet = visual.debug_bounds("user-profile-sheet").unwrap();
    let illustration = visual.debug_bounds("profile-illustration").unwrap();
    assert_eq!(illustration.size, gpui::size(px(64.), px(64.)));
    assert_eq!(sheet.size.height, px(506.));
    let first = visual.debug_bounds("profile-space-0").unwrap();
    let second = visual.debug_bounds("profile-space-1").unwrap();
    let fifth = visual.debug_bounds("profile-space-4").unwrap();
    assert_eq!(sheet.size.width, px(450.));
    assert_eq!(first.size.width, px(64.));
    assert_eq!(second.origin.x - first.origin.x, px(76.));
    assert_eq!(first.origin.x - sheet.origin.x, px(38.));
    assert!(fifth.origin.x + fifth.size.width <= sheet.origin.x + sheet.size.width - px(38.));
    let heading = visual.debug_bounds("profile-heading").unwrap();
    let body = visual.debug_bounds("profile-body").unwrap();
    let tile = visual.debug_bounds("profile-space-tile-0").unwrap();
    let check = visual.debug_bounds("profile-space-check-0").unwrap();
    let label = visual.debug_bounds("profile-space-label-0").unwrap();
    let back = visual.debug_bounds("profile-back").unwrap();
    let create = visual.debug_bounds("profile-accept").unwrap();
    assert_eq!(heading.origin.y - illustration.bottom(), px(18.));
    assert_eq!(body.origin.y - heading.bottom(), px(12.));
    assert_eq!(body.size.height, px(90.)); // Five lines at 18 px.
    assert_eq!(tile.origin.y - body.bottom(), px(10.));
    assert_eq!(tile.size, gpui::size(px(64.), px(95.)));
    assert_eq!(check.origin.y - tile.bottom(), px(12.));
    assert_eq!(label.origin.y - check.bottom(), px(8.));
    assert!(back.origin.y - label.bottom() >= px(30.));
    assert_eq!(back.origin.y, create.origin.y);
    assert_eq!(back.origin.x - sheet.origin.x, px(165.));
    assert_eq!(create.origin.x - back.right(), px(8.));
    assert_eq!(sheet.right() - create.right(), px(38.));
    assert_eq!(create.size.height, px(36.));
    assert_eq!(sheet.bottom() - create.bottom(), px(28.));
    // Debug bounds can retain controls from previous frames. Exercise the
    // current footer instead of inferring visibility from stale bounds.
    click(visual, "profile-back");
    assert!(
        dialog.read_with(visual, |dialog, _| dialog.step == Step::Name
            && !dialog.closed)
    );
    assert_eq!(
        dialog.read_with(visual, |dialog, cx| dialog
            .input
            .read(cx)
            .text()
            .to_string()),
        "Layout"
    );
    assert!(events.borrow().is_empty());
}

#[gpui::test]
fn settings_creation_selects_space_and_submits_without_mouse(cx: &mut TestAppContext) {
    let browser = fixture();
    let target = browser.spaces[1].id;
    let (owner, dialog, visual, events, _subscription) =
        open(cx, browser, CreationSource::Settings, false);
    visual.simulate_input("Keyboard Profile");
    visual.simulate_keystrokes("enter");
    visual.simulate_keystrokes("tab");
    assert!(visual.update(|window, cx| dialog.read(cx).spaces[0].focus.is_focused(window)));
    visual.simulate_keystrokes("right");
    assert!(visual.update(|window, cx| dialog.read(cx).spaces[1].focus.is_focused(window)));
    visual.simulate_keystrokes("space");
    assert_eq!(
        dialog.read_with(visual, |dialog, _| dialog.selected_spaces.clone()),
        [target].into()
    );
    visual.simulate_keystrokes("space");
    assert!(!dialog.read_with(visual, |dialog, cx| dialog.enabled(cx)));
    visual.simulate_keystrokes("enter");
    assert!(events.borrow().is_empty());
    visual.simulate_keystrokes("space");
    visual.simulate_keystrokes("left");
    visual.simulate_keystrokes("tab");
    assert!(visual.update(|window, cx| dialog.read(cx).spaces[1].focus.is_focused(window)));
    visual.simulate_keystrokes("enter");
    assert_eq!(events.borrow().len(), 1);
    owner.read_with(visual, |owner, _| {
        assert_eq!(
            owner.browser.spaces[0].user_profile_id,
            owner.browser.default_user_profile_id()
        );
        let assigned = owner.browser.spaces[1].user_profile_id;
        assert_ne!(assigned, owner.browser.default_user_profile_id());
        assert_eq!(
            owner.browser.user_profile(assigned).unwrap().name(),
            "Keyboard Profile"
        );
    });
}

#[gpui::test]
fn switching_selects_destination_and_confirms_without_mouse(cx: &mut TestAppContext) {
    let mut browser = fixture();
    let space = browser.spaces[0].id;
    let original = browser.default_user_profile_id();
    let destination = browser.create_user_profile("Keyboard Destination").unwrap();
    let (owner, dialog, visual, events, _subscription) =
        open(cx, browser, CreationSource::File(space), true);
    visual.simulate_keystrokes("tab");
    assert!(visual.update(|window, cx| dialog.read(cx).dropdown_focus.is_focused(window)));
    visual.simulate_keystrokes("space");
    visual.simulate_keystrokes("down");
    assert_eq!(
        dialog.read_with(visual, |dialog, _| dialog.selected_profile),
        Some(original)
    );
    visual.simulate_keystrokes("escape");
    assert!(dialog.read_with(visual, |dialog, _| !dialog.dropdown_open && !dialog.closed));
    for key in ["enter", "down", "up", "down", "enter"] {
        visual.simulate_keystrokes(key);
    }
    assert_eq!(
        dialog.read_with(visual, |dialog, _| dialog.selected_profile),
        Some(destination)
    );
    assert!(events.borrow().is_empty());
    visual.simulate_keystrokes("tab");
    assert!(visual.update(|window, cx| dialog.read(cx).secondary_focus.is_focused(window)));
    visual.simulate_keystrokes("shift-tab");
    assert!(visual.update(|window, cx| dialog.read(cx).dropdown_focus.is_focused(window)));
    for key in ["tab", "tab", "enter"] {
        visual.simulate_keystrokes(key);
    }
    assert!(dialog.read_with(visual, |dialog, _| dialog.step == Step::ConfirmSwitch));
    assert!(events.borrow().is_empty());
    assert_eq!(
        owner.read_with(visual, |owner, _| owner.browser.spaces[0].user_profile_id),
        original
    );
    visual.simulate_keystrokes("enter");
    visual.simulate_keystrokes("enter");
    assert_eq!(events.borrow().len(), 1);
    assert_eq!(
        owner.read_with(visual, |owner, _| owner.browser.spaces[0].user_profile_id),
        destination
    );
}

#[gpui::test]
fn keyboard_dropdown_reaches_new_profile(cx: &mut TestAppContext) {
    let browser = fixture();
    let space = browser.spaces[1].id;
    let (owner, _, visual, events, _subscription) =
        open(cx, browser, CreationSource::File(space), true);
    for key in ["tab", "space", "end", "home", "end", "enter"] {
        visual.simulate_keystrokes(key);
    }
    assert!(
        matches!(events.borrow().as_slice(), [DialogEvent::NewProfile(CreationSource::File(id))] if *id == space)
    );
    assert_eq!(
        owner.read_with(visual, |owner, _| owner.browser.user_profiles().count()),
        1
    );
}

fn assert_highlighted_profile_visible(
    dialog: &Entity<UserProfileDialog>,
    visual: &mut VisualTestContext,
) {
    visual.run_until_parked();
    let (viewport, row, offset) = dialog.read_with(visual, |dialog, _| {
        assert!(dialog.dropdown_open);
        let scroll = &dialog.dropdown_scroll;
        (
            scroll.bounds(),
            scroll
                .bounds_for_item(dialog.dropdown_index)
                .expect("highlighted row laid out"),
            scroll.offset(),
        )
    });
    let top = row.top() + offset.y;
    let bottom = row.bottom() + offset.y;
    assert!(
        top >= viewport.top(),
        "highlighted row above menu: {row:?}, {viewport:?}, {offset:?}"
    );
    assert!(
        bottom <= viewport.bottom(),
        "highlighted row below menu: {row:?}, {viewport:?}, {offset:?}"
    );
    let height = visual.update(|window, _| window.viewport_size().height);
    assert!(viewport.top() >= px(0.));
    assert!(viewport.bottom() <= height);
    assert!(viewport.size.height <= px(240.));
}

#[gpui::test]
fn many_profile_keyboard_navigation_reveals_destination_and_new_profile(cx: &mut TestAppContext) {
    let mut browser = fixture();
    let space = browser.spaces[0].id;
    let mut destination = browser.default_user_profile_id();
    for index in 1..=30 {
        destination = browser
            .create_user_profile(&format!("Profile {index:02}"))
            .unwrap();
    }
    let (owner, dialog, visual, events, _subscription) =
        open(cx, browser, CreationSource::File(space), true);
    for key in ["tab", "space"] {
        visual.simulate_keystrokes(key);
    }
    assert_highlighted_profile_visible(&dialog, visual);
    assert!(dialog.read_with(
        visual,
        |dialog, _| dialog.dropdown_scroll.max_offset().height > px(0.)
    ));
    visual.simulate_keystrokes("end");
    assert_eq!(
        dialog.read_with(visual, |dialog, _| dialog.dropdown_index),
        31
    );
    assert_highlighted_profile_visible(&dialog, visual);
    visual.simulate_keystrokes("home");
    assert_eq!(
        dialog.read_with(visual, |dialog, _| dialog.dropdown_index),
        0
    );
    assert_highlighted_profile_visible(&dialog, visual);
    for _ in 0..30 {
        visual.simulate_keystrokes("down");
        assert_highlighted_profile_visible(&dialog, visual);
    }
    visual.simulate_keystrokes("enter");
    assert_eq!(
        dialog.read_with(visual, |dialog, _| dialog.selected_profile),
        Some(destination)
    );
    assert!(events.borrow().is_empty());
    for key in ["space", "end"] {
        visual.simulate_keystrokes(key);
    }
    assert_highlighted_profile_visible(&dialog, visual);
    visual.simulate_keystrokes("enter");
    assert!(
        matches!(events.borrow().as_slice(), [DialogEvent::NewProfile(CreationSource::File(id))] if *id == space)
    );
    assert_eq!(
        owner.read_with(visual, |owner, _| owner.browser.spaces[0].user_profile_id),
        owner.read_with(visual, |owner, _| owner.browser.default_user_profile_id())
    );
}

#[gpui::test]
fn initial_switch_sheet_matches_measured_reference_and_fits_long_labels(cx: &mut TestAppContext) {
    assert_eq!(
        SWITCH_WARNING,
        "Each Profile has different browser data, so you might get logged out of some websites, see different Favorites up top, or need to re-install extensions."
    );
    for long_labels in [false, true] {
        let mut browser = fixture();
        let source = browser.spaces[0].id;
        if long_labels {
            browser.rename_space(source, "Echo Space Reference 20261023");
            browser
                .rename_user_profile(browser.default_user_profile_id(), "Grosst")
                .unwrap();
        }
        let (_, dialog, visual, events, _subscription) =
            open(cx, browser, CreationSource::File(source), true);
        let sheet = visual.debug_bounds("user-profile-sheet").unwrap();
        let art = visual.debug_bounds("profile-illustration").unwrap();
        let heading = visual.debug_bounds("profile-heading").unwrap();
        let body = visual.debug_bounds("profile-body").unwrap();
        let row = visual.debug_bounds("profile-switch-row").unwrap();
        let label = visual.debug_bounds("profile-switch-label").unwrap();
        let pill = visual.debug_bounds("profile-dropdown").unwrap();
        let footer = visual.debug_bounds("profile-footer").unwrap();
        let cancel = visual.debug_bounds("profile-cancel").unwrap();
        let change = visual.debug_bounds("profile-accept").unwrap();
        let extra = if long_labels { px(19.) } else { px(0.) };
        assert_eq!(sheet.size, gpui::size(px(450.), px(338.) + extra));
        assert_eq!(art.origin - sheet.origin, gpui::point(px(38.), px(38.)));
        assert_eq!(art.size, gpui::size(px(64.), px(64.)));
        assert_eq!(
            heading.origin - sheet.origin,
            gpui::point(px(38.), px(126.))
        );
        assert_eq!(heading.size.height, px(22.) + extra);
        assert_eq!(
            body.origin - sheet.origin,
            gpui::point(px(38.), px(160.) + extra)
        );
        assert_eq!(body.size, gpui::size(px(374.), px(54.)));
        assert_eq!(
            row.origin - sheet.origin,
            gpui::point(px(38.), px(228.) + extra)
        );
        assert_eq!(row.size, gpui::size(px(374.), px(24.)));
        assert_eq!(label.origin.x, row.origin.x);
        assert_eq!(pill.origin.x - label.right(), px(8.));
        assert!(pill.right() <= row.right());
        assert!(pill.size.width <= px(160.));
        assert_eq!(pill.size.height, px(24.));
        if long_labels {
            assert!(
                pill.size.width < px(160.),
                "a short selected name must not stretch its pill to the cap"
            );
            let (full_width, outside_style, selected_id, selected_name, selected_text_width) =
                visual.update(|window, cx| {
                    let text = "Echo Space Reference 20261023’s Profile";
                    let mut style = window.text_style().clone();
                    let outside_style = style.clone();
                    style.font_weight = gpui::FontWeight::BOLD;
                    let full_width = window
                        .text_system()
                        .shape_line(text.into(), px(12.), &[style.to_run(text.len())], None)
                        .width;
                    let dialog = dialog.read(cx);
                    let selected_name = dialog
                        .profiles
                        .iter()
                        .find(|profile| Some(profile.id) == dialog.selected_profile)
                        .unwrap()
                        .name
                        .clone();
                    style.font_weight = gpui::FontWeight::NORMAL;
                    let selected_text_width = window
                        .text_system()
                        .shape_line(
                            selected_name.clone().into(),
                            px(13.),
                            &[style.to_run(selected_name.len())],
                            None,
                        )
                        .width;
                    (
                        full_width,
                        outside_style,
                        dialog.selected_profile,
                        selected_name,
                        selected_text_width,
                    )
                });
            let metrics = format!(
                "full_width={full_width:?}, selected_text_width={selected_text_width:?}, pill={pill:?}, label={label:?}, row={row:?}, selected_id={selected_id:?}, selected_name={selected_name:?}, outside_style={outside_style:?}; render overrides: label=12px bold, pill=13px normal, inherited family"
            );
            eprintln!("[profile-row-metrics] {metrics}");
            // GPUI TestPlatform uses NoopTextSystem, whose layout_line ignores
            // font runs and gives every BMP character a 0.6 * font_size advance.
            // That makes this label 280.8 px and Grosst's pill 91 px: 379.8 px
            // including the gap. Native variable-width font fit is a separate
            // screenshot gate; never alter the measured UI to fit fake metrics.
            if full_width + pill.size.width + px(8.) <= row.size.width {
                assert!(
                    label.size.width + px(1.) >= full_width,
                    "fitting label was elided: {metrics}"
                );
            } else {
                assert!(
                    label.size.width < full_width,
                    "oversize label must truncate: {metrics}"
                );
                assert!(label.size.width > px(0.), "label collapsed: {metrics}");
                assert_eq!(
                    label.size.width + pill.size.width + px(8.),
                    row.size.width,
                    "truncated label must use the remaining row width: {metrics}"
                );
            }
        }
        assert_eq!(
            footer.origin - sheet.origin,
            gpui::point(px(38.), px(274.) + extra)
        );
        assert_eq!(footer.size, gpui::size(px(374.), px(36.)));
        assert_eq!(cancel.origin.y - sheet.origin.y, px(274.) + extra);
        assert_eq!(cancel.size, gpui::size(px(110.), px(36.)));
        assert_eq!(change.size, gpui::size(px(148.), px(36.)));
        assert_eq!(change.origin.x - cancel.right(), px(8.));
        assert_eq!(change.origin.y, cancel.origin.y);
        assert_eq!(sheet.right() - change.right(), px(38.));
        assert_eq!(sheet.bottom() - change.bottom(), px(28.));
        // Opening the menu must preserve the fixed sheet and compact trigger.
        for key in ["tab", "enter"] {
            visual.simulate_keystrokes(key);
        }
        visual.run_until_parked();
        assert!(dialog.read_with(visual, |dialog, _| dialog.dropdown_open));
        assert_eq!(visual.debug_bounds("user-profile-sheet").unwrap(), sheet);
        assert_eq!(visual.debug_bounds("profile-dropdown").unwrap(), pill);
        assert_eq!(visual.debug_bounds("profile-footer").unwrap(), footer);
        visual.simulate_keystrokes("escape");
        assert!(
            dialog.read_with(visual, |dialog, _| dialog.step == Step::Switch
                && !dialog.closed)
        );
        assert!(events.borrow().is_empty());
    }
}

#[gpui::test]
fn very_long_open_menu_names_remain_single_line_and_keyboard_visible(cx: &mut TestAppContext) {
    let mut browser = fixture();
    let source = browser.spaces[0].id;
    let default = browser.default_user_profile_id();
    let full_name = "A very long profile name with spaces and an_unbroken_segment_".repeat(80);
    browser.rename_user_profile(default, &full_name).unwrap();
    for index in 1..=15 {
        browser
            .create_user_profile(&format!("Profile {index}"))
            .unwrap();
    }
    let destination = browser.create_user_profile(&full_name).unwrap();
    let (owner, dialog, visual, events, _subscription) =
        open(cx, browser, CreationSource::File(source), true);
    let sheet = visual.debug_bounds("user-profile-sheet").unwrap();
    for key in ["tab", "enter"] {
        visual.simulate_keystrokes(key);
    }
    assert_highlighted_profile_visible(&dialog, visual);
    let first = visual.debug_bounds("profile-choice-0").unwrap();
    let first_label = visual.debug_bounds("profile-choice-label-0").unwrap();
    assert_eq!(first.size.height, px(32.));
    assert!(first.size.width <= px(374.));
    assert!(first_label.size.height <= px(18.));
    assert!(first_label.right() <= first.right() - px(10.));
    visual.simulate_keystrokes("end");
    assert_highlighted_profile_visible(&dialog, visual);
    assert_eq!(
        visual.debug_bounds("profile-new").unwrap().size.height,
        px(32.)
    );
    visual.simulate_keystrokes("up");
    assert_highlighted_profile_visible(&dialog, visual);
    let last = visual.debug_bounds("profile-choice-16").unwrap();
    let last_label = visual.debug_bounds("profile-choice-label-16").unwrap();
    assert_eq!(last.size.height, px(32.));
    assert!(last_label.size.height <= px(18.));
    assert!(last_label.right() <= last.right() - px(10.));
    visual.simulate_keystrokes("home");
    assert_highlighted_profile_visible(&dialog, visual);
    for _ in 0..16 {
        visual.simulate_keystrokes("down");
    }
    assert_highlighted_profile_visible(&dialog, visual);
    assert_eq!(visual.debug_bounds("user-profile-sheet").unwrap(), sheet);
    visual.simulate_keystrokes("enter");
    assert_eq!(
        dialog.read_with(visual, |dialog, _| dialog.selected_profile),
        Some(destination)
    );
    owner.read_with(visual, |owner, _| {
        assert_eq!(
            owner.browser.user_profile(destination).unwrap().name(),
            full_name
        );
        assert_eq!(owner.browser.spaces[0].user_profile_id, default);
    });
    assert!(events.borrow().is_empty());
}

#[gpui::test]
fn deferred_option_over_footer_does_not_click_cancel_or_submit(cx: &mut TestAppContext) {
    let mut browser = fixture();
    let space = browser.spaces[0].id;
    let destination = browser.create_user_profile("Other").unwrap();
    let (owner, dialog, visual, events, _subscription) =
        open(cx, browser, CreationSource::File(space), true);
    let before = owner.read_with(visual, |owner, _| {
        browser_core::session::to_json(&owner.browser)
    });
    click(visual, "profile-dropdown");
    let option = visual.debug_bounds("profile-choice-1").unwrap();
    let cancel = visual.debug_bounds("profile-cancel").unwrap();
    let submit = visual.debug_bounds("profile-accept").unwrap();
    // Deliberately click the shared painted area. A normal center click can
    // accidentally miss the underlying control and conceal a hit-test bug.
    let overlap = [cancel, submit]
        .into_iter()
        .find_map(|button| {
            let left = option.left().max(button.left());
            let right = option.right().min(button.right());
            let top = option.top().max(button.top());
            let bottom = option.bottom().min(button.bottom());
            (right > left && bottom > top).then(|| {
                gpui::Bounds::new(
                    gpui::point(left, top),
                    gpui::size(right - left, bottom - top),
                )
            })
        })
        .expect("reference layout must exercise a menu option over a footer button");
    visual.simulate_mouse_move(overlap.center(), None, Modifiers::none());
    visual.run_until_parked();
    visual.simulate_click(overlap.center(), Modifiers::none());
    visual.run_until_parked();
    assert!(
        dialog.read_with(visual, |dialog, _| dialog.step == Step::Switch
            && !dialog.closed
            && !dialog.pending
            && !dialog.dropdown_open)
    );
    assert_eq!(
        dialog.read_with(visual, |dialog, _| dialog.selected_profile),
        Some(destination)
    );
    assert!(events.borrow().is_empty());
    assert_eq!(
        owner.read_with(visual, |owner, _| browser_core::session::to_json(
            &owner.browser
        )),
        before
    );
    // The next distinct click opens confirmation; it still must not submit.
    click(visual, "profile-accept");
    assert!(
        dialog.read_with(visual, |dialog, _| dialog.step == Step::ConfirmSwitch
            && !dialog.closed)
    );
    assert!(events.borrow().is_empty());
    click(visual, "profile-cancel");
    assert!(matches!(events.borrow().as_slice(), [DialogEvent::Closed]));
    assert_eq!(
        owner.read_with(visual, |owner, _| browser_core::session::to_json(
            &owner.browser
        )),
        before
    );
}

#[gpui::test]
fn unusually_long_switch_row_labels_stay_bounded_beside_pill(cx: &mut TestAppContext) {
    let mut browser = fixture();
    let source = browser.spaces[0].id;
    browser.rename_space(source, &"A long workspace label ".repeat(5));
    browser
        .rename_user_profile(
            browser.default_user_profile_id(),
            &"A long profile label ".repeat(10),
        )
        .unwrap();
    let (_, _, visual, _, _subscription) = open(cx, browser, CreationSource::File(source), true);
    let row = visual.debug_bounds("profile-switch-row").unwrap();
    let label = visual.debug_bounds("profile-switch-label").unwrap();
    let pill = visual.debug_bounds("profile-dropdown").unwrap();
    assert_eq!(row.size.width, px(374.));
    assert_eq!(label.origin.x, row.origin.x);
    assert_eq!(pill.origin.x - label.right(), px(8.));
    assert!(pill.right() <= row.right());
    assert!(pill.size.width <= px(160.));
    assert!(label.size.height <= px(18.));
}
