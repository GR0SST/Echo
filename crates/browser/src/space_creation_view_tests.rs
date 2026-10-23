//! Real GPUI input/layout tests, including the observed two-stage Escape flow.
use super::*;
use crate::space_creation::SpaceCreationController;
use browser_core::{Browser, session};
use gpui::{Modifiers, TestAppContext, VisualTestContext};
use std::{cell::RefCell, rc::Rc};

type Events = Rc<RefCell<Vec<SpaceCreationEvent>>>;

/// The reference form viewport is 250 px; window resize tests keep this
/// viewport stable while exercising its height and bottom anchoring.
struct FormTestHost {
    form: Entity<SpaceCreationView>,
    width: f32,
}
impl Render for FormTestHost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().w(px(self.width)).h_full().child(self.form.clone())
    }
}

fn open(
    cx: &mut TestAppContext,
) -> (
    Entity<SpaceCreationView>,
    &mut VisualTestContext,
    Events,
    Subscription,
) {
    open_at_width(cx, 250.)
}

fn open_at_width(
    cx: &mut TestAppContext,
    width: f32,
) -> (
    Entity<SpaceCreationView>,
    &mut VisualTestContext,
    Events,
    Subscription,
) {
    cx.update(|cx| {
        text_input::bind_keys(cx);
        bind_keys(cx);
    });
    let browser = Browser::new();
    let mut controller = SpaceCreationController::new(&browser);
    controller.begin(&browser, browser.space().id).unwrap();
    let draft = controller.request().unwrap();
    let mut form = None;
    let (_, visual) = cx.add_window_view(|window, cx| {
        let view =
            cx.new(|cx| SpaceCreationView::new(&draft, browser.active_user_profile().name(), cx));
        view.read(cx).focus_name(window, cx);
        form = Some(view.clone());
        FormTestHost { form: view, width }
    });
    let view = form.expect("reference form host");
    let events: Events = Rc::default();
    let log = events.clone();
    let subscription = visual.update(|_, cx| {
        cx.subscribe(&view, move |_, event: &SpaceCreationEvent, _| {
            log.borrow_mut().push(event.clone())
        })
    });
    visual.simulate_resize(gpui::size(px(1920.), px(1050.)));
    visual.run_until_parked();
    (view, visual, events, subscription)
}

fn click(visual: &mut VisualTestContext, selector: &'static str) {
    visual.run_until_parked();
    let bounds = visual
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("missing {selector}"));
    visual.simulate_mouse_move(bounds.center(), None, Modifiers::none());
    visual.run_until_parked();
    visual.simulate_click(bounds.center(), Modifiers::none());
    visual.run_until_parked();
}

#[gpui::test]
fn real_typing_enables_submission_and_whitespace_does_not(cx: &mut TestAppContext) {
    let (view, visual, events, _subscription) = open(cx);
    click(visual, "space-creation-submit");
    assert!(events.borrow().is_empty());
    click(visual, "space-creation-name");
    visual.simulate_input("   ");
    click(visual, "space-creation-submit");
    assert!(
        !events
            .borrow()
            .iter()
            .any(|event| matches!(event, SpaceCreationEvent::Submit(_)))
    );
    view.update_in(visual, |view, window, cx| view.focus_name(window, cx));
    visual.simulate_keystrokes("cmd-a");
    visual.simulate_input("Research");
    click(visual, "space-creation-submit");
    let events = events.borrow();
    assert!(
        events
            .iter()
            .any(|event| *event == SpaceCreationEvent::NameChanged("Research".into()))
    );
    let Some(SpaceCreationEvent::Submit(draft)) = events.last() else {
        panic!("expected submission")
    };
    assert_eq!(draft.name, "Research");
    assert_eq!(
        draft.user_profile_id,
        Browser::new().active_user_profile_id()
    );
}

#[gpui::test]
fn picker_rows_emit_captured_draft_intents_without_choosers(cx: &mut TestAppContext) {
    let (view, visual, events, _subscription) = open(cx);
    visual.simulate_input("Keep this name");
    let captured = view.read_with(visual, |view, _| view.draft.clone());
    for selector in [
        "space-creation-icon",
        "space-creation-profile",
        "space-creation-profile-info",
        "space-creation-theme",
    ] {
        click(visual, selector);
    }
    let events = events.borrow();
    assert_eq!(
        &events[events.len() - 4..],
        [
            SpaceCreationEvent::ChooseIcon(captured.clone()),
            SpaceCreationEvent::ChooseProfile(captured.clone()),
            SpaceCreationEvent::ProfileInfo(captured.clone()),
            SpaceCreationEvent::ChooseTheme(captured),
        ]
    );
    assert!(visual.debug_bounds("space-creation-form").is_some());
}

#[gpui::test]
fn cancel_intent_allows_controller_to_discard_without_session_or_allocator_changes(
    cx: &mut TestAppContext,
) {
    let (_, visual, events, _subscription) = open(cx);
    let browser = Browser::new();
    let before = session::to_json(&browser);
    let mut controller = SpaceCreationController::new(&browser);
    controller.begin(&browser, browser.space().id).unwrap();
    visual.simulate_input("Unsaved");
    click(visual, "space-creation-cancel");
    for event in events.borrow().iter() {
        match event {
            SpaceCreationEvent::NameChanged(name) => {
                controller.edit_draft(|draft| draft.name = name.clone())
            }
            SpaceCreationEvent::Cancel => {
                controller.cancel(&browser);
            }
            _ => panic!("unexpected event"),
        }
    }
    assert_eq!(events.borrow().last(), Some(&SpaceCreationEvent::Cancel));
    assert!(controller.draft().is_none());
    assert_eq!(session::to_json(&browser), before);
}

fn assert_geometry(visual: &mut VisualTestContext, height: f32) {
    visual.run_until_parked();
    let form = visual.debug_bounds("space-creation-form").unwrap();
    assert_eq!(form.size, gpui::size(px(250.), px(height)));
    for (selector, y, h) in [
        ("space-creation-name", 237., 36.),
        ("space-creation-profile-row", 279., 37.),
        ("space-creation-theme", 323., 37.),
    ] {
        let row = visual.debug_bounds(selector).unwrap();
        assert_eq!(row.origin - form.origin, gpui::point(px(14.), px(y)));
        assert_eq!(row.size, gpui::size(px(223.), px(h)));
    }
    let art = visual.debug_bounds("space-creation-art").unwrap();
    assert_eq!(art.center() - form.origin, gpui::point(px(125.), px(83.)));
    assert_eq!(art.size, gpui::size(px(64.), px(48.)));
    assert_eq!(
        visual
            .debug_bounds("space-creation-title")
            .unwrap()
            .origin
            .y
            - form.origin.y,
        px(143.)
    );
    let subtitle = visual.debug_bounds("space-creation-subtitle").unwrap();
    assert_eq!(subtitle.origin.y - form.origin.y, px(175.));
    assert_eq!(subtitle.size.height, px(36.));
    let submit = visual.debug_bounds("space-creation-submit").unwrap();
    assert_eq!(
        submit.origin - form.origin,
        gpui::point(px(14.), px(height - 132.))
    );
    assert_eq!(submit.size, gpui::size(px(223.), px(36.)));
    let cancel = visual.debug_bounds("space-creation-cancel").unwrap();
    assert_eq!(
        cancel.center() - form.origin,
        gpui::point(px(125.5), px(height - 72.))
    );
}

#[gpui::test]
fn measured_form_geometry_and_bottom_anchors_survive_resize_and_typing(cx: &mut TestAppContext) {
    let (_, visual, _, _subscription) = open(cx);
    assert_geometry(visual, 1050.);
    visual.simulate_input("A long Space name that stays in its input row");
    assert_geometry(visual, 1050.);
    visual.simulate_resize(gpui::size(px(1280.), px(720.)));
    assert_geometry(visual, 720.);
    visual.simulate_resize(gpui::size(px(1600.), px(900.)));
    assert_geometry(visual, 900.);
}

#[gpui::test]
fn return_keeps_the_draft_and_escape_leaves_the_field_before_cancelling(cx: &mut TestAppContext) {
    let (view, visual, events, _subscription) = open(cx);
    visual.simulate_keystrokes("enter");
    assert!(events.borrow().is_empty());
    visual.simulate_input("Keyboard draft");
    visual.simulate_keystrokes("enter");
    visual.run_until_parked();
    assert_eq!(
        view.read_with(visual, |view, _| view.draft.name.clone()),
        "Keyboard draft"
    );
    assert!(!events.borrow().iter().any(|event| matches!(
        event,
        SpaceCreationEvent::Submit(_) | SpaceCreationEvent::Cancel
    )));
    visual.simulate_keystrokes("escape");
    visual.run_until_parked();
    assert!(visual.update(|window, cx| view.read(cx).focus_handle(cx).is_focused(window)));
    assert!(
        !events
            .borrow()
            .iter()
            .any(|event| matches!(event, SpaceCreationEvent::Cancel))
    );
    visual.simulate_keystrokes("escape");
    visual.run_until_parked();
    assert_eq!(events.borrow().last(), Some(&SpaceCreationEvent::Cancel));
}

#[gpui::test]
fn approved_picker_result_sync_retains_name_and_updates_submitted_fields(cx: &mut TestAppContext) {
    let (view, visual, events, _subscription) = open(cx);
    visual.simulate_input("Retained");
    let mut browser = Browser::new();
    let profile = browser
        .create_user_profile("Long alternate profile")
        .unwrap();
    let mut draft = view.read_with(visual, |view, _| view.draft.clone());
    draft.user_profile_id = profile;
    draft.custom_icon = Some("🌱".into());
    draft.hue = 123.;
    view.update(visual, |view, cx| {
        view.sync_draft(&draft, "Long alternate profile", cx)
    });
    click(visual, "space-creation-submit");
    assert_eq!(
        events.borrow().last(),
        Some(&SpaceCreationEvent::Submit(draft))
    );
}

/// Regression against the observed row structure: icon/label alignment,
/// compact current-profile pill, and a separately anchored info control.
fn assert_row_controls(visual: &mut VisualTestContext, width: f32) {
    visual.run_until_parked();
    let form = visual.debug_bounds("space-creation-form").unwrap();
    let profile = visual.debug_bounds("space-creation-profile-row").unwrap();
    let theme = visual.debug_bounds("space-creation-theme").unwrap();
    assert_eq!(profile.origin - form.origin, gpui::point(px(14.), px(279.)));
    assert_eq!(theme.origin - form.origin, gpui::point(px(14.), px(323.)));
    assert_eq!(profile.size, gpui::size(px(width - 27.), px(37.)));
    assert_eq!(theme.size, profile.size);
    for (icon, label, row) in [
        (
            "space-creation-profile-icon",
            "space-creation-profile-label",
            profile,
        ),
        (
            "space-creation-theme-icon",
            "space-creation-theme-label",
            theme,
        ),
    ] {
        let icon = visual.debug_bounds(icon).unwrap();
        let label = visual.debug_bounds(label).unwrap();
        assert_eq!(icon.size, gpui::size(px(12.), px(12.)));
        assert_eq!(icon.center().x - form.origin.x, px(31.));
        assert_eq!(icon.center().y, row.center().y);
        assert_eq!(label.origin.x - form.origin.x, px(50.));
        assert_eq!(label.center().y, row.center().y);
    }
    let label = visual.debug_bounds("space-creation-profile-label").unwrap();
    let pill = visual.debug_bounds("space-creation-profile").unwrap();
    let value = visual.debug_bounds("space-creation-profile-value").unwrap();
    let chevron = visual
        .debug_bounds("space-creation-profile-chevron")
        .unwrap();
    let info = visual.debug_bounds("space-creation-profile-info").unwrap();
    assert_eq!(pill.origin.x - label.right(), px(6.));
    assert!(pill.size.width <= px(160.));
    assert_eq!(pill.size.height, px(28.));
    assert_eq!(pill.center().y, profile.center().y);
    assert!(value.origin.x >= pill.origin.x);
    assert!(value.right() <= chevron.origin.x);
    assert_eq!(chevron.size, gpui::size(px(12.), px(12.)));
    assert_eq!(chevron.center().y, pill.center().y);
    assert!(chevron.right() <= pill.right());
    assert!(pill.right() <= info.origin.x - px(6.));
    assert_eq!(info.size, gpui::size(px(22.), px(22.)));
    assert_eq!(info.center().x - form.origin.x, px(width - 25.));
    assert_eq!(info.center().y, profile.center().y);
    assert_eq!(profile.right() - info.right(), px(1.));
    let add = visual.debug_bounds("space-creation-add-icon").unwrap();
    let name = visual.debug_bounds("space-creation-name").unwrap();
    assert_eq!(add.size, gpui::size(px(18.), px(18.)));
    assert_eq!(add.center().y, name.center().y);
    assert!(add.origin.x > name.origin.x && add.right() < name.right());
}

#[gpui::test]
fn observed_profile_theme_controls_keep_compact_pill_and_separate_info_hit_target(
    cx: &mut TestAppContext,
) {
    let (_, visual, events, _subscription) = open(cx);
    assert_row_controls(visual, 250.);
    let pill = visual.debug_bounds("space-creation-profile").unwrap();
    let row = visual.debug_bounds("space-creation-profile-row").unwrap();
    assert!(pill.size.width < row.size.width / 2.);
    click(visual, "space-creation-profile-info");
    assert_eq!(events.borrow().len(), 1);
    assert!(matches!(
        events.borrow().last(),
        Some(SpaceCreationEvent::ProfileInfo(_))
    ));
    click(visual, "space-creation-profile");
    assert_eq!(events.borrow().len(), 2);
    assert!(matches!(
        events.borrow().last(),
        Some(SpaceCreationEvent::ChooseProfile(_))
    ));
}

#[gpui::test]
fn long_profile_truncates_inside_pill_at_240px_and_keeps_full_intent(cx: &mut TestAppContext) {
    let (view, visual, events, _subscription) = open_at_width(cx, 240.);
    let draft = view.read_with(visual, |view, _| view.draft.clone());
    // NoopTextSystem gives every ASCII glyph a 0.6-em advance. At 240 px,
    // "Profile" consumes 58.8 px, leaving 83.2 px for the pill. "Default"
    // needs 86.6 px including chrome, so it already clamps like the long name.
    // Use a genuinely fitting short label to exercise natural-width growth.
    view.update(visual, |view, cx| view.sync_draft(&draft, "A", cx));
    assert_row_controls(visual, 240.);
    let short_width = visual
        .debug_bounds("space-creation-profile")
        .unwrap()
        .size
        .width;
    let short_pill = visual.debug_bounds("space-creation-profile").unwrap();
    let info = visual.debug_bounds("space-creation-profile-info").unwrap();
    let available = (info.origin.x - px(6.) - short_pill.origin.x).min(px(160.));
    assert!(short_width < available, "short fixture must fit naturally");
    view.update(visual, |view, cx| {
        view.sync_draft(
            &draft,
            "A profile with a very long name that must never cover the info control",
            cx,
        )
    });
    assert_row_controls(visual, 240.);
    let long_width = visual
        .debug_bounds("space-creation-profile")
        .unwrap()
        .size
        .width;
    assert!(long_width > short_width);
    assert!(
        (long_width - available).abs() <= px(1.),
        "long name should fill the available pill width"
    );
    click(visual, "space-creation-profile");
    assert_eq!(
        events.borrow().last(),
        Some(&SpaceCreationEvent::ChooseProfile(draft))
    );
    visual.simulate_resize(gpui::size(px(1280.), px(720.)));
    assert_row_controls(visual, 240.);
    let form = visual.debug_bounds("space-creation-form").unwrap();
    assert_eq!(
        visual
            .debug_bounds("space-creation-art")
            .unwrap()
            .center()
            .x,
        form.center().x
    );
    assert_eq!(
        visual
            .debug_bounds("space-creation-submit")
            .unwrap()
            .origin
            .y
            - form.origin.y,
        px(588.)
    );
}
