use super::*;
use crate::library::media::{MediaAction, MediaView};
use browser_core::media::{FolderGrant, MediaFolder, MediaItem, MediaKind, MediaSource};
use gpui::px;
fn open_media_pages(
    cx: &mut TestAppContext,
) -> (Entity<BrowserWindow>, &mut VisualTestContext, FakeEngine) {
    let (view, visual, fake) = open_pages(cx);
    view.update(visual, |v, _| {
        let profile = v.browser.active_user_profile_id();
        v.profile.handle.edit_settings(|b| {
            b.user_profile_mut(profile).unwrap().settings.media = Default::default()
        });
        v.refresh_profile_projection();
    });
    visual.run_until_parked();
    (view, visual, fake)
}
fn media(view: &Entity<BrowserWindow>, cx: &VisualTestContext) -> Entity<MediaView> {
    view.read_with(cx, |v, _| v.sidebar_navigation.media.clone().unwrap())
}
fn answer(cx: &mut VisualTestContext, folder: MediaFolder, path: Option<std::path::PathBuf>) {
    cx.cx.update(|app| {
        let pending = &mut app.global_mut::<crate::media_access::TestPicker>().pending;
        assert_eq!(pending.len(), 1);
        let (actual, reply) = pending.remove(0);
        assert_eq!(actual, folder);
        reply.send(Ok(path)).unwrap();
    });
    cx.run_until_parked();
}
fn fixture(dir: &std::path::Path, name: &str, color: [u8; 4]) -> MediaItem {
    let path = dir.join(name);
    image::RgbaImage::from_pixel(120, 80, image::Rgba(color))
        .save(&path)
        .unwrap();
    MediaItem {
        stamp: echo_media::stamp(&path).unwrap(),
        kind: MediaKind::Image,
        source: MediaSource::Folder,
        name: name.into(),
    }
}
#[gpui::test]
fn media_permissions_prompt_only_on_entry_persist_and_revoke_without_scanning_unknown_folders(
    cx: &mut TestAppContext,
) {
    let (view, visual, _) = open_media_pages(cx);
    assert!(!visual
        .cx
        .update(|app| app.has_global::<crate::media_access::TestPicker>()));
    view.update(visual, |v, cx| v.open_library_downloads(cx));
    visual.run_until_parked();
    assert!(!visual
        .cx
        .update(|app| app.has_global::<crate::media_access::TestPicker>()));
    click(visual, "library-nav-media");
    visual.executor().advance_clock(Duration::from_millis(400));
    visual.run_until_parked();
    let entity = media(&view, visual);
    assert!(!entity.read_with(visual, |v, _| v.test_media_scanning()));
    answer(visual, MediaFolder::Documents, None);
    answer(visual, MediaFolder::Desktop, None);
    let dir = tempfile::tempdir().unwrap();
    answer(visual, MediaFolder::Downloads, Some(dir.path().into()));
    let profile = view.read_with(visual, |v, _| v.browser.active_user_profile_id());
    assert_eq!(
        view.read_with(visual, |v, _| v
            .profile
            .handle
            .media_access(profile)
            .unwrap()
            .paths()),
        vec![dir.path().to_path_buf()]
    );
    assert!(entity.read_with(visual, |v, _| v.test_media_scanning()));
    visual.simulate_keystrokes("cmd-,");
    let (_, mut settings) = settings_window(visual);
    click(&mut settings, "settings-tab-Advanced");
    click(&mut settings, "settings-media-Downloads");
    visual.run_until_parked();
    assert_eq!(
        view.read_with(visual, |v, _| v
            .profile
            .handle
            .media_access(profile)
            .unwrap()
            .grant(MediaFolder::Downloads)),
        FolderGrant::Denied
    );
    assert!(!entity.read_with(visual, |v, _| v.test_media_scanning()));
}
#[gpui::test]
fn media_late_native_approval_cannot_restore_a_revoked_profile_grant(cx: &mut TestAppContext) {
    let (view, visual, _) = open_media_pages(cx);
    view.update(visual, |v, cx| v.open_library_media(cx));
    visual.run_until_parked();
    let profile = view.read_with(visual, |v, _| v.browser.active_user_profile_id());
    view.update(visual, |v, _| {
        v.profile.handle.edit_settings(|b| {
            b.user_profile_mut(profile)
                .unwrap()
                .settings
                .media
                .revoke(MediaFolder::Documents)
        })
    });
    let dir = tempfile::tempdir().unwrap();
    answer(visual, MediaFolder::Documents, Some(dir.path().into()));
    assert_eq!(
        view.read_with(visual, |v, _| v
            .profile
            .handle
            .media_access(profile)
            .unwrap()
            .grant(MediaFolder::Documents)),
        FolderGrant::Denied
    );
    answer(visual, MediaFolder::Desktop, None);
    answer(visual, MediaFolder::Downloads, None);
}
#[gpui::test]
fn media_grid_selection_search_and_file_actions_keep_the_live_document_and_exact_file_target(
    cx: &mut TestAppContext,
) {
    let (view, visual, fake) = open_media_pages(cx);
    visual.simulate_resize(gpui::size(gpui::px(1000.), gpui::px(760.)));
    let before = view.read_with(visual, |v, _| v.browser.space().active_tab);
    view.update(visual, |v, cx| v.open_library_media(cx));
    visual.run_until_parked();
    visual.executor().advance_clock(Duration::from_millis(400));
    visual.run_until_parked();
    let entity = media(&view, visual);
    let dir = tempfile::tempdir().unwrap();
    let red = fixture(dir.path(), "echo-red.png", [180, 60, 70, 255]);
    let blue = fixture(dir.path(), "echo-blue.png", [60, 100, 180, 255]);
    entity.update(visual, |v, cx| {
        v.test_media_items(vec![red.clone(), blue.clone()], cx)
    });
    visual.run_until_parked();
    fake.take_calls();
    click(visual, "media-item-echo-red.png");
    assert_eq!(
        view.read_with(visual, |v, _| v.native_page_viewport()),
        crate::pages::PageViewport::Hidden
    );
    let preview = visual.debug_bounds("library-media-preview").unwrap();
    assert_eq!(preview.origin, gpui::point(gpui::px(417.), gpui::px(10.)));
    assert_eq!(preview.size, gpui::size(gpui::px(573.), gpui::px(740.)));
    visual.simulate_keystrokes("cmd-c enter");
    visual.run_until_parked();
    assert_eq!(
        entity.read_with(visual, |v, _| v.test_media_effects()),
        vec![
            (MediaAction::Copy, red.stamp.clone()),
            (MediaAction::Open, red.stamp.clone())
        ]
    );
    click(visual, "media-action-Reveal");
    assert_eq!(
        entity
            .read_with(visual, |v, _| v.test_media_effects())
            .last(),
        Some(&(MediaAction::Reveal, red.stamp.clone()))
    );
    click(visual, "library-media-search");
    visual.simulate_input("blue");
    visual.run_until_parked();
    assert_eq!(
        entity.read_with(visual, |v, _| v.test_media_names()),
        vec!["echo-blue.png"]
    );
    assert!(!entity.read_with(visual, |v, _| v.has_media_selection()));
    click(visual, "media-item-echo-blue.png");
    click(visual, "media-preview-close");
    assert_eq!(
        view.read_with(visual, |v, _| v.native_page_viewport()),
        crate::pages::PageViewport::LibraryMedia
    );
    assert_eq!(
        view.read_with(visual, |v, _| v.browser.space().active_tab),
        before
    );
    assert!(!fake
        .take_calls()
        .iter()
        .any(|c| matches!(c, Call::Navigate(..) | Call::Close(..) | Call::Reload(..))));
}
#[gpui::test]
fn media_trash_and_keyboard_undo_use_a_fixture_trash_and_never_replace_an_existing_file(
    cx: &mut TestAppContext,
) {
    let (view, visual, _) = open_media_pages(cx);
    view.update(visual, |v, cx| v.open_library_media(cx));
    visual.run_until_parked();
    visual.executor().advance_clock(Duration::from_millis(400));
    visual.run_until_parked();
    let entity = media(&view, visual);
    let dir = tempfile::tempdir().unwrap();
    let item = fixture(dir.path(), "echo-trash.png", [80, 120, 150, 255]);
    let bytes = std::fs::read(&item.stamp.path).unwrap();
    entity.update(visual, |v, cx| v.test_media_items(vec![item.clone()], cx));
    visual.run_until_parked();
    click(visual, "media-item-echo-trash.png");
    click(visual, "media-action-Trash");
    visual.run_until_parked();
    assert!(!item.stamp.path.exists());
    std::fs::write(&item.stamp.path, b"replacement fixture").unwrap();
    visual.simulate_keystrokes("cmd-z");
    visual.run_until_parked();
    assert_eq!(
        std::fs::read(&item.stamp.path).unwrap(),
        b"replacement fixture"
    );
    std::fs::remove_file(&item.stamp.path).unwrap();
    // Undo may restore the user's own receipt after revocation, but must not
    // repopulate Media with a folder whose grant was withdrawn.
    view.update(visual, |v, _| {
        let profile = v.browser.active_user_profile_id();
        v.profile.handle.edit_settings(|b| {
            b.user_profile_mut(profile)
                .unwrap()
                .settings
                .media
                .revoke(MediaFolder::Downloads)
        });
        v.refresh_profile_projection();
    });
    visual.run_until_parked();
    visual.simulate_keystrokes("cmd-z");
    visual.run_until_parked();
    assert_eq!(std::fs::read(&item.stamp.path).unwrap(), bytes);
    assert!(entity
        .read_with(visual, |v, _| v.test_media_names())
        .is_empty());
}

#[gpui::test]
fn media_context_actions_capture_the_clicked_file_and_outside_click_dismisses(
    cx: &mut TestAppContext,
) {
    let (view, visual, _) = open_pages(cx);
    visual.simulate_resize(gpui::size(px(1000.), px(760.)));
    view.update(visual, |v, cx| v.open_library_media(cx));
    visual.run_until_parked();
    visual.executor().advance_clock(Duration::from_millis(400));
    visual.run_until_parked();
    let entity = media(&view, visual);
    let dir = tempfile::tempdir().unwrap();
    let red = fixture(dir.path(), "menu-red.png", [180, 60, 70, 255]);
    let blue = fixture(dir.path(), "menu-blue.png", [60, 100, 180, 255]);
    entity.update(visual, |v, cx| {
        v.test_media_items(vec![red.clone(), blue.clone()], cx)
    });
    visual.run_until_parked();
    for name in ["media-item-menu-red.png", "media-item-menu-blue.png"] {
        let at = visual.debug_bounds(name).unwrap().center();
        visual.simulate_mouse_move(at, None, Modifiers::none());
        visual.simulate_mouse_down(at, gpui::MouseButton::Right, Modifiers::none());
        visual.simulate_mouse_up(at, gpui::MouseButton::Right, Modifiers::none());
        visual.run_until_parked();
        assert!(visual.debug_bounds("media-context-menu").is_some());
        if name.ends_with("red.png") {
            let old_action = visual.debug_bounds("media-menu-Copy").unwrap().center();
            let outside = gpui::point(px(900.), px(500.));
            visual.simulate_mouse_move(outside, None, Modifiers::none());
            visual.simulate_click(outside, Modifiers::none());
            visual.executor().advance_clock(Duration::from_millis(20));
            visual.run_until_parked();
            // GPUI retains debug selector bounds after removal. Verify both
            // dismissal and that the old action's position no longer acts.
            assert!(!entity.read_with(visual, |v, _| v.test_media_menu_open()));
            visual.simulate_mouse_move(old_action, None, Modifiers::none());
            visual.simulate_click(old_action, Modifiers::none());
            visual.run_until_parked();
            assert!(entity
                .read_with(visual, |v, _| v.test_media_effects())
                .is_empty());
        } else {
            click(visual, "media-menu-Copy");
        }
    }
    assert_eq!(
        entity.read_with(visual, |v, _| v.test_media_effects()),
        vec![(MediaAction::Copy, blue.stamp)]
    );
}
