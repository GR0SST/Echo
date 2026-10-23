//! Actual BrowserWindow routes and responder lifecycle, with canonical model
//! fixtures. These fixtures do not claim a native transfer or destination policy.
use super::*;
use crate::{library::LibrarySection, sidebar_pages::SidebarPage};
use browser_core::downloads::{DownloadCompletion, DownloadTime};

fn seed(view: &Entity<BrowserWindow>, cx: &mut VisualTestContext, name: &str) {
    let normal = view.read_with(cx, |view, _| view.profile.handle.clone());
    normal
        .record_completed_download(
            normal.default_user_profile_id(),
            DownloadCompletion {
                final_path: std::env::temp_dir().join(name),
                source_url: "https://fixture.test/file".into(),
                original_url: "https://fixture.test/file".into(),
                mime_type: "application/octet-stream".into(),
                finished_at: DownloadTime::WindowsEpochMicros(
                    11_644_473_600_000_000 + chrono::Utc::now().timestamp_micros(),
                ),
            },
        )
        .unwrap();
    cx.run_until_parked();
}
fn assert_downloads(view: &Entity<BrowserWindow>, cx: &mut VisualTestContext) {
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.sidebar_navigation.controller.selected(),
            SidebarPage::Library
        );
        assert_eq!(
            view.sidebar_navigation.library_section,
            LibrarySection::Downloads
        );
        assert!(view.command_bar.is_none());
        assert!(view.native_page_content_visible());
    });
    assert!(cx.debug_bounds("downloads-search").is_some());
    assert!(cx.debug_bounds("downloads-preview").is_some());
}

#[gpui::test]
fn downloads_shortcut_menu_command_and_rail_mount_canonical_rows_without_document_changes(
    cx: &mut TestAppContext,
) {
    let (view, cx, fake) = open_pages(cx);
    seed(&view, cx, "route-fixture.bin");
    let before = view.read_with(cx, |view, _| view.session_json());
    let native = fake.0.borrow().focused_page.unwrap();
    for route in 0..4 {
        fake.take_calls();
        match route {
            0 => cx.simulate_keystrokes("cmd-shift-j"),
            1 => dispatch_menu(cx, "Window", "View Downloads…"),
            2 => run_named_command(&view, cx, "view downloads"),
            _ => {
                cx.simulate_keystrokes("cmd-shift-l");
                cx.run_until_parked();
                click(cx, "library-nav-downloads");
            }
        }
        cx.run_until_parked();
        assert_downloads(&view, cx);
        assert!(cx.debug_bounds("downloads-row-0-0").is_some());
        assert!(!fake.take_calls().iter().any(|call| matches!(
            call,
            Call::Create(..) | Call::Close(..) | Call::Navigate(..)
        )));
        assert_eq!(view.read_with(cx, |view, _| view.session_json()), before);
        assert!(view.read_with(cx, |view, _| view
            .pages
            .as_ref()
            .unwrap()
            .borrow()
            .is_visible(view.browser.active_tab().unwrap().id)));
        click(cx, "library-nav-spaces");
        click(cx, "library-space-select-0");
        assert_eq!(fake.0.borrow().focused_page, Some(native));
    }
}

#[gpui::test]
fn downloads_query_survives_sections_and_command_dismissal_restores_exact_filter_control(
    cx: &mut TestAppContext,
) {
    let (view, cx, _) = open_pages(cx);
    seed(&view, cx, "keep-query.bin");
    cx.simulate_keystrokes("cmd-shift-j");
    cx.run_until_parked();
    click(cx, "downloads-search");
    cx.simulate_input("keep");
    cx.run_until_parked();
    click(cx, "downloads-filter");
    let focus = cx.update(|window, cx| window.focused(cx).unwrap());
    cx.simulate_keystrokes("cmd-t");
    cx.run_until_parked();
    bar(&view, cx).simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(cx.update(|window, _| focus.is_focused(window)));
    click(cx, "downloads-filter-done");
    click(cx, "library-nav-media");
    click(cx, "library-nav-downloads");
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.sidebar_navigation
                .downloads
                .host
                .as_ref()
                .unwrap()
                .query(),
            "keep"
        )
    });
    click(cx, "downloads-search");
    cx.simulate_input(" absent");
    cx.run_until_parked();
    assert!(cx.debug_bounds("downloads-search-empty").is_some());
    crate::sidebar_title::take_painted_titles();
    cx.update(|window, cx| {
        window.refresh();
        let _ = window.draw(cx);
    });
    assert!(!crate::sidebar_title::take_painted_titles()
        .iter()
        .any(|paint| paint.original == "keep-query.bin"));
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.sidebar_navigation
                .downloads
                .host
                .as_ref()
                .unwrap()
                .query(),
            "keep absent"
        )
    });
}

#[gpui::test]
fn downloads_native_focus_supersedes_control_cache_and_inventory_updates_while_hidden(
    cx: &mut TestAppContext,
) {
    let (view, cx, fake) = open_pages(cx);
    let native = fake.0.borrow().focused_page.unwrap();
    cx.simulate_keystrokes("cmd-shift-j");
    cx.run_until_parked();
    click(cx, "downloads-search");
    fake.send(cx, engine::Event::Focused { view: native });
    cx.simulate_keystrokes("cmd-t");
    cx.run_until_parked();
    assert!(view.read_with(cx, |view, _| view
        .sidebar_navigation
        .preview_native_focus
        .is_some()));
    // Activation can report GPUI's retained control handle while the actual
    // responder belongs to the popup/native preview. It is not a new click.
    let presenter = view.read_with(cx, |view, _| {
        view.sidebar_navigation
            .downloads
            .host
            .as_ref()
            .unwrap()
            .view()
    });
    presenter.update(cx, |_, cx| {
        cx.emit(crate::downloads_view::DownloadsControlFocused::Observed)
    });
    cx.run_until_parked();
    assert!(view.read_with(cx, |view, _| view
        .sidebar_navigation
        .preview_native_focus
        .is_some()));
    fake.take_calls();
    bar(&view, cx).simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(fake.take_calls().contains(&Call::FocusPage(raw(native))));
    assert!(view.read_with(cx, |view, _| view
        .pages
        .as_ref()
        .unwrap()
        .borrow()
        .capture_resize_focus()
        .has_native_focus()));
    assert_eq!(fake.0.borrow().focused_page, Some(native));
    click(cx, "library-nav-media");
    seed(&view, cx, "arrived-while-hidden.bin");
    click(cx, "library-nav-downloads");
    assert!(cx.debug_bounds("downloads-row-0-0").is_some());
    let shortcut = crate::window::page_shortcuts()
        .into_iter()
        .find(|shortcut| {
            shortcut.cmd && shortcut.shift && !shortcut.alt && !shortcut.ctrl && shortcut.key == "j"
        })
        .unwrap();
    fake.send(cx, fake.shortcut(shortcut));
    assert_downloads(&view, cx);
}

#[gpui::test]
fn downloads_route_retirement_invalidates_old_tickets_even_without_an_intervening_frame(
    cx: &mut TestAppContext,
) {
    let (view, cx, _) = open_pages(cx);
    cx.simulate_keystrokes("cmd-shift-j");
    cx.run_until_parked();
    let (ticket, sender) = view.read_with(cx, |view, _| {
        let host = view.sidebar_navigation.downloads.host.as_ref().unwrap();
        (host.owner(), host.view())
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.handle_library_intent(
                crate::library::LibraryIntent::Navigate(LibrarySection::Media),
                window,
                cx,
            );
            view.handle_library_intent(
                crate::library::LibraryIntent::Navigate(LibrarySection::Downloads),
                window,
                cx,
            );
            view.sync_library_downloads(window, cx);
            let host = view.sidebar_navigation.downloads.host.as_mut().unwrap();
            assert_eq!(
                host.view(),
                sender,
                "the same window-local presenter retains its query"
            );
            assert_ne!(host.owner(), ticket);
            assert!(!host.handle_intent(
                ticket,
                &sender,
                &view.profile.handle,
                &crate::downloads_view::DownloadsIntent::SearchChanged("stale".into()),
                crate::downloads_view::DownloadsColors {
                    column: gpui::rgb(0xffffff).into(),
                    preview: gpui::rgb(0xffffff).into(),
                    text: gpui::rgb(0).into(),
                    muted: gpui::rgb(0).into(),
                    control: gpui::rgb(0).into(),
                    selected: gpui::rgb(0).into(),
                    hover: gpui::rgb(0).into(),
                },
                window,
                cx
            ));
            assert_eq!(host.query(), "");
        })
    });
    cx.run_until_parked();
    assert_downloads(&view, cx);
}
