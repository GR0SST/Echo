#[path = "capture_sound_tests.rs"]
mod capture_sound_tests;

use super::*;
use std::io::Cursor;
fn current(view: &Entity<BrowserWindow>, cx: &mut VisualTestContext) -> VisualTestContext {
    let handle = view.read_with(cx, |view, _| {
        view.capture
            .as_ref()
            .expect("Capture opened")
            .overlay
            .expect("Capture is over native pages")
    });
    let mut overlay = VisualTestContext::from_window(handle.into(), &cx.cx);
    overlay.run_until_parked();
    overlay
}
fn select(overlay: &mut VisualTestContext, frame: Bounds) {
    let start = gpui::point(
        gpui::px(frame.x as f32 + 20.),
        gpui::px(frame.y as f32 + 30.),
    );
    let end = start + gpui::point(gpui::px(100.), gpui::px(60.));
    overlay.simulate_mouse_move(start, None, Modifiers::none());
    overlay.simulate_mouse_down(start, gpui::MouseButton::Left, Modifiers::none());
    overlay.simulate_mouse_move(end, gpui::MouseButton::Left, Modifiers::none());
    overlay.run_until_parked();
    overlay.simulate_mouse_up(end, gpui::MouseButton::Left, Modifiers::none());
    overlay.run_until_parked();
}
fn image(frame: Bounds) -> Vec<u8> {
    let buffer = image::RgbaImage::from_pixel(
        (frame.width * 2.) as u32,
        (frame.height * 2.) as u32,
        image::Rgba([10, 20, 30, 255]),
    );
    let mut png = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(buffer)
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    png.into_inner()
}
fn complete(
    view: &Entity<BrowserWindow>,
    cx: &mut VisualTestContext,
    fake: &FakeEngine,
    request: u64,
    png: Vec<u8>,
) {
    fake.0.borrow_mut().snapshots.push(engine::PageSnapshot {
        view: page(1),
        request,
        png: Some(png),
    });
    view.update(cx, |view, _| {
        view.pages.as_ref().unwrap().borrow_mut().take_events();
    });
    cx.executor().advance_clock(Duration::from_millis(40));
    cx.run_until_parked();
}
#[gpui::test]
fn capture_drag_crops_retina_pixels_copies_and_saves_only_to_its_canonical_profile(
    cx: &mut TestAppContext,
) {
    let (view, cx, fake) = open_pages(cx);
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("session.json");
    view.update(cx, |view, cx| view.profile.handle.save_session_to(path, cx));
    fake.0.borrow_mut().capture_supported = true;
    fake.take_calls();
    cx.simulate_keystrokes("cmd-shift-2");
    cx.run_until_parked();
    let mut overlay = current(&view, cx);
    let frame = fake.0.borrow().bounds[&1];
    select(&mut overlay, frame);
    let request = fake
        .take_calls()
        .into_iter()
        .find_map(|call| {
            if let Call::Snapshot(1, request) = call {
                Some(request)
            } else {
                None
            }
        })
        .expect("captures existing focused page");
    complete(&view, cx, &fake, request, image(frame));
    overlay.run_until_parked();
    click(&mut overlay, "capture-Copy");
    let bytes = cx.update(|_, cx| cx.read_from_clipboard()).unwrap();
    let png = bytes
        .entries()
        .iter()
        .find_map(|entry| {
            if let gpui::ClipboardEntry::Image(image) = entry {
                Some(image.bytes())
            } else {
                None
            }
        })
        .unwrap();
    let cropped = image::load_from_memory(&png).unwrap();
    assert_eq!((cropped.width(), cropped.height()), (200, 120));
    assert_eq!(cropped.to_rgba8().get_pixel(0, 0).0, [10, 20, 30, 255]);
    click(&mut overlay, "capture-Save to Library");
    assert!(overlay.debug_bounds("capture-Saved to Library").is_some());
    let record = view.read_with(cx, |view, _| {
        view.profile.handle.read_settings(|browser| {
            browser
                .active_user_profile()
                .captures
                .items()
                .next()
                .unwrap()
                .clone()
        })
    });
    let file = root
        .path()
        .join("Captures")
        .join(format!(
            "profile-{}",
            serde_json::to_string(
                &view.read_with(cx, |view, _| view.browser.active_user_profile_id())
            )
            .unwrap()
        ))
        .join(record.filename());
    assert!(file.exists());
    assert_eq!((record.width, record.height), (200, 120));
    assert_eq!(record.source_url, active_url(&view, cx).unwrap());
    click(&mut overlay, "capture-Saved to Library");
    assert_eq!(
        view.read_with(cx, |view, _| view.profile.handle.read_settings(|b| b
            .active_user_profile()
            .captures
            .items()
            .count())),
        1
    );
    click(&mut overlay, "capture-Retake");
    overlay.executor().advance_clock(Duration::from_millis(40));
    overlay.run_until_parked();
    assert!(view.read_with(cx, |view, _| view.capture.as_ref().unwrap().is_selecting()));
    overlay.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(view.read_with(cx, |view, _| view.capture.is_none()));
}
#[gpui::test]
fn capture_rejects_a_late_image_after_a_same_url_document_replacement(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    fake.0.borrow_mut().capture_supported = true;
    fake.take_calls();
    cx.simulate_keystrokes("cmd-shift-2");
    cx.run_until_parked();
    let mut overlay = current(&view, cx);
    let frame = fake.0.borrow().bounds[&1];
    select(&mut overlay, frame);
    let request = fake
        .take_calls()
        .into_iter()
        .find_map(|call| {
            if let Call::Snapshot(1, request) = call {
                Some(request)
            } else {
                None
            }
        })
        .unwrap();
    *fake.0.borrow_mut().document_epochs.entry(1).or_default() += 1;
    complete(&view, cx, &fake, request, image(frame));
    assert!(view.read_with(cx, |view, _| view.capture.is_none()));
    assert_eq!(
        view.read_with(cx, |view, _| view.profile.handle.read_settings(|b| b
            .active_user_profile()
            .captures
            .items()
            .count())),
        0
    );
}

#[gpui::test]
fn capture_cannot_adopt_a_screenshot_pending_before_its_document_epoch(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    fake.0.borrow_mut().capture_supported = true;
    view.update(cx, |view, _| {
        let tab = view.browser.active_tab().unwrap().id;
        view.pages
            .as_ref()
            .unwrap()
            .borrow_mut()
            .request_snapshot(tab)
            .unwrap();
    });
    *fake.0.borrow_mut().document_epochs.entry(1).or_default() += 1;
    fake.take_calls();
    cx.simulate_keystrokes("cmd-shift-2");
    cx.run_until_parked();
    let mut overlay = current(&view, cx);
    let frame = fake.0.borrow().bounds[&1];
    select(&mut overlay, frame);
    assert!(view.read_with(cx, |view, _| view.capture.is_none()));
    assert!(!fake
        .take_calls()
        .iter()
        .any(|call| matches!(call, Call::Snapshot(..))));
}


#[gpui::test]
fn file_capture_menu_opens_over_the_retained_page_and_escape_cancels(cx: &mut TestAppContext) {
    let (view, visual, fake) = open_pages(cx);
    fake.0.borrow_mut().capture_supported = true;
    let original = active_url(&view, visual);
    fake.take_calls();
    dispatch_menu(visual, "File", "Capture…");
    let mut overlay = current(&view, visual);
    assert!(overlay.debug_bounds("page-capture").is_some());
    overlay.simulate_keystrokes("escape");
    overlay.run_until_parked();
    assert!(view.read_with(visual, |view, _| view.capture.is_none()));
    assert_eq!(active_url(&view, visual), original);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Navigate(..) | Call::Reload(..))));
}

#[gpui::test]
fn file_capture_menu_from_command_bar_closes_the_draft_and_keeps_capture_focus(cx: &mut TestAppContext) {
    let (view, visual, fake) = open_pages(cx);
    fake.0.borrow_mut().capture_supported = true;
    visual.simulate_keystrokes("cmd-t");
    let mut command = bar(&view, visual);
    command.simulate_input("unfinished capture query");
    fake.take_calls();
    dispatch_menu(&mut command, "File", "Capture…");
    visual.executor().advance_clock(Duration::from_millis(700));
    visual.run_until_parked();
    assert!(view.read_with(visual, |view, _| view.command_bar.is_none()));
    let mut overlay = current(&view, visual);
    assert!(overlay.debug_bounds("page-capture").is_some());
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::FocusPage(_) | Call::Navigate(..) | Call::Create(..))));
    overlay.simulate_keystrokes("escape");
    overlay.run_until_parked();
    assert!(view.read_with(visual, |view, _| view.capture.is_none()));
}
