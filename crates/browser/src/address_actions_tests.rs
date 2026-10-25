use super::*;
#[gpui::test]
fn address_share_uses_system_services_for_the_captured_document_and_escape_keeps_the_page(
    cx: &mut TestAppContext,
) {
    let (view, cx, _) = open_pages(cx);
    let url = active_url(&view, cx).unwrap();
    right_click(cx, "address");
    let mut menu = tab_menu(&view, cx);
    click(&mut menu, "tab-menu-share");
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert_eq!(view.share_picker.as_ref().unwrap().urls, vec![url]);
        assert!(view.tab_menu.is_some());
    });
    menu.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(view.read_with(cx, |view, _| view.tab_menu.is_none()));
    assert!(active_url(&view, cx).is_some());
}
#[gpui::test]
fn address_capture_opens_the_existing_picker_and_escape_dismisses_it(
    cx: &mut TestAppContext,
) {
    let (view, cx, fake) = open_pages(cx);
    fake.0.borrow_mut().capture_supported = true;
    right_click(cx, "address");
    let mut menu = tab_menu(&view, cx);
    click(&mut menu, "tab-menu-capture");
    cx.run_until_parked();
    let overlay = view.read_with(cx, |view, _| {
        view.capture.as_ref().unwrap().overlay.unwrap()
    });
    let mut capture = VisualTestContext::from_window(overlay.into(), &cx.cx);
    capture.run_until_parked();
    capture.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(view.read_with(cx, |view, _| view.capture.is_none()));
}
#[gpui::test]
fn address_share_rejects_a_document_replaced_before_hover(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    right_click(cx, "address");
    let mut menu = tab_menu(&view, cx);
    let bounds = menu.debug_bounds("tab-menu-share").unwrap();
    *fake.0.borrow_mut().document_epochs.entry(1).or_default() += 1;
    menu.simulate_mouse_move(bounds.center(), None, Modifiers::none());
    cx.run_until_parked();
    assert!(view.read_with(cx, |view, _| view.share_picker.is_none()));
}
