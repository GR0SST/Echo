//! Runtime-reference correction: Library chrome and selected native page coexist.
use super::*;
use crate::{library::LibrarySection, pages::PageViewport, sidebar_pages::SidebarPage};
use gpui::px;

fn enter(view: &Entity<BrowserWindow>, cx: &mut VisualTestContext) {
    cx.simulate_keystrokes("cmd-shift-l");
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert_eq!(view.sidebar_navigation.controller.selected(), SidebarPage::Library);
        assert_eq!(view.sidebar_navigation.library_section, LibrarySection::Media);
        assert!(!view.page_content_visible(), "Library still owns browser chrome");
    });
}

fn assert_preview(fake: &FakeEngine, document: ViewId, cx: &mut VisualTestContext) {
    let preview = cx.debug_bounds("library-page-canvas").unwrap();
    let actual = fake.0.borrow().bounds[&raw(document)];
    assert_eq!(actual, Bounds {
        x: f64::from(f32::from(preview.origin.x)),
        y: f64::from(f32::from(preview.origin.y)),
        width: f64::from(f32::from(preview.size.width)),
        height: f64::from(f32::from(preview.size.height)),
    }, "native viewport must come from actual GPUI preview bounds");
    assert!(actual.width > 0. && actual.height > 0.);
}

fn unchanged(calls: &[Call]) {
    assert!(!calls.iter().any(|call| matches!(call,
        Call::Create(..) | Call::Close(..) | Call::Navigate(..) | Call::MoveHost(..)
        | Call::Reload(..) | Call::ForceReload(..))), "{calls:?}");
}

#[gpui::test]
fn media_retains_the_same_page_in_measured_preview_and_search_keeps_focus(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let document = fake.0.borrow().focused_page.unwrap();
    let before = view.read_with(cx, |view, _| view.session_json());
    fake.take_calls();
    enter(&view, cx);
    assert_preview(&fake, document, cx);
    click(cx, "library-media-search");
    cx.simulate_input("retained query");
    for viewport in [gpui::size(px(1280.), px(800.)), gpui::size(px(970.), px(660.))] {
        cx.simulate_resize(viewport);
        cx.run_until_parked();
        assert_preview(&fake, document, cx);
    }
    cx.simulate_input(" plus");
    view.read_with(cx, |view, cx| {
        assert_eq!(view.sidebar_navigation.media.as_ref().unwrap().read(cx).query(cx), "retained query plus");
        assert!(view.pages.as_ref().unwrap().borrow().is_visible(view.browser.active_tab().unwrap().id));
        assert_eq!(view.session_json(), before);
    });
    let calls = fake.take_calls();
    unchanged(&calls);
    assert!(!calls.iter().any(|call| matches!(call, Call::FocusPage(_))), "{calls:?}");
}

#[gpui::test]
fn spaces_hides_media_preview_and_return_restores_the_same_document(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let document = fake.0.borrow().focused_page.unwrap();
    enter(&view, cx);
    fake.take_calls();
    click(cx, "library-nav-spaces");
    view.read_with(cx, |view, _| {
        assert!(!view.native_page_content_visible());
        assert!(!view.pages.as_ref().unwrap().borrow().is_visible(view.browser.active_tab().unwrap().id));
    });
    assert!(fake.take_calls().contains(&Call::Visible(raw(document), false)));
    click(cx, "library-nav-media");
    assert_preview(&fake, document, cx);
    view.read_with(cx, |view, _| assert!(view.pages.as_ref().unwrap().borrow().is_visible(view.browser.active_tab().unwrap().id)));
    let calls = fake.take_calls();
    unchanged(&calls);
    assert!(calls.contains(&Call::Visible(raw(document), true)));
    assert!(!calls.iter().any(|call| matches!(call, Call::FocusPage(_))));
}

#[gpui::test]
fn an_empty_normal_window_keeps_a_blank_media_preview(cx: &mut TestAppContext) {
    cx.update(init);
    let fake = FakeEngine::default();
    let (view, cx) = cx.add_window_view(|window, cx| {
        let view = BrowserWindow::new(browser_core::Browser::new(), Some(Box::new(fake.clone())), window, cx);
        view.focus(window);
        view
    });
    cx.run_until_parked();
    enter(&view, cx);
    assert!(cx.debug_bounds("library-media-preview").unwrap().size.width > px(0.));
    view.read_with(cx, |view, _| {
        assert!(view.browser.active_tab().is_none());
        assert!(view.pages.as_ref().unwrap().borrow().surface_bounds().is_empty());
    });
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::FocusPage(..))));
}

#[gpui::test]
fn a_native_preview_focus_event_preserves_library_and_can_return_to_query(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let document = fake.0.borrow().focused_page.unwrap();
    enter(&view, cx);
    click(cx, "library-media-search");
    cx.simulate_input("before");
    fake.send(cx, Event::Focused { view: document });
    view.read_with(cx, |view, _| {
        assert_eq!(view.sidebar_navigation.controller.selected(), SidebarPage::Library);
        assert!(view.pages.as_ref().unwrap().borrow().capture_resize_focus().has_native_focus());
        assert!(!view.sidebar_navigation.media_search_focus);
    });
    click(cx, "library-media-search");
    cx.simulate_input("after");
    view.read_with(cx, |view, cx| {
        let query = view.sidebar_navigation.media.as_ref().unwrap().read(cx).query(cx);
        assert!(query.contains("before") && query.contains("after"), "{query}");
    });
    assert!(fake.take_calls().contains(&Call::FocusUi));
    assert_preview(&fake, document, cx);
}

#[gpui::test]
fn dismissal_and_stale_normal_canvas_cannot_override_media_bounds_or_query_focus(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let document = fake.0.borrow().focused_page.unwrap();
    let (pages, old_generation, old_bounds) = view.read_with(cx, |view, _| {
        let pages = view.pages.as_ref().unwrap().clone();
        let generation = pages.borrow().viewport_generation();
        let bounds = fake.0.borrow().bounds[&raw(document)];
        (pages, generation, bounds)
    });
    enter(&view, cx);
    click(cx, "library-media-search");
    cx.simulate_input("keep");
    cx.simulate_keystrokes("cmd-t");
    cx.run_until_parked();
    fake.take_calls();
    bar(&view, cx).simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(!pages.borrow_mut().set_viewport_bounds(old_generation, old_bounds));
    cx.simulate_input(" query");
    cx.run_until_parked();
    assert_preview(&fake, document, cx);
    view.read_with(cx, |view, cx| {
        assert!(view.command_bar.is_none());
        assert_eq!(view.sidebar_navigation.controller.selected(), SidebarPage::Library);
        assert_eq!(view.sidebar_navigation.media.as_ref().unwrap().read(cx).query(cx), "keep query");
    });
    let calls = fake.take_calls();
    unchanged(&calls);
    assert!(!calls.iter().any(|call| matches!(call, Call::FocusPage(_))), "{calls:?}");
}

#[gpui::test]
fn library_profile_reassignment_retires_the_old_generation_before_preview_input(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let document = fake.0.borrow().focused_page.unwrap();
    enter(&view, cx);
    click(cx, "library-media-search");
    cx.simulate_input("same query");
    let old = view.read_with(cx, |view, _| view.pages.as_ref().unwrap().borrow().identity_for(document).unwrap());
    view.update(cx, |view, cx| {
        view.edit_browser(|browser| {
            let profile = browser.create_user_profile("Library generation test").unwrap();
            browser.assign_space_user_profile(browser.space().id, profile).unwrap();
        });
        cx.notify();
    });
    cx.run_until_parked();
    fake.send(cx, Event::Focused { view: document });
    view.read_with(cx, |view, cx| {
        let pages = view.pages.as_ref().unwrap().borrow();
        assert!(!pages.accepts_identity(old));
        let current = page(fake.0.borrow().next_view);
        let identity = pages.identity_for(current).unwrap();
        assert_ne!(identity.generation, old.generation);
        assert_ne!(identity.profile, old.profile);
        assert!(pages.is_visible(old.tab));
        assert_eq!(view.sidebar_navigation.controller.selected(), SidebarPage::Library);
        assert!(view.sidebar_navigation.media_search_focus, "retired native event must not steal query intent");
        assert_eq!(view.sidebar_navigation.media.as_ref().unwrap().read(cx).query(cx), "same query");
    });
    assert!(fake.take_calls().contains(&Call::Close(raw(document))));
}

#[gpui::test]
fn repeated_open_keeps_media_query_and_its_native_preview(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let document = fake.0.borrow().focused_page.unwrap();
    enter(&view, cx);
    click(cx, "library-media-search");
    cx.simulate_input("repeat");
    fake.take_calls();
    cx.simulate_keystrokes("cmd-shift-l");
    dispatch_menu(cx, "Window", "Open Library…");
    cx.run_until_parked();
    cx.simulate_input(" stays");
    view.read_with(cx, |view, cx| {
        assert_eq!(view.sidebar_navigation.library_section, LibrarySection::Media);
        assert_eq!(view.sidebar_navigation.controller.selected(), SidebarPage::Library);
        assert_eq!(view.sidebar_navigation.media.as_ref().unwrap().read(cx).query(cx), "repeat stays");
    });
    assert_preview(&fake, document, cx);
    let calls = fake.take_calls();
    unchanged(&calls);
    assert!(!calls.iter().any(|call| matches!(call, Call::FocusPage(_))));
}

#[gpui::test]
fn downloads_preview_seam_accepts_presenter_bounds_and_rejects_media_generation(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let document = fake.0.borrow().focused_page.unwrap();
    enter(&view, cx);
    let pages = view.read_with(cx, |view, _| view.pages.as_ref().unwrap().clone());
    let media_generation = pages.borrow().viewport_generation();
    // Presenter integration owns its Downloads layout. Exercise the same host
    // seam without inventing a Downloads presenter or assuming its list width.
    let mut pages = pages.borrow_mut();
    pages.set_viewport(PageViewport::LibraryDownloads);
    let generation = pages.viewport_generation();
    let bounds = Bounds { x: 427., y: 19., width: 709., height: 631. };
    assert!(!pages.set_viewport_bounds(media_generation, bounds));
    assert!(pages.set_viewport_bounds(generation, bounds));
    assert_eq!(fake.0.borrow().bounds[&raw(document)], bounds);
}

#[gpui::test]
fn media_retains_gpui_history_without_allocating_a_fake_native_page(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    cx.simulate_keystrokes("cmd-y");
    cx.run_until_parked();
    assert!(view.read_with(cx, |view, _| view.history_is_open()));
    fake.take_calls();
    enter(&view, cx);
    let preview = cx.debug_bounds("library-media-preview").unwrap();
    let history = cx.debug_bounds("history-page").unwrap();
    assert!(history.origin.x >= preview.origin.x && history.right() <= preview.right());
    assert!(history.origin.y >= preview.origin.y && history.bottom() <= preview.bottom());
    view.read_with(cx, |view, _| {
        assert!(view.pages.as_ref().unwrap().borrow().surface_bounds().is_empty());
    });
    click(cx, "library-media-search");
    cx.simulate_input("media, not history");
    cx.run_until_parked();
    view.read_with(cx, |view, cx| {
        assert_eq!(view.sidebar_navigation.media.as_ref().unwrap().read(cx).query(cx), "media, not history");
    });
    unchanged(&fake.take_calls());
    // Exact reviewed sequence: Media search -> real GPUI History search ->
    // Cmd+T -> dismiss. Subsequent typing must return to History's responder.
    let history_input = view.read_with(cx, |view, _| view.history_page.input.clone());
    history_input.update(cx, |input, cx| input.set_text("", cx));
    let history_bounds = cx.debug_bounds("history-search").unwrap();
    let position = gpui::point(history_bounds.origin.x + px(24.), history_bounds.center().y);
    cx.simulate_mouse_move(position, None, Modifiers::none());
    cx.run_until_parked();
    cx.simulate_click(position, Modifiers::none());
    cx.run_until_parked();
    assert!(cx.update(|window, cx| history_input.read(cx).focus_handle(cx).is_focused(window)));
    cx.simulate_input("history before");
    cx.simulate_keystrokes("cmd-t");
    cx.run_until_parked();
    bar(&view, cx).simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(cx.update(|window, cx| history_input.read(cx).focus_handle(cx).is_focused(window)));
    cx.simulate_input(" after");
    cx.run_until_parked();
    view.read_with(cx, |view, cx| {
        assert!(!view.sidebar_navigation.media_search_focus);
        assert_eq!(view.history_page.input.read(cx).text(), "history before after");
        assert_eq!(view.sidebar_navigation.media.as_ref().unwrap().read(cx).query(cx), "media, not history");
        assert_eq!(view.sidebar_navigation.controller.selected(), SidebarPage::Library);
    });
    unchanged(&fake.take_calls());
}

#[gpui::test]
fn media_preserves_native_source_and_existing_split_layout_contracts(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    cx.simulate_keystrokes("cmd-alt-u");
    cx.run_until_parked();
    let source = fake.0.borrow().focused_page.unwrap();
    assert!(view.read_with(cx, |view, _| view.browser.active_tab().unwrap().url.starts_with("view-source:")));
    fake.take_calls();
    enter(&view, cx);
    assert_preview(&fake, source, cx);
    unchanged(&fake.take_calls());
    click(cx, "library-nav-spaces");
    click(cx, "library-space-select-0");
    let (left, right) = view.update(cx, |view, cx| {
        let pair = view.edit_browser(|browser| {
            let left = browser.new_tab("https://preview.example/left", std::time::SystemTime::UNIX_EPOCH);
            let right = browser.new_tab("https://preview.example/right", std::time::SystemTime::UNIX_EPOCH);
            browser.add_split(left, right, browser_core::splits::Placement::Right, std::time::SystemTime::UNIX_EPOCH).unwrap();
            (left, right)
        });
        cx.notify();
        pair
    });
    cx.run_until_parked();
    fake.take_calls();
    enter(&view, cx);
    let preview = cx.debug_bounds("library-media-preview").unwrap();
    for tab in [left, right] {
        let selector: &'static str = Box::leak(format!("split-pane-{}", tab.get()).into_boxed_str());
        let pane = cx.debug_bounds(selector).unwrap();
        assert!(pane.origin.x >= preview.origin.x && pane.right() <= preview.right());
        assert!(pane.origin.y >= preview.origin.y && pane.bottom() <= preview.bottom());
    }
    view.read_with(cx, |view, _| {
        let pages = view.pages.as_ref().unwrap().borrow();
        assert!(pages.is_visible(left) && pages.is_visible(right));
        assert_eq!(pages.surface_bounds().len(), 2);
    });
    unchanged(&fake.take_calls());
}


#[gpui::test]
fn native_library_page_shortcuts_reach_the_same_document_but_spaces_rejects_them(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let document = fake.0.borrow().focused_page.unwrap();
    enter(&view, cx);
    fake.send(cx, Event::Focused { view: document });
    let cases = [
        ("r", Call::Reload(raw(document))),
        ("[", Call::Back(raw(document))),
        ("]", Call::Forward(raw(document))),
        ("=", Call::Zoom(raw(document), 1)),
        ("-", Call::Zoom(raw(document), -1)),
        ("0", Call::Zoom(raw(document), 0)),
    ];
    fake.take_calls();
    for (key, expected) in &cases {
        fake.send(cx, Event::Shortcut { view: document, shortcut: Shortcut {
            cmd: true, key: (*key).into(), ..Default::default()
        }});
        assert_eq!(fake.take_calls(), vec![expected.clone()]);
        assert_preview(&fake, document, cx);
        assert_eq!(view.read_with(cx, |view, _| view.sidebar_navigation.controller.selected()), SidebarPage::Library);
    }
    click(cx, "library-nav-spaces");
    fake.take_calls();
    // Exercise the GPUI actions too: native provenance rejects hidden input,
    // while the shared page-command helper must independently exclude Spaces.
    cx.simulate_keystrokes("cmd-r cmd-[ cmd-] cmd-= cmd-- cmd-0");
    for (key, _) in &cases {
        fake.send(cx, Event::Shortcut { view: document, shortcut: Shortcut {
            cmd: true, key: (*key).into(), ..Default::default()
        }});
    }
    assert!(fake.take_calls().is_empty());
    assert_eq!(view.read_with(cx, |view, _| view.sidebar_navigation.library_section), LibrarySection::Spaces);
}

#[gpui::test]
fn accepted_native_preview_focus_supersedes_an_already_queued_popup_restore(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let document = fake.0.borrow().focused_page.unwrap();
    enter(&view, cx);
    click(cx, "library-media-search");
    cx.simulate_input("keep query");
    cx.simulate_keystrokes("cmd-t");
    cx.run_until_parked();
    fake.take_calls();
    // One GPUI event turn, deliberately without parking between dismissal and
    // native delivery: queue Restore, accept a newer visible native Focused,
    // then allow the stale restoration continuation to execute.
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.close_command_bar(cx);
            let pages = view.pages.as_ref().unwrap().clone();
            let focused = pages.borrow().capture_event(Event::Focused { view: document }).unwrap();
            view.apply_page_events(vec![focused], window, cx);
            assert!(pages.borrow().capture_resize_focus().has_native_focus());
            assert!(!view.focus_handle.is_focused(window));
        });
    });
    cx.run_until_parked();
    assert!(cx.update(|window, cx| !view.read(cx).focus_handle.is_focused(window)),
        "queued popup dismissal must not focus the GPUI root after newer native intent");
    view.read_with(cx, |view, cx| {
        assert!(view.command_bar.is_none());
        assert_eq!(view.sidebar_navigation.controller.selected(), SidebarPage::Library);
        assert!(view.pages.as_ref().unwrap().borrow().capture_resize_focus().has_native_focus());
        assert!(!view.sidebar_navigation.media_search_focus);
        assert!(view.sidebar_navigation.preview_ui_focus.is_none());
        assert_eq!(view.sidebar_navigation.media.as_ref().unwrap().read(cx).query(cx), "keep query");
    });
    assert_preview(&fake, document, cx);
    let calls = fake.take_calls();
    unchanged(&calls);
    assert!(!calls.iter().any(|call| matches!(call, Call::FocusUi | Call::FocusPage(_))), "{calls:?}");
}

#[gpui::test]
fn native_preview_command_bar_round_trip_restores_exact_native_input_without_a_click(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let document = fake.0.borrow().focused_page.unwrap();
    enter(&view, cx);
    fake.send(cx, Event::Focused { view: document });
    let identity = view.read_with(cx, |view, _| view.pages.as_ref().unwrap().borrow().identity_for(document).unwrap());
    cx.simulate_keystrokes("cmd-t");
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert!(view.sidebar_navigation.preview_native_focus.is_some());
        assert!(!view.pages.as_ref().unwrap().borrow().capture_resize_focus().has_native_focus());
    });
    fake.take_calls();
    bar(&view, cx).simulate_keystrokes("escape");
    cx.run_until_parked();
    let calls = fake.take_calls();
    assert!(calls.contains(&Call::FocusPage(raw(document))), "dismissal must restore the native responder: {calls:?}");
    assert!(!calls.iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..) | Call::MoveHost(..))));
    view.read_with(cx, |view, _| {
        let pages = view.pages.as_ref().unwrap().borrow();
        assert!(pages.capture_resize_focus().has_native_focus());
        assert_eq!(pages.identity_for(document), Some(identity));
        assert_eq!(view.sidebar_navigation.controller.selected(), SidebarPage::Library);
        assert!(view.sidebar_navigation.preview_native_focus.is_none());
    });
    assert_eq!(fake.0.borrow().focused_page, Some(document));
    // No native Focused event or click between dismissal and the next input.
    fake.send(cx, Event::Shortcut { view: document, shortcut: Shortcut {
        cmd: true, key: "r".into(), ..Default::default()
    }});
    assert_eq!(fake.take_calls(), vec![Call::Reload(raw(document))]);
    assert_preview(&fake, document, cx);
}

#[gpui::test]
fn native_popup_snapshot_rejects_an_externally_retired_profile_generation(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let document = fake.0.borrow().focused_page.unwrap();
    enter(&view, cx);
    fake.send(cx, Event::Focused { view: document });
    let old = view.read_with(cx, |view, _| view.pages.as_ref().unwrap().borrow().identity_for(document).unwrap());
    let space = view.read_with(cx, |view, _| view.browser.space().id);
    let (other, _) = another_profile_window(&view, cx);
    cx.simulate_keystrokes("cmd-t");
    cx.run_until_parked();
    fake.take_calls();
    // Mutate canonical ownership after Restore is queued but before it runs.
    cx.cx.update(|cx| {
        view.update(cx, |view, cx| view.close_command_bar(cx));
        other.update(cx, |other, _| other.edit_profile(|browser| {
            let profile = browser.create_user_profile("Native popup replacement").unwrap();
            browser.assign_space_user_profile(space, profile).unwrap();
        }));
    });
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert!(!view.pages.as_ref().unwrap().borrow().accepts_identity(old));
        assert!(!view.pages.as_ref().unwrap().borrow().capture_resize_focus().has_native_focus());
        assert!(view.sidebar_navigation.preview_native_focus.is_none());
        assert_eq!(view.sidebar_navigation.controller.selected(), SidebarPage::Library);
    });
    assert!(!fake.take_calls().contains(&Call::FocusPage(raw(document))));
}

#[gpui::test]
fn native_popup_snapshot_restores_only_the_original_live_inspector_frontend_generation(cx: &mut TestAppContext) {
    for replace_frontend in [false, true] {
        let (view, cx, fake) = open_pages(cx);
        accept_recording_inspector(&view, cx, &fake);
        let document = fake.0.borrow().focused_page.unwrap();
        enter(&view, cx);
        fake.send(cx, Event::Focused { view: document });
        // Establish an applicable live frontend after the Library viewport is
        // mounted; a route change can legitimately retire a previous dock.
        view.update(cx, |view, cx| {
            let tab = view.browser.active_tab().unwrap().id;
            view.pages.as_ref().unwrap().borrow_mut().run_devtools(tab, crate::devtools::Command::Toggle).unwrap();
            cx.notify();
        });
        cx.run_until_parked();
        let inspector = inspector_status(&view, cx).unwrap();
        inspector_event(&fake, cx, engine::InspectorEvent::Focused { inspector: inspector.id });
        cx.simulate_keystrokes("cmd-t");
        cx.run_until_parked();
        assert!(view.read_with(cx, |view, _| view.sidebar_navigation.preview_native_focus.is_some()));
        if replace_frontend {
            fake.0.borrow_mut().inspector.as_mut().unwrap().generation += 1;
        }
        fake.take_calls();
        bar(&view, cx).simulate_keystrokes("escape");
        cx.run_until_parked();
        let calls = fake.take_calls();
        assert_eq!(calls.contains(&Call::InspectorFocus(inspector.id)), !replace_frontend, "{calls:?}");
        assert!(!calls.iter().any(|call| matches!(call, Call::FocusPage(_))), "a stale inspector must not fall back to its page: {calls:?}");
        assert_eq!(view.read_with(cx, |view, _| view.sidebar_navigation.controller.selected()), SidebarPage::Library);
    }
}

#[gpui::test]
fn history_popup_cache_rejects_external_same_tab_url_and_owner_changes_before_repaint(cx: &mut TestAppContext) {
    for change_owner in [false, true] {
        let (view, cx, fake) = open_pages(cx);
        cx.simulate_keystrokes("cmd-y");
        cx.run_until_parked();
        enter(&view, cx);
        click(cx, "library-media-search");
        cx.simulate_input("media stays");
        let input = view.read_with(cx, |view, _| view.history_page.input.clone());
        let field = cx.debug_bounds("history-search").unwrap();
        let position = gpui::point(field.origin.x + px(24.), field.center().y);
        cx.simulate_mouse_move(position, None, Modifiers::none());
        cx.run_until_parked();
        cx.simulate_click(position, Modifiers::none());
        cx.run_until_parked();
        cx.simulate_input("history stays");
        let (tab, space) = view.read_with(cx, |view, _| (view.browser.active_tab().unwrap().id, view.browser.space().id));
        let (other, _) = another_profile_window(&view, cx);
        cx.simulate_keystrokes("cmd-t");
        cx.run_until_parked();
        assert!(view.read_with(cx, |view, _| view.sidebar_navigation.preview_ui_focus.is_some()));
        // Keep exactly the same selected TabId. Queue dismissal first and
        // replace URL or owner through another window before any new frame.
        cx.cx.update(|cx| {
            view.update(cx, |view, cx| view.close_command_bar(cx));
            other.update(cx, |other, _| other.edit_profile(|browser| {
                if change_owner {
                    let profile = browser.create_user_profile("History popup owner").unwrap();
                    browser.assign_space_user_profile(space, profile).unwrap();
                } else {
                    browser.set_url(tab, "https://external.example/replaced-history");
                }
            }));
        });
        cx.run_until_parked();
        assert!(cx.update(|window, cx| !input.read(cx).focus_handle(cx).is_focused(window)),
            "a same-TabId external edit must not restore the stale History handle (owner={change_owner})");
        assert!(view.read_with(cx, |view, _| view.sidebar_navigation.preview_ui_focus.is_none()));
        cx.simulate_input(" must not reach old History");
        cx.run_until_parked();
        view.read_with(cx, |view, cx| {
            assert_eq!(view.browser.active_tab().unwrap().id, tab);
            assert_eq!(view.history_page.input.read(cx).text(), "history stays");
            assert_eq!(view.sidebar_navigation.media.as_ref().unwrap().read(cx).query(cx), "media stays");
            assert_eq!(view.sidebar_navigation.controller.selected(), SidebarPage::Library);
        });
        // URL replacement may create its real native page; it must never
        // resurrect a fake CEF History document or navigate one to browser://.
        assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(_, url) | Call::Navigate(_, url) if url.starts_with("browser://"))));
    }
}

#[gpui::test]
fn find_and_other_popup_dismissal_restore_native_preview_input_without_another_click(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let document = fake.0.borrow().focused_page.unwrap();
    enter(&view, cx);
    fake.send(cx, Event::Focused { view: document });
    let identity = view.read_with(cx, |view, _| view.pages.as_ref().unwrap().borrow().identity_for(document).unwrap());
    cx.simulate_keystrokes("cmd-f");
    cx.run_until_parked();
    assert!(view.read_with(cx, |view, _| view.sidebar_navigation.preview_native_focus.is_some()));
    let mut find = find_overlay(&view, cx);
    find.simulate_input("needle");
    cx.run_until_parked();
    assert!(fake.take_calls().iter().any(|call| matches!(call, Call::Find(id, query, _, _) if *id == raw(document) && query == "needle")));
    find.simulate_keystrokes("escape");
    cx.run_until_parked();
    let calls = fake.take_calls();
    assert!(calls.contains(&Call::StopFinding(raw(document))));
    assert!(calls.contains(&Call::FocusPage(raw(document))), "Find dismissal must restore the exact ViewId: {calls:?}");
    assert!(!calls.iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..) | Call::MoveHost(..))));
    view.read_with(cx, |view, _| {
        assert!(view.find_bar.is_none());
        let pages = view.pages.as_ref().unwrap().borrow();
        assert_eq!(pages.identity_for(document), Some(identity));
        assert!(pages.capture_resize_focus().has_native_focus());
        assert_eq!(view.sidebar_navigation.controller.selected(), SidebarPage::Library);
    });
    assert_eq!(fake.0.borrow().focused_page, Some(document));
    // No native Focused callback and no click after Escape: input routes to the
    // same document immediately, rather than requiring a responder repair click.
    fake.send(cx, Event::Shortcut { view: document, shortcut: Shortcut {
        cmd: true, key: "r".into(), ..Default::default()
    }});
    assert_eq!(fake.take_calls(), vec![Call::Reload(raw(document))]);
    assert_preview(&fake, document, cx);

    // Audit a second existing popup owner which clears Pages intent without a
    // per-entry hook. The shared native-intent/restore seam must cover it too,
    // including after the prior restore consumed its pending popup snapshot.
    dispatch_menu(cx, "File", "New Profile");
    cx.run_until_parked();
    let overlay = view.read_with(cx, |view, _| view.user_profile_dialog.as_ref().unwrap().overlay.unwrap());
    let mut profile = VisualTestContext::from_window(overlay.into(), cx);
    fake.take_calls();
    profile.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(fake.take_calls().contains(&Call::FocusPage(raw(document))));
    view.read_with(cx, |view, _| {
        assert!(view.user_profile_dialog.is_none());
        assert!(view.pages.as_ref().unwrap().borrow().capture_resize_focus().has_native_focus());
        assert_eq!(view.pages.as_ref().unwrap().borrow().identity_for(document), Some(identity));
        assert_eq!(view.sidebar_navigation.controller.selected(), SidebarPage::Library);
    });
    fake.send(cx, Event::Shortcut { view: document, shortcut: Shortcut {
        cmd: true, key: "[".into(), ..Default::default()
    }});
    assert_eq!(fake.take_calls(), vec![Call::Back(raw(document))]);
    assert_preview(&fake, document, cx);
}

#[gpui::test]
fn library_from_an_empty_space_never_leaks_the_other_spaces_live_page(cx: &mut TestAppContext) {
    cx.update(init);
    let fake = FakeEngine::default();
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut browser = demo_browser();
        browser.add_space("Empty reference", 250.);
        let view = BrowserWindow::new(browser, Some(Box::new(fake.clone())), window, cx);
        view.focus(window);
        view
    });
    cx.run_until_parked();
    let document = fake.0.borrow().focused_page.unwrap();
    fake.take_calls();
    cx.simulate_keystrokes("ctrl-4");
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    assert!(view.read_with(cx, |view, _| view.browser.active_tab().is_none()));
    assert_eq!(view.read_with(cx, |view, _| view.browser.space().name.clone()), "Empty reference");
    enter(&view, cx);
    for section in ["library-nav-downloads", "library-nav-media"] {
        click(cx, section);
        view.read_with(cx, |view, _| assert!(view.pages.as_ref().unwrap().borrow().surface_bounds().is_empty()));
    }
    let calls = fake.take_calls();
    unchanged(&calls);
    assert!(!calls.iter().any(|call| matches!(call, Call::Visible(_, true) | Call::FocusPage(_))));
    click(cx, "library-nav-spaces");
    click(cx, "library-space-select-0");
    let calls = fake.take_calls();
    unchanged(&calls);
    assert!(calls.contains(&Call::Visible(raw(document), true)));
}

#[gpui::test]
fn library_slides_the_same_page_without_hiding_or_reflow_and_returns_it(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    cx.simulate_resize(gpui::size(px(1000.), px(760.)));
    cx.run_until_parked();
    let document = fake.0.borrow().focused_page.unwrap();
    let original = fake.0.borrow().bounds[&raw(document)];
    let session = view.read_with(cx, |view, _| view.session_json());
    fake.take_calls();
    enter(&view, cx);
    let mut previous = original.x;
    for millis in [32, 48, 80, 140] {
        cx.executor().advance_clock(Duration::from_millis(millis));
        cx.run_until_parked();
        let frame = fake.0.borrow().bounds[&raw(document)];
        assert_eq!((frame.width, frame.height), (original.width, original.height));
        assert!(frame.x >= previous && frame.x <= 417.);
        previous = frame.x;
    }
    assert_eq!(previous, 417.);
    assert_preview(&fake, document, cx);
    cx.simulate_keystrokes("ctrl-1");
    cx.run_until_parked();
    for millis in [32, 80, 120, 140] {
        cx.executor().advance_clock(Duration::from_millis(millis));
        cx.run_until_parked();
        let frame = fake.0.borrow().bounds[&raw(document)];
        assert_eq!((frame.width, frame.height), (original.width, original.height));
        assert!(frame.x <= previous && frame.x >= original.x);
        previous = frame.x;
    }
    assert_eq!(fake.0.borrow().bounds[&raw(document)], original);
    let calls = fake.take_calls();
    unchanged(&calls);
    assert!(!calls.contains(&Call::Visible(raw(document), false)), "{calls:?}");
    assert_eq!(view.read_with(cx, |view, _| view.session_json()), session);
}

#[gpui::test]
fn library_keeps_the_hidden_sidebar_viewport_width_and_tracks_window_resize(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    cx.simulate_resize(gpui::size(px(1000.), px(760.)));
    cx.simulate_keystrokes("cmd-s");
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    let document = fake.0.borrow().focused_page.unwrap();
    let original = fake.0.borrow().bounds[&raw(document)];
    assert_eq!(original.width, 980.);
    enter(&view, cx);
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    assert_eq!(fake.0.borrow().bounds[&raw(document)].width, original.width);
    cx.simulate_resize(gpui::size(px(1100.), px(760.)));
    cx.run_until_parked();
    let frame = fake.0.borrow().bounds[&raw(document)];
    assert_eq!((frame.x, frame.width, frame.height), (417., 1080., original.height));
}

#[gpui::test]
fn opening_a_new_page_from_library_cancels_the_library_offset(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let original = fake.0.borrow().bounds[&raw(fake.0.borrow().focused_page.unwrap())];
    enter(&view, cx);
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("https://new.example/after-library");
    bar(&view, cx).simulate_keystrokes("enter");
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    assert!(view.read_with(cx, |view, _| view.page_content_visible()));
    let document = fake.0.borrow().focused_page.unwrap();
    assert_eq!(fake.0.borrow().bounds[&raw(document)], original);
}
