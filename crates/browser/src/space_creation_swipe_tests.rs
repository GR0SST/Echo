use super::*;
use gpui::TouchPhase::*;
use crate::sidebar_pages::SidebarPage;

fn last_space(view: &Entity<BrowserWindow>, cx: &mut VisualTestContext) {
    view.update(cx, |view, cx| view.switch_space(view.browser.spaces.len() - 1, cx));
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
}

#[gpui::test]
fn create_pull_is_reversible_and_the_form_waits_for_a_fully_armed_release(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    last_space(&view, cx);
    let inventory = view.read_with(cx, |view, _| (view.browser.spaces.len(), view.browser.all_tabs().count()));
    fake.take_calls();
    crate::native::HAPTIC_PULSES.with(|n| n.set(0));
    scroll(cx, "sidebar", Started, 0., 0.);
    scroll(cx, "sidebar", Moved, -120., 0.);
    cx.run_until_parked();
    assert!((view.read_with(cx, |view, _| view.space_creation_pull_progress()) - 0.5).abs() < 0.001);
    assert!(view.read_with(cx, |view, _| view.sidebar_navigation.form.is_none()));
    assert!(view.read_with(cx, |view, _| view.space_creation_mark_visible(view.browser.space().id)));
    assert!(cx.debug_bounds("space-creation-pull").is_some());
    scroll(cx, "sidebar", Moved, -150., 0.);
    cx.run_until_parked();
    assert_eq!(view.read_with(cx, |view, _| view.space_creation_pull_progress()), 1.);
    assert!(view.read_with(cx, |view, _| view.sidebar_navigation.controller.draft().is_none()));
    assert_eq!(crate::native::HAPTIC_PULSES.with(|n| n.get()), 1);
    scroll(cx, "sidebar", Moved, 100., 0.);
    cx.run_until_parked();
    assert!(view.read_with(cx, |view, _| view.space_creation_pull_progress()) < 1.);
    assert_eq!(crate::native::HAPTIC_PULSES.with(|n| n.get()), 2);
    scroll(cx, "sidebar", Ended, 0., 0.);
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    assert!(view.read_with(cx, |view, _| view.sidebar_navigation.controller.draft().is_none()));
    assert_eq!(view.read_with(cx, |view, _| view.space_creation_pull_progress()), 0.);
    assert!(!view.read_with(cx, |view, _| view.space_creation_mark_visible(view.browser.space().id)));
    scroll(cx, "sidebar", Started, 0., 0.);
    scroll(cx, "sidebar", Moved, -270., 0.);
    cx.run_until_parked();
    scroll(cx, "sidebar", Moved, 100., 0.);
    cx.run_until_parked();
    scroll(cx, "sidebar", Moved, -100., 0.);
    cx.run_until_parked();
    assert!(view.read_with(cx, |view, _| view.sidebar_navigation.form.is_none()));
    scroll(cx, "sidebar", Ended, 0., 0.);
    cx.run_until_parked();
    assert!(view.read_with(cx, |view, _| view.sidebar_navigation.controller.selected() == SidebarPage::CreateSpace));
    assert!(view.read_with(cx, |view, _| view.sidebar_navigation.form.is_some()));
    assert_eq!(view.read_with(cx, |view, _| (view.browser.spaces.len(), view.browser.all_tabs().count())), inventory);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn swiping_back_discards_the_creation_draft_only_when_its_page_has_left(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    last_space(&view, cx);
    swipe(cx, "sidebar", -320., 10);
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    let count = view.read_with(cx, |view, _| view.browser.spaces.len());
    scroll(cx, "space-creation-form", Started, 0., 0.);
    scroll(cx, "space-creation-form", Moved, 260., 0.);
    cx.run_until_parked();
    assert!(view.read_with(cx, |view, _| view.sidebar_navigation.controller.draft().is_some()));
    assert!(cx.debug_bounds("space-creation-form").unwrap().left() > gpui::px(100.));
    assert!(view.read_with(cx, |view, _| view.space_creation_mark_visible(view.browser.space().id)));
    scroll(cx, "sidebar", Moved, 240., 0.);
    cx.run_until_parked();
    assert!(!view.read_with(cx, |view, _| view.space_creation_mark_visible(view.browser.space().id)));
    scroll(cx, "sidebar", Moved, -240., 0.);
    cx.run_until_parked();
    assert!(view.read_with(cx, |view, _| view.space_creation_mark_visible(view.browser.space().id)));
    scroll(cx, "sidebar", Ended, 0., 0.);
    cx.run_until_parked();
    assert!(view.read_with(cx, |view, _| view.sidebar_navigation.creation_exiting));
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert!(view.sidebar_navigation.controller.draft().is_none());
        assert!(view.sidebar_navigation.form.is_none());
        assert!(!view.sidebar_navigation.creation_exiting);
        assert!(!view.space_creation_mark_visible(view.browser.space().id));
        assert_eq!(view.browser.spaces.len(), count);
    });
}

#[gpui::test]
fn a_fast_short_pull_and_its_momentum_cannot_open_creation(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    last_space(&view, cx);
    let origin = view.read_with(cx, |view, _| view.browser.space().id);
    scroll(cx, "sidebar", Started, 0., 0.);
    for _ in 0..2 {
        cx.executor().advance_clock(Duration::from_millis(16));
        scroll(cx, "sidebar", Moved, -60., 0.);
    }
    scroll(cx, "sidebar", Ended, 0., 0.);
    scroll(cx, "sidebar", Moved, -300., 0.);
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert_eq!(view.sidebar_navigation.controller.selected(), SidebarPage::Space(origin));
        assert!(view.sidebar_navigation.form.is_none());
        assert!(!view.space_creation_mark_visible(origin));
    });
}
